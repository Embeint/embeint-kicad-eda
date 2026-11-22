#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the fixed-seed corpus and refusal fixtures for the rtree_node unit.

Writes port/cases/<unit>.jsonl (successful operations only: every case must be a
result on both sides) and fixtures/refusals.requests.jsonl with the expected
refusal replies in fixtures/refusals.replies.jsonl.  Run from the repository root:

    python rust/kicad-rtree-node-shim/generate_cases.py [--check]

The small allocator model below only predicts node numbering so that generated
programs stay inside the reference's defined domain (no use after free, no
removal past count, no signed overflow); it is not compared with anything.
"""
import argparse
import json
from pathlib import Path
import random

UNIT = "libs.kimath.include.geometry.rtree.application.pure.p003"
ROOT = Path(__file__).resolve().parents[2]
CASES = ROOT / "port" / "cases" / f"{UNIT}.jsonl"
FIXTURES = Path(__file__).resolve().parent / "fixtures"
I32 = (-(1 << 31), (1 << 31) - 1)
I64 = (-(1 << 63), (1 << 63) - 1)
U32 = (1 << 32) - 1
DIMS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 16, 21, 22, 32, 33, 64, 65]
FANOUTS = [4, 8, 12, 16, 20, 24, 28]
CONFIGS = {  # name: (element range, NUMDIMS, MAXNODES)
    "i32.d2.m16": (I32, 2, 16), "i32.d2.m4": (I32, 2, 4), "i32.d2.m28": (I32, 2, 28),
    "i32.d3.m16": (I32, 3, 16), "i32.d3.m5": (I32, 3, 5), "i32.d3.m32": (I32, 3, 32),
    "i32.d1.m8": (I32, 1, 8), "i64.d2.m16": (I64, 2, 16), "i64.d3.m8": (I64, 3, 8),
}


def fits(value):
    return I64[0] <= value <= I64[1]


def area_ok(lo, hi):
    """ChildArea's int64 chain, in evaluation order."""
    area = 1
    for a, b in zip(lo, hi):
        extent = b - a
        if not fits(extent) or not fits(area * extent):
            return False
        area *= extent
    return True


def perimeter_ok(lo, hi):
    """ChildPerimeter's int64 chain, in evaluation order."""
    perimeter = 0
    for a, b in zip(lo, hi):
        extent = b - a
        if not fits(extent) or not fits(perimeter + extent):
            return False
        perimeter += extent
    return fits(2 * perimeter)


class Model:
    """Predicts SLAB_ALLOCATOR handles to number nodes like both adapters do."""

    def __init__(self, maxnodes):
        self.allocators = [None] * 4
        self.seen = []
        self.counts = {}
        self.payload = {}
        self.tokens = 0
        self.maxnodes = maxnodes
        self.allocated = 0

    def owner(self, handle):
        for index, allocator in enumerate(self.allocators):
            if allocator and any(page[0] == handle[0] for page in allocator["pages"]):
                return index
        return None

    def live(self, handle):
        index = self.owner(handle)
        if index is None:
            return False
        page = next(p for p in self.allocators[index]["pages"] if p[0] == handle[0])
        return handle[1] in page[1]

    def allocate(self, index):
        allocator = self.allocators[index]
        self.allocated += 1
        if allocator["free"]:
            handle = allocator["free"].pop()
        else:
            handle = None
            for token, used in allocator["pages"]:
                if len(used) < 256:
                    handle = (token, min(set(range(256)) - used))
                    break
            if handle is None:
                self.tokens += 1
                allocator["pages"].append((self.tokens, set()))
                handle = (self.tokens, 0)
        page = next(p for p in allocator["pages"] if p[0] == handle[0])
        page[1].add(handle[1])
        self.counts[handle] = 0
        self.payload[handle] = [None] * self.maxnodes
        if handle not in self.seen:
            self.seen.append(handle)
        return self.seen.index(handle)

    def free(self, index, node):
        handle = self.seen[node]
        page = next(p for p in self.allocators[index]["pages"] if p[0] == handle[0])
        page[1].discard(handle[1])
        self.allocators[index]["free"].append(handle)

    def live_ids(self):
        return [i for i, handle in enumerate(self.seen) if handle and self.live(handle)]

    def known_ids(self):
        return [i for i, handle in enumerate(self.seen) if handle]

    def owned(self, index):
        tokens = {page[0] for page in self.allocators[index]["pages"]}
        return [i for i, handle in enumerate(self.seen) if handle and handle[0] in tokens]

    def move_assign(self, target, source):
        """Returns False when the destroyed pages would leave a dangling child."""
        if target == source:
            return True
        doomed = {self.seen[i] for i in self.owned(target)}
        for i in self.live_ids():
            handle = self.seen[i]
            if handle not in doomed and any(entry in doomed for entry in self.payload[handle]):
                return False
        self.seen = [None if handle in doomed else handle for handle in self.seen]
        self.allocators[target] = self.allocators[source]
        self.allocators[source] = {"pages": [], "free": []}
        return True


def request(name, payload):
    return {"unit": UNIT, "case": name, "input": payload}


