#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Golden review policy and staged phase-0 CI; Python >=3.11, stdlib only.

The infrastructure stage is explicitly INCOMPLETE, never phase-0 qualification.
Final integration enables --require-phase0; it requires three live green layer-0 units
and valid exhaustive extraction accounting. Platform coverage stays reported.

Only a unit record declared `partial` runs the difftest with --partial: its live run
is accepted with at least one exact comparison, zero failures and enumerated Rust
deferrals. Every other unit, green included, runs strict and must be zero-deferral.
Partial units never count towards the phase-0 gate.
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
    lines = data.decode("utf-8").split("\n")
    if lines[-1] == "":
        lines.pop()
    for line in lines:
        line = line.removesuffix("\r")
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


def committed_bytes(root, revision, path, *, allow_missing=False):
    result = subprocess.run(["git", "show", f"{revision}:{path}"], cwd=root, capture_output=True)
    if result.returncode == 0:
        return result.stdout
    if allow_missing and result.returncode == 128:
        return b""
    raise ValueError(f"cannot read committed file {revision}:{path}")


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
        old = committed_bytes(root, base, name, allow_missing=True)
        new = committed_bytes(root, "HEAD", name, allow_missing=True)
        if not new:
            raise ValueError(f"{unit}: golden removal requires an explicit reviewed retirement, not blessing")
        cases = committed_bytes(root, "HEAD", f"port/cases/{unit}.jsonl")
        errors = []
        for evidence_path in evidence_paths:
            try:
                value = json.loads(committed_bytes(root, "HEAD", evidence_path), object_pairs_hook=unique)
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


def case_ids(path, uid):
    # JSONL frames end only at LF, as in replies(); Unicode separators stay in strings.
    lines = path.read_bytes().decode("utf-8").split("\n")
    if lines[-1] == "":
        lines.pop()
    ids = [exact(line.removesuffix("\r")).get("case") for line in lines]
    if not ids or len(set(ids)) != len(ids) or not all(isinstance(i, str) and i for i in ids):
        raise ValueError(f"{uid}: invalid case identities")
    return ids


