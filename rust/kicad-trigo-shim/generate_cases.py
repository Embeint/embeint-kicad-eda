#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and undefined-domain controls for the trigo.cpp source unit.

Writes port/cases/<unit>.jsonl (requests whose pinned evaluation is defined) and
port/oracle/fixtures/trigo-controls.jsonl (requests whose pinned evaluation reaches signed
overflow, the negation or std::abs of the minimum, or a std::minmax whose values break the strict
weak ordering requirement (a NaN with two non-equivalent values), with the refusal both programs
must give). No result is written anywhere: the split replays only the pinned integer
arithmetic exactly (Python integers for int, int64_t and __int128; IEEE binary64 products and
quotients, which Python evaluates exactly as the oracle does without contraction).

Rotations whose angle is not a cardinal or diagonal shortcut use sin/cos, which this generator
does not evaluate: such a request is admitted only where every rotated value within the rotation's
magnitude bound keeps the following int sums in range, and is otherwise dropped. Angles taken by
EDA_ANGLE( VECTOR2D ) from atan2 are treated the same way.

A few controls are undefined only after a call the Rust port defers (the trigo.h RotatePoint
wrapper in CalcArcMid, or a logging KiROUND): they carry "rust_unported" and check that the oracle
refuses while Rust defers to that exact owner first.

