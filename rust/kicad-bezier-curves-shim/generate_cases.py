#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and controls for the bezier_curves.cpp unit.

Writes port/cases/<unit>.jsonl (requests whose pinned evaluation is defined and inside the
adapter's budgets) and port/oracle/fixtures/bezier-curves-controls.jsonl (requests both programs
must refuse, with the refusal: an undefined pinned evaluation, a GetPoly whose loops need more
than STEP_BUDGET iterations or an ellipse needing more than MAX_BEZIERS curves, or a malformed
input). No result is written anywhere.

The split uses exact arithmetic only:

- IEEE binary64 sums, differences, products and quotients, which Python evaluates exactly as the
  oracle does without contraction (a quotient by zero is spelled out, since Python raises);
- a correctly rounded square root computed with integers (std::sqrt is sqrtsd);
- for getQuadPoly's point count, which also needs glibc's pow and hypot, rigorous rational
  intervals: those results are within 2^-50 relative of the exact values (both are below one
  ulp), and every IEEE operation after them within half an ulp. A request is public only when
  the whole interval of n is in [0, 4096]; a control only when the whole interval is beyond the
  budget or beyond int; anything else is dropped.

The cubic path (cubicParabolicApprox and recursiveSegmentation) has no libm call besides sqrt,
so it is replayed exactly, counting the adapter's loop iterations. A curve on the
recursiveSegmentation stack that holds a NaN never terminates (no comparison with it succeeds and
every half keeps a NaN); a subdivision that reproduces its curve bit for bit repeats forever. Both
are beyond the budget without running it out. No host libm function and no float power is
evaluated, so the output is identical on every host.

--check verifies the committed files equal the generator output.
"""
import argparse
from fractions import Fraction
import json
import math
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.bezier_curves.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/bezier-curves-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
STEP_BUDGET = 1 << 20
MAX_BEZIERS = 4096
MAX_POINTS = 4096
PUBLIC_POINTS = 4096
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined bezier operation'}
OUTSIDE = {'kind': 'invalid_input', 'message': 'outside the verified domain'}
SEED = 20261128
INF = float('inf')
NAN = float('nan')
DEG2RAD = math.pi / 180.0
HALF_PI = 2 * math.pi / 4


class Undefined(Exception):
    """The pinned evaluation reaches undefined behaviour."""


class Outside(Exception):
    """The evaluation needs more than the adapter's budget."""


class Unknown(Exception):
    """The split cannot be decided rigorously."""


def bits(v):
    return struct.pack('>d', v).hex()


def raw(h):
    return struct.unpack('>d', bytes.fromhex(h))[0]


def nan(payload, negative=False, quiet=True):
    word = (0xfff0000000000000 if negative else 0x7ff0000000000000) | payload
    if quiet:
        word |= 0x0008000000000000
    return struct.unpack('>d', word.to_bytes(8, 'big'))[0]


def same(a, b):
    return bits(a) == bits(b)


def is_nan(v):
    return v != v


def fdiv(a, b):
    """IEEE a / b (Python raises for a zero divisor)."""
    if b == 0.0:
        if is_nan(a) or a == 0.0:
            return NAN
        return math.copysign(INF, a) * math.copysign(1.0, b)
    return a / b


def sqrt_rn(x):
    """Correctly rounded sqrt (sqrtsd), with integers only."""
    if is_nan(x) or x < 0.0:
        return NAN
    if x == 0.0 or x == INF:
        return x
    f = Fraction(x)
    n, d = f.numerator, f.denominator
    # sqrt( n / d ) = sqrt( n * d * 4^s ) / ( d * 2^s ); q has at least 120 bits.
    s = max(0, (240 - (n * d).bit_length()) // 2 + 1)
    m = n * d * 4**s
    q = math.isqrt(m)
    den = d * 2**s
    if q * q == m:
        return float(Fraction(q, den))
    # Strictly between q and q + 1: the midpoint rounds the same way (no tie is possible).
    return float(Fraction(2 * q + 1, 2 * den))


# --- VECTOR2<double> operations (values only; a NaN's bits never change a comparison) ---

def vsub(a, b):
    return (a[0] - b[0], a[1] - b[1])


def cross(a, b):
    return a[0] * b[1] - a[1] * b[0]


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1]


def sqnorm(a):
    return a[0] * a[0] + a[1] * a[1]


def equal(a, b):
    return b[0] == a[0] and b[1] == a[1]


def has_nan(points):
    return any(is_nan(x) or is_nan(y) for x, y in points)


def identical(p, q):
    return all(same(a[0], b[0]) and same(a[1], b[1]) for a, b in zip(p, q))


class Steps:
    def __init__(self):
        self.used = 0

    def add(self, count):
        if count > STEP_BUDGET - self.used:
            raise Outside()
        self.used += count


def is_flat4(p, e):
    delta = vsub(p[3], p[0])
    d21 = vsub(p[1], p[0])
    d31 = vsub(p[2], p[0])
    c1 = cross(delta, d21)
    c2 = cross(delta, d31)
    inv = fdiv(1.0, sqnorm(delta))
    d1 = (c1 * c1) * inv
    d2 = (c2 * c2) * inv
    factor = 3.0 / 4.0 if c1 * c2 > 0.0 else 4.0 / 9.0
    f2 = factor * factor
    tol = e * e
    return d1 * f2 <= tol and d2 * f2 <= tol


def lerp(a, b, t):
    d = vsub(b, a)
    return (d[0] * t + a[0], d[1] * t + a[1])


def subdivide4(p, t):
    lc1 = lerp(p[0], p[1], t)
    tmp = lerp(p[1], p[2], t)
    lc2 = lerp(lc1, tmp, t)
    rc2 = lerp(p[2], p[3], t)
    rc1 = lerp(tmp, rc2, t)
    shared = lerp(lc2, rc1, t)
    return [p[0], lc1, lc2, shared], [shared, rc1, rc2, p[3]]


