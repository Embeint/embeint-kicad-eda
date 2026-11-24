#!/usr/bin/env python3
"""Summarize failed outcomes of a kicad-difftest report: case IDs and the first differing leaves."""
import json
import sys


def leaves(value, path=""):
    if isinstance(value, dict):
        for key, item in value.items():
            yield from leaves(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            yield from leaves(item, f"{path}[{index}]")
    else:
        yield path, value


report = json.load(open(sys.argv[1]))
outcomes = report.get("outcomes") or report.get("cases_detail") or []
failed = [o for o in outcomes if o.get("outcome", o.get("status")) not in ("compared",)]
print(f"{len(failed)} failed outcomes; keys of first: {sorted(failed[0]) if failed else None}")
for outcome in failed:
    case = outcome.get("case")
    try:
        oracle = json.loads(outcome["oracle"])
        rust = json.loads(outcome["rust"])
    except (KeyError, TypeError, ValueError):
        print(case, outcome.get("reason"))
        continue
    a = dict(leaves(oracle))
    b = dict(leaves(rust))
    diffs = [(k, a.get(k), b.get(k)) for k in sorted(set(a) | set(b)) if a.get(k) != b.get(k)]
    print(case, len(diffs), diffs[:3])