--check verifies the committed files equal the generator output.
"""
import argparse
import decimal
import json
import math
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.trigo.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/trigo-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1
M_SQRT1_2 = float.fromhex('0x1.6a09e667f3bcdp-1')
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined trigo operation'}
HEADER_ROTATE = {'upstream': 'RotatePoint(VECTOR2I&, const VECTOR2I&, EDA_ANGLE)',
                 'unit': 'libs.kimath.include.application.ui.p001'}
LOG_OVERFLOW = {'upstream': 'kimathLogOverflow', 'unit': 'libs.kimath.src.math.util.application.ui.p001'}
SEED = 20261127


class Undefined(Exception):
    """The pinned evaluation reaches undefined behaviour."""


class Unknown(Exception):
    """The split cannot be decided without evaluating sin, cos or atan2."""


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


def sub_i(a, b):
    return (I(a[0] - b[0]), I(a[1] - b[1]))


def dot_l(a, b):
    return L(L(a[0] * b[0]) + L(a[1] * b[1]))


class Log:
    """Records the first logging KiROUND; a later undefined operation is then reached in C++ only
    after the call the Rust port defers."""

    def __init__(self):
        self.logged = False


def kiround(v, log):
    """KiROUND<double, int>( v ): saturating, logging; llround halves away from zero."""
    if v != v:
        log.logged = True
        return 0
    if v >= float(INT_MAX) + 0.5:
        log.logged = True
        return INT_MAX
    if v - float(INT_MIN) <= -0.5:
        log.logged = True
        return INT_MIN
    a = abs(v)
    r = math.floor(a)
    if a - r >= 0.5:
        r += 1
    return int(r if v >= 0 else -r)


# ---------------------------------------------------------------------------------------------
# EDA_ANGLE replay (degrees). Returns None where atan2 decides the value.

def fold(v, period):
    if not math.isfinite(v):
        return 0.0
    if abs(v) < 2.0 * period:
        return v
    return math.fmod(v, period) + math.copysign(period, v)


def normalize(v):
    v = fold(v, 360.0)
    while v < -0.0:
        v += 360.0
    while v >= 360.0:
        v -= 360.0
    return v


def normalize180(v):
    v = fold(v, 360.0)
    while v <= -180.0:
        v += 360.0
    while v > 180.0:
        v -= 360.0
    return v


def vector_angle(x, y):
    x, y = float(x), float(y)
    if x == 0.0 and y == 0.0:
        return 0.0
    if y == 0.0:
        return 0.0 if x >= 0 else -180.0
    if x == 0.0:
        return 90.0 if y >= 0.0 else -90.0
    if x == y:
        return 45.0 if x >= 0.0 else -180.0 + 45.0
    if x == -y:
        return -45.0 if x >= 0.0 else 180.0 - 45.0
    return None


def rotate_i(x, y, angle, log):
    """RotatePoint( int*, int*, const EDA_ANGLE& ). angle None: unknown (atan2) angle."""
    if angle is not None:
        a = normalize(angle)
        if a == 0.0:
            return (x, y)
        if a == 90.0:
            return (y, I(-x))
        if a == 180.0:
            return (I(-x), I(-y))
        if a == 270.0:
            return (I(-y), x)
        shortcut = {45.0: (M_SQRT1_2, M_SQRT1_2), 135.0: (M_SQRT1_2, -M_SQRT1_2),
                    225.0: (-M_SQRT1_2, -M_SQRT1_2), 315.0: (-M_SQRT1_2, M_SQRT1_2)}
        if a in shortcut:
            s, c = shortcut[a]
            return (kiround(float(y) * s + float(x) * c, log),
                    kiround(float(y) * c - float(x) * s, log))
    # A general rotation keeps |result| within the magnitude, plus one for rounding. KiROUND
    # saturates, so the rotation itself is always defined.
    return ('bound', math.isqrt(x * x + y * y) + 2)


def add_rotated(rotated, c):
    """The int sums of a rotated point with a centre."""
    if rotated[0] == 'bound':
        bound = rotated[1]
        if bound >= 2**31 - 1:
            raise Unknown
        for k in (0, 1):
            if not (INT_MIN <= c[k] - bound and c[k] + bound <= INT_MAX):
                raise Unknown
        return None
    return (I(rotated[0] + c[0]), I(rotated[1] + c[1]))


def logged_then(log, fn):
    """Runs a replay step; an undefined operation after a logging KiROUND is a Rust deferral."""
    try:
        return fn()
    except Undefined:
        if log.logged:
            raise DeferredUndefined(LOG_OVERFLOW)
        raise


class DeferredUndefined(Exception):
    def __init__(self, owner):
        super().__init__(owner)
        self.owner = owner


# ---------------------------------------------------------------------------------------------
# Replays of the pinned bodies.

def replay_point_on_segment(s, e, p):
    seg = sub_i(e, s)
    pt = sub_i(p, s)
    if seg[0] * pt[1] - seg[1] * pt[0]:
        return
    L(seg[0] * pt[0] + seg[1] * pt[1])
    L(pt[0] * pt[0] + pt[1] * pt[1])


def replay_segment_intersects(a1, a2, b1, b2, point):
    log = Log()
    dxa, dya = a2[0] - a1[0], a2[1] - a1[1]
    dxb, dyb = b2[0] - b1[0], b2[1] - b1[1]
    dxab, dyab = b1[0] - a1[0], b1[1] - a1[1]
    den = L(L(dya * dxb) - L(dyb * dxa))
    if den == 0:
        return
    num_a = L(L(dyab * dxb) - L(dyb * dxab))
    num_b = L(L(dyab * dxa) - L(dya * dxab))
    if point is not None:
        rx = kiround(float(dxa) * float(num_a) / float(den), log)
        logged_then(log, lambda: I(a1[0] + rx))
        ry = kiround(float(dya) * float(num_b) / float(den), log)
        logged_then(log, lambda: I(a1[1] + ry))
    if den < 0 and I64_MIN in (den, num_a, num_b):
        if log.logged:
            raise DeferredUndefined(LOG_OVERFLOW)
        raise Undefined


def replay_squared_distance(a, b, p):
    ab = (b[0] - a[0], b[1] - a[1])
    ap = (p[0] - a[0], p[1] - a[1])
    e = dot_l(ap, ab)
    if e <= 0:
        return
    f = min(ab[0] ** 2 + ab[1] ** 2, I64_MAX)
    if e >= f:
        bp = (p[0] - b[0], p[1] - b[1])
        dot_l(bp, bp)


def replay_segment_hit(ref, s, e, dist):
    delta = sub_i(s, ref)
    xmin, xmax = min(s[0], e[0]), max(s[0], e[0])
    ymin, ymax = min(s[1], e[1]), max(s[1], e[1])
    if I(ymin - ref[1]) > dist or I(ref[1] - ymax) > dist:
        return
    if I(xmin - ref[0]) > dist or I(ref[0] - xmax) > dist:
        return
    if s[0] == e[0] and ymin < ref[1] < ymax:
        if delta[0] == INT_MIN:
            raise Undefined
        return
    if s[1] == e[1] and xmin < ref[0] < xmax:
        if delta[1] == INT_MIN:
            raise Undefined
        return
    replay_squared_distance(s, e, ref)
    I(dist + 1)


def replay_rotate_about(p, c, angle):
    log = Log()
    o = sub_i(p, c)
    rotated = rotate_i(o[0], o[1], angle, log)
    logged_then(log, lambda: add_rotated(rotated, c))


def replay_arc_mid(start, end, center, min_arc):
    sv = sub_i(start, center)
    ev = sub_i(end, center)
    sa, ea = vector_angle(*sv), vector_angle(*ev)
    angle = None
    if sa is not None and ea is not None:
        angle = normalize180(sa - ea) / 2
        if not min_arc:
            angle += 180.0
    try:
        replay_rotate_about(start, center, angle)
    except (Undefined, DeferredUndefined):
        # Everything after the differences follows the deferred trigo.h wrapper in Rust.
        raise DeferredUndefined(HEADER_ROTATE)


# ---------------------------------------------------------------------------------------------

class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.names = set()

    def request(self, name, inp):
        assert name not in self.names, name
        self.names.add(name)
        return {'unit': UNIT, 'case': name, 'input': inp}

    def add(self, name, inp, classify=None):
        """classify() raises Undefined, DeferredUndefined or Unknown (dropped)."""
        try:
            if classify:
                classify()
        except Unknown:
            return False
        except DeferredUndefined as d:
            self.controls.append({'error': UNDEFINED, 'request': self.request(f'control-{name}', inp),
                                  'rust_unported': d.owner})
            return True
        except Undefined:
            self.controls.append({'error': UNDEFINED, 'request': self.request(f'control-{name}', inp)})
            return True
        self.cases.append(self.request(name, inp))
        return True


LIMITS = [INT_MIN, INT_MIN + 1, -2**30, -65536, -1, 0, 1, 65536, 2**30, INT_MAX - 1, INT_MAX]
ANGLES = [0.0, -0.0, 90.0, 180.0, 270.0, 360.0, -90.0, -180.0, -270.0, 450.0, 720.0, -720.0,
          45.0, 135.0, 225.0, 315.0, -45.0, 30.0, 60.0, 1.0, 0.1, 1e-9, -1e-12, 89.99999999999999,
          90.00000000000001, 179.9, 359.99999999999994, 123.456, -987.654, 1e6 + 0.5, 1e10, 3e15,
          1e300, -1e300, math.inf, -math.inf, math.nan, 5e-324, 1080.0 + 45.0]
F64_VALUES = [0.0, -0.0, 1.0, -1.0, 0.5, 3.0, -7.25, 1e-310, 5e-324, 1e300, -1e300,
              1.7976931348623157e308, 123456.789, -98765.4321, 2.0**53, 2.0**31, -2.0**31 + 0.5,
              1e17, -3e-8, math.inf, -math.inf, math.nan]


def circle_point(rng, ctr, r):
    """A lattice point within one unit of the circle, from integer arithmetic only."""
    dx = rng.randint(-r, r)
    dy = math.isqrt(r * r - dx * dx) * rng.choice([1, -1])
    return (ctr[0] + dx, ctr[1] + dy) if rng.random() < 0.5 else (ctr[0] + dy, ctr[1] + dx)


def raise_undefined():
    raise Undefined


def minmax_defined(a, b, c):
    """std::minmax( { a, b, c } )'s strict weak ordering requirement on these values: only the
    transitivity of equivalence can fail, exactly when a NaN (equivalent to everything) is given
    with two non-NaN values that are not equivalent (+0 and -0 are equivalent)."""
    finite = [v for v in (a, b, c) if v == v]
    return len(finite) == 3 or all(not (v < finite[0] or finite[0] < v) for v in finite)


def value(h):
    """The double of a hex bit string (NaN payloads are only checked for NaN-ness here)."""
    return raw(h)


def arc_center_d_classify(s, m, e):
    """degenerateArcCenter calls std::minmax on the x and then the y triple first."""
    def check():
        xs = [value(p[0]) for p in (s, m, e)]
        ys = [value(p[1]) for p in (s, m, e)]
        if not minmax_defined(*xs) or not minmax_defined(*ys):
            raise Undefined
    return check


def pt(p):
    return [p[0], p[1]]


def ptd(p):
    return [bits(p[0]), bits(p[1])]


def rand_coord(rng, scale):
    return rng.randint(-scale, scale)


def rand_point(rng):
    kind = rng.random()
    if kind < 0.1:
        return (rng.choice(LIMITS), rng.choice(LIMITS))
    scale = rng.choice([10, 1000, 10**6, 10**8, 2**30, INT_MAX])
    return (max(INT_MIN, min(INT_MAX, rand_coord(rng, scale))), max(INT_MIN, min(INT_MAX, rand_coord(rng, scale))))


def rand_double(rng):
    r = rng.random()
    if r < 0.15:
        return rng.choice(F64_VALUES)
    scale = rng.choice([1.0, 1e3, 1e6, 1e9, 1e12])
    return rng.uniform(-scale, scale)


def gen_point_on_segment(c, rng):
    fixed = [((0, 0), (10, 10), (5, 5)), ((0, 0), (10, 10), (10, 10)), ((0, 0), (10, 10), (0, 0)),
             ((0, 0), (10, 10), (11, 11)), ((0, 0), (10, 10), (-1, -1)), ((0, 0), (10, 10), (5, 6)),
             ((0, 0), (0, 0), (0, 0)), ((0, 0), (0, 0), (1, 0)), ((5, 5), (5, 5), (5, 5)),
             ((-100, 0), (100, 0), (0, 0)), ((-100, 0), (100, 0), (101, 0)),
             ((0, -100), (0, 100), (0, 100)), ((INT_MIN, 0), (INT_MAX, 0), (0, 0)),
             ((0, 0), (INT_MAX, INT_MAX), (2**30, 2**30)), ((0, 0), (-2**30, -2**30), (-2**30, -2**30)),
             ((-2**30, -2**30), (2**30 - 1, 2**30 - 1), (0, 0)),
             # Controls: int differences, and the dot product or squared norm sum reaching 2^63.
             ((INT_MIN, 0), (1, 0), (0, 0)), ((0, 0), (1, 0), (INT_MIN, 0)),
             ((0, 0), (INT_MIN, INT_MIN), (INT_MIN, INT_MIN)), ((1, 1), (1, 1), (INT_MIN + 1, 1)),
             ((INT_MAX, INT_MAX), (-1, -1), (-1, -1))]
    for i, (s, e, p) in enumerate(fixed):
        c.add(f'pos-fixed-{i}', {'op': 'point_on_segment', 'start': pt(s), 'end': pt(e), 'point': pt(p)},
              lambda s=s, e=e, p=p: replay_point_on_segment(s, e, p))
    for i in range(300):
        s = rand_point(rng)
        if rng.random() < 0.6:
            # Points on the supporting line: s + k * step.
            step = (rng.randint(-50, 50), rng.randint(-50, 50))
            k = rng.randint(1, 1000)
            e = (s[0] + k * step[0], s[1] + k * step[1])
            j = rng.randint(-200, 1200)
            p = (s[0] + j * step[0], s[1] + j * step[1])
            if not all(INT_MIN <= v <= INT_MAX for v in e + p):
                continue
        else:
            e, p = rand_point(rng), rand_point(rng)
        c.add(f'pos-{i}', {'op': 'point_on_segment', 'start': pt(s), 'end': pt(e), 'point': pt(p)},
              lambda s=s, e=e, p=p: replay_point_on_segment(s, e, p))


def gen_segment_intersects(c, rng):
    fixed = [((0, 0), (10, 10), (0, 10), (10, 0), (7, 7)), ((0, 0), (10, 0), (0, 1), (10, 1), (7, 7)),
             ((0, 0), (1, 1), (10, 0), (0, 10), (0, 0)), ((0, 0), (10, 10), (0, 10), (10, 0), None),
             ((0, 0), (10, 0), (10, 0), (10, 10), (1, 2)), ((0, 0), (10, 0), (5, 0), (5, 5), (1, 2)),
             ((0, 0), (10, 0), (5, 1), (5, 5), (1, 2)), ((0, 0), (10, 0), (5, -1), (5, -5), None),
             ((0, 0), (0, 0), (1, 1), (2, 2), (3, 3)), ((0, 0), (10, 0), (20, 0), (30, 0), (3, 3)),
             ((INT_MIN, INT_MIN), (INT_MAX, INT_MAX), (INT_MIN, INT_MAX), (INT_MAX, INT_MIN), None),
             ((-2**30, 0), (2**30, 0), (0, -2**30), (0, 2**30), (5, 5)),
             ((0, 0), (3, 7), (1, 0), (0, 5), (0, 0)), ((0, 0), (3, 1), (2, -5), (2, 5), (0, 0)),
             # Controls: den products and difference, num products, the negations of INT64_MIN.
             ((0, -1), (INT_MIN, INT_MAX), (0, 0), (INT_MIN, INT_MIN), None),
             ((INT_MIN, INT_MIN), (INT_MAX, INT_MAX), (INT_MAX, INT_MIN), (INT_MIN, INT_MAX), None),
             ((INT_MIN, 0), (INT_MAX, 1), (0, INT_MIN), (1, INT_MAX), None),
             ((0, 0), (1, 1), (INT_MIN, INT_MAX), (INT_MAX, INT_MIN), None),
             # The x sum after the offset, with the KiROUND inside the int range.
             ((INT_MAX - 10, 0), (INT_MAX, 1), (INT_MAX - 10, 100), (INT_MAX - 9, 99), (0, 0))]
    for i, (a1, a2, b1, b2, p) in enumerate(fixed):
        c.add(f'ssi-fixed-{i}', {'op': 'segment_intersects', 'a1': pt(a1), 'a2': pt(a2), 'b1': pt(b1),
                                 'b2': pt(b2), 'point': None if p is None else pt(p)},
              lambda a1=a1, a2=a2, b1=b1, b2=b2, p=p: replay_segment_intersects(a1, a2, b1, b2, p))
    for i in range(500):
        mode = rng.random()
        if mode < 0.5:
            base = rand_point(rng)
            scale = rng.choice([10, 1000, 10**6])
            ps = [(max(INT_MIN, min(INT_MAX, base[0] + rand_coord(rng, scale))),
                   max(INT_MIN, min(INT_MAX, base[1] + rand_coord(rng, scale)))) for _ in range(4)]
        elif mode < 0.65:
            # Parallel or collinear pairs.
            a1 = (rand_coord(rng, 10**6), rand_coord(rng, 10**6))
            d = (rand_coord(rng, 1000), rand_coord(rng, 1000))
            k = rng.randint(-3, 3)
            off = (rng.choice([0, rng.randint(-5, 5)]), rng.choice([0, rng.randint(-5, 5)]))
            b1 = (a1[0] + off[0] + k * d[0], a1[1] + off[1] + k * d[1])
            ps = [a1, (a1[0] + d[0], a1[1] + d[1]), b1, (b1[0] + 2 * d[0], b1[1] + 2 * d[1])]
        else:
            ps = [rand_point(rng) for _ in range(4)]
        p = None if rng.random() < 0.3 else rand_point(rng)
        a1, a2, b1, b2 = ps
        c.add(f'ssi-{i}', {'op': 'segment_intersects', 'a1': pt(a1), 'a2': pt(a2), 'b1': pt(b1),
                           'b2': pt(b2), 'point': None if p is None else pt(p)},
              lambda a1=a1, a2=a2, b1=b1, b2=b2, p=p: replay_segment_intersects(a1, a2, b1, b2, p))


def gen_segment_hit(c, rng):
    fixed = [((5, 1), (0, 0), (10, 0), 1), ((5, 2), (0, 0), (10, 0), 1), ((1, 5), (0, 0), (0, 10), 1),
             ((11, 1), (0, 0), (10, 0), 1), ((12, 1), (0, 0), (10, 0), 1), ((0, 0), (0, 0), (0, 0), 0),
             ((0, 0), (0, 0), (0, 0), -1), ((3, 3), (0, 0), (10, 10), 0), ((3, 4), (0, 0), (10, 10), 0),
             ((3, 4), (0, 0), (10, 10), 1), ((10, 0), (0, 0), (10, 0), 0), ((5, 0), (10, 0), (0, 0), 0),
             ((0, 5), (0, 10), (0, 0), 0), ((-1, 5), (0, 10), (0, 0), 1), ((0, 0), (1, 1), (2, 3), INT_MAX - 1),
             ((0, 0), (INT_MIN, 0), (INT_MAX, 0), 5), ((0, 7), (INT_MIN, 0), (INT_MAX, 0), 5),
             ((0, 0), (0, INT_MIN), (0, INT_MAX), 0), ((INT_MAX, INT_MAX), (INT_MAX, INT_MAX), (INT_MAX, INT_MAX), 0),
             # Controls: the delta, the bounding box differences, std::abs( INT_MIN ), aDist + 1 and the
             # SquaredDistance dot products.
             ((INT_MAX, 0), (-1, 0), (0, 0), 0), ((0, INT_MIN), (0, 1), (0, 2), INT_MAX),
             ((INT_MIN, 0), (0, 0), (5, 5), INT_MAX), ((INT_MIN, 0), (0, -5), (0, 5), INT_MAX),
             ((0, INT_MIN), (-5, 0), (5, 0), INT_MAX), ((0, 0), (1, 1), (2, 3), INT_MAX),
             ((0, 0), (INT_MIN, INT_MIN), (INT_MAX, INT_MAX), INT_MAX),
             ((INT_MIN + 1, INT_MIN + 1), (INT_MIN, INT_MIN), (INT_MAX, INT_MAX - 1), INT_MAX)]
    for i, (r, s, e, dist) in enumerate(fixed):
        c.add(f'hit-fixed-{i}', {'op': 'segment_hit', 'ref': pt(r), 'start': pt(s), 'end': pt(e), 'dist': dist},
              lambda r=r, s=s, e=e, dist=dist: replay_segment_hit(r, s, e, dist))
    for i in range(500):
        s = rand_point(rng)
        mode = rng.random()
        if mode < 0.3:
            # Axis-aligned segments take the easy branches.
            length = rng.randint(-10**6, 10**6)
            e = (s[0], s[1] + length) if rng.random() < 0.5 else (s[0] + length, s[1])
            e = (max(INT_MIN, min(INT_MAX, e[0])), max(INT_MIN, min(INT_MAX, e[1])))
        else:
            e = (max(INT_MIN, min(INT_MAX, s[0] + rand_coord(rng, 10**6))),
                 max(INT_MIN, min(INT_MAX, s[1] + rand_coord(rng, 10**6))))
        t = rng.random()
        r = (max(INT_MIN, min(INT_MAX, round(s[0] + t * (e[0] - s[0])) + rand_coord(rng, 50))),
             max(INT_MIN, min(INT_MAX, round(s[1] + t * (e[1] - s[1])) + rand_coord(rng, 50))))
        if rng.random() < 0.1:
            r = rand_point(rng)
        dist = rng.choice([0, 1, 5, 10, 25, 50, 1000, -1, -10, 2**20, INT_MAX - 1, INT_MAX, INT_MIN])
        c.add(f'hit-{i}', {'op': 'segment_hit', 'ref': pt(r), 'start': pt(s), 'end': pt(e), 'dist': dist},
              lambda r=r, s=s, e=e, dist=dist: replay_segment_hit(r, s, e, dist))


def gen_arc_mid(c, rng):
    fixed = [((100, 0), (0, 100), (0, 0), True), ((100, 0), (0, 100), (0, 0), False),
             ((100, 0), (0, 100), (0, 0), None), ((100, 0), (-100, 0), (0, 0), True),
             ((0, 0), (0, 0), (0, 0), True), ((100, 0), (100, 0), (0, 0), False),
             ((10, 10), (-10, -10), (0, 0), True), ((1000, 2000), (3000, -500), (1500, 700), True),
             ((1000, 2000), (3000, -500), (1500, 700), False), ((INT_MAX, 0), (0, INT_MAX), (0, 0), True),
             # Controls: the differences with the centre, then (after the deferred wrapper in Rust)
             # the 90 degree negation of INT_MIN and the final sum with the centre.
             ((INT_MIN, 0), (0, 0), (1, 0), True), ((0, 0), (INT_MAX, 0), (-1, 0), True),
             ((INT_MIN, 0), (5, 0), (0, 0), True), ((0, INT_MIN), (0, 5), (0, 0), True),
             ((INT_MAX, 0), (INT_MAX, 10), (INT_MAX - 10, 0), True),
             ((0, 0), (10, 0), (INT_MAX - 5, 0), False)]
    for i, (s, e, ctr, m) in enumerate(fixed):
        inp = {'op': 'arc_mid', 'start': pt(s), 'end': pt(e), 'center': pt(ctr)}
        if m is not None:
            inp['min_arc_angle'] = m
        c.add(f'mid-fixed-{i}', inp,
              lambda s=s, e=e, ctr=ctr, m=m: replay_arc_mid(s, e, ctr, True if m is None else m))
    for i in range(300):
        ctr = (rand_coord(rng, 10**8), rand_coord(rng, 10**8))
        r = rng.choice([1, 10, 1000, 10**6, 10**8])
        s, e = circle_point(rng, ctr, r), circle_point(rng, ctr, r)
        m = rng.choice([True, False, None])
        inp = {'op': 'arc_mid', 'start': pt(s), 'end': pt(e), 'center': pt(ctr)}
        if m is not None:
            inp['min_arc_angle'] = m
        c.add(f'mid-{i}', inp, lambda s=s, e=e, ctr=ctr, m=m: replay_arc_mid(s, e, ctr, True if m is None else m))


def gen_rotations(c, rng):
    i = 0
    points = [(3, 4), (0, 0), (100, 0), (-7, 13), (INT_MAX, 0), (0, INT_MAX), (INT_MIN, 0), (0, INT_MIN),
              (INT_MIN, INT_MIN), (INT_MAX, INT_MAX), (2**30, -2**30), (1, -1), (123456789, -987654321)]
    for p in points:
        for a in ANGLES:
            c.add(f'rot-i-{i}', {'op': 'rotate_i', 'point': pt(p), 'angle': bits(a)},
                  lambda p=p, a=a: rotate_i_classify(p, a))
            i += 1
    for k in range(200):
        p = rand_point(rng)
        a = rng.choice(ANGLES) if rng.random() < 0.3 else rng.uniform(-720, 720)
        c.add(f'rot-i-r{k}', {'op': 'rotate_i', 'point': pt(p), 'angle': bits(a)},
              lambda p=p, a=a: rotate_i_classify(p, a))
    centres = [(0, 0), (5, 5), (-1000, 2000), (INT_MAX, INT_MAX), (INT_MIN, INT_MIN), (2**30, 2**30)]
    i = 0
    for p in [(10, 0), (INT_MAX, 0), (INT_MIN, INT_MAX), (0, 0), (-123, 456), (2**30, -2**30)]:
        for ctr in centres:
            for a in [0.0, 90.0, 180.0, 270.0, 45.0, 30.0, -135.0, 1e-9, math.nan]:
                c.add(f'rot-ia-{i}', {'op': 'rotate_i_about', 'point': pt(p), 'center': pt(ctr), 'angle': bits(a)},
                      lambda p=p, ctr=ctr, a=a: replay_rotate_about(p, ctr, a))
                i += 1
    for k in range(250):
        ctr = rand_point(rng) if rng.random() < 0.2 else (rand_coord(rng, 10**8), rand_coord(rng, 10**8))
        p = (max(INT_MIN, min(INT_MAX, ctr[0] + rand_coord(rng, 10**7))),
             max(INT_MIN, min(INT_MAX, ctr[1] + rand_coord(rng, 10**7))))
        a = rng.choice(ANGLES) if rng.random() < 0.3 else rng.uniform(-720, 720)
        c.add(f'rot-ia-r{k}', {'op': 'rotate_i_about', 'point': pt(p), 'center': pt(ctr), 'angle': bits(a)},
              lambda p=p, ctr=ctr, a=a: replay_rotate_about(p, ctr, a))
    i = 0
    for p in [(1.0, 0.0), (3.0, -4.0), (0.0, -0.0), (math.nan, 1.0), (math.inf, 2.0), (1e300, -1e300),
              (5e-324, 1.5), (-0.0, -0.0)]:
        for a in ANGLES:
            c.add(f'rot-d-{i}', {'op': 'rotate_d', 'point': ptd(p), 'angle': bits(a)})
            i += 1
    for k in range(150):
        p = (rand_double(rng), rand_double(rng))
        a = rng.choice(ANGLES) if rng.random() < 0.3 else rng.uniform(-720, 720)
        c.add(f'rot-d-r{k}', {'op': 'rotate_d', 'point': ptd(p), 'angle': bits(a)})
        ctr = (rand_double(rng), rand_double(rng))
        c.add(f'rot-da-r{k}', {'op': 'rotate_d_about', 'point': ptd(p), 'center': ptd(ctr), 'angle': bits(a)})
    for k, (p, ctr) in enumerate([((2.0, 1.0), (1.0, 1.0)), ((math.inf, 0.0), (math.inf, 0.0)),
                                  ((1.0, math.nan), (0.0, 0.0)), ((1e308, 1e308), (-1e308, -1e308))]):
        for a in [0.0, 90.0, 180.0, 270.0, 45.0, 33.0]:
            c.add(f'rot-da-{k}-{bits(a)}', {'op': 'rotate_d_about', 'point': ptd(p), 'center': ptd(ctr),
                                             'angle': bits(a)})


def rotate_i_classify(p, a):
    rotate_i(p[0], p[1], a, Log())


def gen_arc_centers(c, rng):
    # The chord/angle form: doubles only.
    i = 0
    for s, e in [((0.0, 0.0), (10.0, 0.0)), ((0.0, 0.0), (0.0, 0.0)), ((1e6, -3e6), (-2e6, 5e5)),
                 ((math.nan, 0.0), (1.0, 1.0)), ((math.inf, 0.0), (0.0, 0.0)), ((1e300, 1e300), (-1e300, 1.0))]:
        for a in [0.0, -0.0, 90.0, -90.0, 180.0, -180.0, 270.0, 360.0, 720.0, 1e-300, 1e-12, 45.0, 359.0,
                  -359.0, 540.0, math.nan, math.inf]:
            c.add(f'center-angle-{i}', {'op': 'arc_center_angle', 'start': ptd(s), 'end': ptd(e), 'angle': bits(a)})
            i += 1
    for k in range(200):
        s, e = (rand_double(rng), rand_double(rng)), (rand_double(rng), rand_double(rng))
        a = rng.choice([0.0, 360.0, -720.0]) if rng.random() < 0.3 else rng.uniform(-400, 400)
        c.add(f'center-angle-r{k}', {'op': 'arc_center_angle', 'start': ptd(s), 'end': ptd(e), 'angle': bits(a)})

    def three(name, op, s, m, e, classify=None):
        if op == 'arc_center_d':
            c.add(name, {'op': op, 'start': ptd(s), 'mid': ptd(m), 'end': ptd(e)}, classify)
        else:
            c.add(name, {'op': op, 'start': pt(s), 'mid': pt(m), 'end': pt(e)}, classify)

    fixed_i = [((-100, 0), (0, 100), (100, 0)), ((0, 0), (1, 1), (2, 0)), ((0, 0), (50, 0), (100, 0)),
               ((0, 0), (50, 0), (0, 0)), ((0, 0), (1, 0), (1000, 1000)), ((0, 0), (1000, 1000), (1, 1)),
               ((0, 0), (1000, 1000), (1, 0)), ((0, 0), (3, 0), (1000, 1000)), ((0, 0), (1000000, 1), (2000000, 0)),
               ((INT_MIN, INT_MIN), (0, INT_MAX), (INT_MAX, INT_MIN)), ((INT_MIN, 0), (0, 1), (INT_MAX, 0)),
               ((INT_MAX, INT_MAX), (INT_MAX - 1, INT_MAX), (INT_MAX, INT_MAX - 1)),
               ((1234, 5678), (1234, 5678), (1234, 5678)), ((0, 0), (0, 4), (0, 8)), ((0, 0), (2, 0), (10, 0)),
               ((1000, 0), (0, 1000), (-1000, 0)), ((1001, 0), (0, 1001), (-1001, 0)),
               ((99999, 50), (-1, 100050), (-100001, 50))]
    for k, (s, m, e) in enumerate(fixed_i):
        for op in ('arc_center_i', 'is_straight_arc', 'arc_center_d'):
            if op == 'arc_center_d':
                three(f'{op}-fixed-{k}', op, tuple(map(float, s)), tuple(map(float, m)), tuple(map(float, e)))
            else:
                three(f'{op}-fixed-{k}', op, s, m, e)
    for k in range(400):
        mode = rng.random()
        if mode < 0.4:
            # A circle about a 10 or 100 nm grid point, points rounded onto it: the snap paths.
            g = rng.choice([1, 10, 100])
            ctr = (rand_coord(rng, 10**7) * g, rand_coord(rng, 10**7) * g)
            r = rng.choice([5, 50, 1000, 10**5, 10**7, 10**8])
            s, m, e = [circle_point(rng, ctr, r) for _ in range(3)]
        elif mode < 0.55:
            # Collinear and near-collinear.
            s = (rand_coord(rng, 10**8), rand_coord(rng, 10**8))
            d = (rand_coord(rng, 10**5), rand_coord(rng, 10**5))
            m = (s[0] + d[0] + rng.choice([0, 0, 1, -1]), s[1] + d[1] + rng.choice([0, 0, 1]))
            e = (s[0] + 2 * d[0] + rng.choice([0, 1]), s[1] + 2 * d[1])
        elif mode < 0.65:
            # Clusters and coincident pairs.
            s = (rand_coord(rng, 10**6), rand_coord(rng, 10**6))
            m = (s[0] + rng.randint(-3, 3), s[1] + rng.randint(-3, 3))
            e = (s[0] + rng.choice([rng.randint(-4, 4), rng.randint(-10**4, 10**4)]),
                 s[1] + rng.choice([rng.randint(-4, 4), rng.randint(-10**4, 10**4)]))
        else:
            s, m, e = rand_point(rng), rand_point(rng), rand_point(rng)
        if not all(INT_MIN <= v <= INT_MAX for v in s + m + e):
            continue
        op = ('arc_center_i', 'is_straight_arc', 'arc_center_d')[k % 3]
        if op == 'arc_center_d':
            jitter = rng.choice([0.0, 0.0, 0.25, -0.5, 1e-7])
            three(f'{op}-r{k}', op, (s[0] + jitter, float(s[1])), tuple(map(float, m)), tuple(map(float, e)))
        else:
            three(f'{op}-r{k}', op, s, m, e)
    # Doubles beyond the int range, signed zeros and infinities.
    special = [((0.0, -0.0), (-0.0, 0.0), (0.0, 0.0)), ((1e300, 0.0), (0.0, 1e300), (-1e300, 0.0)),
               ((math.inf, 0.0), (0.0, 1.0), (1.0, 0.0)), ((-math.inf, math.inf), (0.0, 1.0), (5.0, 0.0)),
               ((1e17, 1.0), (-1e17, 3.0), (0.0, 1e9)), ((5e-324, 0.0), (0.0, 5e-324), (10.0, 10.0)),
               ((0.5, 0.25), (100.75, 0.125), (200.0, 0.0)), ((2.0**60, 3.0), (2.0**60 + 1e5, 1e5), (2.0**60, 2e5))]
    for k, (s, m, e) in enumerate(special):
        three(f'arc_center_d-special-{k}', 'arc_center_d', s, m, e)
    # NaN coordinates: x { NaN, 1, 2 } and y { 0, 10, -NaN } break std::minmax's strict weak
    # ordering (controls); y { 0, NaN, 0 } does not, and takes the cluster mean (a case).
    for k, (s, m, e) in enumerate([((math.nan, 0.0), (1.0, 1.0), (2.0, 0.0)),
                                   ((0.0, 0.0), (1.0, math.nan), (2.0, 0.0)),
                                   ((0.0, 0.0), (10.0, 10.0), (20.0, -math.nan))]):
        three(f'arc_center_d-nan-{k}', 'arc_center_d', s, m, e,
              arc_center_d_classify(ptd(s), ptd(m), ptd(e)))


# NaN payloads and signs (quiet and signalling), for the operand order of each compiled site.
NANS = ['7ff80000000000aa', 'fff80000000000bb', '7ff800000000cccc', '7ff00000000000dd',
        'fff40000000000ee']
SMALL = [0.0, -0.0, 1.0, -1.0, 10.0, 100.0, math.inf, -math.inf, 1e300, -1e300, 1e-300, 5e-324]


def gen_nan_orders(c):
    """Defined requests whose result is a NaN chosen by operand order: RotatePoint about a centre
    and the three-point VECTOR2D CalcArcCenter (its inlined det2 calls, the std::fma arguments and
    the final sums), with NaN inputs that keep std::minmax defined, internal invalid operations
    from infinities, and overflow from finite inputs. Their own seed leaves the earlier cases
    unchanged."""
    rng = random.Random(SEED + 103)
    # The PR #103 review reproducers.
    c.add('rot-da-review-payload', {'op': 'rotate_d_about', 'point': ['0000000000000000', NANS[0]],
                                    'center': ['0000000000000000', '7ff80000000000bb'],
                                    'angle': '0000000000000000'})
    review = [('inf', ('0000000000000000', '0000000000000000'), ('0000000000000000', '7ff0000000000000'),
               ('0000000000000000', '4024000000000000')),
              ('finite', ('01a56e1fc2f8f359', '4059000000000000'), ('0000000000000000', '7e37e43c8800759c'),
               ('01a56e1fc2f8f359', '8000000000000000'))]
    for name, s, m, e in review:
        c.add(f'arc_center_d-review-{name}', {'op': 'arc_center_d', 'start': list(s), 'mid': list(m),
                                              'end': list(e)})
    # RotatePoint( double*, double*, double, double, EDA_ANGLE ): NaNs in the point and the centre
    # together, on both axes, through the shortcuts and the general sin/cos path.
    pool = NANS[:2] + [NANS[3]] + [bits(v) for v in (math.inf, -math.inf, 0.0, 1.5, -0.0)]
    i = 0
    for a in pool:
        for b in pool:
            for angle in [0.0, 90.0, 180.0, 270.0, 45.0, 33.0, math.nan, math.inf]:
                c.add(f'rot-da-nan-{i}', {'op': 'rotate_d_about', 'point': [a, b], 'center': [b, a],
                                          'angle': bits(angle)})
                i += 1

    def axis(mode):
        if mode == 'finite':
            return [bits(rng.choice(SMALL + [rng.uniform(-1e3, 1e3)])) for _ in range(3)]
        if mode == 'allnan':
            return [rng.choice(NANS) for _ in range(3)]
        if mode == 'onenan':
            v = rng.choice(SMALL + [rng.uniform(-1e3, 1e3)])
            out = [bits(v), bits(-v if v == 0 and rng.random() < 0.5 else v), rng.choice(NANS)]
        elif mode == 'twonan':
            out = [bits(rng.choice(SMALL)), rng.choice(NANS), rng.choice(NANS)]
        else:
            # A NaN with two distinct values: undefined (a control).
            a = rng.choice(SMALL)
            b = rng.choice([v for v in SMALL if v != a])
            out = [bits(a), bits(b), rng.choice(NANS)]
        rng.shuffle(out)
        return out

    # NaNs on one axis (the other finite), or on both axes with each point's x and y sharing a
    # payload: the coincidence tests' SquaredEuclideanNorm then never adds two different NaNs.
    # 'both' draws both axes independently; where a difference has two different NaNs, the vector
    # owner's deferral (VECTOR floating arithmetic: distinct NaN operands) is reached first.
    nan_modes = ['allnan'] * 5 + ['onenan'] * 4 + ['twonan'] * 3 + ['bad']
    kinds = ['x'] * 8 + ['y'] * 8 + ['diag'] * 3 + ['both']
    for k in range(600):
        kind = rng.choice(kinds)
        if kind == 'diag':
            xs = axis('allnan')
            ys = list(xs)
        elif kind == 'both':
            xs, ys = axis(rng.choice(nan_modes)), axis(rng.choice(nan_modes))
        else:
            xs, ys = axis(rng.choice(nan_modes)), axis('finite')
            if kind == 'y':
                xs, ys = ys, xs
        s, m, e = (xs[0], ys[0]), (xs[1], ys[1]), (xs[2], ys[2])
        c.add(f'arc_center_d-nan-r{k}', {'op': 'arc_center_d', 'start': list(s), 'mid': list(m),
                                         'end': list(e)}, arc_center_d_classify(s, m, e))
    # No NaN input: invalid operations (inf - inf, 0 * inf) inside det2 and the fma calls.
    for k in range(600):
        s, m, e = [(bits(rng.choice(SMALL)), bits(rng.choice(SMALL))) for _ in range(3)]
        c.add(f'arc_center_d-sv-r{k}', {'op': 'arc_center_d', 'start': list(s), 'mid': list(m),
                                        'end': list(e)})
    # Finite inputs whose squares and products overflow or underflow.
    for k in range(200):
        def big():
            mag = pow10(rng.uniform(150, 308)) if rng.random() < 0.8 else pow10(rng.uniform(-320, -150))
            return bits(math.copysign(mag, rng.random() - 0.5))
        s, m, e = [(big(), big()) for _ in range(3)]
        c.add(f'arc_center_d-big-r{k}', {'op': 'arc_center_d', 'start': list(s), 'mid': list(m),
                                         'end': list(e)})


def generate():
    rng = random.Random(SEED)
    c = Corpus()
    gen_point_on_segment(c, rng)
    gen_segment_intersects(c, rng)
    gen_segment_hit(c, rng)
    gen_arc_mid(c, rng)
    gen_rotations(c, rng)
    gen_arc_centers(c, rng)
    gen_nan_orders(c)
    return c


def lines_text(items):
    return ''.join(json.dumps(x, separators=(',', ':'), sort_keys=True) + '\n' for x in items)


def pow10(exponent):
    """10 ** exponent rounded to a double without the host libm: `float ** float` calls the
    platform's pow, whose last bit differs between glibc and the Windows runtime, which made the
    committed corpus depend on the machine that generated it."""
    with decimal.localcontext() as ctx:
        ctx.prec = 60
        ctx.Emin = -decimal.MAX_EMAX
        ctx.Emax = decimal.MAX_EMAX
        return float(decimal.Decimal(10) ** decimal.Decimal(exponent))


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
