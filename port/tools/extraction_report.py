#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure completed raw/reconciled graphs without equating exclusions with parses."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import sys
import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import digest, read_json, write_json


def summarize(graph, inventory, stale=()):
    provenance = graph["provenance"]
    if inventory["provenance"] != provenance:
        raise ValueError("graph/inventory provenance differs")
    tus = graph["translation_units"]
    if len(tus) != provenance["commands_attempted"]:
        raise ValueError("translation-unit accounting differs from provenance")
    stale = set(stale)
    statuses = Counter(t["status"] for t in tus)
    exclusions = [{"file": t["file"], "recorded_status": t["status"],
                   "classification": "stale-deleted/excluded", "diagnostics": t["diagnostics"]}
                  for t in tus if t["file"] in stale]
    if any(t["status"] == "ok" for t in tus if t["file"] in stale):
        raise ValueError("deleted source incorrectly counted as a successful parse")
    coverage = defaultdict(Counter)
    for entry in inventory["files"]:
        coverage[entry["category"]][entry["coverage"]] += entry["lines"]
    return {
        "compile_commands_sha256": provenance["compile_commands_sha256"],
        "source_inventory_sha256": provenance["source_inventory_sha256"],
        "source_manifest_sha256": provenance["source_manifest_sha256"],
        "extractor_sha256": provenance["extractor_sha256"],
        "commands": provenance["compile_commands_count"], "attempted": len(tus),
        "parsed_ok": statuses["ok"], "recorded_statuses": dict(sorted(statuses.items())),
        "stale_deleted_excluded": exclusions,
        "non_stale_failures": [t for t in tus if t["status"] != "ok" and t["file"] not in stale],
        "coverage_flags": {k: provenance[k] for k in ("inventory_complete", "parse_complete",
            "compile_database_complete", "all_configurations_complete", "gate_complete")},
        "functions": len(graph["functions"]), "edges": len(graph["edges"]),
        "virtual_destructors": sum(f.get('virtual', False) and '::~' in f.get('name','') for f in graph['functions'].values()),
        "build_generated_definitions": sum(f['file'].startswith('@build/') for f in graph['functions'].values()),
        "implicit_lifetime_diagnostics": sum('implicit destructor' in c['expression'] or 'temporary lifetime' in c['expression']
                                              for c in graph.get('unresolved_calls', [])),
        "edge_kinds": dict(sorted(Counter(e[2] for e in graph["edges"]).items())),
        "override_relationships": len(graph["overrides"]),
        "template_aliases": len(graph.get("template_aliases", {})),
        "template_alias_variants": provenance.get("template_alias_variant_count", 0),
        "cache_reused_commands": provenance.get("cache_reused_commands", 0),
        "additional_definition_bodies": len(graph.get("definition_copies", {})),
        "conservative_range_count": sum(f.get("range_kind", "function") != "function"
                                        for f in graph["functions"].values()),
        "unresolved_call_sites": provenance["unresolved_call_count"],
        "function_wx_classes": dict(sorted(Counter(f["wx_class"] for f in graph["functions"].values()).items())),
        "source_files": len(inventory["files"]),
        "tracked_source_files": sum(f.get('origin') != 'build-generated' for f in inventory['files']),
        "build_generated_files": sum(f.get('origin') == 'build-generated' for f in inventory['files']),
        "tracked_source_lines_by_category": dict(sorted(Counter({c: sum(f['lines'] for f in inventory['files']
            if f['category'] == c and f.get('origin') != 'build-generated') for c in coverage}).items())),
        "build_generated_lines_by_category": dict(sorted(Counter({c: sum(f['lines'] for f in inventory['files']
            if f['category'] == c and f.get('origin') == 'build-generated') for c in coverage}).items())),
        "source_lines_by_category": dict(sorted(Counter({c: sum(values.values())
            for c, values in coverage.items()}).items())),
        "source_lines_by_category_and_coverage": {c: dict(sorted(values.items()))
            for c, values in sorted(coverage.items())},
        "uncompiled_application_sources": provenance["uncompiled_application_sources"],
    }


