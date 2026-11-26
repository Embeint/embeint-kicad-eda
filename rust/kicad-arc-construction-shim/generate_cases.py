#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for the KIGEOM arc construction helpers; inputs only.

The cases are the upstream QA inputs of qa/tests/libs/kimath/geometry/test_arc_construction.cpp
(public test data at the pin) plus fixed-seed families for every operation: degenerate,
collinear, near-collinear, tiny, huge, signed, radius, toIU-limit and tangent special values.

Python floats are IEEE binary64 with correctly rounded +, -, *, / and sqrt; std::fma is
evaluated exactly through fractions and std::hypot follows the glibc 2.39 finite kernel, so the
mirror below decides which source branch each request reaches. It labels branches for the
evidence table and checks every reachable branch is hit; it never writes a result.

Tangents are given as exact IEEE bits. A tangent whose VECTOR2<double>::EuclideanNorm reaches
std::hypot with an infinite or NaN operand is a real C++ operation that still defers to the
vector owner (libs.kimath.include.math.application.ui.p002); those cases stay in the corpus and
are counted as deferrals, never dropped.
"""
import argparse
from collections import Counter
from fractions import Fraction
import json
import math
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.arc_construction.application.ui.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'arc-construction-controls.jsonl'
SEED = 0xA2C5EED
INT_MIN, INT_MAX = -2 ** 31, 2 ** 31 - 1
LIMIT = float(INT_MAX - 1)


def bits(v):
    return '%016x' % struct.unpack('<Q', struct.pack('<d', v))[0]


def from_bits(s):
    return struct.unpack('<d', struct.pack('<Q', int(s, 16)))[0]


def fma(a, b, c):
    exact = Fraction(a) * Fraction(b) + Fraction(c)
    if exact == 0:
        return 0.0
    return float(exact)


def cross_z(ax, ay, bx, by):
    p = ay * bx
    e = fma(ay, bx, -p)
    return fma(ax, by, -p) - e


def hypot_finite(x, y):
    """glibc 2.39 e_hypot.c, non-FMA branches (the same order as rust/kicad-math/src/hypot.rs)."""
    x, y = abs(x), abs(y)
    ax, ay = (y, x) if x < y else (x, y)
    scale, large = 2.0 ** -600, 2.0 ** 511
    tiny, eps = 2.0 ** -459, 2.0 ** -54

    def kernel(ax, ay):
        h = math.sqrt(ax * ax + ay * ay)
        if h <= 2.0 * ay:
            delta = h - ay
            t1 = ax * (2.0 * delta - ax)
            t2 = (delta - 2.0 * (ax - ay)) * delta
        else:
            delta = h - ax
            t1 = 2.0 * delta * (ax - 2.0 * ay)
            t2 = (4.0 * delta - ay) * ay + delta * delta
        return h - (t1 + t2) / (2.0 * h)

    if ax > large:
        if ay <= ax * eps:
            return ax + ay
        return kernel(ax * scale, ay * scale) / scale
    if ay < tiny:
        if ax >= ay / eps:
            return ax + ay
        return kernel(ax / scale, ay / scale) * scale
    if ay <= ax * eps:
        return ax + ay
    return kernel(ax, ay)


class Deferred(Exception):
    pass


def norm(x, y, labels, where):
    """VECTOR2<double>::EuclideanNorm."""
    if abs(x) == abs(y):
        labels.add(f'{where}-norm-diagonal')
        return abs(x) * math.sqrt(2.0) if not math.isinf(x) else math.inf
    if x == 0:
        labels.add(f'{where}-norm-axis')
        return abs(y)
    if y == 0:
        labels.add(f'{where}-norm-axis')
        return abs(x)
    if math.isfinite(x) and math.isfinite(y):
        labels.add(f'{where}-norm-hypot')
        return hypot_finite(x, y)
    labels.add(f'{where}-norm-deferred')
    raise Deferred()


def kiround(v):
    a = abs(v)
    f = math.floor(a)
    r = f + 1 if a - f >= 0.5 else f
    return int(-r if v < 0 else r)


def to_iu(x, y):
    if not math.isfinite(x) or not math.isfinite(y) or abs(x) > LIMIT or abs(y) > LIMIT:
        return None
    return (kiround(x), kiround(y))


def build(s, e, side, ct, radius, labels):
    cx, cy = float(e[0]) - s[0], float(e[1]) - s[1]
    ln = norm(cx, cy, labels, 'build')
    if ln <= 0.0 or not math.isfinite(radius) or radius <= 0.0:
        labels.add('build-zero-chord' if ln <= 0.0 else 'build-bad-radius')
        return
    nx, ny = -cy / ln, cx / ln
    mx, my = (float(s[0]) + e[0]) * 0.5, (float(s[1]) + e[1]) * 0.5
    half = ln * 0.5
    off = abs(ct)
    if side * ct <= 0.0:
        labels.add('build-height-shallow')
        h = half * half / (radius + off)
    else:
        labels.add('build-height-deep')
        h = radius + off
    mid = to_iu(mx + nx * (side * h), my + ny * (side * h))
    if mid is None:
        labels.add('build-toiu-fail')
        return
    if cross_z(cx, cy, float(mid[0]) - s[0], float(mid[1]) - s[1]) == 0.0:
        labels.add('build-apex-on-chord')
        return
    labels.add('build-valid')
    labels.add('build-side-pos' if side > 0 else 'build-side-neg')


def project(s, e, c, labels):
    cx, cy = float(e[0]) - s[0], float(e[1]) - s[1]
    ln = norm(cx, cy, labels, 'project')
    if ln <= 0.0:
        labels.add('project-zero-chord')
        return c
    nx, ny = -cy / ln, cx / ln
    mx, my = (float(s[0]) + e[0]) * 0.5, (float(s[1]) + e[1]) * 0.5
    rx, ry = c[0] - mx, c[1] - my
    dot = rx * nx + ry * ny
    p = to_iu(mx + nx * dot, my + ny * dot)
    if p is None:
        labels.add('project-toiu-fail')
        return c
    labels.add('project-ok')
    return p


def through(s, o, e, labels):
    bx, by = float(e[0]) - s[0], float(e[1]) - s[1]
    cx, cy = float(o[0]) - s[0], float(o[1]) - s[1]
    d = cross_z(bx, by, cx, cy)
    if d == 0.0:
        labels.add('through-collinear')
        return
    bb = bx * bx + by * by
    cc = cx * cx + cy * cy
    rx = (cy * bb - by * cc) / (2.0 * d)
    ry = (bx * cc - cx * bb) / (2.0 * d)
    ln = norm(bx, by, labels, 'through')
    ct = (-rx * by + ry * bx) / ln
    labels.add('through-ccw' if d > 0.0 else 'through-cw')
    build(s, e, 1.0 if d > 0.0 else -1.0, ct, norm(rx, ry, labels, 'through-rel'), labels)


def center(s, e, c, major, labels):
    cx, cy = float(e[0]) - s[0], float(e[1]) - s[1]
    ln = norm(cx, cy, labels, 'center')
    if ln <= 0.0:
        labels.add('center-zero-chord')
        return
    nx, ny = -cy / ln, cx / ln
    mx, my = (float(s[0]) + e[0]) * 0.5, (float(s[1]) + e[1]) * 0.5
    ct = (c[0] - mx) * nx + (c[1] - my) * ny
    labels.add('center-t-zero' if ct == 0.0 else 'center-t-neg' if ct < 0.0 else 'center-t-pos')
    side = -1.0 if ct < 0.0 else 1.0
    labels.add('center-major' if major else 'center-minor')
    if not major:
        side = -side
    build(s, e, side, ct, hypot_finite(ln * 0.5, ct), labels)


def tangent(s, t, e, labels):
    tl = norm(t[0], t[1], labels, 'tangent')
    if tl <= 0.0 or not math.isfinite(tl):
        labels.add('tangent-nan-length' if math.isnan(tl) else
                   'tangent-zero' if tl <= 0.0 else 'tangent-infinite-length')
        return
    tx, ty = t[0] / tl, t[1] / tl
    dx, dy = float(e[0]) - s[0], float(e[1]) - s[1]
    dd = dx * dx + dy * dy
    cr = cross_z(dx, dy, tx, ty)
    if dd <= 0.0 or cr == 0.0:
        labels.add('tangent-coincident' if dd <= 0.0 else 'tangent-on-line')
        return
    k = -dd / (2.0 * cr) if math.isfinite(-dd / (2.0 * cr)) else math.copysign(math.inf, -cr)
    labels.add('tangent-left' if cr > 0.0 else 'tangent-right')
    ln = math.sqrt(dd)
    dot = (-ty) * (-dy / ln) + tx * (dx / ln)
    ct = k * dot if not (math.isinf(k) and dot == 0.0) else math.nan
    build(s, e, 1.0 if cr > 0.0 else -1.0, ct, abs(k), labels)


def mirror(inp):
    labels = set()
    op = inp['op']
    labels.add(f'op-{op}')
    try:
        if op == 'project':
            project(inp['start'], inp['end'], inp['cursor'], labels)
        elif op == 'through':
            through(inp['start'], inp['on_arc'], inp['end'], labels)
        elif op == 'mid_drag':
            p = project(inp['start'], inp['end'], inp['cursor'], labels)
            through(inp['start'], p, inp['end'], labels)
        elif op == 'center_drag':
            center(inp['start'], inp['end'], inp['cursor'], inp['major'], labels)
        else:
            tangent(inp['start'], [from_bits(v) for v in inp['tangent']], inp['end'], labels)
    except Deferred:
        labels.add('deferred')
    return labels


def clamp(v):
    return max(INT_MIN, min(INT_MAX, v))


def generate():
    rng = random.Random(SEED)
    cases = []

    def add(name, **inp):
        cases.append({'unit': UNIT, 'case': name, 'input': inp})

    def pt(scale):
        return [rng.randint(max(INT_MIN, -scale), min(INT_MAX, scale)) for _ in range(2)]

    def tbits(x, y):
        return [bits(float(x)), bits(float(y))]

    # Upstream QA inputs (test_arc_construction.cpp at the pin).
    add('qa-project-0', op='project', start=[0, 0], end=[1000000, 0], cursor=[700000, 300000])
    add('qa-project-1', op='project', start=[0, 0], end=[1000000, 0], cursor=[-40, -900000])
    add('qa-project-2', op='project', start=[0, 0], end=[1000000, 1000000], cursor=[900000, 100000])
    add('qa-project-3', op='project', start=[0, 0], end=[0, 0], cursor=[12, 34])
    add('qa-through-quarter', op='through', start=[1000000, 0], on_arc=[707107, 707107], end=[0, 1000000])
    add('qa-through-major', op='through', start=[1000000, 0], on_arc=[-707107, -707107], end=[0, 1000000])
    add('qa-through-rev', op='through', start=[0, 1000000], on_arc=[707107, 707107], end=[1000000, 0])
    add('qa-through-up', op='through', start=[0, 0], on_arc=[300000, 200000], end=[1000000, 0])
    add('qa-through-down', op='through', start=[0, 0], on_arc=[300000, -200000], end=[1000000, 0])
    for i, (s, o, e) in enumerate([([0, 0], [500, 0], [1000, 0]), ([0, 0], [500, 500], [1000, 1000]),
                                   ([0, 0], [0, 0], [1000, 1000]), ([5, 5], [700, 900], [5, 5]),
                                   ([5, 5], [5, 5], [5, 5])]):
        add(f'qa-through-degenerate-{i}', op='through', start=s, on_arc=o, end=e)
    add('qa-through-shallow', op='through', start=[0, 0], on_arc=[1000000000, 1000], end=[2000000000, 0])
    add('qa-through-1nm', op='through', start=[-2000000000, 0], on_arc=[0, 1], end=[2000000000, 0])
    add('qa-through-limits', op='through', start=[-2000000000, 0], on_arc=[0, 1500000000],
        end=[2000000000, 0])
    a, b = [-1000000000, -1000000000], [1000000000, 1000000000]
    add('qa-corners-through', op='through', start=a, on_arc=[1000000000, -1000000000], end=b)
    add('qa-corners-mid', op='mid_drag', start=a, end=b, cursor=[300000000, -700000000])
    add('qa-corners-center', op='center_drag', start=a, end=b, cursor=[0, 0], major=False)
    add('qa-corners-tangent', op='tangent', start=a, tangent=tbits(1, 0), end=b)
    add('qa-corners-project', op='project', start=a, end=b, cursor=[1000000000, -1000000000])
    add('qa-mid-0', op='mid_drag', start=[0, 0], end=[1000000, 0], cursor=[700000, 300000])
    add('qa-mid-1', op='mid_drag', start=[0, 0], end=[1000000, 0], cursor=[100, -250000])
    add('qa-mid-diagonal', op='mid_drag', start=[123457, -98765], end=[1000003, 777777], cursor=[100000, 900000])
    add('qa-mid-on-chord-0', op='mid_drag', start=[0, 0], end=[1000000, 0], cursor=[300000, 0])
    add('qa-mid-on-chord-1', op='mid_drag', start=[0, 0], end=[1000000, 1000000], cursor=[900000, 900000])
    add('qa-mid-on-chord-2', op='mid_drag', start=[7, 7], end=[7, 7], cursor=[900000, 900000])
    for major in (False, True):
        m = 'major' if major else 'minor'
        add(f'qa-center-{m}', op='center_drag', start=[0, 0], end=[1000000, 0], cursor=[500000, 500000],
            major=major)
        add(f'qa-center-below-{m}', op='center_drag', start=[0, 0], end=[1000000, 0],
            cursor=[500000, -500000], major=major)
        add(f'qa-center-on-chord-{m}', op='center_drag', start=[0, 0], end=[1000000, 0], cursor=[500000, 0],
            major=major)
        add(f'qa-center-zero-{m}', op='center_drag', start=[3, 3], end=[3, 3], cursor=[500, 500], major=major)
        add(f'qa-center-limits-{m}', op='center_drag', start=[-2000000000, 0], end=[2000000000, 0],
            cursor=[0, 2000000000], major=major)
    start = (250000, -130000)
    for ti, (tx, ty) in enumerate([(1, 0), (0, -1), (3, 4), (-5, 2)]):
        n = math.hypot(tx, ty)
        ux, uy = tx / n, ty / n
        for degrees in range(5, 360, 10):
            for radius in (3000.0, 400000.0, 90000000.0):
                ang = degrees * math.pi / 180.0
                end = [start[0] + kiround(radius * (ux * math.cos(ang) - uy * math.sin(ang))),
                       start[1] + kiround(radius * (ux * math.sin(ang) + uy * math.cos(ang)))]
                add(f'qa-tangent-sweep-{ti}-{degrees}-{int(radius)}', op='tangent', start=list(start),
                    tangent=tbits(tx, ty), end=end)
    add('qa-tangent-quarter', op='tangent', start=[0, 0], tangent=tbits(1, 0), end=[100000, 100000])
    add('qa-tangent-behind', op='tangent', start=[0, 0], tangent=tbits(1, 0), end=[-100000, 100000])
    add('qa-tangent-right', op='tangent', start=[0, 0], tangent=tbits(1, 0), end=[100000, -100000])

    scales = [4, 1000, 1000000, 1000000000, INT_MAX]

    # Fixed-seed general families at every scale.
    for si, scale in enumerate(scales):
        for i in range(60):
            add(f'project-random-{si}-{i}', op='project', start=pt(scale), end=pt(scale), cursor=pt(scale))
            add(f'through-random-{si}-{i}', op='through', start=pt(scale), on_arc=pt(scale), end=pt(scale))
            add(f'mid-random-{si}-{i}', op='mid_drag', start=pt(scale), end=pt(scale), cursor=pt(scale))
            add(f'center-random-{si}-{i}', op='center_drag', start=pt(scale), end=pt(scale), cursor=pt(scale),
                major=bool(i & 1))
            ang = rng.uniform(-math.pi, math.pi)
            mag = 10.0 ** rng.uniform(-6, 6)
            add(f'tangent-random-{si}-{i}', op='tangent', start=pt(scale),
                tangent=tbits(mag * math.cos(ang), mag * math.sin(ang)), end=pt(scale))

    # Degenerate: zero chords, coincident and collinear points, at the integer limits too.
    for i, p in enumerate([[0, 0], [INT_MIN, INT_MIN], [INT_MAX, INT_MAX], [INT_MIN, INT_MAX], [17, -3]]):
        q = pt(INT_MAX)
        add(f'project-zero-chord-{i}', op='project', start=p, end=p, cursor=q)
        add(f'through-zero-chord-{i}', op='through', start=p, on_arc=q, end=p)
        add(f'through-all-same-{i}', op='through', start=p, on_arc=p, end=p)
        add(f'mid-zero-chord-{i}', op='mid_drag', start=p, end=p, cursor=q)
        add(f'center-zero-chord-{i}', op='center_drag', start=p, end=p, cursor=q, major=bool(i & 1))
        add(f'tangent-coincident-{i}', op='tangent', start=p, tangent=tbits(3, -4), end=p)
    for i in range(40):
        s = pt(1000000000)
        step = [rng.randint(-1000, 1000), rng.randint(-1000, 1000)]
        k1, k2 = rng.randint(-50000, 50000), rng.randint(-50000, 50000)
        o = [clamp(s[0] + k1 * step[0]), clamp(s[1] + k1 * step[1])]
        e = [clamp(s[0] + k2 * step[0]), clamp(s[1] + k2 * step[1])]
        add(f'through-collinear-{i}', op='through', start=s, on_arc=o, end=e)
        add(f'tangent-on-line-{i}', op='tangent', start=s, tangent=tbits(step[0] or 1, step[1]),
            end=[clamp(s[0] + k2 * (step[0] or 1)), clamp(s[1] + k2 * step[1])])
        # Near-collinear: one nanometre off the chord line, either side.
        off = [-step[1], step[0]] if any(step) else [0, 1]
        g = math.gcd(off[0], off[1]) or 1
        off = [off[0] // g, off[1] // g]
        sign = 1 if i & 1 else -1
        add(f'through-near-collinear-{i}', op='through', start=s,
            on_arc=[clamp(o[0] + sign * off[0]), clamp(o[1] + sign * off[1])], end=e)
        add(f'mid-on-chord-{i}', op='mid_drag', start=s, end=e, cursor=o)

    # Apex within rounding of a long chord: the chord line passes through its own lattice
    # midpoint, and the on-arc point sits 1/|chord| off that line near the middle.
    for i in range(30):
        a_ = rng.randint(1, 1000000000)
        b_ = rng.randint(1, 1000000000)
        g = math.gcd(a_, b_)
        a_, b_ = a_ // g, b_ // g
        # Solve b*x - a*y = 1 near (a, b).
        x0 = pow(b_, -1, a_) if a_ > 1 else 0
        y0 = (b_ * x0 - 1) // a_ if a_ > 1 else -1
        t = (a_ - x0) // a_ if a_ > 1 else 0
        on = [x0 + t * a_, y0 + t * b_]
        sx, sy = rng.choice([1, -1]), rng.choice([1, -1])
        s = [0, 0]
        e = [2 * a_ * sx, 2 * b_ * sy]
        on = [on[0] * sx, on[1] * sy]
        if max(abs(v) for v in e + on) > INT_MAX:
            continue
        add(f'through-apex-rounding-{i}', op='through', start=s, on_arc=on, end=e)
        add(f'mid-apex-rounding-{i}', op='mid_drag', start=s, end=e, cursor=on)

    # Center drag: centers far along the bisector make the apex round onto the chord (minor) or
    # leave the coordinate range (major); centers on the chord give semicircles.
    for i in range(40):
        half = rng.randint(1, 100000) * 2
        y = rng.randint(-1000000000, 1000000000)
        x0 = rng.randint(-1000000000, 1000000000)
        s, e = [x0 - half, y], [x0 + half, y]
        far = rng.choice([1, -1]) * rng.randint(1000000000, INT_MAX)
        for major in (False, True):
            add(f'center-far-{i}-{int(major)}', op='center_drag', start=s, end=e,
                cursor=[x0 + rng.randint(-half, half), clamp(y + far)], major=major)
            add(f'center-on-chord-{i}-{int(major)}', op='center_drag', start=s, end=e,
                cursor=[x0 + rng.randint(-10 * half, 10 * half), y], major=major)
            add(f'center-at-mid-{i}-{int(major)}', op='center_drag', start=s, end=e, cursor=[x0, y],
                major=major)

    # toIU limits: projections and apexes at and just past INT_MAX - 1.
    for i, (s, e, c) in enumerate([
            ([INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [INT_MAX, INT_MAX]),
            ([INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [INT_MAX - 1, INT_MAX - 1]),
            ([INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [INT_MAX - 2, INT_MAX - 2]),
            ([INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [INT_MIN, INT_MIN]),
            ([INT_MIN + 1, INT_MAX], [INT_MAX, INT_MIN + 1], [INT_MIN + 1, INT_MIN + 1]),
            ([INT_MIN + 2, INT_MAX], [INT_MAX, INT_MIN + 2], [INT_MIN + 2, INT_MIN + 2]),
            ([INT_MIN, 0], [INT_MAX, 0], [0, INT_MAX]),
            ([INT_MIN, 0], [INT_MAX, 0], [0, INT_MIN]),
            ([0, INT_MIN], [0, INT_MAX], [INT_MAX, 0]),
            ([-1, -1], [1, 1], [INT_MAX, 0]),
            ([-1, 1], [1, -1], [INT_MIN, INT_MIN]),
            ([0, 0], [2, 2], [INT_MAX, INT_MIN])]):
        add(f'project-limit-{i}', op='project', start=s, end=e, cursor=c)
        add(f'mid-limit-{i}', op='mid_drag', start=s, end=e, cursor=c)
        add(f'through-limit-{i}', op='through', start=s, on_arc=c, end=e)
        for major in (False, True):
            add(f'center-limit-{i}-{int(major)}', op='center_drag', start=s, end=e, cursor=c, major=major)
    for i in range(30):
        s = [rng.randint(INT_MIN, INT_MIN + 1000), rng.randint(INT_MIN, INT_MAX)]
        e = [rng.randint(INT_MAX - 1000, INT_MAX), rng.randint(INT_MIN, INT_MAX)]
        corners = [[INT_MIN, INT_MIN], [INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [INT_MAX, INT_MAX]]
        c = corners[i % 4]
        add(f'project-huge-{i}', op='project', start=s, end=e, cursor=c)
        add(f'through-huge-{i}', op='through', start=s, on_arc=c, end=e)
        add(f'mid-huge-{i}', op='mid_drag', start=s, end=e, cursor=c)
        add(f'center-huge-{i}', op='center_drag', start=s, end=e, cursor=c, major=bool(i & 2))
        add(f'tangent-huge-{i}', op='tangent', start=s, tangent=tbits(c[0], -c[1]), end=e)

    # Tiny chords and unit-scale geometry, every orientation.
    for i, (s, o, e) in enumerate([([0, 0], [0, 1], [1, 1]), ([0, 0], [1, 0], [1, 1]), ([0, 0], [1, 1], [0, 1]),
                                   ([0, 0], [1, 0], [0, 1]), ([0, 0], [-1, 0], [0, 1]), ([0, 0], [1, 1], [2, 0]),
                                   ([0, 0], [1, -1], [2, 0]), ([0, 0], [0, 1], [1, 0]), ([1, 1], [0, 0], [-1, 1]),
                                   ([-1, -1], [0, 1], [1, -1]), ([0, 0], [1, 2], [3, 0]), ([0, 0], [2, 1], [3, 3])]):
        add(f'through-tiny-{i}', op='through', start=s, on_arc=o, end=e)
        add(f'mid-tiny-{i}', op='mid_drag', start=s, end=e, cursor=o)
        add(f'project-tiny-{i}', op='project', start=s, end=e, cursor=o)
        for major in (False, True):
            add(f'center-tiny-{i}-{int(major)}', op='center_drag', start=s, end=e, cursor=o, major=major)
        add(f'tangent-tiny-{i}', op='tangent', start=s, tangent=tbits(o[0] - s[0] or 1, o[1] - s[1]), end=e)

    # Tangent special values: signed zeros, subnormals, overflowing lengths, the diagonal and
    # axis norm branches, and (deferred) infinite or NaN general-branch operands.
    nan = math.nan
    inf = math.inf
    specials = [
        ('pos-zero', 0.0, 0.0), ('neg-zero', -0.0, -0.0), ('mixed-zero', 0.0, -0.0),
        ('min-subnormal-x', 5e-324, 0.0), ('min-subnormal-diag', 5e-324, 5e-324),
        ('min-subnormal-general', 5e-324, 1e-323), ('subnormal-y', 1.0, 1e-310),
        ('tiny-y', 1.0, 1e-300), ('tiny-general', 3e-200, 4e-200), ('huge-general', 3e300, 4e300),
        ('huge-diag', 1.7e308, -1.7e308), ('max-axis', -1.7976931348623157e308, 0.0),
        ('overflow-general', 1.7e308, 1.0e308), ('axis-negative', 0.0, -7.25),
        ('diag-negative', -2.5, 2.5), ('unit-x', 1.0, 0.0), ('unit-neg-y', 0.0, -1.0),
        ('three-four', 3.0, 4.0), ('nan-x-axis', nan, 0.0), ('nan-y-axis', 0.0, nan),
        ('inf-x-axis', inf, 0.0), ('neg-inf-y-axis', 0.0, -inf), ('inf-diag', inf, -inf),
        ('inf-general', inf, 1.0), ('nan-general', nan, 1.0), ('nan-both', nan, nan),
        ('inf-nan', inf, nan)]
    for name, tx, ty in specials:
        for j, (s, e) in enumerate([([0, 0], [100000, 100000]), ([0, 0], [100000, -1]),
                                    ([-5, 7], [-5, 1000007]), ([INT_MIN, INT_MIN], [INT_MAX, INT_MAX]),
                                    ([0, 0], [100000, 0])]):
            add(f'tangent-special-{name}-{j}', op='tangent', start=s, tangent=tbits(tx, ty), end=e)
    add('tangent-signalling-nan-axis', op='tangent', start=[0, 0],
        tangent=['0000000000000000', '7ff0000000000001'], end=[10, 10])
    add('tangent-negative-nan-axis', op='tangent', start=[0, 0],
        tangent=['fff8000000000000', '8000000000000000'], end=[10, 10])
    # Ends almost on the tangent line: the compensated cross product decides.
    for i in range(30):
        s = pt(1000000000)
        dx, dy = rng.randint(1, 1000000000), rng.randint(-1000000000, 1000000000)
        t = [float(dx), float(dy)]
        e = [clamp(s[0] + dx), clamp(s[1] + dy + rng.choice([-1, 0, 1]))]
        add(f'tangent-near-line-{i}', op='tangent', start=s, tangent=tbits(*t), end=e)
    return cases


def controls():
    """Harness controls: malformed requests both adapters must refuse identically, before any
    pinned call. No valid 32-bit point or IEEE tangent input leaves the defined C++ domain (every
    KiROUND is guarded by toIU, all other arithmetic is IEEE double, and hypot/fma/sqrt are defined
    for every operand), so there are no undefined-domain controls; these never enter the corpus."""
    p = [0, 0]
    rows = [
        ('op-missing', {'start': p}, 'expected string: op'),
        ('op-number', {'op': 3}, 'expected string: op'),
        ('op-unknown', {'op': 'rotate'}, 'unknown operation'),
        ('project-missing-field', {'op': 'project', 'start': p, 'end': p}, 'unexpected or missing field'),
        ('project-extra-field', {'op': 'project', 'start': p, 'end': p, 'cursor': p, 'x': 1},
         'unexpected or missing field'),
        ('project-short-point', {'op': 'project', 'start': [0], 'end': p, 'cursor': p}, 'expected point'),
        ('project-point-object', {'op': 'project', 'start': p, 'end': {'x': 0}, 'cursor': p},
         'expected point'),
        ('through-above-int', {'op': 'through', 'start': p, 'on_arc': [2 ** 31, 0], 'end': p},
         'point out of range'),
        ('through-below-int', {'op': 'through', 'start': p, 'on_arc': [0, -2 ** 31 - 1], 'end': p},
         'point out of range'),
        ('through-unsigned-huge', {'op': 'through', 'start': p, 'on_arc': [2 ** 64 - 1, 0], 'end': p},
         'point out of range'),
        ('mid-float-coordinate', {'op': 'mid_drag', 'start': p, 'end': [0.0, 1], 'cursor': p},
         'point out of range'),
        ('mid-string-coordinate', {'op': 'mid_drag', 'start': p, 'end': ['1', 1], 'cursor': p},
         'point out of range'),
        ('center-major-number', {'op': 'center_drag', 'start': p, 'end': p, 'cursor': p, 'major': 1},
         'expected boolean: major'),
        ('center-missing-major', {'op': 'center_drag', 'start': p, 'end': p, 'cursor': p},
         'unexpected or missing field'),
        ('tangent-numbers', {'op': 'tangent', 'start': p, 'tangent': [0, 0], 'end': p}, 'expected double bits'),
        ('tangent-upper-hex', {'op': 'tangent', 'start': p, 'tangent': ['3FF0000000000000', '0' * 16],
                               'end': p}, 'expected double bits'),
        ('tangent-short-hex', {'op': 'tangent', 'start': p, 'tangent': ['3ff000000000000', '0' * 16],
                               'end': p}, 'expected double bits'),
        ('tangent-long-hex', {'op': 'tangent', 'start': p, 'tangent': ['0' * 16, '0' * 17], 'end': p},
         'expected double bits'),
        ('tangent-one-component', {'op': 'tangent', 'start': p, 'tangent': ['3ff0000000000000'], 'end': p},
         'expected vector'),
        ('tangent-bad-end-after-good', {'op': 'tangent', 'start': p, 'tangent': ['0' * 16, '0' * 16],
                                        'end': [1, 2, 3]}, 'expected point'),
    ]
    return [{'error': {'kind': 'invalid_input', 'message': m},
             'request': {'case': f'control-{n}', 'input': i, 'unit': UNIT}} for n, i, m in rows]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='fail unless the corpus is current')
    parser.add_argument('--branches', action='store_true', help='print branch label counts')
    args = parser.parse_args()
    cases = generate()
    names = [c['case'] for c in cases]
    assert len(set(names)) == len(names), 'duplicate case name'
    text = ''.join(json.dumps(c, sort_keys=True, separators=(',', ':')) + '\n' for c in cases)
    counts = Counter()
    for c in cases:
        counts.update(mirror(c['input']))
    required = {
        'project-zero-chord', 'project-toiu-fail', 'project-ok', 'through-collinear', 'through-ccw',
        'through-cw', 'build-height-shallow', 'build-height-deep', 'build-toiu-fail',
        'build-apex-on-chord', 'build-valid', 'build-side-pos', 'build-side-neg',
        'build-bad-radius', 'center-zero-chord', 'center-t-zero', 'center-t-neg', 'center-t-pos',
        'center-major', 'center-minor', 'tangent-zero', 'tangent-nan-length',
        'tangent-infinite-length', 'tangent-coincident', 'tangent-on-line', 'tangent-left',
        'tangent-right', 'tangent-norm-diagonal', 'tangent-norm-axis', 'tangent-norm-hypot',
        'tangent-norm-deferred', 'build-norm-hypot', 'build-norm-axis', 'build-norm-diagonal',
        'through-rel-norm-hypot', 'deferred'}
    missing = required - set(counts)
    if args.branches:
        for label, n in sorted(counts.items()):
            print(f'{label}: {n}')
        print(f'cases: {len(cases)}, deferred: {counts["deferred"]}')
    if missing:
        print(f'unreached branches: {sorted(missing)}', file=sys.stderr)
        return 1
    control_text = ''.join(json.dumps(c, sort_keys=True, separators=(',', ':')) + '\n'
                           for c in controls())
    if args.check:
        for path, expected in ((CASES, text), (CONTROLS, control_text)):
            if not path.exists() or path.read_text(encoding='utf-8') != expected:
                print(f'{path} is stale; run generate_cases.py', file=sys.stderr)
                return 1
        return 0
    CASES.write_bytes(text.encode('utf-8'))
    CONTROLS.write_bytes(control_text.encode('utf-8'))
    print(f'wrote {len(cases)} cases ({counts["deferred"]} reach a deferral)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
