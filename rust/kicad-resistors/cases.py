#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reproduce the public fixed-seed resistor corpus; no customer inputs."""
import json
import math
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = "pcb_calculator.application.pure.p001"


def bits(value):
    return struct.unpack(">Q", struct.pack(">d", value))[0]


def main():
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
            power = 10. ** exponent
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
    all_e1 = [10. ** i for i in range(1, 7)]
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
    destination = ROOT / "port/cases" / (UNIT + ".jsonl")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text("".join(json.dumps(case, separators=(",", ":"), allow_nan=False) + "\n" for case in cases), encoding="utf-8", newline="\n")
    print(f"{len(cases)} cases: {destination}")


if __name__ == "__main__":
    main()