def measure_units(units, report):
    layers, classes = defaultdict(Counter), defaultdict(Counter)
    for unit in units:
        layers[unit["layer"]]["units"] += 1
        layers[unit["layer"]]["lines"] += unit["est_lines"]
        classes[unit["wx_class"]]["units"] += 1
        classes[unit["wx_class"]]["lines"] += unit["est_lines"]
    pilots = [{k: u[k] for k in ("id", "upstream", "source_ranges", "est_lines", "functions",
              "depends_on", "soft_depends_on", "wx_class", "status", "coverage_gaps")}
              for u in sorted(units, key=lambda u: u["id"])
              if u["layer"] == 0 and u["wx_class"] == "pure" and not u["coverage_gaps"] and u["functions"]]
    return {"unit_count": len(units), "accounted_lines": sum(u["est_lines"] for u in units),
            "statuses": dict(sorted(Counter(u["status"] for u in units).items())),
            "layers": {str(k): dict(v) for k, v in sorted(layers.items())},
            "wx_classes": {k: dict(v) for k, v in sorted(classes.items())},
            "back_edge_count": len(report["back_edges"]),
            "back_edge_weight": sum(e["weight"] for e in report["back_edges"]),
            "hard_edge_count": sum(len(u["depends_on"]) for u in units),
            "oversized_units": [{"id": u["id"], "lines": u["est_lines"]}
                                for u in sorted(units, key=lambda u: u["id"]) if u["est_lines"] > report["target_lines"]],
            "pure_l0_candidates": pilots}


def fingerprint(path):
    if not path.exists():
        path = path.with_suffix(path.suffix + ".gz")
    return {"filename": path.name, "bytes": path.stat().st_size, "sha256": digest(path.read_bytes())}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raw-output", type=Path, required=True)
    p.add_argument("--reconciled-output", type=Path, required=True)
    p.add_argument("--corrected-output", type=Path, help="separate post-review extraction; never replaces historical reconciled coverage")
    p.add_argument("--reconciliation", type=Path, required=True)
    p.add_argument("--units-dir", type=Path, default=Path("port/units"))
    p.add_argument("--units-report", type=Path, default=Path("port/extractor/generated/units.json"))
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    reconciliation = read_json(a.reconciliation)
    runs = {}
    directories = [("raw", a.raw_output), ("reconciled", a.reconciled_output)]
    if a.corrected_output:
        directories.append(('corrected', a.corrected_output))
    for name, directory in directories:
        graph, inventory = (read_json(directory / f"{file}.json") for file in ("graph", "inventory"))
        runs[name] = summarize(graph, inventory, reconciliation["stale_deleted_sources"] if name == "raw" else ())
        expected = reconciliation[f"{'reconciled' if name == 'corrected' else name}_compile_commands_sha256"]
        if runs[name]["compile_commands_sha256"] != expected:
            raise ValueError(f"{name} compile database hash differs from reconciliation")
        runs[name]["artifacts"] = [fingerprint(directory / f"{file}.json") for file in ("graph", "inventory")]
    if runs["raw"]["source_inventory_sha256"] != runs["reconciled"]["source_inventory_sha256"]:
        raise ValueError("raw and reconciled runs used different source inventories")
    if a.corrected_output:
        corrected = read_json(a.corrected_output / 'inventory.json')
        if corrected['provenance']['tracked_source_inventory_sha256'] != runs['raw']['source_inventory_sha256']:
            raise ValueError('corrected run changed the tracked source snapshot')
    units = [tomllib.loads(path.read_text(encoding="utf-8")) for path in sorted(a.units_dir.glob("*.toml"))]
    if not units:
        raise ValueError("no unit records to measure")
    unit_report = read_json(a.units_report)
    result = {"schema_version": 1, "runs": runs,
              "units": measure_units(units, unit_report),
              "canonical": {"artifacts": [fingerprint(a.units_report.parent / f"{file}.json")
                                          for file in ("graph", "inventory", "units")],
                            "range_normalizations": unit_report.get("range_normalizations", []),
                            "range_postprocessing": unit_report.get("range_postprocessing")},
              "reconciliation": reconciliation,
              "interpretation": "Raw and reviewed reconciled coverage remain historical. Corrected coverage, when present, is an independent extraction with its own AST/cache identity and generated-source inventory. Linux active branches only. No port or oracle parity is claimed."}
    write_json(a.output, result)
    print(json.dumps({"output": str(a.output), "unit_count": len(units),
                      "raw_parsed_ok": runs["raw"]["parsed_ok"],
                      "reconciled_parsed_ok": runs["reconciled"]["parsed_ok"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
