#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate solidcore-compatible units and an auditable weighted cycle break."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import sys
import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import CLASSES, identifier, layers, weighted_dag, write_json, read_json, write_graph

CORE = ("id", "upstream", "rust_module", "depends_on", "soft_depends_on", "est_lines",
        "layer", "tests", "status", "claimed_by", "claimed_at", "notes")


def split(graph, inventory, target=2000):
    functions = graph["functions"]
    by_file = defaultdict(list)
    for usr, f in sorted(functions.items()):
        by_file[f["file"]].append((usr, f))
    bins = defaultdict(list)
    files = {f["path"]: f for f in inventory["files"]}
    for path, entry in sorted(files.items()):
        if entry["category"] in ("qa", "thirdparty") and not path.startswith("thirdparty/clipper2/"):
            continue
        fns = sorted(by_file[path], key=lambda pair: (pair[1]["start"], pair[0]))
        start = 1
        while start <= entry["lines"]:
            end = min(start + target - 1, entry["lines"])
            # Never split a function, including nested lambda/template spans.
            while True:
                crossing = [f["end"] for _, f in fns if f["start"] <= end < f["end"]]
                if not crossing:
                    break
                end = max(end, *crossing)
            owned = [usr for usr, f in fns if start <= f["start"] <= end]
            coupling = max((functions[u]["wx_class"] for u in owned),
                           key=CLASSES.index, default="pure")
            # Small math source files are useful independent leaf units; other
            # files share directory/classification bins near the target budget.
            group = str(Path(path).with_suffix("")) if path.startswith("libs/kimath/src/") else str(Path(path).parent)
            bins[(group, coupling, entry["category"])].append(
                {"file": path, "start": start, "end": end, "functions": owned})
            start = end + 1
    units, ownership = {}, {}
    for (group, coupling, category), records in sorted(bins.items()):
        chunks, current, size = [], [], 0
        for record in records:
            lines = record["end"] - record["start"] + 1
            if current and size + lines > target:
                chunks.append(current)
                current, size = [], 0
            current.append(record)
            size += lines
        if current:
            chunks.append(current)
        for number, chunk in enumerate(chunks, 1):
            uid = f"{identifier(group) or 'root'}.{identifier(coupling)}.p{number:03}"
            ids = sorted(u for r in chunk for u in r["functions"])
            # Missing configurations are explicit blockers, even when a file
            # has declarations available through another translation unit.
            gaps = sorted({r["file"] for r in chunk if files[r["file"]]["coverage"] != "parsed"})
            units[uid] = {"id": uid, "upstream": sorted({r["file"] for r in chunk}),
                          "rust_module": "kicad_port::" + "::".join(
                              "unit_" + part if part[0].isdigit() or part in
                              {"type", "mod", "fn", "impl", "match", "loop", "move", "ref", "self", "super", "crate", "in", "use"}
                              else part for part in uid.replace("-", "_").split(".")),
                          "depends_on": [], "soft_depends_on": [],
                          "est_lines": sum(r["end"] - r["start"] + 1 for r in chunk),
                          "layer": 0, "tests": [], "status": "blocked" if gaps else "todo",
                          "claimed_by": "", "claimed_at": "",
                          "notes": "Extraction coverage gaps: " + ", ".join(gaps) if gaps else "Awaiting port and oracle cases.",
                          "schema_version": 1, "wx_class": coupling, "source_category": category,
                          "functions": ids, "source_ranges": [f"{r['file']}:{r['start']}:{r['end']}" for r in chunk],
                          "coverage_gaps": gaps}
            for usr in ids:
                ownership[usr] = uid
    weights = Counter()
    call_records = defaultdict(list)
    for caller, callee, kind in graph["edges"]:
        a, b = ownership.get(caller), ownership.get(callee)
        if a and b and a != b:
            weights[a, b] += 1 if kind == "override" else 10
            call_records[a, b].append([caller, callee, kind])
    adj, dropped = weighted_dag(units, weights)
    levels = layers(adj)
    soft = defaultdict(set)
    for edge in dropped:
        soft[edge["caller"]].add(edge["callee"])
        edge["calls"] = sorted(call_records[edge["caller"], edge["callee"]])
    for uid, unit in units.items():
        unit["depends_on"] = sorted(adj[uid])
        unit["soft_depends_on"] = sorted(soft[uid])
        unit["layer"] = levels[uid]
    return units, {"schema_version": 1, "target_lines": target,
                   "weight_policy": {"static": 10, "virtual": 10, "override": 1},
                   "algorithm": "descending-weight greedy acyclic insertion; deterministic lexical ties; not minimum",
                   "back_edges": dropped, "function_units": ownership,
                   "unit_count": len(units), "accounted_lines": sum(u["est_lines"] for u in units.values()),
                   "unassigned_functions": sorted(set(functions) - set(ownership)),
                   "excluded_categories": ["qa", "thirdparty (except clipper2)"],
                   "classification_counts": dict(Counter(u["wx_class"] for u in units.values()))}


def dump(unit):
    keys = list(CORE) + sorted(set(unit) - set(CORE))
    return "# Generated by port/tools/split_units.py; edit status/claims/tests/notes when porting.\n" + "".join(
        f"{k} = {json.dumps(unit[k], ensure_ascii=True)}\n" for k in keys)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph", type=Path, default=Path("port/extractor/generated/graph.json"))
    parser.add_argument("--inventory", type=Path, default=Path("port/extractor/generated/inventory.json"))
    parser.add_argument("--units-dir", type=Path, default=Path("port/units"))
    parser.add_argument("--report", type=Path, default=Path("port/extractor/generated/units.json"))
    parser.add_argument("--target-lines", type=int, default=2000)
    a = parser.parse_args()
    if a.target_lines < 1:
        parser.error("target lines must be positive")
    graph, inventory = read_json(a.graph), read_json(a.inventory)
    units, report = split(graph, inventory, a.target_lines)
    a.units_dir.mkdir(parents=True, exist_ok=True)
    stale = {p.stem for p in a.units_dir.glob("*.toml")} - set(units)
    if stale:
        raise ValueError("Stale units require explicit reconciliation: " + ", ".join(sorted(stale)))
    for uid, unit in sorted(units.items()):
        path = a.units_dir / (uid + ".toml")
        if path.exists():
            previous = tomllib.loads(path.read_text())
            for key in ("status", "claimed_by", "claimed_at", "tests", "notes"):
                unit[key] = previous[key]
        path.write_text(dump(unit), encoding="utf-8")
    write_json(a.report, report)
    # Older local extraction runs may have written plain JSON; canonical graph
    # storage is deterministic gzip, while read_json accepts either form.
    write_graph(a.graph if a.graph.suffix != ".gz" else a.graph.with_suffix(""), graph)
    if a.graph.suffix != ".gz" and a.graph.exists():
        a.graph.unlink()
    print(f"{len(units)} units, {report['accounted_lines']} physical upstream lines; {len(report['back_edges'])} back-edges")


if __name__ == "__main__":
    main()