def number_of_inflection_points(p):
    d21, d32, d43 = vsub(p[1], p[0]), vsub(p[2], p[1]), vsub(p[3], p[2])
    cross1 = cross(d21, d32) * cross(d32, d43)
    cross2 = cross(d21, d32) * cross(d21, d43)
    if cross1 < 0.0:
        return 1
    if cross2 > 0.0:
        return 0
    if (dot(d21, d32) > 0.0) ^ (dot(d32, d43) > 0.0):
        return 0
    return -1


def find_inflection_points(p):
    def coefficients(p0, p1, p2, p3):
        return ((-p0 + 3 * p1 - 3 * p2) + p3, (3 * p0 - 6 * p1) + 3 * p2, -3 * p0 + 3 * p1)
    ax, bx, cx = coefficients(p[0][0], p[1][0], p[2][0], p[3][0])
    ay, by, cy = coefficients(p[0][1], p[1][1], p[2][1], p[3][1])
    a = 3 * cross((ax, ay), (bx, by))
    b = 3 * cross((ax, ay), (cx, cy))
    c = cross((bx, by), (cx, cy))
    r2 = b * b - 4 * a * c
    if r2 >= 0.0 and a != 0.0:
        r = sqrt_rn(r2)
        t1 = fdiv(-b + r, 2 * a)
        t2 = fdiv(-b - r, 2 * a)
        if (0.0 < t1 < 1.0) and (0.0 < t2 < 1.0):
            if t1 > t2:
                t1, t2 = t2, t1
            return (2 if t2 - t1 > 0.00001 else 1), t1, t2
        if 0.0 < t1 < 1.0:
            return 1, t1, 0.0
        if 0.0 < t2 < 1.0:
            return 1, t2, 0.0
        return 0, 0.0, 0.0
    return 0, 0.0, 0.0


def third_control_point_deviation(p):
    delta = vsub(p[1], p[0])
    len_sq = sqnorm(delta)
    if len_sq < 1e-6:
        return 0.0
    length = sqrt_rn(len_sq)
    r = fdiv(p[1][1] - p[0][1], length)
    s = fdiv(p[0][0] - p[1][0], length)
    u = fdiv(p[1][0] * p[0][1] - p[0][0] * p[1][1], length)
    return abs(r * p[2][0] + s * p[2][1] + u)


def recursive_segmentation(p, e, steps):
    stack = [p]
    pushed = 0
    while stack:
        steps.add(1)
        bez = stack[-1]
        if equal(bez[3], bez[0]):
            stack.pop()
        elif is_flat4(bez, e):
            pushed += 1
            stack.pop()
        else:
            if has_nan(bez):
                raise Outside()  # never flat, never zero length, and so are both halves
            left, right = subdivide4(bez, 0.5)
            if identical(left, bez):
                raise Outside()  # the same state forever
            stack[-1] = right
            stack.append(left)
    return pushed


def cubic_parabolic_approx(p, e, steps):
    c = p
    pushed = 0
    while True:
        steps.add(1)
        if has_nan(c):
            break
        if is_flat4(c, e):
            return pushed + 1
        dev = third_control_point_deviation(c)
        if dev == 0.0:
            return pushed + recursive_segmentation(c, e, steps)
        t = 2 * sqrt_rn(fdiv(e, 3.0 * dev))
        if t > 1.0:
            return pushed + recursive_segmentation(c, e, steps)
        b1, b2 = subdivide4(c, t)
        if is_flat4(b1, e):
            pushed += 1
        else:
            pushed += recursive_segmentation(b1, e, steps)
        if identical(b2, c):
            raise Outside()  # the same state forever
        c = b2
    return pushed


def cubic_points(p, e):
    """Points getCubicPoly appends, or Outside."""
    steps = Steps()
    pushed = 1
    if number_of_inflection_points(p) == 0:
        return pushed + cubic_parabolic_approx(p, e, steps)
    count, t1, _ = find_inflection_points(p)
    if count == 2:
        sub1, tmp1 = subdivide4(p, t1)
        pushed += cubic_parabolic_approx(sub1, e, steps)
        count, t1, _ = find_inflection_points(tmp1)
        if count not in (1, 2):
            return pushed + 1
        sub2, sub3 = subdivide4(tmp1, t1)
        pushed += recursive_segmentation(sub2, e, steps)
        return pushed + cubic_parabolic_approx(sub3, e, steps)
    if count == 1:
        sub1, sub2 = subdivide4(p, t1)
        pushed += cubic_parabolic_approx(sub1, e, steps)
        return pushed + cubic_parabolic_approx(sub2, e, steps)
    return pushed + cubic_parabolic_approx(p, e, steps)


# --- getQuadPoly's n, with rigorous intervals for the libm results ---

REL = Fraction(1, 2**50)


def root4_bounds(z):
    """[lo, hi] with lo <= z^(1/4) <= hi, z a nonnegative rational."""
    if z == 0:
        return Fraction(0), Fraction(0)
    k = 64
    m = z * 2**(4 * k)
    lo_int = math.isqrt(math.isqrt(math.floor(m)))
    hi_int = lo_int + 1
    while Fraction(hi_int)**4 < m:
        hi_int += 1
    return Fraction(lo_int, 2**k), Fraction(hi_int, 2**k)


def sqrt_bounds(z):
    if z == 0:
        return Fraction(0), Fraction(0)
    k = 64
    m = z * 2**(2 * k)
    lo_int = math.isqrt(math.floor(m))
    return Fraction(lo_int, 2**k), Fraction(lo_int + 1, 2**k)


