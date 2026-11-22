#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Report upstream changes touching ported functions, relative to the wave pin.

Uses zero-context git diff hunks against PIN, not extraction-time line numbers:
the fork may already differ from PIN. Deleted/renamed files and declaration-only
changes are reported conservatively. Function-pointer analysis is not required.
"""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import posixpath
import re
import subprocess
import sys
import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import PIN, write_json, read_json


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], encoding="utf-8")


def hunks(diff):
    return [(int(m[0]), int(m[1] or "1"), int(m[2]), int(m[3] or "1")) for m in re.findall(
        r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", diff, re.M)]


def touched(start, end, changes):
    return any((n > 0 and start <= pos + n - 1 and pos <= end)
               or (n == 0 and start <= pos <= end) for pos, n in changes)


def map_range(start, end, changes):
    """Map extraction source lines back to the pin; changed spans are conservative."""
    def point(line, upper=False):
        delta = 0
        for old, old_n, new, new_n in changes:
            if new_n and new <= line < new + new_n:
                return old + max(old_n - 1, 0) if upper else old
            if new_n == 0 and line <= new:
                break
            if new + new_n > line:
                break
            delta += old_n - new_n
        return max(1, line + delta)
    return point(start), point(end, True)


def report(root, graph, units, target="HEAD", include_todo=False):
    pin = graph["provenance"]["upstream_pin"]
    if pin != PIN:
        raise ValueError("Unexpected upstream pin")
    base = graph["provenance"]["source_commit"]
    git(root, "rev-parse", "--verify", pin + "^{commit}")
    git(root, "rev-parse", "--verify", base + "^{commit}")
    target_sha = git(root, "rev-parse", "--verify", target + "^{commit}").strip()
    changed_paths = set(git(root, "diff", "--name-only", "--no-renames", "-z", pin, target_sha).split("\0")) - {""}
    by_file = defaultdict(list)
    for unit in units:
        if not include_todo and unit["status"] not in ("partial", "green"):
            continue
        for usr in unit["functions"]:
            if usr not in graph["functions"]:
                raise ValueError(f"Unknown function in {unit['id']}: {usr}")
            by_file[graph["functions"][usr]["file"]].append((unit["id"], usr))
    affected, files = [], []
    for path, entries in sorted(by_file.items()):
        if path not in changed_paths:
            continue
        diff = git(root, "diff", "--no-ext-diff", "--no-renames", "--unified=0", pin, target_sha, "--", path)
        if not diff:
            continue
        # Map ranges from the original extraction commit, not a mutable worktree.
        baseline_diff = git(root, "diff", "--no-ext-diff", "--no-renames", "--unified=0", pin, base, "--", path)
        mapping = hunks(baseline_diff)
        changes = hunks(diff)
        old_ranges = [(old, count) for old, count, _, _ in changes]
        files.append({"path": path, "change": "deleted" if "+++ /dev/null" in diff else
                      "added" if "--- /dev/null" in diff else "modified",
                      "hunks": changes})
        matched = False
        for uid, usr in entries:
            fn = graph["functions"][usr]
            start, end = map_range(fn["start"], fn["end"], mapping)
            if touched(start, end, old_ranges) or "+++ /dev/null" in diff or "--- /dev/null" in diff:
                affected.append({"unit": uid, "usr": usr, "function": fn["name"], "file": path,
                                 "pin_range": [start, end], "extracted_range": [fn["start"], fn["end"]],
                                 "reason": "body/signature range overlaps upstream change"})
                matched = True
        if not matched:
            # Includes, macros, type/field declarations and globals can alter a
            # body without changing its text; require unit re-checks explicitly.
            for uid in sorted({uid for uid, _ in entries}):
                affected.append({"unit": uid, "file": path, "function": None,
                                 "reason": "file changed outside recorded function ranges; dependency review required"})
    # Headers/declarations owned by ported units need review even with no
    # function definition in the file.
    seen = {(a["unit"], a["file"]) for a in affected}
    for unit in sorted(units, key=lambda u: u["id"]):
        if not include_todo and unit["status"] not in ("partial", "green"):
            continue
        function_files = {graph["functions"][usr]["file"] for usr in unit["functions"]}
        for path in unit["upstream"]:
            if path not in changed_paths or (unit["id"], path) in seen or path in function_files:
                continue
            affected.append({"unit": unit["id"], "file": path, "function": None,
                             "reason": "owned declarations/source changed; unit review required"})
    input_changes = defaultdict(set)
    generated_dependencies = graph['provenance'].get('generated_input_dependencies', {})
    changed_cmake = {p for p in changed_paths if p.endswith(('CMakeLists.txt', '.cmake'))}
    def recipe_inputs(paths):
        pending, seen = list(paths), set()
        while pending:
            path = posixpath.normpath(pending.pop())
            if path in seen:
                continue
            seen.add(path)
            pending.extend(generated_dependencies.get(path, []))
        return seen
    for tu in graph.get("translation_units", []):
        included = set(tu.get("includes", [])) | {tu["file"]}
        changed = changed_paths & recipe_inputs(included)
        if any(path.startswith('@build/') for path in included):
            changed.update(changed_cmake)
        if changed:
            for path in included:
                input_changes[path].update(changed)
    dependency_reviews = []
    for unit in sorted(units, key=lambda u: u["id"]):
        if not include_todo and unit["status"] not in ("partial", "green"):
            continue
        owned_files = {graph["functions"][usr]["file"] for usr in unit["functions"]} | set(unit['upstream'])
        reasons = set()
        for path in owned_files:
            reasons.update(input_changes[path])
        for path in unit['upstream']:
            if path.startswith('@build/'):
                reasons.update(changed_paths & recipe_inputs([path]))
                reasons.update(changed_cmake)
        if reasons:
            dependency_reviews.append({"unit": unit["id"], "changed_inputs": sorted(reasons),
                                       "reason": "an extraction translation unit or generated-source recipe has changed inputs; conservative dependency review"})
    return {"schema_version": 1, "upstream_pin": pin, "extraction_commit": base,
            "target": target_sha, "ported_statuses": ["partial", "green"],
            "include_todo": include_todo, "affected": affected,
            "ported_unit_count": sum(u["status"] in ("partial", "green") for u in units),
            "ported_function_count": sum(len(u["functions"]) for u in units if u["status"] in ("partial", "green")),
            "dependency_reviews": dependency_reviews,
            "affected_units": sorted({a["unit"] for a in affected + dependency_reviews}), "changed_files": files,
            "limitations": ["Conservative text-hunk and extraction-TU include analysis; dependencies in uncompiled configurations are unknown",
                            "Reports committed target changes, not working-tree edits"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--target", default="HEAD")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--include-todo", action="store_true")
    parser.add_argument("--fail-on-drift", action="store_true")
    a = parser.parse_args()
    try:
        graph = read_json(a.root / "port/extractor/generated/graph.json")
        units = [tomllib.loads(p.read_text(encoding="utf-8")) for p in sorted((a.root / "port/units").glob("*.toml"))]
        result = report(a.root, graph, units, a.target, a.include_todo)
        if a.output:
            write_json(a.output, result)
        print(json.dumps(result, indent=2, ensure_ascii=True))
        return int(a.fail_on_drift and bool(result["affected_units"]))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as exc:
        print(f"drift report failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
