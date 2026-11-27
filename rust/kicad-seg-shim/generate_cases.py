#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for SEG (libs/kimath/src/geometry/seg.cpp); inputs only.

Python integers are exact and Python floats are IEEE binary64 with correctly rounded
arithmetic, so the mirror below follows every pinned body in evaluation order. It is used
only to classify a candidate request and label the source branches it reaches; it never
writes a result:

* defined: the pinned evaluation has no undefined behaviour. The request enters the public
  corpus. The mirror also records the dependency calls the Rust port still defers (util.cpp's
  kimathLogOverflow, reached by a nonquiet KiROUND overflow), so the expected deferral count is
  known. The seg.h bodies SEG::Square and SEG::SquaredLength are ported by the header unit
  (libs.kimath.include.geometry.application.ui.p003) and are no longer deferrals.
* undefined: the evaluation reaches signed overflow, negation of the minimum or an
  out-of-range floating-to-integer cast before any deferred call. The request is a separate
  control; both programs must refuse it identically. It never enters the public corpus.
* not representable: the evaluation reaches undefined behaviour only after a call the Rust port
  defers, so Rust stops at the deferral while C++ would refuse later. Such candidates are
  counted and dropped; they become controls once the deferred owner is ported.
"""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.seg.application.ui.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'seg-controls.jsonl'
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined seg operation'}
SEED = 0x5E6C0DE
INT_MIN, INT_MAX = -2 ** 31, 2 ** 31 - 1
I64_MIN, I64_MAX = -2 ** 63, 2 ** 63 - 1
TWO63 = float(2 ** 63)


class Undefined(Exception):
    pass


def bits(v):
    return '%016x' % struct.unpack('<Q', struct.pack('<d', v))[0]


def from_bits(text):
    return struct.unpack('<d', struct.pack('<Q', int(text, 16)))[0]


def I(v):
    if not INT_MIN <= v <= INT_MAX:
        raise Undefined
    return v


def L(v):
    if not I64_MIN <= v <= I64_MAX:
        raise Undefined
    return v


def wrap64(v):
    v &= (1 << 64) - 1
    return v - (1 << 64) if v >= 1 << 63 else v


def wrap32(v):
    v &= (1 << 32) - 1
    return v - (1 << 32) if v >= 1 << 31 else v


def tdiv(a, b):
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) else -q


def rescale(n, v, d, narrow):
    """util.cpp rescale<int> / rescale<int64_t> (__int128): exact, then modular narrowing."""
    num = n * v
    half = tdiv(d, 2)
    r = tdiv(num - half if (num < 0) ^ (d < 0) else num + half, d)
    return narrow(r)


def sq_norm(x, y):
    """VECTOR2<int or int64>::SquaredEuclideanNorm: saturating, never undefined."""
    return min(x * x + y * y, I64_MAX)


def isqrt(v):
    """util.h isqrt<int64_t>: negative input returns sqrt(INT64_MAX)."""
    return math.isqrt(I64_MAX) if v < 0 else math.isqrt(v)


def llround(v):
    t = math.trunc(v)
    if abs(v - t) >= 0.5:
        t += 1 if v > 0 else -1
    return t


class Run:
    """One pinned evaluation. events: deferred calls reached in order (Rust stops at the first)."""

    def __init__(self):
        self.events = []
        self.labels = set()

    def defer(self, name):
        self.events.append(name)

    def kiround(self, v, lo, hi):
        if v != v:
            self.defer('kimathLogOverflow')
            return 0
        if v >= float(hi) + 0.5:
            self.defer('kimathLogOverflow')
            return hi
        if v - float(lo) <= -0.5:
            self.defer('kimathLogOverflow')
            return lo
        r = llround(v)
        c = min(max(r, lo), hi)
        if c != r:
            self.defer('kimathLogOverflow')
        return c

    # Vector helpers -------------------------------------------------------------------------
    @staticmethod
    def subI(a, b):
        return (I(a[0] - b[0]), I(a[1] - b[1]))

    @staticmethod
    def addI(a, b):
        return (I(a[0] + b[0]), I(a[1] + b[1]))

    @staticmethod
    def dotL(a, b):
        return L(L(a[0] * b[0]) + L(a[1] * b[1]))

    @staticmethod
    def crossL(a, b):
        return L(L(a[0] * b[1]) - L(a[1] * b[0]))

    def half(self, v):
        """VECTOR2I::operator/( double 2 ): KiROUND( x / 2.0 ) per component."""
        return (self.kiround(v[0] / 2.0, INT_MIN, INT_MAX), self.kiround(v[1] / 2.0, INT_MIN, INT_MAX))

    @staticmethod
    def narrow(x, y):
        return (min(max(x, INT_MIN), INT_MAX), min(max(y, INT_MIN), INT_MAX))

    # Bodies --------------------------------------------------------------------------------
    def squared_distance_point(self, s, p):
        A, B = s
        ab = (B[0] - A[0], B[1] - A[1])
        ap = (p[0] - A[0], p[1] - A[1])
        e = self.dotL(ap, ab)
        if e <= 0:
            self.labels.add('sqdist-point-before-a')
            return sq_norm(*ap)
        f = sq_norm(*ab)
        if e >= f:
            self.labels.add('sqdist-point-after-b')
            bp = (p[0] - B[0], p[1] - B[1])
            return self.dotL(bp, bp)
        cross = float(ab[0] * ap[1] - ab[1] * ap[0])
        g = cross * cross / float(f)
        if not (g >= 0) or g > TWO63:
            self.labels.add('sqdist-point-lagrange-out-of-range')
            return 0
        self.labels.add('sqdist-point-lagrange')
        return self.kiround(g, I64_MIN, I64_MAX)

    def distance_point(self, s, p):
        return wrap32(isqrt(self.squared_distance_point(s, p)))

    def nearest_point_point(self, s, p):
        A, B = s
        d = (B[0] - A[0], B[1] - A[1])
        l = L(L(d[0] * d[0]) + L(d[1] * d[1]))
        if l == 0:
            self.labels.add('nearest-point-degenerate')
            return A
        pa = (p[0] - A[0], p[1] - A[1])
        t = self.dotL(d, pa)
        if t < 0:
            self.labels.add('nearest-point-before-a')
            return A
        if t > l:
            self.labels.add('nearest-point-after-b')
            return B
        self.labels.add('nearest-point-inside')
        xp = rescale(t, d[0], l, wrap64)
        yp = rescale(t, d[1], l, wrap64)
        return self.narrow(L(A[0] + xp), L(A[1] + yp))

    def collinear_overlap(self, s, o, use_x, ignore, want):
        k, c = (0, 1) if use_x else (1, 0)
        s1s, s1e, s2s, s2e = s[0][k], s[1][k], o[0][k], o[1][k]
        c1s, c1e = s[0][c], s[1][c]
        s1min, s1max, s2min, s2max = min(s1s, s1e), max(s1s, s1e), min(s2s, s2e), max(s2s, s2e)
        if not (s1max >= s2min and s2max >= s1min):
            self.labels.add('collinear-disjoint')
            return False, None
        start, end = max(s1min, s2min), min(s1max, s2max)
        if ignore and start == end:
            if (start in (s1min, s1max)) and (start in (s2min, s2max)):
                self.labels.add('collinear-endpoint-touch-ignored')
                return False, None
            self.labels.add('collinear-zero-overlap-not-endpoint')
        if not want:
            self.labels.add('collinear-overlap')
            return True, None
        proj = tdiv(I(start + end), 2)
        if s1e != s1s:
            self.labels.add('collinear-point-interpolated')
            n = I(proj - s1s)
            v = I(c1e - c1s)
            d = I(s1e - s1s)
            other = I(c1s + rescale(n, v, d, wrap32))
        else:
            self.labels.add('collinear-point-degenerate-axis')
            other = c1s
        return True, ((proj, other) if use_x else (other, proj))

    def intersects(self, s, o, ignore=False, lines=False, want=False):
        A, B = s
        if not lines:
            if (max(A[0], B[0]) < min(o[0][0], o[1][0]) or max(o[0][0], o[1][0]) < min(A[0], B[0])
                    or max(A[1], B[1]) < min(o[0][1], o[1][1]) or max(o[0][1], o[1][1]) < min(A[1], B[1])):
                self.labels.add('intersects-bbox-reject')
                return False, None
        dir1 = (B[0] - A[0], B[1] - A[1])
        dir2 = (o[1][0] - o[0][0], o[1][1] - o[0][1])
        offset = (o[0][0] - A[0], o[0][1] - A[1])
        det = self.crossL(dir2, dir1)
        if det == 0:
            if self.crossL(dir1, offset) != 0:
                self.labels.add('intersects-parallel')
                return False, None
            if lines:
                pt = None
                if want:
                    if o[0] == o[1]:
                        self.labels.add('lines-collinear-other-point')
                        pt = o[0]
                    elif A == B:
                        self.labels.add('lines-collinear-this-point')
                        pt = A
                    else:
                        self.labels.add('lines-collinear-midpoint')
                        pt = self.half(self.addI(A, o[0]))
                else:
                    self.labels.add('lines-collinear')
                return True, pt
            return self.collinear_overlap(s, o, abs(dir1[0]) >= abs(dir1[1]), ignore, want)
        p2 = self.crossL(dir2, offset)
        p1 = self.crossL(dir1, offset)
        if not lines:
            if det > 0:
                if p1 < 0 or p1 > det or p2 < 0 or p2 > det:
                    self.labels.add('intersects-param-out-positive')
                    return False, None
            elif p1 > 0 or p1 < det or p2 > 0 or p2 < det:
                self.labels.add('intersects-param-out-negative')
                return False, None
            if ignore and p1 in (0, det) and p2 in (0, det):
                self.labels.add('intersects-endpoint-ignored')
                return False, None
        if want:
            sx = rescale(p1, dir2[0], det, wrap64)
            sy = rescale(p1, dir2[1], det, wrap64)
            rx, ry = L(o[0][0] + sx), L(o[0][1] + sy)
            if not (INT_MIN <= rx <= INT_MAX and INT_MIN <= ry <= INT_MAX):
                self.labels.add('intersects-point-overflow')
                return False, None
            self.labels.add('intersects-crossing-point')
            return True, (rx, ry)
        self.labels.add('intersects-crossing')
        return True, None

    def intersect(self, s, o, ignore=False, lines=False):
        hit, pt = self.intersects(s, o, ignore, lines, True)
        return pt if hit else None

    def squared_distance_seg(self, s, o):
        if s[0] == s[1]:
            self.labels.add('sqdist-seg-this-point')
            return self.squared_distance_point(o, s[0])
        if o[0] == o[1]:
            self.labels.add('sqdist-seg-other-point')
            return self.squared_distance_point(s, o[0])
        if self.intersects(s, o)[0]:
            self.labels.add('sqdist-seg-intersecting')
            return 0
        pts = [self.subI(self.nearest_point_point(o, s[0]), s[0]),
               self.subI(self.nearest_point_point(o, s[1]), s[1]),
               self.subI(self.nearest_point_point(s, o[0]), o[0]),
               self.subI(self.nearest_point_point(s, o[1]), o[1])]
        self.labels.add('sqdist-seg-endpoints')
        return min(I64_MAX, *(sq_norm(*p) for p in pts))

    def nearest_point_seg(self, s, o):
        p = self.intersect(s, o)
        if p is not None:
            self.labels.add('nearest-seg-intersection')
            return p
        origin = [self.nearest_point_point(o, s[0]), self.nearest_point_point(o, s[1]),
                  self.nearest_point_point(s, o[0]), self.nearest_point_point(s, o[1])]
        for q, r in zip(origin, (s[0], s[1], o[0], o[1])):
            self.subI(q, r)
        self.labels.add('nearest-seg-endpoints')
        return None

    def intersects_line(self, s, slope, offset):
        A, B = s
        dx, dy = B[0] - A[0], B[1] - A[1]
        if dx == 0:
            y = slope * A[0] + offset
            if y >= min(A[1], B[1]) and y <= max(A[1], B[1]):
                self.labels.add('line-vertical-hit')
                self.kiround(y, INT_MIN, INT_MAX)
            else:
                self.labels.add('line-vertical-miss')
            return
        scaled = slope * 1000
        # static_cast<ecoord>: defined only for a finite value whose truncation fits.
        if not math.isfinite(scaled) or not -TWO63 <= math.trunc(scaled) < TWO63:
            raise Undefined
        cross = self.crossL((dx, dy), (1000, math.trunc(scaled)))
        if cross == 0:
            expected = slope * A[0] + offset
            if abs(A[1] - expected) < 0.5:
                self.labels.add('line-collinear')
                self.half(self.addI(A, B))
            else:
                self.labels.add('line-parallel-miss')
            return
        num = slope * float(A[0]) + offset - float(A[1])
        den = float(dy) - slope * float(dx)
        try:
            t = num / den
        except ZeroDivisionError:
            t = math.nan if num == 0 or num != num else math.copysign(math.inf, num) * math.copysign(1.0, den)
        if 0.0 <= t <= 1.0:
            self.labels.add('line-crossing-hit')
            self.kiround(float(A[0]) + t * float(dx), INT_MIN, INT_MAX)
            self.kiround(float(A[1]) + t * float(dy), INT_MIN, INT_MAX)
        else:
            self.labels.add('line-crossing-miss')

    def perpendicular_seg(self, s, p):
        d = self.subI(s[1], s[0])
        return (p, self.addI((I(-d[1]), d[0]), p))

    def parallel_seg(self, s, p):
        return (p, self.addI(self.subI(s[1], s[0]), p))

    def collide(self, s, o, clearance):
        if clearance < 0:
            self.labels.add('collide-negative-clearance')
            return
        if s[0] == s[1]:
            self.labels.add('collide-this-point')
            self.distance_point(o, s[0])
            return
        if o[0] == o[1]:
            self.labels.add('collide-other-point')
            self.distance_point(s, o[0])
            return
        if self.intersects(s, o)[0]:
            self.labels.add('collide-intersecting')
            return
        best = I64_MAX
        for seg, p in ((s, o[0]), (s, o[1]), (o, s[0]), (o, s[1])):
            d = self.squared_distance_point(seg, p)
            if d == 0:
                self.labels.add('collide-zero-endpoint-distance')
                return
            best = min(best, d)
        self.labels.add('collide-within' if best < clearance * clearance else 'collide-clear')

    def reflect_or_project(self, s, p, reflect):
        A = s[0]
        d = self.subI(s[1], A)
        l = L(d[0] * d[0] + d[1] * d[1])
        if not reflect and l == 0:
            self.labels.add('project-degenerate')
            return
        q = self.subI(p, A)
        t = L(d[0] * q[0] + d[1] * q[1])
        if l == 0:
            self.labels.add('reflect-degenerate')
            return
        cx = L(A[0] + rescale(t, d[0], l, wrap64))
        cy = L(A[1] + rescale(t, d[1], l, wrap64))
        if reflect:
            L(L(2 * cx) - p[0])
            L(L(2 * cy) - p[1])
        self.labels.add('reflect' if reflect else 'project')

    @staticmethod
    def canonical(s):
        A, B = s
        p = A[1] - B[1]
        q = B[0] - A[0]
        r = L(L(-p * A[0]) - L(q * A[1]))
        l = L(L(p * p) + L(q * q))
        return p, q, r, l

    @staticmethod
    def det(p, q, r, pt):
        return L(L(L(p * pt[0]) + L(q * pt[1])) + r)

    def line_distance(self, s, pt):
        p, q, r, l = self.canonical(s)
        det = self.det(p, q, r, pt)
        self.labels.add('line-distance-degenerate' if l == 0 else 'line-distance')
        if l > 0 and rescale(det, det, l, wrap64) < 0:
            self.labels.add('line-distance-narrowed-negative')

    def mutual(self, s, o):
        la = sq_norm(*self.subI(s[0], s[1]))
        lb = sq_norm(*self.subI(o[0], o[1]))
        a, b = (o, s) if la < lb else (s, o)
        p, q, r, l = self.canonical(a)
        if l == 0:
            self.labels.add('mutual-degenerate')
            return None
        d1 = self.det(p, q, r, b[0])
        d2 = self.det(p, q, r, b[1])
        q1 = rescale(d1, d1, l, wrap64)
        q2 = rescale(d2, d2, l, wrap64)
        sg = lambda v: (v > 0) - (v < 0)
        return L(sg(d1) * q1), L(sg(d2) * q2)

    def approx(self, s, o, threshold, collinear):
        t = threshold * threshold
        m = self.mutual(s, o)
        if m is None:
            return
        d1, d2 = m
        if collinear:
            if L(abs(d1)) <= t:
                L(abs(d2))
        else:
            L(abs(L(d1 - d2)))
        self.labels.add('approx-collinear' if collinear else 'approx-parallel')


def evaluate(inp):
    """Runs the mirror; returns (classification, labels, first deferral)."""
    run = Run()
    op = inp['op']
    seg = tuple(tuple(p) for p in inp['seg'])
    other = tuple(tuple(p) for p in inp['other']) if 'other' in inp else None
    pt = tuple(inp['point']) if 'point' in inp else None
    try:
        if op in ('squared_distance_point', 'contains_point'):
            run.squared_distance_point(seg, pt)
        elif op == 'distance_point':
            run.distance_point(seg, pt)
        elif op == 'nearest_point_point':
            run.nearest_point_point(seg, pt)
        elif op in ('reflect_point', 'line_project'):
            run.reflect_or_project(seg, pt, op == 'reflect_point')
        elif op == 'perpendicular_seg':
            run.perpendicular_seg(seg, pt)
        elif op == 'parallel_seg':
            run.parallel_seg(seg, pt)
        elif op == 'line_distance':
            run.line_distance(seg, pt)
        elif op in ('squared_distance_seg', 'distance_seg'):
            run.squared_distance_seg(seg, other)
        elif op == 'angle':
            run.subI(seg[0], seg[1])
            run.subI(other[0], other[1])
            run.labels.add('angle')
        elif op in ('nearest_point_seg', 'nearest_points'):
            run.nearest_point_seg(seg, other)
        elif op == 'intersects':
            run.intersects(seg, other)
        elif op == 'intersect':
            run.intersect(seg, other, inp.get('ignore_endpoints', False), inp.get('lines', False))
        elif op == 'intersects_line':
            run.intersects_line(seg, from_bits(inp['slope']), from_bits(inp['offset']))
        elif op == 'collide':
            run.collide(seg, other, inp['clearance'])
        elif op in ('approx_collinear', 'approx_parallel'):
            run.approx(seg, other, inp.get('threshold', 1), op == 'approx_collinear')
        elif op == 'approx_perpendicular':
            perp = run.perpendicular_seg(seg, seg[0])
            run.approx(other, perp, 1, False)
        else:
            raise ValueError(op)
    except Undefined:
        return ('unrepresentable' if run.events else 'undefined'), run.labels, None
    return 'defined', run.labels, (run.events[0] if run.events else None)


# Inputs ---------------------------------------------------------------------------------------
EXTREMES = [INT_MIN, INT_MIN + 1, -2 ** 30, -46341, -46340, -1, 0, 1, 46340, 46341, 2 ** 30,
            INT_MAX - 1, INT_MAX]


def rand_point(rng, scale):
    if scale == 'extreme':
        return [rng.choice(EXTREMES), rng.choice(EXTREMES)]
    if scale == 'full':
        return [rng.randint(INT_MIN, INT_MAX), rng.randint(INT_MIN, INT_MAX)]
    return [rng.randint(-scale, scale), rng.randint(-scale, scale)]


def rand_seg(rng, scale):
    return [rand_point(rng, scale), rand_point(rng, scale)]


def related_segs(rng, scale):
    """Pairs that share a line, an endpoint, a direction or are degenerate."""
    kind = rng.randrange(7)
    a = rand_point(rng, scale)
    d = rand_point(rng, scale if scale != 'full' else 2 ** 20)
    if kind == 0:  # collinear, possibly overlapping
        k1, k2, k3 = rng.randint(-3, 3), rng.randint(-3, 3), rng.randint(-3, 3)
        return [a, [a[0] + d[0], a[1] + d[1]]], [[a[0] + k1 * d[0], a[1] + k1 * d[1]],
                                                  [a[0] + (k2 + k3) * d[0], a[1] + (k2 + k3) * d[1]]]
    if kind == 1:  # shared endpoint
        return [a, rand_point(rng, scale)], [a, rand_point(rng, scale)]
    if kind == 2:  # parallel offset
        o = rand_point(rng, 50)
        b = [a[0] + d[0], a[1] + d[1]]
        return [a, b], [[a[0] + o[0], a[1] + o[1]], [b[0] + o[0], b[1] + o[1]]]
    if kind == 3:  # this a point
        return [a, a], rand_seg(rng, scale)
    if kind == 4:  # other a point
        return rand_seg(rng, scale), [a, a]
    if kind == 5:  # touch at a collinear endpoint
        b = [a[0] + d[0], a[1] + d[1]]
        return [a, b], [b, [b[0] + d[0], b[1] + d[1]]]
    # T-junction: other ends on this segment
    b = [a[0] + 2 * d[0], a[1] + 2 * d[1]]
    m = [a[0] + d[0], a[1] + d[1]]
    return [a, b], [m, [m[0] - d[1], m[1] + d[0]]]


def clamp_seg(s):
    return [[min(max(c, INT_MIN), INT_MAX) for c in p] for p in s]


HAND_SEGS = [
    [[0, 0], [0, 0]], [[0, 0], [10, 0]], [[0, 0], [0, 10]], [[0, 0], [10, 10]], [[10, 0], [0, 0]],
    [[-5, -5], [5, 5]], [[0, 0], [3, 7]], [[1, 1], [1, 1]], [[0, 0], [INT_MAX, 0]],
    [[INT_MIN, 0], [INT_MAX, 0]], [[0, INT_MIN], [0, INT_MAX]], [[INT_MIN, INT_MIN], [INT_MAX, INT_MAX]],
    [[INT_MAX, INT_MAX], [INT_MAX, INT_MAX]], [[INT_MIN, INT_MAX], [INT_MAX, INT_MIN]],
    [[-46340, -46340], [46340, 46340]], [[0, 0], [2 ** 30, 2 ** 30]], [[0, 0], [2 ** 30, -2 ** 30]],
    [[-1, 0], [1, 0]], [[0, -1], [0, 1]], [[5, 5], [5, -5]],
]
HAND_POINTS = [[0, 0], [5, 0], [5, 5], [-5, 3], [10, 0], [11, 0], [9, 1], [0, 10], [INT_MAX, INT_MAX],
               [INT_MIN, INT_MIN], [INT_MIN, INT_MAX], [3, -7], [2, 1], [1, 2], [-1, -1]]
SLOPES = [0.0, -0.0, 1.0, -1.0, 0.5, 2.0, 1e-3, 1e-4, 1e6, -1e6, 9.2e15, 9.3e15, -9.3e15, 1e300,
          math.inf, -math.inf, math.nan, 5e-324, 0.3333333333333333, 1.5, 1000.0, 0.001]
OFFSETS = [0.0, -0.0, 0.5, -0.5, 1.0, 10.0, -10.0, 1e9, -1e9, 2.5, 1e300, math.inf, -math.inf,
           math.nan, 0.4999999999999999, 3.0]

POINT_OPS = ['squared_distance_point', 'distance_point', 'nearest_point_point', 'reflect_point',
             'line_project', 'perpendicular_seg', 'parallel_seg', 'contains_point', 'line_distance']
SEG_OPS = ['squared_distance_seg', 'distance_seg', 'angle', 'nearest_point_seg', 'nearest_points',
           'intersects', 'intersect', 'collide', 'approx_collinear', 'approx_parallel',
           'approx_perpendicular']
SCALES = [12, 1000, 10 ** 6, 2 ** 30, 'full', 'extreme']


def optional_args(rng, op, inp, variant):
    """Adds the optional trailing arguments; variant 0 keeps every C++ default."""
    if op == 'contains_point' and variant:
        inp['threshold'] = rng.choice([0, 1, 3, 4, 100, -1, INT_MAX, INT_MIN])
    if op == 'line_distance' and variant:
        inp['determine_side'] = bool(variant % 2)
    if op == 'intersect' and variant:
        inp['ignore_endpoints'] = rng.random() < 0.5
        if variant > 1:
            inp['lines'] = rng.random() < 0.5
    if op == 'collide':
        inp['clearance'] = rng.choice([0, 1, 2, 5, 10, 100, 10 ** 6, -1, INT_MIN, INT_MAX, 46341])
        if variant:
            inp['actual'] = rng.choice([0, -7, 12345, INT_MIN])
    if op in ('approx_collinear', 'approx_parallel') and variant:
        inp['threshold'] = rng.choice([0, 1, 2, 10, -3, 46341, INT_MAX])
    return inp


def candidates():
    rng = random.Random(SEED)
    out = []
    # Hand-picked boundary and degenerate inputs for every operation and argument variant.
    for op in POINT_OPS:
        for si, s in enumerate(HAND_SEGS):
            for pi, p in enumerate(HAND_POINTS):
                v = (si + pi) % 3
                out.append((f'{op}-hand-{si}-{pi}', optional_args(rng, op, {'op': op, 'seg': s, 'point': p}, v)))
    for op in SEG_OPS:
        for si, s in enumerate(HAND_SEGS):
            for oi, o in enumerate(HAND_SEGS):
                if (si * 7 + oi) % 3 and op != 'intersect':
                    continue
                v = (si + oi) % 3
                out.append((f'{op}-hand-{si}-{oi}', optional_args(rng, op, {'op': op, 'seg': s, 'other': o}, v)))
    for si, s in enumerate(HAND_SEGS):
        for ki, k in enumerate(SLOPES):
            for oi, o in enumerate(OFFSETS):
                if (si + ki + oi) % 4:
                    continue
                out.append((f'intersects_line-hand-{si}-{ki}-{oi}',
                            {'op': 'intersects_line', 'seg': s, 'slope': bits(k), 'offset': bits(o),
                             'initial': [7, -7]}))
    # Fixed-seed random families at every scale.
    for scale in SCALES:
        name = scale if isinstance(scale, str) else f's{scale}'
        for op in POINT_OPS:
            for n in range(40):
                inp = {'op': op, 'seg': rand_seg(rng, scale), 'point': rand_point(rng, scale)}
                if n % 4 == 0:
                    inp['seg'][1] = list(inp['seg'][0])
                out.append((f'{op}-{name}-{n}', optional_args(rng, op, inp, n % 3)))
        for op in SEG_OPS:
            for n in range(40):
                if n % 2:
                    s, o = related_segs(rng, scale)
                    s, o = clamp_seg(s), clamp_seg(o)
                    fam = 'related'
                else:
                    s, o = rand_seg(rng, scale), rand_seg(rng, scale)
                    fam = 'random'
                out.append((f'{op}-{name}-{fam}-{n}',
                            optional_args(rng, op, {'op': op, 'seg': s, 'other': o}, n % 3)))
        for n in range(60):
            s = rand_seg(rng, scale)
            if n % 5 == 0:
                s[1][0] = s[0][0]
            k = rng.choice([rng.uniform(-3, 3), rng.choice(SLOPES), float(rng.randint(-5, 5)),
                            rng.uniform(-1e7, 1e7)])
            o = rng.choice([rng.uniform(-1e6, 1e6), rng.choice(OFFSETS), float(rng.randint(-100, 100))])
            if n % 7 == 0 and s[1][0] != s[0][0]:
                # Put s[0] on the line so the collinear and crossing branches are reached.
                o = float(s[0][1]) - k * float(s[0][0])
            out.append((f'intersects_line-{name}-{n}',
                        {'op': 'intersects_line', 'seg': s, 'slope': bits(k), 'offset': bits(o),
                         'initial': rand_point(rng, 100)}))
    # Exact lines with integer slopes through integer points reach the collinear branch.
    for n in range(40):
        k = rng.randint(-4, 4)
        x0, y0 = rng.randint(-1000, 1000), rng.randint(-1000, 1000)
        dx = rng.randint(1, 50)
        s = [[x0, y0], [x0 + dx, y0 + k * dx]]
        out.append((f'intersects_line-collinear-{n}',
                    {'op': 'intersects_line', 'seg': s, 'slope': bits(float(k)),
                     'offset': bits(float(y0 - k * x0) + rng.choice([0.0, 0.25, -0.25, 0.75])),
                     'initial': [0, 0]}))
    return out


def build():
    corpus, controls = [], []
    stats = Counter()
    labels = Counter()
    deferrals = Counter()
    seen = set()
    for case, inp in candidates():
        assert case not in seen, case
        seen.add(case)
        kind, lab, first = evaluate(inp)
        stats[kind] += 1
        request = {'case': case, 'input': inp, 'unit': UNIT}
        if kind == 'defined':
            for l in lab:
                labels[l] += 1
            deferrals[first or 'none'] += 1
            corpus.append(request)
        elif kind == 'undefined':
            controls.append({'error': UNDEFINED, 'request': request})
    return corpus, controls, stats, labels, deferrals


# checkCollinearOverlap's disjoint return is unreachable: it is called only for segments whose
# bounding boxes already overlap on both axes, so the projected ranges always overlap.
REQUIRED_LABELS = {
    'sqdist-point-before-a', 'sqdist-point-after-b', 'sqdist-point-lagrange', 'nearest-point-degenerate',
    'nearest-point-before-a', 'nearest-point-after-b', 'nearest-point-inside',
    'collinear-endpoint-touch-ignored', 'collinear-overlap', 'collinear-point-interpolated',
    'collinear-point-degenerate-axis', 'intersects-bbox-reject', 'intersects-parallel',
    'lines-collinear-other-point', 'lines-collinear-this-point', 'lines-collinear-midpoint',
    'intersects-param-out-positive', 'intersects-param-out-negative', 'intersects-endpoint-ignored',
    'intersects-crossing-point', 'intersects-crossing', 'sqdist-seg-this-point', 'sqdist-seg-other-point',
    'sqdist-seg-intersecting', 'sqdist-seg-endpoints', 'nearest-seg-intersection', 'nearest-seg-endpoints',
    'line-vertical-hit', 'line-vertical-miss', 'line-collinear', 'line-parallel-miss', 'line-crossing-hit',
    'line-crossing-miss', 'collide-negative-clearance', 'collide-this-point', 'collide-other-point',
    'collide-intersecting', 'collide-within', 'collide-clear', 'project-degenerate', 'reflect-degenerate',
    'reflect', 'project', 'line-distance-degenerate', 'line-distance', 'angle',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='verify the committed files')
    args = parser.parse_args()
    corpus, controls, stats, labels, deferrals = build()
    missing = REQUIRED_LABELS - set(labels)
    if missing:
        sys.exit(f'unreached branches: {sorted(missing)}')
    text = ''.join(json.dumps(r, sort_keys=True, separators=(',', ':')) + '\n' for r in corpus)
    ctext = ''.join(json.dumps(c, sort_keys=True, separators=(',', ':')) + '\n' for c in controls)
    if args.check:
        ok = CASES.read_text(encoding='utf-8') == text and CONTROLS.read_text(encoding='utf-8') == ctext
        if not ok:
            sys.exit('generated corpus or controls differ from the committed files')
    else:
        CASES.write_text(text, encoding='utf-8', newline='\n')
        CONTROLS.write_text(ctext, encoding='utf-8', newline='\n')
    print(f'{len(corpus)} public cases, {len(controls)} undefined-domain controls, '
          f'{stats["unrepresentable"]} dropped (undefined only after a deferred call)')
    print('expected first Rust deferral per public case:', dict(sorted(deferrals.items())))
    print('branch labels:', dict(sorted(labels.items())))


if __name__ == '__main__':
    main()
