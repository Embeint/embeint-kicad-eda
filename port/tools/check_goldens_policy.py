#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Golden review policy and staged phase-0 CI; Python >=3.11, stdlib only.

The infrastructure stage is explicitly INCOMPLETE, never phase-0 qualification.
Final integration enables --require-phase0; it requires three live green units
and valid exhaustive extraction accounting. Platform coverage stays reported.
"""
import argparse
from dataclasses import dataclass
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
NATIVE = {"embeint-layout-cli", "embeint-ipc", "embeint-mcp", "kicad-difftest"}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def command(args, root=ROOT):
    return subprocess.check_output(args, cwd=root, text=True)


@dataclass(frozen=True)
class Number:
    token: str


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key}")
        result[key] = value
    return result


def exact(text):
    def invalid(token):
        raise ValueError(f"non-JSON number {token}")
    return json.loads(text, parse_int=Number, parse_float=Number,
                      parse_constant=invalid, object_pairs_hook=unique)


def replies(data, unit):
    result = []
    seen = set()
    for line in data.decode("utf-8").splitlines():
        record = exact(line)
        case = record.get("case")
        if (record.get("unit") != unit or not isinstance(case, str) or not case
                or case in seen or set(record) not in
                ({"unit", "case", "result"}, {"unit", "case", "error"})):
            raise ValueError(f"{unit}: invalid or duplicate reply identity")
        if "error" in record:
            raise ValueError(f"{unit}/{case}: errors/deferrals cannot be golden results")
        seen.add(case)
        result.append((case, line, record))
    return result


def changes(old, new):
    before = {case: (i, raw, value) for i, (case, raw, value) in enumerate(old)}
    after = {case for case, _, _ in new}
    result = []
    for i, (case, raw, value) in enumerate(new):
        previous = before.get(case)
        if previous is None or previous[0] != i or previous[2] != value:
            result.append({"case": case, "old_index": previous[0] if previous else None,
                           "new_index": i, "old": previous[1] if previous else None,
                           "new": raw})
    for i, (case, raw, _) in enumerate(old):
        if case not in after:
            result.append({"case": case, "old_index": i, "new_index": None,
                           "old": raw, "new": None})
    return result


def verify_evidence(evidence, unit, old, new, cases):
    expected = {"format", "unit", "reason", "changes", "cases_sha256",
                "old_sha256", "new_sha256", "oracle"}
    if set(evidence) != expected or evidence["format"] != 1 or evidence["unit"] != unit:
        raise ValueError(f"{unit}: invalid evidence schema/identity")
    if not isinstance(evidence["reason"], str) or not evidence["reason"].strip():
        raise ValueError(f"{unit}: empty blessing reason")
    for key, data in (("old_sha256", old), ("new_sha256", new), ("cases_sha256", cases)):
        if evidence[key] != sha(data):
            raise ValueError(f"{unit}: stale evidence {key}")
    oracle = evidence["oracle"]
    if (not isinstance(oracle, dict) or not isinstance(oracle.get("provenance"), dict)
            or not oracle["provenance"]):
        raise ValueError(f"{unit}: missing baseline oracle provenance")
    for key in ("oracle_sha256", "rust_sha256"):
        if not re.fullmatch("[0-9a-f]{64}", oracle.get(key, "")):
            raise ValueError(f"{unit}: missing executable identity {key}")
    if evidence["changes"] != changes(replies(old, unit), replies(new, unit)):
        raise ValueError(f"{unit}: evidence must enumerate exact case IDs, old/new values and order")


def check_changes(root, base):
    paths = command(["git", "diff", "--no-renames", "--name-only", base, "HEAD", "--", "port/goldens"], root).splitlines()
    added = command(["git", "diff", "--no-renames", "--name-only", "--diff-filter=A", base, "HEAD", "--", "port/goldens/evidence"], root).splitlines()
    evidence_paths = [p for p in added if p.endswith(".json")]
    if any(p.startswith("port/goldens/evidence/") and p not in added for p in paths):
        raise ValueError("blessing evidence is immutable; add a new artifact rather than editing/removing one")
    for name in paths:
        path = Path(name)
        if path.parent.as_posix() != "port/goldens" or path.suffix != ".jsonl":
            continue
        unit = path.stem
        old_result = subprocess.run(["git", "show", f"{base}:{name}"], cwd=root, capture_output=True)
        if old_result.returncode and old_result.returncode != 128:
            raise ValueError(f"cannot read base golden {name}")
        old = old_result.stdout if old_result.returncode == 0 else b""
        new = (root / path).read_bytes() if (root / path).exists() else b""
        if not new:
            raise ValueError(f"{unit}: golden removal requires an explicit reviewed retirement, not blessing")
        cases = (root / "port/cases" / f"{unit}.jsonl").read_bytes()
        errors = []
        for evidence_path in evidence_paths:
            try:
                value = json.loads((root / evidence_path).read_text(encoding="utf-8"), object_pairs_hook=unique)
                verify_evidence(value, unit, old, new, cases)
                break
            except (ValueError, KeyError, TypeError, AttributeError) as error:
                errors.append(str(error))
        else:
            raise ValueError(f"{unit}: changed golden needs new matching evidence: {errors}")
    print("Golden change evidence valid")


def portable_packages(metadata):
    members = set(metadata["workspace_members"])
    result = []
    for package in metadata["packages"]:
        if package["id"] not in members:
            continue
        portable = (package.get("metadata") or {}).get("kicad", {}).get("portable")
        if portable is not None and type(portable) is not bool:
            raise ValueError(f"{package['name']}: metadata.kicad.portable must be boolean")
        if portable is False or package["name"] in NATIVE:
            continue
        result.append(package["name"])
    if not {"embeint-layout", "embeint-web"}.issubset(result):
        raise ValueError("existing portable crates cannot be silently omitted")
    return sorted(result)


def wasm(root):
    manifest = str(root / "rust/Cargo.toml")
    metadata = json.loads(command(["cargo", "metadata", "--locked", "--no-deps",
                                   "--format-version", "1", "--manifest-path", manifest], root))
    packages = portable_packages(metadata)
    print("WebAssembly build for every portable workspace crate:", ", ".join(packages), flush=True)
    args = ["cargo", "build", "--locked", "--manifest-path", manifest,
            "--target", "wasm32-unknown-unknown", "-j8"]
    for package in packages:
        args += ["-p", package]
    subprocess.run(args, cwd=root, check=True)


def ci(root, runner, oracle, reports, required):
    reports.mkdir(parents=True, exist_ok=True)
    validator = root / "port/tools/validate_units.py"
    units = {}
    if validator.exists():
        subprocess.run([sys.executable, str(validator)], cwd=root, check=True)
        for path in sorted((root / "port/units").glob("*.toml")):
            value = tomllib.loads(path.read_text(encoding="utf-8"))
            units[value["id"]] = value
    elif required:
        raise ValueError("phase0 gate needs generated unit files and validator")
    graph = root / "port/extractor/generated/graph.json"
    compressed_graph = graph.with_suffix(".json.gz")
    if compressed_graph.exists():
        with gzip.open(compressed_graph, "rt", encoding="utf-8") as stream:
            provenance = json.load(stream).get("provenance", {})
    else:
        provenance = json.loads(graph.read_text(encoding="utf-8")).get("provenance", {}) if graph.exists() else {}
    green = {uid for uid, unit in units.items() if unit["status"] == "green"}
    cases = sorted((root / "port/cases").glob("*.jsonl"))
    tested = set()
    for path in cases:
        uid = path.stem
        if uid not in units:
            raise ValueError(f"{uid}: cases without generated unit record")
        config = units[uid].get("difftest", {})
        package, binary = config.get("rust_package"), config.get("rust_binary")
        if not all(isinstance(s, str) and re.fullmatch(r"[A-Za-z0-9_-]+", s) for s in (package, binary)):
            raise ValueError(f"{uid}: add [difftest] rust_package/rust_binary to its unit record")
        subprocess.run(["cargo", "build", "--locked", "--manifest-path", str(root / "rust/Cargo.toml"),
                        "-j8", "-p", package, "--bin", binary], cwd=root, check=True)
        shim = root / "rust/target/debug" / (binary + (".exe" if os.name == "nt" else ""))
        args = [str(runner), "check", "--root", str(root), "--unit", uid,
                "--oracle", str(oracle), "--rust", str(shim), "--report", str(reports / f"{uid}.json")]
        rust_args = config.get("rust_args", [])
        if not isinstance(rust_args, list) or not all(isinstance(arg, str) for arg in rust_args):
            raise ValueError(f"{uid}: rust_args must be a string array")
        for arg in rust_args:
            args += ["--rust-arg", arg]
        subprocess.run(args, cwd=root, check=True)
        tested.add(uid)
    if green - tested:
        raise ValueError(f"green units missing live cases: {sorted(green - tested)}")
    for golden in (root / "port/goldens").glob("*.jsonl"):
        if golden.stem not in tested:
            raise ValueError(f"{golden.stem}: orphan/untested golden")
    passed = len(green & tested)
    complete = required and passed >= 3 and bool(units)
    summary = {"phase0": "COMPLETE" if complete else "INCOMPLETE", "strict_gate_enabled": required,
               "live_green_units": passed, "required_green_units": 3,
               "validated_unit_records": len(units), "extraction": provenance}
    (reports / "phase0.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    coverage = {key: provenance.get(key) for key in
                ("inventory_complete", "parse_complete", "compile_database_complete",
                 "all_configurations_complete", "gate_complete", "unresolved_call_count")}
    for key in ("parse_failures", "uncompiled_application_sources"):
        coverage[key] = len(provenance.get(key, []))
    message = (f"Phase 0: {summary['phase0']}; {passed}/3 live green units; "
               f"{len(units)} validated unit records.\n\n"
               f"Extraction/configuration coverage (reported separately): `{json.dumps(coverage, sort_keys=True)}`\n")
    print(message)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(message)
    if required and not complete:
        raise ValueError("phase0 strict gate requires at least three live green units and all generated valid unit records")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--base", help="verify changed golden evidence against this Git revision")
    parser.add_argument("--wasm", action="store_true")
    parser.add_argument("--ci", action="store_true")
    parser.add_argument("--require-phase0", action="store_true")
    parser.add_argument("--runner", type=Path)
    parser.add_argument("--oracle", type=Path)
    parser.add_argument("--reports", type=Path, default=Path("difftest-reports"))
    args = parser.parse_args()
    try:
        if args.base:
            check_changes(args.root, args.base)
        if args.wasm:
            wasm(args.root)
        if args.ci:
            if args.runner is None or args.oracle is None:
                parser.error("--ci requires --runner and --oracle (paths need exist when cases exist)")
            ci(args.root.resolve(), args.runner.resolve(), args.oracle.resolve(), args.reports.resolve(), args.require_phase0)
        if not (args.base or args.wasm or args.ci):
            parser.error("choose --base, --wasm or --ci")
        return 0
    except (ValueError, OSError, KeyError, TypeError, AttributeError, subprocess.CalledProcessError) as error:
        print(f"goldens policy: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
