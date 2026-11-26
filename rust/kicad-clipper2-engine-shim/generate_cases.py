#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the public fixed-seed corpus and refusal controls for the Clipper2 engine source
unit thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001 (clipper.engine.cpp lines
1-2271).

The corpus (port/cases/<unit>.jsonl) holds only inputs inside the reference's defined domain
and the harness's rules: every case yields a C++ result. Requests the programs must refuse
(undefined behaviour in the reference: signed overflow, out-of-range conversions, null
dereferences, endless pointer walks; and harness rules: invalid handles, the engine coordinate
domain, misuse of the edge lists) go to port/oracle/fixtures/clipper2-engine-p001-controls.jsonl,
where both programs must refuse them identically. `--check` verifies both files are current.
"""
import argparse
import json
import math
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001"
CASES = ROOT / "port/cases" / f"{UNIT}.jsonl"
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-engine-p001-controls.jsonl"

I64_MAX = 2**63 - 1
I64_MIN = -2**63
I32_MAX = 2**31 - 1
I32_MIN = -2**31
MAX_COORD = I64_MAX >> 2
U64_MAX = 2**64 - 1
CLIP_TYPES = ["NoClip", "Intersection", "Union", "Difference", "Xor"]
FILL_RULES = ["EvenOdd", "NonZero", "Positive", "Negative"]


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

    def coin(self):
        return self.next() & 1 == 1


def h(x):
    """A double as its 16 hex digit bit pattern (accepts floats or raw bit strings)."""
    if isinstance(x, str):
        return x
    return format(struct.unpack("<Q", struct.pack("<d", x))[0], "016x")


MAX_DBL = h(sys.float_info.max)
NEG_MAX_DBL = h(-sys.float_info.max)
NAN = "7ff8000000000000"


def p(x, y, z=0):
    return [x, y, z]


def path(coords, z0=0):
    return [p(x, y, z0 + i) for i, (x, y) in enumerate(coords)]


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.ids = set()

    def _request(self, name, payload):
        assert name not in self.ids, name
        self.ids.add(name)
        return {"unit": UNIT, "case": name, "input": payload}

    def case(self, name, payload):
        self.cases.append(self._request(name, payload))

    def refuse(self, name, payload, message):
        request = self._request(name, payload)
        self.controls.append({"request": request, "error": {"kind": "invalid_input", "message": message}})

    def ub(self, name, payload):
        self.refuse(name, payload, f"undefined behaviour: {payload['op']}")

    def rule(self, name, payload, what):
        self.refuse(name, payload, f"invalid input: {what}")


def settings(ct="Union", fr="NonZero", polytree=False, preserve=True, z=None, default_z=0):
    return {"s": "settings", "clip_type": ct, "fill_rule": fr, "using_polytree": polytree,
            "preserve_collinear": preserve, "z": z, "default_z": default_z}


def add(paths, kind="subject", is_open=False, target="base"):
    return {"s": "add", "target": target, "path_type": kind, "is_open": is_open, "paths": paths}


def engine(*steps):
    return {"op": "engine", "steps": list(steps)}


def new_active(minima, bound):
    return {"s": "new_active", "minima": minima, "bound": bound}


def set_active(e, wind_cnt=None, wind_cnt2=None, curr_x=None, is_left_bound=None, dx=None):
    return {"s": "set_active", "e": e, "wind_cnt": wind_cnt, "wind_cnt2": wind_cnt2,
            "curr_x": curr_x, "is_left_bound": is_left_bound, "dx": dx}


def step(name, **fields):
    out = {"s": name}
    out.update(fields)
    return out


# --- pure helpers -------------------------------------------------------------------------

def pure(c, rng):
    pairs = [((0, 0), (10, 0)), ((10, 0), (0, 0)), ((5, 5), (5, 5)), ((0, 0), (0, 7)),
             ((0, 0), (3, 7)), ((3, 7), (0, 0)), ((-4, 9), (8, -3)), ((0, 0), (1, 3))]
    for i, (a, b) in enumerate(pairs):
        c.case(f"get_dx/basic/{i}", {"op": "get_dx", "a": p(*a), "b": p(*b, 9)})
    for i in range(40):
        a = (rng.int(-MAX_COORD, MAX_COORD), rng.int(-MAX_COORD, MAX_COORD))
        b = (rng.int(-MAX_COORD, MAX_COORD), rng.int(-MAX_COORD, MAX_COORD))
        c.case(f"get_dx/random/{i}", {"op": "get_dx", "a": p(*a), "b": p(*b)})
    # Full int64 range where the differences still fit.
    c.case("get_dx/extreme/0", {"op": "get_dx", "a": p(I64_MIN, -1), "b": p(-1, I64_MAX - 1)})
    c.case("get_dx/extreme/1", {"op": "get_dx", "a": p(I64_MAX, 0), "b": p(0, 0)})
    c.case("get_dx/extreme/2", {"op": "get_dx", "a": p(I64_MIN, 0), "b": p(I64_MAX, 0)})
    c.ub("get_dx/overflow/dy", {"op": "get_dx", "a": p(0, I64_MIN), "b": p(0, 1)})
    c.ub("get_dx/overflow/dx", {"op": "get_dx", "a": p(I64_MIN, 0), "b": p(1, 5)})

    def top_x(name, bot, top, dx, y):
        c.case(name, {"op": "top_x", "bot": p(*bot), "top": p(*top), "dx": h(dx), "y": y})

    top_x("top_x/at_top", (0, 10), (5, 0), -0.5, 0)
    top_x("top_x/vertical", (3, 10), (3, 0), 0.0, 4)
    top_x("top_x/at_bot", (0, 10), (5, 0), -0.5, 10)
    top_x("top_x/horizontal_at_y", (0, 4), (9, 4), -sys.float_info.max, 4)
    for i, (dx, dy) in enumerate([(0.5, 1), (0.5, 3), (-0.5, 1), (-0.5, 3), (1.5, 1), (2.5, 1),
                                  (0.25, 2), (-2.5, 1), (1.0 / 3.0, 3), (0.1, 7)]):
        top_x(f"top_x/rounding/{i}", (100, 0), (101, 1000), dx, dy)
    for i in range(30):
        bot = (rng.int(-10**12, 10**12), rng.int(-10**12, 10**12))
        top = (rng.int(-10**12, 10**12), rng.int(-10**12, 10**12))
        dx = (top[0] - bot[0]) / (top[1] - bot[1]) if top[1] != bot[1] else 0.0
        top_x(f"top_x/random/{i}", bot, top, dx, rng.int(min(bot[1], top[1]), max(bot[1], top[1])))
    c.ub("top_x/horizontal_inf", {"op": "top_x", "bot": p(0, 4), "top": p(9, 4), "dx": MAX_DBL, "y": 5})
    c.ub("top_x/nan", {"op": "top_x", "bot": p(0, 0), "top": p(9, 4), "dx": NAN, "y": 2})
    c.ub("top_x/dy_overflow", {"op": "top_x", "bot": p(0, I64_MIN), "top": p(9, 4), "dx": h(0.0), "y": 2})
    c.ub("top_x/conversion", {"op": "top_x", "bot": p(0, 0), "top": p(9, 4), "dx": h(1e300), "y": 2})
    c.ub("top_x/sum_overflow", {"op": "top_x", "bot": p(I64_MAX - 1, 0), "top": p(9, 4), "dx": h(5.0), "y": 2})

    tris = [((0, 0), (10, 0), (0, 10)), ((0, 0), (0, 10), (10, 0)), ((1, 1), (2, 2), (3, 3)),
            ((0, 0), (0, 0), (0, 0)), ((-5, 3), (7, -2), (4, 9))]
    for i, t in enumerate(tris):
        c.case(f"area_triangle/basic/{i}", {"op": "area_triangle", "a": p(*t[0]), "b": p(*t[1]), "c": p(*t[2])})
    for i in range(30):
        pts = [p(rng.int(-MAX_COORD, MAX_COORD), rng.int(-MAX_COORD, MAX_COORD)) for _ in range(3)]
        c.case(f"area_triangle/random/{i}", {"op": "area_triangle", "a": pts[0], "b": pts[1], "c": pts[2]})
    c.ub("area_triangle/overflow", {"op": "area_triangle", "a": p(0, I64_MAX), "b": p(1, 1), "c": p(2, 1)})

    close = [((0, 0), (1, 1)), ((0, 0), (2, 0)), ((0, 0), (1, 2)), ((5, 5), (4, 6)), ((0, 0), (-1, -1)),
             ((0, 0), (0, -2)), ((I64_MAX, I64_MIN), (I64_MAX - 1, I64_MIN + 1))]
    for i, (a, b) in enumerate(close):
        c.case(f"pts_really_close/{i}", {"op": "pts_really_close", "a": p(*a), "b": p(*b)})
    # The y difference is not evaluated once the x difference is 2 or more.
    c.case("pts_really_close/short_circuit", {"op": "pts_really_close", "a": p(5, I64_MIN), "b": p(0, 1)})
    c.ub("pts_really_close/x_overflow", {"op": "pts_really_close", "a": p(I64_MIN, 0), "b": p(1, 0)})
    c.ub("pts_really_close/llabs_min", {"op": "pts_really_close", "a": p(-1, 0), "b": p(I64_MAX, 0)})
    c.ub("pts_really_close/y_overflow", {"op": "pts_really_close", "a": p(0, I64_MIN), "b": p(1, 1)})

    for i, v in enumerate([0, 1, -1, 2, -3, I32_MAX, I32_MIN, 12345678]):
        c.case(f"is_odd/{i}", {"op": "is_odd", "value": v})
    for i, (a, b) in enumerate([((0, 5), (1, 5)), ((1, 5), (0, 5)), ((0, 5), (0, 5)), ((0, 6), (0, 5)),
                                ((0, 5), (0, 6)), ((9, 6), (0, 5))]):
        c.case(f"intersect_list_sort/{i}", {"op": "intersect_list_sort", "a": p(*a), "b": p(*b)})
        c.case(f"loc_min_sorter/{i}", {"op": "loc_min_sorter", "a": p(*a), "b": p(*b)})
    for i, y in enumerate([0, -7, I64_MAX, I64_MIN]):
        c.case(f"scanline/{i}", {"op": "scanline", "y": y})


# --- StripeIndex ----------------------------------------------------------------------------

def stripe_request(source, pts, queries, ys):
    return {"op": "stripe_index", "source": source, "path": pts, "queries": queries, "ys": ys}


def stripes(c, rng):
    square = path([(0, 0), (10, 0), (10, 10), (0, 10)])
    queries = [p(5, 5), p(10, 5), p(20, 5), p(0, 0), p(5, 10), p(5, -1), p(-5, 15), p(10, 10)]
    shapes = {
        "empty": [],
        "one": path([(1, 1)]),
        "two": path([(0, 0), (5, 5)]),
        "square": square,
        "duplicates": path([(0, 0), (0, 0), (10, 0), (10, 10), (10, 10), (0, 10), (0, 0)]),
        "flat": path([(0, 3), (5, 3), (9, 3)]),
        "triangle": path([(0, 0), (20, 5), (3, 17)]),
        "concave": path([(0, 0), (20, 0), (20, 20), (10, 5), (0, 20)]),
        "comb": path([(0, 0), (1, 100), (2, 0), (3, 100), (4, 0), (5, 100), (6, 0), (7, 100), (8, 0),
                      (9, 100), (10, 0), (10, -1), (0, -1)]),
        "z_values": [p(0, 0, 7), p(10, 0, -3), p(10, 10, 99), p(0, 10, 1)],
    }
    ys = [-100, -1, 0, 3, 5, 9, 10, 11, 1000]
    for name, pts in shapes.items():
        for source in ["path", "ring"]:
            c.case(f"stripe_index/{source}/{name}", stripe_request(source, pts, queries, ys))
    # Many edges: stripe counts above one and the comb decline.
    big_comb = [(k * 10, 0 if k % 2 == 0 else 1000) for k in range(100)] + [(990, -10), (0, -10)]
    for source in ["path", "ring"]:
        c.case(f"stripe_index/{source}/big_comb", stripe_request(source, path(big_comb), [p(5, 5), p(15, 500)], [0, 500]))
    for n in [9, 16, 30, 64, 100]:
        star = []
        for i in range(n):
            r = 1000 if i % 2 == 0 else 400
            a = 2 * math.pi * i / n
            star.append((round(r * math.cos(a)), round(r * math.sin(a))))
        qs = [p(rng.int(-1100, 1100), rng.int(-1100, 1100)) for _ in range(24)] + [p(*star[0]), p(0, 0)]
        for source in ["path", "ring"]:
            c.case(f"stripe_index/{source}/star{n}", stripe_request(source, path(star), qs,
                                                                       [rng.int(-1200, 1200) for _ in range(6)]))
    for i in range(20):
        n = rng.int(3, 40)
        pts = [p(rng.int(-500, 500), rng.int(-500, 500)) for _ in range(n)]
        qs = [p(rng.int(-600, 600), rng.int(-600, 600)) for _ in range(12)] + [pts[0], pts[n // 2]]
        c.case(f"stripe_index/random/{i}", stripe_request(rng.choice(["path", "ring"]), pts, qs,
                                                          [rng.int(-700, 700) for _ in range(4)]))
    # The whole int64 range where no reference step overflows.
    # y_range + stripe_count_ - 1 must fit: the widest range for four edges is 2^63 - 3.
    wide = [p(-10, -2**62 + 2), p(10, -2**62 + 2), p(10, 2**62 - 1), p(-10, 2**62 - 1)]
    for source in ["path", "ring"]:
        c.case(f"stripe_index/{source}/wide", stripe_request(source, wide, [p(0, 0), p(-10, 2**62 - 1), p(11, -5)],
                                                             [0, -2**62, 2**62 - 1, I64_MIN // 2]))
    c.ub("stripe_index/overflow/wide_x", stripe_request(
        "path", [p(I64_MIN, -2**62), p(I64_MAX, -2**62), p(I64_MAX, 2**62 - 1), p(I64_MIN, 2**62 - 1)], [p(0, 0)], []))
    c.ub("stripe_index/overflow/range", stripe_request("path", [p(0, I64_MIN), p(1, I64_MAX), p(2, 0)], [], []))
    c.ub("stripe_index/overflow/query", stripe_request("path", square, [p(0, I64_MIN)], []))
    c.ub("stripe_index/overflow/ys", stripe_request("path", path([(0, 10), (10, 10), (10, 20)]), [], [I64_MIN]))
    c.ub("stripe_index/overflow/height", stripe_request(
        "path", [p(-10, -2**62), p(10, -2**62), p(10, 2**62 - 1), p(-10, 2**62 - 1)], [], []))
    c.ub("stripe_index/overflow/cross", stripe_request("path", [p(I64_MIN, 0), p(0, 5), p(5, 10)], [p(I64_MAX, 5)], []))
    c.rule("stripe_index/source", stripe_request("other", square, [], []), "source")


# --- engine: paths, minima, scanlines -------------------------------------------------------

def paths_and_minima(c, rng):
    shapes = {
        "square": [path([(0, 0), (10, 0), (10, 10), (0, 10)])],
        "two": [path([(0, 0), (10, 10)])],
        "one": [path([(4, 4)])],
        "dupes": [path([(0, 0), (0, 0), (10, 0), (10, 0), (10, 10), (0, 10), (0, 0)])],
        "closing": [path([(0, 0), (10, 5), (0, 10), (0, 0)])],
        "flat": [path([(0, 5), (5, 5), (9, 5)])],
        "zigzag": [path([(0, 0), (2, 10), (4, 0), (6, 10), (8, 0), (8, 20), (0, 20)])],
        "plateaus": [path([(0, 10), (2, 10), (3, 0), (5, 0), (6, 10), (9, 10), (9, 20), (0, 20)])],
        "empty_paths": [[], path([(0, 0), (5, 5), (0, 9)]), []],
        "none": [],
        "all_empty": [[], []],
        "several": [path([(0, 0), (10, 0), (5, 9)]), path([(20, 20), (30, 25), (22, 30)]),
                    path([(1, 1)]), path([(7, 7), (7, 7)])],
    }
    for name, paths in shapes.items():
        for is_open in [False, True]:
            for kind in ["subject", "clip"]:
                c.case(f"engine/add/{name}/{'open' if is_open else 'closed'}/{kind}",
                       engine(add(paths, kind, is_open)))
            c.case(f"engine/add_reset/{name}/{'open' if is_open else 'closed'}",
                   engine(add(paths, "subject", is_open), add(paths, "clip", not is_open), step("reset")))
    c.case("engine/add_path/single", engine(step("add_path", path_type="clip", is_open=False,
                                                  path=path([(0, 0), (9, 2), (3, 8)])), step("reset")))
    c.case("engine/add_path/empty", engine(step("add_path", path_type="subject", is_open=True, path=[])))
    for i in range(40):
        calls = []
        for _ in range(rng.int(1, 3)):
            paths = []
            for _ in range(rng.int(0, 3)):
                n = rng.int(0, 9)
                paths.append([p(rng.int(-20, 20), rng.int(-20, 20), rng.int(-3, 3)) for _ in range(n)])
            calls.append(add(paths, rng.choice(["subject", "clip"]), rng.coin()))
        calls.append(step("reset"))
        if rng.coin():
            calls.append(step("reset"))
        c.case(f"engine/add_random/{i}", engine(*calls))
    # AddPaths_ and Reset only compare coordinates: the whole int64 range is defined.
    extreme = [path([(I64_MIN, I64_MIN), (I64_MAX, I64_MIN), (I64_MAX, I64_MAX), (I64_MIN, I64_MAX)]),
               path([(0, I64_MAX), (I64_MIN, 0), (5, I64_MIN)])]
    c.case("engine/add_reset/extreme", engine(add(extreme), add(extreme, "clip", True), step("reset"),
                                              step("pop_scanline"), step("pop_local_minima", y=I64_MAX)))
    c.rule("engine/domain/insert_local_minima",
           engine(add(extreme), step("reset"), settings(), step("insert_local_minima", y=I64_MAX)),
           "outside the guarded engine domain: engine")
    c.ub("engine/new_active/get_dx_overflow",
         engine(add([path([(0, I64_MAX), (10, I64_MIN), (20, I64_MAX)])]), step("reset"),
                new_active(0, "left"), new_active(0, "right")))

    # Reuseable data: shared vertices, repeated adds, clears.
    sq = [path([(0, 0), (10, 0), (10, 10), (0, 10)])]
    op = [path([(0, 5), (5, 0), (9, 5)])]
    c.case("engine/reuse/basic", engine(add(sq, target="reuse"), add(op, "clip", True, "reuse"),
                                        step("add_reuseable_data"), step("reset")))
    c.case("engine/reuse/twice", engine(add(sq, target="reuse"), step("add_reuseable_data"),
                                        step("add_reuseable_data"), add(op, "subject", True), step("reset"),
                                        new_active(0, "left"), new_active(1, "right"),
                                        step("maxima_pair", e=0)))
    c.case("engine/reuse/empty", engine(add([], target="reuse"), step("add_reuseable_data")))
    c.case("engine/reuse/clear_unborrowed", engine(add(sq, target="reuse"), step("clear", target="reuse"),
                                                   add(op, "subject", True, "reuse")))
    c.case("engine/reuse/clear_after_base", engine(add(sq, target="reuse"), step("add_reuseable_data"),
                                                   step("clear", target="base"), step("clear", target="reuse")))
    c.case("engine/reuse/clean_up_keeps", engine(add(sq, target="reuse"), step("add_reuseable_data"),
                                                 step("clean_up"), step("reset")))
    c.rule("engine/reuse/clear_borrowed", engine(add(sq, target="reuse"), step("add_reuseable_data"),
                                                 step("clear", target="reuse")), "reuse cleared while borrowed")
    c.rule("engine/target", engine(add(sq, target="other")), "target")

    # Scanlines and minima iteration.
    c.case("engine/scanlines", engine(step("insert_scanline", y=5), step("insert_scanline", y=9),
                                      step("insert_scanline", y=5), step("insert_scanline", y=-3),
                                      step("pop_scanline"), step("pop_scanline"), step("pop_scanline"),
                                      step("pop_scanline")))
    many = [path([(0, 0), (10, 0), (5, 9)]), path([(20, 0), (30, 0), (25, 9)]),
            path([(0, 20), (10, 20), (5, 29)]), path([(40, 0), (50, 3), (45, 9)])]
    c.case("engine/pop_local_minima", engine(add(many), add(many[:2], "clip"), step("reset"),
                                             step("pop_local_minima", y=0), step("pop_local_minima", y=29),
                                             step("pop_local_minima", y=9), step("pop_local_minima", y=9),
                                             step("pop_local_minima", y=9), step("pop_local_minima", y=3),
                                             step("pop_local_minima", y=29)))
    c.case("engine/clear/iterator", engine(add(many), step("clear", target="base"),
                                           step("pop_local_minima", y=0)))
    c.rule("engine/iterator/fresh", engine(step("pop_local_minima", y=0)), "iterator")
    c.rule("engine/iterator/after_add", engine(add(many), step("reset"), add(many),
                                               step("pop_local_minima", y=9)), "iterator")


# --- engine: active edges -------------------------------------------------------------------

POLYS = {
    "diamond": [(20, 0), (40, 20), (20, 40), (0, 20)],
    "kite": [(25, -5), (35, 15), (25, 45), (5, 15)],
    "a": [(0, 0), (40, 0), (40, 40), (0, 40)],
    "b": [(10, -10), (30, -10), (30, 30), (10, 30)],
    "tri": [(0, 0), (50, 10), (20, 40)],
    "v": [(0, 0), (10, 30), (20, 0), (20, 50), (0, 50)],
    "slant": [(5, 0), (25, 0), (35, 40), (-5, 40)],
}


def edge_setup(polys, opens=()):
    """Steps adding closed subject/clip polygons and open subject paths, then Reset."""
    steps = []
    for kind, coords in polys:
        steps.append(add([path(coords)], kind))
    for coords in opens:
        steps.append(add([path(coords)], "subject", True))
    steps.append(step("reset"))
    return steps


def active_edges(c, rng):
    base = edge_setup([("subject", POLYS["a"]), ("clip", POLYS["b"])])
    for ct in CLIP_TYPES:
        for fr in FILL_RULES:
            steps = list(base) + [settings(ct, fr)]
            steps += [new_active(0, "left"), new_active(0, "right"), new_active(1, "left"), new_active(1, "right")]
            steps += [step("insert_left_edge", e=0), step("insert_right_edge", e=0, e2=1),
                      step("wind_closed", e=0), step("insert_left_edge", e=2), step("insert_right_edge", e=2, e2=3),
                      step("wind_closed", e=2), step("contributing", e=0), step("contributing", e=2),
                      step("contributing", e=3)]
            c.case(f"engine/edges/wind/{ct}/{fr}", engine(*steps))
    # Wind counts set explicitly: every IsContributingClosed / IsContributingOpen branch.
    for ct in CLIP_TYPES:
        for fr in FILL_RULES:
            steps = edge_setup([("subject", POLYS["a"]), ("clip", POLYS["b"])], [[(0, 20), (20, 5), (45, 20)]])
            steps += [settings(ct, fr), new_active(0, "left"), new_active(1, "left"), new_active(2, "right")]
            for wc in [-2, -1, 0, 1, 2]:
                for wc2 in [-1, 0, 1]:
                    steps += [set_active(0, wc, wc2), step("contributing", e=0),
                              set_active(1, wc, wc2), step("contributing", e=1),
                              set_active(2, wc, wc2), step("contributing", e=2)]
            c.case(f"engine/edges/contributing/{ct}/{fr}", engine(*steps))
    # SetWindCountForClosedPathEdge over existing edges with chosen counts.
    for i in range(30):
        fr = rng.choice(FILL_RULES)
        steps = edge_setup([("subject", POLYS["a"]), ("subject", POLYS["slant"]), ("clip", POLYS["b"])])
        steps.append(settings(rng.choice(CLIP_TYPES), fr))
        for m in range(3):
            steps += [new_active(m, "left"), new_active(m, "right")]
        order = [0, 2, 4]
        for k, e in enumerate(order):
            steps.append(set_active(e, curr_x=k * 10, wind_cnt=rng.int(-3, 3) or 1,
                                    wind_cnt2=rng.int(-2, 2)))
            steps.append(set_active(e + 1, curr_x=k * 10 + 5, wind_cnt=rng.int(-3, 3) or -1,
                                    wind_cnt2=rng.int(-2, 2)))
        steps += [step("insert_left_edge", e=0), step("insert_right_edge", e=0, e2=1),
                  step("insert_left_edge", e=2), step("insert_right_edge", e=2, e2=3),
                  step("insert_left_edge", e=4), step("insert_right_edge", e=4, e2=5)]
        target = rng.choice([1, 2, 3, 4, 5])
        steps += [step("wind_closed", e=target), step("contributing", e=target)]
        c.case(f"engine/edges/wind_random/{i}", engine(*steps))
    # Open path edges and SetWindCountForOpenPathEdge.
    for fr in FILL_RULES:
        steps = edge_setup([("subject", POLYS["a"]), ("clip", POLYS["b"])], [[(-5, 20), (20, 0), (50, 20)]])
        steps += [settings("Intersection", fr), new_active(0, "left"), new_active(0, "right"),
                  new_active(1, "left"), new_active(1, "right"), new_active(2, "right")]
        steps += [set_active(0, curr_x=0, wind_cnt=1), set_active(1, curr_x=40, wind_cnt=1),
                  set_active(2, curr_x=10, wind_cnt=-1, wind_cnt2=1), set_active(3, curr_x=30, wind_cnt=-1),
                  set_active(4, curr_x=20)]
        steps += [step("insert_left_edge", e=0), step("insert_right_edge", e=0, e2=1),
                  step("insert_left_edge", e=2), step("insert_right_edge", e=2, e2=3),
                  step("insert_left_edge", e=4), step("wind_open", e=4), step("contributing", e=4)]
        c.case(f"engine/edges/wind_open/{fr}", engine(*steps))
    # IsValidAelOrder: curr_x, turning direction, collinear and newly inserted ties.
    order_polys = [("subject", [(0, 0), (20, 20), (0, 40), (-20, 20)]),
                   ("subject", [(0, 0), (10, 20), (0, 30), (-10, 20)]),
                   ("clip", [(0, 0), (20, 20), (20, 40), (-30, 30)]),
                   ("subject", [(0, 10), (20, 30), (0, 50), (-20, 30)])]
    for i in range(24):
        steps = edge_setup(order_polys, [[(0, 0), (30, 30)], [(0, 0), (40, 40), (60, 30)]])
        steps.append(settings())
        count = 0
        for m in range(6):
            for bound in ["left", "right"]:
                steps.append(new_active(m, bound))
                count += 1
        a, b = rng.int(0, count - 1), rng.int(0, count - 1)
        if rng.coin():
            steps.append(set_active(a, curr_x=0, is_left_bound=rng.coin()))
            steps.append(set_active(b, curr_x=0, is_left_bound=rng.coin()))
        steps += [step("valid_ael_order", a=a, b=b), step("valid_ael_order", a=b, b=a)]
        c.case(f"engine/edges/valid_ael_order/{i}", engine(*steps))
    # InsertLeftEdge into a populated AEL (with a join-free right neighbour).
    for i in range(16):
        steps = edge_setup([("subject", POLYS["a"]), ("subject", POLYS["tri"]), ("clip", POLYS["b"]),
                            ("clip", POLYS["v"])])
        steps.append(settings())
        handles = []
        for m in range(5):
            for bound in ["left", "right"]:
                steps.append(new_active(m, bound))
                handles.append(len(handles))
        xs = list(range(len(handles)))
        for e in handles:
            steps.append(set_active(e, curr_x=rng.choice([0, 0, 10, 20, 40]) if rng.coin() else xs[e] * 3))
        picked = []
        for e in handles:
            if rng.coin() or not picked:
                steps.append(step("insert_left_edge", e=e))
                picked.append(e)
        c.case(f"engine/edges/insert_left/{i}", engine(*steps))
    c.rule("engine/edges/insert_left_twice",
           engine(*edge_setup([("subject", POLYS["a"])]), new_active(0, "left"),
                  step("insert_left_edge", e=0), step("insert_left_edge", e=0)), "list")
    c.rule("engine/edges/bad_handle", engine(*edge_setup([("subject", POLYS["a"])]), step("contributing", e=3)),
           "edge")
    c.rule("engine/edges/wind_outside_ael",
           engine(*edge_setup([("subject", POLYS["a"])]), new_active(0, "left"), step("wind_closed", e=0)), "list")
    c.rule("engine/edges/wind_domain",
           engine(*edge_setup([("subject", POLYS["a"])]), new_active(0, "left"), set_active(0, wind_cnt=I32_MAX)),
           "wind")
    c.rule("engine/edges/minima_index", engine(*edge_setup([("subject", POLYS["a"])]), new_active(4, "left")),
           "minima")
    # Vertex navigation, maxima and horizontal trimming.
    flats = [("subject", [(0, 0), (10, 0), (20, 0), (20, 10), (10, 10), (15, 20), (0, 20)]),
             ("subject", [(30, 0), (40, 10), (50, 10), (60, 10), (60, 20), (30, 20)]),
             ("clip", [(70, 0), (80, 0), (75, 0), (90, 10), (70, 10)])]
    for preserve in [True, False]:
        steps = edge_setup(flats, [[(0, 30), (10, 40), (20, 40), (30, 40)], [(0, 50), (10, 50), (20, 50)]])
        steps.append(settings(preserve=preserve))
        n = 0
        for m in range(5):
            for bound in ["left", "right"]:
                steps.append(new_active(m, bound))
                n += 1
        for e in range(n):
            steps += [step("next_vertex", e=e), step("prev_prev_vertex", e=e), step("curr_y_maxima", e=e, open=True),
                      step("maxima_pair", e=e), step("matching_loc_min", e=e), step("prev_hot_edge", e=e)]
        for e in range(n):
            steps.append(step("trim_horz", e=e, preserve_collinear=preserve))
        c.case(f"engine/edges/navigation/{preserve}", engine(*steps))
    c.case("engine/edges/curr_y_maxima_closed",
           engine(*edge_setup(flats), settings(), new_active(0, "left"), new_active(0, "right"),
                  new_active(1, "left"), new_active(1, "right"),
                  *[step("curr_y_maxima", e=e, open=False) for e in range(4)]))
    # An open path flat from end to end: the closed-path variant never stops.
    c.ub("engine/edges/curr_y_maxima_endless",
         engine(*edge_setup([], [[(0, 5), (5, 5), (9, 5)]]), new_active(0, "right"),
                step("curr_y_maxima", e=0, open=False)))
    # Edges entering the AEL in a sequence: maxima pairs, matching minima, adjacency, top x.
    steps = edge_setup([("subject", POLYS["diamond"]), ("clip", POLYS["kite"])])
    steps += [settings(), new_active(0, "left"), new_active(0, "right"), new_active(1, "left"), new_active(1, "right"),
              set_active(0, curr_x=0), set_active(1, curr_x=40), set_active(2, curr_x=10), set_active(3, curr_x=30),
              step("insert_left_edge", e=0), step("insert_right_edge", e=0, e2=1), step("insert_left_edge", e=2),
              step("insert_right_edge", e=2, e2=3)]
    for e in range(4):
        steps += [step("maxima_pair", e=e), step("matching_loc_min", e=e),
                  step("top_x", e=e, y=0), step("top_x", e=e, y=20), step("top_x", e=e, y=40)]
    for a in range(4):
        for b in range(4):
            if a != b:
                steps.append(step("edges_adjacent", e1=a, e2=b))
    c.case("engine/edges/ael_queries", engine(*steps))
    c.ub("engine/edges/top_x_horizontal",
         engine(*edge_setup([("subject", [(0, 9), (10, 9), (5, 0)])]), new_active(0, "left"),
                new_active(0, "right"), step("top_x", e=0, y=5), step("top_x", e=1, y=5)))
    # PushHorz / PopHorz / ExtractFromSEL / Insert1Before2InSEL.
    steps = edge_setup([("subject", POLYS["a"]), ("clip", POLYS["b"])])
    steps += [new_active(0, "left"), new_active(0, "right"), new_active(1, "left"), new_active(1, "right"),
              step("push_horz", e=0), step("insert_before_sel", e1=1, e2=0), step("push_horz", e=1),
              step("insert_before_sel", e1=2, e2=1), step("push_horz", e=2), step("extract_from_sel", e=0),
              step("push_horz", e=3), step("extract_from_sel", e=1), step("pop_horz"), step("pop_horz"),
              step("pop_horz"), step("pop_horz")]
    c.case("engine/sel/sequence", engine(*steps))
    c.ub("engine/sel/extract_unlinked",
         engine(*edge_setup([("subject", POLYS["a"])]), new_active(0, "left"), new_active(0, "right"),
                step("push_horz", e=0), step("push_horz", e=1), step("extract_from_sel", e=0)))
    c.rule("engine/sel/push_twice",
           engine(*edge_setup([("subject", POLYS["a"])]), new_active(0, "left"), step("push_horz", e=0),
                  step("push_horz", e=0)), "list")

    # Cleanup is defined with a populated SEL, but the adapter cannot observe its
    # now-deleted links. Keep this harness limitation separate from source UB.
    cleanup_setup = [add([path([(0, 10), (10, 10), (10, 0)])], is_open=True), step("reset"),
                     new_active(0, "right"), step("insert_left_edge", e=0), step("push_horz", e=0)]
    c.rule("engine/cleanup/deleted_sel_observation", engine(*cleanup_setup, step("clean_up")),
           "deleted list observation")
    c.rule("engine/clear/deleted_sel_observation", engine(*cleanup_setup, step("clear", target="base")),
           "deleted list observation")
    c.case("engine/cleanup/drained_sel", engine(*cleanup_setup, step("pop_horz"),
                                                step("clean_up"), step("reset")))


# --- engine: output records from edges -------------------------------------------------------

def outrecs_from_edges(c, rng):
    sub = [("subject", POLYS["a"]), ("clip", POLYS["b"]), ("subject", POLYS["tri"])]
    opens = [[(-5, 20), (20, 0), (50, 20)], [(0, 35), (15, 45), (30, 35)]]
    # AddLocalMinPoly: open/closed, with and without a previous hot edge, polytree owners.
    for polytree in [False, True]:
        for is_new in [None, False, True]:
            steps = edge_setup(sub, opens)
            steps.append(settings(polytree=polytree))
            for m in range(4):
                steps += [new_active(m, "left"), new_active(m, "right")]
            steps += [set_active(e, curr_x=e * 5) for e in range(8)]
            steps += [step("insert_left_edge", e=0), step("insert_right_edge", e=0, e2=1),
                      step("insert_left_edge", e=2), step("insert_right_edge", e=2, e2=3),
                      step("local_min_poly", e1=0, e2=1, pt=p(0, 40), is_new=is_new),
                      step("local_min_poly", e1=2, e2=3, pt=p(10, 30), is_new=is_new),
                      step("insert_left_edge", e=4), step("insert_right_edge", e=4, e2=5),
                      step("local_min_poly", e1=4, e2=5, pt=p(5, 25), is_new=is_new),
                      step("prev_hot_edge", e=4)]
            steps += [step("insert_left_edge", e=6), step("local_min_poly", e1=6, e2=7, pt=p(20, 0), is_new=is_new)]
            c.case(f"engine/outrec/local_min/{polytree}/{is_new}", engine(*steps))
    # AddOutPt to the front and the back, repeated points, StartOpenPath.
    steps = edge_setup(sub, opens)
    steps += [settings(), new_active(0, "left"), new_active(0, "right"), new_active(3, "right"),
              step("local_min_poly", e1=0, e2=1, pt=p(0, 40), is_new=True),
              step("add_out_pt", e=0, pt=p(0, 30)), step("add_out_pt", e=0, pt=p(0, 30)),
              step("add_out_pt", e=1, pt=p(40, 30)), step("add_out_pt", e=1, pt=p(40, 30)),
              step("add_out_pt", e=1, pt=p(40, 20)), step("add_out_pt", e=0, pt=p(0, 20)),
              step("start_open_path", e=2, pt=p(20, 0)), step("add_out_pt", e=2, pt=p(30, 10)),
              step("swap_outrecs", e1=0, e2=1), step("add_out_pt", e=0, pt=p(1, 1)),
              step("swap_outrecs", e1=0, e2=2), step("swap_outrecs", e1=0, e2=2),
              step("uncouple", e=1), step("uncouple", e=1)]
    c.case("engine/outrec/add_out_pt", engine(*steps))
    # AddLocalMaxPoly: same record (closing), different records (joins), side mismatches.
    for polytree in [False, True]:
        steps = edge_setup(sub, opens)
        steps.append(settings(polytree=polytree))
        for m in range(5):
            steps += [new_active(m, "left"), new_active(m, "right")]
        steps += [set_active(e, curr_x=e * 4) for e in range(10)]
        for a, b in [(0, 1), (2, 3), (4, 5)]:
            steps += [step("insert_left_edge", e=a), step("insert_right_edge", e=a, e2=b)]
        steps += [step("local_min_poly", e1=0, e2=1, pt=p(0, 40), is_new=True),
                  step("local_min_poly", e1=2, e2=3, pt=p(10, 30), is_new=True),
                  step("local_min_poly", e1=4, e2=5, pt=p(20, 40), is_new=False),
                  step("add_out_pt", e=1, pt=p(40, 0)), step("add_out_pt", e=2, pt=p(10, 0)),
                  step("local_max_poly", e1=1, e2=2, pt=p(25, 0)),
                  step("local_max_poly", e1=4, e2=5, pt=p(15, 0)),
                  step("local_max_poly", e1=0, e2=3, pt=p(5, -5)),
                  step("real_outrec", o=0), step("real_outrec", o=1)]
        c.case(f"engine/outrec/local_max/{polytree}", engine(*steps))
    # Both front edges of closed records: AddLocalMaxPoly fails (succeeded_ = false).
    steps = edge_setup(sub)
    steps += [settings(), new_active(0, "left"), new_active(0, "right"), new_active(1, "left"), new_active(1, "right"),
              step("local_min_poly", e1=0, e2=1, pt=p(0, 40), is_new=True),
              step("local_min_poly", e1=2, e2=3, pt=p(10, 30), is_new=True),
              step("local_max_poly", e1=0, e2=2, pt=p(5, 0))]
    c.case("engine/outrec/local_max/fails", engine(*steps))
    # Open ends: the side swap before the join.
    steps = edge_setup([], opens)
    steps += [settings(), new_active(0, "right"), new_active(1, "right"), new_active(2, "left"),
              step("start_open_path", e=0, pt=p(20, 0)), step("start_open_path", e=1, pt=p(15, 45)),
              step("add_out_pt", e=0, pt=p(30, 10)), step("add_out_pt", e=1, pt=p(22, 40)),
              step("join_outrec_paths", e1=0, e2=1)]
    c.case("engine/outrec/join_open", engine(*steps))
    for front in [True, False]:
        steps = edge_setup(sub)
        steps += [settings(), new_active(0, "left"), new_active(0, "right"), new_active(1, "left"),
                  new_active(1, "right"),
                  step("local_min_poly", e1=0, e2=1, pt=p(0, 40), is_new=front),
                  step("local_min_poly", e1=2, e2=3, pt=p(10, 30), is_new=not front),
                  step("add_out_pt", e=0, pt=p(0, 0)), step("add_out_pt", e=3, pt=p(30, 0)),
                  step("join_outrec_paths", e1=0, e2=3)]
        c.case(f"engine/outrec/join_closed/{front}", engine(*steps))
    c.ub("engine/outrec/add_out_pt_cold",
         engine(*edge_setup(sub), new_active(0, "left"), step("add_out_pt", e=0, pt=p(0, 0))))
    c.ub("engine/outrec/swap_cold",
         engine(*edge_setup(sub), new_active(0, "left"), new_active(0, "right"), step("swap_outrecs", e1=0, e2=1)))
    c.ub("engine/outrec/uncouple_open",
         engine(*edge_setup([], opens), new_active(0, "right"), step("start_open_path", e=0, pt=p(0, 0)),
                step("uncouple", e=0)))
    c.ub("engine/outrec/local_max_cold",
         engine(*edge_setup(sub), new_active(0, "left"), new_active(0, "right"),
                step("local_max_poly", e1=0, e2=1, pt=p(0, 0))))
    c.rule("engine/outrec/join_same",
           engine(*edge_setup(sub), new_active(0, "left"), new_active(0, "right"),
                  step("local_min_poly", e1=0, e2=1, pt=p(0, 0), is_new=True),
                  step("join_outrec_paths", e1=0, e2=1)), "owner")
    c.rule("engine/outrec/intersect_same",
           engine(*edge_setup(sub), new_active(0, "left"), step("intersect_edges", e1=0, e2=0, pt=p(0, 0))), "edge")


def intersections(c, rng):
    """IntersectEdges over fixed-seed states: polytypes, hot edges, wind counts, open paths,
    clip types, fill rules and the Z callback."""
    polys = [("subject", POLYS["a"]), ("clip", POLYS["b"]), ("subject", POLYS["slant"]), ("clip", POLYS["tri"])]
    opens = [[(-5, 20), (20, 0), (50, 20)], [(0, 35), (15, 45), (30, 35)]]
    for i in range(160):
        with_open = rng.int(0, 3) == 0
        steps = edge_setup(polys, opens if with_open else ())
        ct = rng.choice(CLIP_TYPES[1:] if rng.int(0, 7) else CLIP_TYPES)
        fr = rng.choice(FILL_RULES)
        z = rng.choice([None, None, 1000, -50])
        steps.append(settings(ct, fr, rng.coin(), rng.coin(), z, rng.choice([0, 7, -9])))
        minima = 6 if with_open else 4
        edges = []
        for m in range(minima):
            for bound in ["left", "right"]:
                if m >= 4 and bound == "left":
                    continue
                steps.append(new_active(m, bound))
                edges.append(len(edges))
        # An AEL in a chosen order, wind counts set, some edges hot.
        for e in edges:
            steps.append(set_active(e, curr_x=e * 3, wind_cnt=rng.choice([-2, -1, 0, 1, 1, 2]),
                                    wind_cnt2=rng.choice([-1, 0, 0, 1]), is_left_bound=e % 2 == 0))
        for e in edges:
            steps.append(step("insert_left_edge", e=e))
        pairs = [(edges[k], edges[k + 1]) for k in range(0, len(edges) - 1, 2)]
        for a, b in pairs:
            if rng.int(0, 2) == 0:
                steps.append(step("local_min_poly", e1=a, e2=b, pt=p(rng.int(0, 40), rng.int(0, 40)),
                                  is_new=rng.coin()))
        if with_open and rng.coin():
            steps.append(step("start_open_path", e=edges[-1], pt=p(20, 0)))
        for _ in range(rng.int(1, 4)):
            a, b = rng.choice(edges), rng.choice(edges)
            if a == b:
                continue
            steps.append(step("intersect_edges", e1=a, e2=b, pt=p(rng.int(0, 40), rng.int(0, 40))))
        c.case(f"engine/intersect/{i}", engine(*steps))
    # The open-path branch where the closed edge matches the open edge's own minimum.
    steps = edge_setup([("clip", POLYS["b"])], [[(0, 30), (20, 0), (40, 30)]])
    steps += [settings("Intersection", "NonZero", z=10), new_active(0, "left"), new_active(0, "right"),
              new_active(1, "left"), new_active(1, "right"),
              set_active(0, curr_x=10, wind_cnt=1), set_active(1, curr_x=30, wind_cnt=1),
              set_active(2, curr_x=20), set_active(3, curr_x=20),
              step("insert_left_edge", e=0), step("insert_left_edge", e=2), step("insert_right_edge", e=2, e2=3),
              step("insert_right_edge", e=0, e2=1), step("local_min_poly", e1=0, e2=1, pt=p(20, 30), is_new=True),
              step("start_open_path", e=3, pt=p(20, 0)), step("intersect_edges", e1=2, e2=0, pt=p(20, 0)),
              step("intersect_edges", e1=3, e2=1, pt=p(25, 5)), step("intersect_edges", e1=2, e2=1, pt=p(20, 0)),
              step("set_z", e1=0, e2=2, pt=p(10, -10)), step("set_z", e1=2, e2=0, pt=p(20, 0, 5)),
              step("set_z", e1=0, e2=2, pt=p(5, 5))]
    c.case("engine/intersect/open_minimum", engine(*steps))


# --- engine: output rings -------------------------------------------------------------------

RINGS = {
    "square": [(0, 0), (10, 0), (10, 10), (0, 10)],
    "collinear": [(0, 0), (5, 0), (10, 0), (10, 10), (5, 10), (0, 10), (0, 5)],
    "spike": [(0, 0), (10, 0), (20, 0), (10, 0), (10, 10), (0, 10)],
    "duplicate": [(0, 0), (10, 0), (10, 0), (10, 10), (0, 10)],
    "tiny": [(0, 0), (1, 0), (1, 1)],
    "triangle": [(0, 0), (30, 0), (15, 20)],
    "bowtie": [(0, 0), (20, 20), (20, 0), (0, 20)],
    "double_bow": [(0, 0), (20, 20), (20, 0), (40, 20), (40, 0), (0, 20)],
    "figure8": [(0, 0), (30, 0), (30, 30), (10, 10), (20, 10), (0, 30)],
    "loop": [(0, 0), (40, 0), (40, 40), (10, 40), (10, 10), (30, 10), (30, 30), (0, 30)],
    "two": [(0, 0), (9, 9)],
    "one": [(4, 4)],
    "flat": [(0, 5), (5, 5), (9, 5), (3, 5)],
    "concave": [(0, 0), (20, 0), (20, 20), (10, 5), (0, 20)],
}


def rings(c, rng):
    for name, coords in RINGS.items():
        for preserve in [True, False]:
            for polytree in [False, True]:
                steps = [settings(polytree=polytree, preserve=preserve, z=None if polytree else 500),
                         step("new_outrec", path=path(coords), owner=None, is_open=False),
                         step("area", o=0) if len(coords) else step("invalid_path", o=0),
                         step("point_count", o=0), step("valid_closed_path", o=0), step("invalid_path", o=0),
                         step("clean_path", o=0), step("clean_collinear", o=0)]
                c.case(f"engine/ring/clean/{name}/{preserve}/{polytree}", engine(*steps))
        if len(coords) >= 3:
            for at in range(len(coords)):
                c.case(f"engine/ring/split/{name}/{at}",
                       engine(settings(polytree=at % 2 == 1, z=None if at % 3 else 70),
                              step("new_outrec", path=path(coords), owner=None, is_open=False),
                              step("do_split_op", o=0, at=at)))
            c.case(f"engine/ring/fix/{name}",
                   engine(settings(polytree=True), step("new_outrec", path=path(coords), owner=None, is_open=False),
                          step("fix_self_intersects", o=0), step("valid_closed_path", o=0)))
        c.case(f"engine/ring/ops/{name}",
               engine(step("new_outrec", path=path(coords), owner=None, is_open=False),
                      step("reverse_out_pts", o=0), step("swap_sides", o=0), step("duplicate_op", op=[0, 0], after=True),
                      step("duplicate_op", op=[0, 0], after=False), step("update_outrec_owner", o=0),
                      step("point_count", o=0)))
    c.case("engine/ring/open_and_owned",
           engine(step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=True),
                  step("new_outrec", path=[], owner=0, is_open=False),
                  step("new_outrec", path=[], owner=1, is_open=False),
                  step("clean_collinear", o=0), step("clean_collinear", o=2), step("clean_collinear", o=None),
                  step("real_outrec", o=2), step("real_outrec", o=None), step("valid_owner", o=0, test=2),
                  step("valid_owner", o=2, test=0), step("valid_owner", o=None, test=None)))
    # Owners: SetOwner's cycle prevention and GetRealOutRec through emptied records.
    for i in range(12):
        n = 6
        steps = []
        for k in range(n):
            owner = rng.int(0, k - 1) if k and rng.coin() else None
            coords = RINGS["square"] if rng.coin() else []
            steps.append(step("new_outrec", path=path([(x + k, y) for x, y in coords]), owner=owner, is_open=False))
        for _ in range(6):
            a, b = rng.int(0, n - 1), rng.int(0, n - 1)
            if a != b:
                steps.append(step("set_owner", o=a, new_owner=b))
        for k in range(n):
            steps += [step("real_outrec", o=k), step("valid_owner", o=k, test=rng.int(0, n - 1))]
        c.case(f"engine/ring/owners/{i}", engine(*steps))
    # Disposal.
    c.case("engine/ring/dispose",
           engine(step("new_outrec", path=path(RINGS["collinear"]), owner=None, is_open=False),
                  step("dispose_out_pt", op=[0, 3]), step("dispose_out_pt", op=[0, 1]),
                  step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=False),
                  step("dispose_out_pts", o=1), step("clean_up")))
    # Point in polygon on rings, cleaned paths and containment (brute force and indexed).
    probe = path(RINGS["loop"])
    pts = [p(5, 5), p(20, 20), p(35, 35), p(0, 0), p(10, 25), p(40, 20), p(50, 50), p(20, 0), p(25, 40), p(-1, 30)]
    c.case("engine/ring/point_in_op_polygon",
           engine(step("new_outrec", path=probe, owner=None, is_open=False),
                  step("new_outrec", path=path(RINGS["flat"]), owner=None, is_open=False),
                  step("new_outrec", path=path(RINGS["two"]), owner=None, is_open=False),
                  *[step("point_in_op_polygon", pt=q, o=o) for o in range(3) for q in pts]))
    inner = [(5, 5), (8, 5), (8, 8), (5, 8)]
    big = [(x * 3, y * 3) for x, y in [(0, 0), (10, 0), (10, 10), (0, 10)]]
    for threshold in [0, 3, 64]:
        c.case(f"engine/ring/contains/{threshold}",
               engine(step("new_outrec", path=path(big), owner=None, is_open=False),
                      step("new_outrec", path=path(inner), owner=None, is_open=False),
                      step("new_outrec", path=path([(100, 100), (110, 100), (110, 110)]), owner=None, is_open=False),
                      step("set_path", o=0, path=path(big)), step("set_path", o=2, path=path(big)),
                      step("contains", o1=1, o2=0), step("contains", o1=0, o2=1), step("contains", o1=2, o2=0),
                      step("contains_indexed", o1=1, o2=0, threshold=threshold),
                      step("contains_indexed", o1=2, o2=0, threshold=threshold),
                      step("contains_indexed", o1=1, o2=2, threshold=threshold)))
    # Many-edge owners for the stripe index (cached on the owner record), and a comb it declines.
    star = []
    for k in range(80):
        r = 1000 if k % 2 == 0 else 700
        a = 2 * math.pi * k / 80
        star.append((round(r * math.cos(a)), round(r * math.sin(a))))
    # 100 teeth: 10 stripes, every tooth edge spans all of them, so the index declines.
    comb = []
    for k in range(100):
        comb.append((k * 10, 0 if k % 2 == 0 else 1000))
    comb += [(990, -10), (0, -10)]
    for name, owner in [("star", star), ("comb", comb)]:
        c.case(f"engine/ring/indexed/{name}",
               engine(step("new_outrec", path=path(owner), owner=None, is_open=False),
                      step("new_outrec", path=path([(-20, -20), (20, -20), (20, 20), (-20, 20)]), owner=None,
                           is_open=False),
                      step("new_outrec", path=path([(2000, 0), (2100, 0), (2100, 100)]), owner=None, is_open=False),
                      step("set_path", o=0, path=path(owner)),
                      step("contains_indexed", o1=1, o2=0, threshold=10),
                      step("contains_indexed", o1=2, o2=0, threshold=10),
                      step("contains_indexed", o1=1, o2=0, threshold=10)))
    # Ambiguous containment falls back to the cleaned paths (clipper.h, deferred).
    c.case("engine/ring/contains_ambiguous",
           engine(step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=False),
                  step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=False),
                  step("contains", o1=0, o2=1)))
    # HorzSegSorter.
    c.case("engine/ring/horz_seg_sorter",
           engine(step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=False),
                  step("horz_seg_sorter", a={"left": [0, 0], "right": [0, 1]}, b={"left": [0, 1], "right": [0, 2]}),
                  step("horz_seg_sorter", a={"left": [0, 2], "right": [0, 1]}, b={"left": [0, 1], "right": [0, 2]}),
                  step("horz_seg_sorter", a={"left": None, "right": None}, b={"left": [0, 1], "right": [0, 2]}),
                  step("horz_seg_sorter", a={"left": [0, 0], "right": [0, 3]}, b={"left": None, "right": None}),
                  step("horz_seg_sorter", a={"left": None, "right": None}, b={"left": None, "right": None})))
    c.ub("engine/ring/horz_seg_sorter_null_left",
         engine(step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=False),
                step("horz_seg_sorter", a={"left": None, "right": [0, 1]}, b={"left": [0, 1], "right": [0, 2]})))
    # Random self-intersecting rings through CleanCollinear with and without the polytree.
    for i in range(60):
        n = rng.int(3, 12)
        coords = [(rng.int(0, 30), rng.int(0, 30)) for _ in range(n)]
        c.case(f"engine/ring/random/{i}",
               engine(settings(polytree=rng.coin(), preserve=rng.coin(), z=rng.choice([None, 300])),
                      step("new_outrec", path=path(coords), owner=None, is_open=False),
                      step("clean_collinear", o=0)))
    c.rule("engine/ring/split_two", engine(step("new_outrec", path=path(RINGS["two"]), owner=None, is_open=False),
                                           step("do_split_op", o=0, at=0)), "ring")
    c.rule("engine/ring/dispose_head", engine(step("new_outrec", path=path(RINGS["square"]), owner=None, is_open=False),
                                              step("dispose_out_pt", op=[0, 0])), "outpt")
    c.rule("engine/ring/set_owner_self", engine(step("new_outrec", path=[], owner=None, is_open=False),
                                                step("set_owner", o=0, new_owner=0)), "owner")
    c.rule("engine/ring/empty", engine(step("new_outrec", path=[], owner=None, is_open=False), step("area", o=0)),
           "ring")
    c.rule("engine/ring/domain",
           engine(step("new_outrec", path=path([(0, 0), (I64_MAX, 0), (0, 9)]), owner=None, is_open=False),
                  step("clean_collinear", o=0)), "outside the guarded engine domain: engine")
    c.rule("engine/ring/bad_outrec", engine(step("area", o=0)), "outrec")
    c.rule("engine/step", engine(step("frobnicate")), "step")


# --- engine: the sweep's own entry points ----------------------------------------------------

def sweeps(c, rng):
    """InsertLocalMinimaIntoAEL and UpdateEdgeIntoAEL from added paths. Open paths alone run
    entirely in this unit; closed ones reach CheckJoinLeft/CheckJoinRight/SwapPositionsInAEL
    (lines 2272 onward) and are deferred in the port."""
    opens = [[(0, 20), (10, 0), (20, 20)], [(5, 30), (25, 10), (45, 30), (60, 0)], [(-10, 0), (40, 0)],
             [(0, 50), (0, 40), (10, 40), (10, 50)]]
    for ct in CLIP_TYPES:
        for fr in FILL_RULES:
            steps = [add([path(o) for o in opens], "subject", True), step("reset"), settings(ct, fr, z=40)]
            for y in [40, 30, 20, 0, -5]:
                steps.append(step("insert_local_minima", y=y))
            steps += [step("pop_scanline"), step("pop_scanline"), step("pop_horz"), step("pop_horz")]
            c.case(f"engine/sweep/open/{ct}/{fr}", engine(*steps))
    for i in range(20):
        paths = [[p(rng.int(-30, 30), rng.int(-30, 30)) for _ in range(rng.int(2, 6))] for _ in range(rng.int(1, 4))]
        steps = [add(paths, "subject", True), step("reset"), settings(rng.choice(CLIP_TYPES), rng.choice(FILL_RULES))]
        for y in sorted({pt[1] for pth in paths for pt in pth}, reverse=True):
            steps.append(step("insert_local_minima", y=y))
        c.case(f"engine/sweep/open_random/{i}", engine(*steps))
    for i, (polys, ct) in enumerate([([("subject", POLYS["a"])], "Union"),
                                     ([("subject", POLYS["a"]), ("clip", POLYS["b"])], "Intersection"),
                                     ([("subject", POLYS["v"])], "NoClip"),
                                     ([("subject", POLYS["tri"]), ("clip", POLYS["slant"])], "Xor")]):
        steps = edge_setup(polys) + [settings(ct)]
        steps += [step("pop_scanline"), step("insert_local_minima", y=40), step("insert_local_minima", y=0)]
        c.case(f"engine/sweep/closed/{i}", engine(*steps))
    # UpdateEdgeIntoAEL: horizontal closed edges are trimmed here; others reach CheckJoinLeft.
    flats = [("subject", [(0, 0), (10, 0), (20, 0), (20, 10), (10, 10), (15, 20), (0, 20)])]
    for preserve in [True, False]:
        steps = edge_setup(flats) + [settings(preserve=preserve), new_active(0, "left"), new_active(0, "right"),
                                     step("next_vertex", e=0), step("next_vertex", e=1),
                                     step("update_edge", e=1), step("update_edge", e=1)]
        c.case(f"engine/sweep/update_edge/{preserve}", engine(*steps))
    c.case("engine/sweep/update_edge_join",
           engine(*edge_setup(flats), settings(), new_active(0, "left"), step("update_edge", e=0)))
    c.case("engine/sweep/update_edge_open",
           engine(*edge_setup([], [[(0, 10), (5, 0), (10, 0), (15, 0), (20, 10)]]), settings(), new_active(0, "right"),
                  step("update_edge", e=0), step("update_edge", e=0)))


def build():
    c = Corpus()
    pure(c, Rng(0x5EED0001))
    stripes(c, Rng(0x5EED0002))
    paths_and_minima(c, Rng(0x5EED0003))
    active_edges(c, Rng(0x5EED0004))
    outrecs_from_edges(c, Rng(0x5EED0005))
    intersections(c, Rng(0x5EED0006))
    rings(c, Rng(0x5EED0007))
    sweeps(c, Rng(0x5EED0008))
    dump = lambda x: json.dumps(x, separators=(",", ":"), ensure_ascii=False)
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
                stale.append(str(target))
        else:
            target.write_text(text, encoding="utf-8", newline="\n")
    if stale:
        sys.exit("stale generated files: " + ", ".join(stale))
    print(f"{cases.count(chr(10))} cases, {controls.count(chr(10))} controls")


if __name__ == "__main__":
    main()
