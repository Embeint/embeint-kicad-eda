#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reproduce the public fixed-seed resistor corpus; no customer inputs."""
import json
import math
import argparse
import hashlib
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = "pcb_calculator.application.pure.p001"
ORIGINAL_COUNT = 199
ORIGINAL_SHA256 = "ee77e9687eb15556fa222c0c3b57e86bf2be7523a8c8c465a2eb9548f7103d15"

# Preserve the original Windows-generated input bytes. Decimal parsing is
# correctly rounded on supported CPython hosts; these two historical pow results
# differed from that conversion. No host libm exponentiation is used.
POWER_BITS = {23: 0x44B52D02C7E14AF7, 126: 0x5A17A2ECC414A040}
IEEE_VALUES = [
    0, 0x8000000000000000, 1, 0x8000000000000001,
    0x000FFFFFFFFFFFFF, 0x800FFFFFFFFFFFFF,
    0x0010000000000000, 0x8010000000000000,
    0x3FF0000000000000, 0xBFF0000000000000,
    0x7FEFFFFFFFFFFFFF, 0xFFEFFFFFFFFFFFFF,
    0x7FF0000000000000, 0xFFF0000000000000,
    0x7FF8000000000000, 0xFFF8000000000000,
    0x7FF8000000000001, 0x7FF8000000000003, 0x7FFFFFFFFFFFFFFF,
    0xFFF8000000000002, 0xFFF8000000000003, 0xFFFFFFFFFFFFFFFF,
    0x7FF0000000000001, 0x7FF0000000000003, 0x7FF7FFFFFFFFFFFF,
    0xFFF0000000000002, 0xFFF0000000000003, 0xFFF7FFFFFFFFFFFF,
]


def decimal_power(exponent):
    if exponent in POWER_BITS:
        return struct.unpack(">d", struct.pack(">Q", POWER_BITS[exponent]))[0]
    return float(f"1e{exponent}")


def bits(value):
    return struct.unpack(">Q", struct.pack(">d", value))[0]