def hilbert_cases(rng):
    cases = []
    for order in range(1, 33):
        top = (1 << order) - 1
        corners = [(0, 0), (top, 0), (0, top), (top, top), (top >> 1, top), (1 << (order - 1), 0),
                   (min(top + 1, U32), 0), (U32, U32), (U32, 0)]
        for x, y in dict.fromkeys(corners):
            cases.append(request(f"hilbert-xy-o{order}-{x}-{y}",
                                 {"kind": "hilbert_xy", "order": order, "x": x, "y": y}))
    for order in range(1, 4):
        for x in range(1 << order):
            for y in range(1 << order):
                cases.append(request(f"hilbert-xy-grid-o{order}-{x}-{y}",
                                     {"kind": "hilbert_xy", "order": order, "x": x, "y": y}))
    for i in range(160):
        order = rng.choice([1, 2, 5, 16, 31, 32, rng.randint(1, 32)])
        bound = U32 if rng.random() < 0.3 else (1 << order) - 1
        cases.append(request(f"hilbert-xy-random-{i}", {"kind": "hilbert_xy", "order": order,
                                                        "x": rng.randint(0, bound),
                                                        "y": rng.randint(0, bound)}))
    for dims in DIMS:
        orders = [-7, 0, 1, 2, 3, 10, 16, 21, 31, 32] if dims != 2 else [1, 2, 16, 32]
        for order in orders:
            for variant in range(3):
                if variant == 0:
                    coords = [U32] * dims
                elif variant == 1:
                    coords = [(1 << 31) >> (k % 32) for k in range(dims)]
                else:
                    coords = [rng.randint(0, U32) for _ in range(dims)]
                cases.append(request(f"hilbert-nd-d{dims}-o{order}-v{variant}",
                                     {"kind": "hilbert_nd", "dims": dims, "order": order,
                                      "coords": coords}))
    return cases


