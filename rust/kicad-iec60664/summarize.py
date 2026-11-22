#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Summarize checked scalar outcomes separately from Compute's always-true return."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import struct

UNIT = "pcb_calculator.calculator_panels.application.pure.p001"
ROOT = Path(__file__).resolve().parents[2]


def floating(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    cases = [json.loads(line) for line in (args.root / "port/cases" / (UNIT + ".jsonl")).read_text(encoding="utf-8").splitlines()]
    replies = [json.loads(line) for line in (args.root / "port/goldens" / (UNIT + ".jsonl")).read_text(encoding="utf-8").splitlines()]
    assert len(cases) == len(replies)
    operations = defaultdict(Counter)
    snapshots = 0
    for case, reply in zip(cases, replies):
        assert reply["case"] == case["case"] and reply["unit"] == case["unit"] == UNIT
        assert "error" not in reply
        steps = case["input"]["steps"]
        assert len(reply["result"]) == len(steps)
        for step, result in zip(steps, reply["result"]):
            op, value, state = step["op"], result["value"], result["state"]
            assert len(state) == 16
            snapshots += 1
            counts = operations[op]
            counts["calls"] += 1
            if op == "compute":
                counts["true" if value is True else "false"] += 1
                outputs = [floating(state[key]) for key in ("clearance", "creepage", "min_groove_width")]
                counts["all_distances_positive" if all(n > 0 for n in outputs) else "has_nonpositive_distance"] += 1
            elif op == "cti":
                counts[f"material_group_{value}"] += 1
            elif op not in ("state", "set", "categories"):
                n = floating(value)
                counts["positive" if n > 0 else "negative" if n < 0 else "zero"] += 1
    print(json.dumps({"unit": UNIT, "cases": len(cases), "snapshots": snapshots,
                      "operations": {op: dict(counts) for op, counts in sorted(operations.items())}}, indent=2))


if __name__ == "__main__":
    main()
