#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and controls for the kimath geometry pure p001 header unit.

Writes port/cases/<unit>.jsonl (requests whose pinned evaluation is defined and inside the
adapter's verified domain) and port/oracle/fixtures/geometry-pure-p001-controls.jsonl (requests
both programs must refuse, with the refusal: an undefined pinned evaluation, a three-point
SHAPE_ARC outside the shape header adapter's verified domain, or a malformed input). No result is
written anywhere.

The split replays only exact integer arithmetic (Python integers for every int difference,
negation, sum, shift and std::abs the headers evaluate). No host libm function and no float
power is evaluated, so the output is identical on every host. DIRECTION_45's vector constructors
call atan2, which this generator does not evaluate: it only needs to know that a nonzero vector
gives a direction in 0..7 and the zero vector UNDEFINED, and a member whose definedness would
depend on which of 0..7 it is is dropped.

The undefined operations are those listed in the adapter: the negation of INT_MIN and the int
differences of the DIRECTION_45 constructors, an OppositeMap index outside 0..8, a Mask shift
count outside 0..31, int overflow and std::abs( INT_MIN ) in Angle, Right and Left,
std::abs( INT_MIN ) in the int snapping templates and a GetClampedCoords padding above INT_MAX.
Every other body is defined for every input.

A request whose DIRECTION_45 comes from a three-point SHAPE_ARC inside the verified domain is a
public case: the oracle evaluates it, and the Rust port defers at the shape_arc.cpp constructor
(libs.kimath.src.geometry.shape_arc.application.ui.p001). A control marked "rust_unported" is
undefined only after that constructor: the oracle refuses it and Rust defers to that owner first.

--check verifies the committed files equal the generator output.
"""
import argparse
import json
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.geometry.application.pure.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/geometry-pure-p001-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
U32_MAX, U64_MAX = 2**32 - 1, 2**64 - 1
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined geometry header operation'}
OUTSIDE = {'kind': 'invalid_input', 'message': 'outside the verified domain'}
ARC_OWNER = {'upstream': 'SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const VECTOR2I&, int)',
             'unit': 'libs.kimath.src.geometry.shape_arc.application.ui.p001'}
ARC_DOMAIN = 2**20
SEED = 20261130
LIMITS = [INT_MIN, INT_MIN + 1, -2**30, -65536, -1, 0, 1, 65536, 2**30, INT_MAX - 1, INT_MAX]


class Undefined(Exception):
    """The pinned evaluation reaches undefined behaviour."""


class Unknown(Exception):
    """The split cannot be decided without evaluating atan2."""


class DeferredUndefined(Exception):
    """Refused by the oracle only after a call the Rust port defers."""

    def __init__(self, error=None):
        super().__init__(error)
        self.error = error or UNDEFINED


class Outside(Exception):
    """Outside the adapter's verified domain of an out-of-line call."""


def I(v):
    if not INT_MIN <= v <= INT_MAX:
        raise Undefined
    return v


def bits(v):
    return struct.pack('>d', v).hex()


def raw(h):
    return struct.unpack('>d', bytes.fromhex(h))[0]


def step(v, n):
    """The binary64 value n representable steps above a finite v (bit arithmetic, no libm)."""
    for _ in range(abs(n)):
        up = n > 0
        if v == 0.0:
            v = 5e-324 if up else -5e-324
            continue
        b = struct.unpack('>q', struct.pack('>d', v))[0]
        b = b + 1 if (v > 0) == up else b - 1
        v = struct.unpack('>d', struct.pack('>q', b))[0]
    return v


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
        """classify() raises Undefined, DeferredUndefined, Outside or Unknown (dropped)."""
        try:
            if classify:
                classify()
        except Unknown:
            return False
        except DeferredUndefined as d:
            self.controls.append({'error': d.error, 'request': self.request(f'control-{name}', inp),
                                  'rust_unported': ARC_OWNER})
            return True
        except Undefined:
            self.controls.append({'error': UNDEFINED, 'request': self.request(f'control-{name}', inp)})
            return True
        except Outside:
            self.controls.append({'error': OUTSIDE, 'request': self.request(f'control-{name}', inp)})
            return True
        self.cases.append(self.request(name, inp))
        return True

    def refuse(self, name, inp, error):
        self.controls.append({'error': error, 'request': self.request(f'control-{name}', inp)})


# ---------------------------------------------------------------------------------------------
# DIRECTION_45 replay.

def arc_domain(start, mid, end, width):
    def within(v):
        return -ARC_DOMAIN <= v <= ARC_DOMAIN
    if not all(within(v) for v in (*start, *mid, *end, width)):
        raise Outside
    d1 = (mid[0] - start[0], mid[1] - start[1])
    d2 = (end[0] - mid[0], end[1] - mid[1])
    n1 = d1[0] ** 2 + d1[1] ** 2
    n2 = d2[0] ** 2 + d2[1] ** 2
    cross = d1[0] * d2[1] - d1[1] * d2[0]
    if start == end or n1 == 0 or n2 == 0 or cross * cross * 64 < n1 * n2:
        raise Outside


def construct(vec):
    """The direction of a constructor vector (y already negated): -1, or 'compass' for 0..7."""
    return -1 if vec == (0, 0) else 'compass'


def replay_direction(spec):
    """(dir, deg90, deferred): dir is an int, or 'compass' for an unknown value in 0..7."""
    ctor = spec['ctor']
    a90 = spec.get('a90', False)
    if ctor == 'default':
        return -1, False, False
    if ctor == 'directions':
        return spec['value'], False, False
    if ctor == 'vector':
        x, y = spec['vec']
        return construct((x, I(-y))), a90, False
    if ctor == 'seg':
        (ax, ay), (bx, by) = spec['a'], spec['b']
        dx, dy = I(bx - ax), I(by - ay)
        return construct((dx, I(-dy))), a90, False
    arc = spec['arc']
    if arc['ctor'] == 'default':
        return -1, a90, False
    arc_domain(tuple(arc['start']), tuple(arc['mid']), tuple(arc['end']), arc['width'])
    s, e = arc['start'], arc['end']
    # Within 2^20 the difference and its negation are in range; start != end.
    return construct((e[0] - s[0], -(e[1] - s[1]))), a90, True


def values_of(d):
    return range(0, 8) if d == 'compass' else [d]


def replay_member(member, dir_spec, other_spec):
    """Raise Undefined for an undefined member, Unknown where it depends on atan2."""
    d, deg90, deferred = replay_direction(dir_spec)
    o = None
    if other_spec is not None:
        # The second constructor runs after the first: once the first has deferred in Rust, a
        # refusal of the second is the oracle's alone.
        try:
            o, _, other_deferred = replay_direction(other_spec)
        except Undefined:
            if deferred:
                raise DeferredUndefined(UNDEFINED)
            raise
        except Outside:
            if deferred:
                raise DeferredUndefined(OUTSIDE)
            raise
        deferred = deferred or other_deferred

    def check(pred):
        """pred( value ) is True where undefined, for every possible value of the direction."""
        outcomes = set()
        for v in values_of(d):
            for w in (values_of(o) if o is not None else [None]):
                outcomes.add(pred(v, w))
        if len(outcomes) > 1:
            raise Unknown
        if outcomes == {True}:
            raise DeferredUndefined if deferred else Undefined

    if member == 'opposite':
        check(lambda v, w: not 0 <= v <= 8)
    elif member == 'mask':
        check(lambda v, w: not 0 <= v <= 31)
    elif member in ('angle', 'is_obtuse'):
        def angle(v, w):
            if v == -1 or w == -1:
                return False
            diff = v - w
            return not INT_MIN < diff <= INT_MAX
        check(angle)
    elif member == 'right':
        check(lambda v, w: v != -1 and v + (2 if deg90 else 1) > INT_MAX)
    elif member == 'left':
        check(lambda v, w: v != -1 and v + 8 > INT_MAX)


DIRECTION_VALUES = [-1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, -2, -5, -7, -8, -9, 15, 16, 31, 32, 33, 100,
                    INT_MIN, INT_MIN + 1, INT_MIN + 2, INT_MIN + 7, INT_MIN + 8, INT_MAX, INT_MAX - 1,
                    INT_MAX - 2, INT_MAX - 7, INT_MAX - 8, INT_MAX - 9]
# Compass vectors, the 22.5 degree boundaries at several scales (tan 22.5 = 0.41421356...),
# the limits and the undefined negation.
COMPASS = [(0, -10), (10, -10), (10, 0), (10, 10), (0, 10), (-10, 10), (-10, 0), (-10, -10), (0, 0),
           (1, 0), (0, 1), (-1, 0), (0, -1), (1000, 414), (1000, 415), (1000000000, 414213562),
           (1000000000, 414213563), (414213562, 1000000000), (414213563, 1000000000),
           (-1000000000, 414213562), (-414213563, -1000000000), (985, 408), (408, 985),
           (2**30, 2**30), (INT_MAX, INT_MAX), (INT_MAX, INT_MIN + 1), (INT_MIN, INT_MAX),
           (INT_MIN, 0), (INT_MAX, 1), (INT_MAX, -1), (INT_MIN, -1), (INT_MIN, 1), (5, INT_MIN),
           (0, INT_MIN), (INT_MIN, INT_MIN), (1, -2), (2, -5), (-3, 7), (12345, -98765)]
MEMBERS = ['state', 'format', 'opposite', 'is_diagonal', 'is_defined', 'right', 'left', 'to_vector',
           'mask', 'all_directions_mask']
BINARY = ['angle', 'is_obtuse', 'eq', 'ne']


def rand_point(rng, scale=None):
    if scale is None:
        if rng.random() < 0.15:
            return [rng.choice(LIMITS), rng.choice(LIMITS)]
        scale = rng.choice([10, 1000, 10**6, 10**8, 2**30, INT_MAX])
    return [rng.randint(-scale, scale), rng.randint(-scale, scale)]


def rand_arc(rng):
    """A three-point arc spec, inside the verified domain most of the time."""
    scale = rng.choice([100, 10**4, ARC_DOMAIN, ARC_DOMAIN * 2])
    return {'ctor': 'three_point', 'start': rand_point(rng, scale), 'mid': rand_point(rng, scale),
            'end': rand_point(rng, scale), 'width': rng.randint(0, min(scale, ARC_DOMAIN))}


def rand_direction(rng):
    r = rng.random()
    if r < 0.25:
        return {'ctor': 'directions', 'value': rng.choice(DIRECTION_VALUES)}
    if r < 0.3:
        return {'ctor': 'default'}
    if r < 0.6:
        spec = {'ctor': 'vector', 'vec': list(rng.choice(COMPASS)) if rng.random() < 0.5 else rand_point(rng)}
    elif r < 0.9:
        spec = {'ctor': 'seg', 'a': rand_point(rng), 'b': rand_point(rng)}
    elif r < 0.95:
        spec = {'ctor': 'arc', 'arc': rand_arc(rng)}
    else:
        spec = {'ctor': 'arc', 'arc': {'ctor': 'default'}}
    if rng.random() < 0.5:
        spec['a90'] = rng.random() < 0.5
    return spec


def gen_direction(c, rng):
    """Every member on every constructor kind."""
    i = 0

    def add(member, d, other=None):
        nonlocal i
        inp = {'op': 'direction45', 'member': member, 'dir': d}
        if other is not None:
            inp['other'] = other
        c.add(f'direction45-{member}-{i}', inp, lambda: replay_member(member, d, other))
        i += 1

    for v in DIRECTION_VALUES:
        d = {'ctor': 'directions', 'value': v}
        for member in MEMBERS:
            add(member, d)
        for w in DIRECTION_VALUES:
            add(BINARY[(i // 7) % 4], d, {'ctor': 'directions', 'value': w})
    for vec in COMPASS:
        for a90 in (None, False, True):
            d = {'ctor': 'vector', 'vec': list(vec)}
            if a90 is not None:
                d['a90'] = a90
            for member in MEMBERS:
                add(member, d)
            add('angle', d, {'ctor': 'vector', 'vec': [10, 0]})
            add('is_obtuse', d, {'ctor': 'vector', 'vec': [10, -10]})
            add('eq', d, {'ctor': 'directions', 'value': 2})
    segs = [([0, 0], [10, -10]), ([5, 5], [5, 5]), ([INT_MIN, 0], [INT_MAX, 0]), ([INT_MAX, 0], [INT_MIN, 0]),
            ([0, INT_MAX], [0, INT_MIN]), ([0, INT_MIN], [0, INT_MAX]), ([0, 1], [0, INT_MIN + 1]),
            ([0, 0], [0, INT_MIN]), ([-1, 0], [INT_MAX, 0]), ([0, -1], [0, INT_MAX]), ([1, 1], [-7, 12])]
    for a, b in segs:
        for a90 in (None, True):
            d = {'ctor': 'seg', 'a': a, 'b': b}
            if a90 is not None:
                d['a90'] = a90
            for member in ('state', 'right', 'left', 'opposite', 'to_vector', 'format'):
                add(member, d)
    arcs = [{'ctor': 'default'},
            {'ctor': 'three_point', 'start': [0, 0], 'mid': [5, 5], 'end': [10, 0], 'width': 0},
            {'ctor': 'three_point', 'start': [-1000, 0], 'mid': [0, 1000], 'end': [1000, 0], 'width': 10},
            {'ctor': 'three_point', 'start': [0, 0], 'mid': [1, 0], 'end': [2, 0], 'width': 0},
            {'ctor': 'three_point', 'start': [0, 0], 'mid': [5, 5], 'end': [0, 0], 'width': 0},
            {'ctor': 'three_point', 'start': [ARC_DOMAIN, 0], 'mid': [0, ARC_DOMAIN],
             'end': [-ARC_DOMAIN, 0], 'width': ARC_DOMAIN},
            {'ctor': 'three_point', 'start': [ARC_DOMAIN + 1, 0], 'mid': [0, ARC_DOMAIN],
             'end': [-ARC_DOMAIN, 0], 'width': 0}]
    for arc in arcs:
        for a90 in (None, True):
            d = {'ctor': 'arc', 'arc': arc}
            if a90 is not None:
                d['a90'] = a90
            for member in MEMBERS:
                add(member, d)
            add('angle', d, {'ctor': 'directions', 'value': 3})
            # Defined in C++ only before the arc's direction is known to be in 0..7: undefined for
            # every value, after the constructor Rust defers.
            add('angle', d, {'ctor': 'directions', 'value': INT_MIN})
    for k in range(400):
        d = rand_direction(rng)
        if rng.random() < 0.3:
            add(rng.choice(BINARY), d, rand_direction(rng))
        else:
            add(rng.choice(MEMBERS), d)


# ---------------------------------------------------------------------------------------------
# HALF_LINE and LINE.

def rand_ray(rng):
    if rng.random() < 0.5:
        return {'ctor': 'points', 'a': rand_point(rng), 'b': rand_point(rng)}
    return {'ctor': 'seg', 'seg': {'a': rand_point(rng), 'b': rand_point(rng),
                                   'index': rng.choice([-1, 0, 7, INT_MIN, INT_MAX])}}


def gen_lines(c, rng):
    fixed = [({'ctor': 'seg', 'seg': {'a': [0, 0], 'b': [100, 100], 'index': 3}},
              {'ctor': 'points', 'a': [0, 0], 'b': [100, 100]}),
             ({'ctor': 'points', 'a': [0, 0], 'b': [100, 100]},
              {'ctor': 'points', 'a': [100, 100], 'b': [0, 0]}),
             ({'ctor': 'points', 'a': [5, 5], 'b': [5, 5]}, {'ctor': 'points', 'a': [5, 5], 'b': [5, 6]}),
             ({'ctor': 'seg', 'seg': {'a': [INT_MIN, INT_MAX], 'b': [INT_MAX, INT_MIN], 'index': INT_MIN}},
              {'ctor': 'seg', 'seg': {'a': [INT_MIN, INT_MAX], 'b': [INT_MAX, INT_MIN], 'index': INT_MAX}})]
    for op in ('half_line', 'line'):
        for k, (a, b) in enumerate(fixed):
            c.add(f'{op}-{k}', {'op': op, 'line': a, 'other': b})
        for k in range(60):
            a = rand_ray(rng)
            b = a if rng.random() < 0.3 else rand_ray(rng)
            c.add(f'{op}-r{k}', {'op': op, 'line': a, 'other': b})


# ---------------------------------------------------------------------------------------------
# geometry_utils.h templates.

NANS = ['7ff8000000000000', 'fff8000000000000', '7ff80000000000aa', 'fff800000000cccc',
        '7ff00000000000dd', 'fff40000000000ee', '7ff0000000000001']
F64_VALUES = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1.5, 2.5, -2.5, 3.0, -7.25, 1e-310, -1e-310, 5e-324,
              1e300, -1e300, 1.7976931348623157e308, -1.7976931348623157e308, 0.9e308, 123456.789,
              -98765.4321, 2.0**53, 2.0**31, -2.0**31, 2.0**31 - 1, -(2.0**31 - 1), 2.0**31 - 2,
              2.0**31 - 1.5, 2147483646.5, -2147483646.5, 2147483645.5, 1e17, -3e-8,
              float('inf'), float('-inf')]


def rand_double(rng):
    r = rng.random()
    if r < 0.3:
        return bits(rng.choice(F64_VALUES))
    if r < 0.4:
        return rng.choice(NANS)
    if r < 0.5:
        return f'{rng.getrandbits(64):016x}'
    scale = rng.choice([1.0, 1e3, 1e6, 1e9, 3e9, 1e12])
    return bits(rng.uniform(-scale, scale))


SNAP_I = [(10, 3), (3, 10), (10, -6), (-6, 10), (10, 5), (5, 10), (10, 4), (4, 10), (10, 10), (-10, 10),
          (0, 0), (0, 7), (7, 0), (-7, 0), (0, -7), (1, 0), (2, 1), (1, 2), (3, 1), (-3, -1), (INT_MAX, 0),
          (INT_MAX, INT_MAX), (INT_MAX, -INT_MAX), (-INT_MAX, 1), (2**30, 2**29), (2**30, 2**29 - 1),
          (2**30 + 1, 2**29), (INT_MIN, 0), (0, INT_MIN), (INT_MIN, INT_MIN), (INT_MIN, 5)]


def snapped_classify(vec):
    def f():
        if INT_MIN in vec:
            raise Undefined
    return f


def gen_snapping(c, rng):
    i = 0
    for op in ('snapped45', 'snapped90'):
        for vec in SNAP_I:
            for only45 in ((None, False, True) if op == 'snapped45' else (None,)):
                inp = {'op': op, 'type': 'int', 'vec': list(vec)}
                if only45 is not None:
                    inp['only45'] = only45
                c.add(f'{op}-int-{i}', inp, snapped_classify(vec))
                i += 1
        for k in range(120):
            vec = rand_point(rng)
            inp = {'op': op, 'type': 'int', 'vec': vec}
            if op == 'snapped45' and rng.random() < 0.6:
                inp['only45'] = rng.random() < 0.5
            c.add(f'{op}-int-r{k}', inp, snapped_classify(vec))
        d_values = [bits(v) for v in (0.0, -0.0, 1.0, -2.0, 3.5, 1e300, 1e308, -1e308, float('inf'),
                                     float('-inf'), 5e-324)] + NANS
        for x in d_values:
            for y in d_values:
                for only45 in ((None, True) if op == 'snapped45' else (None,)):
                    inp = {'op': op, 'type': 'double', 'vec': [x, y]}
                    if only45 is not None:
                        inp['only45'] = only45
                    c.add(f'{op}-double-{i}', inp)
                    i += 1
        for k in range(150):
            inp = {'op': op, 'type': 'double', 'vec': [rand_double(rng), rand_double(rng)]}
            if op == 'snapped45' and rng.random() < 0.6:
                inp['only45'] = rng.random() < 0.5
            c.add(f'{op}-double-r{k}', inp)


PADDINGS = [None, 0, 1, 2, 1000, 20000000, 200000000, INT_MAX - 1, INT_MAX, INT_MAX + 1, U32_MAX]


def padding_classify(padding):
    def f():
        if padding is not None and padding > INT_MAX:
            raise Undefined
    return f


def gen_clamping(c, rng):
    combos = [('int', 'int'), ('double', 'double'), ('double', 'int')]
    i = 0
    int_coords = [[0, 0], [INT_MIN, INT_MAX], [INT_MAX, INT_MIN], [INT_MAX - 1, -(INT_MAX - 1)],
                  [INT_MAX - 2, INT_MIN + 2], [123, -456], [2**30, -2**30], [INT_MAX - 20000000, 0]]
    edge = [bits(v) for v in F64_VALUES] + NANS
    for in_t, ret_t in combos:
        for padding in PADDINGS:
            coords = int_coords if in_t == 'int' else [[x, y] for x, y in zip(edge, edge[3:] + edge[:3])]
            for co in coords:
                inp = {'op': 'clamped_coords', 'in': in_t, 'ret': ret_t, 'coords': co}
                if padding is not None:
                    inp['padding'] = padding
                c.add(f'clamped-{in_t}-{ret_t}-{i}', inp, padding_classify(padding))
                i += 1
        # The exact clamp bounds and their neighbours.
        if in_t == 'double':
            for padding in (0, 1, 7, 20000000, INT_MAX):
                m = float(INT_MAX - padding)
                for v in (m, -m, step(m, 1), step(m, -1), step(-m, 1), step(-m, -1), m - 0.5, -m + 0.5):
                    inp = {'op': 'clamped_coords', 'in': in_t, 'ret': ret_t,
                           'coords': [bits(v), bits(-v)], 'padding': padding}
                    c.add(f'clamped-{in_t}-{ret_t}-{i}', inp)
                    i += 1
        for k in range(100):
            padding = rng.choice(PADDINGS + [rng.randint(0, INT_MAX), rng.randint(0, U32_MAX)])
            co = rand_point(rng) if in_t == 'int' else [rand_double(rng), rand_double(rng)]
            inp = {'op': 'clamped_coords', 'in': in_t, 'ret': ret_t, 'coords': co}
            if padding is not None:
                inp['padding'] = padding
            c.add(f'clamped-{in_t}-{ret_t}-r{k}', inp, padding_classify(padding))
    safe_d = [2.0**31, -2.0**31, 2.0**31 - 1, -(2.0**31 - 1), step(2.0**31 - 1, -1),
              step(-(2.0**31), 1), step(-(2.0**31), -1), 0.0, -0.0, 1e300, float('inf'),
              float('-inf'), 2147483646.5, -2147483647.5]
    for x in [bits(v) for v in safe_d] + NANS[:3]:
        for y in (bits(0.0), bits(2.0**31 - 1.5), NANS[0]):
            c.add(f'safe-double-{i}', {'op': 'is_vec2_safe_xy', 'type': 'double', 'vec': [x, y]})
            c.add(f'safe-double-{i}-t', {'op': 'is_vec2_safe_xy', 'type': 'double', 'vec': [y, x]})
            i += 1
    for x in LIMITS:
        for y in (0, INT_MIN, INT_MAX, INT_MAX - 1):
            c.add(f'safe-int-{i}', {'op': 'is_vec2_safe_xy', 'type': 'int', 'vec': [x, y]})
            i += 1
    for k in range(40):
        c.add(f'safe-double-r{k}', {'op': 'is_vec2_safe_xy', 'type': 'double',
                                    'vec': [rand_double(rng), rand_double(rng)]})


# ---------------------------------------------------------------------------------------------
# geometry_predicates.h.

M = 1500000000


def gen_predicates(c, rng):
    tri = [([0, 0], [10, 0], [0, 10]), ([0, 0], [0, 10], [10, 0]), ([0, 0], [5, 5], [10, 10]),
           ([-1500000000, -1500000000], [1500000001, 1500000000], [1500000002, 1500000001]),
           ([-2000000000, 0], [2000000000, 0], [2000000000, 1000]), ([0, 0], [0, 0], [0, 0]),
           ([0, 0], [1000, 0], [1000, 100]), ([0, 0], [1000, 0], [1000, 101]), ([0, 0], [1000, 0], [500, 0]),
           ([INT_MIN, INT_MIN], [INT_MAX, INT_MAX], [INT_MIN, INT_MAX]), ([INT_MAX, INT_MAX], [INT_MIN, INT_MIN], [INT_MAX, INT_MIN]),
           ([0, 0], [1, 0], [10, 0]), ([0, 0], [10, 0], [0, 1]), ([0, 0], [1, 0], [0, 10]), ([0, 0], [10, 0], [0, 1]),
           ([7, 7], [7, 7], [8, 8])]
    for k, (a, b, cc) in enumerate(tri):
        for op in ('orientation_sign', 'is_sliver_triangle'):
            c.add(f'{op}-{k}', {'op': op, 'a': a, 'b': b, 'c': cc})
    quads = [([0, 0], [1000000, 0], [0, 1000000], [1000000, 1000000]),
             ([0, 0], [1000000, 0], [0, 1000000], [999999, 999999]),
             ([0, 0], [1000000, 0], [0, 1000000], [1000001, 1000001]),
             ([0, 0], [284000, 42000], [276000, 68000], [38000, 126000]),
             ([5, 0], [0, 5], [-5, 0], [0, -5]), ([5, 0], [0, 5], [-5, 0], [3, 4]), ([5, 0], [0, 5], [-5, 0], [3, -4]),
             ([M, 0], [0, M], [-M, 0], [0, 0]), ([M, 0], [0, M], [-M, 0], [0, -M]),
             ([M + 1, 0], [0, M], [-M, 0], [0, 0]), ([M, M], [-M, M], [-M, -M], [0, 0]),
             ([M, M], [-M, M], [-M, -M], [M, -M]), ([M + 1, M], [-M, M], [-M, -M], [M, -M]),
             ([INT_MAX, 0], [0, INT_MAX], [INT_MIN, 0], [0, INT_MIN]), ([INT_MAX, 0], [0, INT_MAX], [INT_MIN, 0], [0, 0]),
             ([0, 0], [0, 0], [0, 0], [0, 0]), ([0, 0], [10, 0], [20, 0], [5, 5])]
    for k, (a, b, cc, p) in enumerate(quads):
        for op in ('in_circle_delaunay_legal', 'in_circle_legal_double'):
            c.add(f'{op}-{k}', {'op': op, 'a': a, 'b': b, 'c': cc, 'p': p})
    for k in range(250):
        scale = rng.choice([10, 1000, 10**6, 10**9, M, INT_MAX])
        pts = [rand_point(rng, scale) for _ in range(4)]
        if rng.random() < 0.2:
            # Collinear or repeated points.
            pts[2] = list(pts[0]) if rng.random() < 0.5 else [2 * pts[1][0] - pts[0][0], 2 * pts[1][1] - pts[0][1]]
            pts[2] = [max(INT_MIN, min(INT_MAX, v)) for v in pts[2]]
        op = ('orientation_sign', 'is_sliver_triangle', 'in_circle_delaunay_legal', 'in_circle_legal_double')[k % 4]
        inp = {'op': op, 'a': pts[0], 'b': pts[1], 'c': pts[2]}
        if op.startswith('in_circle'):
            inp['p'] = pts[3]
        c.add(f'{op}-r{k}', inp)
    # Near-cocircular quads on large circles, where the double filter and the exact test differ.
    for k in range(40):
        r = rng.choice([5, 25, 65, 1105, 5525, 32045])
        # Scaled so every coordinate (and p's offset of one) stays inside int.
        s = min(rng.choice([1, 1000, 10**5, 2 * 10**5]), (INT_MAX - 1) // r)
        base = [[r * s, 0], [0, r * s], [-r * s, 0]]
        p = [0, -r * s + rng.choice([-1, 0, 1])]
        op = ('in_circle_delaunay_legal', 'in_circle_legal_double')[k % 2]
        c.add(f'{op}-cocircular-{k}', {'op': op, 'a': base[0], 'b': base[1], 'c': base[2], 'p': p})


# ---------------------------------------------------------------------------------------------
# fracture_edge_index_utils.h.

U64S = [0, 1, 2, 3, 4, 7, 8, 9, 99, 100, 4095, 4096, 2**32, 2**32 + 1, 2**32 - 1, 2**33, 2**53, 2**53 + 1,
        2**63, 2**64 - 2**11, U64_MAX - 1, U64_MAX, U64_MAX // 2, U64_MAX // 4, U64_MAX // 4 + 1, 32767, 32768]


def gen_fracture(c, rng):
    for k, n in enumerate(U64S + [v * v for v in (3, 65535, 65536, 65537, 2**20)] +
                          [v * v - 1 for v in (3, 65536, 2**20)]):
        c.add(f'stripe_count_for-{k}', {'op': 'stripe_count_for', 'edge_count': n})
    for k in range(40):
        c.add(f'stripe_count_for-r{k}', {'op': 'stripe_count_for', 'edge_count': rng.getrandbits(rng.choice([8, 20, 33, 52, 64]))})
    ys = [INT_MIN, INT_MIN + 1, -1000, -1, 0, 1, 999, 1000, 1001, INT_MAX - 1, INT_MAX]
    counts = [0, 1, 2, 7, 64, 65536, U32_MAX - 1, U32_MAX]
    i = 0
    for (lo, hi) in [(0, 1000), (1000, 0), (5, 5), (INT_MIN, INT_MAX), (-1000, 1000), (INT_MAX, INT_MAX)]:
        for y in ys:
            for n in counts:
                c.add(f'map_y_to_stripe-{i}', {'op': 'map_y_to_stripe', 'y': y, 'min_y': lo, 'max_y': hi, 'stripe_count': n})
                i += 1
    for k in range(80):
        lo, hi = sorted([rng.randint(INT_MIN, INT_MAX), rng.randint(INT_MIN, INT_MAX)])
        if rng.random() < 0.1:
            lo, hi = hi, lo
        inp = {'op': 'stripe_span', 'y1': rng.randint(INT_MIN, INT_MAX), 'y2': rng.randint(lo, hi) if lo <= hi else 0,
               'min_y': lo, 'max_y': hi, 'stripe_count': rng.choice(counts + [rng.randint(0, U32_MAX)])}
        c.add(f'stripe_span-r{k}', inp)
    for k, (t, n, s) in enumerate([(0, 0, 0), (5, U64_MAX, 0), (0, U64_MAX, 1), (1, U64_MAX, 1), (0, U64_MAX // 4, 4),
                                   (3, U64_MAX // 4, 4), (4, U64_MAX // 4, 4), (U64_MAX, 0, 9), (U64_MAX, 1, 1),
                                   (U64_MAX - 3, 1, 3), (U64_MAX - 3, 1, 4), (2**32, 2**32, 2**32 - 1), (7, 2**32, 2**32)]):
        c.add(f'checked_add-{k}', {'op': 'checked_add', 'total': t, 'count': n, 'element_size': s})
    for k, (n, s) in enumerate([(0, 0), (1, 1), (U64_MAX // 2, 1), (U64_MAX // 2 + 1, 1), (U64_MAX, 0), (2**40, 2**23),
                                (2**40, 2**24), (2**40, 2**23 + 1), (1000, 48)]):
        c.add(f'capacity_budget-{k}', {'op': 'capacity_budget', 'edge_count': n, 'edge_size': s})
    fits = [(1, 1, 1, 1, 1, 38), (1, 1, 1, 1, 1, 37), (0, 0, U64_MAX, 0, 1, U64_MAX), (0, 0, 0, 0, 0, 0),
            (0, 0, 0, 0, 0, 7), (0, 0, 0, 0, 0, 8), (0, 0, 0, U64_MAX, 1, U64_MAX), (0, 0, 0, 2**61, 8, U64_MAX),
            (U64_MAX // 4, 0, 0, 0, 0, U64_MAX), (10, 20, 30, 40, 48, 10**6), (10, 20, 30, 40, 48, 2420)]
    for k, f in enumerate(fits):
        c.add(f'capacity_fits-{k}', dict(zip(['op', 'bucket_ids', 'long_ids', 'stripe_count', 'hole_count', 'node_size',
                                               'budget'], ('capacity_fits',) + f)))
    for k in range(60):
        vals = [rng.choice(U64S + [rng.getrandbits(rng.choice([4, 16, 40, 62]))]) for _ in range(6)]
        c.add(f'capacity_fits-r{k}', dict(zip(['op', 'bucket_ids', 'long_ids', 'stripe_count', 'hole_count', 'node_size',
                                                'budget'], ['capacity_fits'] + vals)))
    names = ['bucket_ids', 'long_ids', 'offsets', 'scratch', 'heads', 'nodes', 'node_size', 'budget']
    actual = [(1, 1, 1, 1, 1, 1, 1, 21), (1, 1, 1, 1, 1, 1, 1, 20), (U64_MAX, 0, 0, 0, 0, 0, 0, U64_MAX),
              (0, 0, 0, 0, 0, U64_MAX, 1, U64_MAX), (0, 0, 0, 0, 0, U64_MAX, 2, U64_MAX), (0,) * 8]
    for k, a in enumerate(actual):
        for bytes_in in (None, 0, 12345):
            inp = dict(zip(['op'] + names, ['actual_capacity_fits', *a]))
            if bytes_in is not None:
                inp['bytes'] = bytes_in
            c.add(f'actual_capacity_fits-{k}-{bytes_in}', inp)
    for k in range(60):
        inp = dict(zip(['op'] + names, ['actual_capacity_fits'] +
                       [rng.choice(U64S + [rng.getrandbits(rng.choice([4, 16, 40, 62]))]) for _ in names]))
        if rng.random() < 0.6:
            inp['bytes'] = rng.getrandbits(64)
        c.add(f'actual_capacity_fits-r{k}', inp)
    for k, (v, h) in enumerate([(32768, 8), (32767, 8), (32768, 7), (0, 0), (U64_MAX, U64_MAX), (U64_MAX, 7)]):
        c.add(f'should_index-{k}', {'op': 'should_index', 'estimated_visits': v, 'hole_count': h})


# ---------------------------------------------------------------------------------------------
# The adopted co-ports and the header types.

def gen_coports(c, rng):
    c.add('header_types', {'op': 'header_types'})
    triples = [[[0, 0], [5, 5], [10, 0]], [[0, 0], [1, 1], [2, 2]], [[3, 3], [9, 9], [3, 3]],
               [[-1000, 0], [0, 1000], [1000, 0]], [[1000, 0], [0, 1000], [-1000, 0]],
               [[0, 0], [1, 1000000], [2, 0]], [[0, 0], [1000000, 1], [2000000, 0]],
               [[INT_MIN, INT_MIN], [0, INT_MAX], [INT_MAX, INT_MIN]], [[INT_MAX, INT_MAX], [INT_MIN, 0], [INT_MAX, INT_MIN]],
               [[0, 0], [1, 1], [0, 2]], [[0, 0], [0, 0], [10, 0]]]
    c.add('arc_chord-none', {'op': 'arc_chord_params', 'computes': []})
    for k, t in enumerate(triples):
        c.add(f'arc_chord-{k}', {'op': 'arc_chord_params', 'computes': [t]})
    # Members keep their values across a refused Compute.
    c.add('arc_chord-seq-0', {'op': 'arc_chord_params', 'computes': [triples[0], triples[1]]})
    c.add('arc_chord-seq-1', {'op': 'arc_chord_params', 'computes': [triples[3], triples[2], triples[10]]})
    c.add('arc_chord-seq-2', {'op': 'arc_chord_params', 'computes': [triples[1], triples[4], triples[7], triples[2]]})
    for k in range(60):
        n = rng.randint(1, 4)
        comp = [[rand_point(rng) for _ in range(3)] for _ in range(n)]
        c.add(f'arc_chord-r{k}', {'op': 'arc_chord_params', 'computes': comp})
    circles = [{'ctor': 'default'}, {'ctor': 'center_radius', 'center': [0, 0], 'radius': 0},
               {'ctor': 'center_radius', 'center': [1, 2], 'radius': 3},
               {'ctor': 'center_radius', 'center': [1, 2], 'radius': 4},
               {'ctor': 'center_radius', 'center': [2, 1], 'radius': 3},
               {'ctor': 'center_radius', 'center': [INT_MIN, INT_MAX], 'radius': -5},
               {'ctor': 'center_radius', 'center': [INT_MIN, INT_MAX], 'radius': INT_MAX}]
    i = 0
    for a in circles:
        for b in circles:
            c.add(f'circle_eq-{i}', {'op': 'circle_eq', 'a': a, 'b': b})
            i += 1


# ---------------------------------------------------------------------------------------------

def gen_malformed(c):
    """Malformed inputs both programs refuse with the same message."""
    def bad(name, inp, message):
        c.refuse(f'malformed-{name}', inp, {'kind': 'invalid_input', 'message': message})

    zero = bits(0.0)
    bad('op', {'op': 'nope'}, 'unknown operation')
    bad('op-type', {'op': 1}, 'expected string: op')
    bad('member', {'op': 'direction45', 'member': 'up', 'dir': {'ctor': 'default'}}, 'unknown member')
    bad('member-extra', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'default'},
                         'other': {'ctor': 'default'}}, 'unexpected or missing field')
    bad('dir-ctor', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'polar'}}, 'unknown direction constructor')
    bad('dir-a90', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'vector', 'vec': [1, 0], 'a90': 1}},
        'expected boolean')
    bad('dir-a90-first', {'op': 'direction45', 'member': 'state',
                          'dir': {'ctor': 'vector', 'vec': [1, INT_MIN], 'a90': 0}}, 'expected boolean')
    bad('dir-extra', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'default', 'x': 1}},
        'unexpected or missing field')
    bad('dir-value', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'directions', 'value': INT_MAX + 1}},
        'integer out of range')
    bad('arc-ctor', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'arc', 'arc': {'ctor': 'centre'}}},
        'unknown arc constructor')
    bad('seg-point', {'op': 'direction45', 'member': 'state', 'dir': {'ctor': 'seg', 'a': [0], 'b': [0, 0]}},
        'expected point')
    bad('line-ctor', {'op': 'half_line', 'line': {'ctor': 'ray'}, 'other': {'ctor': 'ray'}}, 'unknown line constructor')
    bad('line-seg', {'op': 'line', 'line': {'ctor': 'seg', 'seg': {'a': [0, 0], 'b': [1, 1]}},
                     'other': {'ctor': 'points', 'a': [0, 0], 'b': [1, 1]}}, 'unexpected or missing field')
    bad('snap-type', {'op': 'snapped45', 'type': 'float', 'vec': [0, 0]}, 'unknown type')
    bad('snap90-only45', {'op': 'snapped90', 'type': 'int', 'vec': [0, 0], 'only45': True}, 'unexpected or missing field')
    bad('snap-hex', {'op': 'snapped90', 'type': 'double', 'vec': [zero, '7FF8000000000000']}, 'invalid hex')
    bad('snap-hex-type', {'op': 'snapped45', 'type': 'double', 'vec': [zero, 0]}, 'expected hex')
    bad('clamp-inst', {'op': 'clamped_coords', 'in': 'int', 'ret': 'double', 'coords': [0, 0]}, 'unsupported instantiation')
    bad('clamp-padding', {'op': 'clamped_coords', 'in': 'int', 'ret': 'int', 'coords': [0, 0], 'padding': -1},
        'integer out of range')
    bad('clamp-padding-u32', {'op': 'clamped_coords', 'in': 'int', 'ret': 'int', 'coords': [0, 0], 'padding': 2**32},
        'integer out of range')
    bad('clamp-coords-first', {'op': 'clamped_coords', 'in': 'double', 'ret': 'int', 'coords': [0, 0],
                               'padding': U32_MAX}, 'expected hex')
    bad('fracture-negative', {'op': 'stripe_count_for', 'edge_count': -1}, 'integer out of range')
    bad('fracture-float', {'op': 'should_index', 'estimated_visits': 1.0, 'hole_count': 8}, 'integer out of range')
    bad('fracture-u65', {'op': 'capacity_budget', 'edge_count': 2**64, 'edge_size': 1}, 'integer out of range')
    bad('stripe-count', {'op': 'map_y_to_stripe', 'y': 0, 'min_y': 0, 'max_y': 1, 'stripe_count': 2**32},
        'integer out of range')
    bad('chord-many', {'op': 'arc_chord_params', 'computes': [[[0, 0], [0, 0], [0, 0]]] * 5},
        'expected at most four computes')
    bad('chord-triple', {'op': 'arc_chord_params', 'computes': [[[0, 0], [0, 0]]]}, 'expected three points')
    bad('circle-ctor', {'op': 'circle_eq', 'a': {'ctor': 'tan'}, 'b': {'ctor': 'default'}}, 'unknown circle constructor')
    bad('types-extra', {'op': 'header_types', 'x': 0}, 'unexpected or missing field')
    bad('predicate-missing', {'op': 'in_circle_delaunay_legal', 'a': [0, 0], 'b': [0, 0], 'c': [0, 0]},
        'unexpected or missing field')


def generate():
    c = Corpus()
    rng = random.Random(SEED)
    gen_direction(c, rng)
    gen_lines(c, rng)
    gen_snapping(c, rng)
    gen_clamping(c, rng)
    gen_predicates(c, rng)
    gen_fracture(c, rng)
    gen_coports(c, rng)
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
    deferred = sum(1 for r in c.cases if '"three_point"' in json.dumps(r))
    print(f'{len(c.cases)} public cases ({deferred} through a three-point SHAPE_ARC), '
          f'{len(c.controls)} controls')


if __name__ == '__main__':
    main()