def mask_cases(rng):
    cases = []
    for elem, (low, high) in [("i32", I32), ("i64", I64)]:
        for fanout in FANOUTS:
            counts = sorted({0, 1, fanout // 2, fanout - 1, fanout, fanout + 1, 31})
            for count in counts:
                for variant in range(4):
                    span = [5, 1000, high][variant % 3]
                    def value():
                        if variant == 3:
                            return rng.choice([low, high, 0, -1, 1, low + 1, high - 1])
                        return rng.randint(-span, span) if span != high else rng.randint(low, high)
                    lo0 = [value() for _ in range(fanout)]
                    hi0 = [value() for _ in range(fanout)]
                    lo1 = [value() for _ in range(fanout)]
                    hi1 = [value() for _ in range(fanout)]
                    qmin = [value(), value()]
                    qmax = [value(), value()]
                    if variant != 3 and rng.random() < 0.7:
                        lo0, hi0 = [min(a, b) for a, b in zip(lo0, hi0)], [max(a, b) for a, b in zip(lo0, hi0)]
                        lo1, hi1 = [min(a, b) for a, b in zip(lo1, hi1)], [max(a, b) for a, b in zip(lo1, hi1)]
                        qmin, qmax = [min(qmin[0], qmax[0]), min(qmin[1], qmax[1])], \
                                     [max(qmin[0], qmax[0]), max(qmin[1], qmax[1])]
                    cases.append(request(f"mask-{elem}-f{fanout}-c{count}-v{variant}", {
                        "kind": "overlap_mask", "elem": elem, "fanout": fanout, "count": count,
                        "min0": lo0, "max0": hi0, "min1": lo1, "max1": hi1,
                        "qmin": qmin, "qmax": qmax}))
        # Touching edges and exact query equality on every slot.
        fanout = 16
        lo = list(range(fanout))
        cases.append(request(f"mask-{elem}-touching", {
            "kind": "overlap_mask", "elem": elem, "fanout": fanout, "count": fanout,
            "min0": lo, "max0": [v + 1 for v in lo], "min1": [0] * fanout, "max1": [0] * fanout,
            "qmin": [7, 0], "qmax": [8, 0]}))
        cases.append(request(f"mask-{elem}-extremes", {
            "kind": "overlap_mask", "elem": elem, "fanout": 4, "count": 4,
            "min0": [low, low, high, 0], "max0": [high, low, high, 0],
            "min1": [low, 0, low, high], "max1": [high, 0, high, high],
            "qmin": [low, low], "qmax": [high, high]}))
    return cases


def coords(rng, low, high, dims, radius):
    lo = [rng.randint(-radius, radius) for _ in range(dims)]
    hi = [v + rng.randint(-2, radius) for v in lo]
    return [max(low, min(high, v)) for v in lo], [max(low, min(high, v)) for v in hi]


def fixed_programs(name, element, dims, maxnodes):
    low, high = element
    zero = [0] * dims
    steps = [{"op": "layout"}, {"op": "new", "a": 0}, {"op": "memory", "a": 0},
             {"op": "state", "a": 0}, {"op": "alloc", "a": 0}, {"op": "snapshot", "node": 0},
             {"op": "predicates", "node": 0}, {"op": "enclosing", "node": 0},
             {"op": "mask", "node": 0, "min": zero, "max": zero},
             {"op": "memory", "a": 0}, {"op": "state", "a": 0}]
    # Fill every slot with distinct boxes, insertion boxes and items; exercise each query.
    for slot in range(maxnodes):
        lo = [slot * 10 - 5 * d for d in range(dims)]
        hi = [v + 3 + slot % 4 for v in lo]
        steps += [{"op": "set_bounds", "node": 0, "slot": slot, "min": lo, "max": hi},
                  {"op": "set_insert", "node": 0, "slot": slot, "min": [v - 1 for v in lo],
                   "max": [v + 1 for v in hi]},
                  {"op": "set_data", "node": 0, "slot": slot, "item": slot % 64}]
    for count in sorted({-5, 0, 1, maxnodes // 2, maxnodes - 1, maxnodes}):
        steps += [{"op": "set_count", "node": 0, "value": count},
                  {"op": "predicates", "node": 0}, {"op": "enclosing", "node": 0}]
        # OverlapMask2D shifts by count, so a negative count is outside its domain.
        if count >= 0 or dims != 2:
            steps.append({"op": "mask", "node": 0, "min": [20] * dims, "max": [60] * dims})
    if dims == 2:
        for count in range(maxnodes + 1, 32):
            steps += [{"op": "set_count", "node": 0, "value": count},
                      {"op": "predicates", "node": 0},
                      {"op": "mask", "node": 0, "min": [low] * dims, "max": [high] * dims}]
    for count in [maxnodes + 1, 1 << 30, high if high < (1 << 31) else (1 << 31) - 1,
                  -(1 << 31), maxnodes * 2 // 5 - 1, maxnodes * 2 // 5]:
        steps += [{"op": "set_count", "node": 0, "value": count}, {"op": "predicates", "node": 0}]
    for level in [-(1 << 31), -1, 0, 1, 7, (1 << 31) - 1]:
        steps += [{"op": "set_level", "node": 0, "value": level}, {"op": "predicates", "node": 0}]
    steps += [{"op": "set_level", "node": 0, "value": 0},
              {"op": "set_count", "node": 0, "value": maxnodes}]
    for slot in range(maxnodes):
        query_lo = [slot * 9 - 3] * dims
        query_hi = [slot * 11 + 4] * dims
        steps += [{"op": "area", "node": 0, "slot": slot}, {"op": "perimeter", "node": 0, "slot": slot},
                  {"op": "get_bounds", "node": 0, "slot": slot}, {"op": "get_insert", "node": 0, "slot": slot},
                  {"op": "overlaps", "node": 0, "slot": slot, "min": query_lo, "max": query_hi},
                  {"op": "overlap_area", "node": 0, "slot": slot, "min": query_lo, "max": query_hi},
                  {"op": "enlargement", "node": 0, "slot": slot, "min": query_lo, "max": query_hi}]
    # Inverted and degenerate boxes: negative extents and zero area.
    steps += [{"op": "set_bounds", "node": 0, "slot": 0, "min": [5] * dims, "max": [-5] * dims},
              {"op": "area", "node": 0, "slot": 0}, {"op": "perimeter", "node": 0, "slot": 0},
              {"op": "overlap_area", "node": 0, "slot": 0, "min": [-1] * dims, "max": [1] * dims},
              {"op": "enlargement", "node": 0, "slot": 0, "min": [-1] * dims, "max": [1] * dims},
              {"op": "set_bounds", "node": 0, "slot": 1, "min": [3] * dims, "max": [3] * dims},
              {"op": "area", "node": 0, "slot": 1},
              {"op": "overlaps", "node": 0, "slot": 1, "min": [3] * dims, "max": [3] * dims},
              {"op": "overlap_area", "node": 0, "slot": 1, "min": [3] * dims, "max": [9] * dims}]
    # Extremes on one axis with a unit extent elsewhere stay inside int64.
    extreme_lo = [low] + [0] * (dims - 1)
    extreme_hi = [high if element == I32 else -1] + [1] * (dims - 1)
    if area_ok(extreme_lo, extreme_hi):
        steps += [{"op": "set_bounds", "node": 0, "slot": 2, "min": extreme_lo, "max": extreme_hi},
                  {"op": "area", "node": 0, "slot": 2}]
        if perimeter_ok(extreme_lo, extreme_hi):
            steps.append({"op": "perimeter", "node": 0, "slot": 2})
        steps += [
                  {"op": "overlaps", "node": 0, "slot": 2, "min": [high] * dims, "max": [high] * dims},
                  {"op": "overlap_area", "node": 0, "slot": 2, "min": extreme_lo, "max": extreme_hi},
                  {"op": "enclosing", "node": 0},
                  {"op": "mask", "node": 0, "min": [low] * dims, "max": [low] * dims}]
    # RemoveChild: middle, first, last and the only child; stale last slots stay.
    steps += [{"op": "snapshot", "node": 0},
              {"op": "remove", "node": 0, "slot": maxnodes // 2}, {"op": "snapshot", "node": 0},
              {"op": "remove", "node": 0, "slot": 0}, {"op": "snapshot", "node": 0},
              {"op": "remove", "node": 0, "slot": maxnodes - 3 if maxnodes >= 3 else 0},
              {"op": "snapshot", "node": 0},
              {"op": "set_count", "node": 0, "value": 1}, {"op": "remove", "node": 0, "slot": 0},
              {"op": "snapshot", "node": 0}, {"op": "predicates", "node": 0},
              {"op": "enclosing", "node": 0}]
    # Internal node: children, null children, removal copies the child pointers.
    steps += [{"op": "alloc", "a": 0}, {"op": "alloc", "a": 0}, {"op": "alloc", "a": 0},
              {"op": "set_level", "node": 1, "value": 1},
              {"op": "set_child", "node": 1, "slot": 0, "child": 2},
              {"op": "set_child", "node": 1, "slot": 1, "child": None},
              {"op": "set_child", "node": 1, "slot": 2 % maxnodes, "child": 3},
              {"op": "set_child", "node": 1, "slot": maxnodes - 1, "child": 0},
              {"op": "set_count", "node": 1, "value": min(3, maxnodes)},
              {"op": "snapshot", "node": 1},
              {"op": "remove", "node": 1, "slot": 0}, {"op": "snapshot", "node": 1},
              {"op": "set_data", "node": 1, "slot": 0, "item": None},
              {"op": "set_child", "node": 2, "slot": 0, "child": 1},
              {"op": "snapshot", "node": 2}, {"op": "predicates", "node": 1}]
    # Copy-on-write reference counts, including two's complement wrap.
    steps += [{"op": "ref_load", "node": 1}, {"op": "ref_add", "node": 1, "value": 1},
              {"op": "ref_sub", "node": 1, "value": 2}, {"op": "ref_load", "node": 1},
              {"op": "ref_store", "node": 1, "value": (1 << 31) - 1},
              {"op": "ref_add", "node": 1, "value": 1}, {"op": "ref_load", "node": 1},
              {"op": "ref_sub", "node": 1, "value": 1}, {"op": "ref_load", "node": 1},
              {"op": "ref_add", "node": 1, "value": -(1 << 31)}, {"op": "ref_load", "node": 1}]
    yield f"slab-{name}-node", steps

    # Allocator: pages, LIFO reuse, re-construction, free-list capacity growth.
    steps = [{"op": "new", "a": 0}, {"op": "free", "a": 0, "node": None},
             {"op": "memory", "a": 0}, {"op": "alloc_many", "a": 0, "n": 256},
             {"op": "state", "a": 0}, {"op": "alloc", "a": 0}, {"op": "state", "a": 0},
             {"op": "find", "a": 0, "node": 0}, {"op": "find", "a": 0, "node": 255},
             {"op": "find", "a": 0, "node": 256}]
    for node in [5, 9, 200, 0, 256, 17, 18, 19, 20]:
        steps += [{"op": "free", "a": 0, "node": node}, {"op": "memory", "a": 0},
                  {"op": "owns", "a": 0, "node": node}, {"op": "find", "a": 0, "node": node}]
    steps += [{"op": "state", "a": 0}, {"op": "alloc", "a": 0}, {"op": "snapshot", "node": 20},
              {"op": "set_count", "node": 20, "value": 3}, {"op": "ref_add", "node": 20, "value": 4},
              {"op": "free", "a": 0, "node": 20}, {"op": "alloc", "a": 0},
              {"op": "snapshot", "node": 20}, {"op": "alloc_many", "a": 0, "n": 9},
              {"op": "state", "a": 0}, {"op": "alloc_many", "a": 0, "n": 300}, {"op": "state", "a": 0}]
    for node in range(30, 63):
        steps += [{"op": "free", "a": 0, "node": node}]
    steps += [{"op": "state", "a": 0}, {"op": "alloc_many", "a": 0, "n": 40}, {"op": "state", "a": 0}]
    yield f"slab-{name}-pages", steps

    # Moves: construction, assignment, self assignment, ownership across allocators.
    steps = [{"op": "new", "a": 0}, {"op": "alloc_many", "a": 0, "n": 3},
             {"op": "free", "a": 0, "node": 1}, {"op": "new", "a": 1}, {"op": "alloc", "a": 1},
             {"op": "owns", "a": 0, "node": 0}, {"op": "owns", "a": 1, "node": 0},
             {"op": "owns", "a": 0, "node": 3}, {"op": "find", "a": 1, "node": 0},
             {"op": "move_new", "a": 2, "src": 0}, {"op": "state", "a": 0}, {"op": "state", "a": 2},
             {"op": "owns", "a": 0, "node": 0}, {"op": "owns", "a": 2, "node": 0},
             {"op": "owns", "a": 2, "node": 1}, {"op": "find", "a": 2, "node": 2},
             {"op": "snapshot", "node": 2}, {"op": "alloc", "a": 0}, {"op": "state", "a": 0},
             {"op": "alloc", "a": 2}, {"op": "state", "a": 2},
             {"op": "move_assign", "a": 2, "src": 2}, {"op": "state", "a": 2},
             {"op": "move_assign", "a": 1, "src": 2}, {"op": "state", "a": 1}, {"op": "state", "a": 2},
             {"op": "owns", "a": 1, "node": 0}, {"op": "owns", "a": 1, "node": 1},
             {"op": "owns", "a": 2, "node": 0}, {"op": "memory", "a": 1}, {"op": "memory", "a": 2},
             {"op": "free", "a": 1, "node": 2}, {"op": "alloc", "a": 1}, {"op": "state", "a": 1},
             {"op": "move_assign", "a": 0, "src": 1}, {"op": "state", "a": 0}, {"op": "state", "a": 1},
             {"op": "alloc", "a": 1}, {"op": "move_new", "a": 3, "src": 1}, {"op": "state", "a": 3},
             {"op": "alloc", "a": 1}, {"op": "find", "a": 1, "node": 0}, {"op": "find", "a": 0, "node": 0}]
    yield f"slab-{name}-moves", steps


def random_program(rng, name, element, dims, maxnodes, index):
    low, high = element
    radius = {1: high if element == I32 else 1 << 61, 2: 1 << 30, 3: 1 << 20}[dims]
    model = Model(maxnodes)
    steps = [{"op": "new", "a": 0}]
    model.allocators[0] = {"pages": [], "free": []}

    def any_seen():
        return rng.choice(model.known_ids())

    while len(steps) < 400:
        live = model.live_ids()
        roll = rng.random()
        existing = [i for i, a in enumerate(model.allocators) if a is not None]
        if roll < 0.12 or not live:
            a = rng.choice(existing)
            if rng.random() < 0.1 and model.allocated < 3000:
                n = rng.randint(1, 300)
                steps.append({"op": "alloc_many", "a": a, "n": n})
                for _ in range(n):
                    model.allocate(a)
            else:
                steps.append({"op": "alloc", "a": a})
                model.allocate(a)
            continue
        if roll < 0.2:
            node = rng.choice(live)
            a = model.owner(model.seen[node])
            steps.append({"op": "free", "a": a, "node": node})
            model.free(a, node)
            continue
        if roll < 0.23:
            empty = [i for i, a in enumerate(model.allocators) if a is None]
            if empty and rng.random() < 0.5:
                target, source = rng.choice(empty), rng.choice(existing)
                steps.append({"op": "move_new", "a": target, "src": source})
                model.allocators[target] = model.allocators[source]
                model.allocators[source] = {"pages": [], "free": []}
            else:
                target, source = rng.choice(existing), rng.choice(existing)
                if model.move_assign(target, source):
                    steps.append({"op": "move_assign", "a": target, "src": source})
            continue
        if roll < 0.27:
            a = rng.choice(existing)
            op = rng.choice(["state", "memory", "owns", "find"])
            step = {"op": op, "a": a}
            if op in ("owns", "find"):
                step["node"] = any_seen()
            steps.append(step)
            continue
        node = rng.choice(live)
        handle = model.seen[node]
        slot = rng.randrange(maxnodes)
        lo, hi = coords(rng, low, high, dims, radius)
        op = rng.choice(["set_bounds", "set_bounds", "set_insert", "set_data", "set_child",
                         "set_count", "set_level", "remove", "area", "perimeter", "overlaps",
                         "overlap_area", "enlargement", "mask", "enclosing", "predicates",
                         "get_bounds", "get_insert", "snapshot", "ref_add", "ref_sub", "ref_load"])
        if op in ("set_bounds", "set_insert", "overlaps", "overlap_area", "enlargement"):
            steps.append({"op": op, "node": node, "slot": slot, "min": lo, "max": hi})
        elif op == "set_data":
            item = None if rng.random() < 0.1 else rng.randrange(64)
            steps.append({"op": op, "node": node, "slot": slot, "item": item})
            model.payload[handle][slot] = None
        elif op == "set_child":
            child = None if rng.random() < 0.1 else any_seen()
            steps.append({"op": op, "node": node, "slot": slot, "child": child})
            model.payload[handle][slot] = None if child is None else model.seen[child]
        elif op == "set_count":
            value = rng.randint(0, maxnodes)
            steps.append({"op": op, "node": node, "value": value})
            model.counts[handle] = value
        elif op == "set_level":
            steps.append({"op": op, "node": node, "value": rng.choice([0, 0, 1, 2, -1])})
        elif op == "remove":
            if model.counts[handle] > 0:
                position = rng.randrange(model.counts[handle])
                last = model.counts[handle] - 1
                steps.append({"op": op, "node": node, "slot": position})
                model.payload[handle][position] = model.payload[handle][last]
                model.counts[handle] -= 1
        elif op in ("area", "perimeter", "get_bounds", "get_insert"):
            steps.append({"op": op, "node": node, "slot": slot})
        elif op == "mask":
            steps.append({"op": op, "node": node, "min": lo, "max": hi})
        elif op in ("ref_add", "ref_sub"):
            steps.append({"op": op, "node": node, "value": rng.randint(-3, 3)})
        else:
            steps.append({"op": op, "node": node})
    for a, allocator in enumerate(model.allocators):
        if allocator is not None:
            steps.append({"op": "state", "a": a})
    return f"slab-{name}-random-{index}", steps


def corpus():
    rng = random.Random(20261122)
    cases = hilbert_cases(rng) + mask_cases(rng)
    for name, (element, dims, maxnodes) in CONFIGS.items():
        for case, steps in fixed_programs(name, element, dims, maxnodes):
            cases.append(request(case, {"kind": "slab", "config": name, "steps": steps}))
        for index in range(4):
            case, steps = random_program(rng, name, element, dims, maxnodes, index)
            cases.append(request(case, {"kind": "slab", "config": name, "steps": steps}))
    # Appended after the original corpus so every earlier case keeps its bytes and
    # index; see review_cases.
    return cases + review_cases()


def review_cases():
    """Regressions for the first independent review of PR 52, in source order."""
    rng = random.Random(52)
    cases = []
    # Morton fallback: above 64 dimensions no bit is selected and the loop never
    # shifts, so every order except INT_MIN (bits - 1 overflows) returns 0.
    for order in [33, 64, 1000, I32[1], 1, 0, -1, I32[0] + 1]:
        for variant, coords in [("zero", [0] * 65), ("max", [U32] * 65),
                                ("random", [rng.randint(0, U32) for _ in range(65)])]:
            cases.append(request(f"review-hilbert-nd-d65-o{order}-{variant}",
                                 {"kind": "hilbert_nd", "dims": 65, "order": order,
                                  "coords": coords}))
    # The lowest order whose loop initializer is still defined.
    for dims in [d for d in DIMS if d != 2]:
        cases.append(request(f"review-hilbert-nd-d{dims}-o-min-plus-one", {
            "kind": "hilbert_nd", "dims": dims, "order": I32[0] + 1,
            "coords": [rng.randint(0, U32) for _ in range(dims)]}))
    return cases


def failure(case, message, kind="invalid_input", unit=UNIT):
    return {"unit": unit, "case": case, "error": {"kind": kind, "message": message}}


def refusals():
    """Malformed and out-of-domain requests with the reply both adapters must give."""
    checks = []

    def add(case, payload, message):
        checks.append((request(case, payload), failure(case, message)))

    def slab(case, steps, message, config="i32.d2.m16"):
        add(case, {"kind": "slab", "config": config, "steps": steps}, message)

    ok = {"kind": "hilbert_xy", "order": 1, "x": 0, "y": 0}
    add("kind-missing", {}, "expected kind string")
    add("kind-type", {"kind": 1}, "expected kind string")
    add("kind-unknown", {"kind": "tree"}, "unknown kind")
    add("xy-extra", dict(ok, extra=0), "unexpected or missing field")
    add("xy-missing", {"kind": "hilbert_xy", "order": 1, "x": 0}, "unexpected or missing field")
    for order in [0, -1, 33, -(1 << 31), (1 << 31) - 1]:
        add(f"xy-order-{order}", dict(ok, order=order), "order outside reference domain")
    for value, name in [((1 << 31), "order"), (1.0, "order"), ("1", "order"), (None, "order")]:
        add(f"xy-order-type-{value}", dict(ok, order=value), f"integer out of range: {name}")
    for value in [-1, U32 + 1, 0.0, True]:
        add(f"xy-x-{value}", dict(ok, x=value), "integer out of range: x")
    add("xy-y-big", dict(ok, y=1 << 64 - 1), "integer out of range: y")
    nd = {"kind": "hilbert_nd", "dims": 3, "order": 4, "coords": [0, 0, 0]}
    add("nd-dims-zero", dict(nd, dims=0), "integer out of range: dims")
    add("nd-dims-big", dict(nd, dims=1025), "integer out of range: dims")
    add("nd-dims-unsupported", dict(nd, dims=10, coords=[0] * 10), "unsupported dimensions")
    add("nd-coords-length", dict(nd, coords=[0, 0]), "invalid coordinates")
    add("nd-coords-negative", dict(nd, coords=[0, -1, 0]), "invalid coordinates")
    add("nd-coords-wide", dict(nd, coords=[0, U32 + 1, 0]), "invalid coordinates")
    add("nd-coords-type", dict(nd, coords="000"), "invalid coordinates")
    add("nd-order-33", dict(nd, order=33), "order outside reference domain")
    add("nd-2d-order-0", dict(nd, dims=2, order=0, coords=[0, 0]), "order outside reference domain")
    add("nd-order-before-dims", dict(nd, dims=10, order=40, coords=[0] * 10),
        "order outside reference domain")
    mask = {"kind": "overlap_mask", "elem": "i32", "fanout": 4, "count": 4, "min0": [0] * 4,
            "max0": [0] * 4, "min1": [0] * 4, "max1": [0] * 4, "qmin": [0, 0], "qmax": [0, 0]}
    add("mask-elem", dict(mask, elem="u32"), "unknown element type")
    add("mask-elem-type", dict(mask, elem=4), "unknown element type")
    add("mask-fanout", dict(mask, fanout=32), "unsupported fanout")
    add("mask-fanout-range", dict(mask, fanout=-4), "integer out of range: fanout")
    for count in [-1, 32, 1 << 30]:
        add(f"mask-count-{count}", dict(mask, count=count), "count outside reference domain")
    add("mask-count-type", dict(mask, count=1 << 31), "integer out of range: count")
    add("mask-lanes", dict(mask, min1=[0] * 3), "invalid coordinates")
    add("mask-i32-range", dict(mask, max0=[0, 0, 0, 1 << 31]), "invalid coordinates")
    add("mask-i64-range", dict(mask, elem="i64", max0=[0, 0, 0, 1 << 63]), "invalid coordinates")
    add("mask-query", dict(mask, qmax=[0]), "invalid coordinates")
    add("mask-elem-before-fanout", dict(mask, elem="f32", fanout=5), "unknown element type")
    add("slab-config", {"kind": "slab", "config": "i32.d2.m8", "steps": [{"op": "layout"}]},
        "unknown config")
    add("slab-config-type", {"kind": "slab", "config": 2, "steps": [{"op": "layout"}]},
        "unknown config")
    add("slab-steps-type", {"kind": "slab", "config": "i32.d2.m16", "steps": {}},
        "expected steps array")
    add("slab-steps-empty", {"kind": "slab", "config": "i32.d2.m16", "steps": []},
        "steps outside 1..4096")
    add("slab-steps-many", {"kind": "slab", "config": "i32.d2.m16",
                            "steps": [{"op": "layout"}] * 4097}, "steps outside 1..4096")
    new = {"op": "new", "a": 0}
    alloc = {"op": "alloc", "a": 0}
    slab("op-missing", [{}], "expected op string")
    slab("op-type", [{"op": 1}], "expected op string")
    slab("step-type", [[]], "expected op string")
    slab("op-unknown", [new, {"op": "grow", "node": 0}], "unknown operation")
    slab("layout-extra", [{"op": "layout", "a": 0}], "unexpected or missing field")
    slab("allocator-range", [{"op": "new", "a": 4}], "integer out of range: a")
    slab("allocator-missing", [alloc], "allocator does not exist")
    slab("allocator-exists", [new, new], "allocator exists")
    slab("move-new-exists", [new, {"op": "move_new", "a": 0, "src": 0}], "allocator exists")
    slab("move-new-source", [{"op": "move_new", "a": 1, "src": 0}], "allocator does not exist")
    slab("move-assign-target", [new, {"op": "move_assign", "a": 1, "src": 0}],
         "allocator does not exist")
    slab("alloc-many-range", [new, {"op": "alloc_many", "a": 0, "n": 1025}],
         "integer out of range: n")
    slab("alloc-many-zero", [new, {"op": "alloc_many", "a": 0, "n": 0}], "integer out of range: n")
    slab("alloc-budget", [new] + [{"op": "alloc_many", "a": 0, "n": 1024}] * 4 + [alloc],
         "allocation budget exceeded")
    slab("node-missing", [new, {"op": "snapshot", "node": 0}], "node does not exist")
    slab("node-range", [new, alloc, {"op": "snapshot", "node": 4096}], "integer out of range: node")
    slab("node-null", [new, alloc, {"op": "owns", "a": 0, "node": None}],
         "integer out of range: node")
    slab("use-after-free", [new, alloc, {"op": "free", "a": 0, "node": 0},
                            {"op": "snapshot", "node": 0}], "node is not live")
    slab("double-free", [new, alloc, {"op": "free", "a": 0, "node": 0},
                         {"op": "free", "a": 0, "node": 0}], "node is not live")
    slab("foreign-free", [new, alloc, {"op": "new", "a": 1}, {"op": "free", "a": 1, "node": 0}],
         "node is not owned")
    slab("free-before-allocator", [new, alloc, {"op": "free", "a": 2, "node": 0}],
         "allocator does not exist")
    slab("set-after-free", [new, alloc, {"op": "free", "a": 0, "node": 0},
                            {"op": "set_count", "node": 0, "value": 1}], "node is not live")
    node = [new, alloc]
    for slot in [-1, 16]:
        slab(f"slot-{slot}", node + [{"op": "area", "node": 0, "slot": slot}], "slot outside node")
    slab("slot-type", node + [{"op": "area", "node": 0, "slot": "0"}], "integer out of range: slot")
    slab("slot-before-live", [new, alloc, {"op": "free", "a": 0, "node": 0},
                              {"op": "area", "node": 0, "slot": 99}], "node is not live")
    slab("coords-length", node + [{"op": "set_bounds", "node": 0, "slot": 0, "min": [0],
                                   "max": [0, 0]}], "invalid coordinates")
    slab("coords-range", node + [{"op": "set_bounds", "node": 0, "slot": 0, "min": [0, 1 << 31],
                                  "max": [0, 0]}], "invalid coordinates")
    slab("item-range", node + [{"op": "set_data", "node": 0, "slot": 0, "item": 64}],
         "integer out of range: item")
    slab("child-missing", node + [{"op": "set_child", "node": 0, "slot": 0, "child": 5}],
         "node does not exist")
    slab("value-range", node + [{"op": "set_count", "node": 0, "value": 1 << 31}],
         "integer out of range: value")
    slab("remove-empty", node + [{"op": "remove", "node": 0, "slot": 0}], "slot outside node")
    slab("remove-past-count", node + [{"op": "set_count", "node": 0, "value": 2},
                                      {"op": "remove", "node": 0, "slot": 2}], "slot outside node")
    for count, op in [(17, "remove"), (17, "enclosing"), (-1, "mask"), (32, "mask")]:
        step = {"op": op, "node": 0}
        if op == "remove":
            step["slot"] = 0
        if op == "mask":
            step.update(min=[0, 0], max=[0, 0])
        slab(f"count-{op}-{count}", node + [{"op": "set_count", "node": 0, "value": count}, step],
             "count outside reference domain")
    slab("count-mask-3d", node + [{"op": "set_count", "node": 0, "value": 17},
                                  {"op": "mask", "node": 0, "min": [0] * 3, "max": [0] * 3}],
         "count outside reference domain", config="i32.d3.m16")
    # Signed 64-bit overflow is undefined in the reference: refused, never wrapped.
    full = {"op": "set_bounds", "node": 0, "slot": 0, "min": [I32[0], I32[0]], "max": [I32[1], I32[1]]}
    over = "arithmetic overflow outside reference domain"
    slab("overflow-area", node + [full, {"op": "area", "node": 0, "slot": 0}], over)
    slab("overflow-enlargement", node + [{"op": "set_bounds", "node": 0, "slot": 0, "min": [0, 0],
                                          "max": [1, 1]},
                                         {"op": "enlargement", "node": 0, "slot": 0,
                                          "min": [I32[0], I32[0]], "max": [I32[1], I32[1]]}], over)
    slab("overflow-overlap-area", node + [full, {"op": "overlap_area", "node": 0, "slot": 0,
                                                 "min": [I32[0], I32[0]], "max": [I32[1], I32[1]]}],
         over)
    slab("overflow-area-3d", node + [{"op": "set_bounds", "node": 0, "slot": 0,
                                      "min": [-(1 << 21)] * 3, "max": [1 << 21] * 3},
                                     {"op": "area", "node": 0, "slot": 0}], over, config="i32.d3.m16")
    slab("overflow-i64-extent", node + [{"op": "set_bounds", "node": 0, "slot": 0,
                                         "min": [I64[0], 0], "max": [0, 1]},
                                        {"op": "perimeter", "node": 0, "slot": 0}], over,
         config="i64.d2.m16")
    slab("overflow-i64-perimeter", node + [{"op": "set_bounds", "node": 0, "slot": 0,
                                            "min": [0, 0], "max": [I64[1], I64[1]]},
                                           {"op": "perimeter", "node": 0, "slot": 0}], over,
         config="i64.d2.m16")
    slab("overflow-i64-doubled", node + [{"op": "set_bounds", "node": 0, "slot": 0,
                                          "min": [0, 0], "max": [1 << 62, 0]},
                                         {"op": "perimeter", "node": 0, "slot": 0}], over,
         config="i64.d2.m16")
    # A page-destroying move assignment retires its nodes and may not leave a live
    # node elsewhere pointing into them.
    retire = [new, alloc, {"op": "new", "a": 1}, {"op": "alloc", "a": 1}, {"op": "new", "a": 2}]
    slab("move-dangling-child", retire + [{"op": "set_child", "node": 0, "slot": 0, "child": 1},
                                          {"op": "move_assign", "a": 1, "src": 2}],
         "move assignment would leave a dangling child")
    slab("move-destroyed-node", retire + [{"op": "move_assign", "a": 1, "src": 2},
                                          {"op": "snapshot", "node": 1}], "node is destroyed")
    slab("move-destroyed-owns", retire + [{"op": "move_assign", "a": 1, "src": 2},
                                          {"op": "owns", "a": 0, "node": 1}], "node is destroyed")
    slab("move-destroyed-child", retire + [{"op": "move_assign", "a": 1, "src": 2},
                                           {"op": "set_child", "node": 0, "slot": 0, "child": 1}],
         "node is destroyed")
    # Review regressions: the Morton loop initializer overflows at INT_MIN; a shift of
    # aOrder - 1 >= 32 happens whenever the loop runs (dimensions 1..64).
    nd_min = "order outside reference domain"
    for dims in [1, 3, 64, 65]:
        add(f"review-nd-d{dims}-order-min", dict(nd, dims=dims, coords=[0] * dims, order=I32[0]),
            nd_min)
    for dims in [1, 33, 64]:
        add(f"review-nd-d{dims}-order-33", dict(nd, dims=dims, coords=[0] * dims, order=33), nd_min)
    # Envelope-level refusals use the shared transport messages.
    checks.append(({"unit": "other.unit", "case": "unit", "input": {}},
                   failure("unit", "unit has no registered shim", "unknown_unit", "other.unit")))
    return checks


def jsonl(records):
    return "".join(json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n"
                   for record in records)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if outputs differ")
    args = parser.parse_args()
    checks = refusals()
    outputs = {
        CASES: jsonl(corpus()),
        FIXTURES / "refusals.requests.jsonl": jsonl(request for request, _ in checks),
        FIXTURES / "refusals.replies.jsonl": jsonl(reply for _, reply in checks),
    }
    names = [json.loads(line)["case"] for line in outputs[CASES].splitlines()]
    assert len(names) == len(set(names)), "duplicate case IDs"
    stale = [path for path, text in outputs.items()
             if not path.exists() or path.read_bytes() != text.encode()]
    if args.check:
        if stale:
            raise SystemExit("stale generated files: " + ", ".join(map(str, stale)))
        return
    FIXTURES.mkdir(exist_ok=True)
    for path, text in outputs.items():
        path.write_bytes(text.encode())
    print(f"{len(names)} cases, {len(checks)} refusals")


if __name__ == "__main__":
    main()