def widen(lo, hi, rel=REL):
    """The interval after one rounding (or a libm result) within rel of its exact value."""
    lo_w = lo - abs(lo) * rel
    hi_w = hi + abs(hi) * rel
    return lo_w, hi_w


def approx_int_bounds(x):
    """|approx_int( x )| bounds for a finite x (an odd function, so |x| suffices)."""
    d = 0.6744897501960817
    d4 = d * d * d * d
    ax = abs(x)
    xx = ax * ax
    if xx == INF:
        raise Unknown()
    z = d4 + xx * 0.25  # IEEE, exactly as compiled
    r_lo, r_hi = widen(*root4_bounds(Fraction(z)))
    one_minus_d = Fraction(1.0 - d)
    den_lo, den_hi = widen(one_minus_d + r_lo, one_minus_d + r_hi)
    return widen(Fraction(ax) / den_hi, Fraction(ax) / den_lo)


def quad_count(p, e):
    """[lo, hi] bounds of getQuadPoly's n before the conversion, or Undefined if provably not an
    int, or Unknown."""
    ddx = 2 * p[1][0] - p[0][0] - p[2][0]
    ddy = 2 * p[1][1] - p[0][1] - p[2][1]
    u0 = (p[1][0] - p[0][0]) * ddx + (p[1][1] - p[0][1]) * ddy
    u2 = (p[2][0] - p[1][0]) * ddx + (p[2][1] - p[1][1]) * ddy
    cr = (p[2][0] - p[0][0]) * ddy - (p[2][1] - p[0][1]) * ddx
    x0 = fdiv(u0, cr)
    x2 = fdiv(u2, cr)
    # approx_int( +-inf ) is inf / inf and approx_int( NaN ) a NaN, so n is a NaN.
    if not (math.isfinite(x0) and math.isfinite(x2)):
        raise Undefined()
    if is_nan(e):
        raise Undefined()
    if not all(math.isfinite(v) for v in (ddx, ddy, u0, u2, cr)) or cr == 0.0:
        raise Unknown()
    dx = abs(x2 - x0)
    if dx == 0.0 or (ddx == 0.0 and ddy == 0.0):
        raise Unknown()
    # Both a's have their x's signs (approx_int is odd), so |a2 - a0| is a sum or a difference.
    a0_lo, a0_hi = approx_int_bounds(x0)
    a2_lo, a2_hi = approx_int_bounds(x2)
    if (x0 >= 0) == (x2 >= 0):
        lo, hi = a2_lo - a0_hi, a2_hi - a0_lo
        if hi < 0:
            lo, hi = -hi, -lo
        elif lo < 0:
            lo = Fraction(0)
    else:
        lo, hi = a0_lo + a2_lo, a0_hi + a2_hi
    a_lo, a_hi = widen(lo, hi)
    h_lo, h_hi = widen(*sqrt_bounds(Fraction(ddx)**2 + Fraction(ddy)**2))
    den_lo, den_hi = widen(h_lo * Fraction(dx), h_hi * Fraction(dx))
    if den_lo <= Fraction(1, 2**1000):
        raise Unknown()
    if e == INF:
        return 0, 0  # scale / inf == 0: n = ceil( 0.5 * |a2 - a0| * 0 ) with a finite difference
    s_lo, s_hi = widen(Fraction(abs(cr)) / den_hi, Fraction(abs(cr)) / den_lo)
    q_lo, q_hi = widen(s_lo / Fraction(e), s_hi / Fraction(e))
    r_lo, r_hi = widen(*sqrt_bounds(q_lo)), widen(*sqrt_bounds(q_hi))
    v_lo, v_hi = widen(Fraction(1, 2) * a_lo * r_lo[0], Fraction(1, 2) * a_hi * r_hi[1])
    return math.ceil(v_lo), math.ceil(v_hi)


def get_poly_points(p, e):
    """Points GetPoly( std::vector<VECTOR2D>&, double ) appends; raises Undefined, Outside or
    Unknown."""
    if e <= 0.0:
        e = 10.0
    if len(p) == 3:
        lo, hi = quad_count(p, e)
        if lo > INT_MAX:
            raise Undefined()
        if hi > INT_MAX:
            raise Unknown()
        if lo > STEP_BUDGET:
            raise Outside()
        if hi > PUBLIC_POINTS:
            raise Unknown()
        return 2 + hi
    if len(p) == 4:
        count = cubic_points(p, e)
        if count > PUBLIC_POINTS:
            raise Unknown()  # defined and within the budget, but too long for the public corpus
        return count
    return 0


# --- TransformEllipseToBeziers' curve count ---

def ellipse_count(start, end):
    arc = -(end - start)
    if arc >= 0.0:
        arc = arc - 360.0
    q = abs(fdiv(arc * DEG2RAD, HALF_PI))
    if is_nan(q) or q == INF:
        raise Undefined()
    n = math.ceil(q)
    if n > INT_MAX:
        raise Undefined()
    if n > MAX_BEZIERS:
        raise Outside()
    return n


# --- Requests ---

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
        """classify() raises Undefined, Outside (controls) or Unknown (dropped)."""
        try:
            if classify:
                classify()
        except Unknown:
            return False
        except Undefined:
            self.controls.append({'error': UNDEFINED, 'request': self.request(f'control-{name}', inp)})
            return False
        except Outside:
            self.controls.append({'error': OUTSIDE, 'request': self.request(f'control-{name}', inp)})
            return False
        self.cases.append(self.request(name, inp))
        return True

    def refuse(self, name, inp, error):
        self.controls.append({'error': error, 'request': self.request(f'control-{name}', inp)})


def pi(p):
    return [int(p[0]), int(p[1])]


def pd(p):
    return [bits(p[0]), bits(p[1])]


