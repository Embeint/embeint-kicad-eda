#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write supplementary/sensitive.requests.jsonl: requests aimed at the reference-build
properties the port is bound to (check_reference_bindings.py), for a live comparison of the
oracle and the Rust shim. They are not corpus cases and have no golden.

- BEZIER<double>: every pair of control points carries two distinct NaNs (quiet/signalling,
  both signs) in x and the swapped kinds in y, for each call and parameter class, so each
  two-NaN operand choice of every site is asked; plus signed zeros, infinities, extremes,
  subnormals and finite values.
- HASH_128 locale grouping: zero bytes at every position, after negative and CHAR_MAX
  groups, long groupings and separators 0, '.', 255.

Deterministic (no randomness); rerun to reproduce the file byte for byte.
"""

import itertools
import json
import pathlib

UNIT = "libs.kimath.include.application.pure.p001"
OUT = pathlib.Path(__file__).resolve().parent / "supplementary" / "sensitive.requests.jsonl"

QNAN_POS = 0x7FF8_0000_0000_00A1
QNAN_NEG = 0xFFF8_0000_0000_00A2
SNAN_POS = 0x7FF0_0000_0000_00B3
SNAN_NEG = 0xFFF0_0000_0000_00B4
NANS = [QNAN_POS, SNAN_NEG, SNAN_POS, QNAN_NEG]
FINITE = [0x3FF0_0000_0000_0000, 0x8000_0000_0000_0000, 0xC000_0000_0000_0000, 0x0000_0000_0000_0000]
SPECIAL = [
    0x0000_0000_0000_0000, 0x8000_0000_0000_0000, 0x7FF0_0000_0000_0000, 0xFFF0_0000_0000_0000,
    0x7FEF_FFFF_FFFF_FFFF, 0xFFEF_FFFF_FFFF_FFFF, 0x0000_0000_0000_0001, 0x8010_0000_0000_0000,
    0x3FF0_0000_0000_0000, 0xBFE0_0000_0000_0000, 0xFFF8_0000_0000_0000, 0x4059_0000_0000_0000,
]
T_POINT = [0x3FE0_0000_0000_0000, 0x3FD0_0000_0000_0000, 0x0000_0000_0000_0000,
           0x8000_0000_0000_0000, 0x7FF0_0000_0000_0000, 0x7FF8_0000_0000_00C5,
           0xFFF0_0000_0000_00C6, 0x3FF8_0000_0000_0000]
T_PAIRS = [
    (0x3FE8_0000_0000_0000, 0x3FD0_0000_0000_0000),  # reversed: out-of-line PointAt
    (0x3FF8_0000_0000_0000, 0x4000_0000_0000_0000),  # both clamp to 1: PointAt( 1 )
    (0x3FD0_0000_0000_0000, 0x3FE8_0000_0000_0000),  # interior: two splits and t0 / t1
    (0x0000_0000_0000_0000, 0x3FE0_0000_0000_0000),  # t0 == 0
    (0x3FE0_0000_0000_0000, 0x3FF0_0000_0000_0000),  # t1 == 1
    (0x7FF8_0000_0000_00C7, 0x3FE0_0000_0000_0000),  # NaN t0
    (0x3FD0_0000_0000_0000, 0xFFF0_0000_0000_00C8),  # NaN t1
    (0x8000_0000_0000_0000, 0x3FF0_0000_0000_0000),  # -0.0 and 1: the full curve
]


def hexs(bits):
    return "%016x" % bits


def bezier(case, points, call, **params):
    data = {"op": "bezier", "type": "f64", "points": [[hexs(x), hexs(y)] for x, y in points],
            "call": call}
    data.update({k: hexs(v) for k, v in params.items()})
    return {"unit": UNIT, "case": case, "input": data}


def nan_pair_curves():
    """Two distinct NaNs in each pair of points; y uses the other two kinds, swapped."""
    for (i, j), (a, b) in itertools.product(itertools.combinations(range(4), 2),
                                            itertools.permutations(range(4), 2)):
        xs = list(FINITE)
        ys = list(reversed(FINITE))
        xs[i], xs[j] = NANS[a], NANS[b]
        others = [k for k in range(4) if k not in (a, b)]
        ys[i], ys[j] = NANS[others[1]], NANS[others[0]]
        yield f"p{i}{j}-n{a}{b}", list(zip(xs, ys))


def requests():
    out = []
    for tag, points in nan_pair_curves():
        for k, t in enumerate(T_POINT):
            out.append(bezier(f"sens-bezier-point-at-{tag}-t{k}", points, "point_at", t=t))
            out.append(bezier(f"sens-bezier-split-{tag}-t{k}", points, "split", t=t))
        for k, (t0, t1) in enumerate(T_PAIRS):
            out.append(bezier(f"sens-bezier-sub-curve-{tag}-r{k}", points, "sub_curve", t0=t0, t1=t1))
    # Specials and extremes in every position, without two NaN operands meeting by design.
    for n, combo in enumerate(itertools.islice(itertools.product(SPECIAL, repeat=2), 0, None, 7)):
        points = [(combo[0], combo[1]), (combo[1], SPECIAL[n % 12]), (SPECIAL[(n * 5) % 12], combo[0]),
                  (SPECIAL[(n * 7) % 12], SPECIAL[(n * 11) % 12])]
        t = T_POINT[n % len(T_POINT)]
        t0, t1 = T_PAIRS[n % len(T_PAIRS)]
        out.append(bezier(f"sens-bezier-special-{n}-point-at", points, "point_at", t=t))
        out.append(bezier(f"sens-bezier-special-{n}-split", points, "split", t=t))
        out.append(bezier(f"sens-bezier-special-{n}-sub-curve", points, "sub_curve", t0=t0, t1=t1))
    # HASH_128 grouping with zero bytes.
    groupings = [
        [1, 2, 0], [1, 0, 3], [3, 0], [0], [0, 3], [2, 0, 0, 1], [4, 0, 1], [1, 1, 0, 2],
        [1, 0x80, 0, 2], [1, 0x7F, 0, 2], [2, 0xFF, 0], [1, 2, 0] + [5] * 100,
        [1] * 15 + [0] + [2] * 50, [0] * 64, [3, 1, 0, 3, 1, 0], [1, 2],
    ]
    values = ["efcdab00000000000000000000000000", "ffffffffffffffffffffffffffffffff",
              "f0debc9a785634123412000000000000", "01000000000000000000000000000080"]
    for g, grouping in enumerate(groupings):
        for s, sep in enumerate([46, 0, 255]):
            for v, value in enumerate(values):
                out.append({"unit": UNIT, "case": f"sens-hash128-grouping-{g}-sep{s}-v{v}",
                            "input": {"op": "hash128", "a": value, "b": values[(v + 1) % 4],
                                      "locale": {"thousands_sep": sep,
                                                 "grouping": bytes(grouping).hex()}}})
    return out


def main():
    OUT.parent.mkdir(exist_ok=True)
    lines = [json.dumps(r, separators=(",", ":")) for r in requests()]
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(lines)} requests -> {OUT.name}")


if __name__ == "__main__":
    main()
