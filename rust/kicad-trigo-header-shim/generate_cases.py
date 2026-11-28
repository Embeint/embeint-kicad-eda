#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and controls for the trigo.h inline body unit.

Writes port/cases/<unit>.jsonl (requests whose pinned evaluation is defined and inside the
adapter's verified domain) and port/oracle/fixtures/trigo-header-controls.jsonl (requests both
programs must refuse, with the refusal: an undefined pinned evaluation, a template angle whose
loops need more passes than the adapter's budget, or a malformed input). No result is written
anywhere.

The split replays only exact arithmetic: Python integers for int sums and negations, IEEE binary64
products and sums (which Python evaluates exactly as the oracle does without contraction) for the
EDA_ANGLE diagonal shortcuts, and fractions for EDA_ANGLE's std::fmod fold. No host libm function
and no float power is evaluated, so the output is identical on every host. A rotation away from
the cardinal and diagonal shortcuts uses sin/cos, which this generator does not evaluate: a
request about a centre is admitted only where every rotated value within the rotation's magnitude
bound keeps the following int sums in range, and is otherwise dropped.

The wrappers forward to the trigo.cpp overloads, whose only undefined operations are int
overflow and the negation of INT_MIN in the 90/180/270 degree shortcuts. A control marked
"rust_unported" is undefined only after a logging KiROUND, which the Rust port defers to the math
util unit: the oracle refuses it and Rust defers to that exact owner first.

The angle templates' passes are counted exactly, with fractions for a double angle (whose
intermediate values are exact up to 2^57; the one pass that can round is replayed with Python's
correctly rounded fraction-to-float conversion). A double loop that makes no progress
(fl( a +- 3600 ) == a: an infinite angle or a magnitude above 2^65) never terminates, which is
undefined; any other angle needing more than 2^24 passes is outside the adapter's budget.

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
UNIT = 'libs.kimath.include.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/trigo-header-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1
# The adapter's pass budget for the template loops, and the angle 3600 * 2^24 that takes it.
PASS_BUDGET = 1 << 24
LOOP_BOUND = 3600 * PASS_BUDGET
M_SQRT1_2 = float.fromhex('0x1.6a09e667f3bcdp-1')
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined trigo.h operation'}
OUTSIDE = {'kind': 'invalid_input', 'message': 'outside the verified domain'}
LOG_OVERFLOW = {'upstream': 'kimathLogOverflow',
                'unit': 'libs.kimath.src.math.util.application.ui.p001'}
SEED = 20261128
# The double template cases, added after the first corpus, use their own seed.
DOUBLE_SEED = 20261129


class Undefined(Exception):
    """The pinned evaluation reaches undefined behaviour."""


class Unknown(Exception):
    """The split cannot be decided without evaluating sin or cos."""


class DeferredUndefined(Exception):
    """Undefined only after a call the Rust port defers."""

    def __init__(self, owner):
        super().__init__(owner)
        self.owner = owner


def bits(v):
    return struct.pack('>d', v).hex()


def raw(h):
    return struct.unpack('>d', bytes.fromhex(h))[0]


def negative(v):
    """The sign bit of a double (std::signbit)."""
    return struct.pack('>d', v)[0] >= 0x80


def finite(v):
    return v == v and v not in (float('inf'), float('-inf'))


def I(v):
    if not INT_MIN <= v <= INT_MAX:
        raise Undefined
    return v


# ---------------------------------------------------------------------------------------------
# EDA_ANGLE replay (degrees).

def fmod(x, y):
    """std::fmod( x, y ) for finite x and y > 0: exact, with the sign of x."""
    q = Fraction(x) / Fraction(y)
    n = int(q)  # truncation toward zero
    r = Fraction(x) - n * Fraction(y)
    if r == 0:
        return -0.0 if negative(x) else 0.0
    return float(r)  # the remainder is exactly representable


def fold(v, period):
    """EDA_ANGLE::foldToTwoPeriods."""
    if not finite(v):
        return 0.0
    if abs(v) < 2.0 * period:
        return v
    return fmod(v, period) + (-period if negative(v) else period)


def normalize(v):
    """EDA_ANGLE::Normalize."""
    v = fold(v, 360.0)
    while v < -0.0:
        v += 360.0
    while v >= 360.0:
        v -= 360.0
    return v


class Log:
    """Records the first logging KiROUND."""

    def __init__(self):
        self.logged = False