def as_double(points):
    return [(float(x), float(y)) for x, y in points]


def rnd_int(rng, magnitude):
    return rng.randint(-magnitude, magnitude)


def rnd_double(rng, magnitude):
    """A finite double with a random significand: an integer over a power of two."""
    return rng.randint(-2**52, 2**52) / 2**52 * magnitude


SHAPES = {
    'arch': [(0, 0), (1000, 2000), (3000, 2000), (4000, 0)],
    's-curve': [(0, 0), (4000, 3000), (-1000, 3000), (3000, 0)],
    'one-inflection': [(0, 0), (1000, 3000), (2000, -3000), (3000, 0)],
    'loop': [(0, 0), (4000, 4000), (-1000, 4000), (3000, 0)],
    'cusp': [(0, 0), (3000, 3000), (0, 3000), (3000, 0)],
    'collinear': [(0, 0), (1000, 1000), (2000, 2000), (3000, 3000)],
    'collinear-back': [(0, 0), (3000, 3000), (1000, 1000), (2000, 2000)],
    'closed': [(0, 0), (2000, 3000), (-2000, 3000), (0, 0)],
    'point': [(5, 5), (5, 5), (5, 5), (5, 5)],
    'ctrl-on-start': [(0, 0), (0, 0), (3000, 2000), (4000, 0)],
    'ctrl-on-end': [(0, 0), (1000, 2000), (4000, 0), (4000, 0)],
    'flat': [(0, 0), (1000, 1), (2000, -1), (3000, 0)],
    'tight': [(0, 0), (100000, 0), (100000, 100000), (0, 100000)],
    'wide': [(-500000, -500000), (900000, -100000), (-900000, 100000), (500000, 500000)],
    'tiny': [(0, 0), (1, 2), (3, 2), (4, 0)],
    'vertical': [(0, 0), (0, 1000), (0, 2000), (10, 3000)],
}

QUADS = {
    'arch': [(0, 0), (1000, 2000), (3000, 0)],
    'symmetric': [(-2000, 0), (0, 2000), (2000, 0)],
    'skewed': [(0, 0), (5000, 100), (5100, 4000)],
    'sharp': [(0, 0), (100000, 100000), (0, 1)],
    'shallow': [(0, 0), (2000, 1), (4000, 0)],
    'tiny': [(0, 0), (1, 1), (2, 0)],
    'large': [(-600000, -600000), (900000, 0), (-600000, 600000)],
}


