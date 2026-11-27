#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public corpus and refusal controls for unit
thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002 (clipper.h,
clipper.minkowski.h and the clipper.offset.h/clipper.rectclip.h declarations).

The corpus calls every inline body of clipper.h and clipper.minkowski.h through the genuine
overloads, including the default-argument forms: the boolean wrappers on paths and trees
(prefilled trees show when they are replaced or cleared), InflatePaths (every join and end type,
single points with round joins, which reach Ellipse), the ClipperOffset members, TranslatePath(s),
RectClip/RectClipLines (Rect64 and RectD, path and paths forms), the PolyTree helpers and stream
insertions, MakePath/MakePathD/MakePathZ/MakePathZD (vector and array forms), TrimCollinear,
Distance, Length, NearCollinear, Ellipse, SimplifyPath(s), Path2ContainsPath1,
RamerDouglasPeucker, Minkowski sums and differences, details::GetNext/GetPrior and the version.
Clipper2Exception results (precision, scale, range, odd counts) are compared values. All inputs
are generated here; no design data is used. The controls are requests both adapters must refuse
identically (input domain and resource guards, malformed requests, and inputs whose reference
evaluation is undefined, which the oracle decides by its UBSan replay); each is followed by a
successful continuation request in compare_controls.py.

    generate_cases.py          write port/cases/<unit>.jsonl and the controls fixture
    generate_cases.py --check  verify both files are current