def kiround(v, log):
    """KiROUND<double, int>( v ): saturating and logging; llround halves away from zero."""
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
    r = int(a)  # a >= 0 here, so truncation is floor
    if a - r >= 0.5:
        r += 1
    return r if v >= 0 else -r


DIAGONALS = {45.0: (M_SQRT1_2, M_SQRT1_2), 135.0: (M_SQRT1_2, -M_SQRT1_2),
             225.0: (-M_SQRT1_2, -M_SQRT1_2), 315.0: (-M_SQRT1_2, M_SQRT1_2)}


def rotate_i(x, y, angle, log):
    """RotatePoint( int*, int*, const EDA_ANGLE& ): the rotated point, or ('bound', b) where sin
    and cos decide it (|result| <= b; KiROUND saturates, so the rotation itself is defined)."""
    a = normalize(angle)
    if a == 0.0:
        return (x, y)
    if a == 90.0:
        return (y, I(-x))
    if a == 180.0:
        return (I(-x), I(-y))
    if a == 270.0:
        return (I(-y), x)
    if a in DIAGONALS:
        s, c = DIAGONALS[a]
        return (kiround(float(y) * s + float(x) * c, log),
                kiround(float(y) * c - float(x) * s, log))
    # |y sin + x cos| <= hypot( x, y ), plus one for rounding.
    return ('bound', isqrt(x * x + y * y) + 2)