def gen_construct(c, rng):
    for name, pts in SHAPES.items():
        c.add(f'construct-points4-{name}', {'op': 'construct', 'form': 'points4', 'points': [pi(p) for p in pts]})
    c.add('construct-points4-limits', {'op': 'construct', 'form': 'points4',
                                       'points': [[INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [0, -1], [-1, 0]]})
    for n in (0, 1, 2, 3, 4, 5, 8):
        pts = [[rnd_int(rng, 2**31 - 1), rnd_int(rng, 2**31 - 1)] for _ in range(n)]
        c.add(f'construct-vector-i-{n}', {'op': 'construct', 'form': 'vector_i', 'points': pts})
    specials = [0.0, -0.0, 1.0, -1.5, 1e-310, -5e-324, 1e308, INF, -INF, nan(0x1234), nan(0x55, True),
                nan(0x7, quiet=False)]
    c.add('construct-vector-d-special', {'op': 'construct', 'form': 'vector_d',
                                         'points': [[bits(x), bits(specials[-1 - i])] for i, x in enumerate(specials)]})


def gen_get_poly(c, rng):
    errors_i = [None, 1, 5, 10, 25, 100, 0, -3]
    for name, pts in list(SHAPES.items()) + list(QUADS.items()):
        kind = 'quad' if len(pts) == 3 else 'cubic'
        for e in errors_i:
            label = 'default' if e is None else str(e).replace('-', 'm')
            for form in (['points4', 'vector_i'] if len(pts) == 4 else ['vector_i']):
                inp = {'op': 'get_poly_i', 'form': form, 'points': [pi(p) for p in pts], 'output': [[7, 7]]}
                if e is not None:
                    inp['max_error'] = e
                err = 10.0 if e is None else float(e)
                c.add(f'get-poly-i-{kind}-{name}-{form}-{label}', inp,
                      lambda p=as_double(pts), err=err: get_poly_points(p, err))
        for e in [None, 0.5, 2.0, 10.0, 50.0, -1.0, 0.0, -0.0, INF]:
            label = 'default' if e is None else bits(e)
            inp = {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in as_double(pts)],
                   'output': [pd((1.5, -2.5))]}
            if e is not None:
                inp['max_error'] = bits(e)
            err = 10.0 if e is None else e
            c.add(f'get-poly-d-{kind}-{name}-{label}', inp, lambda p=as_double(pts), err=err: get_poly_points(p, err))

    # Random int curves of both orders and magnitudes, every form.
    for index in range(160):
        order = 4 if index % 3 else 3
        magnitude = rng.choice([100, 10000, 1000000, 2**20])
        pts = [(rnd_int(rng, magnitude), rnd_int(rng, magnitude)) for _ in range(order)]
        e = rng.choice([1, 3, 10, 50, magnitude // 200 + 1])
        form = rng.choice(['points4', 'vector_i']) if order == 4 else 'vector_i'
        inp = {'op': 'get_poly_i', 'form': form, 'points': [pi(p) for p in pts], 'output': [], 'max_error': e}
        c.add(f'get-poly-i-random-{index:03d}', inp, lambda p=as_double(pts), e=float(e): get_poly_points(p, e))

    # Random double curves: fractional coordinates, errors and orders.
    for index in range(160):
        order = 4 if index % 3 else 3
        magnitude = rng.choice([1.0, 100.0, 12345.678, 1e6])
        pts = [(rnd_double(rng, magnitude), rnd_double(rng, magnitude)) for _ in range(order)]
        e = magnitude * rng.choice([0.001, 0.01, 0.05])
        inp = {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in pts], 'output': [],
               'max_error': bits(e)}
        c.add(f'get-poly-d-random-{index:03d}', inp, lambda p=pts, e=e: get_poly_points(p, e))

    # Non-finite control points.
    for index, special in enumerate([nan(0x99), nan(0x42, True), INF, -INF]):
        for slot in range(4):
            pts = [(0.0, 0.0), (1000.0, 2000.0), (3000.0, 2000.0), (4000.0, 0.0)]
            pts[slot] = (special, pts[slot][1]) if slot % 2 else (pts[slot][0], special)
            c.add(f'get-poly-d-cubic-special-{index}-{slot}',
                  {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in pts], 'output': [],
                   'max_error': bits(10.0)}, lambda p=pts: get_poly_points(p, 10.0))
            q = pts[:3] if slot < 3 else [(0.0, 0.0), (1000.0, 2000.0), (special, 0.0)]
            c.add(f'get-poly-d-quad-special-{index}-{slot}',
                  {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in q], 'output': [],
                   'max_error': bits(10.0)}, lambda p=q: get_poly_points(p, 10.0))
    for name, e in (('nan', nan(0x5)), ('tiny', 1e-300)):
        for shape in ('arch', 'symmetric'):
            pts = as_double(SHAPES.get(shape) or QUADS[shape])
            c.add(f'get-poly-d-error-{name}-{shape}',
                  {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in pts], 'output': [],
                   'max_error': bits(e)}, lambda p=pts, e=e: get_poly_points(p, e))
    # Long outputs: a fine cubic and quad within the budget.
    for name, pts, e in (('cubic', SHAPES['wide'], 2.0), ('quad', QUADS['large'], 200.0)):
        pts = as_double(pts)
        c.add(f'get-poly-d-{name}-fine', {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in pts],
                                          'output': [], 'max_error': bits(e)},
              lambda p=pts, e=e: get_poly_points(p, e))
    # A quad needing between 2^20 and 2^31 points, and one needing more than an int holds.
    pts = as_double(QUADS['large'])
    for name, e in (('budget', 1e-8), ('beyond-int', 1e-18)):
        c.add(f'get-poly-d-quad-{name}', {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in pts],
                                          'output': [], 'max_error': bits(e)},
              lambda p=pts, e=e: get_poly_points(p, e))

    # Other control point counts assert in GetPoly (the oracle records the call; Rust defers).
    for n in (0, 1, 2, 5, 6):
        pts = [[i * 100, (i % 2) * 50] for i in range(n)]
        c.add(f'get-poly-i-assert-{n}', {'op': 'get_poly_i', 'form': 'vector_i', 'points': pts,
                                          'output': [[1, 1]], 'max_error': 10})
        c.add(f'get-poly-d-assert-{n}', {'op': 'get_poly_d', 'form': 'vector_d',
                                          'points': [pd(as_double([p])[0]) for p in pts],
                                          'output': [pd((9.0, 9.0))]})
    # A long prior output is kept by the VECTOR2D overload and cleared by the VECTOR2I one.
    prior = [[i, -i] for i in range(PUBLIC_POINTS - 64)]
    pts = SHAPES['arch']
    c.add('get-poly-i-long-prior', {'op': 'get_poly_i', 'form': 'points4', 'points': [pi(p) for p in pts],
                                    'output': prior}, lambda: get_poly_points(as_double(pts), 10.0))
    c.add('get-poly-d-long-prior', {'op': 'get_poly_d', 'form': 'vector_d', 'points': [pd(p) for p in as_double(pts)],
                                    'output': [pd((float(x), float(y))) for x, y in prior]},
          lambda: get_poly_points(as_double(pts), 10.0))


def special_doubles(rng):
    return [0.0, -0.0, 1.0, -2.5, 3.0, 1e-300, 1e300, INF, -INF, nan(0xa), nan(0xb, True), nan(0xc, quiet=False),
            rnd_double(rng, 1000.0)]


