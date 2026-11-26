#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and undefined-domain controls for the CIRCLE source unit.

Writes port/cases/<unit>.jsonl (requests whose pinned evaluation is defined) and
port/oracle/fixtures/circle-controls.jsonl (requests whose pinned evaluation reaches signed
overflow or std::abs( INT_MIN ), with the refusal both programs must give). No result is
written anywhere: the split replays only the pinned integer arithmetic exactly (Python
integers for int/int64_t/__int128, IEEE binary64 sqrt and products for Resize's lengths).

A few controls are undefined only after a call the Rust port defers (RotatePoint): they carry
"rust_unported" and check that the oracle refuses while Rust defers to that exact owner first.
Intersect( CIRCLE ) requests whose solutions would need a non-cardinal rotation are generated
only where every solution sum is far inside the int range.

--check verifies the committed files equal the generator output.
"""
import argparse
import json
import math
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.circle.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/circle-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1
MIN_PRECISION_IU = 4
M_SQRT1_2 = float.fromhex('0x1.6a09e667f3bcdp-1')
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined circle operation'}
ROTATE = {'upstream': 'RotatePoint(VECTOR2I&, EDA_ANGLE)',
          'unit': 'libs.kimath.include.application.ui.p001'}
TAN_TAN_BOUND = 2**16


class Undefined(Exception):
    """The pinned evaluation reaches undefined behaviour."""


def bits(v):
    return struct.pack('>d', v).hex()


def raw(h):
    return struct.unpack('>d', bytes.fromhex(h))[0]


def I(v):
    if not INT_MIN <= v <= INT_MAX:
        raise Undefined
    return v


def L(v):
    if not I64_MIN <= v <= I64_MAX:
        raise Undefined
    return v


def trunc_div(a, b):
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) else -q


def wrap64(v):
    return (v + 2**63) % 2**64 - 2**63


def rescale64(n, v, d):
    """util.cpp rescale<int64_t>, Linux __int128 branch; the int64_t return wraps."""
    num = n * v
    half = trunc_div(d, 2)
    return wrap64(trunc_div(num - half if (num < 0) != (d < 0) else num + half, d))


def rounded_hypot(x, y):
    n = x * x + y * y
    f = math.isqrt(n)
    return f + (1 if n - f * f > f else 0)


def norm_i(x, y):
    """VECTOR2<int>::EuclideanNorm."""
    return min(rounded_hypot(x, y), INT_MAX)


def norm_l(x, y):
    """VECTOR2<int64_t>::EuclideanNorm."""
    return min(rounded_hypot(x, y), I64_MAX)


def kiround_int(v):
    """KiROUND<double, int>: saturates (logging, deferred in Rust) outside the range."""
    if v < INT_MIN:
        return INT_MIN, True
    if v > INT_MAX:
        return INT_MAX, True
    return int(math.floor(abs(v) + 0.5)) * (1 if v >= 0 else -1), False


def resize_i(x, y, length):
    """VECTOR2<int>::Resize; returns ((x, y), logged)."""
    if x == 0 and y == 0:
        return (0, 0), False
    if x == INT_MIN or y == INT_MIN:
        raise Undefined  # std::abs
    if abs(x) == abs(y):
        if length == INT_MIN:
            raise Undefined
        nx = ny = float(abs(length)) * M_SQRT1_2
    else:
        xs, ys = x * x, y * y
        ls, ns = xs + ys, length * length
        nx = math.sqrt(float(rescale64(ns, xs, ls)))
        ny = math.sqrt(float(rescale64(ns, ys, ls)))
    kx, lx = kiround_int(nx)
    ky, ly = kiround_int(ny)
    s = (length > 0) - (length < 0)
    return ((-kx if x < 0 else kx) * s, (-ky if y < 0 else ky) * s), lx or ly


def line_project(a, b, p):
    """SEG::LineProject."""
    d = (I(b[0] - a[0]), I(b[1] - a[1]))
    lsq = L(d[0] * d[0] + d[1] * d[1])
    if lsq == 0:
        return a
    t = L(d[0] * I(p[0] - a[0]) + d[1] * I(p[1] - a[1]))
    x = L(a[0] + rescale64(t, d[0], lsq))
    y = L(a[1] + rescale64(t, d[1], lsq))
    clamp = lambda v: max(INT_MIN, min(INT_MAX, v))
    return (clamp(x), clamp(y))


def seg_squared_distance(a, b, p):
    """The integer guard of SEG::SquaredDistance( VECTOR2I ) (the rest is defined)."""
    ab = (b[0] - a[0], b[1] - a[1])
    ap = (p[0] - a[0], p[1] - a[1])
    e = L(L(ap[0] * ab[0]) + L(ap[1] * ab[1]))
    if e <= 0:
        return
    f = min(ab[0] ** 2 + ab[1] ** 2, I64_MAX)
    if e >= f:
        bp = (p[0] - b[0], p[1] - b[1])
        L(L(bp[0] * bp[0]) + L(bp[1] * bp[1]))


def intersect_line(c, r, a, b):
    """CIRCLE::IntersectLine; returns the points' integer evaluation (for Intersect( SEG ))."""
    m = line_project(a, b, c)
    om = norm_l(m[0] - c[0], m[1] - c[1])
    if om > r + MIN_PRECISION_IU:
        return [], False
    if om >= r - MIN_PRECISION_IU:
        return [m], False
    dist = int(math.sqrt(float(r * r - om * om)))
    d = (I(b[0] - a[0]), I(b[1] - a[1]))
    (vx, vy), logged = resize_i(d[0], d[1], dist)
    return [(I(vx + m[0]), I(vy + m[1])), (I(-vx + m[0]), I(-vy + m[1]))], logged


def near_far(c, r, p, furthest):
    v = [I(c[0] - p[0]), I(c[1] - p[1])] if furthest else [I(p[0] - c[0]), I(p[1] - c[1])]
    if v == [0, 0]:
        v[0] = 1
    (rx, ry), logged = resize_i(v[0], v[1], r)
    I(rx + c[0])
    I(ry + c[1])
    return logged


def intersect_circle(c1, r1, c2, r2):
    """CIRCLE::Intersect( CIRCLE ) up to RotatePoint: returns None (no points) or (x, y)."""
    vx, vy = c2[0] - c1[0], c2[1] - c1[1]
    d = norm_l(vx, vy)
    if d > r1 + r2 or d < abs(r1 - r2):
        return None
    if d == 0:
        return None
    num = L(L(L(d * d) + r1 * r1) - r2 * r2)
    x = trunc_div(num, 2 * d)
    rem = L(r1 * r1 - L(x * x))
    if rem < 0:
        return None
    y, _ = kiround_int(math.sqrt(float(rem)))
    return x, y, (vx, vy)


def cardinal_solutions(c1, x, y, vec):
    """The rotated solutions when EDA_ANGLE( vecCtoC ) is 0, 90, 180 or 270 degrees."""
    vx, vy = vec
    if vy == 0 and vx > 0:
        rot = lambda px, py: (px, py)  # -0 degrees
    elif vy == 0:
        rot = lambda px, py: (-px, -py)  # -(-180) = 180
    elif vx == 0 and vy > 0:
        rot = lambda px, py: (-py, px)  # -90 -> 270
    elif vx == 0:
        rot = lambda px, py: (py, -px)  # -(-90) = 90
    else:
        return None
    sols = [rot(wrap32(x), wrap32(y))]
    if y != 0:
        sols.append(rot(wrap32(x), wrap32(-y)))
    return [(s[0] + c1[0], s[1] + c1[1]) for s in sols]


def wrap32(v):
    return (v + 2**31) % 2**32 - 2**31


def fits(v):
    return INT_MIN <= v <= INT_MAX


class Corpus:
    def __init__(self):
        self.cases, self.controls, self.names = [], [], set()

    def request(self, name, inp):
        assert name not in self.names, name
        self.names.add(name)
        return {'unit': UNIT, 'case': name, 'input': inp}

    def add(self, name, inp, classify=None):
        """classify() raises Undefined for an undefined request."""
        try:
            if classify:
                classify()
        except Undefined:
            self.controls.append({'error': UNDEFINED, 'request': self.request(f'control-{name}', inp)})
            return
        self.cases.append(self.request(name, inp))

    def add_deferred_control(self, name, inp):
        self.controls.append({'error': UNDEFINED, 'request': self.request(f'control-{name}', inp),
                              'rust_unported': ROTATE})


def circ(c, r):
    return {'center': [c[0], c[1]], 'radius': r}


LIMITS = [INT_MIN, INT_MIN + 1, -2**30, -1, 0, 1, 2**30, INT_MAX - 1, INT_MAX]
RADII = [0, 1, -1, 5, 100, 12345, -12345, 2**30, INT_MAX, INT_MIN, INT_MIN + 1]
F64_VALUES = [0.0, -0.0, 1.0, -1.0, 0.5, 3.0, -7.25, 1e-310, 5e-324, -5e-324, 1e300, -1e300,
              1.7976931348623157e308, 123456.789, -98765.4321, 2.0**53, 2.0**31, 2.0**31 + 0.5,
              -2.0**31 - 1.0, 1e15, math.inf, -math.inf]
NANS = ['7ff8000000000000', 'fff8000000000123', '7ff4000000000abc', '7ff80000000000aa']


def generate():
    c = Corpus()
    rng = random.Random(0xC1C1E)

    # Constructors, copy and the defaulted operator==.
    c.add('default', {'op': 'default'})
    for i, x in enumerate(LIMITS):
        for j, r in enumerate(RADII):
            y = LIMITS[(i + j) % len(LIMITS)]
            c.add(f'new-{i}-{j}', {'op': 'new', 'center': [x, y], 'radius': r})
            c.add(f'copy-{i}-{j}', {'op': 'copy', 'circle': circ((y, x), r)})
    base = ((7, -3), 9)
    for i, (cc, rr) in enumerate([((7, -3), 9), ((7, -3), 10), ((8, -3), 9), ((7, -2), 9),
                                  ((-3, 7), 9), ((7, -3), -9), ((INT_MIN, INT_MAX), INT_MIN),
                                  ((0, 0), 0)]):
        c.add(f'equals-{i}', {'op': 'equals', 'circle': circ(*base), 'other': circ(cc, rr)})
        c.add(f'equals-self-{i}', {'op': 'equals', 'circle': circ(cc, rr), 'other': circ(cc, rr)})

    # Contains: both overloads around the circumference band, the centre and the limits.
    for i, (cc, r) in enumerate([((0, 0), 100), ((1000, -2000), 5), ((-7, 7), 0), ((3, 4), -3),
                                 ((0, 0), 4), ((0, 0), INT_MAX), ((INT_MIN, INT_MIN), INT_MAX),
                                 ((INT_MAX, 0), INT_MIN), ((12345, 6789), 3000)]):
        offsets = sorted({0, 1, -1, 3, -3, 4, -4, 5, -5, 6, -6})
        pts = [(cc[0], cc[1])]
        for k in offsets:
            for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1)):
                pts.append((cc[0] + dx * (r + k), cc[1] + dy * (r + k)))
            # 3-4-5 directions: distances r + k exactly when 5 divides the length.
            if (r + k) % 5 == 0:
                s = (r + k) // 5
                pts.append((cc[0] + 3 * s, cc[1] + 4 * s))
        pts += [(INT_MAX, INT_MAX), (INT_MIN, INT_MIN), (INT_MIN, INT_MAX), (0, 0)]
        for j, p in enumerate(pts):
            if not (fits(p[0]) and fits(p[1])):
                continue
            for ov in ('const', 'non_const'):
                inp = {'op': 'contains', 'circle': circ(cc, r), 'point': list(p), 'overload': ov}
                classify = (lambda p=p, cc=cc: (I(p[0] - cc[0]), I(p[1] - cc[1]))) \
                    if ov == 'non_const' else None
                c.add(f'contains-{ov}-{i}-{j}', inp, classify)
    for i in range(300):
        scale = rng.choice([10, 1000, 2**20, 2**29, 2**31])
        cc = (rng.randint(-scale, scale - 1), rng.randint(-scale, scale - 1))
        cc = (max(INT_MIN, min(INT_MAX, cc[0])), max(INT_MIN, min(INT_MAX, cc[1])))
        r = rng.randint(0, scale)
        r = min(r, INT_MAX)
        ang = rng.random() * 2 * math.pi
        k = rng.randint(-6, 6)
        p = (cc[0] + int((r + k) * math.cos(ang)), cc[1] + int((r + k) * math.sin(ang)))
        p = (max(INT_MIN, min(INT_MAX, p[0])), max(INT_MIN, min(INT_MAX, p[1])))
        for ov in ('const', 'non_const'):
            inp = {'op': 'contains', 'circle': circ(cc, r), 'point': list(p), 'overload': ov}
            classify = (lambda p=p, cc=cc: (I(p[0] - cc[0]), I(p[1] - cc[1]))) \
                if ov == 'non_const' else None
            c.add(f'contains-rand-{ov}-{i}', inp, classify)

    # NearestPoint / FurthestPoint (int): random, the centre special case, diagonal and axis
    # Resize branches, every radius sign, and the int limits.
    for op in ('nearest_i32', 'furthest_i32'):
        far = op == 'furthest_i32'
        for i, (cc, r, p) in enumerate([
                ((10, 10), 5, (20, 10)), ((10, 10), 5, (10, 10)), ((10, 10), 5, (13, 14)),
                ((10, 10), 5, (15, 15)), ((10, 10), -5, (20, 10)), ((10, 10), 0, (20, 10)),
                ((0, 0), INT_MAX, (7, 0)), ((0, 0), INT_MAX, (0, -7)), ((0, 0), INT_MAX, (7, 7)),
                ((0, 0), INT_MIN, (7, 0)), ((0, 0), INT_MIN, (0, 7)), ((0, 0), INT_MIN, (7, 7)),
                ((0, 0), INT_MIN, (0, 0)), ((0, 0), INT_MIN + 1, (7, 7)),
                ((0, 0), 100, (INT_MIN, 0)), ((0, 0), 100, (0, INT_MIN)),
                ((0, 0), 100, (INT_MAX, INT_MAX)), ((0, 0), 100, (INT_MIN + 1, INT_MIN + 1)),
                ((INT_MIN, 0), 100, (INT_MAX, 0)), ((INT_MAX, 0), 100, (INT_MIN, 0)),
                ((INT_MAX - 100, 0), 100, (INT_MAX, 0)), ((INT_MAX - 99, 0), 100, (INT_MAX, 0)),
                ((INT_MAX - 100, 0), 100, (0, 0)), ((INT_MAX - 99, 0), 100, (0, 0)),
                ((INT_MIN + 100, 5), 100, (0, 5)), ((INT_MIN + 99, 5), 100, (0, 5)),
                ((INT_MIN + 100, 5), 100, (INT_MIN, 5)), ((INT_MIN + 99, 5), 100, (INT_MIN, 5)),
                ((5, INT_MAX - 3), 3, (5, 0)), ((5, INT_MAX - 2), 3, (5, 0)),
                ((5, INT_MAX - 3), 3, (5, INT_MAX)), ((5, INT_MAX - 2), 3, (5, INT_MAX)),
                ((0, 0), 1, (3, 4)), ((0, 0), 2, (1, 1)), ((0, 0), 1, (1, 1)),
                ((-1, -1), 2**30, (INT_MAX, INT_MAX)), ((1, 1), 2**30, (INT_MIN, INT_MIN))]):
            inp = {'op': op, 'circle': circ(cc, r), 'point': list(p)}
            c.add(f'{op}-hand-{i}', inp, lambda cc=cc, r=r, p=p, far=far: near_far(cc, r, p, far))
        for i in range(250):
            scale = rng.choice([8, 1000, 2**20, 2**28])
            cc = (rng.randint(-scale, scale), rng.randint(-scale, scale))
            r = rng.randint(-scale, scale)
            p = (rng.randint(-scale, scale), rng.randint(-scale, scale))
            if i % 10 == 0:
                p = cc
            inp = {'op': op, 'circle': circ(cc, r), 'point': list(p)}
            c.add(f'{op}-rand-{i}', inp, lambda cc=cc, r=r, p=p, far=far: near_far(cc, r, p, far))

    # NearestPoint / FurthestPoint (double): defined for every input; exact IEEE bits,
    # signed zeros at the centre, infinities, single and distinct NaN payloads.
    for op in ('nearest_f64', 'furthest_f64'):
        for i, (cc, r) in enumerate([((0, 0), 5), ((10, -20), 7), ((3, 3), 0), ((3, 3), -4),
                                     ((0, 0), INT_MAX), ((INT_MIN, INT_MAX), INT_MIN)]):
            vals = F64_VALUES + [float(cc[0]), float(cc[1])]
            for j in range(40):
                x = vals[(j * 7 + i) % len(vals)]
                y = vals[(j * 11 + 3 * i + 1) % len(vals)]
                c.add(f'{op}-pool-{i}-{j}', {'op': op, 'circle': circ(cc, r),
                                             'point': [bits(x), bits(y)]})
            # aP equal to the centre (signed zeros), so vec.x becomes 1.
            for j, (dx, dy) in enumerate([(0.0, 0.0), (0.0, -0.0), (-0.0, 0.0), (-0.0, -0.0)]):
                px = float(cc[0]) + dx if dx == 0.0 and math.copysign(1, dx) > 0 else float(cc[0])
                c.add(f'{op}-centre-{i}-{j}', {'op': op, 'circle': circ(cc, r),
                                               'point': [bits(px), bits(float(cc[1]))]})
            for j, n in enumerate(NANS):
                m = NANS[(j + 1) % len(NANS)]
                for k, pt in enumerate([[n, bits(1.0)], [bits(-2.5), n], [n, n], [n, m],
                                        [n, bits(math.inf)], [bits(math.inf), bits(math.inf)],
                                        [bits(-math.inf), bits(math.inf)]]):
                    c.add(f'{op}-special-{i}-{j}-{k}', {'op': op, 'circle': circ(cc, r),
                                                         'point': pt})
        for i in range(150):
            scale = rng.choice([1.0, 1e3, 1e9, 1e150])
            cc = (rng.randint(-2**20, 2**20), rng.randint(-2**20, 2**20))
            r = rng.randint(-2**20, 2**20)
            p = [bits(rng.uniform(-scale, scale)), bits(rng.uniform(-scale, scale))]
            c.add(f'{op}-rand-{i}', {'op': op, 'circle': circ(cc, r), 'point': p})

    # Intersect( CIRCLE ): apart, nested, co-centred, tangent, crossing (RotatePoint), the
    # remainder test, negative radii, and int64 overflow of equation (3).
    def ic(name, c1, r1, c2, r2):
        inp = {'op': 'intersect_circle', 'circle': circ(c1, r1), 'other': circ(c2, r2)}

        def classify():
            res = intersect_circle(c1, r1, c2, r2)
            if res is None:
                return
            x, y, vec = res
            sols = cardinal_solutions(c1, x, y, vec)
            if sols is None:
                # A general rotation: generated only far inside the int range.
                bound = abs(c1[0]) + abs(c1[1]) + 2 * (abs(x) + abs(y)) + 4
                assert bound <= INT_MAX, name
                return
            for s in sols:
                I(s[0])
                I(s[1])

        c.add(name, inp, classify)

    for i, (c1, r1, c2, r2) in enumerate([
            ((0, 0), 5, (11, 0), 5), ((0, 0), 5, (10, 0), 5), ((0, 0), 5, (8, 0), 5),
            ((0, 0), 5, (0, 8), 5), ((0, 0), 5, (-8, 0), 5), ((0, 0), 5, (0, -8), 5),
            ((0, 0), 5, (6, 8), 5), ((0, 0), 10, (3, 0), 7), ((0, 0), 10, (2, 0), 7),
            ((0, 0), 10, (1, 0), 7), ((0, 0), 5, (0, 0), 5), ((0, 0), 5, (0, 0), 3),
            ((0, 0), -5, (3, 0), 5), ((0, 0), 5, (3, 0), -5), ((0, 0), -5, (3, 0), -5),
            ((0, 0), 0, (0, 0), 0), ((0, 0), 0, (5, 0), 5), ((0, 0), 1, (1, 1), 1),
            ((100, 100), 50, (130, 140), 50), ((-1000, 2000), 1000, (1000, 2000), 1000),
            ((-1000, 2000), 1000, (1000, 2000), 999), ((0, 0), 3, (1, 1), 2),
            ((0, 0), INT_MAX, (0, 0), INT_MAX), ((0, 0), INT_MAX, (INT_MAX, 0), INT_MAX),
            ((0, 0), INT_MAX, (INT_MAX, INT_MAX), INT_MAX),
            ((0, 0), INT_MAX, (2**31 - 1, 2**31 - 2), INT_MAX),
            ((INT_MIN, 0), 2**30, (INT_MIN + 2**30, 0), 2**30),
            ((-1250000000, 0), 1000000000, (1250000000, 0), 2000000000),
            ((0, -1250000000), 2000000000, (0, 1250000000), 1000000000),
            ((INT_MIN, INT_MIN), INT_MAX, (INT_MAX, INT_MAX), INT_MAX),
            ((0, 0), INT_MIN, (5, 0), INT_MAX), ((0, 0), 2**30, (2**31 - 1, 0), 2**30),
            ((0, -2000000000), 1500000000, (0, 1000000000), 1600000000)]):
        if not all(fits(v) for v in (*c1, *c2)):
            continue
        ic(f'intersect_circle-hand-{i}', c1, r1, c2, r2)
    # No negative-remainder cases are established here. The former equal-radius
    # search emitted none: d <= 2*r implies trunc(d/2) <= r. Keep that coverage
    # gap explicit instead of claiming the attempted search generated cases.
    for i in range(400):
        scale = rng.choice([20, 1000, 2**20, 2**28])
        c1 = (rng.randint(-scale, scale), rng.randint(-scale, scale))
        r1 = rng.randint(-scale // 8, scale)
        if i % 3 == 0:
            # Near-intersecting pairs.
            ang = rng.random() * 2 * math.pi
            dist = rng.randint(0, 2 * max(1, r1))
            c2 = (c1[0] + int(dist * math.cos(ang)), c1[1] + int(dist * math.sin(ang)))
            r2 = max(0, r1 + rng.randint(-3, 3))
        else:
            c2 = (rng.randint(-scale, scale), rng.randint(-scale, scale))
            r2 = rng.randint(-scale // 8, scale)
        if i % 17 == 0:
            c2 = c1
        ic(f'intersect_circle-rand-{i}', c1, r1, c2, r2)
    # Undefined after RotatePoint: the solution sum overflows (cardinal geometry, exact).
    for i, (c1, r1, c2, r2) in enumerate([((0, INT_MAX - 50), 100, (16, INT_MAX - 50), 100),
                                          ((0, INT_MIN + 50), 100, (16, INT_MIN + 50), 100),
                                          ((INT_MAX - 50, 0), 100, (INT_MAX - 50, 16), 100),
                                          ((INT_MAX - 3, 7), 30, (INT_MAX - 19, 7), 40)]):
        res = intersect_circle(c1, r1, c2, r2)
        sols = cardinal_solutions(c1, res[0], res[1], res[2])
        assert not all(fits(v) for s in sols for v in s), i
        c.add_deferred_control(f'intersect_circle-after-rotate-{i}',
                               {'op': 'intersect_circle', 'circle': circ(c1, r1),
                                'other': circ(c2, r2)})

    # IntersectLine and Intersect( SEG ): miss, tangent band, chords, degenerate lines,
    # endpoints and the containment threshold, and the int/int64 limits.
    def lines(name, cc, r, a, b):
        for op in ('intersect_line', 'intersect_seg'):
            inp = {'op': op, 'circle': circ(cc, r), 'seg': [list(a), list(b)]}

            def classify(op=op):
                pts, _ = intersect_line(cc, r, a, b)
                if op == 'intersect_seg':
                    for p in pts:
                        seg_squared_distance(a, b, p)

            c.add(f'{op}-{name}', inp, classify)

    for k in range(-7, 8):
        lines(f'band-h-{k}', (0, 0), 100, (-1000, 100 + k), (1000, 100 + k))
        lines(f'band-v-{k}', (50, -50), 100, (150 + k, -1000), (150 + k, 1000))
        lines(f'band-d-{k}', (0, 0), 1000, (-5000, 1414 + k - 5000), (5000, 1414 + k + 5000))
    for i, (cc, r, a, b) in enumerate([
            ((0, 0), 100, (-1000, 60), (1000, 60)), ((0, 0), 100, (0, 60), (1000, 60)),
            ((0, 0), 100, (80, 60), (1000, 60)), ((0, 0), 100, (81, 60), (1000, 60)),
            ((0, 0), 100, (79, 60), (1000, 60)), ((0, 0), 100, (-80, 60), (80, 60)),
            ((0, 0), 100, (-79, 61), (79, 61)), ((0, 0), 100, (-1000, 0), (1000, 0)),
            ((0, 0), 100, (1000, 0), (-1000, 0)), ((0, 0), 100, (30, 40), (30, 40)),
            ((0, 0), 100, (300, 400), (300, 400)), ((0, 0), 100, (96, 0), (96, 0)),
            ((0, 0), 0, (-10, 0), (10, 0)), ((0, 0), 4, (-10, 0), (10, 0)),
            ((0, 0), 5, (-10, 0), (10, 0)), ((0, 0), -5, (-10, 0), (10, 0)),
            ((0, 0), -1, (-10, 0), (10, 0)), ((0, 0), -2, (-10, 0), (10, 0)),
            ((0, 0), INT_MAX, (-10, 0), (10, 0)), ((0, 0), INT_MAX, (INT_MIN, -1), (INT_MAX, -1)),
            ((0, 0), 2**30, (INT_MIN, 0), (INT_MAX, 0)), ((0, 0), 2**30, (-1, INT_MIN), (-1, INT_MAX)),
            ((0, 0), 100, (INT_MAX, 0), (-1, 0)), ((0, 0), 100, (INT_MAX, 0), (0, 0)),
            ((0, 0), 100, (-1, 5), (INT_MAX, 5)), ((0, 0), 100, (INT_MIN, 5), (-1, 5)),
            ((0, 0), 100, (INT_MIN, 5), (0, 5)), ((0, 0), 100, (INT_MIN, INT_MIN), (INT_MAX, INT_MAX)),
            ((0, 0), 100, (-2**30, -2**30), (2**30, 2**30)),
            ((INT_MAX - 50, 0), 100, (INT_MAX - 50, -1000), (INT_MAX - 50, 1000)),
            ((INT_MAX - 50, 0), 100, (INT_MAX - 100, 1000), (INT_MAX - 100, -1000)),
            ((0, INT_MAX - 50), 100, (-1000, INT_MAX - 50), (1000, INT_MAX - 50)),
            ((0, INT_MAX - 50), 100, (-1000, INT_MAX - 120), (1000, INT_MAX - 120)),
            ((0, 0), 2**30, (INT_MIN + 1, 1), (INT_MAX, 1)), ((5, 5), 2**31 - 10, (INT_MIN, 0), (-1, 0)),
            ((0, 0), 3037000, (2**31 - 1, -2**31), (-2**31, 2**31 - 1)),
            ((1000, 1000), 500, (0, 0), (3, 4)), ((1000, 1000), 500, (2000, 0), (0, 2000))]):
        lines(f'hand-{i}', cc, r, a, b)
    for i in range(500):
        scale = rng.choice([30, 1000, 2**20, 2**28])
        cc = (rng.randint(-scale, scale), rng.randint(-scale, scale))
        r = rng.randint(-scale // 10, scale)
        if i % 4 == 0:
            # A chord at a chosen distance from the centre.
            ang = rng.random() * 2 * math.pi
            off = rng.randint(0, max(1, r) + 6)
            m = (cc[0] + int(off * math.cos(ang)), cc[1] + int(off * math.sin(ang)))
            dx, dy = -math.sin(ang), math.cos(ang)
            span = rng.randint(1, 3 * max(1, abs(r)) + 10)
            a = (m[0] - int(span * dx), m[1] - int(span * dy))
            b = (m[0] + int(span * dx) + rng.randint(-1, 1), m[1] + int(span * dy))
        else:
            a = (rng.randint(-scale, scale), rng.randint(-scale, scale))
            b = (rng.randint(-scale, scale), rng.randint(-scale, scale))
        if i % 29 == 0:
            b = a
        lines(f'rand-{i}', cc, r, a, b)

    # ConstructFromTanTanPt inside the adapter's audited domain: exactly parallel pairs or
    # angles whose sine is at least 1/8, coordinates within 2^16, non-degenerate lines.
    def tan_tan(name, init, la, lb, p):
        assert all(abs(v) <= TAN_TAN_BOUND for s in (la, lb) for q in s for v in q), name
        assert all(abs(v) <= TAN_TAN_BOUND for v in p) and la[0] != la[1] and lb[0] != lb[1]
        da = (la[1][0] - la[0][0], la[1][1] - la[0][1])
        db = (lb[1][0] - lb[0][0], lb[1][1] - lb[0][1])
        cross = da[0] * db[1] - da[1] * db[0]
        assert cross == 0 or cross * cross * 64 >= (da[0]**2 + da[1]**2) * (db[0]**2 + db[1]**2)
        c.add(f'tan_tan_pt-{name}', {'op': 'tan_tan_pt', 'circle': circ(*init),
                                     'line_a': [list(q) for q in la], 'line_b': [list(q) for q in lb],
                                     'point': list(p)})

    init = ((11, -13), 17)
    configs = [
        (((0, 0), (100, 0)), ((0, 50), (100, 50)), (40, 10)),
        (((0, 0), (100, 0)), ((0, 50), (100, 50)), (40, 25)),
        (((0, 0), (100, 0)), ((0, 50), (100, 50)), (-40, 60)),
        (((0, 0), (0, 100)), ((40, 0), (40, 100)), (10, 10)),
        (((0, 0), (100, 100)), ((0, 40), (100, 140)), (50, 70)),
        (((0, 0), (100, 0)), ((100, 50), (0, 50)), (5, 5)),
        (((0, 0), (100, 0)), ((0, 0), (0, 100)), (10, 20)),
        (((0, 0), (100, 0)), ((0, 0), (0, 100)), (0, 0)),
        (((0, 0), (100, 0)), ((0, 0), (0, 100)), (30, 30)),
        (((0, 0), (100, 0)), ((0, 0), (0, 100)), (-30, 20)),
        (((0, 0), (100, 0)), ((0, 0), (100, 100)), (60, 20)),
        (((0, 0), (100, 0)), ((0, 0), (100, 100)), (60, 0)),
        (((0, 0), (100, 0)), ((0, 0), (-100, 100)), (-20, 60)),
        (((10, 10), (110, 10)), ((10, 10), (60, 96)), (50, 30)),
        (((0, 0), (1000, 0)), ((0, 0), (1000, 1000)), (500, 1)),
        (((-500, 300), (700, 300)), ((200, -800), (200, 900)), (-100, 650)),
        (((0, 0), (65536, 0)), ((0, 0), (0, 65536)), (40000, 30000)),
        (((-65536, -65536), (65536, 65536)), ((-65536, 65536), (65536, -65536)), (100, 5000)),
        (((0, 0), (100, 0)), ((0, 0), (0, 100)), (100, 0)),
        (((0, 0), (100, 0)), ((300, 0), (300, 100)), (200, 50)),
        (((0, 0), (8, 1)), ((0, 0), (1, 8)), (20, 20)),
        (((0, 0), (100, 0)), ((5, 50), (95, 50)), (50, 25)),
        (((0, 0), (3, 0)), ((0, 2), (3, 2)), (1, 1)),
        (((0, 0), (100, 0)), ((0, 1), (100, 1)), (50, 0)),
    ]
    for i, (la, lb, p) in enumerate(configs):
        tan_tan(f'hand-{i}', init, la, lb, p)
        tan_tan(f'hand-swap-{i}', ((0, 0), 0), lb, la, p)
    dirs = [(1, 0), (0, 1), (1, 1), (1, -1), (2, 1), (1, 2), (3, 4), (-4, 3), (5, -2), (7, 3)]
    for i in range(60):
        da, db = rng.sample(dirs, 2)
        if rng.random() < 0.25:
            db = da
        cross = da[0] * db[1] - da[1] * db[0]
        if cross != 0 and cross * cross * 64 < (da[0]**2 + da[1]**2) * (db[0]**2 + db[1]**2):
            continue  # Outside the adapter's audited angle domain.
        o = (rng.randint(-2000, 2000), rng.randint(-2000, 2000))
        ka, kb = rng.randint(50, 3000), rng.randint(50, 3000)
        shift = (rng.randint(-500, 500), rng.randint(-500, 500)) if db == da else (0, 0)
        la = (o, (o[0] + da[0] * ka, o[1] + da[1] * ka))
        ob = (o[0] + shift[0], o[1] + shift[1])
        lb = (ob, (ob[0] + db[0] * kb, ob[1] + db[1] * kb))
        if la[0] == lb[0] and lb[0] == lb[1]:
            continue
        p = (o[0] + rng.randint(-1500, 1500), o[1] + rng.randint(-1500, 1500))
        tan_tan(f'rand-{i}', init, la, lb, p)
    return c


def lines_text(items):
    return ''.join(json.dumps(x, separators=(',', ':'), sort_keys=True) + '\n' for x in items)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    c = generate()
    outputs = {CASES: lines_text(c.cases), CONTROLS: lines_text(c.controls)}
    if args.check:
        stale = [str(p.relative_to(ROOT)) for p, text in outputs.items()
                 if not p.exists() or p.read_bytes() != text.encode()]
        if stale:
            raise SystemExit(f'stale generated files: {stale}')
    else:
        for p, text in outputs.items():
            p.write_bytes(text.encode())
    print(f'{len(c.cases)} public cases, {len(c.controls)} undefined-domain controls')


if __name__ == '__main__':
    main()