def render():
    cases = []

    def case(name, value):
        cases.append({"unit": UNIT, "case": name, "input": value})

    def calc(name, actions):
        case(name, {"op": "calculator", "actions": actions})

    def new(value):
        return {"op": "new", "target_bits": bits(value)}

    def exclude(values):
        return {"op": "exclude", "values_bits": list(map(bits, values))}

    for series in [*range(8), -1, -2147483648, 2147483647, 8]:
        case(f"series-{series}", {"op": "series", "series": series,
             "decades": [-2147483648, -1000, *range(-325, 311), 1000, 2147483647]})

    values = [0., -0., -1., -1e308, math.inf, -math.inf, math.nan,
              math.ulp(0.), math.nextafter(0., 1.) * 100, float.fromhex("0x1.fffffffffffffp+1023")]
    for exponent in range(-323, 309):
        try:
            power = decimal_power(exponent)
            for factor in [1., 1.005, 1.015, 4.995, 9.995, 9.999]:
                value = power * factor
                if math.isfinite(value):
                    values.extend([math.nextafter(value, -math.inf), value, math.nextafter(value, math.inf)])
        except OverflowError:
            pass
    for offset in range(0, len(values), 2048):
        case(f"format-boundaries-{offset // 2048}", {"op": "format", "values_bits": list(map(bits, values[offset:offset + 2048]))})
    rng = random.Random(0x524553495354)
    random_values = []
    for _ in range(4096):
        random_values.append(rng.getrandbits(64))
    case("format-fixed-seed-bits", {"op": "format", "values_bits": random_values})

    case("resistance-default-empty-explicit-parts", {"op": "resistance", "values": [
        {"value_bits": bits(10.), "name": "10R"},
        {"value_bits": bits(-0.), "name": "", "parts_bits": []},
        {"value_bits": bits(math.inf), "name": "infinite", "parts_bits": [bits(2.), bits(1.), bits(2.)]},
        {"value_bits": bits(math.nan), "name": "NaN"},
        {"value_bits": bits(7.), "name": "(a | b) + c", "parts_bits": [bits(3.), bits(4.)]},
    ]})
    def resistance(value, name, parts):
        return {"value_bits": bits(value), "name": name, "parts_bits": list(map(bits, parts))}

    for i, (a, b, text, symbol) in enumerate([
        ((10., "a | b", [10., 10.]), (20., "c + d", [20., 30.]), "a | (b + c)", "|"),
        ((10., "(a | b)", [10., 10.]), (20., "(c + d)", [20.]), "(a | b) + c", "|"),
        ((10., "z", [10., 10., 10.]), (10., "a", [10., 10.]), "a + b", "+"),
        ((10., "a", [10., 10.]), (10., "z", [10., 10.]), "((a + b) | c)", "+"),
        ((10., "a", [10., 20.]), (10., "a", [20., 10.]), ")a + b(", "+"),
        ((0., "zero", [0., 1e-12, math.nextafter(1e-12, math.inf), -0.]),
         (0., "other", [math.nextafter(0., -math.inf), -1e-12, math.nextafter(-1e-12, -math.inf)]), "", "+"),
        ((10., "many", list(reversed(range(64)))), (20., "dup", [1.] * 64), "(((a)))|b", "|"),
        ((float.fromhex("0x1.fffffffffffffp+1023"), "maximum", [1.]),
         (float.fromhex("0x1.fffffffffffffp+1023"), "maximum", [1.]), "a\x00b", "\x00"),
        ((math.ulp(0.), "tiny", [math.ulp(0.)]), (math.ulp(0.), "tiny", [math.ulp(0.)]), "a | b + c", "+"),
        ((0., "positive-zero", [0.]), (-0., "negative-zero", [-0.]), "a + b | c", "|"),
        ((10., "NaN-part", [math.nan, 10., 10.]), (10., "finite-parts", [10., 10.]), "((a))", "("),
    ]):
        case(f"helpers-parentheses-tiebreak-{i}", {"op": "helpers", "first": resistance(*a), "second": resistance(*b), "text": text, "symbol": symbol})
    for i, candidates in enumerate([
        [], [(10., "a", [10.])],
        [(9., "z", [1., 2.]), (11., "a", [3., 3.]), (100., "far", [1.])],
        [(10., "z", [1., 1., 1.]), (10., "a", [1., 1.]), (10., "0", [1., 2.]), (10., "b", [1., 1.])],
        [(10., "z", [1., 1.]), (10. + 5e-13, "a", [1., 1.]), (10. + 2e-12, "0", [1.]), (9., "far", [2.])],
        [(math.nan, "NaN", [1.]), (math.inf, "infinite", [1.]), (10., "finite", [1.])],
    ]):
        case(f"collector-real-continuations-{i}", {"op": "collector", "target_bits": bits(10.), "candidates": [resistance(*item) for item in candidates]})
    empty_parts = {**resistance(10., "empty", []), "clear_parts": True}
    case("helpers-empty-public-parts", {"op": "helpers", "first": empty_parts,
         "second": resistance(10., "singleton", [10.]), "text": "a-b", "symbol": "-"})
    case("collector-empty-public-parts", {"op": "collector", "target_bits": bits(10.),
         "candidates": [resistance(10., "one", [10.]), empty_parts]})
    calc("setter-full-u32-and-nan-exclusion-no-index", [{"op": "set_series", "series": 4294967295},
         exclude([math.nan]), {"op": "state"}, {"op": "set_series", "series": 2}, new(20.), {"op": "calculate"}])
    calc("constructor-default-and-private-build-all-series", [{"op": "state"},
         exclude([math.nan, -1., 0., 1., math.inf, -math.inf]),
         {"op": "solution", "level": 3}, {"op": "solution", "level": 4},
         *[{"op": "build", "series": i} for i in range(8)]])
    for series in range(5):
        setup = [{"op": "set_series", "series": series}]
        targets = [5., 10., 20., 15., 4.7, 123.456789, 1000., 1e6, 2e6, -1., 0.,
                   math.ulp(0.), 1e308, math.nextafter(20., -math.inf), math.nextafter(20., math.inf),
                   20. + 1e-12, 20. - 1e-12]
        for i, target in enumerate(targets):
            calc(f"calculate-e{series}-{i}", [*setup, new(target), {"op": "calculate"}, {"op": "state"}])
        # Force each real helper even when Calculate would short circuit at 2R.
        calc(f"private-search-and-all-levels-e{series}", [*setup, new(20.), {"op": "prepare"},
            {"op": "find", "values_bits": list(map(bits, [-math.inf, -1., 0., 5., math.nextafter(5., math.inf), 10., 20.,
                123.456, 2e6, math.nextafter(2e6, -math.inf), 1e308, math.inf, math.nan]))},
            *[{"op": "solution", "level": level} for level in [2, 3, 4]], {"op": "buffers"}])
        calc(f"exclusion-boundaries-reset-repeated-e{series}", [*setup, new(123.456),
            exclude([10., 10., 10. + 5e-13, 10. + 2e-12, 1e6, math.nan, math.inf, -math.inf, 0., -1., 11.234]),
            {"op": "calculate"}, {"op": "calculate"}, new(20.), {"op": "calculate"}])
        for i, target in enumerate([math.nan, math.inf, -math.inf, -1e308]):
            calc(f"collector-extreme-target-e{series}-{i}", [*setup, new(target), {"op": "calculate"}])
    calc("series-mutation-and-buffer-persistence", [new(123.456), {"op": "calculate"}, {"op": "set_series", "series": 0},
         {"op": "state"}, new(20.), {"op": "state"}, {"op": "calculate"}, exclude([10.]), {"op": "calculate"},
         {"op": "set_series", "series": 4}, new(12345.67), {"op": "calculate"}, new(20.), {"op": "calculate"}])
    # E1 is exactly [10, 100, ..., 1M]. Empty buffer creation is defined; looking
    # up its first element is not. A single remaining value is fully defined.
    all_e1 = [decimal_power(i) for i in range(1, 7)]
    calc("empty-and-singleton-buffers-defined-domain", [{"op": "set_series", "series": 0}, new(17.),
        exclude(all_e1), {"op": "prepare"}, {"op": "buffers"}, new(17.), exclude(all_e1[1:]),
        {"op": "calculate"}, {"op": "buffers"}, *[{"op": "solution", "level": i} for i in [2, 3, 4]]])
    # Adjacent lookup ties and multi-topology targets, including exact 3R/4R.
    for i, target in enumerate([30., 40., 10. + 10. / 2., 10. / 3., 10. / 4.,
                                10. + 100. * 1000. / 1100., 10. * 1100. / 1110.,
                                (10. + 100.) * (1000. + 10000.) / 11110.,
                                10. + 100. * 11000. / 11100., 10. * 11100. / 11110., 7.5, 55.]):
        calc(f"mixed-topology-{i}", [{"op": "set_series", "series": 0}, new(target), {"op": "calculate"},
             {"op": "prepare"}, *[{"op": "solution", "level": level} for level in [2, 3, 4]]])
    for i in range(24):
        target = rng.uniform(1., 2e6)
        calc(f"search-fixed-seed-{i}", [{"op": "set_series", "series": i % 5}, new(target),
             exclude([10., 1000.]) if i % 2 else exclude([]), {"op": "calculate"}])
    case("reader-null-live-parser", {"op":"reader", "readers":[], "parsers":[None],
         "actions":[{"op":"get","parser":0},{"op":"bind","parser":0,"reader":None},{"op":"get","parser":0}]})
    case("reader-real-memory-alias-and-rebinding", {"op":"reader", "readers":[
        {"text":"", "source":"generated-empty"}, {"text":"(regulators)\n", "source":"public-generated"},
        {"text":"line1\nline2\x00tail", "source":"generated-utf8-\u03bc"}], "parsers":[0,0,1,None,2],
        "actions":[*[{"op":"get","parser":i} for i in range(5)],
             {"op":"bind","parser":0,"reader":1},{"op":"get","parser":1},
             {"op":"bind","parser":3,"reader":2},{"op":"bind","parser":4,"reader":None},
             {"op":"get","parser":3},{"op":"bind","parser":0,"reader":0},{"op":"get","parser":0}]})
    case("reader-equal-content-distinct-identity", {"op":"reader", "readers":[
        {"text":"same", "source":"same"}, {"text":"same", "source":"same"}], "parsers":[0,1],
        "actions":[{"op":"get","parser":0},{"op":"get","parser":1},
             {"op":"bind","parser":0,"reader":1},{"op":"get","parser":0},
             {"op":"get","parser":1}]})
    case("accounted-unit-scale-constants", {"op":"unit_scales"})
    calc("private-builder-caller-supplied-real-series-vector", [{"op":"build_values","values":values}
         for values in [[100], [200,400,600], [3,5,9], [65535,1,0], [1,0,65535],
                        [100,99,101,100], [0,1], [0,0,65535], [10,1,10]]])
    encode = lambda case: json.dumps(case, separators=(",", ":"), allow_nan=False) + "\n"
    original = "".join(map(encode, cases)).encode("utf-8")
    if len(cases) != ORIGINAL_COUNT or hashlib.sha256(original).hexdigest() != ORIGINAL_SHA256:
        raise ValueError("original 199 request bytes changed; review the generator inputs")

    def helper_bits(name, first, second):
        case(name, {"op":"helpers", "first":{"value_bits":first,"name":"a","parts_bits":[bits(1.)]},
             "second":{"value_bits":second,"name":"b","parts_bits":[bits(1.)]}, "text":"a+b", "symbol":"+"})

    for first in IEEE_VALUES:
        for second in IEEE_VALUES:
            helper_bits(f"helpers-ieee-{first:016x}-{second:016x}", first, second)
    payload_rng = random.Random(0x4E414E5041594C4F)
    for i in range(64):
        operands = [0x7FF0000000000000 | (payload_rng.getrandbits(51) or 1)
                    | (payload_rng.getrandbits(1) << 51) | (payload_rng.getrandbits(1) << 63) for _ in range(2)]
        helper_bits(f"helpers-generated-nan-payload-{i}", *operands)
    case("format-explicit-ieee-classes", {"op":"format", "values_bits":IEEE_VALUES})
    wire = "".join(map(encode, cases))
    # Keep literal integer -0 in the wire corpus; JSON reserialization would
    # erase the distinction from a floating token -0.0 or an unsigned token 0.
    wire += '{"unit":"' + UNIT + '","case":"literal-integer-minus-zero-format","input":{"op":"format","values_bits":[-0]}}\n'
    wire += '{"unit":"' + UNIT + '","case":"literal-integer-minus-zero-series","input":{"op":"calculator","actions":[{"op":"set_series","series":-0}]}}\n'
    return wire.encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare regenerated bytes without writing")
    args = parser.parse_args()
    destination = ROOT / "port/cases" / (UNIT + ".jsonl")
    data = render()
    if args.check:
        if destination.read_bytes() != data:
            raise SystemExit("corpus differs from deterministic regeneration")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    print(f"{len(data.splitlines())} cases, SHA-256 {hashlib.sha256(data).hexdigest()}: {destination}")


if __name__ == "__main__":
    main()