def gen_private(c, rng):
    specials = special_doubles(rng)
    nan_values = [nan(0xa), nan(0xb, True), nan(0xc, quiet=False), nan(0xd)]
    base3 = [(0.0, 0.0), (1000.0, 2000.0), (3000.0, 0.0)]
    base4 = [(0.0, 0.0), (1000.0, 2000.0), (3000.0, 2000.0), (4000.0, 0.0)]

    def variants(base, name):
        """The base curve, then each component replaced by a NaN, and NaN pairs."""
        yield name, base
        for slot in range(len(base)):
            for axis in range(2):
                p = [list(q) for q in base]
                p[slot][axis] = nan_values[(slot + axis) % 4]
                yield f'{name}-nan-{slot}{axis}', [tuple(q) for q in p]
        for a in range(len(base)):
            for b in range(len(base)):
                if a != b:
                    p = [list(q) for q in base]
                    p[a][0] = nan_values[0]
                    p[b][0] = nan_values[1]
                    p[a][1] = nan_values[2]
                    p[b][1] = nan_values[3]
                    yield f'{name}-nan-pair-{a}{b}', [tuple(q) for q in p]

    # is_nan
    for name, p in [('empty', []), ('one', [(1.0, 2.0)]), ('inf', [(INF, -INF)]) ] + list(variants(base4, 'cubic'))[:9]:
        c.add(f'is-nan-{name}', {'op': 'is_nan', 'points': [pd(q) for q in p]})

    # eval: every size, special parameters, NaN operand combinations.
    for n in (0, 1, 2, 3, 4, 5):
        pts = [(rnd_double(rng, 100.0), rnd_double(rng, 100.0)) for _ in range(n)]
        for t in (0.0, 0.5, 1.0, -0.25, 2.0):
            c.add(f'eval-size-{n}-{bits(t)}', {'op': 'eval', 'points': [pd(q) for q in pts], 't': bits(t)})
    for base, kind in ((base3, 'quad'), (base4, 'cubic')):
        for name, p in variants(base, kind):
            for t in (0.25, nan(0x77), INF):
                c.add(f'eval-{name}-{bits(t)}', {'op': 'eval', 'points': [pd(q) for q in p], 't': bits(t)})
        for index in range(40):
            p = [(rnd_double(rng, 1e4), rnd_double(rng, 1e4)) for _ in base]
            t = rng.choice(specials + [rng.random(), rng.random()])
            c.add(f'eval-{kind}-random-{index:02d}', {'op': 'eval', 'points': [pd(q) for q in p], 't': bits(t)})

    # subdivide: both orders with NaN operands, special parameters, wider and smaller outputs.
    for base, kind in ((base3, 'quad'), (base4, 'cubic')):
        size = len(base)
        for name, p in variants(base, kind):
            for t in (0.5, 0.3, nan(0x66)):
                c.add(f'subdivide-{name}-{bits(t)}', {
                    'op': 'subdivide', 'points': [pd(q) for q in p], 't': bits(t),
                    'left': [pd((9.0, 9.0))] * size, 'right': [pd((8.0, 8.0))] * size})
        for index in range(30):
            p = [(rnd_double(rng, 1e5), rnd_double(rng, 1e5)) for _ in base]
            t = rng.choice(specials + [rng.random()])
            extra = rng.choice([0, 1, 3])
            c.add(f'subdivide-{kind}-random-{index:02d}', {
                'op': 'subdivide', 'points': [pd(q) for q in p], 't': bits(t),
                'left': [pd((float(i), 0.0)) for i in range(size + extra)],
                'right': [pd((0.0, float(i))) for i in range(size + rng.choice([0, 2]))]})
        for short in range(size):
            for side in ('left', 'right'):
                inp = {'op': 'subdivide', 'points': [pd(q) for q in base], 't': bits(0.5),
                       'left': [pd((1.0, 1.0))] * size, 'right': [pd((2.0, 2.0))] * size}
                inp[side] = [pd((3.0, 3.0))] * short
                c.refuse(f'subdivide-{kind}-short-{side}-{short}', inp, UNDEFINED)
    for n in (0, 1, 2, 5):
        c.add(f'subdivide-assert-{n}', {'op': 'subdivide', 'points': [pd((float(i), 1.0)) for i in range(n)],
                                         't': bits(0.5), 'left': [pd((4.0, 4.0))], 'right': []})

    # is_flat: every size; the three-point projection's clamps; the four-point factor.
    for n in (0, 1, 2, 5):
        c.add(f'is-flat-assert-{n}', {'op': 'is_flat', 'points': [pd((float(i), 0.0)) for i in range(n)],
                                       'max_error': bits(1.0)})

    def flat3(p):
        d21, d31 = vsub(p[1], p[0]), vsub(p[2], p[0])
        if is_nan(fdiv(dot(d21, d31), sqnorm(d31))):
            raise Undefined()

    three = [
        ('mid', [(0.0, 0.0), (5.0, 3.0), (10.0, 0.0)]),
        ('before', [(0.0, 0.0), (-5.0, 3.0), (10.0, 0.0)]),
        ('beyond', [(0.0, 0.0), (20.0, 0.0), (10.0, 0.0)]),
        ('diagonal', [(0.0, 0.0), (3.0, 3.0), (0.0, 6.0)]),
        ('axis-x', [(0.0, 0.0), (4.0, 0.0), (0.0, 6.0)]),
        ('axis-y', [(0.0, 0.0), (0.0, 4.0), (6.0, 0.0)]),
        ('general', [(1.5, -2.0), (7.25, 3.5), (12.0, 1.0)]),
        ('degenerate', [(1.0, 1.0), (4.0, 5.0), (1.0, 1.0)]),
        ('all-same', [(1.0, 1.0), (1.0, 1.0), (1.0, 1.0)]),
        ('nan', [(0.0, 0.0), (nan(1), 3.0), (10.0, 0.0)]),
        ('inf', [(0.0, 0.0), (5.0, 3.0), (INF, 0.0)]),
    ]
    for name, p in three:
        for e in (0.0, 1.0, 1.5, 2.5, 5.0, 100.0, nan(0x3), INF, -1.0):
            c.add(f'is-flat-quad-{name}-{bits(e)}', {'op': 'is_flat', 'points': [pd(q) for q in p], 'max_error': bits(e)},
                  lambda p=p: flat3(p))
    for name, pts in SHAPES.items():
        for e in (0.0, 1.0, 10.0, 100.0, 1000.0, nan(0x3), INF):
            c.add(f'is-flat-cubic-{name}-{bits(e)}', {'op': 'is_flat', 'points': [pd(q) for q in as_double(pts)],
                                                       'max_error': bits(e)})
    for name, p in list(variants(base4, 'cubic'))[:9]:
        c.add(f'is-flat-{name}', {'op': 'is_flat', 'points': [pd(q) for q in p], 'max_error': bits(INF)})

    # numberOfInflectionPoints / findInflectionPoints: the shapes, random curves, NaN, sizes.
    for name, pts in SHAPES.items():
        p = as_double(pts)
        c.add(f'number-of-inflection-points-{name}', {'op': 'number_of_inflection_points', 'points': [pd(q) for q in p]})
        c.add(f'find-inflection-points-{name}', {'op': 'find_inflection_points', 'points': [pd(q) for q in p],
                                                  't1': bits(nan(0x11)), 't2': bits(-7.0)})
    for index in range(80):
        p = [(rnd_double(rng, 1e4), rnd_double(rng, 1e4)) for _ in range(4 + (index % 5 == 0))]
        c.add(f'number-of-inflection-points-random-{index:02d}', {'op': 'number_of_inflection_points',
                                                                   'points': [pd(q) for q in p]})
        c.add(f'find-inflection-points-random-{index:02d}', {'op': 'find_inflection_points',
                                                              'points': [pd(q) for q in p],
                                                              't1': bits(0.5), 't2': bits(0.5)})
    for name, p in list(variants(base4, 'cubic'))[:5]:
        c.add(f'number-of-inflection-points-{name}', {'op': 'number_of_inflection_points', 'points': [pd(q) for q in p]})
        c.add(f'find-inflection-points-{name}', {'op': 'find_inflection_points', 'points': [pd(q) for q in p],
                                                  't1': bits(1.0), 't2': bits(2.0)})
    for n in (0, 1, 2, 3):
        p = [pd((float(i), float(i * i))) for i in range(n)]
        c.refuse(f'number-of-inflection-points-size-{n}', {'op': 'number_of_inflection_points', 'points': p}, UNDEFINED)
        c.refuse(f'find-inflection-points-size-{n}', {'op': 'find_inflection_points', 'points': p,
                                                       't1': bits(0.0), 't2': bits(0.0)}, UNDEFINED)

    # thirdControlPointDeviation: the 1e-6 threshold, NaN operand combinations, sizes.
    for name, p in variants(base3, 'quad'):
        c.add(f'third-control-point-deviation-{name}', {'op': 'third_control_point_deviation',
                                                         'points': [pd(q) for q in p]})
    threshold = [
        ('below', [(0.0, 0.0), (0.0, 0.000999), (5.0, 5.0)]),
        ('at', [(0.0, 0.0), (0.001, 0.0), (5.0, 5.0)]),
        ('above', [(0.0, 0.0), (0.0011, 0.0), (5.0, 5.0)]),
        ('zero', [(3.0, 4.0), (3.0, 4.0), (5.0, 5.0)]),
        ('on-line', [(0.0, 0.0), (10.0, 10.0), (20.0, 20.0)]),
        ('four', [(0.0, 0.0), (10.0, 0.0), (5.0, 7.0), (1.0, 1.0)]),
        ('inf', [(0.0, 0.0), (10.0, 0.0), (INF, 7.0)]),
    ]
    for name, p in threshold:
        c.add(f'third-control-point-deviation-{name}', {'op': 'third_control_point_deviation',
                                                         'points': [pd(q) for q in p]})
    for index in range(60):
        p = [(rnd_double(rng, 1e5), rnd_double(rng, 1e5)) for _ in range(3)]
        c.add(f'third-control-point-deviation-random-{index:02d}', {'op': 'third_control_point_deviation',
                                                                     'points': [pd(q) for q in p]})
    for n in (0, 1, 2):
        c.refuse(f'third-control-point-deviation-size-{n}', {'op': 'third_control_point_deviation',
                                                              'points': [pd((1.0, 2.0))] * n}, UNDEFINED)