def verify_report(path, uid, mode, action, cases, units):
    """Check the runner's own report against the declared mode and corpus.

    A strict run must be green with every case compared. A partial run is accepted as
    green or partial: at least one exact comparison, no failures, and every remaining
    case listed as a deferral owned by an existing unit record. Returns the summary.
    """
    report = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique)
    accepted = {"strict": {"green"}, "partial": {"green", "partial"}}[mode]
    if (report.get("unit") != uid or report.get("mode") != mode or report.get("action") != action
            or report.get("status") not in accepted or report.get("failures") != 0):
        raise ValueError(f"{uid}: {mode} {action} report is not an accepted run: {report.get('status')}")
    compared, deferred = report.get("compared_cases"), report.get("deferred_cases")
    if not isinstance(compared, list) or not isinstance(deferred, list):
        raise ValueError(f"{uid}: report must list compared and deferred case IDs")
    deferred_ids = [d.get("case") if isinstance(d, dict) else None for d in deferred]
    order = {case: index for index, case in enumerate(cases)}
    for name, ids in (("compared", compared), ("deferred", deferred_ids)):
        if ids != sorted(ids, key=lambda case: order.get(case, -1)) or any(i not in order for i in ids):
            raise ValueError(f"{uid}: {name} case IDs do not follow the corpus")
    if (sorted(compared + deferred_ids, key=order.get) != cases or report.get("cases") != len(cases)
            or report.get("compared") != len(compared) or report.get("deferred") != len(deferred)):
        raise ValueError(f"{uid}: report counts do not account for every case exactly once")
    if not compared:
        raise ValueError(f"{uid}: all-deferred run proves no successful operation")
    if (report["status"] == "partial") != bool(deferred) or (mode == "strict" and deferred):
        raise ValueError(f"{uid}: report status contradicts its deferrals")
    for item in deferred:
        if (set(item) != {"case", "upstream", "unit"} or not isinstance(item["upstream"], str)
                or not item["upstream"]):
            raise ValueError(f"{uid}: invalid deferral record {item}")
        if item["unit"] not in units:
            raise ValueError(f"{uid}/{item['case']}: deferral owner {item['unit']!r} names no unit record")
    return {"declared_status": units[uid]["status"], "mode": mode, "action": action,
            "result": report["status"], "cases": len(cases), "compared": len(compared),
            "deferred": len(deferred)}


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
    results = {}
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
        # Partial mode follows the declared record only. Goldens follow a green
        # difftest, so a partial unit without one is compared live; one that has a
        # golden is checked against all of its reference replies.
        mode = "partial" if units[uid]["status"] == "partial" else "strict"
        golden = root / "port/goldens" / f"{uid}.jsonl"
        action = "compare" if mode == "partial" and not golden.exists() else "check"
        report = reports / f"{uid}.json"
        report.unlink(missing_ok=True)
        args = [str(runner), action, "--root", str(root), "--unit", uid,
                "--oracle", str(oracle), "--rust", str(shim), "--report", str(report)]
        if mode == "partial":
            args.append("--partial")
        rust_args = config.get("rust_args", [])
        if not isinstance(rust_args, list) or not all(isinstance(arg, str) for arg in rust_args):
            raise ValueError(f"{uid}: rust_args must be a string array")
        for arg in rust_args:
            args += ["--rust-arg", arg]
        returncode = subprocess.run(args, cwd=root).returncode
        if returncode != 0:
            raise ValueError(f"{uid}: {mode} live difftest {action} failed (exit {returncode}); see {report}")
        results[uid] = verify_report(report, uid, mode, action, case_ids(path, uid), units)
        tested.add(uid)
    if green - tested:
        raise ValueError(f"green units missing live cases: {sorted(green - tested)}")
    for golden in (root / "port/goldens").glob("*.jsonl"):
        if golden.stem not in tested:
            raise ValueError(f"{golden.stem}: orphan/untested golden")
    live_green = green & tested
    passed = sum(units[uid]["layer"] == 0 for uid in live_green)
    complete = required and passed >= 3 and bool(units)
    partial = sorted(uid for uid, result in results.items() if result["mode"] == "partial")
    deferred_cases = sum(results[uid]["deferred"] for uid in partial)
    summary = {"phase0": "COMPLETE" if complete else "INCOMPLETE", "strict_gate_enabled": required,
               "live_green_units": passed, "required_green_units": 3, "required_layer": 0,
               "total_live_green_units": len(live_green),
               "validated_unit_records": len(units), "extraction": provenance,
               "live_partial_units": len(partial), "partial_deferred_cases": deferred_cases,
               "partial_units_without_deferrals": [uid for uid in partial if not results[uid]["deferred"]],
               "units": results}
    (reports / "phase0.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    coverage = {key: provenance.get(key) for key in
                ("inventory_complete", "parse_complete", "compile_database_complete",
                 "all_configurations_complete", "gate_complete", "unresolved_call_count")}
    for key in ("parse_failures", "uncompiled_application_sources"):
        coverage[key] = len(provenance.get(key, []))
    message = (f"Phase 0: {summary['phase0']}; {passed}/3 live green layer-0 units; "
               f"{len(units)} validated unit records.\n\n"
               f"Partial units (never counted green): {len(partial)} live, "
               f"{deferred_cases} deferred cases; declared partial without corpus deferrals: "
               f"{summary['partial_units_without_deferrals']}.\n\n"
               f"Extraction/configuration coverage (reported separately): `{json.dumps(coverage, sort_keys=True)}`\n")
    print(message)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(message)
    if required and not complete:
        raise ValueError("phase0 strict gate requires at least three live green layer-0 units and all generated valid unit records")


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
