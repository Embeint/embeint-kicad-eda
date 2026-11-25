#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the public fixed-seed corpus and undefined-behaviour controls for the Clipper2
core/engine header unit.

The corpus (port/cases/<unit>.jsonl) holds only inputs inside the reference's defined domain;
every case must yield a C++ result. Inputs on which the reference expression is undefined
(signed overflow, out-of-range double to integer conversion, out-of-range indexing, use of a
destroyed node) go to port/oracle/fixtures/clipper2-core-engine-controls.jsonl, where both
programs must refuse them identically. The same file holds the Clipper64 engine-domain
controls: coordinates outside Clipper2's declared range, which the adapter refuses as a harness
limit before the pinned engine runs. `--check` verifies both files are current.
"""
import argparse
import json
import math
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001"
CASES = ROOT / "port/cases" / f"{UNIT}.jsonl"
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-core-engine-controls.jsonl"

I64_MAX = 2**63 - 1
I64_MIN = -2**63
MAX_COORD = I64_MAX >> 2
U64_MAX = 2**64 - 1


class Rng:
    """SplitMix64: a fixed, version-independent generator."""

    def __init__(self, seed):
        self.state = seed & U64_MAX

    def next(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & U64_MAX
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & U64_MAX
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & U64_MAX
        return z ^ (z >> 31)

    def int(self, lo, hi):
        return lo + self.next() % (hi - lo + 1)

    def choice(self, items):
        return items[self.next() % len(items)]

    def real(self, lo, hi):
        return lo + (hi - lo) * (self.next() >> 11) / float(1 << 53)


def h(x):
    """A double as its 16 hex digit bit pattern (accepts floats or raw bit strings)."""
    if isinstance(x, str):
        return x
    return format(struct.unpack("<Q", struct.pack("<d", x))[0], "016x")


NAN = "7ff8000000000000"
NEG_NAN = "fff8000000000000"
INF = math.inf
TWO63 = 2.0**63
BELOW_TWO63 = 9223372036854774784.0  # largest double below 2^63
NEG_TWO63 = -(2.0**63)
BELOW_NEG_TWO63 = -9223372036854777856.0  # next double below -2^63


def p64(x, y, z=0):
    return [x, y, z]


def pd(x, y, z=0):
    return [h(x), h(y), z]


def pt(t, x, y, z=0):
    return p64(x, y, z) if t == "i64" else pd(x, y, z)


def num(t, x):
    return x if t == "i64" else h(x)


def rect(t, l, tp, r, b):
    return [num(t, l), num(t, tp), num(t, r), num(t, b)]


def path(t, coords, z=0):
    return [pt(t, x, y, z) for x, y in coords]


STATES = [
    None,
    {"flags": ["dec", "skipws"], "precision": 6, "width": 0, "fill": " "},
    {"flags": ["hex", "showbase", "uppercase"], "precision": 6, "width": 0, "fill": " "},
    {"flags": ["oct", "showbase"], "precision": 6, "width": 0, "fill": " "},
    {"flags": ["dec", "showpos"], "precision": 3, "width": 0, "fill": " "},
    {"flags": ["dec", "left"], "precision": 6, "width": 12, "fill": "*"},
    {"flags": ["dec", "internal"], "precision": 6, "width": 9, "fill": "0"},
    {"flags": ["dec", "right"], "precision": 6, "width": 9, "fill": "."},
    {"flags": ["fixed"], "precision": 2, "width": 0, "fill": " "},
    {"flags": ["scientific", "uppercase"], "precision": 4, "width": 0, "fill": " "},
    {"flags": ["fixed", "scientific"], "precision": 6, "width": 0, "fill": " "},
    {"flags": ["showpoint"], "precision": 0, "width": 0, "fill": " "},
    {"flags": [], "precision": -1, "width": 20, "fill": "#"},
]


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.ids = set()

    def _id(self, name):
        if name in self.ids:
            raise ValueError("duplicate case " + name)
        self.ids.add(name)
        return name

    def add(self, name, **inp):
        self.cases.append({"unit": UNIT, "case": self._id(name), "input": inp})

    def ub(self, name, **inp):
        request = {"unit": UNIT, "case": self._id(name), "input": inp}
        message = "undefined behaviour: " + inp["op"]
        self.controls.append({"request": request, "error": {"kind": "invalid_input", "message": message}})

    def domain(self, name, **inp):
        request = {"unit": UNIT, "case": self._id(name), "input": inp}
        message = "outside the guarded engine domain: " + inp["op"]
        self.controls.append({"request": request, "error": {"kind": "invalid_input", "message": message}})


def core(c, rng):
    c.add("constants", op="constants")
    for code in [1, 2, 4, 32, 64, 0, 3, -1, 128, 2**31 - 1]:
        c.add(f"do-error-{code}", op="do_error", code=code)
    # The last message has an embedded NUL: `std::string(const char*)` keeps only "a".
    for i, message in enumerate(["", "Values exceed permitted range", "Ω unicode \"quoted\"", "a\u0000b"]):
        c.add(f"exception-{i}", op="exception", message=message)

    # Point construction and conversion (Init rounding with std::round, ties away from zero).
    ties = [0.5, -0.5, 1.5, 2.5, -2.5, 0.49999999999999994, -0.49999999999999994, -0.0, 0.0,
            BELOW_TWO63, NEG_TWO63, 4503599627370497.0, 4503599627370496.5, 1e18, -1e-300,
            9007199254740993.0, 123456.75]
    for i, x in enumerate(ties):
        for z in [None, -7]:
            c.add(f"construct-i64-f64-{i}-{z}", op="point_construct", to="i64", **{"from": "f64"},
                  x=h(x), y=h(-x if x != NEG_TWO63 else BELOW_TWO63), z=z)
    for i, x in enumerate([I64_MAX, I64_MIN, 2**53 + 1, -(2**53 + 1), 2**62 + 1, 0, 9007199254740993]):
        c.add(f"construct-f64-i64-{i}", op="point_construct", to="f64", **{"from": "i64"}, x=x, y=-x if x != I64_MIN else x, z=3)
        c.add(f"construct-i64-i64-{i}", op="point_construct", to="i64", **{"from": "i64"}, x=x, y=x, z=None)
    for i, x in enumerate([NAN, NEG_NAN, INF, -INF, -0.0, 5e-324, 1.7976931348623157e308]):
        c.add(f"construct-f64-f64-{i}", op="point_construct", to="f64", **{"from": "f64"}, x=h(x), y=h(1.5), z=None)
    for i, x in enumerate([NAN, INF, -INF, TWO63, BELOW_NEG_TWO63, 9.3e18, -1e19, I64_MAX + 0.0]):
        c.ub(f"ub-construct-{i}", op="point_construct", to="i64", **{"from": "f64"}, x=h(x), y=h(0.0), z=None)
    for i, (to, frm, point) in enumerate([
            ("i64", "f64", pd(2.5, -2.5, 9)), ("i64", "f64", pd(BELOW_TWO63, NEG_TWO63, -1)),
            ("f64", "i64", p64(I64_MAX, I64_MIN, 4)), ("i64", "i64", p64(5, 6, 7)),
            ("f64", "f64", pd(NAN, -0.0, 2))]):
        for z in [None, 11]:
            c.add(f"convert-{i}-{z}", op="point_convert", to=to, **{"from": frm}, point=point, z=z)
            c.add(f"init-{i}-{z}", op="point_init", to=to, **{"from": frm}, point=pt(to, 1, 1, 99),
                  x=point[0], y=point[1], z=z)
    c.ub("ub-convert", op="point_convert", to="i64", **{"from": "f64"}, point=pd(1.0, NAN), z=None)
    c.ub("ub-init", op="point_init", to="i64", **{"from": "f64"}, point=p64(0, 0), x=h(INF), y=h(0.0), z=1)
    for t in ["i64", "f64"]:
        c.add(f"default-{t}", op="point_default", type=t)

    # operator*(double), unary and binary point operators.
    for i, (p, s) in enumerate([((3, -3), 0.5), ((5, 7), 1.0 / 3.0), ((1, -1), 2.5), ((MAX_COORD, -MAX_COORD), 2.0),
                                ((10**15, 3), 1e-9), ((7, 9), -0.0)]):
        c.add(f"scale-i64-{i}", op="point_scale", type="i64", point=p64(*p, z=5), scale=h(s))
    for i, s in enumerate([0.5, NAN, INF, -1.0]):
        c.add(f"scale-f64-{i}", op="point_scale", type="f64", point=pd(1.25, -0.0, 3), scale=h(s))
    c.ub("ub-scale-overflow", op="point_scale", type="i64", point=p64(I64_MAX, 0), scale=h(2.0))
    c.ub("ub-scale-nan", op="point_scale", type="i64", point=p64(1, 2), scale=h(NAN))
    for kind in ["negated", "negate", "set_z"]:
        c.add(f"unary-i64-{kind}", op="point_unary", type="i64", point=p64(I64_MAX, -5, 8), kind=kind, z=-2)
        c.add(f"unary-f64-{kind}", op="point_unary", type="f64", point=pd(-0.0, NAN, 8), kind=kind, z=I64_MIN)
    c.ub("ub-negated", op="point_unary", type="i64", point=p64(I64_MIN, 0), kind="negated", z=0)
    c.ub("ub-negate", op="point_unary", type="i64", point=p64(0, I64_MIN), kind="negate", z=0)
    for kind in ["plus", "minus", "eq", "ne"]:
        c.add(f"binary-i64-{kind}", op="point_binary", type="i64", a=p64(I64_MAX - 3, I64_MIN + 3, 1),
              b=p64(-3, 3, 2), kind=kind)
        c.add(f"binary-i64-eq-z-{kind}", op="point_binary", type="i64", a=p64(4, 5, 1), b=p64(4, 5, 9), kind=kind)
        c.add(f"binary-f64-{kind}", op="point_binary", type="f64", a=pd(0.1, NAN, 1), b=pd(0.2, 1.0, 2), kind=kind)
        c.add(f"binary-f64-zero-{kind}", op="point_binary", type="f64", a=pd(0.0, 1.0), b=pd(-0.0, 1.0, 4), kind=kind)
    c.ub("ub-plus", op="point_binary", type="i64", a=p64(I64_MAX, 0), b=p64(1, 0), kind="plus")
    c.ub("ub-minus", op="point_binary", type="i64", a=p64(0, I64_MIN), b=p64(0, 1), kind="minus")
    for i, (a, b) in enumerate([((-3, 3), (0, -4)), ((I64_MAX, I64_MIN), (-1, 0)), ((7, -7), (8, -8))]):
        c.add(f"midpoint-i64-{i}", op="midpoint", type="i64", a=p64(*a, z=4), b=p64(*b, z=5))
    c.add("midpoint-f64", op="midpoint", type="f64", a=pd(1e308, -0.0), b=pd(1e308, -0.0))
    c.ub("ub-midpoint", op="midpoint", type="i64", a=p64(I64_MAX, 0), b=p64(I64_MAX - 1, 0))


def rects(c, rng):
    for t in ["i64", "f64"]:
        for kind in ["default", "valid", "invalid", "invalid_rect", "invalid_constant"]:
            c.add(f"rect-{t}-{kind}", op="rect_construct", type=t, kind=kind, rect=None)
    c.add("rect-i64-values", op="rect_construct", type="i64", kind="values", rect=rect("i64", -1, 2, -3, 4))
    c.add("rect-f64-values", op="rect_construct", type="f64", kind="values", rect=[h(NAN), h(-0.0), h(INF), h(1.0)])
    samples = {
        "i64": [(0, 0, 10, 10), (5, 5, 5, 5), (10, 0, 0, 10), (-7, -9, 3, 1), (MAX_COORD, -MAX_COORD, -1, 1)],
        "f64": [(0.0, 0.0, 10.0, 10.0), (0.5, 0.25, -0.5, 4.0), (-INF, -INF, INF, INF), (NAN, 0.0, 1.0, 1.0),
                (1.7976931348623157e308, 1.7976931348623157e308, -1.7976931348623157e308, -1.7976931348623157e308)],
    }
    for t, items in samples.items():
        for i, r in enumerate(items):
            for kind in ["is_valid", "is_empty", "width", "height", "mid_point", "as_path"]:
                c.add(f"rect-query-{t}-{i}-{kind}", op="rect_query", type=t, rect=rect(t, *r), kind=kind)
            for kind in ["width", "height"]:
                value = 3 if t == "i64" else 2.5
                c.add(f"rect-set-{t}-{i}-{kind}", op="rect_set", type=t, rect=rect(t, *r), kind=kind, value=num(t, value))
    inv = [I64_MAX, I64_MAX, I64_MIN, I64_MIN]
    for kind in ["width", "height"]:
        c.ub(f"ub-rect-{kind}", op="rect_query", type="i64", rect=inv, kind=kind)
    c.add("rect-query-invalid-mid", op="rect_query", type="i64", rect=inv, kind="mid_point")
    c.ub("ub-rect-mid_point", op="rect_query", type="i64", rect=[I64_MAX, 0, 1, 0], kind="mid_point")
    c.ub("ub-rect-set", op="rect_set", type="i64", rect=[I64_MAX, 0, 0, 0], kind="width", value=1)
    for t in ["i64", "f64"]:
        base = rect(t, 0, 0, 10, 10)
        for i, (x, y) in enumerate([(5, 5), (0, 5), (10, 5), (5, 0), (5, 10), (-1, 5), (11, 11)]):
            c.add(f"rect-contains-point-{t}-{i}", op="rect_contains", type=t, rect=base, kind="point", other=pt(t, x, y))
        for i, r in enumerate([(0, 0, 10, 10), (1, 1, 9, 9), (-1, 0, 10, 10), (0, 0, 10, 11), (5, 5, 4, 4)]):
            c.add(f"rect-contains-rect-{t}-{i}", op="rect_contains", type=t, rect=base, kind="rect", other=rect(t, *r))
        pairs = [((0, 0, 10, 10), (10, 10, 20, 20)), ((0, 0, 10, 10), (11, 0, 20, 10)), ((0, 0, 10, 10), (2, 3, 4, 5)),
                 ((5, 5, 1, 1), (0, 0, 10, 10))]
        if t == "f64":
            pairs += [((NAN, 0, 10, 10), (0, 0, 10, 10)), ((0, 0, 10, 10), (NAN, NAN, NAN, NAN)),
                      ((-0.0, 0.0, 1.0, 1.0), (0.0, -0.0, 1.0, 1.0))]
        for i, (a, b) in enumerate(pairs):
            for kind in ["intersects", "eq", "plus", "add_assign"]:
                c.add(f"rect-binary-{t}-{i}-{kind}", op="rect_binary", type=t, a=rect(t, *a), b=rect(t, *b), kind=kind)
    for i, (r, s) in enumerate([((5, -5, 3, 7), 0.5), ((1, 2, 3, 4), -2.5), ((MAX_COORD, 0, 0, 0), 3.9), ((9, 9, 9, 9), 1 / 3)]):
        c.add(f"rect-scale-i64-{i}", op="rect_scale", type="i64", rect=rect("i64", *r), scale=h(s))
    c.add("rect-scale-f64", op="rect_scale", type="f64", rect=rect("f64", 0.1, 0.2, 0.3, NAN), scale=h(3.0))
    c.ub("ub-rect-scale", op="rect_scale", type="i64", rect=[0, 0, I64_MAX, 0], scale=h(1.0))
    c.ub("ub-rect-scale-nan", op="rect_scale", type="i64", rect=[0, 0, 1, 0], scale=h(NAN))
    for to in ["i64", "f64"]:
        for frm in ["i64", "f64"]:
            for i, s in enumerate([0.5, -1.5, 2.0, 1e-3]):
                r = rect(frm, 3, -3, 5, 7) if frm == "i64" else rect(frm, 2.5, -2.5, 0.49999999999999994, 1e15 + 0.5)
                c.add(f"scale-rect-{to}-{frm}-{i}", op="scale_rect", to=to, **{"from": frm}, rect=r, scale=h(s))
    c.ub("ub-scale-rect", op="scale_rect", to="i64", **{"from": "f64"}, rect=rect("f64", 1e19, 0, 0, 0), scale=h(1.0))
    c.ub("ub-scale-rect-int", op="scale_rect", to="i64", **{"from": "i64"}, rect=[I64_MAX, 0, 0, 0], scale=h(2.0))


def paths(c, rng):
    data = {
        "i64": [[], [p64(3, -4, 1)], [p64(1, 2), p64(-5, 9), p64(7, -8, 3)], [p64(I64_MAX, I64_MIN), p64(I64_MIN, I64_MAX)]],
        "f64": [[], [pd(NAN, 1.0), pd(2.5, NAN)], [pd(-0.0, 0.0), pd(0.0, -0.0)], [pd(1.5, -2.5), pd(-INF, 7.0), pd(3.0, INF)]],
    }
    for to in ["i64", "f64"]:
        for frm in ["i64", "f64"]:
            for i, p in enumerate(data[frm]):
                if to == "i64" and frm == "f64" and i == 3:
                    continue
                c.add(f"bounds-path-{to}-{frm}-{i}", op="get_bounds", to=to, **{"from": frm}, shape="path", value=p)
            c.add(f"bounds-paths-{to}-{frm}", op="get_bounds", to=to, **{"from": frm}, shape="paths",
                  value=[p for j, p in enumerate(data[frm]) if not (to == "i64" and frm == "f64" and j == 3)])
            c.add(f"bounds-paths-empty-{to}-{frm}", op="get_bounds", to=to, **{"from": frm}, shape="paths", value=[[], []])
    c.add("bounds-trunc", op="get_bounds", to="i64", **{"from": "f64"}, shape="path",
          value=[pd(-2.9, 2.9), pd(BELOW_TWO63, NEG_TWO63)])
    c.ub("ub-bounds", op="get_bounds", to="i64", **{"from": "f64"}, shape="path", value=[pd(1e19, 0.0)])
    c.ub("ub-bounds-inf", op="get_bounds", to="i64", **{"from": "f64"}, shape="paths", value=[[pd(0.0, -INF)]])
    for to in ["i64", "f64"]:
        for frm in ["i64", "f64"]:
            c.add(f"append-point-{to}-{frm}", op="append_point", to=to, **{"from": frm},
                  path=path(to, [(1, 2)]), point=pt(frm, 3, 4, 5) if frm == "i64" else pd(2.5, -0.5, 6))
    c.ub("ub-append-point", op="append_point", to="i64", **{"from": "f64"}, path=[], point=pd(NAN, 0.0))
    for t in ["i64", "f64"]:
        c.add(f"append-path-{t}", op="append_path", type=t, paths=[path(t, [(1, 1)])], path=path(t, [(2, 2), (3, 3)], 4))
        c.add(f"append-path-empty-{t}", op="append_path", type=t, paths=[], path=[])

    # ScalePath / ScalePaths: zero-scale and range exceptions with their error codes.
    src = {"i64": [p64(3, -3, 2), p64(5, 7, -1)], "f64": [pd(1.25, -2.5, 2), pd(0.5, 1e-3, 4)]}
    for to in ["i64", "f64"]:
        for frm in ["i64", "f64"]:
            for i, (sx, sy) in enumerate([(2.0, None), (0.5, 3.0), (0.0, None), (1.0, 0.0), (-0.0, 2.0), (-1.5, None)]):
                for code in [0, 5]:
                    c.add(f"scale-path-{to}-{frm}-{i}-{code}", op="scale_path", to=to, **{"from": frm}, value=src[frm],
                          scale_x=h(sx), scale_y=None if sy is None else h(sy), error_code=code)
            groups = [[src[frm], []], [], [[]], [src[frm], src[frm]]]
            for i, g in enumerate(groups):
                for j, (sx, sy) in enumerate([(4.0, None), (0.0, None), (-1.0, 1.0), (1.0, NAN)]):
                    if to == "i64" and j == 3 and any(g):
                        continue
                    c.add(f"scale-paths-{to}-{frm}-{i}-{j}", op="scale_paths", to=to, **{"from": frm}, value=g,
                          scale_x=h(sx), scale_y=None if sy is None else h(sy), error_code=1)
    huge = {"i64": [[p64(MAX_COORD, 0)]], "f64": [[pd(2.0**61, 0.0)]]}
    for frm in ["i64", "f64"]:
        for i, s in enumerate([1.0, 1.0000000000000002, -1.0, 0.5]):
            c.add(f"scale-paths-range-{frm}-{i}", op="scale_paths", to="i64", **{"from": frm}, value=huge[frm],
                  scale_x=h(s), scale_y=None, error_code=0)
        c.add(f"scale-paths-range-y-{frm}", op="scale_paths", to="i64", **{"from": frm},
              value=[[pt(frm, 0, 2**40)]], scale_x=h(1.0), scale_y=h(2.0**22), error_code=8)
        c.add(f"scale-paths-to-f64-{frm}", op="scale_paths", to="f64", **{"from": frm}, value=huge[frm],
              scale_x=h(1e300), scale_y=h(-1e300), error_code=0)
    c.add("scale-paths-inf", op="scale_paths", to="i64", **{"from": "f64"}, value=[[pd(INF, 0.0)]],
          scale_x=h(1.0), scale_y=None, error_code=0)
    c.ub("ub-scale-path", op="scale_path", to="i64", **{"from": "f64"}, value=[pd(1e300, 0.0)],
         scale_x=h(1.0), scale_y=None, error_code=0)
    c.ub("ub-scale-paths-nan", op="scale_paths", to="i64", **{"from": "f64"}, value=[[pd(NAN, 0.0)]],
         scale_x=h(1.0), scale_y=None, error_code=0)
    c.ub("ub-scale-paths-nan-scale", op="scale_paths", to="i64", **{"from": "i64"}, value=[[p64(1, 1)]],
         scale_x=h(NAN), scale_y=None, error_code=0)
    for to in ["i64", "f64"]:
        for frm in ["i64", "f64"]:
            value = [pt(frm, 2, 3, 4), pd(-2.5, 0.5, 6) if frm == "f64" else p64(I64_MIN, I64_MAX, 6)]
            c.add(f"transform-path-{to}-{frm}", op="transform_path", to=to, **{"from": frm}, value=value)
            c.add(f"transform-paths-{to}-{frm}", op="transform_paths", to=to, **{"from": frm}, value=[value, [], value[:1]])
    c.ub("ub-transform-path", op="transform_path", to="i64", **{"from": "f64"}, value=[pd(TWO63, 0.0)])
    c.ub("ub-transform-paths", op="transform_paths", to="i64", **{"from": "f64"}, value=[[], [pd(0.0, -INF)]])

    for i, v in enumerate([I64_MAX, I64_MIN, -3, 0]):
        c.add(f"sqr-i64-{i}", op="sqr", type="i64", value=v)
    for i, v in enumerate([NAN, -0.0, 1e200, 3.0]):
        c.add(f"sqr-f64-{i}", op="sqr", type="f64", value=h(v))
    for t in ["i64", "f64"]:
        for i, (a, b, m) in enumerate([((0, 0), (3, 4), 25.0), ((0, 0), (3, 4), 25.000000000000004), ((1, 1), (1, 1), 0.0),
                                       ((1, 1), (1, 1), 1e-300), ((0, 0), (1, 0), INF), ((0, 0), (1, 0), NAN)]):
            c.add(f"near-equal-{t}-{i}", op="near_equal", type=t, a=pt(t, *a), b=pt(t, *b), max=h(m))
    c.ub("ub-near-equal", op="near_equal", type="i64", a=p64(I64_MAX, 0), b=p64(-1, 0), max=h(1.0))
    seqs = [[], [(0, 0)], [(0, 0), (0, 1), (0, 3), (5, 5), (5, 6), (0, 1)], [(0, 0), (1, 0), (1, 1), (0, 1), (0, 0)],
            [(4, 4)] * 4, [(0, 0), (10, 0), (10, 10), (1, 1)]]
    for t in ["i64", "f64"]:
        for i, s in enumerate(seqs):
            for closed in [False, True]:
                for m in [2.0, 0.0, INF]:
                    c.add(f"strip-near-{t}-{i}-{closed}-{m}", op="strip_near_equal", type=t, shape="path",
                          value=path(t, s), max=h(m), closed=closed)
            c.add(f"strip-dup-{t}-{i}-open", op="strip_duplicates", type=t, shape="path", value=path(t, s), closed=False)
            c.add(f"strip-dup-{t}-{i}-closed", op="strip_duplicates", type=t, shape="path", value=path(t, s), closed=True)
        group = [path(t, s) for s in seqs]
        c.add(f"strip-near-paths-{t}", op="strip_near_equal", type=t, shape="paths", value=group, max=h(1.5), closed=True)
        c.add(f"strip-dup-paths-{t}", op="strip_duplicates", type=t, shape="paths", value=group, closed=True)
    zpath = [p64(1, 1, 1), p64(1, 1, 2), p64(2, 2, 3), p64(1, 1, 4), p64(1, 1, 5)]
    c.add("strip-dup-z", op="strip_duplicates", type="i64", shape="path", value=zpath, closed=True)
    c.add("strip-dup-nan", op="strip_duplicates", type="f64", shape="path", value=[pd(NAN, 0.0), pd(NAN, 0.0), pd(-0.0, 1.0), pd(0.0, 1.0)], closed=True)
    c.add("strip-near-z", op="strip_near_equal", type="i64", shape="path", value=zpath, max=h(1.0), closed=True)
    c.ub("ub-strip-near", op="strip_near_equal", type="i64", shape="path", value=[p64(I64_MIN, 0), p64(1, 0)], max=h(1.0), closed=False)
    c.ub("ub-strip-near-closed", op="strip_near_equal", type="i64", shape="path",
         value=[p64(-1, 0), p64(0, 0), p64(I64_MAX, 0)], max=h(1e300), closed=True)


def misc(c, rng):
    for p in range(-10, 11):
        for code in [None, 0, 6]:
            c.add(f"precision-{p}-{code}", op="check_precision", precision=p, error_code=code)
    c.add("precision-min", op="check_precision", precision=-2**31, error_code=1)
    for i, x in enumerate([I64_MIN, -1, 0, 1, I64_MAX]):
        c.add(f"tri-sign-{i}", op="tri_sign", x=x)
    pairs = [(0, 0), (U64_MAX, U64_MAX), (U64_MAX, 1), (2**32, 2**32), (2**32 - 1, 2**32 + 1)]
    pairs += [(rng.next(), rng.next()) for _ in range(12)]
    for i, (a, b) in enumerate(pairs):
        c.add(f"multiply-{i}", op="multiply_u64", a=a, b=b)
    for i, (a, b) in enumerate([([1, 2], [1, 2]), ([1, 2], [1, 3]), ([0, 2], [1, 2]), ([U64_MAX, 0], [U64_MAX, 0])]):
        c.add(f"uint128-eq-{i}", op="uint128_eq", a=a, b=b)
    quads = [(I64_MIN, I64_MIN, I64_MIN, I64_MIN), (I64_MIN, -1, I64_MAX, 1), (I64_MIN, 1, I64_MIN, 1), (0, 5, 0, -7),
             (0, 5, 1, 0), (3, 4, 2, 6), (-3, 4, 2, -6), (-3, 4, 2, 6), (I64_MAX, I64_MAX, I64_MAX, I64_MAX),
             (2**32, 2**32, 2**33, 2**31), (2**32 + 1, 2**32, 2**33 + 2, 2**31)]
    for _ in range(12):
        a, b = rng.int(-2**62, 2**62), rng.int(-2**62, 2**62)
        quads.append((a, b, b, a) if rng.next() % 2 else (a, b, a, b + 1))
    for i, q in enumerate(quads):
        c.add(f"products-equal-{i}", op="products_are_equal", a=q[0], b=q[1], c=q[2], d=q[3])


def products(c, rng):
    def tri_i64(lim):
        return [p64(rng.int(-lim, lim), rng.int(-lim, lim), rng.int(-9, 9)) for _ in range(3)]

    triples = [[p64(0, 0), p64(1, 1), p64(2, 2)], [p64(0, 0), p64(1, 0), p64(1, 1)], [p64(0, 0), p64(1, 0), p64(1, -1)],
               [p64(-MAX_COORD, -MAX_COORD), p64(0, 0), p64(MAX_COORD, MAX_COORD)],
               [p64(-MAX_COORD, -MAX_COORD), p64(0, 0), p64(MAX_COORD, MAX_COORD - 1)],
               [p64(0, 0), p64(2**62, 2**62 - 1), p64(2 * 2**62 - 1, 2 * 2**62 - 3)],
               [p64(5, 5), p64(5, 5), p64(5, 5)], [p64(0, 0), p64(0, 0), p64(3, 4)],
               [p64(0, 0), p64(I64_MAX, 1), p64(I64_MAX, 2)], [p64(-1, -1), p64(I64_MAX - 1, I64_MAX - 1), p64(I64_MAX, I64_MAX)]]
    triples += [tri_i64(2**61) for _ in range(10)] + [tri_i64(1000) for _ in range(6)]
    for i, tr in enumerate(triples):
        for op in ["cross_product_sign", "is_collinear", "cross3", "dot3", "perpendic_dist"]:
            c.add(f"{op}-i64-{i}", op=op, type="i64", points=tr)
    dtriples = [[pd(0.0, 0.0), pd(0.9, 0.9), pd(1.9, 1.9)], [pd(0.5, 0.0), pd(1.5, 0.0), pd(1.5, 1.0)],
                [pd(-0.0, 0.0), pd(1e18, 1e18), pd(2e18, 2e18 + 512.0)], [pd(0.0, 0.0), pd(9.2e18, -9.2e18), pd(0.0, 0.0)],
                [pd(0.0, 0.0), pd(1e18, 1.0), pd(2e18, 1e18)], [pd(1.0, 1.0), pd(1.0, 1.0), pd(1.0, 1.0)]]
    dtriples += [[pd(rng.real(-1e6, 1e6), rng.real(-1e6, 1e6), 1) for _ in range(3)] for _ in range(8)]
    for i, tr in enumerate(dtriples):
        for op in ["cross_product_sign", "cross3", "dot3", "perpendic_dist"]:
            c.add(f"{op}-f64-{i}", op=op, type="f64", points=tr)
        c.add(f"is_collinear-f64-{i}", op="is_collinear", type="f64", points=tr)
    for i, tr in enumerate([[pd(NAN, 0.0), pd(1.0, 1.0), pd(2.0, 2.0)], [pd(INF, 0.0), pd(1.0, 1.0), pd(2.0, 2.0)],
                            [pd(0.0, 0.0), pd(2e38, 1.0), pd(4e38, 2e38)], [pd(0.0, 0.0), pd(1e37, 1.0), pd(2e37, 1e37)]]):
        c.ub(f"ub-cross-sign-f64-{i}", op="cross_product_sign", type="f64", points=tr)
    c.ub("ub-collinear-f64", op="is_collinear", type="f64", points=[pd(0.0, 0.0), pd(1e19, 1.0), pd(2.0, 2.0)])
    c.ub("ub-collinear-f64-nan", op="is_collinear", type="f64", points=[pd(0.0, 0.0), pd(1.0, NAN), pd(2.0, 2.0)])
    big = [p64(I64_MIN, 0), p64(I64_MAX, 0), p64(0, 0)]
    for op in ["cross_product_sign", "is_collinear", "cross3", "dot3", "perpendic_dist"]:
        c.ub(f"ub-{op}-i64", op=op, type="i64", points=big)
    pairs = [[p64(3, 4, 1), p64(-5, 6, 2)], [p64(2**31, 2**31), p64(2**31, -2**31)], [p64(0, 0), p64(I64_MAX, I64_MIN)],
             [p64(-3037000499, 3037000499), p64(3037000499, 3037000499)]]
    pairs += [[p64(rng.int(-2**31, 2**31), rng.int(-2**31, 2**31)) for _ in range(2)] for _ in range(6)]
    for i, pr in enumerate(pairs):
        for op in ["cross2", "dot2"]:
            c.add(f"{op}-i64-{i}", op=op, type="i64", points=pr)
        if i != 2:
            c.add(f"distance_sqr-i64-{i}", op="distance_sqr", type="i64", points=pr)
    for i, pr in enumerate([[pd(0.1, 0.2), pd(0.3, NAN)], [pd(1e200, 1e200), pd(1e200, -1e200)], [pd(-0.0, 0.0), pd(0.0, -0.0)]]):
        for op in ["cross2", "dot2", "distance_sqr"]:
            c.add(f"{op}-f64-{i}", op=op, type="f64", points=pr)
    for op in ["cross2", "dot2"]:
        c.ub(f"ub-{op}", op=op, type="i64", points=[p64(2**32, 2**32), p64(2**31, 2**31)])
    c.ub("ub-distance-sqr", op="distance_sqr", type="i64", points=[p64(I64_MIN, 0), p64(1, 0)])


def polygons(c, rng):
    def poly_i64(n, lim):
        return [p64(rng.int(-lim, lim), rng.int(-lim, lim), rng.int(0, 3)) for _ in range(n)]

    def poly_f64(n, lim):
        return [pd(rng.real(-lim, lim), rng.real(-lim, lim), 1) for _ in range(n)]

    shapes = {"i64": [[], [p64(1, 1)], [p64(0, 0), p64(5, 5)], path("i64", [(0, 0), (10, 0), (10, 10)]),
                      path("i64", [(0, 0), (10, 0), (10, 10), (0, 10)]), path("i64", [(0, 0), (0, 10), (10, 10), (10, 0)]),
                      path("i64", [(0, 0), (5, 0), (10, 0)]), path("i64", [(0, 0), (4, 0), (4, 4), (2, 1), (0, 4)]),
                      path("i64", [(-MAX_COORD, -MAX_COORD), (MAX_COORD, -MAX_COORD), (MAX_COORD, MAX_COORD)])]
                     + [poly_i64(n, 2**40) for n in (3, 4, 5, 6, 7, 16, 33)],
              "f64": [[], path("f64", [(0.5, 0.5), (10.25, 0.0), (10.0, 10.0)]), path("f64", [(0, 0), (1, 0), (1, 1), (0, 1)]),
                      [pd(0.0, 0.0), pd(NAN, 1.0), pd(1.0, 1.0)], [pd(0.0, 0.0), pd(1e308, 0.0), pd(1e308, 1e308)]]
                     + [poly_f64(n, 1e3) for n in (3, 4, 5, 8, 13)]}
    for t, items in shapes.items():
        for i, s in enumerate(items):
            c.add(f"area-{t}-{i}", op="area", type=t, shape="path", value=s)
            c.add(f"is-positive-{t}-{i}", op="is_positive", type=t, shape="path", value=s)
        c.add(f"area-paths-{t}", op="area", type=t, shape="paths", value=items)
        c.add(f"area-paths-empty-{t}", op="area", type=t, shape="paths", value=[])
    c.ub("ub-area", op="area", type="i64", shape="path", value=[p64(0, I64_MAX), p64(1, 1), p64(2, 0)])
    c.ub("ub-area-x", op="area", type="i64", shape="path", value=[p64(I64_MIN, 0), p64(1, 0), p64(2, 0), p64(3, 0)])
    c.ub("ub-is-positive", op="is_positive", type="i64", shape="path", value=[p64(0, 0), p64(1, I64_MAX), p64(2, 1)])
    c.ub("ub-area-paths", op="area", type="i64", shape="paths", value=[[], [p64(I64_MAX, 0), p64(-1, 0), p64(0, 0)]])

    # PointInPolygon: vertices, edges, horizontals, starts on the query line, wrap-around.
    square = path("i64", [(0, 0), (10, 0), (10, 10), (0, 10)])
    concave = path("i64", [(0, 0), (10, 0), (10, 10), (5, 3), (0, 10)])
    start_on = path("i64", [(0, 5), (0, 0), (10, 0), (10, 10), (0, 10)])
    line = path("i64", [(0, 5), (3, 5), (7, 5)])
    star = path("i64", [(0, 0), (6, 0), (6, 6), (3, 2), (0, 6), (0, 3), (-2, 3), (0, 1)])
    queries = [(5, 5), (0, 0), (10, 5), (5, 10), (11, 5), (-1, 5), (5, -1), (3, 5), (5, 3), (0, 5), (10, 10), (5, 4)]
    for name, poly in [("square", square), ("concave", concave), ("start-on", start_on), ("line", line), ("star", star),
                       ("short", square[:2])]:
        for j, q in enumerate(queries):
            c.add(f"pip-i64-{name}-{j}", op="point_in_polygon", type="i64", point=p64(*q), polygon=poly)
    for j in range(24):
        poly = poly_i64(rng.int(3, 12), 50)
        c.add(f"pip-i64-random-{j}", op="point_in_polygon", type="i64", point=p64(rng.int(-50, 50), rng.int(-50, 50)), polygon=poly)
    for j in range(6):
        poly = poly_i64(rng.int(3, 8), 2**61)
        c.add(f"pip-i64-big-{j}", op="point_in_polygon", type="i64", point=p64(rng.int(-2**61, 2**61), rng.int(-2**61, 2**61)), polygon=poly)
    fsq = path("f64", [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])
    for j, q in enumerate([(5.0, 5.0), (0.25, 0.5), (10.0, 5.0), (10.5, 5.0), (9.9, 9.9), (0.0, 10.0)]):
        c.add(f"pip-f64-{j}", op="point_in_polygon", type="f64", point=pd(*q), polygon=fsq)
    c.add("pip-f64-diagonal", op="point_in_polygon", type="f64", point=pd(0.5, 0.75),
          polygon=path("f64", [(0.0, 0.0), (1.9, 1.5), (0.0, 1.0)]))
    c.ub("ub-pip-f64-nan-y", op="point_in_polygon", type="f64", point=pd(5.0, NAN), polygon=fsq)
    c.ub("ub-pip-f64-nan", op="point_in_polygon", type="f64", point=pd(NAN, 5.0), polygon=fsq)
    for j in range(8):
        c.add(f"pip-f64-random-{j}", op="point_in_polygon", type="f64", point=pd(rng.real(-5, 5), rng.real(-5, 5)),
              polygon=poly_f64(rng.int(3, 9), 5.0))
    c.ub("ub-pip", op="point_in_polygon", type="i64", point=p64(0, 0),
         polygon=[p64(I64_MIN, -1), p64(I64_MAX, 1), p64(0, 5)])
    c.ub("ub-pip-f64", op="point_in_polygon", type="f64", point=pd(0.0, 0.0),
         polygon=[pd(-1e38, -1.0), pd(1e38, 1e38), pd(0.0, 5.0)])


def lines(c, rng):
    cases = [  # ln1a, ln1b, ln2a, ln2b, ip
        ((0, 0, 4), (10, 10, 5), (0, 10, 6), (10, 0, 7)), ((0, 0, 4), (10, 0, 5), (0, 5, 6), (10, 5, 7)),
        ((0, 0, 4), (10, 10, 5), (-5, 0, 6), (-5, 10, 7)), ((0, 0, 4), (10, 10, 5), (20, 0, 6), (20, 10, 7)),
        ((0, 0, 4), (3, 7, 5), (0, 5, 6), (9, -1, 7)), ((0, 0, 4), (0, 0, 5), (1, 1, 6), (2, 3, 7)),
        ((-MAX_COORD, 0, 1), (MAX_COORD, 1, 2), (0, -MAX_COORD, 3), (1, MAX_COORD, 4)),
    ]
    for i, (a, b, d, e) in enumerate(cases):
        c.add(f"line-i64-{i}", op="line_intersect", type="i64", points=[p64(*a), p64(*b), p64(*d), p64(*e)], ip=p64(99, 98, 97))
        c.add(f"line-f64-{i}", op="line_intersect", type="f64", points=[pd(*a), pd(*b), pd(*d), pd(*e)], ip=pd(99, 98, 97))
    c.add("line-f64-nan", op="line_intersect", type="f64", points=[pd(0, 0), pd(NAN, 1), pd(0, 1), pd(1, 0)], ip=pd(1, 1, 1))
    c.add("line-f64-inf", op="line_intersect", type="f64", points=[pd(0, 0), pd(INF, 1), pd(0, 1), pd(1, 0)], ip=pd(1, 1, 1))
    for j in range(10):
        pts = [p64(rng.int(-2**40, 2**40), rng.int(-2**40, 2**40), j) for _ in range(4)]
        c.add(f"line-i64-random-{j}", op="line_intersect", type="i64", points=pts, ip=p64(0, 0, -1))
    c.ub("ub-line-sub", op="line_intersect", type="i64", points=[p64(I64_MIN, 0), p64(I64_MAX, 0), p64(0, 0), p64(1, 1)], ip=p64(0, 0))
    c.ub("ub-line-cast", op="line_intersect", type="i64",
         points=[p64(I64_MAX - 1000, 0), p64(I64_MAX, 1000), p64(I64_MAX - 10, 1000), p64(I64_MAX - 10, 0)], ip=p64(0, 0))
    for t in ["i64", "f64"]:
        for i, (dx, dy) in enumerate([(0.5, -0.5), (2.5, -2.5), (1e-9, 0.49999999999999994), (-0.0, 0.0)]):
            c.add(f"translate-{t}-{i}", op="translate_point", type=t, point=pt(t, 3, -3, 6), dx=h(dx), dy=h(dy))
        c.add(f"reflect-{t}", op="reflect_point", type=t, point=pt(t, 3, -3, 6), pivot=pt(t, -10, 20, 7))
    c.add("translate-f64-nan", op="translate_point", type="f64", point=pd(1, 1, 1), dx=h(NAN), dy=h(INF))
    c.add("reflect-i64-extreme", op="reflect_point", type="i64", point=p64(I64_MAX, I64_MIN + 1, 1), pivot=p64(0, -1, 2))
    c.ub("ub-reflect-extreme", op="reflect_point", type="i64", point=p64(I64_MAX, I64_MIN, 1), pivot=p64(0, 0, 2))
    c.ub("ub-translate", op="translate_point", type="i64", point=p64(I64_MAX, 0), dx=h(1024.0), dy=h(0.0))
    c.ub("ub-translate-nan", op="translate_point", type="i64", point=p64(0, 0), dx=h(0.0), dy=h(NAN))
    c.ub("ub-reflect", op="reflect_point", type="i64", point=p64(-1, 0), pivot=p64(I64_MAX, 0))
    c.ub("ub-reflect-sub", op="reflect_point", type="i64", point=p64(I64_MIN, 0), pivot=p64(0, 0))
    for i, v in enumerate([I64_MIN, -1, 0, 1, I64_MAX]):
        c.add(f"sign-i64-{i}", op="get_sign", type="i64", value=v)
    for i, v in enumerate([NAN, NEG_NAN, -0.0, 0.0, 5e-324, -INF, 2.0]):
        c.add(f"sign-f64-{i}", op="get_sign", type="f64", value=h(v))
    segs = [((0, 0), (10, 10), (0, 10), (10, 0)), ((0, 0), (10, 10), (10, 10), (20, 0)), ((0, 0), (10, 0), (5, 0), (5, 5)),
            ((0, 0), (10, 0), (0, 0), (20, 0)), ((0, 0), (10, 0), (11, -1), (11, 1)), ((0, 0), (4, 4), (4, 4), (8, 0)),
            ((0, 0), (10, 0), (5, 5), (5, 0)), ((5, 5), (5, 0), (0, 0), (10, 0)), ((0, 0), (1, 1), (2, 2), (3, 3)),
            ((10, 0), (0, 0), (5, -5), (5, 5)), ((0, 10), (0, 0), (-5, 5), (5, 5))]
    segs += [tuple((rng.int(-20, 20), rng.int(-20, 20)) for _ in range(4)) for _ in range(14)]
    for i, s in enumerate(segs):
        for inc in [None, False, True]:
            c.add(f"segments-{i}-{inc}", op="segments_intersect", points=[p64(*q) for q in s], inclusive=inc)
    c.add("segments-extreme", op="segments_intersect", points=[p64(-MAX_COORD, 0), p64(MAX_COORD, 1), p64(0, -MAX_COORD), p64(1, MAX_COORD)], inclusive=True)
    c.ub("ub-segments", op="segments_intersect", points=[p64(I64_MIN, 0), p64(I64_MAX, 1), p64(0, 0), p64(1, 1)], inclusive=None)
    c.add("segments-parallel-extreme", op="segments_intersect", points=[p64(I64_MIN, 0), p64(I64_MIN, 1), p64(1, 0), p64(1, 1)], inclusive=None)
    c.ub("ub-segments-t", op="segments_intersect", points=[p64(I64_MIN, 0), p64(I64_MIN, 1), p64(1, 0), p64(2, 1)], inclusive=None)
    closest = [((5, 5, 1), (0, 0, 2), (10, 0, 3)), ((-5, 5, 1), (0, 0, 2), (10, 0, 3)), ((15, 5, 1), (0, 0, 2), (10, 0, 3)),
               ((1, 0, 1), (0, 0, 2), (0, 0, 3)), ((3, 2, 1), (0, 0, 2), (5, 0, 3)), ((5, 3, 1), (0, 0, 2), (4, 4, 3)),
               ((1, 4, 1), (0, 0, 2), (2, 6, 3)), ((0, 5, 0), (0, 0, 0), (5, 5, 0))]
    for t in ["i64", "f64"]:
        for i, (o, a, b) in enumerate(closest):
            c.add(f"closest-{t}-{i}", op="closest_point", type=t, points=[pt(t, *o), pt(t, *a), pt(t, *b)])
    c.add("closest-tie", op="closest_point", type="i64", points=[p64(5, 5), p64(0, 0), p64(1, 1)])
    c.add("closest-f64-tiny", op="closest_point", type="f64", points=[pd(1.0, 1.0), pd(0.0, 0.0), pd(1e-200, 0.0)])
    c.add("closest-f64-nan", op="closest_point", type="f64", points=[pd(NAN, 1.0), pd(0.0, 0.0), pd(1.0, 0.0)])
    c.ub("ub-closest", op="closest_point", type="i64", points=[p64(0, 0), p64(I64_MIN, 0), p64(1, 0)])
    c.add("closest-near-max", op="closest_point", type="i64", points=[p64(I64_MAX, 0), p64(I64_MAX - 10, 0), p64(I64_MAX, 1)])
    # dx = 2^53 + 3 rounds up to 2^53 + 4 as a double, so the clamped offset overshoots seg2.
    c.ub("ub-closest-add", op="closest_point", type="i64",
         points=[p64(I64_MAX, 0), p64(I64_MAX - (2**53 + 3), 0), p64(I64_MAX, 0)])


def streams(c, rng):
    values = {
        "i64": {"point": p64(-42, I64_MAX, I64_MIN), "rect": [I64_MIN, 0, I64_MAX, -1],
                "path": [p64(1, -2, 3), p64(0, 0, 0)], "paths": [[p64(1, 2)], [], [p64(-3, 4, 5), p64(6, -7, 8)]]},
        "f64": {"point": pd(0.1, -0.0, 7), "rect": [h(NAN), h(NEG_NAN), h(INF), h(-INF)],
                "path": [pd(1e-5, 123456789.0, 1), pd(1.5, 1e100, -2)], "paths": [[pd(2.0 / 3.0, 1e21)], []]},
    }
    for t, shapes in values.items():
        for shape, value in shapes.items():
            for i, state in enumerate(STATES):
                c.add(f"stream-{t}-{shape}-{i}", op="stream", type=t, shape=shape, value=value, state=state)
        for i in [0, 5]:
            c.add(f"stream-{t}-empty-path-{i}", op="stream", type=t, shape="path", value=[], state=STATES[i])


def engine(c, rng):
    flags = [0, 1, 2, 4, 8, 15, 2**32 - 1, 2**31]
    for a in flags:
        for b in flags[::2]:
            for kind in ["and", "or"]:
                c.add(f"flags-{a}-{b}-{kind}", op="vertex_flags", a=a, b=b, kind=kind)
    c.add("engine-defaults", op="engine_defaults")
    for kind in ["outpt", "local_minima", "intersect_node", "horz_segment", "horz_join"]:
        for flag in [False, True]:
            for j, point in enumerate([p64(3, -4, 0), p64(I64_MAX, I64_MIN, 6)]):
                c.add(f"construct-{kind}-{flag}-{j}", op="engine_construct", kind=kind, point=point, flag=flag)
    square = path("i64", [(0, 0), (10, 0), (10, 10), (0, 10)])
    c.add("stripe-index-build", op="stripe_index", path=square, query=p64(5, 5))
    c.add("stripe-index-empty-path", op="stripe_index", path=[], query=None)
    # StripeIndex::Build's y range, (y_range + stripe_count_ - 1) and YToStripe(pt.y) at their
    # defined limits, then one past each, and an overflowing CrossProductSign in the stripe.
    c.add("stripe-index-height-limit", op="stripe_index", path=[p64(0, -1), p64(1, I64_MAX - 2), p64(2, 0)],
          query=None)
    c.add("stripe-index-query-limit", op="stripe_index", path=[p64(0, -1), p64(10, -1), p64(10, 10)],
          query=p64(5, I64_MAX - 1))
    c.add("stripe-index-declined-far-query", op="stripe_index",
          path=[p64(x, 0 if x % 2 else 10**6) for x in range(100)], query=p64(4, I64_MIN))
    c.ub("ub-stripe-index-y-range", op="stripe_index", path=[p64(0, I64_MIN), p64(1, I64_MAX), p64(2, 0)],
         query=None)
    c.ub("ub-stripe-index-height", op="stripe_index", path=[p64(0, -1), p64(1, I64_MAX - 1), p64(2, 0)],
         query=None)
    c.ub("ub-stripe-index-query-stripe", op="stripe_index", path=[p64(0, 1), p64(10, 1), p64(10, 10)],
         query=p64(5, I64_MIN))
    c.ub("ub-stripe-index-cross-product", op="stripe_index", path=[p64(I64_MIN, 0), p64(I64_MAX, 0), p64(0, 10)],
         query=p64(0, 5))
    for cls, precision in [("64", None), ("D", None), ("D", -8), ("D", 8), ("D", 9), ("D", -9), ("D", 0)]:
        for j, (pc, rs, th) in enumerate([(None, None, None), (False, True, 0), (True, False, U64_MAX)]):
            c.add(f"settings-{cls}-{precision}-{j}", op="clipper_settings", **{"class": cls}, precision=precision,
                  preserve_collinear=pc, reverse_solution=rs, pip_index_threshold=th)
    for flag in [False, True]:
        c.add(f"clipper64-z-{flag}", op="clipper64_z_callback", set=flag)
    pts5 = [p64(1, 2, 3), p64(-4, 5, 6), p64(I64_MAX, I64_MIN, 7), p64(0, 0, 8), p64(100, -100, 9)]
    for op in ["clipperd_zcb", "clipperd_check_callback"]:
        for precision in [None, -8, -1, 0, 3, 8, 9]:
            for callback in [False, True]:
                for invoke in [False, True]:
                    c.add(f"{op}-{precision}-{callback}-{invoke}", op=op, precision=precision, callback=callback,
                          points=pts5, z=-77, invoke=invoke)
    sub = [[pd(0.0, 0.0), pd(10.5, 0.0), pd(10.5, 10.25), pd(0.0, 10.25)]]
    for kind in ["subject", "open_subject", "clip"]:
        for precision in [None, 8, -8, 12]:
            c.add(f"clipperd-add-{kind}-{precision}", op="clipperd_add", precision=precision, kind=kind, paths=sub)
        c.add(f"clipperd-add-range-{kind}", op="clipperd_add", precision=8, kind=kind, paths=[[pd(2.0**61 / 2**26 * 1.0000000000000002, 0.0)]])
        c.add(f"clipperd-add-inf-{kind}", op="clipperd_add", precision=None, kind=kind, paths=[[pd(-INF, 0.0)]])
        c.add(f"clipperd-add-empty-{kind}", op="clipperd_add", precision=None, kind=kind, paths=[])
    c.ub("ub-clipperd-add", op="clipperd_add", precision=None, kind="subject", paths=[[pd(NAN, 0.0)]])
    clip = [[pd(5.0, 5.0), pd(15.0, 5.0), pd(15.0, 15.0), pd(5.0, 15.0)]]
    for output in ["paths", "paths_open", "tree", "tree_open"]:
        c.add(f"clipper64-execute-{output}", op="clipper64_execute", precision=None, subjects=[square],
              open_subjects=[path("i64", [(-5, 5), (20, 5)])], clips=[path("i64", [(5, 5), (15, 5), (15, 15), (5, 15)])],
              clip_type="Union", fill_rule="NonZero", output=output)
        c.add(f"clipperd-execute-{output}", op="clipperd_execute", precision=3, subjects=sub, open_subjects=[],
              clips=clip, clip_type="Intersection", fill_rule="EvenOdd", output=output)
    c.add("clipper64-execute-empty", op="clipper64_execute", precision=None, subjects=[], open_subjects=[], clips=[],
          clip_type="NoClip", fill_rule="Positive", output="paths")
    for output in ["paths", "tree_open"]:
        c.add(f"clipper64-execute-no-input-{output}", op="clipper64_execute", precision=None, subjects=None,
              open_subjects=None, clips=None, clip_type="Intersection", fill_rule="EvenOdd", output=output)
        c.add(f"clipperd-execute-no-input-{output}", op="clipperd_execute", precision=None, subjects=None,
              open_subjects=None, clips=None, clip_type="Union", fill_rule="NonZero", output=output)
    c.add("clipperd-execute-range", op="clipperd_execute", precision=None, subjects=[[pd(1e300, 0.0)]], open_subjects=[],
          clips=[], clip_type="Xor", fill_rule="Negative", output="tree")
    c.add("clipperd-execute-precision", op="clipperd_execute", precision=-12, subjects=[], open_subjects=[],
          clips=[], clip_type="Difference", fill_rule="EvenOdd", output="paths")
    # Each ClipperD Add* scales its group (ScalePaths) before AddPaths: a NaN coordinate passes
    # the range check and then converts to int64_t, which is undefined, in every group.
    for i, field in enumerate(["subjects", "open_subjects", "clips"]):
        groups = {f: None for f in ["subjects", "open_subjects", "clips"]}
        groups[field] = [[pd(NAN, 0.0)]]
        c.ub(f"ub-clipperd-execute-{field}", op="clipperd_execute", precision=None, **groups,
             clip_type="Union", fill_rule="NonZero", output=["paths", "tree", "tree_open"][i])
    # A range exception from an earlier group stops before a later group is scaled.
    c.add("clipperd-execute-range-before-nan", op="clipperd_execute", precision=None, subjects=[[pd(1e300, 0.0)]],
          open_subjects=None, clips=[[pd(NAN, 0.0)]], clip_type="Union", fill_rule="NonZero", output="paths")
    # Clipper64 runs the pinned engine only within MIN_COORD..MAX_COORD (a harness domain).
    edge = path("i64", [(-MAX_COORD, -MAX_COORD), (MAX_COORD, -MAX_COORD), (MAX_COORD, MAX_COORD), (-MAX_COORD, MAX_COORD)])
    c.add("clipper64-execute-domain-limit", op="clipper64_execute", precision=None, subjects=[edge], open_subjects=None,
          clips=None, clip_type="Union", fill_rule="NonZero", output="paths")
    for i, (field, point) in enumerate([("subjects", p64(MAX_COORD + 1, 0)), ("open_subjects", p64(0, -MAX_COORD - 1)),
                                        ("clips", p64(I64_MIN, I64_MAX))]):
        groups = {f: [square] for f in ["subjects", "open_subjects", "clips"]}
        groups[field] = [[point, p64(0, 0), p64(1, 1)]]
        c.domain(f"domain-clipper64-execute-{field}", op="clipper64_execute", precision=None, **groups,
                 clip_type="Union", fill_rule="NonZero", output=["paths", "paths_open", "tree"][i])

    def tri(t, s, x=0):
        return path(t, [(x, 0), (x + s, 0), (x + s, s), (x, s)])

    ops64 = [{"op": "add", "node": 0, "path_type": "i64", "path": tri("i64", 100)},
             {"op": "add", "node": 1, "path_type": "i64", "path": tri("i64", 50)},
             {"op": "add", "node": 2, "path_type": "i64", "path": tri("i64", 20)},
             {"op": "add", "node": 3, "path_type": "i64", "path": tri("i64", 5)},
             {"op": "add", "node": 0, "path_type": "i64", "path": tri("i64", 7, 200)},
             {"op": "child", "node": 0, "index": 1, "subscript": True},
             {"op": "child", "node": 1, "index": 0, "subscript": False}]
    c.add("polytree-64", op="polytree", type="i64", root=None, root_type="i64", ops=ops64)
    c.add("polytree-64-clear", op="polytree", type="i64", root=tri("i64", 3), root_type="i64",
          ops=ops64 + [{"op": "clear", "node": 2}, {"op": "add", "node": 2, "path_type": "i64", "path": []},
                       {"op": "clear", "node": 0}, {"op": "add", "node": 0, "path_type": "i64", "path": tri("i64", 1)}])
    c.add("polytree-64-empty", op="polytree", type="i64", root=None, root_type="i64", ops=[])
    third = [pd(0.0, 0.0), pd(0.1, 0.0), pd(0.1, 0.3)]
    opsd = [{"op": "add", "node": 0, "path_type": "i64", "path": tri("i64", 10)},
            {"op": "set_scale", "node": 1, "scale": h(0.1)},
            {"op": "add", "node": 1, "path_type": "i64", "path": tri("i64", 3)},
            {"op": "add", "node": 2, "path_type": "f64", "path": third},
            {"op": "set_scale", "node": 0, "scale": h(0.0)},
            {"op": "add", "node": 0, "path_type": "i64", "path": tri("i64", 3)},
            {"op": "add", "node": 0, "path_type": "i64", "path": []},
            {"op": "add", "node": 0, "path_type": "f64", "path": third},
            {"op": "child", "node": 2, "index": 0, "subscript": True}]
    c.add("polytree-d", op="polytree", type="f64", root=None, root_type="f64", ops=opsd)
    c.add("polytree-d-root64", op="polytree", type="f64", root=tri("i64", 9), root_type="i64", ops=opsd[:3])
    c.add("polytree-d-rootd", op="polytree", type="f64", root=third, root_type="f64",
          ops=[{"op": "set_scale", "node": 0, "scale": h(-2.5)}, {"op": "add", "node": 0, "path_type": "i64", "path": [p64(1, 3, 5)]},
               {"op": "clear", "node": 0}])
    c.ub("ub-polytree-child", op="polytree", type="i64", root=None, root_type="i64",
         ops=[{"op": "child", "node": 0, "index": 0, "subscript": True}])
    c.ub("ub-polytree-destroyed", op="polytree", type="i64", root=None, root_type="i64",
         ops=[{"op": "add", "node": 0, "path_type": "i64", "path": []}, {"op": "clear", "node": 0},
              {"op": "add", "node": 1, "path_type": "i64", "path": []}])
    c.ub("ub-polytree-area", op="polytree", type="i64", root=None, root_type="i64",
         ops=[{"op": "add", "node": 0, "path_type": "i64", "path": [p64(0, I64_MAX), p64(1, 1), p64(2, 0)]}])
    c.add("polypath-base-count", op="polypath_base_count")


def build():
    c = Corpus()
    rng = Rng(0xC11FFE2)
    for section in [core, rects, paths, misc, products, polygons, lines, streams, engine]:
        section(c, rng)
    dump = lambda o: json.dumps(o, ensure_ascii=False, separators=(",", ":"))
    return ("".join(dump(x) + "\n" for x in c.cases), "".join(dump(x) + "\n" for x in c.controls))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if the committed files differ")
    args = parser.parse_args()
    cases, controls = build()
    stale = []
    for target, text in [(CASES, cases), (CONTROLS, controls)]:
        if args.check:
            if not target.exists() or target.read_text(encoding="utf-8") != text:
                stale.append(str(target.relative_to(ROOT)))
        else:
            target.write_text(text, encoding="utf-8", newline="\n")
    if stale:
        print("stale generated files: " + ", ".join(stale), file=sys.stderr)
        return 1
    print(f"{cases.count(chr(10))} cases, {controls.count(chr(10))} controls")
    return 0


if __name__ == "__main__":
    sys.exit(main())