def gen_ellipse(c, rng):
    def ellipse(kind, center, major, minor, rotation, start, end, output=()):
        scalar = (lambda v: v) if kind == 'int' else bits
        point = pi if kind == 'int' else pd
        return {'op': 'ellipse_to_beziers', 'type': kind, 'center': point(center), 'major': scalar(major),
                'minor': scalar(minor), 'rotation': bits(rotation), 'start': bits(start), 'end': bits(end),
                'output': [[point(q) for q in curve] for curve in output]}

    sweeps = [(0.0, 360.0), (0.0, 90.0), (0.0, 45.0), (30.0, 300.0), (300.0, 30.0), (90.0, -90.0),
              (0.0, 0.0), (45.0, 45.0), (0.0, 1e-9), (0.0, 359.9), (0.0, -0.0), (-0.0, 0.0), (1e300, 1e300 + 1e290),
              (720.0, 0.0), (0.0, 720.0), (12.5, 12.75), (-170.0, 170.0), (0.0, 5e-324), (5e-324, 0.0)]
    for kind in ('double', 'int'):
        for index, (start, end) in enumerate(sweeps):
            for rotation in (0.0, 30.0, -90.0, 180.0, 1e10):
                if kind == 'int':
                    inp = ellipse(kind, (1000, -2000), 5000, 2000, rotation, start, end)
                else:
                    inp = ellipse(kind, (1000.5, -2000.25), 5000.0, 2000.0, rotation, start, end)
                c.add(f'ellipse-{kind}-sweep-{index:02d}-{bits(rotation)}', inp, lambda s=start, e=end: ellipse_count(s, e))
        for index in range(60):
            start = rng.choice([0.0, 90.0, rnd_double(rng, 400.0)])
            end = rnd_double(rng, 800.0)
            rotation = rnd_double(rng, 360.0)
            if kind == 'int':
                center = (rnd_int(rng, 2**30), rnd_int(rng, 2**30))
                major, minor = rng.randint(0, 2**28), rng.randint(-2**10, 2**28)
            else:
                center = (rnd_double(rng, 1e6), rnd_double(rng, 1e6))
                major, minor = rnd_double(rng, 1e6), rnd_double(rng, 1e6)
            prior = [[center] * 4] if index % 7 == 0 else []
            c.add(f'ellipse-{kind}-random-{index:02d}', ellipse(kind, center, major, minor, rotation, start, end, prior),
                  lambda s=start, e=end: ellipse_count(s, e))
    # int extremes: sums beyond int saturate in the casting constructor.
    for index, (center, major, minor) in enumerate([((INT_MAX, INT_MIN), INT_MAX, INT_MAX),
                                                    ((INT_MIN, INT_MAX), INT_MAX, INT_MIN),
                                                    ((0, 0), INT_MIN, INT_MIN), ((0, 0), 0, 0)]):
        for rotation in (0.0, 45.0, nan(0x9)):
            c.add(f'ellipse-int-extreme-{index}-{bits(rotation)}',
                  ellipse('int', center, major, minor, rotation, 0.0, 360.0), lambda: ellipse_count(0.0, 360.0))
    # double specials: NaN payloads in the centre and radii, infinities, NaN rotation.
    nans = [nan(0xa), nan(0xb, True), nan(0xc, quiet=False)]
    centers = [(0.0, 0.0), (nans[0], 1.0), (2.0, nans[1]), (nans[0], nans[1]), (INF, -INF)]
    radii = [(1.0, 1.0), (nans[2], 1.0), (1.0, nans[1]), (INF, 2.0), (0.0, -0.0), (-3.0, 1e308)]
    for ci, center in enumerate(centers):
        for ri, (major, minor) in enumerate(radii):
            for rotation in (0.0, 33.0, nans[0], INF):
                c.add(f'ellipse-double-special-{ci}-{ri}-{bits(rotation)}',
                      ellipse('double', center, major, minor, rotation, 10.0, 200.0),
                      lambda: ellipse_count(10.0, 200.0))
    # An undefined or excessive curve count.
    for name, (start, end) in {'nan-end': (0.0, nan(0x1)), 'nan-start': (nan(0x2), 0.0), 'inf-end': (0.0, INF),
                               'inf-both': (INF, INF), 'huge': (0.0, 1e300), 'beyond-int': (0.0, -360.0 * 2**30),
                               'budget': (0.0, -360.0 * 1026.0), 'budget-edge': (0.0, 360.0 * 1025.0 + 0.5),
                               'within': (0.0, -360.0 * 1023.0)}.items():
        for kind in ('double', 'int'):
            inp = ellipse(kind, (0, 0) if kind == 'int' else (0.0, 0.0), 1 if kind == 'int' else 1.0,
                          1 if kind == 'int' else 1.0, 0.0, start, end)
            c.add(f'ellipse-{kind}-count-{name}', inp, lambda s=start, e=end: ellipse_count(s, e))