"""
import argparse
import json
import random
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002"
CASES = ROOT / "port/cases" / f"{UNIT}.jsonl"
CONTROLS = ROOT / "port/oracle/fixtures/clipper2-include-p002-controls.jsonl"
SEED = 20261127
ENGINE_COORD = (2**63 - 1) >> 2
NAN = float("nan")
INF = float("inf")


def hexd(value):
    return struct.pack(">d", float(value)).hex()


def pt(x, y, z=0):
    return [int(x), int(y), int(z)]


def ptd(x, y, z=0):
    return [hexd(x), hexd(y), int(z)]


def to_d(path, div=100.0):
    return [ptd(p[0] / div, p[1] / div, p[2]) for p in path]


def rect_path(x, y, w, h, z=0):
    return [pt(x, y, z), pt(x + w, y, z), pt(x + w, y + h, z), pt(x, y + h, z)]


def rev(path):
    return list(reversed(path))


class Gen:
    def __init__(self, seed):
        self.r = random.Random(seed)

    def poly(self, n, span, z=False):
        """A random (possibly self-intersecting) closed path."""
        return [pt(self.r.randint(-span, span), self.r.randint(-span, span),
                   self.r.randint(0, 9) if z else 0) for _ in range(n)]

    def walk(self, n, step, x=0, y=0):
        out = []
        for _ in range(n):
            out.append(pt(x, y))
            x += self.r.randint(-step, step)
            y += self.r.randint(-step, step)
        return out

    def collinear_heavy(self, n, span):
        """Points on a few lines, with repeats, so TrimCollinear removes many of them."""
        out = []
        while len(out) < n:
            x0, y0 = self.r.randint(-span, span), self.r.randint(-span, span)
            dx, dy = self.r.choice([(1, 0), (0, 1), (1, 1), (2, -1), (-3, 1)])
            for k in range(self.r.randint(1, 4)):
                out.append(pt(x0 + k * dx * 7, y0 + k * dy * 7))
        return out[:n]

    def noisy_line(self, n, length, noise):
        return [pt(i * length // max(n - 1, 1), self.r.randint(-noise, noise)) for i in range(n)]


def boolean(t, fn, subjects, clips=None, ct=None, fr=1, precision=None, prefill=None):
    return {"op": "boolean", "type": t, "fn": fn, "clip_type": ct, "fill_rule": fr,
            "subjects": subjects, "clips": clips, "precision": precision, "prefill": prefill}


def inflate(t, paths, delta, join, end, ml=None, precision=None, at=None):
    return {"op": "inflate", "type": t, "paths": paths, "delta": hexd(delta), "join": join,
            "end": end, "miter_limit": None if ml is None else hexd(ml), "precision": precision,
            "arc_tolerance": None if at is None else hexd(at)}


def group(paths, join, end, single=False):
    return {"paths": paths, "join": join, "end": end, "single": single}


def members(ctor, sets, groups, clear, delta):
    enc = []
    for i, v in enumerate(ctor):
        enc.append(hexd(v) if i < 2 else v)
    return {"op": "offset_members", "ctor": enc,
            "set": [[n, hexd(v) if isinstance(v, float) else v] for n, v in sets],
            "groups": groups, "clear": clear, "delta": None if delta is None else hexd(delta)}


def corpus():
    g = Gen(SEED)
    out = []

    def add(name, value):
        out.append((name, value))

    # --- boolean operations ------------------------------------------------------------
    sets = {
        "squares": ([rect_path(0, 0, 100, 100)], [rect_path(50, 50, 100, 100)]),
        "nested": ([rect_path(0, 0, 300, 300), rev(rect_path(50, 50, 200, 200)),
                    rect_path(100, 100, 50, 50)], [rect_path(120, -20, 40, 400)]),
        "random": ([g.poly(9, 500), g.poly(7, 500)], [g.poly(8, 500)]),
        "random-z": ([g.poly(12, 2000, True)], [g.poly(10, 2000, True), g.poly(5, 2000, True)]),
        "large": ([rect_path(-10**15, -10**15, 2 * 10**15, 2 * 10**15)],
                  [g.poly(6, 10**15)]),
        "edge": ([rect_path(-ENGINE_COORD, -ENGINE_COORD, ENGINE_COORD, ENGINE_COORD)],
                 [rect_path(-5, -5, 10, 10)]),
        "empty": ([], []),
        "touching": ([rect_path(0, 0, 10, 10), rect_path(10, 0, 10, 10)],
                     [rect_path(5, 5, 10, 10)]),
    }
    for sname, (s, c) in sets.items():
        for fr in range(4):
            for ct in range(5):
                add(f"bool-i64-op-{sname}-ct{ct}-fr{fr}",
                    boolean("i64", "booleanop", s, c, ct=ct, fr=fr))
            for fn in ["intersect", "union", "difference", "xor"]:
                add(f"bool-i64-{fn}-{sname}-fr{fr}", boolean("i64", fn, s, c, fr=fr))
            add(f"bool-i64-union1-{sname}-fr{fr}", boolean("i64", "union_subjects", s, fr=fr))
        for ct in range(5):
            add(f"bool-i64-tree-{sname}-ct{ct}",
                boolean("i64", "booleanop_tree", s, c, ct=ct, fr=1,
                        prefill=[rect_path(7, 7, 3, 3)]))
    # A failing execution (coordinates beyond the engine range are rejected by AddPaths? no:
    # within the domain, so every tree is replaced) and an empty prefill.
    add("bool-i64-tree-noprefill", boolean("i64", "booleanop_tree", sets["nested"][0],
                                           sets["nested"][1], ct=2, fr=0, prefill=[]))
    dsets = {k: ([to_d(p) for p in v[0]], [to_d(p) for p in v[1]])
             for k, v in sets.items() if k not in ("large", "edge")}
    dsets["fractions"] = ([[ptd(0.125, 0.5), ptd(10.3, 0.1), ptd(10.7, 9.9), ptd(0.01, 10.0)]],
                          [[ptd(5.55, -1.0), ptd(15.0, 5.0), ptd(5.0, 11.111)]])
    dsets["huge"] = ([[ptd(-1e17, -1e17), ptd(1e17, -1e17), ptd(1e17, 1e17)]], [])
    for sname, (s, c) in dsets.items():
        for precision in [None, -2, 0, 3, 8, 9, -9]:
            ptag = "def" if precision is None else str(precision)
            add(f"bool-f64-op-{sname}-p{ptag}",
                boolean("f64", "booleanop", s, c, ct=1 + (len(out) % 4), fr=len(out) % 4,
                        precision=precision))
            for fn in ["intersect", "union", "difference", "xor", "union_subjects"]:
                add(f"bool-f64-{fn}-{sname}-p{ptag}",
                    boolean("f64", fn, s, None if fn == "union_subjects" else c,
                            fr=(len(out) + 1) % 4, precision=precision))
            add(f"bool-f64-tree-{sname}-p{ptag}",
                boolean("f64", "booleanop_tree", s, c, ct=2, fr=1, precision=precision,
                        prefill=[[ptd(1.5, 1.5), ptd(2.5, 1.5), ptd(2.5, 2.5)]]))

    # --- InflatePaths --------------------------------------------------------------------
    shapes = {
        "square": [rect_path(0, 0, 100, 100)],
        "concave": [[pt(0, 0), pt(100, 0), pt(100, 100), pt(50, 30), pt(0, 100)]],
        "reversed": [rev(rect_path(0, 0, 80, 40)), rect_path(200, 0, 30, 30)],
        "open": [[pt(0, 0), pt(50, 10), pt(100, 0), pt(120, 60)]],
        "two": [[pt(0, 0), pt(40, 30)]],
        "random": [g.poly(8, 300, True)],
        "single": [[pt(5, 7, 3)]],
        "mixed": [[pt(0, 0, 1)], [pt(10, 10), pt(30, 10)], rect_path(100, 100, 20, 20)],
    }
    for sname, paths in shapes.items():
        for join in range(4):
            for end in range(5):
                for delta in [-7.5, 0.0, 0.4, 3.0, 25.25]:
                    add(f"inflate-i64-{sname}-j{join}-e{end}-d{delta}",
                        inflate("i64", paths, delta, join, end))
        add(f"inflate-i64-{sname}-ml", inflate("i64", paths, 12.0, 3, 0, ml=1.5))
        add(f"inflate-i64-{sname}-ml-at", inflate("i64", paths, 12.0, 2, 4, ml=3.0, at=0.25))
        add(f"inflate-i64-{sname}-at-big", inflate("i64", paths, 40.0, 2, 4, ml=2.0, at=5.0))
    # Single points with round joins reach Ellipse (formerly a typed deferral).
    for delta in [1.0, 1.5, 2.0, 3.7, 10.0, 99.5, 1000.0, -5.0]:
        for at in [None, 0.01, 0.5, 4.0]:
            add(f"inflate-i64-ellipse-d{delta}-at{at}",
                inflate("i64", [[pt(-3, 9, 5)], [pt(100, 100, 2)]], delta, 2, 0,
                        ml=None if at is None else 2.0, at=at))
            add(f"inflate-i64-ellipse-open-d{delta}-at{at}",
                inflate("i64", [[pt(0, 0, 7)]], delta, 2, 4, ml=None if at is None else 2.0,
                        at=at))
    dshapes = {k: [to_d(p, 10.0) for p in v] for k, v in shapes.items()}
    for sname, paths in dshapes.items():
        for join in range(4):
            for end in [0, 2, 4]:
                add(f"inflate-f64-{sname}-j{join}-e{end}",
                    inflate("f64", paths, 1.25, join, end))
        for precision in [-1, 0, 1, 4, 8, 9, -9]:
            add(f"inflate-f64-{sname}-p{precision}",
                inflate("f64", paths, 2.5, 2, 0, ml=2.0, precision=precision))
            add(f"inflate-f64-{sname}-p{precision}-at",
                inflate("f64", paths, 2.5, 2, 4, ml=2.0, precision=precision, at=0.05))
        add(f"inflate-f64-{sname}-ml", inflate("f64", paths, -0.5, 3, 0, ml=4.0))
        add(f"inflate-f64-{sname}-zero", inflate("f64", paths, 0.0, 0, 0))
        add(f"inflate-f64-{sname}-zero-badprec",
            inflate("f64", paths, 0.0, 0, 0, ml=2.0, precision=12))

    # --- ClipperOffset members --------------------------------------------------------------
    sq = [rect_path(0, 0, 50, 50)]
    for ci, ctor in enumerate([[], [3.0], [3.0, 0.5], [3.0, 0.5, True], [3.0, 0.5, True, True],
                 [1.0, 0.0, False, True]]):
        for clear in [False, True]:
            add(f"members-ctor{ci}-clear{clear}",
                members(ctor, [], [group(sq, 3, 0), group([[pt(5, 5)]], 2, 0, True)], clear,
                        4.0))
    add("members-setters", members([], [("miter_limit", 5.0), ("arc_tolerance", 0.125),
                                         ("preserve_collinear", True),
                                         ("reverse_solution", True), ("miter_limit", 1.25)],
                                   [group([[pt(0, 0), pt(40, 0), pt(40, 5), pt(20, 5),
                                            pt(20, 40), pt(0, 40)]], 3, 0)], False, 3.0))
    add("members-noexec", members([2.5, 1.0], [("reverse_solution", True)], [], False, None))
    add("members-collinear", members([], [("preserve_collinear", True)],
                                     [group([[pt(0, 0), pt(10, 0), pt(20, 0), pt(20, 10),
                                              pt(0, 10)]], 0, 0)], False, 2.0))

    # --- TranslatePath(s) ---------------------------------------------------------------------
    for i in range(6):
        p = g.poly(5, 10**6, True)
        dx, dy = g.r.randint(-10**9, 10**9), g.r.randint(-10**9, 10**9)
        add(f"translate-i64-path-{i}", {"op": "translate", "type": "i64", "form": "path",
                                        "value": p, "dx": dx, "dy": dy})
        add(f"translate-i64-paths-{i}", {"op": "translate", "type": "i64", "form": "paths",
                                         "value": [p, [], g.poly(3, 50)], "dx": -dx, "dy": dy})
        add(f"translate-f64-path-{i}", {"op": "translate", "type": "f64", "form": "path",
                                        "value": to_d(p, 7.0), "dx": hexd(dx / 3),
                                        "dy": hexd(-0.1)})
        add(f"translate-f64-paths-{i}", {"op": "translate", "type": "f64", "form": "paths",
                                         "value": [to_d(p, 3.0), []], "dx": hexd(0.1),
                                         "dy": hexd(dy / 7)})
    add("translate-i64-edge", {"op": "translate", "type": "i64", "form": "path",
                               "value": [pt(2**63 - 2, -2**63 + 1)], "dx": 1, "dy": -1})
    add("translate-f64-special", {"op": "translate", "type": "f64", "form": "path",
                                  "value": [ptd(INF, 1.0), ptd(NAN, -0.0)], "dx": hexd(-INF),
                                  "dy": hexd(0.0)})

    # --- RectClip / RectClipLines -----------------------------------------------------------
    rects64 = {"r": [0, 0, 100, 80], "empty": [10, 10, 10, 50], "inv": [100, 0, 0, 80],
               "tiny": [40, 40, 41, 41], "far": [-10**12, -10**12, 10**12, 10**12]}
    paths64 = {"star": [g.poly(9, 150)], "inside": [rect_path(10, 10, 20, 20)],
               "around": [rect_path(-50, -50, 300, 300)], "lines": [g.walk(12, 60, 50, 40)],
               "mixed": [g.poly(6, 120), [], g.walk(5, 90)], "none": []}
    for rname, r in rects64.items():
        for pname, ps in paths64.items():
            for lines in [False, True]:
                tag = "lines" if lines else "clip"
                add(f"rect-i64-{tag}-{rname}-{pname}",
                    {"op": "rectclip", "type": "i64", "lines": lines, "form": "paths",
                     "rect": r, "value": ps, "precision": None})
                add(f"rect-i64-{tag}-{rname}-{pname}-path",
                    {"op": "rectclip", "type": "i64", "lines": lines, "form": "path",
                     "rect": r, "value": ps[0] if ps else [], "precision": None})
    rectsd = {"r": [0.0, 0.0, 1.0, 0.8], "empty": [0.5, 0.5, 0.5, 0.9],
              "frac": [-0.123, 0.004, 0.777, 0.3333]}
    for rname, r in rectsd.items():
        for pname, ps in paths64.items():
            dps = [to_d(p, 100.0) for p in ps]
            for lines in [False, True]:
                tag = "lines" if lines else "clip"
                for precision in [None, 0, 4, 9]:
                    ptag = "def" if precision is None else str(precision)
                    add(f"rect-f64-{tag}-{rname}-{pname}-p{ptag}",
                        {"op": "rectclip", "type": "f64", "lines": lines, "form": "paths",
                         "rect": [hexd(v) for v in r], "value": dps, "precision": precision})
                add(f"rect-f64-{tag}-{rname}-{pname}-path",
                    {"op": "rectclip", "type": "f64", "lines": lines, "form": "path",
                     "rect": [hexd(v) for v in r], "value": dps[0] if dps else [],
                     "precision": 3})
    add("rect-f64-range", {"op": "rectclip", "type": "f64", "lines": False, "form": "paths",
                           "rect": [hexd(0), hexd(0), hexd(1), hexd(1)],
                           "value": [[ptd(1e17, 0), ptd(0, 1), ptd(1, 1)]], "precision": 4})

    # --- PolyTree helpers and streams ---------------------------------------------------------
    def nested(depth, breadth, size=1000, x=0, y=0, parent=-1, nodes=None, outside=False):
        nodes = [] if nodes is None else nodes
        for b in range(breadth):
            w = size // (breadth + 1)
            px = x + b * (size // breadth)
            poly = rect_path(px, y, w, w)
            if outside and depth == 1 and b == breadth - 1:
                poly = rect_path(px + 5 * size, y, w, w)
            nodes.append({"parent": parent, "polygon": poly})
            me = len(nodes) - 1
            if depth > 1:
                nested(depth - 1, breadth, w - 4, px + 2, y + 2, me, nodes, outside)
        return nodes

    trees = {
        "empty": [],
        "flat": nested(1, 3),
        "deep": nested(5, 1),
        "wide": nested(2, 4),
        "outside": nested(3, 2, outside=True),
        "partial": [{"parent": -1, "polygon": rect_path(0, 0, 100, 100)},
                    {"parent": 0, "polygon": [pt(10, 10), pt(150, 10), pt(150, 20),
                                              pt(10, 20)]}],
        "onedge": [{"parent": -1, "polygon": rect_path(0, 0, 100, 100)},
                   {"parent": 0, "polygon": [pt(0, 0), pt(100, 0), pt(50, 50)]}],
        "chain": [{"parent": i - 1, "polygon": rect_path(i, i, 200 - 2 * i, 200 - 2 * i)}
                  for i in range(40)],
    }
    states = [None, {"flags": ["dec", "skipws"], "precision": 6, "width": 9, "fill": "*"},
              {"flags": ["hex", "showbase", "left"], "precision": 3, "width": 4, "fill": "#"},
              {"flags": ["oct", "showpos", "internal"], "precision": 0, "width": -3, "fill": " "}]
    for tname, nodes in trees.items():
        nodes_d = [{"parent": n["parent"], "polygon": to_d(n["polygon"], 8.0)} for n in nodes]
        for node in sorted({-1, 0, len(nodes) // 2, len(nodes) - 1}):
            if node >= len(nodes):
                continue
            for fn in ["to_paths", "path_to_paths", "contains_children", "node_contains"]:
                if fn in ("path_to_paths", "node_contains") and node < 0 and not nodes:
                    pass
                add(f"tree-i64-{tname}-{fn}-n{node}",
                    {"op": "polytree", "type": "i64", "nodes": nodes, "fn": fn, "node": node,
                     "state": None})
            for fn in ["to_paths", "path_to_paths"]:
                add(f"tree-f64-{tname}-{fn}-n{node}",
                    {"op": "polytree", "type": "f64", "nodes": nodes_d, "fn": fn,
                     "node": node, "state": None})
            for si, state in enumerate(states):
                add(f"tree-i64-{tname}-stream-n{node}-s{si}",
                    {"op": "polytree", "type": "i64", "nodes": nodes, "fn": "stream",
                     "node": node, "state": state})
                add(f"tree-f64-{tname}-stream-n{node}-s{si}",
                    {"op": "polytree", "type": "f64", "nodes": nodes_d, "fn": "stream",
                     "node": node, "state": state})
    for i, (idx, hole, cnt, pre) in enumerate([(0, False, 0, ""), (1, True, 1, "  "),
                                               (17, False, 2, "    "), (2**63, True, 2**64 - 1,
                                                                        "x y"),
                                               (5, False, 1, "-" * 40)]):
        for si, state in enumerate(states):
            add(f"outline-{i}-s{si}", {"op": "outline", "idx": idx, "is_hole": hole,
                                       "count": cnt, "preamble": pre, "state": state})

    # --- MakePath ------------------------------------------------------------------------------
    ints = {"i64": [3, -4, 2**62, -2**63, 7, 0, 1, 2, -9, 10],
            "i32": [2**31 - 1, -2**31, 5, 6, -7, 8, 9, 10, 11, 12],
            "u64": [2**64 - 1, 2**63, 5, 0, 7, 8, 9, 10, 11, 12],
            "u8": [255, 0, 1, 2, 3, 4, 5, 6, 7, 8]}
    for elem, vals in ints.items():
        for n in [0, 1, 2, 3, 4, 6, 7, 8, 9, 10]:
            add(f"make-{elem}-vec-{n}", {"op": "make_path", "fn": "make_path", "elem": elem,
                                         "values": vals[:n], "array": False})
            add(f"make-d-{elem}-vec-{n}", {"op": "make_path", "fn": "make_path_d", "elem": elem,
                                           "values": vals[:n], "array": False})
        for n in [2, 4, 6, 8]:
            for fn in ["make_path", "make_path_d"]:
                add(f"make-{fn}-{elem}-arr-{n}", {"op": "make_path", "fn": fn, "elem": elem,
                                                  "values": vals[:n], "array": True})
        for n in [3, 6, 9]:
            for fn in ["make_path_z", "make_path_zd"]:
                add(f"make-{fn}-{elem}-arr-{n}", {"op": "make_path", "fn": fn, "elem": elem,
                                                  "values": vals[:n], "array": True})
    dvals = [hexd(v) for v in [1.5, -2.5, 0.49999999999999994, 2.5, -0.0, 1e300, 3.25, -7.75, 9.0]]
    for n in [0, 1, 2, 5, 8]:
        add(f"make-d-f64-vec-{n}", {"op": "make_path", "fn": "make_path_d", "elem": "f64",
                                    "values": dvals[:n], "array": False})
    for n in [2, 4, 6, 8]:
        add(f"make-d-f64-arr-{n}", {"op": "make_path", "fn": "make_path_d", "elem": "f64",
                                    "values": dvals[:n], "array": True})
    zd = [hexd(v) for v in [1.5, 2.5, 3.9, -4.5, 5.0, -6.99, 7.0, 8.0, 2.0**62]]
    for n in [3, 6, 9]:
        add(f"make-zd-f64-arr-{n}", {"op": "make_path", "fn": "make_path_zd", "elem": "f64",
                                     "values": zd[:n], "array": True})

    # --- TrimCollinear ------------------------------------------------------------------------
    trim_paths = [[], [pt(0, 0)], [pt(0, 0), pt(0, 0)], [pt(0, 0), pt(5, 5)],
                  [pt(0, 0), pt(5, 0), pt(10, 0)], [pt(0, 0), pt(10, 0), pt(10, 10), pt(5, 10),
                                                    pt(0, 10), pt(0, 5)],
                  [pt(0, 0), pt(10, 0), pt(20, 0), pt(10, 0)],
                  rect_path(0, 0, 10, 10), [pt(5, 0), pt(10, 0), pt(10, 10), pt(0, 10), pt(0, 0)]]
    for i in range(30):
        trim_paths.append(g.collinear_heavy(g.r.randint(3, 25), 50))
    for i, p in enumerate(trim_paths):
        for open_ in [None, False, True]:
            add(f"trim-i64-{i}-o{open_}", {"op": "trim", "type": "i64", "path": p,
                                           "precision": None, "open": open_})
        if i % 2 == 0:
            for precision in [0, 2, -1, 9]:
                add(f"trim-f64-{i}-p{precision}",
                    {"op": "trim", "type": "f64", "path": to_d(p, 10.0), "precision": precision,
                     "open": None if precision else True})

    # --- Distance / Length / NearCollinear ------------------------------------------------------
    for i in range(12):
        a, b, c = g.poly(3, 10**9, True)
        add(f"distance-i64-{i}", {"op": "distance", "type": "i64", "a": a, "b": b})
        add(f"distance-f64-{i}", {"op": "distance", "type": "f64", "a": to_d([a], 3.0)[0],
                                  "b": to_d([b], 7.0)[0]})
        add(f"near-i64-{i}", {"op": "near_collinear", "type": "i64", "a": a, "b": b, "c": c,
                              "value": hexd([0.0, 1e-6, 0.01, 0.5][i % 4])})
        add(f"near-f64-{i}", {"op": "near_collinear", "type": "f64", "a": to_d([a])[0],
                              "b": to_d([b])[0], "c": to_d([pt(a[0] + 1, a[1], 0)])[0],
                              "value": hexd(1e-3)})
    add("near-i64-coincident", {"op": "near_collinear", "type": "i64", "a": pt(1, 1),
                                "b": pt(1, 1), "c": pt(5, 5), "value": hexd(0.1)})
    for i in range(10):
        p = g.poly(g.r.randint(0, 9), 10**6)
        for closed in [None, False, True]:
            add(f"length-i64-{i}-c{closed}", {"op": "length", "type": "i64", "path": p,
                                              "closed": closed})
        add(f"length-f64-{i}", {"op": "length", "type": "f64", "path": to_d(p, 9.0),
                                "closed": True})

    # --- Ellipse -----------------------------------------------------------------------------
    for t in ["i64", "f64"]:
        center = pt(17, -23, 4) if t == "i64" else ptd(1.7, -2.3, 4)
        for rx, ry, steps in [(10.0, None, None), (10.0, 0.0, None), (10.0, 4.0, None),
                              (10.0, 4.0, 0), (10.0, 4.0, 1), (10.0, 4.0, 2), (10.0, 4.0, 3),
                              (10.0, 4.0, 64), (0.0, None, None), (-1.0, 5.0, None),
                              (0.1, None, None), (0.5, 0.5, 2), (1.0, None, None),
                              (1e6, 2e5, None), (12345.678, -1.0, 100), (3.0, NAN, 5)]:
            if t == "i64" and ry is not None and ry != ry:
                continue
            add(f"ellipse-{t}-{rx}-{ry}-{steps}",
                {"op": "ellipse", "type": t, "form": "center", "center": center, "rect": None,
                 "radius_x": hexd(rx), "radius_y": None if ry is None else hexd(ry),
                 "steps": steps})
        for r, steps in [([0, 0, 100, 40], None), ([0, 0, 100, 40], 12), ([10, 10, 10, 50], None),
                         ([-7, -9, 3, 11], 7), ([100, 0, 0, 40], None), ([0, 0, 1, 1], None)]:
            rr = r if t == "i64" else [hexd(v / 4) for v in r]
            add(f"ellipse-{t}-rect-{r}-{steps}",
                {"op": "ellipse", "type": t, "form": "rect", "center": None, "rect": rr,
                 "radius_x": None, "radius_y": None, "steps": steps})
    add("ellipse-f64-nan-steps", {"op": "ellipse", "type": "f64", "form": "center",
                                  "center": ptd(0, 0), "rect": None, "radius_x": hexd(NAN),
                                  "radius_y": hexd(1.0), "steps": 6})

    # --- SimplifyPath(s) ----------------------------------------------------------------------
    for i in range(20):
        p = g.noisy_line(g.r.randint(0, 40), 1000, g.r.choice([0, 2, 20]))
        for eps in [0.0, 1.5, 10.0, 1e9]:
            for closed in [None, False, True]:
                add(f"simplify-i64-{i}-e{eps}-c{closed}",
                    {"op": "simplify", "type": "i64", "form": "path", "value": p,
                     "epsilon": hexd(eps), "closed": closed})
        add(f"simplify-f64-{i}", {"op": "simplify", "type": "f64", "form": "path",
                                  "value": to_d(p, 10.0), "epsilon": hexd(0.75), "closed": False})
    add("simplify-i64-paths", {"op": "simplify", "type": "i64", "form": "paths",
                               "value": [g.poly(12, 100), [], g.poly(3, 10), rect_path(0, 0, 5, 5)],
                               "epsilon": hexd(4.0), "closed": None})
    add("simplify-f64-paths", {"op": "simplify", "type": "f64", "form": "paths",
                               "value": [to_d(g.poly(15, 100))], "epsilon": hexd(NAN),
                               "closed": True})
    add("simplify-i64-big", {"op": "simplify", "type": "i64", "form": "path",
                             "value": g.noisy_line(3000, 10**6, 50), "epsilon": hexd(30.0),
                             "closed": True})

    # --- Path2ContainsPath1 -------------------------------------------------------------------
    outer = rect_path(0, 0, 100, 100)
    for i, inner in enumerate([rect_path(10, 10, 20, 20), rect_path(-10, -10, 20, 20),
                               rect_path(200, 200, 5, 5), [pt(0, 0), pt(100, 0), pt(50, 50)],
                               [pt(0, 50), pt(50, 0), pt(100, 50), pt(50, 100)],
                               [pt(0, 0), pt(100, 0), pt(100, 100)], [],
                               [pt(50, 0), pt(50, 100), pt(50, 50)],
                               [pt(0, 0), pt(100, 0), pt(100, 100), pt(0, 100)],
                               [pt(1, 1), pt(200, 1), pt(1, 2)]]):
        add(f"contains-i64-{i}", {"op": "contains", "type": "i64", "path1": inner,
                                  "path2": outer})
        add(f"contains-i64-swap-{i}", {"op": "contains", "type": "i64", "path1": outer,
                                       "path2": inner})
        add(f"contains-f64-{i}", {"op": "contains", "type": "f64", "path1": to_d(inner),
                                  "path2": to_d(outer)})
    for i in range(15):
        add(f"contains-i64-rand-{i}", {"op": "contains", "type": "i64", "path1": g.poly(6, 80),
                                       "path2": g.poly(7, 100)})

    # --- RamerDouglasPeucker ------------------------------------------------------------------
    for i in range(20):
        p = g.noisy_line(g.r.randint(0, 60), 2000, g.r.choice([0, 5, 50]))
        if i % 5 == 0 and len(p) > 3:
            p[-1] = list(p[0])
        for eps in [0.0, 2.0, 25.0, INF]:
            add(f"rdp-i64-{i}-e{eps}", {"op": "rdp", "type": "i64", "form": "path", "value": p,
                                        "epsilon": hexd(eps)})
        add(f"rdp-f64-{i}", {"op": "rdp", "type": "f64", "form": "path", "value": to_d(p, 3.0),
                             "epsilon": hexd(-1.5)})
    add("rdp-i64-paths", {"op": "rdp", "type": "i64", "form": "paths",
                          "value": [g.walk(30, 20), [], g.walk(4, 5), [pt(1, 1)] * 6],
                          "epsilon": hexd(3.0)})
    add("rdp-i64-spiral", {"op": "rdp", "type": "i64", "form": "path",
                           "value": [pt(i * i, (i % 2) * i * 3) for i in range(1000)],
                           "epsilon": hexd(0.5)})

    # --- Minkowski ------------------------------------------------------------------------------
    pats = {"square": rect_path(-2, -2, 4, 4), "tri": [pt(0, 0), pt(5, 1), pt(1, 4)],
            "point": [pt(3, 3)], "empty": []}
    pathsm = {"zig": [pt(0, 0), pt(20, 10), pt(40, 0), pt(60, 15)],
              "box": rect_path(0, 0, 30, 30), "single": [pt(7, 7)], "random": g.poly(8, 100),
              "empty": []}
    for pn, pat in pats.items():
        for qn, q in pathsm.items():
            for closed in [False, True]:
                for s in [True, False]:
                    tag = "sum" if s else "diff"
                    add(f"mink-i64-{tag}-{pn}-{qn}-c{closed}",
                        {"op": "minkowski", "type": "i64", "sum": s, "pattern": pat, "path": q,
                         "closed": closed, "decimal_places": None})
            add(f"mink-f64-sum-{pn}-{qn}",
                {"op": "minkowski", "type": "f64", "sum": True, "pattern": to_d(pat, 4.0),
                 "path": to_d(q, 4.0), "closed": True, "decimal_places": None})
            add(f"mink-f64-diff-{pn}-{qn}-dp",
                {"op": "minkowski", "type": "f64", "sum": False, "pattern": to_d(pat, 4.0),
                 "path": to_d(q, 4.0), "closed": False, "decimal_places": 3})
    for dp in [-400, -3, 0, 1, 10]:
        add(f"mink-f64-dp{dp}", {"op": "minkowski", "type": "f64", "sum": True,
                                 "pattern": to_d(pats["tri"]), "path": to_d(pathsm["zig"]),
                                 "closed": False, "decimal_places": dp})
    add("mink-f64-dp400-empty", {"op": "minkowski", "type": "f64", "sum": True, "pattern": [],
                                 "path": [], "closed": True, "decimal_places": 400})

    # --- details::GetNext / GetPrior ----------------------------------------------------------
    for i in range(25):
        n = g.r.randint(1, 12)
        flags = [g.r.random() < 0.5 for _ in range(n)]
        if all(flags):
            flags[g.r.randrange(n)] = False
        cur = g.r.randrange(n)
        for fn in ["get_next", "get_prior"]:
            add(f"detail-{fn}-{i}", {"op": "detail", "fn": fn, "current": cur, "high": n - 1,
                                     "flags": flags})
    for fn in ["get_next", "get_prior"]:
        add(f"detail-{fn}-single", {"op": "detail", "fn": fn, "current": 0, "high": 0,
                                    "flags": [False]})
        add(f"detail-{fn}-last", {"op": "detail", "fn": fn, "current": 4, "high": 4,
                                  "flags": [True, False, True, True, True]})

    add("version", {"op": "version"})
    return out


def controls():
    big = 2**40 + 1
    ok_sq = [rect_path(0, 0, 10, 10)]
    items = [
        # Input domain and resource guards.
        ("bool-domain", boolean("i64", "union", [rect_path(0, 0, ENGINE_COORD + 1, 1)], []),
         "invalid input: domain"),
        ("bool-precision-i64", boolean("i64", "union", ok_sq, [], precision=2),
         "invalid input: precision"),
        ("bool-fn", boolean("i64", "nope", ok_sq, []), "invalid input: fn"),
        ("bool-clips-single", boolean("i64", "union_subjects", ok_sq, []), "invalid input: clips"),
        ("bool-ct-wrapper", boolean("i64", "union", ok_sq, [], ct=1), "invalid input: clip_type"),
        ("bool-prefill", boolean("i64", "union", ok_sq, [], prefill=[]), "invalid input: prefill"),
        ("inflate-domain", inflate("i64", [[pt(big, 0)]], 1.0, 0, 0), "invalid input: domain"),
        ("inflate-delta", inflate("i64", ok_sq, 2.0**41, 0, 0), "invalid input: domain"),
        ("inflate-delta-nan", inflate("i64", ok_sq, NAN, 0, 0), "invalid input: domain"),
        ("inflate-miter", inflate("i64", ok_sq, 1.0, 3, 0, ml=INF), "invalid input: domain"),
        ("inflate-steps", inflate("i64", ok_sq, 1e9, 2, 0, ml=2.0, at=1e-6),
         "invalid input: arc steps"),
        ("inflate-f64-domain", inflate("f64", [[ptd(1e11, 0)]], 1.0, 0, 0),
         "invalid input: domain"),
        # ScalePaths' range exception lies beyond the offset domain: a harness limit.
        ("inflate-f64-range", inflate("f64", [[ptd(1e30, 0)]], 1.0, 0, 0),
         "invalid input: domain"),
        ("inflate-f64-nan", inflate("f64", [[ptd(NAN, 0)]], 1.0, 0, 0), "invalid input: domain"),
        ("inflate-defaults", inflate("i64", ok_sq, 1.0, 0, 0, at=0.5), "invalid input: defaults"),
        ("inflate-defaults-prec", inflate("i64", ok_sq, 1.0, 0, 0, ml=2.0, precision=2),
         "invalid input: defaults"),
        ("inflate-f64-defaults", inflate("f64", ok_sq and [to_d(ok_sq[0])], 1.0, 0, 0,
                                         precision=2), "invalid input: defaults"),
        ("members-domain", members([], [], [group([[pt(big, 0)]], 0, 0)], False, 1.0),
         "invalid input: domain"),
        ("members-steps", members([], [("arc_tolerance", 1e-9)], [group(ok_sq, 2, 0)], False,
                                  1e8), "invalid input: arc steps"),
        ("members-single", members([], [], [group([[pt(0, 0)], [pt(1, 1)]], 0, 0, True)], False,
                                   None), "invalid input: single"),
        ("members-ctor", {"op": "offset_members", "ctor": [hexd(1), hexd(1), True, True, True],
                          "set": [], "groups": [], "clear": False, "delta": None},
         "invalid input: ctor"),
        ("members-set", members([], [("bogus", 1.0)], [], False, None), "invalid input: set"),
        ("ellipse-steps", {"op": "ellipse", "type": "i64", "form": "center", "center": pt(0, 0),
                           "rect": None, "radius_x": hexd(1e13), "radius_y": None,
                           "steps": None}, "invalid input: steps"),
        ("ellipse-steps-given", {"op": "ellipse", "type": "f64", "form": "center",
                                 "center": ptd(0, 0), "rect": None, "radius_x": hexd(1.0),
                                 "radius_y": hexd(1.0), "steps": 2**20 + 1},
         "invalid input: steps"),
        ("ellipse-defaults", {"op": "ellipse", "type": "i64", "form": "center",
                              "center": pt(0, 0), "rect": None, "radius_x": hexd(1.0),
                              "radius_y": None, "steps": 8}, "invalid input: defaults"),
        ("ellipse-form", {"op": "ellipse", "type": "i64", "form": "rect", "center": pt(0, 0),
                          "rect": [0, 0, 1, 1], "radius_x": None, "radius_y": None,
                          "steps": None}, "invalid input: form"),
        ("rdp-nan", {"op": "rdp", "type": "i64", "form": "path", "value": [pt(0, 0)] * 6,
                     "epsilon": hexd(NAN)}, "invalid input: epsilon"),
        ("rdp-size", {"op": "rdp", "type": "i64", "form": "path",
                      "value": [pt(i, 0) for i in range(1025)], "epsilon": hexd(1.0)},
         "invalid input: size"),
        ("simplify-size", {"op": "simplify", "type": "i64", "form": "path",
                           "value": [pt(i, 0) for i in range(4097)], "epsilon": hexd(1.0),
                           "closed": None}, "invalid input: size"),
        ("mink-size", {"op": "minkowski", "type": "i64", "sum": True,
                       "pattern": [pt(i, 0) for i in range(200)],
                       "path": [pt(0, i) for i in range(100)], "closed": True,
                       "decimal_places": None}, "invalid input: size"),
        ("mink-domain", {"op": "minkowski", "type": "i64", "sum": True,
                         "pattern": [pt(ENGINE_COORD // 2 + 1, 0)], "path": [pt(0, 0)],
                         "closed": True, "decimal_places": None}, "invalid input: domain"),
        ("mink-f64-domain", {"op": "minkowski", "type": "f64", "sum": True,
                             "pattern": [ptd(5e16, 0)], "path": [ptd(0, 0)], "closed": True,
                             "decimal_places": 2}, "invalid input: domain"),
        ("mink-f64-dp20", {"op": "minkowski", "type": "f64", "sum": True,
                           "pattern": [ptd(5.0, 0)], "path": [ptd(0, 0)], "closed": False,
                           "decimal_places": 18}, "invalid input: domain"),
        ("mink-dp-i64", {"op": "minkowski", "type": "i64", "sum": True, "pattern": [],
                         "path": [], "closed": True, "decimal_places": 2},
         "invalid input: decimal_places"),
        ("detail-size", {"op": "detail", "fn": "get_next", "current": 0, "high": 1,
                         "flags": [False]}, "invalid input: detail"),
        ("detail-current", {"op": "detail", "fn": "get_next", "current": 3, "high": 1,
                            "flags": [False, True]}, "invalid input: detail"),
        ("detail-empty", {"op": "detail", "fn": "get_next", "current": 0, "high": 0,
                          "flags": []}, "invalid input: size"),
        ("make-elem", {"op": "make_path", "fn": "make_path", "elem": "f64",
                       "values": [hexd(1), hexd(2)], "array": False}, "invalid input: elem"),
        ("make-z-vector", {"op": "make_path", "fn": "make_path_z", "elem": "i64",
                           "values": [1, 2, 3], "array": False}, "invalid input: array"),
        ("make-array-size", {"op": "make_path", "fn": "make_path", "elem": "i64",
                             "values": [1, 2, 3], "array": True}, "invalid input: array size"),
        ("make-element-range", {"op": "make_path", "fn": "make_path", "elem": "u8",
                                "values": [256, 0], "array": False}, "invalid input: element"),
        ("outline-preamble", {"op": "outline", "idx": 0, "is_hole": False, "count": 0,
                              "preamble": "\t", "state": None}, "invalid input: preamble"),
        ("tree-parent", {"op": "polytree", "type": "i64", "nodes": [{"parent": 0,
                                                                      "polygon": []}],
                         "fn": "to_paths", "node": -1, "state": None}, "invalid input: parent"),
        ("tree-fn-f64", {"op": "polytree", "type": "f64", "nodes": [], "fn": "contains_children",
                         "node": -1, "state": None}, "invalid input: fn"),
        ("tree-state", {"op": "polytree", "type": "i64", "nodes": [], "fn": "to_paths",
                        "node": -1, "state": {"flags": [], "precision": 6, "width": 0,
                                              "fill": " "}}, "invalid input: state"),
        ("trim-precision-i64", {"op": "trim", "type": "i64", "path": [], "precision": 2,
                                "open": None}, "invalid input: precision"),
        ("type", {"op": "trim", "type": "i32", "path": [], "precision": None, "open": None},
         "invalid input: type"),
        ("op", {"op": "triangulate", "type": "i64"}, "invalid input: op"),
        ("op-missing", {"type": "i64"}, "invalid input: op"),
        ("fields", {"op": "version", "extra": 1}, "invalid input: fields"),
        ("double", {"op": "distance", "type": "f64", "a": ["0", "0", 0], "b": ptd(0, 0)},
         "invalid input: double"),
        ("size", {"op": "length", "type": "i64", "path": [pt(0, 0)] * 65537, "closed": None},
         "invalid input: size"),
        # Undefined reference evaluations (decided by the oracle's UBSan replay).
        ("ub-translate", {"op": "translate", "type": "i64", "form": "path",
                          "value": [pt(2**63 - 1, 0)], "dx": 1, "dy": 0},
         "undefined behaviour: translate"),
        ("ub-translate-paths", {"op": "translate", "type": "i64", "form": "paths",
                                "value": [[pt(0, -2**63)]], "dx": 0, "dy": -1},
         "undefined behaviour: translate"),
        ("ub-detail-next", {"op": "detail", "fn": "get_next", "current": 1, "high": 2,
                            "flags": [True, True, True]}, "undefined behaviour: detail"),
        ("ub-detail-prior", {"op": "detail", "fn": "get_prior", "current": 0, "high": 1,
                             "flags": [True, True]}, "undefined behaviour: detail"),
        ("ub-ellipse-i64-nan", {"op": "ellipse", "type": "i64", "form": "center",
                                "center": pt(0, 0), "rect": None, "radius_x": hexd(NAN),
                                "radius_y": hexd(1.0), "steps": 4},
         "undefined behaviour: ellipse"),
        ("ub-ellipse-nan-steps", {"op": "ellipse", "type": "f64", "form": "center",
                                  "center": ptd(0, 0), "rect": None, "radius_x": hexd(1.0),
                                  "radius_y": hexd(NAN), "steps": None},
         "undefined behaviour: ellipse"),
        ("ub-ellipse-rect-overflow", {"op": "ellipse", "type": "i64", "form": "rect",
                                      "center": None, "rect": [-2**62, 0, 2**62, 2], "radius_x":
                                      None, "radius_y": None, "steps": 8},
         "undefined behaviour: ellipse"),
        ("ub-trim", {"op": "trim", "type": "i64", "path": [pt(-2**63, 0), pt(2**63 - 1, 0),
                                                           pt(0, 5)], "precision": None,
                     "open": None}, "undefined behaviour: trim"),
        ("ub-trim-f64", {"op": "trim", "type": "f64", "path": [ptd(1e300, 0), ptd(0, 0),
                                                               ptd(0, 1)], "precision": 2,
                         "open": None}, "undefined behaviour: trim"),
        ("ub-distance", {"op": "distance", "type": "i64", "a": pt(-2**63, 0),
                         "b": pt(2**63 - 1, 0)}, "undefined behaviour: distance"),
        ("ub-length", {"op": "length", "type": "i64", "path": [pt(0, 0), pt(2**63 - 1, 0),
                                                               pt(-5, 0)], "closed": None},
         "undefined behaviour: length"),
        ("ub-near", {"op": "near_collinear", "type": "i64", "a": pt(2**63 - 1, 0),
                     "b": pt(-1, 0), "c": pt(0, 0), "value": hexd(0.1)},
         "undefined behaviour: near_collinear"),
        ("ub-contains-mid", {"op": "contains", "type": "i64",
                             # On, inside, on: the midpoint check runs and its bounds
                             # overflow (left + right > INT64_MAX).
                             "path1": [pt(2**62 - 10, 2**62), pt(2**62 + 1, 2**62 + 1),
                                       pt(2**62 + 20, 2**62)],
                             "path2": rect_path(2**62 - 10, 2**62 - 10, 30, 30)},
         "undefined behaviour: contains"),
        ("ub-simplify", {"op": "simplify", "type": "i64", "form": "path",
                         "value": [pt(-2**63, 0), pt(0, 1), pt(2**63 - 1, 0), pt(0, -1)],
                         "epsilon": hexd(1.0), "closed": None}, "undefined behaviour: simplify"),
        ("ub-rdp", {"op": "rdp", "type": "i64", "form": "path",
                    "value": [pt(-2**63, 0), pt(0, 1), pt(1, 1), pt(5, 1), pt(2**63 - 1, 0)],
                    "epsilon": hexd(1.0)}, "undefined behaviour: rdp"),
        ("ub-make-zd", {"op": "make_path", "fn": "make_path_zd", "elem": "f64",
                        "values": [hexd(1), hexd(2), hexd(1e30)], "array": True},
         "undefined behaviour: make_path"),
        ("ub-make-d-nan-z", {"op": "make_path", "fn": "make_path_zd", "elem": "f64",
                             "values": [hexd(1), hexd(2), hexd(NAN)], "array": True},
         "undefined behaviour: make_path"),
        ("ub-rect-scale", {"op": "rectclip", "type": "f64", "lines": False, "form": "paths",
                           "rect": [hexd(0), hexd(0), hexd(1e300), hexd(1)],
                           "value": [[ptd(0, 0)]], "precision": 2},
         "undefined behaviour: rectclip"),
        ("ub-rect-mid", {"op": "rectclip", "type": "i64", "lines": True, "form": "paths",
                         "rect": [1, 1, 2**63 - 1, 2**63 - 1], "value": [[pt(1, 1), pt(5, 5)]],
                         "precision": None}, "undefined behaviour: rectclip"),
        ("ub-mink-cast", {"op": "minkowski", "type": "f64", "sum": True,
                          "pattern": [ptd(1e17, 0)], "path": [ptd(0, 0)], "closed": True,
                          "decimal_places": 2}, "undefined behaviour: minkowski"),
        ("ub-mink-scale", {"op": "minkowski", "type": "f64", "sum": True,
                           "pattern": [ptd(1, 1)], "path": [ptd(0, 0)], "closed": True,
                           "decimal_places": 400}, "undefined behaviour: minkowski"),
        ("ub-inflate-open-empty", inflate("i64", [[]], 3.0, 0, 2),
         "undefined behaviour: inflate"),
        ("ub-inflate-singleton", inflate("i64", [[pt(0, 0)]], 2.0**32, 0, 2),
         "undefined behaviour: inflate"),
        ("ub-bool-f64-nan", boolean("f64", "union", [[ptd(NAN, 0), ptd(1, 0), ptd(1, 1)]], []),
         "undefined behaviour: boolean"),
        ("ub-tree-contains", {"op": "polytree", "type": "i64",
                              "nodes": [{"parent": -1, "polygon": [pt(-2**63, 0), pt(2**63 - 1, 0),
                                                                   pt(0, 5)]},
                                        {"parent": 0, "polygon": [pt(0, 1)]}],
                              "fn": "contains_children", "node": -1, "state": None},
         "undefined behaviour: polytree"),
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
    dupes = sorted({n for n in names if names.count(n) > 1})
    assert not dupes, f"duplicate case names: {dupes[:5]}"
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
