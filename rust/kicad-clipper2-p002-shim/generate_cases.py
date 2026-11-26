#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for unit
thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002 (clipper.engine.cpp lines 2272-3337 and
clipper.offset.cpp).

The corpus drives every engine-remainder body through Clipper64/ClipperD (unions,
intersections, differences, xors under every fill rule; horizontals, joins, splits, maxima,
open paths, polytrees, Z callbacks, repeated executions) and every offset body through
ClipperOffset (every join and end type, positive/negative/insignificant deltas, miter limits,
arc tolerances, reversed groups, single points, two-point joined paths, delta callbacks set or
passed to Execute, trees), plus the offset file's local helpers. All inputs are generated
here; no design data is used. The controls are requests both adapters must refuse identically
(input domain, resource guard, malformed requests); each is followed by a successful
continuation request in compare_controls.py.

    generate_cases.py          write port/cases/<unit>.jsonl and the controls fixture
    generate_cases.py --check  verify both files are current
"""
import argparse
import json
import math
import random
import struct
import sys
from decimal import Decimal, localcontext
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002"
CASES = ROOT / "port/cases" / f"{UNIT}.jsonl"
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-engine-p002-controls.jsonl"
SEED = 20261127
ENGINE_COORD = (2**63 - 1) >> 2


def hexd(value):
    return struct.pack(">d", float(value)).hex()


def pt(x, y, z=0):
    return [int(x), int(y), int(z)]


def ptd(x, y, z=0):
    return [hexd(x), hexd(y), int(z)]


def rect(x, y, w, h, z=0):
    return [pt(x, y, z), pt(x + w, y, z), pt(x + w, y + h, z), pt(x, y + h, z)]


def reverse(path):
    return list(reversed(path))


def clip64(subjects, clips=(), open_=(), ct=2, fr=1, preserve=True, rev=False, pip=64,
           z="none", output="paths", runs=1):
    return {"op": "clip64", "subjects": list(subjects), "open": list(open_), "clips": list(clips),
            "clip_type": ct, "fill_rule": fr, "preserve_collinear": preserve,
            "reverse_solution": rev, "pip_threshold": pip, "z": z, "output": output, "runs": runs}


def clipd(subjects, clips=(), open_=(), precision=2, ct=2, fr=1, preserve=True, rev=False,
          z="none", output="paths", runs=1):
    return {"op": "clipd", "precision": precision, "subjects": list(subjects),
            "open": list(open_), "clips": list(clips), "clip_type": ct, "fill_rule": fr,
            "preserve_collinear": preserve, "reverse_solution": rev, "z": z, "output": output,
            "runs": runs}


def group(paths, join, end, single=False):
    return {"paths": list(paths), "join": join, "end": end, "single": single}


def offset(groups, executes, miter=2.0, arc=0.0, preserve=False, rev=False, z="none",
           table=None, set_callback=False):
    return {"op": "offset", "groups": list(groups), "miter_limit": hexd(miter),
            "arc_tolerance": hexd(arc), "preserve_collinear": preserve, "reverse_solution": rev,
            "z": z, "delta_table": None if table is None else [hexd(v) for v in table],
            "set_callback": set_callback,
            "executes": [{"kind": k, "delta": hexd(d)} for k, d in executes]}


def helper(fn, **args):
    out = {"op": "helper", "fn": fn}
    out.update(args)
    return out


# --- shapes ---------------------------------------------------------------------------

def input_sin_cos(angle):
    """Generate inputs without host libm, as in the accepted arc corpus generator.

    Evaluate the exact binary64 angle with 100 decimal digits and 120 Taylor
    terms before rounding once to binary64. These are input coordinates, never
    reference answers; the genuine C++ oracle still computes every result.
    """
    with localcontext() as context:
        context.prec = 100
        x = Decimal.from_float(angle)
        minus_square = -x * x
        sin_term, cos_term = x, Decimal(1)
        sin_sum, cos_sum = sin_term, cos_term
        for n in range(1, 120):
            sin_term *= minus_square / ((2 * n) * (2 * n + 1))
            cos_term *= minus_square / ((2 * n - 1) * (2 * n))
            sin_sum += sin_term
            cos_sum += cos_term
        return float(sin_sum), float(cos_sum)


def star(cx, cy, r1, r2, n, z=0):
    pts = []
    for i in range(2 * n):
        r = r1 if i % 2 == 0 else r2
        a = float.fromhex("0x1.921fb54442d18p+1") * i / n
        sin_a, cos_a = input_sin_cos(a)
        pts.append(pt(cx + round(r * cos_a), cy + round(r * sin_a), z))
    return pts


def staircase(x, y, steps, w, h):
    pts = [pt(x, y)]
    cx, cy = x, y
    for _ in range(steps):
        cx += w
        pts.append(pt(cx, cy))
        cy += h
        pts.append(pt(cx, cy))
    pts.append(pt(x, cy))
    return pts


def comb(x, y, teeth, w, h):
    pts = [pt(x, y)]
    for i in range(teeth):
        pts += [pt(x + i * 2 * w, y + h), pt(x + i * 2 * w + w, y + h), pt(x + i * 2 * w + w, y)]
    pts += [pt(x + teeth * 2 * w, y), pt(x + teeth * 2 * w, y - h), pt(x, y - h)]
    return pts


def random_poly(rng, grid, n, scale=1, z=False):
    return [pt(rng.randint(0, grid) * scale, rng.randint(0, grid) * scale,
               rng.choice([0, 0, 1, 2, 7]) if z else 0) for _ in range(n)]


def random_ortho(rng, grid, n, scale=1):
    """A rectilinear walk: many horizontals, collinear runs and shared vertices."""
    x, y = rng.randint(0, grid), rng.randint(0, grid)
    pts = [pt(x * scale, y * scale)]
    for i in range(n):
        if i % 2 == 0:
            x = rng.randint(0, grid)
        else:
            y = rng.randint(0, grid)
        pts.append(pt(x * scale, y * scale))
    return pts


# --- corpus ---------------------------------------------------------------------------

def corpus():
    rng = random.Random(SEED)
    cases = []

    def add(name, value):
        cases.append((name, value))

    # Fixed clipping scenes across every clip type and fill rule.
    scenes = {
        "overlap": ([rect(0, 0, 10, 10), rect(5, 5, 10, 10)], [rect(3, -2, 4, 20)]),
        "nested": ([rect(0, 0, 30, 30), rect(10, 10, 10, 10)], [rect(5, 5, 20, 20)]),
        "touching": ([rect(0, 0, 10, 10), rect(10, 0, 10, 10)], [rect(5, 5, 10, 10)]),
        "staircase": ([staircase(0, 0, 4, 5, 5)], [rect(2, 2, 20, 6)]),
        "comb": ([comb(0, 0, 4, 3, 6)], [rect(1, -3, 20, 5)]),
        "star": ([star(50, 50, 40, 15, 7)], [star(55, 45, 30, 20, 5)]),
        "bowtie": ([[pt(0, 0), pt(10, 10), pt(10, 0), pt(0, 10)]], [rect(2, 2, 6, 6)]),
        "reversed": ([reverse(rect(0, 0, 10, 10)), rect(2, 2, 6, 6)], [rect(5, -5, 10, 20)]),
        "collinear": ([[pt(0, 0), pt(5, 0), pt(10, 0), pt(10, 10), pt(5, 10), pt(0, 10)]],
                      [[pt(0, 5), pt(10, 5), pt(20, 5), pt(20, 8), pt(0, 8)]]),
        "spike": ([[pt(0, 0), pt(20, 0), pt(20, 10), pt(30, 10), pt(20, 10), pt(0, 10)]],
                  [rect(10, -5, 5, 20)]),
    }
    for name, (subj, clip) in scenes.items():
        for ct in range(5):
            for fr in range(4):
                add(f"scene-{name}-ct{ct}-fr{fr}", clip64(subj, clip, ct=ct, fr=fr))
        for output in ["tree", "paths_open", "tree_open"]:
            add(f"scene-{name}-{output}", clip64(subj, clip, ct=1, fr=1, output=output))
        add(f"scene-{name}-reverse", clip64(subj, clip, ct=2, fr=0, rev=True))
        add(f"scene-{name}-nocollinear", clip64(subj, clip, ct=2, fr=1, preserve=False))
        add(f"scene-{name}-z", clip64(subj, clip, ct=3, fr=1, z="recorder"))
        add(f"scene-{name}-runs", clip64(subj, clip, ct=4, fr=1, runs=2, output="tree"))
        add(f"scene-{name}-pip0", clip64(subj, clip, ct=3, fr=1, pip=0, output="tree"))

    # Open paths through polygons, including horizontal, vertical and diagonal lines and
    # lines that start or end on edges.
    lines = [[pt(-5, 5), pt(25, 5)], [pt(5, -5), pt(5, 25)], [pt(-5, -5), pt(25, 25)],
             [pt(0, 0), pt(10, 0), pt(10, 10)], [pt(3, 3), pt(3, 3), pt(7, 3), pt(7, 12)],
             [pt(-5, 10), pt(30, 10), pt(30, 20), pt(-5, 20)]]
    for i, line in enumerate(lines):
        for ct in range(1, 5):
            for fr in [0, 1, 2, 3]:
                add(f"open-{i}-ct{ct}-fr{fr}",
                    clip64([rect(0, 0, 20, 20)], [rect(5, 5, 10, 10)], open_=[line], ct=ct,
                           fr=fr, output="paths_open"))
        add(f"open-{i}-tree", clip64([rect(0, 0, 20, 20)], open_=[line], ct=1, fr=1,
                                     output="tree_open", z="recorder"))
        add(f"open-{i}-only", clip64([], [rect(0, 0, 20, 20)], open_=[line], ct=1, fr=1,
                                     output="paths_open"))

    # Degenerate and boundary inputs.
    big = ENGINE_COORD
    add("empty", clip64([], ct=2))
    add("noclip", clip64([rect(0, 0, 10, 10)], ct=0))
    add("segment", clip64([[pt(0, 0), pt(10, 10)]], ct=2))
    add("flat", clip64([[pt(0, 0), pt(10, 0), pt(20, 0)]], ct=2))
    add("duplicates", clip64([[pt(0, 0), pt(0, 0), pt(10, 0), pt(10, 0), pt(10, 10), pt(0, 10)]]))
    add("tiny-triangle", clip64([[pt(0, 0), pt(1, 0), pt(0, 1)]], ct=2))
    add("big-squares", clip64([rect(-big, -big, big, big), rect(0, 0, big, big)], ct=2))
    add("big-overlap", clip64([rect(-big, -big, big, big)], [rect(-big // 2, -big // 2, big, big)],
                              ct=1, fr=1, output="tree"))
    add("big-star", clip64([star(0, 0, big // 2, big // 5, 9)], [rect(-big, -big // 4, big, big)],
                           ct=3, fr=1))
    add("z-tags", clip64([rect(0, 0, 10, 10, z=5)], [rect(5, 5, 10, 10, z=9)], ct=2, fr=1,
                         z="recorder", output="tree"))

    # Random scenes on small grids (coincident vertices, horizontals, collinear runs).
    for i in range(320):
        grid = rng.choice([4, 8, 16, 40])
        scale = rng.choice([1, 1, 1, 3, 1000, 10**12])
        maker = rng.choice(["poly", "ortho", "mixed"])

        def shape():
            if maker == "poly" or (maker == "mixed" and rng.random() < 0.5):
                return random_poly(rng, grid, rng.randint(3, 9), scale, z=True)
            return random_ortho(rng, grid, rng.randint(3, 10), scale)

        subj = [shape() for _ in range(rng.randint(1, 4))]
        clip = [shape() for _ in range(rng.randint(0, 3))]
        open_ = [random_poly(rng, grid, rng.randint(2, 5), scale)
                 for _ in range(rng.choice([0, 0, 0, 1, 2]))]
        output = rng.choice(["paths", "paths", "paths_open", "tree", "tree_open"])
        add(f"random-{i:03d}",
            clip64(subj, clip, open_, ct=rng.randint(0, 4), fr=rng.randint(0, 3),
                   preserve=rng.random() < 0.7, rev=rng.random() < 0.3,
                   pip=rng.choice([0, 3, 64, 2**32]), z=rng.choice(["none", "recorder"]),
                   output=output, runs=rng.choice([1, 1, 2])))

    # ClipperD: scaling, precision limits and exceptions.
    dscenes = [
        [[ptd(0, 0), ptd(10.5, 0), ptd(10.5, 10.25), ptd(0, 10.25)]],
        [[ptd(-1.125, -1.125), ptd(3.75, -0.5), ptd(2.0625, 4.5)]],
        [[ptd(0.1, 0.2), ptd(0.7, 0.2), ptd(0.7, 0.9), ptd(0.1, 0.9)]],
    ]
    dclip = [[ptd(2.5, -3), ptd(6.25, -3), ptd(6.25, 20), ptd(2.5, 20)]]
    for i, subj in enumerate(dscenes):
        for precision in [-2, 0, 2, 5, 8]:
            for ct in [1, 2, 3, 4]:
                add(f"clipd-{i}-p{precision}-ct{ct}",
                    clipd(subj, dclip, precision=precision, ct=ct, fr=1))
        add(f"clipd-{i}-tree", clipd(subj, dclip, ct=3, output="tree_open",
                                     open_=[[ptd(-1, 1.5), ptd(12, 1.5)]], z="recorder"))
        add(f"clipd-{i}-runs", clipd(subj, dclip, ct=2, runs=2, rev=True, preserve=False))
    add("clipd-precision-9", clipd(dscenes[0], precision=9))
    add("clipd-precision-neg9", clipd(dscenes[0], precision=-9))
    add("clipd-infinite", clipd([[ptd(0, 0), ptd(math.inf, 0), ptd(1, 1)]]))
    add("clipd-out-of-range", clipd([[ptd(0, 0), ptd(1e300, 0), ptd(1, 1)]]))
    for i in range(40):
        subj = [[ptd(rng.randint(-400, 400) / 8, rng.randint(-400, 400) / 8,
                     rng.choice([0, 3])) for _ in range(rng.randint(3, 7))]
                for _ in range(rng.randint(1, 3))]
        clip = [[ptd(rng.randint(-400, 400) / 8, rng.randint(-400, 400) / 8)
                 for _ in range(rng.randint(3, 6))] for _ in range(rng.randint(0, 2))]
        add(f"clipd-random-{i:02d}",
            clipd(subj, clip, precision=rng.randint(-1, 6), ct=rng.randint(1, 4),
                  fr=rng.randint(0, 3), output=rng.choice(["paths", "tree", "paths_open"]),
                  z=rng.choice(["none", "recorder"])))

    # ClipperOffset: every join and end type.
    poly = [pt(0, 0), pt(100, 0), pt(100, 60), pt(60, 60), pt(60, 100), pt(0, 100)]
    acute = [pt(0, 0), pt(200, 0), pt(0, 30)]
    line = [pt(0, 0), pt(100, 0), pt(100, 50), pt(150, 80)]
    for join in range(4):
        for end in range(5):
            shape = poly if end == 0 else line
            for delta in [10.0, -7.5, 0.25, 33.0]:
                add(f"offset-j{join}-e{end}-d{delta}",
                    offset([group([shape], join, end)], [("paths", delta)]))
            add(f"offset-j{join}-e{end}-acute",
                offset([group([acute], join, end)], [("paths", 12.0)], miter=4.0))
            add(f"offset-j{join}-e{end}-tree",
                offset([group([poly, rect(20, 20, 20, 20)], join, end)], [("tree", 5.0)]))
    for miter in [0.0, 1.0, 1.5, 2.0, 10.0]:
        add(f"offset-miter-{miter}", offset([group([acute], 3, 0)], [("paths", 15.0)],
                                            miter=miter))
    for arc in [0.0, 0.1, 0.25, 2.0, 50.0]:
        add(f"offset-arc-{arc}", offset([group([poly], 2, 0), group([line], 0, 4)],
                                        [("paths", 20.0)], arc=arc))
    add("offset-reversed-group", offset([group([reverse(poly)], 0, 0)], [("paths", 8.0)]))
    add("offset-reversed-mixed", offset([group([reverse(poly), rect(30, 30, 10, 10)], 1, 0),
                                         group([line], 1, 2)], [("paths", 6.0)], rev=True))
    add("offset-preserve", offset([group([[pt(0, 0), pt(50, 0), pt(100, 0), pt(100, 100),
                                           pt(0, 100)]], 3, 0)], [("paths", 5.0)],
                                  preserve=True))
    add("offset-single-square", offset([group([[pt(5, 5, 3)]], 0, 0, single=True)],
                                       [("paths", 4.0), ("paths", 0.75)]))
    add("offset-single-round", offset([group([[pt(5, 5, 3)]], 2, 0, single=True)],
                                      [("paths", 4.0)]))
    add("offset-single-miter-neg", offset([group([[pt(5, 5)]], 3, 2)], [("paths", -4.0)]))
    add("offset-two-joined", offset([group([[pt(0, 0), pt(40, 10)]], 0, 1),
                                     group([[pt(0, 50), pt(40, 60)]], 2, 1),
                                     group([[pt(0, 90), pt(40, 90), pt(80, 120)]], 1, 1)],
                                    [("paths", 6.0)]))
    add("offset-two-polygon", offset([group([[pt(0, 0), pt(40, 10)]], 3, 0)], [("paths", 6.0)]))
    add("offset-empty-path", offset([group([[], poly], 0, 0)], [("paths", 3.0)]))
    add("offset-empty-group", offset([group([], 0, 0)], [("paths", 3.0)]))
    add("offset-no-groups", offset([], [("paths", 3.0)]))
    add("offset-flat", offset([group([[pt(0, 0), pt(10, 0), pt(20, 0)]], 0, 0)],
                              [("paths", 3.0)]))
    add("offset-duplicates", offset([group([[pt(0, 0), pt(0, 0), pt(30, 0), pt(30, 30),
                                             pt(0, 30), pt(0, 0)]], 3, 0)], [("paths", 2.0)]))
    add("offset-insignificant", offset([group([poly], 0, 0), group([line], 1, 3)],
                                       [("paths", 0.4), ("tree", -0.49)]))
    add("offset-z", offset([group([[pt(0, 0, 4), pt(50, 0, 4), pt(50, 50, 9), pt(0, 50, 9)],
                                   [pt(25, 25, 4), pt(75, 25, 0), pt(75, 75, 0)]], 0, 0)],
                           [("paths", 10.0)], z="recorder"))
    add("offset-z-open", offset([group([[pt(0, 0, 2), pt(60, 0, 3), pt(0, 30, 2)]], 2, 4)],
                                [("tree", 9.0)], z="recorder"))
    add("offset-large", offset([group([rect(-2**39, -2**39, 2**39, 2**39)], 2, 0)],
                               [("paths", 2.0**38)]))
    add("offset-callback-execute",
        offset([group([poly], 0, 0), group([line], 2, 3)], [("callback", 0.0), ("paths", 4.0)],
               table=[3.0, -2.0, 7.5, 0.0]))
    add("offset-callback-set",
        offset([group([poly, [pt(150, 150)]], 2, 0)], [("paths", 4.0), ("tree", 9.0)],
               table=[5.0, 1.5], set_callback=True, arc=0.5))
    add("offset-callback-open",
        offset([group([line], 1, 2), group([acute], 3, 4)], [("callback", 0.0)],
               table=[2.0, 4.0, 6.0]))
    add("offset-callback-reversed",
        offset([group([reverse(poly)], 3, 0)], [("callback", 0.0)], table=[4.0, -1.0]))
    add("offset-lifecycle", offset([group([poly], 3, 0)],
                                   [("paths", 5.0), ("tree", -5.0), ("paths", 0.1),
                                    ("paths", 12.0)]))
    # Defined neighbors of the source-derived empty-path and int-cast guards.
    for end in range(5):
        add(f"offset-empty-insignificant-{end}",
            offset([group([[]], 0, end)], [("paths", 0.49), ("tree", -0.49)]))
    add("offset-empty-polygon", offset([group([[]], 0, 0)], [("paths", 1.0)]))
    for join in [0, 1, 3]:
        add(f"offset-singleton-int-max-{join}",
            offset([group([[pt(0, 0)]], join, 0)], [("paths", 2**31 - 1)]))
    add("offset-large-multipoint",
        offset([group([poly], 0, 0)], [("paths", 2**31)]))
    add("offset-singleton-callback-small",
        offset([group([[pt(0, 0)]], 0, 0)], [("paths", 2**31)],
               table=[1.0], set_callback=True))
    add("offset-singleton-callback-negative-skip",
        offset([group([[pt(0, 0)]], 0, 0)], [("callback", 0.0)], table=[-2**31]))
    add("offset-singleton-callback-reversed-skip",
        offset([group([reverse(poly), [pt(100, 100)]], 0, 0)],
               [("callback", 0.0)], table=[1.0] * len(poly) + [2**31]))
    add("offset-large-table-multipoint-only",
        offset([group([line, [pt(100, 100)]], 0, 2)], [("callback", 0.0)],
               table=[2**31] + [1.0] * 63))
    for i in range(160):
        shapes = []
        for _ in range(rng.randint(1, 3)):
            kind = rng.random()
            n = rng.randint(1, 8) if kind < 0.15 else rng.randint(2, 9)
            shapes.append(random_poly(rng, rng.choice([10, 50, 400]), n,
                                      rng.choice([1, 1, 7, 1000]), z=rng.random() < 0.3))
        groups = [group(shapes, rng.randint(0, 3), rng.randint(0, 4))
                  for _ in range(rng.randint(1, 2))]
        executes = [(rng.choice(["paths", "paths", "tree"]),
                     rng.choice([-30.0, -5.5, -1.0, 0.3, 1.0, 2.5, 12.0, 47.25]))
                    for _ in range(rng.randint(1, 2))]
        table = None
        set_cb = False
        if rng.random() < 0.15:
            table = [rng.choice([-3.0, 0.0, 1.0, 2.0, 6.5]) for _ in range(rng.randint(1, 4))]
            set_cb = rng.random() < 0.5
            if not set_cb:
                executes[0] = ("callback", 0.0)
        add(f"offset-random-{i:03d}",
            offset(groups, executes, miter=rng.choice([0.0, 2.0, 3.5]),
                   arc=rng.choice([0.0, 0.0, 0.5, 3.0]), preserve=rng.random() < 0.3,
                   rev=rng.random() < 0.3, z=rng.choice(["none", "recorder"]), table=table,
                   set_callback=set_cb))

    # ClipperD open paths returned as paths (BuildPathsD's open branch).
    for i, subj in enumerate(dscenes):
        add(f"clipd-{i}-paths-open",
            clipd(subj, dclip, ct=1, fr=1, output="paths_open", rev=i == 1,
                  open_=[[ptd(-1, 1.5), ptd(12, 1.5)], [ptd(0.5, -2), ptd(0.5, 30), ptd(3, 30)]]))

    # Polytrees of self-intersecting and rectilinear scenes: splits, horizontal joins that
    # split or join records, and split ownership checks.
    rng2 = random.Random(SEED + 1)
    for i in range(240):
        grid = rng2.choice([5, 6, 8, 12])
        if rng2.random() < 0.5:
            subj = [random_ortho(rng2, grid, rng2.randint(5, 14)) for _ in range(rng2.randint(1, 3))]
        else:
            subj = [random_poly(rng2, grid, rng2.randint(5, 12)) for _ in range(rng2.randint(1, 3))]
        clip = [random_ortho(rng2, grid, rng2.randint(3, 8)) for _ in range(rng2.randint(0, 2))]
        add(f"tree-split-{i:03d}",
            clip64(subj, clip, ct=rng2.choice([1, 2, 2, 3, 4]), fr=rng2.randint(0, 3),
                   preserve=rng2.random() < 0.5, rev=rng2.random() < 0.3,
                   pip=rng2.choice([0, 64]), output=rng2.choice(["tree", "tree_open"]),
                   z=rng2.choice(["none", "recorder"])))

    # Near-parallel steep and shallow edges at large coordinates, where the computed
    # intersection can fall outside the scanbeam (AddNewIntersectNode's corrections).
    for i in range(120):
        base = rng2.choice([10**9, 10**13, 10**15, 10**17])
        h = rng2.choice([1, 2, 3, 7, 50])
        def sliver():
            x0 = rng2.randint(-base, base)
            dx = rng2.randint(1, 5) * base // rng2.choice([1, 3, 7])
            y0 = rng2.randint(-h, h)
            return [pt(x0, y0), pt(x0 + dx, y0 + rng2.randint(-h, h) + h),
                    pt(x0 + dx + rng2.randint(1, 9), y0 + rng2.randint(-h, h)),
                    pt(x0 + rng2.randint(1, 9), y0 - h - rng2.randint(0, h))]
        def steep():
            y0 = rng2.randint(-base, base)
            dy = rng2.randint(1, 5) * base
            x0 = rng2.randint(-h, h)
            return [pt(x0, y0), pt(x0 + h, y0 + dy), pt(x0 + h + rng2.randint(1, 3), y0 + dy),
                    pt(x0 + rng2.randint(1, 3), y0 - 1)]
        maker = sliver if i % 2 == 0 else steep
        add(f"rounding-{i:03d}",
            clip64([maker() for _ in range(rng2.randint(2, 4))], [maker()],
                   ct=rng2.randint(1, 4), fr=rng2.randint(0, 3),
                   output=rng2.choice(["paths", "tree"])))

    # File-local helpers of clipper.offset.cpp.
    for x, y in [(3.0, 4.0), (0.0, 0.0), (-0.0, 1e-300), (1e300, 1e300), (2.0**60, -7.0),
                 (math.inf, 1.0), (math.nan, 2.0)]:
        add(f"helper-hypot-{x}-{y}", helper("hypot", x=hexd(x), y=hexd(y)))
    for a, b in [((0, 0), (10, 0)), ((0, 0), (0, 0)), ((5, 5), (2, 9)),
                 ((-ENGINE_COORD, 0), (ENGINE_COORD, 1)), ((7, -3), (7, 100))]:
        add(f"helper-normal-{a}-{b}", helper("unit_normal", a=pt(*a), b=pt(*b)))
    for v in [(3.0, 4.0), (0.0005, 0.0), (0.0, 0.0), (-1e-3, 0.0), (1e10, -1e10)]:
        add(f"helper-normalize-{v}", helper("normalize", v=ptd(*v, 2)))
        add(f"helper-avg-{v}", helper("avg_unit", a=ptd(*v), b=ptd(1.0, 0.0, 5)))
    for v, eps in [(0.0, None), (0.0009, None), (-0.001, None), (0.5, 1.0), (math.nan, None)]:
        add(f"helper-almost-{v}-{eps}",
            helper("almost_zero", v=hexd(v), epsilon=None if eps is None else hexd(eps)))
    for p, n, d in [((10, 20, 3), (0.6, -0.8), 5.0), ((0, 0, 0), (1.0, 0.0), -2.5),
                    ((2**40, -2**40, 1), (-0.707, 0.707), 2.0**39), ((7, 7, 0), (0.5, 0.5), 0.5)]:
        add(f"helper-perpendic-{p}-{d}", helper("perpendic", pt=pt(*p), norm=ptd(*n),
                                                delta=hexd(d)))
        add(f"helper-perpendic-d-{p}-{d}", helper("perpendic_d", pt=pt(*p), norm=ptd(*n),
                                                  delta=hexd(d)))
    add("helper-negate", helper("negate_path", path=[ptd(1.5, -2.0, 4), ptd(-0.0, 0.0, 1)]))
    add("helper-negate-empty", helper("negate_path", path=[]))
    for end in range(5):
        add(f"helper-closed-{end}", helper("is_closed_path", end=end))
    lowest = [
        [rect(0, 0, 10, 10), rect(20, 20, 10, 10)],
        [reverse(rect(0, 0, 10, 10)), rect(-5, -5, 30, 30)],
        [[pt(0, 0), pt(10, 0), pt(20, 0)], reverse(rect(0, 5, 3, 3))],
        [[], [pt(4, 4)]],
        [],
        [[pt(0, 10), pt(10, 10), pt(5, 0)], [pt(0, 10), pt(-10, 10), pt(-5, 20)]],
    ]
    for i, paths in enumerate(lowest):
        for initial in [False, True]:
            add(f"helper-lowest-{i}-{initial}",
                helper("lowest_closed_path_info", paths=paths, is_neg_area=initial))
    return cases


def controls():
    """(request input, expected error message) pairs both adapters refuse."""
    ok_poly = [rect(0, 0, 10, 10)]
    big = ENGINE_COORD + 1
    items = [
        ("clip64-domain", clip64([rect(0, 0, big, 10)]), "invalid input: domain"),
        ("clip64-domain-negative", clip64([], [[pt(-big, 0), pt(0, 5), pt(5, 0)]]),
         "invalid input: domain"),
        ("clip64-runs", clip64(ok_poly, runs=3), "invalid input: runs"),
        ("clip64-enum", clip64(ok_poly, ct=5), "invalid input: enum"),
        ("clip64-pip", clip64(ok_poly, pip=-1), "invalid input: int64"),
        ("clip64-output", clip64(ok_poly, output="list"), "invalid input: output"),
        ("clip64-point", clip64([[[0, 0]]]), "invalid input: point"),
        ("clipd-nan", clipd([[ptd(0, 0), ptd(math.nan, 1), ptd(1, 1)]]), "invalid input: domain"),
        ("clipd-precision", clipd([[ptd(0, 0), ptd(1, 1), ptd(1, 0)]], precision=65),
         "invalid input: int"),
        ("offset-domain", offset([group([rect(0, 0, 2**40 + 1, 5)], 0, 0)], [("paths", 1.0)]),
         "invalid input: domain"),
        ("offset-delta-nan", offset([group(ok_poly, 0, 0)], [("paths", math.nan)]),
         "invalid input: delta"),
        ("offset-delta-inf", offset([group(ok_poly, 0, 0)], [("paths", math.inf)]),
         "invalid input: delta"),
        ("offset-delta-huge", offset([group(ok_poly, 0, 0)], [("paths", 2.0**41)]),
         "invalid input: delta"),
        ("offset-arc-steps", offset([group(ok_poly, 2, 0)], [("paths", 2.0**39)], arc=0.01),
         "invalid input: arc steps"),
        ("offset-arc-steps-table", offset([group(ok_poly, 0, 4)], [("callback", 0.0)],
                                          arc=1e-9, table=[1e6]), "invalid input: arc steps"),
        ("offset-callback", offset([group(ok_poly, 0, 0)], [("callback", 0.0)]),
         "invalid input: callback"),
        ("offset-set-callback", offset([group(ok_poly, 0, 0)], [("paths", 1.0)],
                                       set_callback=True), "invalid input: callback"),
        ("offset-executes", offset([group(ok_poly, 0, 0)], []), "invalid input: executes"),
        ("offset-single", offset([group(ok_poly + ok_poly, 0, 0, single=True)],
                                 [("paths", 1.0)]), "invalid input: single"),
        ("offset-groups", offset([group(ok_poly, 0, 0)] * 65, [("paths", 1.0)]),
         "invalid input: size"),
        ("helper-fn", helper("ellipse"), "invalid input: fn"),
        ("helper-normal-domain", helper("unit_normal", a=pt(big, 0), b=pt(0, 0)),
         "invalid input: domain"),
        ("op", {"op": "minkowski"}, "invalid input: op"),
    ]
    for end in [1, 2, 3, 4]:
        for kind in ["paths", "callback"]:
            items.append((f"offset-empty-open-{end}-{kind}",
                          offset([group([[]], 0, end)], [(kind, 1.0)],
                                 table=[1.0] if kind == "callback" else None),
                          "undefined behaviour: offset"))
    for join in [0, 1, 3]:
        for delta in [2**31 - 0.5, 2**31, -2**31]:
            # Open groups take abs(delta); all three reach the int conversion.
            items.append((f"offset-singleton-int-{join}-{delta}",
                          offset([group([[pt(0, 0)]], join, 2)], [("paths", delta)]),
                          "undefined behaviour: offset"))
    items += [
        ("offset-singleton-stripped",
         offset([group([[pt(0, 0), pt(0, 0)]], 0, 0)], [("paths", 2**31)]),
         "undefined behaviour: offset"),
        ("offset-singleton-callback-set-overflow",
         offset([group([[pt(0, 0)]], 0, 0)], [("paths", 1.0)],
                table=[2**31], set_callback=True), "undefined behaviour: offset"),
        ("offset-singleton-callback-persists",
         offset([group([[pt(0, 0)]], 0, 0)], [("callback", 0.0), ("paths", 1.0)],
                table=[1.0, 2**31]), "undefined behaviour: offset"),
        ("offset-singleton-callback-reversed-overflow",
         offset([group([reverse(ok_poly[0]), [pt(100, 100)]], 0, 0)],
                [("callback", 0.0)], table=[1.0] * 4 + [-2**31]),
         "undefined behaviour: offset"),
    ]
    out = []
    for name, request, message in items:
        out.append({"request": {"unit": UNIT, "case": f"control-{name}", "input": request},
                    "error": {"kind": "invalid_input", "message": message}})
    return out


def render():
    lines = [json.dumps({"unit": UNIT, "case": name, "input": value}, separators=(",", ":"))
             for name, value in corpus()]
    names = [json.loads(line)["case"] for line in lines]
    assert len(names) == len(set(names)), "duplicate case names"
    control_lines = [json.dumps(c, separators=(",", ":")) for c in controls()]
    return "\n".join(lines) + "\n", "\n".join(control_lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    cases, control_text = render()
    if args.check:
        stale = [p for p, text in [(CASES, cases), (CONTROLS, control_text)]
                 if not p.exists() or p.read_text(encoding="utf-8") != text]
        if stale:
            print("stale: " + ", ".join(str(p.relative_to(ROOT)) for p in stale))
            return 1
        print(f"{len(cases.splitlines())} cases and {len(control_text.splitlines())} controls current")
        return 0
    CASES.write_text(cases, encoding="utf-8", newline="\n")
    CONTROLS.write_text(control_text, encoding="utf-8", newline="\n")
    print(f"wrote {len(cases.splitlines())} cases and {len(control_text.splitlines())} controls")
    return 0


if __name__ == "__main__":
    sys.exit(main())
