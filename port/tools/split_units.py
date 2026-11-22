#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate solidcore-compatible units and an auditable weighted cycle break."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path, PurePosixPath
import sys
import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import CLASSES, digest, identifier, layers, weighted_dag, write_json, read_json, write_graph

CORE = ("id", "upstream", "rust_module", "depends_on", "soft_depends_on", "est_lines",
        "layer", "tests", "status", "claimed_by", "claimed_at", "notes")
PORT_FIELDS = ("rust_module", "status", "claimed_by", "claimed_at", "tests", "notes", "difftest")


def normalize_eof_ranges(graph, inventory, source_root):
    """Convert proven exclusive EOF locations to inclusive physical lines.

    A null function extent can recover to the translation unit's range, whose
    end is the empty position after the final newline. Preserve that clang
    observation and verify the original bytes before changing the line bound.
    """
    files = {entry["path"]: entry for entry in inventory["files"]}
    changes = []
    for usr, fn in sorted(graph["functions"].items()):
        entry = files.get(fn["file"])
        if not entry or fn["end"] <= entry["lines"]:
            continue
        data = (source_root / fn["file"]).read_bytes()
        if (fn.get("range_kind") != "enclosing-declaration"
                or fn["end"] != entry["lines"] + 1 or not data.endswith(b"\n")
                or fn.get("end_offset") != len(data)
                or digest(data.replace(b"\r\n", b"\n")) != entry["sha256"]
                or digest(data[fn["start_offset"]:fn["end_offset"]].replace(b"\r\n", b"\n")) != fn["sha256"]):
            raise ValueError(f"Unproven out-of-file function range: {usr}")
        old = fn["end"]
        fn.update(clang_end_line=old, end=entry["lines"], lines=entry["lines"] - fn["start"] + 1)
        changes.append({"function": usr, "file": fn["file"], "clang_end_line": old,
                        "physical_end_line": fn["end"], "end_offset": fn["end_offset"],
                        "source_bytes_sha256": digest(data)})
    return changes


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
            if entry["coverage"] != "parsed":
                coupling = "UI"
            # Small math source files are useful independent leaf units; other
            # files share directory/classification bins near the target budget.
            group = str(PurePosixPath(path).with_suffix("")) if path.startswith("libs/kimath/src/") else str(PurePosixPath(path).parent)
            bins[(group, coupling, entry["category"])].append(
                {"file": path, "start": start, "end": end, "functions": owned})
            start = end + 1
    units, ownership = {}, {}
    prefixes = Counter((identifier(g) or "root", c, category) for g, c, category in bins)
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
            prefix = identifier(group) or "root"
            if prefixes[prefix, coupling, category] > 1:
                prefix += "." + digest(group.encode())[:8]
            uid = f"{prefix}.{category}.{identifier(coupling)}.p{number:03}"
            if uid in units:
                raise ValueError(f"Unit ID collision: {uid}")
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
    tables = [key for key in keys if isinstance(unit[key], dict)]
    text = "# Generated by port/tools/split_units.py; edit status/claims/tests/notes when porting.\n" + "".join(
        f"{key} = {json.dumps(unit[key], ensure_ascii=True)}\n" for key in keys if key not in tables)
    for key in tables:
        text += f"\n[{key}]\n"
        for name, value in sorted(unit[key].items()):
            if isinstance(value, dict):
                raise ValueError(f"Unsupported nested unit table: {key}.{name}")
            text += f"{name} = {json.dumps(value, ensure_ascii=True)}\n"
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph", type=Path, default=Path("port/extractor/generated/graph.json"))
    parser.add_argument("--inventory", type=Path, default=Path("port/extractor/generated/inventory.json"))
    parser.add_argument("--units-dir", type=Path, default=Path("port/units"))
    parser.add_argument("--report", type=Path, default=Path("port/extractor/generated/units.json"))
    parser.add_argument("--target-lines", type=int, default=2000)
    parser.add_argument("--source-root", type=Path, help="original snapshot bytes for proving exclusive EOF ranges")
    a = parser.parse_args()
    if a.target_lines < 1:
        parser.error("target lines must be positive")
    graph, inventory = read_json(a.graph), read_json(a.inventory)
    source_root = a.source_root or Path(graph.get("provenance", {}).get("source_root", "."))
    if not source_root.is_dir():
        source_root = Path(__file__).resolve().parents[2]
    corrections = normalize_eof_ranges(graph, inventory, source_root)
    if corrections:
        original = a.graph if a.graph.exists() else a.graph.with_suffix(a.graph.suffix + ".gz")
        graph["range_normalizations"] = corrections
        graph["range_postprocessing"] = {"policy": "byte-verified exclusive EOF to inclusive physical line",
            "input_graph_sha256": digest(original.read_bytes()),
            "splitter_sha256": digest(Path(__file__).read_bytes())}
    units, report = split(graph, inventory, a.target_lines)
    report["range_normalizations"] = graph.get("range_normalizations", [])
    report["range_postprocessing"] = graph.get("range_postprocessing")
    a.units_dir.mkdir(parents=True, exist_ok=True)
    stale = {p.stem for p in a.units_dir.glob("*.toml")} - set(units)
    if stale:
        raise ValueError("Stale units require explicit reconciliation: " + ", ".join(sorted(stale)))
    for uid, unit in sorted(units.items()):
        path = a.units_dir / (uid + ".toml")
        if path.exists():
            previous = tomllib.loads(path.read_text(encoding="utf-8"))
            for key in PORT_FIELDS:
                if key in previous:
                    unit[key] = previous[key]
        path.write_text(dump(unit), encoding="utf-8", newline="\n")
    write_json(a.report, report)
    # Older local extraction runs may have written plain JSON; canonical graph
    # storage is deterministic gzip, while read_json accepts either form.
    write_graph(a.graph if a.graph.suffix != ".gz" else a.graph.with_suffix(""), graph)
    if a.graph.suffix != ".gz" and a.graph.exists():
        a.graph.unlink()
    print(f"{len(units)} units, {report['accounted_lines']} physical upstream lines; {len(report['back_edges'])} back-edges")


if __name__ == "__main__":
    main()