def isqrt(n):
    """floor( sqrt( n ) ) for a non-negative integer, by Newton's method on integers."""
    if n < 2:
        return n
    x = 1 << ((n.bit_length() + 1) // 2)
    while True:
        y = (x + n // x) // 2
        if y >= x:
            return x
        x = y


def replay_rotate(p, angle):
    rotate_i(p[0], p[1], angle, Log())


def replay_rotate_about(p, c, angle):
    """RotatePoint( int*, int*, int cx, int cy, const EDA_ANGLE& ): differences, rotation and sums."""
    log = Log()
    o = (I(p[0] - c[0]), I(p[1] - c[1]))
    rotated = rotate_i(o[0], o[1], angle, log)
    if rotated[0] == 'bound':
        bound = rotated[1]
        for k in (0, 1):
            if not (INT_MIN <= c[k] - bound and c[k] + bound <= INT_MAX):
                raise Unknown
        return
    try:
        I(rotated[0] + c[0])
        I(rotated[1] + c[1])
    except Undefined:
        if log.logged:
            raise DeferredUndefined(LOG_OVERFLOW)
        raise


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

    def refuse(self, name, inp, error):
        self.controls.append({'error': error, 'request': self.request(f'control-{name}', inp)})


LIMITS = [INT_MIN, INT_MIN + 1, -2**30, -65536, -1, 0, 1, 65536, 2**30, INT_MAX - 1, INT_MAX]
NAN = float('nan')
INF = float('inf')
ANGLES = [0.0, -0.0, 90.0, 180.0, 270.0, 360.0, -90.0, -180.0, -270.0, -360.0, 450.0, 720.0,
          -720.0, 45.0, 135.0, 225.0, 315.0, -45.0, -135.0, 405.0, 30.0, 60.0, 1.0, 0.1, 1e-9,
          -1e-12, 89.99999999999999, 90.00000000000001, 179.9, 359.99999999999994, 123.456,
          -987.654, 1e6 + 0.5, 1e10, 3e15, 1e300, -1e300, INF, -INF, NAN, 5e-324, 1080.0 + 45.0,
          1080.0 + 270.0, -1e16]
NANS = ['7ff8000000000000', 'fff8000000000000', '7ff80000000000aa', 'fff800000000cccc',
        '7ff00000000000dd', 'fff40000000000ee', '7ff0000000000001']
F64_VALUES = [0.0, -0.0, 1.0, -1.0, 0.5, 3.0, -7.25, 1e-310, -1e-310, 5e-324, 1e300, -1e300,
              1.7976931348623157e308, -1.7976931348623157e308, 123456.789, -98765.4321, 2.0**53,
              2.0**31, -2.0**31 + 0.5, 1e17, -3e-8, INF, -INF, NAN]


def pt(p):
    return [p[0], p[1]]


def ptd(p):
    return [x if isinstance(x, str) else bits(x) for x in p]


def rand_point(rng):
    kind = rng.random()
    if kind < 0.15:
        return (rng.choice(LIMITS), rng.choice(LIMITS))
    scale = rng.choice([10, 1000, 10**6, 10**8, 2**30, INT_MAX])
    return (rng.randint(-scale, scale), rng.randint(-scale, scale))


def rand_double(rng):
    r = rng.random()
    if r < 0.15:
        return rng.choice(F64_VALUES)
    if r < 0.25:
        return rng.choice(NANS)
    if r < 0.35:
        return raw(f'{rng.getrandbits(64):016x}')
    scale = rng.choice([1.0, 1e3, 1e6, 1e9, 1e12])
    return rng.uniform(-scale, scale)


def rand_angle(rng):
    return rng.choice(ANGLES) if rng.random() < 0.4 else rng.uniform(-720, 720)


def gen_rotate_i(c, rng):
    """RotatePoint( VECTOR2I&, EDA_ANGLE ) and GetRotated( VECTOR2I, EDA_ANGLE )."""
    points = [(10, 3), (0, 0), (1, -1), (-7, 12), (1000, 0), (0, -1000), (123456, -654321),
              (INT_MAX, INT_MAX), (INT_MIN, 0), (0, INT_MIN), (INT_MIN, INT_MIN), (INT_MAX, INT_MIN),
              (INT_MIN + 1, INT_MAX), (2**30, -2**30), (1518500250, 1518500250)]
    i = 0
    for p in points:
        for a in ANGLES:
            op = ('rotate_i', 'get_rotated_i')[i % 2]
            c.add(f'{op}-{i}', {'op': op, 'point': pt(p), 'angle': bits(a)},
                  lambda p=p, a=a: replay_rotate(p, a))
            i += 1
    for k in range(200):
        p, a = rand_point(rng), rand_angle(rng)
        op = ('rotate_i', 'get_rotated_i')[k % 2]
        c.add(f'{op}-r{k}', {'op': op, 'point': pt(p), 'angle': bits(a)},
              lambda p=p, a=a: replay_rotate(p, a))


def gen_rotate_i_about(c, rng):
    """RotatePoint( VECTOR2I&, const VECTOR2I&, EDA_ANGLE ) and GetRotated about a centre."""
    pairs = [((11, 5), (1, 2)), ((0, 0), (0, 0)), ((100, 0), (0, 0)), ((-5, 7), (3, -9)),
             ((INT_MAX, 0), (-1, 0)), ((0, INT_MAX), (0, -1)), ((INT_MIN, 0), (1, 0)),
             ((INT_MAX, INT_MAX), (INT_MAX, INT_MAX)), ((INT_MIN, INT_MIN), (0, 0)),
             ((INT_MAX - 1, INT_MAX - 1), (1, 1)), ((2**30, 2**30), (-2**30, -2**30)),
             ((10**9, -10**9), (10**9, 10**9)), ((INT_MAX, INT_MIN), (0, 0)),
             ((1518500249, 1518500249), (1, 1)), ((-1518500249, 0), (-2**30, 2**30))]
    i = 0
    for p, ctr in pairs:
        for a in ANGLES:
            op = ('rotate_i_about', 'get_rotated_i_about')[i % 2]
            c.add(f'{op}-{i}', {'op': op, 'point': pt(p), 'center': pt(ctr), 'angle': bits(a)},
                  lambda p=p, ctr=ctr, a=a: replay_rotate_about(p, ctr, a))
            i += 1
    for k in range(300):
        ctr = rand_point(rng)
        if rng.random() < 0.5:
            p = (max(INT_MIN, min(INT_MAX, ctr[0] + rng.randint(-10**7, 10**7))),
                 max(INT_MIN, min(INT_MAX, ctr[1] + rng.randint(-10**7, 10**7))))
        else:
            p = rand_point(rng)
        a = rand_angle(rng)
        op = ('rotate_i_about', 'get_rotated_i_about')[k % 2]
        c.add(f'{op}-r{k}', {'op': op, 'point': pt(p), 'center': pt(ctr), 'angle': bits(a)},
              lambda p=p, ctr=ctr, a=a: replay_rotate_about(p, ctr, a))


def gen_rotate_d(c, rng):
    """The VECTOR2D RotatePoint wrappers and GetRotated: double arithmetic only, always defined."""
    points = [(1.0, 0.0), (3.0, -4.0), (0.0, -0.0), (-0.0, -0.0), (NAN, 1.0), (1.0, NANS[3]),
              (NANS[4], NANS[2]), (INF, 2.0), (1e300, -1e300), (5e-324, 1.5), (-1e-310, 7.0)]
    i = 0
    for p in points:
        for a in ANGLES:
            op = ('rotate_d', 'get_rotated_d')[i % 2]
            c.add(f'{op}-{i}', {'op': op, 'point': ptd(p), 'angle': bits(a)})
            i += 1
    for k in range(200):
        p = (rand_double(rng), rand_double(rng))
        op = ('rotate_d', 'get_rotated_d')[k % 2]
        c.add(f'{op}-r{k}', {'op': op, 'point': ptd(p), 'angle': bits(rand_angle(rng))})
    pairs = [((2.0, 1.0), (1.0, 1.0)), ((INF, 0.0), (INF, 0.0)), ((1.0, NAN), (0.0, 0.0)),
             ((0.0, NANS[2]), (0.0, NANS[3])), ((NANS[4], 0.0), (NANS[5], 1.0)),
             ((1e308, 1e308), (-1e308, -1e308)), ((-0.0, 0.0), (0.0, -0.0))]
    i = 0
    for p, ctr in pairs:
        for a in [0.0, 90.0, 180.0, 270.0, 45.0, 135.0, 33.0, NAN, INF, -90.0]:
            c.add(f'rotate_d_about-{i}', {'op': 'rotate_d_about', 'point': ptd(p),
                                          'center': ptd(ctr), 'angle': bits(a)})
            i += 1
    for k in range(150):
        p, ctr = (rand_double(rng), rand_double(rng)), (rand_double(rng), rand_double(rng))
        c.add(f'rotate_d_about-r{k}', {'op': 'rotate_d_about', 'point': ptd(p), 'center': ptd(ctr),
                                       'angle': bits(rand_angle(rng))})


def gen_conversions(c, rng):
    """DEG2RAD, RAD2DEG and RAD2DECIDEG: one product and one quotient, defined for every value."""
    values = [bits(v) for v in F64_VALUES] + NANS + [
        bits(v) for v in (180.0, 90.0, 45.0, 360.0, -180.0, 3.141592653589793, 1.5707963267948966,
                          6.283185307179586, 1e-320, 5.7e306, 1.0e307, 3.2e305, 1.0e308)]
    for op in ('deg2rad', 'rad2deg', 'rad2decideg'):
        for k, v in enumerate(values):
            c.add(f'{op}-{k}', {'op': op, 'value': v})
        for k in range(80):
            v = rand_double(rng)
            c.add(f'{op}-r{k}', {'op': op, 'value': v if isinstance(v, str) else bits(v)})


TEMPLATE_OPS = ('normalize_angle_pos', 'normalize_angle_pos_ref', 'normalize_angle_180')


def template_passes(op, x):
    """The literal passes of the template loops (both together) for an int, long long or finite
    double angle of magnitude at most 2^57, where every pass but the one that leaves the first
    loop is exact."""
    x = Fraction(x)
    if op == 'normalize_angle_180':
        if x <= -1800:
            return math.floor((-1800 - x) / 3600) + 1
        if x > 1800:
            return math.ceil((x - 1800) / 3600)
        return 0
    if x < 0:
        k = math.ceil(-x / 3600)
        # The last pass, from the exact x + 3600 ( k - 1 ), rounds; at 3600 the second loop runs
        # once more. That needs a last value within 2^-42 of 0, so only for k == 1.
        if float(x + 3600 * k) >= 3600.0:
            assert k == 1, x
            return k + 1
        return k
    if x >= 3600:
        return math.floor(x / 3600)
    return 0


def double_refusal(op, x):
    """The refusal of a double template request, or None where both programs evaluate it."""
    if x != x:
        return None
    half = op == 'normalize_angle_180'
    first = x <= -1800.0 if half else x < 0.0
    second = x > 1800.0 if half else x >= 3600.0
    # A pass that makes no progress repeats forever: undefined.
    if (first and x + 3600.0 == x) or (second and x - 3600.0 == x):
        return UNDEFINED
    # Up to 2^57 each pass moves the angle by exactly 3600.
    if abs(x) > 2.0**40 or template_passes(op, x) > PASS_BUDGET:
        return OUTSIDE
    return None


def next_up(v):
    """The next binary64 value above a finite v (bit arithmetic, no libm)."""
    if v == 0.0:
        return 5e-324
    n = struct.unpack('>q', struct.pack('>d', v))[0]
    return struct.unpack('>d', struct.pack('>q', n + 1 if v > 0 else n - 1))[0]


def next_down(v):
    return -next_up(-v)


def gen_templates(c, rng):
    """NormalizeAnglePos, NORMALIZE_ANGLE_POS and NormalizeAngle180 for int and long long.
    Returns the original 'beyond' requests that fit the pass budget (compared at the end)."""
    within = []
    edges = [-3601, -3600, -3599, -1801, -1800, -1799, -1, 0, 1, 1799, 1800, 1801, 3599, 3600,
             3601, 7199, 7200, 36000, -36000]
    ints = edges + [INT_MIN, INT_MIN + 1, INT_MIN + 1799, INT_MAX, INT_MAX - 1, INT_MAX - 1800,
                    -2**30, 2**30, 123456789, -987654321]
    longs = edges + [LOOP_BOUND, -LOOP_BOUND, LOOP_BOUND - 1, -LOOP_BOUND + 1, LOOP_BOUND - 1800,
                     -LOOP_BOUND + 1800, INT_MIN, INT_MAX, INT_MIN - 1, INT_MAX + 1, 2**35 + 17,
                     -(2**35) - 17]
    for op in TEMPLATE_OPS:
        for k, a in enumerate(ints):
            c.add(f'{op}-int-{k}', {'op': op, 'type': 'int', 'angle': a})
        for k, a in enumerate(longs):
            c.add(f'{op}-ll-{k}', {'op': op, 'type': 'long long', 'angle': a})
        for k in range(40):
            a = rng.randint(INT_MIN, INT_MAX) if k % 2 else rng.randint(-10**6, 10**6)
            c.add(f'{op}-int-r{k}', {'op': op, 'type': 'int', 'angle': a})
            b = rng.randint(-LOOP_BOUND, LOOP_BOUND) if k % 2 else rng.randint(-10**7, 10**7)
            c.add(f'{op}-ll-r{k}', {'op': op, 'type': 'long long', 'angle': b})
        # Past 3600 * 2^24: a harness refusal on both programs where the loops need more passes
        # than the budget. The first two take exactly the budget for some operations: those are
        # compared, at the end of the corpus.
        for k, a in enumerate([LOOP_BOUND + 1, -LOOP_BOUND - 1, I64_MAX, I64_MIN, 2**62]):
            if template_passes(op, a) <= PASS_BUDGET:
                within.append((op, k, a))
                continue
            c.refuse(f'{op}-ll-beyond-{k}', {'op': op, 'type': 'long long', 'angle': a}, OUTSIDE)
    return within


def gen_template_budget(c, within):
    """The long long requests at the exact pass budget: compared where they fit, refused one past."""
    for op, k, a in within:
        c.add(f'{op}-ll-within-{k}', {'op': op, 'type': 'long long', 'angle': a})
    n = PASS_BUDGET
    for op in TEMPLATE_OPS:
        if op == 'normalize_angle_180':
            edges = [3600 * n + 1800, 3600 * n + 1801, -3600 * n - 1799, -3600 * n - 1800]
        else:
            edges = [3600 * (n + 1) - 1, 3600 * (n + 1), -3600 * n, -3600 * n - 1]
        for k, a in enumerate(edges):
            inp = {'op': op, 'type': 'long long', 'angle': a}
            if template_passes(op, a) <= PASS_BUDGET:
                c.add(f'{op}-ll-budget-{k}', inp)
            else:
                c.refuse(f'{op}-ll-budget-{k}', inp, OUTSIDE)


def gen_templates_double(c, rng):
    """The double instantiation: signed zeros, NaN payloads, subnormals, the rounding last pass,
    every loop boundary, the pass budget, magnitudes that terminate beyond the budget and those
    that never terminate."""
    n = PASS_BUDGET
    budget = [float(3600 * (n + 1)), float(-3600 * n), float(3600 * n + 1800),
              float(-3600 * n - 1800)]
    values = [0.0, -0.0, 5e-324, -5e-324, 1e-310, -1e-310, -1e-300, -2.0**-42, -2.0**-43,
              -1.5 * 2.0**-42, -2.0**-41, -2.0**-30, -0.1, -0.25, 0.25, 0.5, -0.5, 1.0, -1.0]
    for e in (-7200.0, -5400.0, -3600.0, -1800.0, 0.0, 1800.0, 3600.0, 5400.0, 7200.0, 36000.5):
        values += [e, next_up(e), next_down(e), e + 0.25, e - 0.25]
    for b in budget:
        values += [b, next_up(b), next_down(b), b + 0.5, b - 0.5]
    values += [1e15, -1e15, 2.0**53, 2.0**53 + 2, -2.0**53 - 2, 2.0**57, -2.0**57, 2.0**57 + 32,
               2.0**65, -2.0**65, next_up(2.0**65), next_down(-2.0**65), 2.0**66, -2.0**66, 1e300,
               -1e300, 1.7976931348623157e308, -1.7976931348623157e308, INF, -INF]
    values = [bits(v) for v in values] + NANS
    for _ in range(30):
        values.append(bits(rng.uniform(-1e4, 1e4)))
    for _ in range(30):
        values.append(bits(rng.uniform(-float(LOOP_BOUND), float(LOOP_BOUND))))
    for _ in range(20):
        values.append(f'{rng.getrandbits(64):016x}')
    for op in TEMPLATE_OPS:
        for k, v in enumerate(values):
            inp = {'op': op, 'type': 'double', 'angle': v}
            refusal = double_refusal(op, raw(v))
            if refusal is None:
                c.add(f'{op}-double-{k}', inp)
            else:
                c.refuse(f'{op}-double-{k}', inp, refusal)
    c.refuse('malformed-double-angle', {'op': 'normalize_angle_pos', 'type': 'double', 'angle': 0},
             {'kind': 'invalid_input', 'message': 'expected hex'})


def gen_intercepts(c, rng):
    """InterceptsPositiveX and InterceptsNegativeX (test_kimath.cpp pairs first)."""
    qa = [(10.0, 20.0), (10.0, 120.0), (10.0, 220.0), (10.0, 320.0), (20.0, 10.0), (345.0, 15.0),
          (145.0, 225.0)]
    values = [0.0, -0.0, 10.0, 15.0, 90.0, 179.99999999999997, 180.0, 180.00000000000003, 270.0,
              359.99999999999994, 360.0, 360.00000000000006, 540.0, 720.0, -10.0, -180.0, -360.0,
              1e300, -1e300, INF, -INF, NAN]
    for op in ('intercepts_positive_x', 'intercepts_negative_x'):
        for k, (s, e) in enumerate(qa):
            c.add(f'{op}-qa-{k}', {'op': op, 'start': bits(s), 'end': bits(e)})
        i = 0
        for s in values:
            for e in values:
                c.add(f'{op}-{i}', {'op': op, 'start': bits(s), 'end': bits(e)})
                i += 1
        for k in range(60):
            s, e = rng.uniform(-400, 800), rng.uniform(-400, 800)
            c.add(f'{op}-r{k}', {'op': op, 'start': bits(s), 'end': bits(e)})


def gen_malformed(c):
    """Malformed inputs both programs refuse with the same message."""
    def bad(name, inp, message):
        c.refuse(f'malformed-{name}', inp, {'kind': 'invalid_input', 'message': message})

    zero = bits(0.0)
    bad('op', {'op': 'nope'}, 'unknown operation')
    bad('extra', {'op': 'rotate_i', 'point': [0, 0], 'angle': zero, 'x': 1},
        'unexpected or missing field')
    bad('missing', {'op': 'rotate_i_about', 'point': [0, 0], 'angle': zero},
        'unexpected or missing field')
    bad('int-range', {'op': 'rotate_i', 'point': [INT_MAX + 1, 0], 'angle': zero},
        'integer out of range')
    bad('int-float', {'op': 'normalize_angle_180', 'type': 'int', 'angle': 1.5},
        'integer out of range')
    bad('ll-range', {'op': 'normalize_angle_pos', 'type': 'long long', 'angle': 2**63},
        'integer out of range')
    bad('type', {'op': 'normalize_angle_pos', 'type': 'short', 'angle': 0}, 'unknown type')
    bad('hex', {'op': 'deg2rad', 'value': '7FF8000000000000'}, 'invalid hex')
    bad('hex-short', {'op': 'rad2deg', 'value': '00'}, 'invalid hex')
    bad('hex-type', {'op': 'rad2decideg', 'value': 0}, 'expected hex')
    bad('point', {'op': 'rotate_d', 'point': [zero], 'angle': zero}, 'expected point')
    bad('op-type', {'op': 1}, 'expected string: op')


def generate():
    c = Corpus()
    rng = random.Random(SEED)
    gen_rotate_i(c, rng)
    gen_rotate_i_about(c, rng)
    gen_rotate_d(c, rng)
    gen_conversions(c, rng)
    within = gen_templates(c, rng)
    gen_intercepts(c, rng)
    gen_malformed(c)
    gen_template_budget(c, within)
    gen_templates_double(c, random.Random(DOUBLE_SEED))
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
