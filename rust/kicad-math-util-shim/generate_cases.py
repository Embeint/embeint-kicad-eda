#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public integer boundaries and fixed-seed requests; never computes reference replies."""
import argparse
import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.kimath.src.math.util.application.ui.p001"
PATH = ROOT / "port/cases" / (UNIT + ".jsonl")


def generate():
    records = []
    counts = {"arithmetic": 0, "refusal": 0, "deferred": 0}
    def add(group, inp, message=None):
        case = f"{group}-{counts[group]:05d}"
        record = {"unit": UNIT, "case": case, "input": inp}
        if group != "arithmetic":
            error = ({"kind": "unported", "upstream": "kimathLogDebug" if inp["op"] == "log_debug" else "kimathLogOverflow", "unit": UNIT}
                     if group == "deferred" else {"kind": "invalid_input", "message": message})
            record = {"request": record, "error": error}
        records.append(record)
        counts[group] += 1
    def hypot(x, y):
        add("arithmetic", {"op": "hypot_wide", "x": x, "y": y})
    def scale(op, n, v, d):
        add("refusal" if d == 0 else "arithmetic",
            {"op": op, "numerator": n, "value": v, "denominator": d}, "zero denominator")
    state = 0x64A17E0328BD591F
    def word():
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        return state
    unsigned = [0, 1, 2, 3, 4, 5, (1 << 32)-1, 1 << 32, (1 << 32)+1,
                3037000499, 3037000500, 6074000999, 6074001000,
                (1 << 63)-1, 1 << 63, (1 << 63)+1, (1 << 64)-2, (1 << 64)-1]
    for x, y in itertools.product(unsigned, repeat=2):
        hypot(x, y)
    # Both sides of u128 sum wrap and the u64 rounded-root wrap, in both orders.
    for delta in range(-16, 17):
        for y in [(1 << 32)+delta, 6074001000+delta]:
            hypot((1 << 64)-1, y)
            hypot(y, (1 << 64)-1)
    for _ in range(2048):
        hypot(word(), word())
    for width, op in [(32, "rescale_i32"), (64, "rescale_i64_int128")]:
        lo, hi = -(1 << (width-1)), (1 << (width-1))-1
        values = [lo, lo+1, -65537, -3, -2, -1, 0, 1, 2, 3, 65537, hi-1, hi]
        for n, v, d in itertools.product(values, repeat=3):
            scale(op, n, v, d)
        # Natural signed domain; quotient narrowing is exercised, not filtered out.
        def signed():
            value = word() >> (64-width)
            return value if value <= hi else value-(1 << width)
        for _ in range(2048):
            scale(op, signed(), signed(), signed())
        for field in ["numerator", "value", "denominator"]:
            for invalid in [lo-1, hi+1, 1.0, True, None, "1", [], {}]:
                inp = {"op": op, "numerator": 1, "value": 2, "denominator": 3}
                inp[field] = invalid
                add("refusal", inp, f"integer out of range: {field}")
    for field in ["x", "y"]:
        for invalid in [-1, 1 << 64, 1.0, True, None, "1", [], {}]:
            inp = {"op": "hypot_wide", "x": 1, "y": 2}
            inp[field] = invalid
            add("refusal", inp, f"integer out of range: {field}")
    for inp in [{}, {"op": 1}, {"op": "unknown"}, {"op": "hypot_wide", "x": 0},
                {"op": "hypot_wide", "x": 0, "y": 1, "extra": 0},
                {"op": "log_debug", "format": 1},
                {"op": "log_overflow", "bits": "F"*16, "type_name": "i"},
                {"op": "log_overflow", "bits": "0"*15, "type_name": "i"},
                {"op": "log_overflow", "bits": "0"*16, "type_name": None}]:
        message = ("expected string: op" if not isinstance(inp.get("op"), str) else
                   "unknown operation" if inp["op"] == "unknown" else
                   "unexpected or missing field" if inp["op"] == "hypot_wide" else
                   "expected string: format" if inp["op"] == "log_debug" else
                   "expected string: type_name" if inp.get("type_name", "i") is None else
                   "invalid hex: bits")
        add("refusal", inp, message)
    for fmt in ["", "plain", "%lld %s"]:
        add("deferred", {"op": "log_debug", "format": fmt})
    for bits in ["0000000000000000", "8000000000000000", "7ff0000000000000", "7ff8000000000001"]:
        add("deferred", {"op": "log_overflow", "bits": bits, "type_name": "i"})
    def encode(items):
        return "".join(json.dumps(r, separators=(",", ":"), allow_nan=False)+"\n" for r in items)
    return (encode(r for r in records if "request" not in r),
            encode(r for r in records if "request" in r), counts)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data, controls, counts = generate()
    controls_path = ROOT / "port/oracle/fixtures/math-util-controls.jsonl"
    if args.check:
        if PATH.read_text() != data or controls_path.read_text() != controls:
            raise SystemExit("corpus differs from deterministic generator")
    else:
        PATH.write_text(data)
        controls_path.write_text(controls)
    print(json.dumps({"cases": sum(counts.values()), **counts}, sort_keys=True))
