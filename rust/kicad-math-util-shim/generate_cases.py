#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public integer boundaries and fixed-seed requests; never computes reference replies.

Logging requests carry binary64 values as exact bit patterns, built from integers and exact
power-of-two scalings only (no floating-point operation that rounds), so the corpus is the same
on every host.
"""
import argparse
import itertools
import json
import math
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "libs.kimath.src.math.util.application.ui.p001"
PATH = ROOT / "port/cases" / (UNIT + ".jsonl")


def generate():
    records = []
    counts = {"arithmetic": 0, "refusal": 0, "log": 0}
    def add(group, inp, message=None):
        case = f"{group}-{counts[group]:05d}"
        record = {"unit": UNIT, "case": case, "input": inp}
        if group == "refusal":
            record = {"request": record, "error": {"kind": "invalid_input", "message": message}}
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
                {"op": "log_debug", "format": "", "args": [], "log_level": 6, "logging": True,
                 "extra": 0},
                {"op": "log_overflow", "bits": "F"*16, "type_name": "i"},
                {"op": "log_overflow", "bits": "0"*15, "type_name": "i"},
                {"op": "log_overflow", "bits": "0"*16, "type_name": None}]:
        message = ("expected string: op" if not isinstance(inp.get("op"), str) else
                   "unknown operation" if inp["op"] == "unknown" else
                   "unexpected or missing field" if inp["op"] == "hypot_wide" else
                   "unexpected or missing field" if inp["op"] == "log_debug" else
                   "expected string: type_name" if inp.get("type_name", "i") is None else
                   "invalid hex: bits")
        add("refusal", inp, message)
    log_cases(add)
    def encode(items):
        return "".join(json.dumps(r, separators=(",", ":"), allow_nan=False)+"\n" for r in items)
    return (encode(r for r in records if "request" not in r),
            encode(r for r in records if "request" in r), counts)


def bits(value):
    """The binary64 bit pattern of an exactly representable Python float, as 16 hex digits."""
    return struct.pack(">d", value).hex()


def raw(pattern):
    return f"{pattern:016x}"


def log_cases(add):
    """kimathLogOverflow and kimathLogDebug: value formatting, type names, level gating."""
    def overflow(pattern, name="i"):
        add("log", {"op": "log_overflow", "bits": pattern, "type_name": name})
    # Every typeid name KiROUND and KiCheckedCast pass on Linux, plus names wx converts
    # specially: empty, a NUL that ends the C string, bytes above 0x7F (wxConvLibc fails in
    # the C locale and the name is empty), printf-looking text (an argument, never a format).
    for name in ["a", "h", "s", "t", "i", "j", "l", "m", "x", "y", "",
                 "a\u0000b", "\u00e9", "i\u00e9", "%s%f%n", "N5KIGFX5COLORE", "x" * 300]:
        overflow(bits(2147483648.0), name)
    specials = [0, 1 << 63, 0x7FF0000000000000, 0xFFF0000000000000, 0x7FF8000000000000,
                0xFFF8000000000000, 0x7FF0000000000001, 0xFFF0000000000001, 0x7FFFFFFFFFFFFFFF,
                0xFFFFFFFFFFFFFFFF, 1, 0x800FFFFFFFFFFFFF, 0x0010000000000000,
                0x7FEFFFFFFFFFFFFF, 0xFFEFFFFFFFFFFFFF]
    for pattern in specials:
        overflow(raw(pattern))
    # The values KiROUND logs at: the half beyond each target's range, its limits, NaN.
    for low, high in [(-128, 127), (0, 255), (-32768, 32767), (0, 65535),
                      (-(1 << 31), (1 << 31) - 1), (0, (1 << 32) - 1),
                      (-(1 << 63), (1 << 63) - 1)]:
        for value in [high + 0.5, high + 1.0, low - 0.5, low - 1.0, float(high), float(low)]:
            overflow(bits(value))
    # Six-digit rounding: exact ties (odd multiples of 2^-7 sit on a sixth-digit half), the
    # values either side of them, and carries through every digit.
    for m in range(1, 64, 2):
        for scale in [128, -128, 1 << 20]:
            overflow(bits(m / scale))
    for pattern in [0x3FEFFFFFFFFFFFFF, 0x3FF0000000000001, 0x3EB0C6F7A0B5ED8D,
                    0x3EB0C6F7A0B5ED8C, 0x3EB0C6F7A0B5ED8E, 0x412E847FFFFFFFFF]:
        overflow(raw(pattern))
    for exponent in range(-30, 1024, 7):
        overflow(bits(math.ldexp(1.0, exponent)))
        overflow(bits(-math.ldexp(1.5, exponent)))
    state = 0x2F6B1C94A3D07E55
    for _ in range(384):
        state = (state * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        overflow(raw(state))

    def debug(fmt, args, level=10000, logging=True):
        add("log", {"op": "log_debug", "format": fmt, "args": args, "log_level": level,
                    "logging": logging})
    rescale = "Overflow in rescale (%lld * %lld + %lld) / %lld"
    limits = [-(1 << 63), -(1 << 63) + 1, -1, 0, 1, (1 << 63) - 1]
    for a, b in itertools.product(limits, repeat=2):
        debug(rescale, [{"lld": a}, {"lld": b}, {"lld": -a // 2}, {"lld": b or 1}])
    for level in [0, 1, 2, 5, 6, 7, 100, 10000]:
        for logging in [True, False]:
            debug(rescale, [{"lld": 1}, {"lld": 2}, {"lld": 3}, {"lld": 4}], level, logging)
            # Below wxLOG_Debug, or disabled, nothing is formatted: missing arguments are fine.
            if level < 6 or not logging:
                debug(rescale, [], level, logging)
                debug("%f %lld", [{"lld": 1}], level, logging)
    for fmt, args in [("", []), ("plain", []), ("%%", []), ("100%% %lld", [{"lld": -7}]),
                      ("line\nbreak", []), ("%f", [{"f": bits(0.0078125)}]),
                      ("%f|%f", [{"f": raw(0x7FF8000000000000)}, {"f": raw(1 << 63)}]),
                      ("%lld%f", [{"lld": 3}, {"f": bits(-2.5)}]),
                      ("excess", [{"lld": 1}, {"f": bits(1.0)}]),
                      ("%lld", [{"lld": 5}, {"lld": 6}, {"lld": 7}, {"lld": 8}]),
                      ("~ !\"#$&'()*+,-./09:;<=>?@AZ[\\]^_`az{|}", [])]:
        debug(fmt, args)

    def refuse(inp, message):
        add("refusal", inp, message)
    base = {"op": "log_debug", "format": "", "args": [], "log_level": 10000, "logging": True}
    for fmt in ["%s", "%d", "%", "%l", "%ll", "%lf", "%5f", "%.3f", "%-lld", "\t", "\u00e9",
                "\u0000", "a%"]:
        refuse({**base, "format": fmt}, "unsupported format")
    refuse({**base, "format": 1}, "expected string: format")
    for args, message in [({}, "invalid args"), ([1], "invalid args"), ([{}], "invalid args"),
                          ([{"lld": 1, "f": "0" * 16}], "invalid args"),
                          ([{"lld": 1}] * 5, "invalid args"),
                          ([{"lld": 1 << 63}], "integer out of range: lld"),
                          ([{"lld": 1.0}], "integer out of range: lld"),
                          ([{"f": 1}], "expected string: f"),
                          ([{"f": "0" * 15}], "invalid hex: f"),
                          ([{"f": "F" * 16}], "invalid hex: f")]:
        refuse({**base, "args": args}, message)
    for level in [-1, 10001, 1.0, True, None]:
        refuse({**base, "log_level": level}, "integer out of range: log_level")
    for logging in [1, None, "true"]:
        refuse({**base, "logging": logging}, "expected boolean: logging")
    for fmt, args in [("%lld", []), ("%f", [{"lld": 1}]), ("%lld", [{"f": "0" * 16}]),
                      ("%lld %lld", [{"lld": 1}]), (rescale, [{"lld": 1}] * 3)]:
        for level in [6, 10000]:
            refuse({**base, "format": fmt, "args": args, "log_level": level},
                   "undefined format arguments")


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
        # LF on every host, so a Windows checkout regenerates the same bytes.
        PATH.write_bytes(data.encode())
        controls_path.write_bytes(controls.encode())
    print(json.dumps({"cases": sum(counts.values()), **counts}, sort_keys=True))
