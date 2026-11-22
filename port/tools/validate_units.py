#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate all unit records, graph ownership, dependency layers and source accounting.

Default mode checks structural correctness, including honest incomplete coverage.
--require-complete additionally gates on WSL command/parse completeness.
No clang installation or upstream build is needed in CI.
"""
import argparse
from collections import defaultdict
import json
from pathlib import Path, PurePosixPath
import re
import sys
import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import CLASSES, PIN, classify, digest, expand_virtual, layers, read_json
from split_units import CORE
from extractor.generated_sources import source_path

STATUSES = {"todo", "claimed", "partial", "green", "blocked"}


def validate(root, require_complete=False, check_sources=True, source_root=None):
    errors, units = [], {}
    directory = root / "port/units"
    paths = sorted(directory.glob("*.toml"))
    if not paths:
        return ["No unit TOML files found"]
    for path in paths:
        try:
            unit = tomllib.loads(path.read_text(encoding="utf-8"))
        except (ValueError, OSError) as exc:
            errors.append(f"{path.name}: {exc}")
            continue
        uid = unit.get("id")
        if uid != path.stem or not re.fullmatch(r"[a-z0-9_.-]+", str(uid)):
            errors.append(f"{path.name}: id must match filename and use safe characters")
            continue
        units[uid] = unit
        for field in CORE:
            if field not in unit:
                errors.append(f"{uid}: missing {field}")
        for key in ("id", "rust_module", "status", "claimed_by", "claimed_at", "notes", "wx_class", "source_category"):
            if not isinstance(unit.get(key), str):
                errors.append(f"{uid}: {key} must be a string")
        for key in ("upstream", "depends_on", "soft_depends_on", "tests", "functions", "source_ranges", "coverage_gaps"):
            value = unit.get(key)
            if not isinstance(value, list) or any(not isinstance(x, str) for x in value):
                errors.append(f"{uid}: {key} must be a string array")
            elif len(value) != len(set(value)):
                errors.append(f"{uid}: duplicate {key}")
        for key in ("est_lines", "layer"):
            if type(unit.get(key)) is not int or unit[key] < (1 if key == "est_lines" else 0):
                errors.append(f"{uid}: invalid {key}")
        if unit.get("status") not in STATUSES:
            errors.append(f"{uid}: invalid status")
        if unit.get("wx_class") not in CLASSES:
            errors.append(f"{uid}: invalid wx classification")
        if unit.get("source_category") not in ("application", "generated", "thirdparty"):
            errors.append(f"{uid}: invalid source category")
        if unit.get("schema_version") != 1:
            errors.append(f"{uid}: invalid schema_version")
        if unit.get("status") == "claimed" and (not unit.get("claimed_by") or not unit.get("claimed_at")):
            errors.append(f"{uid}: claimed requires owner and timestamp")
        if unit.get("status") == "green" and (not unit.get("tests") or unit.get("coverage_gaps")):
            errors.append(f"{uid}: green requires tests and no coverage gaps")
    if errors:
        return errors
    generated = root / "port/extractor/generated"
    try:
        graph = read_json(generated / "graph.json")
        inventory = json.loads((generated / "inventory.json").read_text(encoding="utf-8"))
        report = json.loads((generated / "units.json").read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return errors + [f"generated input: {exc}"]
    provenance = graph.get("provenance", {})
    if provenance.get("upstream_pin") != PIN or inventory.get("provenance") != provenance:
        errors.append("missing/mismatched upstream pin or inventory provenance")
    for key in ("inventory_complete", "parse_complete", "compile_database_complete", "all_configurations_complete", "gate_complete"):
        if type(provenance.get(key)) is not bool:
            errors.append(f"provenance: missing boolean {key}")
    if require_complete and not provenance.get("gate_complete"):
        errors.append("WSL extraction diagnostic incomplete; inspect provenance coverage and parse failures")
    if bool(provenance.get("parse_failures")) and provenance.get("parse_complete"):
        errors.append("parse failures hidden by parse_complete=true")
    if provenance.get("gate_complete") and not all(provenance.get(k) for k in
            ("inventory_complete", "parse_complete", "compile_database_complete")):
        errors.append("extractor diagnostic contradicts coverage fields")
    translation_units = graph.get("translation_units")
    if translation_units is not None:
        if len(translation_units) != provenance.get("commands_attempted"):
            errors.append("translation-unit count differs from provenance")
        if provenance.get("compile_database_complete") and len(translation_units) != provenance.get("compile_commands_count"):
            errors.append("compile database silently omitted commands")
        if provenance.get("parse_complete") and any(t["status"] != "ok" for t in translation_units):
            errors.append("translation-unit parse failures hidden")
    files = {f["path"]: f for f in inventory["files"]}
    if len(files) != len(inventory["files"]):
        errors.append("duplicate inventory path")
    for usr, fn in graph["functions"].items():
        if fn["file"] not in files:
            errors.append(f"function source absent from inventory: {usr}")
    manifest_path = generated / "source-manifest.json"
    if manifest_path.exists():
        manifest = read_json(manifest_path)
        tracked = {p for p, f in files.items() if f.get('origin') != 'build-generated'}
        if set(manifest["files"]) != tracked or len(manifest["files"]) != len(tracked):
            errors.append("tracked source manifest differs from inventory")
        if manifest["source_commit"] != provenance.get("source_commit") or manifest["upstream_pin"] != PIN:
            errors.append("source manifest provenance mismatch")
        manifest_bytes = manifest_path.read_bytes()
        if provenance.get("source_manifest_hash_policy") == "SHA-256 after CRLF-to-LF normalization":
            manifest_bytes = manifest_bytes.replace(b"\r\n", b"\n")
        if digest(manifest_bytes) != provenance.get("source_manifest_sha256"):
            errors.append("source manifest fingerprint mismatch")
    if digest(json.dumps([{k: v for k, v in f.items() if k != "coverage"} for f in inventory["files"]], sort_keys=True).encode()) != provenance.get("source_inventory_sha256"):
        errors.append("inventory fingerprint mismatch")
    build_files = {p: f for p, f in files.items() if f.get('origin') == 'build-generated'}
    if build_files:
        manifest = read_json(generated / 'generated-sources.json')
        if digest((generated / 'generated-sources.json').read_bytes().replace(b'\r\n', b'\n')) != provenance.get('generated_sources_manifest_sha256'):
            errors.append('generated source manifest fingerprint mismatch')
        expected = [{k: v for k, v in f.items() if k != 'coverage'} for f in build_files.values()]
        if manifest['files'] != expected or manifest['upstream_pin'] != PIN:
            errors.append('generated source inventory/provenance mismatch')
        generation = manifest.get('provenance', {})
        configuration = generation.get('reconciliation.json', {}).get('record')
        if configuration is not None and (configuration.get('source_commit') != provenance.get('source_commit')
                or configuration.get('upstream_pin') != PIN):
            errors.append('generated configure source pin differs from extraction')
        recipes = generated / 'generated-recipes.json'
        if manifest.get('recipes_sha256') and digest(recipes.read_bytes().replace(b'\r\n', b'\n')) != manifest['recipes_sha256']:
            errors.append('generated recipes fingerprint mismatch')
        for entry in build_files.values():
            source_path(entry, source_root or root, generated)
        tracked_inventory = [{k:v for k,v in f.items() if k != 'coverage'} for f in inventory['files']
                             if f.get('origin') != 'build-generated']
        if digest(json.dumps(tracked_inventory, sort_keys=True).encode()) != provenance.get('tracked_source_inventory_sha256'):
            errors.append('tracked source inventory fingerprint mismatch')
    ownership, ranges = {}, defaultdict(list)
    for uid, unit in units.items():
        for path in unit["upstream"]:
            if path not in files or PurePosixPath(path).is_absolute() or ".." in PurePosixPath(path).parts:
                errors.append(f"{uid}: unknown/unsafe upstream path {path}")
        if not re.fullmatch(r"[a-z_][a-z0-9_]*(?:::[a-z_][a-z0-9_]*)+", unit["rust_module"]):
            errors.append(f"{uid}: invalid Rust module")
        for dep in unit["depends_on"] + unit["soft_depends_on"]:
            if dep not in units or dep == uid:
                errors.append(f"{uid}: unknown/self dependency {dep}")
        if set(unit["depends_on"]) & set(unit["soft_depends_on"]):
            errors.append(f"{uid}: dependency both hard and soft")
        total, owned_paths = 0, set()
        for location in unit["source_ranges"]:
            try:
                path, start, end = location.rsplit(":", 2)
                start, end = int(start), int(end)
                if path not in unit["upstream"] or not 1 <= start <= end <= files[path]["lines"]:
                    raise ValueError("invalid range")
                ranges[path].append((start, end, uid))
                owned_paths.add(path)
                total += end - start + 1
            except (ValueError, KeyError):
                errors.append(f"{uid}: invalid source range {location}")
        if total != unit["est_lines"]:
            errors.append(f"{uid}: est_lines differs from measured ranges")
        if any(files[p]["category"] != unit["source_category"] for p in owned_paths):
            errors.append(f"{uid}: source category differs from owned inventory ranges")
        expected_gaps = {p for p in owned_paths if files[p]["coverage"] != "parsed"}
        if set(unit["coverage_gaps"]) != expected_gaps:
            errors.append(f"{uid}: coverage gaps differ from owned inventory ranges")
        if expected_gaps and unit["wx_class"] != "UI":
            errors.append(f"{uid}: unparsed source ranges require conservative UI classification")
        for usr in unit["functions"]:
            if usr not in graph["functions"] or usr in ownership:
                errors.append(f"{uid}: unknown or multiply owned function {usr}")
                continue
            ownership[usr] = uid
            fn = graph["functions"][usr]
            if not any(p == fn["file"] and s <= fn["start"] <= fn["end"] <= e
                       for p, s, e in ( (r.rsplit(":", 2)[0], int(r.rsplit(":", 2)[1]), int(r.rsplit(":", 2)[2]))
                                       for r in unit["source_ranges"])):
                errors.append(f"{uid}: function outside owned ranges: {usr}")
            if CLASSES.index(fn["wx_class"]) > CLASSES.index(unit["wx_class"]):
                errors.append(f"{uid}: wx coupling understated")
    for path, entry in files.items():
        if entry["category"] in ("qa", "thirdparty") and not path.startswith("thirdparty/clipper2/"):
            continue
        cursor = 1
        for start, end, uid in sorted(ranges[path]):
            if start != cursor:
                errors.append(f"{path}: gap/overlap before {start} ({uid})")
            cursor = end + 1
        if cursor != entry["lines"] + 1:
            errors.append(f"{path}: source lines unaccounted")
    if ownership != report.get("function_units"):
        errors.append("unit function ownership differs from generated report")
    required = {u for u, f in graph["functions"].items() if f["file"] in ranges}
    if set(ownership) != required:
        errors.append("application functions missing from units")
    back = {(e["caller"], e["callee"]) for e in report["back_edges"]}
    soft = {(uid, dep) for uid, u in units.items() for dep in u["soft_depends_on"]}
    if back != soft:
        errors.append("soft dependencies do not exactly match recorded back-edges")
    actual_calls = defaultdict(list)
    for caller, callee, kind in graph["edges"]:
        a, b = ownership.get(caller), ownership.get(callee)
        if kind not in ("static", "virtual", "override"):
            errors.append(f"invalid call kind {kind}")
        if caller not in graph["functions"]:
            errors.append(f"call from unknown function {caller}")
        if a and b and a != b:
            actual_calls[a, b].append([caller, callee, kind])
            if b not in units[a]["depends_on"] and (a, b) not in soft:
                errors.append(f"unrecorded cross-unit call {a} -> {b}")
    hard = {(uid, dep) for uid, u in units.items() for dep in u["depends_on"]}
    if hard | soft != set(actual_calls):
        errors.append("unit dependencies differ from cross-unit graph calls")
    for edge in report["back_edges"]:
        calls = actual_calls[edge["caller"], edge["callee"]]
        if sorted(calls) != edge.get("calls") or edge["weight"] != sum(1 if c[2] == "override" else 10 for c in calls):
            errors.append("back-edge calls/weight not preserved")
    if "overrides" in graph:
        virtual_calls = [(a, b, k == "virtual") for a, b, k in graph["edges"] if k != "override"]
        if expand_virtual(virtual_calls, graph["overrides"]) != sorted(tuple(e) for e in graph["edges"]):
            errors.append("virtual override expansion incomplete or inconsistent")
    expected_classification = {usr: {"wx": fn.get("wx", [])} for usr, fn in graph["functions"].items()}
    classify(expected_classification, graph["edges"])
    for usr, expected in expected_classification.items():
        if graph["functions"][usr]["wx_class"] != expected["wx_class"]:
            errors.append(f"incorrect propagated wx classification: {usr}")
    if report.get("unit_count") != len(units) or report.get("accounted_lines") != sum(u["est_lines"] for u in units.values()):
        errors.append("splitter summary count/lines mismatch")
    try:
        actual = layers({uid: set(u["depends_on"]) for uid, u in units.items()})
        for uid, layer in actual.items():
            if units[uid]["layer"] != layer:
                errors.append(f"{uid}: wrong deterministic topological layer")
    except (ValueError, KeyError) as exc:
        errors.append(str(exc))
    if check_sources:
        for path, entry in files.items():
            source = source_path(entry, source_root or root, generated)
            if not source.is_file() or digest(source.read_bytes().replace(b"\r\n", b"\n")) != entry["sha256"]:
                errors.append(f"source changed since extraction: {path}; regenerate graph or use drift_report")
            elif entry.get('origin') == 'build-generated' and (len(source.read_bytes().splitlines()) != entry['lines']
                    or digest(source.read_bytes()) != entry['bytes_sha256']):
                errors.append(f'generated source byte/line fingerprint mismatch: {path}')
    return sorted(set(errors))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--require-complete", action="store_true")
    parser.add_argument("--source-root", type=Path, help="optional byte-equivalent source snapshot for hash checks")
    parser.add_argument("--skip-source-hashes", action="store_true", help="validate historical graph after upstream edits")
    parser.add_argument("--json", action="store_true")
    a = parser.parse_args()
    try:
        errors = validate(a.root, a.require_complete, not a.skip_source_hashes, a.source_root)
    except (KeyError, TypeError, ValueError, OSError) as exc:
        errors = [f"Malformed extraction data: {exc}"]
    print(json.dumps({"valid": not errors, "errors": errors}) if a.json else
          "\n".join(errors) if errors else "All unit records and source accounting valid")
    return bool(errors)


if __name__ == "__main__":
    sys.exit(main())
