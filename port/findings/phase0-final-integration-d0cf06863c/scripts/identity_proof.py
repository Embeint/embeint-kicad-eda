#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prove which committed layer every integrated path comes from.

For each layer X (39, 42, 43, 44) a path is X-owned when X's blob differs from
every base X could have inherited it from (current rust, X's rust merge base,
the earlier corrected #39 snapshot and final #39). HEAD must hold exactly the
owning blob; paths owned by several layers with different blobs are the
resolved merges, listed for review. Unit records are compared field by field
against final #39.
"""
import json
from pathlib import Path
import subprocess
import sys
import tomllib

repo = Path(sys.argv[1]).resolve()
layers = json.loads(sys.argv[2])  # {"39": sha, "42": sha, ...}
rust = sys.argv[3]
old39 = "19035296e129cf8640e89331bc9e8ad84f0fe38c"


def git(*args):
    return subprocess.run(["git", "-c", "core.autocrlf=false", *args], cwd=repo, check=True,
                          capture_output=True, text=True, encoding="utf-8").stdout


def tree(revision):
    result = {}
    for row in git("ls-tree", "-r", "-z", "--full-tree", revision).split("\0"):
        if row:
            meta, path = row.split("\t", 1)
            result[path] = meta.split()[2]
    return result


head = git("rev-parse", "HEAD").strip()
trees = {name: tree(sha) for name, sha in layers.items()}
base_trees = {"rust": tree(rust), "old39": tree(old39)}
head_tree = tree("HEAD")
own = {}
for name, sha in layers.items():
    bases = [base_trees["rust"], trees["39"]]
    if name != "39":
        bases += [tree(git("merge-base", sha, rust).strip()), base_trees["old39"]]
    paths = set(trees[name]) | set().union(*(set(b) for b in bases))
    own[name] = {p for p in paths if all(trees[name].get(p) != b.get(p) for b in bases)}
    if name == "39":
        own[name] = {p for p in paths if trees["39"].get(p) != base_trees["rust"].get(p)}

report = {"head": head, "rust": rust, "layers": layers, "owned_counts": {k: len(v) for k, v in own.items()},
          "exact": 0, "resolved": [], "head_differs_from_sole_owner": [], "unowned_changes": []}
for path in sorted(set(head_tree) | set().union(*own.values())):
    owners = {name: trees[name].get(path) for name in layers if path in own[name]}
    expected = set(owners.values())
    if len(expected) > 1:
        report["resolved"].append({"path": path, "owners": sorted(owners),
                                   "head_blob": head_tree.get(path)})
        continue
    if owners:
        if head_tree.get(path) == next(iter(expected)):
            report["exact"] += 1
        else:
            report["head_differs_from_sole_owner"].append(
                {"path": path, "owners": sorted(owners), "owner_blob": next(iter(expected)),
                 "head_blob": head_tree.get(path)})
    elif head_tree.get(path) != base_trees["rust"].get(path):
        report["unowned_changes"].append(path)

# Unit records: every protected field identical to final #39.
editable = {"rust_module", "tests", "status", "claimed_by", "claimed_at", "notes", "difftest"}
units = [p for p in head_tree if p.startswith("port/units/") and p.endswith(".toml")]
changed_records, protected_diffs = [], []
for path in sorted(units):
    if head_tree[path] == trees["39"].get(path):
        continue
    new = tomllib.loads(git("show", f"HEAD:{path}"))
    old = tomllib.loads(git("show", f"{layers['39']}:{path}"))
    changed_records.append({"path": path, "status": new.get("status"),
                            "fields": sorted(k for k in set(new) | set(old) if new.get(k) != old.get(k))})
    for key in set(new) | set(old):
        if key not in editable and new.get(key) != old.get(key):
            protected_diffs.append({"path": path, "field": key})
report.update({"unit_records": len(units), "unit_records_identical_to_39": len(units) - len(changed_records),
               "changed_unit_records": changed_records, "protected_field_differences": protected_diffs})
print(json.dumps(report, indent=2))
