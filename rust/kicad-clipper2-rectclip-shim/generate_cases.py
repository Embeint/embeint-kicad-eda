#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the public fixed-seed corpus and refusal controls for the Clipper2 rectangle
clipping source unit (clipper.rectclip.cpp).

The corpus (port/cases/<unit>.jsonl) holds only inputs on which the pinned source is defined;
every case must yield a C++ result. Inputs on which the source executes undefined behaviour
(signed overflow in its products or in Rect64::MidPoint, out-of-range double to int64_t
conversion, out-of-range std::vector subscripts) go to
port/oracle/fixtures/clipper2-rectclip-controls.jsonl, where both programs must refuse them
identically. `--check` verifies both files are current. All data is generated here.
"""
import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003"
CASES = ROOT / "port/cases" / f"{UNIT}.jsonl"
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-rectclip-controls.jsonl"

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


def p(x, y, z=0):
    return [x, y, z]


def path(points, z=0):
    return [p(x, y, z) for x, y in points]


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.names = set()

    def _id(self, name):
        if name in self.names:
            raise ValueError("duplicate case " + name)
        self.names.add(name)
        return name

    def add(self, name, **inp):
        self.cases.append({"unit": UNIT, "case": self._id(name), "input": inp})

    def ub(self, name, **inp):
        self.controls.append({"request": {"unit": UNIT, "case": self._id(name), "input": inp},
                              "error": {"kind": "invalid_input",
                                        "message": "undefined behaviour: " + inp["op"]}})


RECT = [0, 0, 10, 10]
SQUARE = [(0, 0), (10, 0), (10, 10), (0, 10)]


def helpers(c, rng):
    rect = RECT
    # GetLocation over a grid that includes every edge, corner and the outside bands.
    for x in range(-2, 13, 2):
        for y in range(-2, 13, 2):
            for loc in (0, 4):
                c.add(f"get-location-{x}-{y}-{loc}", op="get_location", rect=rect, pt=p(x, y, 7), loc=loc)
    for x in (-1, 0, 1, 9, 10, 11):
        for y in (-1, 0, 1, 9, 10, 11):
            c.add(f"get-location-edge-{x}-{y}", op="get_location", rect=rect, pt=p(x, y), loc=2)
    big = [-MAX_COORD, -MAX_COORD, MAX_COORD, MAX_COORD]
    for i, pt in enumerate([p(I64_MIN, 0), p(I64_MAX, I64_MAX), p(-MAX_COORD, 0), p(0, MAX_COORD)]):
        c.add(f"get-location-huge-{i}", op="get_location", rect=big, pt=pt, loc=4)
    c.add("get-location-inverted", op="get_location", rect=[10, 10, 0, 0], pt=p(5, 5), loc=4)
    c.add("get-location-empty", op="get_location", rect=[5, 0, 5, 10], pt=p(5, 5), loc=4)

    for i in range(20):
        a = p(rng.int(-5, 5), rng.int(-5, 5))
        b = p(rng.int(-5, 5), rng.choice([a[1], rng.int(-5, 5)]))
        c.add(f"is-horizontal-{i}", op="is_horizontal", points=[a, b])

    # GetSegmentIntersection: crossings, touches, shared end points, collinear overlaps,
    # parallel and disjoint segments, each ordering of the arguments.
    fixed = [
        [(0, 0), (10, 10), (0, 10), (10, 0)], [(0, 0), (10, 0), (5, 0), (15, 0)],
        [(0, 0), (10, 0), (10, 0), (10, 10)], [(5, 0), (5, 10), (0, 5), (5, 5)],
        [(5, 5), (5, 10), (0, 5), (10, 5)], [(0, 0), (4, 4), (5, 5), (9, 9)],
        [(0, 0), (10, 0), (0, 1), (10, 1)], [(0, 5), (10, 5), (0, 0), (0, 10)],
        [(-5, 5), (5, 5), (0, 0), (0, 10)], [(0, 0), (0, 10), (0, 5), (0, 20)],
        [(3, 3), (3, 3), (0, 0), (10, 10)], [(0, 0), (10, 3), (10, 0), (0, 3)],
        [(1, 7), (9, 2), (4, 0), (4, 10)], [(0, 10), (0, 0), (0, 0), (10, 0)],
        [(10, 5), (0, 5), (5, 0), (5, 10)], [(0, 0), (3, 1), (3, 1), (6, 2)],
    ]
    for i, pts in enumerate(fixed):
        for j, order in enumerate([(0, 1, 2, 3), (1, 0, 2, 3), (2, 3, 0, 1), (0, 1, 3, 2)]):
            q = [p(*pts[k], k + 1) for k in order]
            c.add(f"segment-intersection-fixed-{i}-{j}", op="get_segment_intersection", points=q,
                  ip=p(-99, -99, -9))
    for i in range(160):
        span = rng.choice([3, 10, 1000])
        q = [p(rng.int(-span, span), rng.int(-span, span), rng.int(0, 3)) for _ in range(4)]
        if rng.int(0, 3) == 0:
            q[2] = list(q[rng.choice([0, 1])])
        c.add(f"segment-intersection-random-{i}", op="get_segment_intersection", points=q, ip=p(0, 0))
    # Large but defined coordinates: every difference fits in int64_t.
    for i in range(24):
        q = [p(rng.int(-MAX_COORD, MAX_COORD), rng.int(-MAX_COORD, MAX_COORD)) for _ in range(4)]
        c.add(f"segment-intersection-large-{i}", op="get_segment_intersection", points=q, ip=p(1, 2, 3))

    rect_path = path(SQUARE)
    for loc in range(5):
        for i in range(24):
            a = p(rng.int(-6, 16), rng.int(-6, 16), 1)
            b = p(rng.int(-6, 16), rng.int(-6, 16), 2)
            c.add(f"get-intersection-{loc}-{i}", op="get_intersection", rect_path=rect_path, p=a, p2=b,
                  loc=loc, ip=p(-1, -1, -1))
        for i, (a, b) in enumerate([((-5, 5), (15, 5)), ((5, -5), (5, 15)), ((-5, -5), (15, 15)),
                                    ((-5, 15), (15, -5)), ((0, 0), (10, 10)), ((0, 5), (10, 5)),
                                    ((-5, 0), (15, 0)), ((5, 5), (5, 5)), ((-1, -1), (11, -1))]):
            c.add(f"get-intersection-fixed-{loc}-{i}", op="get_intersection", rect_path=rect_path,
                  p=p(*a, 4), p2=p(*b, 5), loc=loc, ip=p(0, 0))
    c.add("get-intersection-extra-corners", op="get_intersection", rect_path=path(SQUARE + [(3, 3)]),
          p=p(-5, 5), p2=p(15, 5), loc=4, ip=p(0, 0))

    for loc in range(5):
        for cw in (False, True):
            c.add(f"adjacent-{loc}-{int(cw)}", op="get_adjacent_location", loc=loc, clockwise=cw)
        for curr in range(5):
            c.add(f"heading-clockwise-{loc}-{curr}", op="heading_clockwise", prev=loc, curr=curr)
            c.add(f"are-opposites-{loc}-{curr}", op="are_opposites", prev=loc, curr=curr)
            a = p(rng.int(-20, 20), rng.int(-20, 20))
            b = p(rng.int(-20, 20), rng.int(-20, 20))
            c.add(f"is-clockwise-{loc}-{curr}", op="is_clockwise", prev=loc, curr=curr, prev_pt=a, curr_pt=b,
                  rect_mp=p(5, 5))
    for i, (a, b) in enumerate([((-5, 5), (15, 5)), ((15, 5), (-5, 5)), ((5, -5), (5, 15)), ((0, 0), (10, 10))]):
        for prev, curr in ((0, 2), (2, 0), (1, 3), (3, 1), (4, 2)):
            c.add(f"is-clockwise-opposite-{i}-{prev}-{curr}", op="is_clockwise", prev=prev, curr=curr,
                  prev_pt=p(*a), curr_pt=p(*b), rect_mp=p(5, 5))

    for x in (-1, 0, 5, 10, 11):
        for y in (-1, 0, 5, 10, 11):
            c.add(f"edges-for-pt-{x}-{y}", op="get_edges_for_pt", pt=p(x, y), rect=rect)
    c.add("edges-for-pt-empty", op="get_edges_for_pt", pt=p(5, 5), rect=[5, 5, 5, 5])
    for edge in (-1, 0, 1, 2, 3, 4, 2**31 - 1, -2**31):
        for i, (a, b) in enumerate([((0, 0), (0, 5)), ((0, 5), (0, 0)), ((0, 0), (5, 0)), ((5, 0), (0, 0)),
                                    ((3, 3), (3, 3))]):
            c.add(f"heading-clockwise-edge-{edge}-{i}", op="is_heading_clockwise", points=[p(*a), p(*b)],
                  edge=edge)
    for op in ("has_horz_overlap", "has_vert_overlap"):
        for i in range(30):
            q = [p(rng.int(-5, 5), rng.int(-5, 5)) for _ in range(4)]
            c.add(f"{op.replace('_', '-')}-{i}", op=op, points=q)

    for i, (outer, inner) in enumerate([
            (SQUARE, [(2, 2), (8, 2), (8, 8), (2, 8)]), ([(2, 2), (8, 2), (8, 8), (2, 8)], SQUARE),
            (SQUARE, SQUARE), (SQUARE, [(-5, 5), (5, -5), (15, 5), (5, 15)]),
            ([(0, 0), (10, 0), (5, 10)], [(4, 1), (6, 1), (5, 3)]), (SQUARE, [(20, 20), (30, 20), (30, 30)]),
            ([(0, 0), (10, 0)], SQUARE), (SQUARE, []), (SQUARE, [(0, 0), (10, 10), (20, 20), (30, 30)])]):
        c.add(f"path1-contains-path2-{i}", op="path1_contains_path2", path1=path(outer), path2=path(inner))
    for i in range(20):
        outer = [(rng.int(-15, 25), rng.int(-15, 25)) for _ in range(rng.int(3, 8))]
        c.add(f"path1-contains-path2-random-{i}", op="path1_contains_path2", path1=path(outer),
              path2=path(SQUARE))

    c.add("start-locs-empty", op="start_locs_are_clockwise", locs=[])
    c.add("start-locs-one", op="start_locs_are_clockwise", locs=[2])
    for i, locs in enumerate([[0, 1], [1, 0], [0, 3], [3, 0], [0, 1, 2, 3, 0], [0, 3, 2, 1, 0], [0, 2], [4, 0, 1],
                              [1, 4, 3], [2, 2, 2], [0, 1, 0, 1]]):
        c.add(f"start-locs-fixed-{i}", op="start_locs_are_clockwise", locs=locs)
    for i in range(30):
        c.add(f"start-locs-random-{i}", op="start_locs_are_clockwise",
              locs=[rng.int(0, 4) for _ in range(rng.int(2, 12))])


def polygons(c, rng):
    def clip(name, rect, *calls, lines=False):
        op = "rect_clip_lines" if lines else "rect_clip"
        c.add(name, op=op, rect=rect, calls=[[path(q) if q and isinstance(q[0], tuple) else q for q in call]
                                              for call in calls])

    fixed = {
        "inside": [(1, 1), (9, 1), (9, 9), (1, 9)],
        "outside": [(20, 20), (30, 20), (30, 30), (20, 30)],
        "contains-ccw": [(-5, -5), (-5, 15), (15, 15), (15, -5)],
        "contains-cw": [(-5, -5), (15, -5), (15, 15), (-5, 15)],
        "contains-diamond": [(5, -20), (30, 5), (5, 30), (-20, 5)],
        "overlap-corner": [(5, 5), (15, 5), (15, 15), (5, 15)],
        "overlap-side": [(-5, 2), (15, 2), (15, 8), (-5, 8)],
        "equal": SQUARE,
        "equal-reversed": SQUARE[::-1],
        "on-boundary-triangle": [(0, 0), (10, 0), (10, 10)],
        "edge-touch-outside": [(10, 0), (20, 0), (20, 10), (10, 10)],
        "corner-touch-outside": [(10, 10), (20, 10), (20, 20)],
        "through-corners": [(-5, -5), (15, -5), (15, 15)],
        "diagonal-band": [(-5, -3), (13, 15), (15, 13), (-3, -5)],
        "u-shape": [(-5, 2), (15, 2), (15, 4), (2, 4), (2, 6), (15, 6), (15, 8), (-5, 8)],
        "comb": [(-5, 1), (15, 1), (15, 3), (-2, 3), (-2, 5), (15, 5), (15, 7), (-2, 7), (-2, 9), (15, 9),
                 (15, 11), (-5, 11)],
        "c-around": [(-5, -5), (15, -5), (15, -2), (-2, -2), (-2, 12), (15, 12), (15, 15), (-5, 15)],
        "spike-in": [(-5, 5), (5, 5), (-5, 6)],
        "spike-out": [(5, 5), (20, 5), (5, 6)],
        "repeated-points": [(5, 5), (5, 5), (15, 5), (15, 5), (15, 15), (5, 15), (5, 15)],
        "collinear-on-edge": [(-5, 0), (2, 0), (5, 0), (8, 0), (15, 0), (15, -5), (-5, -5)],
        "collinear-inside": [(2, 2), (5, 2), (8, 2), (8, 8), (2, 8)],
        "along-left-edge": [(0, -5), (0, 15), (-5, 15), (-5, -5)],
        "bowtie": [(-5, -5), (15, 15), (15, -5), (-5, 15)],
        "zero-area": [(-5, 5), (15, 5), (5, 5)],
        "wide-thin": [(-100, 4), (100, 4), (100, 6), (-100, 6)],
        "tall-thin": [(4, -100), (6, -100), (6, 100), (4, 100)],
        "star": [(5, -10), (7, 3), (20, 5), (7, 7), (5, 20), (3, 7), (-10, 5), (3, 3)],
        "outside-around-corner": [(-5, -5), (5, -5), (5, -1), (-1, -1), (-1, 5), (-5, 5)],
        "enter-exit-same-side": [(-5, 2), (5, 5), (-5, 8)],
        "two-points": [(-5, 5), (15, 5)],
        "one-point": [(5, 5)],
    }
    for name, q in fixed.items():
        clip(f"clip-{name}", RECT, [q])
        clip(f"lines-{name}", RECT, [q], lines=True)
    # Holes and several paths in one call, z values carried through.
    holes = [path([(-5, -5), (15, -5), (15, 15), (-5, 15)], 1), path([(2, 2), (2, 8), (8, 8), (8, 2)], 2),
             path([(4, -3), (6, -3), (6, 13), (4, 13)], 3)]
    clip("clip-holes", RECT, holes)
    clip("lines-holes", RECT, holes, lines=True)
    clip("clip-z", RECT, [path([(5, 5), (15, 5), (15, 15), (5, 15)], 77)])
    clip("clip-empty-call", RECT, [])
    clip("clip-empty-paths", RECT, [[], [p(1, 1)], [p(1, 1), p(2, 2)]])
    clip("lines-empty-paths", RECT, [[], [p(1, 1)], [p(-1, 1), p(-2, 2)]], lines=True)
    # Several Execute calls on one object.
    clip("clip-calls", RECT, [fixed["overlap-corner"]], [fixed["inside"], fixed["outside"]], [fixed["comb"]])
    clip("lines-calls", RECT, [fixed["comb"]], [fixed["two-points"]], lines=True)
    # Empty, inverted and degenerate rectangles return nothing; huge defined rectangles work.
    for name, rect in {"empty-width": [5, 0, 5, 10], "empty-height": [0, 5, 10, 5], "inverted": [10, 10, 0, 0],
                       "point": [5, 5, 5, 5], "unit": [0, 0, 1, 1],
                       "huge": [-MAX_COORD, -MAX_COORD, MAX_COORD, MAX_COORD]}.items():
        clip(f"clip-rect-{name}", rect, [fixed["overlap-corner"]])
        clip(f"lines-rect-{name}", rect, [fixed["overlap-corner"]], lines=True)
    # The full int64_t span: left + right stays defined; a path inside or wholly outside the
    # bounds needs no product.
    span = [I64_MIN + 10, I64_MIN + 10, I64_MAX - 10, I64_MAX - 10]
    clip("clip-rect-span-inside", span, [[(I64_MIN + 11, I64_MIN + 11), (I64_MAX - 11, I64_MIN + 11), (0, I64_MAX - 11)]])
    clip("clip-rect-span-outside", span, [[(I64_MIN, I64_MIN), (I64_MIN + 5, I64_MIN), (I64_MIN, I64_MIN + 5)]])
    clip("lines-rect-span-inside", span, [[(I64_MIN + 11, 0), (I64_MAX - 11, 0)]], lines=True)

    rects = [RECT, [-3, -7, 4, 2], [100, 200, 101, 260]]
    for i in range(260):
        rect = rng.choice(rects)
        l, t, r, b = rect
        w, h = r - l, b - t
        n = rng.int(3, 12)
        mode = rng.int(0, 3)
        q = []
        for _ in range(n):
            if mode == 0:  # anywhere around the rectangle
                q.append((rng.int(l - w, r + w), rng.int(t - h, b + h)))
            elif mode == 1:  # snapped to the rectangle's lines
                q.append((rng.choice([l - w, l, (l + r) // 2, r, r + w]), rng.choice([t - h, t, (t + b) // 2, b, b + h])))
            elif mode == 2 and q:  # orthogonal walk
                x, y = q[-1]
                q.append((rng.int(l - w, r + w), y) if len(q) % 2 else (x, rng.int(t - h, b + h)))
            else:
                q.append((rng.int(l - 2, r + 2), rng.int(t - 2, b + 2)))
        extra = [] if rng.int(0, 2) else [[(rng.int(l - w, r + w), rng.int(t - h, b + h)) for _ in range(rng.int(3, 6))]]
        clip(f"clip-random-{i}", rect, [q] + extra)
        clip(f"lines-random-{i}", rect, [q[: max(2, n - 3)]] + extra, lines=True)
    # Large defined coordinates: the path stays within MAX_COORD so no difference overflows.
    for i in range(30):
        big = MAX_COORD // 2
        rect = [-big // 4, -big // 4, big // 4, big // 4]
        q = [(rng.int(-big, big), rng.int(-big, big)) for _ in range(rng.int(3, 8))]
        clip(f"clip-large-{i}", rect, [q])
        clip(f"lines-large-{i}", rect, [q], lines=True)


def controls(c, rng):
    m = I64_MAX
    # Rect64::MidPoint in the constructor: left + right overflows.
    c.ub("ub-midpoint-x", op="rect_clip", rect=[m - 10, 0, m, 10], calls=[])
    c.ub("ub-midpoint-y-lines", op="rect_clip_lines", rect=[0, I64_MIN, 10, I64_MIN + 10], calls=[])
    c.ub("ub-midpoint-near-max", op="rect_clip", rect=[m - 20, m - 20, m - 10, m - 10],
         calls=[[path([(m - 23, m - 19), (m - 7, m - 19), (m - 10, m - 10)])]])
    # A crossing path in the full-span rectangle: its differences to the corners overflow.
    c.ub("ub-clip-span-crossing", op="rect_clip", rect=[I64_MIN + 10, I64_MIN + 10, m - 10, m - 10],
         calls=[[path([(m - 15, m - 15), (m, m - 15), (m, m)])]])
    # rectPath subscripts past the end of a short rectangle path.
    c.ub("ub-get-intersection-short-path", op="get_intersection", rect_path=path([(0, 0), (10, 0)]),
         p=p(-5, 5), p2=p(15, 5), loc=0, ip=p(0, 0))
    c.ub("ub-get-intersection-empty-path", op="get_intersection", rect_path=[], p=p(-5, 5), p2=p(15, 5),
         loc=4, ip=p(0, 0))
    # CrossProductSign differences overflow.
    c.ub("ub-segment-intersection", op="get_segment_intersection",
         points=[p(I64_MIN, 0), p(m, 0), p(0, -1), p(0, 1)], ip=p(0, 0))
    c.ub("ub-is-clockwise", op="is_clockwise", prev=0, curr=2, prev_pt=p(I64_MIN, 0), curr_pt=p(m, 0),
         rect_mp=p(m, 1))
    c.ub("ub-path1-contains-path2", op="path1_contains_path2",
         path1=path([(I64_MIN, -1), (m, -1), (m, 1)]), path2=path([(0, 0)]))
    # Paths whose own edges overflow the products while crossing a small rectangle.
    c.ub("ub-clip-wide", op="rect_clip", rect=RECT,
         calls=[[path([(I64_MIN + 5, 5), (m - 5, 5), (m - 5, 6)])]])
    c.ub("ub-lines-wide", op="rect_clip_lines", rect=RECT, calls=[[path([(I64_MIN + 5, 5), (m - 5, 6)])]])


def build():
    c = Corpus()
    rng = Rng(0x5EC7C11)
    for section in [helpers, polygons, controls]:
        section(c, rng)
    dump = lambda o: json.dumps(o, ensure_ascii=False, separators=(",", ":"))
    return ("".join(dump(x) + "\n" for x in c.cases), "".join(dump(x) + "\n" for x in c.controls))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if the committed files differ")
    args = parser.parse_args()
    cases, controls_text = build()
    stale = []
    for target, text in [(CASES, cases), (CONTROLS, controls_text)]:
        if args.check:
            if not target.exists() or target.read_text(encoding="utf-8") != text:
                stale.append(str(target.relative_to(ROOT)))
        else:
            target.write_text(text, encoding="utf-8", newline="\n")
    if stale:
        print("stale generated files: " + ", ".join(stale), file=sys.stderr)
        return 1
    print(f"{cases.count(chr(10))} cases, {controls_text.count(chr(10))} controls")
    return 0


if __name__ == "__main__":
    sys.exit(main())