def gen_malformed(c):
    zero = bits(0.0)

    def bad(name, inp, message):
        c.refuse(f'malformed-{name}', inp, {'kind': 'invalid_input', 'message': message})

    arch = [pi(p) for p in SHAPES['arch']]
    bad('op', {'op': 'nope'}, 'unknown operation')
    bad('op-type', {'op': 1}, 'expected string: op')
    bad('extra', {'op': 'construct', 'form': 'points4', 'points': arch, 'x': 1}, 'unexpected or missing field')
    bad('missing', {'op': 'get_poly_i', 'form': 'points4', 'points': arch}, 'unexpected or missing field')
    bad('optional-extra', {'op': 'get_poly_d', 'form': 'vector_d', 'points': [], 'output': [], 'max_error': zero,
                           'y': 2}, 'unexpected or missing field')
    bad('form', {'op': 'construct', 'form': 'vector', 'points': arch}, 'unknown form')
    bad('form-type', {'op': 'construct', 'form': 4, 'points': arch}, 'expected string: form')
    bad('points4-three', {'op': 'construct', 'form': 'points4', 'points': arch[:3]}, 'expected four points')
    bad('points-type', {'op': 'construct', 'form': 'vector_i', 'points': {}}, 'expected array: points')
    bad('points-many', {'op': 'construct', 'form': 'vector_i', 'points': [[0, 0]] * (MAX_POINTS + 1)},
        'too many entries: points')
    bad('output-many', {'op': 'get_poly_i', 'form': 'points4', 'points': arch, 'output': [[0, 0]] * (MAX_POINTS + 1)},
        'too many entries: output')
    bad('point', {'op': 'construct', 'form': 'vector_i', 'points': [[0]]}, 'expected point')
    bad('int-range', {'op': 'construct', 'form': 'vector_i', 'points': [[INT_MAX + 1, 0]]}, 'integer out of range')
    bad('int-float', {'op': 'get_poly_i', 'form': 'points4', 'points': arch, 'output': [], 'max_error': 1.5},
        'integer out of range')
    bad('hex', {'op': 'eval', 'points': [], 't': '3FF0000000000000'}, 'invalid hex')
    bad('hex-short', {'op': 'eval', 'points': [], 't': '00'}, 'invalid hex')
    bad('hex-type', {'op': 'is_flat', 'points': [], 'max_error': 0}, 'expected hex')
    bad('type', {'op': 'ellipse_to_beziers', 'type': 'float'}, 'unknown type')
    bad('type-missing', {'op': 'ellipse_to_beziers'}, 'expected string: type')
    bad('ellipse-missing', {'op': 'ellipse_to_beziers', 'type': 'int', 'center': [0, 0]}, 'unexpected or missing field')
    bad('curve', {'op': 'ellipse_to_beziers', 'type': 'int', 'center': [0, 0], 'major': 1, 'minor': 1,
                  'rotation': zero, 'start': zero, 'end': zero, 'output': [[[0, 0]]]}, 'expected curve')


def generate():
    c = Corpus()
    rng = random.Random(SEED)
    gen_construct(c, rng)
    gen_get_poly(c, rng)
    gen_private(c, rng)
    gen_ellipse(c, rng)
    gen_malformed(c)
    return c


def lines_text(rows):
    return ''.join(json.dumps(r, sort_keys=True, separators=(',', ':')) + '\n' for r in rows)


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
    print(f'{len(c.cases)} public cases, {len(c.controls)} controls')


if __name__ == '__main__':
    main()
