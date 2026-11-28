#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and controls for the shape_line_chain.h / shape_index.h /
shape_index_list.h inline body unit (libs.kimath.include.geometry.application.ui.p004).

Writes port/cases/<unit>.jsonl (requests whose pinned evaluation is defined and inside the
adapter's verified domain) and port/oracle/fixtures/shape-line-chain-controls.jsonl (requests both
programs must refuse, with the refusal). No result is written anywhere.

The split replays the guarded evaluation with exact Python integers only: the SHAPE_LINE_CHAIN
members and its cached BOX2I (Compute, Inflate's inflateAxis, Merge with Normalize, Move, the
modular int conversions of GetRight/GetBottom), the SHAPE_CIRCLE and SHAPE_LINE_CHAIN boxes, the
BOX2 box distances (KiROUND( sqrt ) replayed with an integer square root, exact for the squared
distances below 2^46 the list cases use) and the iterator positions. No host libm function and no
float arithmetic is evaluated, so the output is identical on every host.

A script stops at its first call the Rust port defers (a call into shape_line_chain.cpp,
shape_arc.cpp or shape_collisions.cpp): such a case is compared by neither program beyond the
deferral, but the oracle still evaluates it inside its verified domain. A control marked
"rust_unported" is undefined only after such a call: the oracle refuses it and Rust defers to the
recorded owner first.

--check verifies the committed files equal the generator output.
"""
import argparse
import json
from pathlib import Path
import random

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.geometry.application.ui.p004'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/shape-line-chain-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1
DOMAIN = 1 << 20
TREE_DOMAIN = 1 << 29
RESERVE_DOMAIN = 1 << 20
POINTS_MAX_SIZE = I64_MAX // 8
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined p004 operation'}
OUTSIDE = {'kind': 'invalid_input', 'message': 'outside the verified domain'}
P001 = 'libs.kimath.src.geometry.shape_line_chain.application.ui.p001'
P002 = 'libs.kimath.src.geometry.shape_line_chain.application.ui.p002'
COLLISIONS = 'libs.kimath.src.geometry.shape_collisions.application.ui.p001'
SEED = 20261128


class Undefined(Exception):
    """The pinned evaluation reaches undefined behaviour."""


class Outside(Exception):
    """A call outside the adapter's verified domain."""


class Drop(Exception):
    """A candidate whose oracle outcome depends on an unmodelled result (a later exact
    collision); it is not emitted."""


class Deferred(Exception):
    """A call the Rust port defers; carries its owner."""

    def __init__(self, upstream, unit):
        super().__init__(upstream)
        self.owner = {'upstream': upstream, 'unit': unit}


def I(v):
    if v < INT_MIN or v > INT_MAX:
        raise Undefined()
    return v


def L(v):
    if v < I64_MIN or v > I64_MAX:
        raise Undefined()
    return v


def to_int(v):
    """C++20 modular conversion to int."""
    v &= 0xFFFFFFFF
    return v - (1 << 32) if v >= 1 << 31 else v


def trunc_div(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def clamp(v):
    return max(INT_MIN, min(INT_MAX, v))


def isqrt(n):
    if n < 2:
        return n
    x = 1 << ((n.bit_length() + 1) // 2)
    while True:
        y = (x + n // x) // 2
        if y >= x:
            return x
        x = y


# BOX2I ------------------------------------------------------------------------------------------

class Box:
    def __init__(self, pos=(0, 0), size=(0, 0), init=False):
        self.pos = list(pos)
        self.size = list(size)
        self.init = init

    def copy(self):
        return Box(self.pos, self.size, self.init)

    def compute(self, points):
        if not points:
            return
        lo = [min(p[0] for p in points), min(p[1] for p in points)]
        hi = [max(p[0] for p in points), max(p[1] for p in points)]
        self.pos = lo
        self.size = [I(hi[0] - lo[0]), I(hi[1] - lo[1])]
        self.init = True

    def inflate(self, d):
        for k in (0, 1):
            pos, size = self.pos[k], self.size[k]
            if size >= 0:
                if size < -2 * d:
                    self.pos[k], self.size[k] = to_int(pos + trunc_div(size, 2)), 0
                else:
                    lo, hi = clamp(pos - d), clamp(pos + size + d)
                    self.pos[k], self.size[k] = lo, hi - lo
            else:
                if size > 2 * d:
                    self.pos[k], self.size[k] = to_int(pos - trunc_div(size, 2)), 0
                else:
                    hi, lo = clamp(pos + d), clamp(pos + size - d)
                    self.pos[k], self.size[k] = hi, lo - hi

    def normalize(self):
        for k in (1, 0):
            if self.size[k] < 0:
                self.size[k] = -self.size[k]
                self.pos[k] = to_int(self.pos[k] - self.size[k])

    def right(self):
        return to_int(self.pos[0] + self.size[0])

    def bottom(self):
        return to_int(self.pos[1] + self.size[1])

    def merge(self, p):
        if not self.init:
            self.pos, self.size, self.init = list(p), [0, 0], True
            return
        self.normalize()
        end = [self.right(), self.bottom()]
        self.pos = [min(self.pos[0], p[0]), min(self.pos[1], p[1])]
        end = [max(end[0], p[0]), max(end[1], p[1])]
        self.size = [end[0] - self.pos[0], end[1] - self.pos[1]]

    def move(self, v):
        self.pos = [I(self.pos[0] + v[0]), I(self.pos[1] + v[1])]


def box_squared_distance(a, b):
    s = 0
    for k in (0, 1):
        p, z, bp, bz = a.pos[k], a.size[k], b.pos[k], b.size[k]
        if bp + bz < p:
            d = bp + bz - p
            s = L(s + L(d * d))
        elif bp > p + z:
            d = bp - z - p
            s = L(s + L(d * d))
    return s


def box_distance(a, b):
    """KiROUND( sqrt( double( s ) ) ), exact for s below 2^46."""
    s = box_squared_distance(a, b)
    assert s < 1 << 46
    n = isqrt(s)
    return n + 1 if s > n * n + n else n


# Shapes -----------------------------------------------------------------------------------------

class Chain:
    def __init__(self):
        self.points = []
        self.closed = False
        self.width = 0
        self.bbox = Box()

    def copy(self):
        c = Chain()
        c.points, c.closed, c.width, c.bbox = list(self.points), self.closed, self.width, self.bbox.copy()
        return c

    def point_count(self):
        return len(self.points)

    def segment_count(self):
        c = len(self.points) - 1
        if self.closed:
            c += 1
        return max(0, c)

    def append(self, p, dup=False):
        if not self.points:
            self.bbox = Box(p, (0, 0), True)
        if not self.points or dup or self.points[-1] != p:
            self.points.append(tuple(p))
            self.bbox.merge(p)

    def bbox_of(self, clearance=0):
        b = Box()
        b.compute(self.points)
        if clearance != 0 or self.width != 0:
            b.inflate(I(clearance + self.width))
        return b

    def cpoint(self, index):
        n = len(self.points)
        if index < 0:
            index += n
        elif index >= n:
            index -= n
        if not 0 <= index < n:
            raise Undefined()
        return self.points[index]

    def within(self):
        return abs(self.width) <= DOMAIN and all(abs(x) <= DOMAIN and abs(y) <= DOMAIN for x, y in self.points)


def circle_bbox(center, radius, clearance=0):
    r = I(radius + clearance)
    pos = (I(center[0] - r), I(center[1] - r))
    I(r * 2)
    return Box(pos, (2 * r, 2 * r), True).normalized()


def _normalized(self):
    self.normalize()
    return self


Box.normalized = _normalized


def spec_shape(spec):
    """The model of a shape specification: ('circle', center, radius) or ('chain', Chain)."""
    if spec is None:
        return None
    if spec['kind'] == 'circle':
        return ('circle', tuple(spec['center']), spec['radius'])
    c = Chain()
    for p in spec['points']:
        c.append(tuple(p))
    c.width = spec.get('width', 0)
    return ('chain', c)


def shape_bbox(shape):
    if shape[0] == 'circle':
        return circle_bbox(shape[1], shape[2])
    return shape[1].bbox_of(0)


def shape_within(shape):
    if shape[0] == 'circle':
        return abs(shape[1][0]) <= DOMAIN and abs(shape[1][1]) <= DOMAIN and 0 <= shape[2] <= DOMAIN
    return shape[1].within()


def tree_domain(b):
    for v in (b.pos[0], b.pos[1], b.right(), b.bottom()):
        if abs(v) > TREE_DOMAIN:
            raise Outside()


# Corpus -----------------------------------------------------------------------------------------

class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.names = set()
        self.deferred = 0

    def request(self, name, inp):
        assert name not in self.names, name
        self.names.add(name)
        return {'unit': UNIT, 'case': name, 'input': inp}

    def case(self, name, inp, deferred=False):
        self.deferred += deferred
        self.cases.append(self.request(name, inp))

    def control(self, name, inp, error, rust_unported=None):
        row = {'error': error, 'request': self.request(f'control-{name}', inp)}
        if rust_unported:
            row['rust_unported'] = rust_unported
        self.controls.append(row)

    def classify(self, name, inp, replay):
        """Run the model replay: a case, a deferred case or a control."""
        try:
            replay()
        except Deferred:
            return self.case(name, inp, True)
        except Undefined:
            return self.control(name, inp, UNDEFINED)
        except Outside:
            return self.control(name, inp, OUTSIDE)
        except OutsideAfter as e:
            return self.control(name, inp, OUTSIDE, e.owner)
        except UndefinedAfter as e:
            return self.control(name, inp, UNDEFINED, e.owner)
        except Drop:
            return None
        self.case(name, inp)


class UndefinedAfter(Exception):
    """Undefined only after a call the port defers."""

    def __init__(self, upstream, unit):
        super().__init__(upstream)
        self.owner = {'upstream': upstream, 'unit': unit}


class OutsideAfter(UndefinedAfter):
    """Outside the verified domain only after a call the port defers."""


def pt(rng, scale):
    return [rng.randint(-scale, scale), rng.randint(-scale, scale)]


SCALES = [4, 100, 5000, DOMAIN, 1 << 28, INT_MAX]
EXTREMES = [INT_MIN, INT_MIN + 1, -1, 0, 1, INT_MAX - 1, INT_MAX]


def rand_point(rng):
    r = rng.random()
    if r < 0.03:
        return [rng.choice(EXTREMES), rng.choice(EXTREMES)]
    return pt(rng, rng.choice(SCALES[:4]) if r < 0.93 else rng.choice(SCALES))


def rand_int(rng):
    r = rng.random()
    if r < 0.05:
        return rng.choice(EXTREMES)
    return rng.randint(-rng.choice(SCALES[:4]), rng.choice(SCALES[:4]))


# Line chain scripts ---------------------------------------------------------------------------------

INLINE_CALLS = ['append', 'append', 'append', 'append_xy', 'append_dup', 'set_width', 'is_closed',
                'width', 'segment_count', 'point_count', 'get_point_count', 'get_segment_count',
                'cpoint', 'get_point', 'clast_point', 'cpoints', 'cshapes', 'carcs', 'cached_bbox',
                'generate_bbox_cache', 'bbox', 'bbox_default', 'move', 'is_solid', 'arc_count',
                'type', 'clear', 'copy', 'assign', 'move_assign', 'ne', 'new', 'reserve_points']
DEFERRING_CALLS = ['set_closed', 'remove', 'csegment', 'get_segment', 'arc_index',
                   'reversed_arc_index', 'distance']
UNDEFINED_CALLS = ['arc', 'amend_arc_start', 'amend_arc_end']


def chain_step(rng, chains, call):
    """One random step and its model replay (a closure raising the classification)."""
    a = 0 if rng.random() < 0.6 else rng.randrange(4)
    c = chains[a]
    if call in ('cpoint', 'get_point', 'clast_point') and not c.points and rng.random() < 0.85:
        call = 'point_count'
    step = {'call': call, 'a': a}
    if call in ('append', 'append_dup'):
        step['call'] = 'append'
        p = rand_point(rng) if rng.random() < 0.8 or not c.points else list(c.points[-1])
        step['point'] = p
        dup = call == 'append_dup'
        if dup or rng.random() < 0.3:
            step['allow_duplication'] = dup
        return step, lambda: c.append(tuple(p), dup)
    if call == 'append_xy':
        p = rand_point(rng)
        step['x'], step['y'] = p
        return step, lambda: c.append(tuple(p))
    if call == 'set_width':
        step['width'] = rand_int(rng)
        return step, lambda: setattr(c, 'width', step['width'])
    if call in ('cpoint', 'get_point'):
        n = c.point_count()
        if n and rng.random() < 0.8:
            step['index'] = rng.randint(-n, 2 * n - 1)
        else:
            step['index'] = rng.choice([rng.randint(-2 * n - 1, 2 * n + 1), rand_int(rng)])
        return step, lambda: c.cpoint(step['index'])
    if call == 'clast_point':
        return step, lambda: c.cpoint(-1) if c.points else (_ for _ in ()).throw(Undefined())
    if call == 'generate_bbox_cache':
        def replay():
            c.bbox.compute(c.points)
            if c.width != 0:
                c.bbox.inflate(c.width)
        return step, replay
    if call in ('bbox', 'bbox_default'):
        if call == 'bbox':
            step['clearance'] = rand_int(rng)
        else:
            step['call'] = 'bbox'
        return step, lambda: c.bbox_of(step.get('clearance', 0))
    if call == 'move':
        v = rand_point(rng) if rng.random() < 0.7 else pt(rng, 10)
        step['vector'] = v

        def replay():
            for p in c.points:
                I(p[0] + v[0]), I(p[1] + v[1])
            c.bbox.move(v)
            c.points = [(p[0] + v[0], p[1] + v[1]) for p in c.points]
        return step, replay
    if call == 'clear':
        def replay():
            c.points, c.closed = [], False
        return step, replay
    if call in ('copy', 'assign', 'move_assign', 'ne'):
        src = rng.randrange(4)
        step['src'] = src

        def replay():
            if call in ('copy', 'assign'):
                chains[a] = chains[src].copy()
            elif call == 'move_assign' and src != a:
                moved = chains[src]
                chains[a] = moved.copy()
                moved.points = []
        return step, replay
    if call == 'new':
        def replay():
            chains[a] = Chain()
        return step, replay
    if call == 'reserve_points':
        size = rng.choice([0, 1, 7, 1000, RESERVE_DOMAIN, RESERVE_DOMAIN + 1, POINTS_MAX_SIZE,
                           POINTS_MAX_SIZE + 1, 1 << 62, 2**64 - 1])
        step['size'] = size

        def replay():
            if RESERVE_DOMAIN < size <= POINTS_MAX_SIZE:
                raise Outside()
        return step, replay
    if call == 'set_closed':
        step['closed'] = rng.random() < 0.7
        return step, lambda: (setattr(c, 'closed', step['closed']),
                              (_ for _ in ()).throw(Deferred('mergeFirstLastPointIfNeeded', P001)))
    if call == 'remove':
        n = c.point_count()
        step['index'] = rng.randint(-n - 2, n + 1)

        def replay():
            if step['index'] < -n:
                raise Outside()
            raise Deferred('Remove', P001)
        return step, replay
    if call in ('csegment', 'get_segment'):
        n = c.segment_count()
        step['index'] = rng.randint(-n - 1, n)

        def replay():
            idx = step['index'] + n if step['index'] < 0 else step['index']
            if not 0 <= idx < n:
                raise Outside()
            raise Deferred('Segment', P001)
        return step, replay
    if call in ('arc_index', 'reversed_arc_index'):
        n = c.point_count()
        step['segment'] = rng.choice([0, max(0, n - 1), n, n + 3, 2**64 - 1])

        def replay():
            if step['segment'] >= n:
                raise UndefinedAfter('SHAPE_LINE_CHAIN::IsSharedPt', P002)
            raise Deferred('IsSharedPt', P002)
        return step, replay
    if call == 'distance':
        step['point'] = pt(rng, rng.choice([100, DOMAIN, DOMAIN * 4]))
        step['outline_only'] = rng.random() < 0.5

        def replay():
            if not c.within() or abs(step['point'][0]) > DOMAIN or abs(step['point'][1]) > DOMAIN:
                raise Outside()
            if c.segment_count() == 0:
                # SquaredDistance returns ECOORD_MAX; its square root does not fit an int.
                raise UndefinedAfter('SHAPE_LINE_CHAIN_BASE::SquaredDistance', P001)
            raise Deferred('SquaredDistance', P001)
        return step, replay
    if call in UNDEFINED_CALLS:
        step['index'] = rng.choice([0, 1, 2**64 - 1])
        if call != 'arc':
            step['point'] = rand_point(rng)
        return step, lambda: (_ for _ in ()).throw(Undefined())
    return step, lambda: None


def gen_chains(corpus, rng):
    for n in range(2400):
        chains = [Chain() for _ in range(4)]
        steps = []
        length = rng.choice([1, 2, 4, 8, 12, 20])
        outcome = None
        for _ in range(length):
            r = rng.random()
            if r < 0.01:
                call = rng.choice(UNDEFINED_CALLS)
            elif r < 0.05:
                call = rng.choice(DEFERRING_CALLS)
            else:
                call = rng.choice(INLINE_CALLS)
            step, replay = chain_step(rng, chains, call)
            steps.append(step)
            try:
                replay()
            except Exception as e:  # noqa: BLE001 - classification
                outcome = e
                break
        inp = {'op': 'chain', 'steps': steps}

        def replay_outcome(outcome=outcome):
            if outcome:
                raise outcome
        corpus.classify(f'chain-{n}', inp, replay_outcome)


def gen_chain_edges(corpus):
    """Deterministic boundary scripts."""
    def script(name, steps, outcome=None):
        def replay():
            if outcome:
                raise outcome
        corpus.classify(f'chain-edge-{name}', {'op': 'chain', 'steps': steps}, replay)

    ap = lambda a, p, **k: {'call': 'append', 'a': a, 'point': p, **k}
    script('empty-state', [{'call': c, 'a': 0} for c in
                           ('is_closed', 'width', 'segment_count', 'point_count', 'get_point_count',
                            'get_segment_count', 'cpoints', 'cshapes', 'carcs', 'cached_bbox',
                            'is_solid', 'arc_count', 'type')] + [{'call': 'bbox', 'a': 0}])
    script('empty-cache', [{'call': 'set_width', 'a': 0, 'width': 5}, {'call': 'generate_bbox_cache', 'a': 0},
                           {'call': 'cached_bbox', 'a': 0}])
    script('empty-bbox-inflate', [{'call': 'bbox', 'a': 0, 'clearance': 7}, {'call': 'bbox', 'a': 0, 'clearance': -7}])
    script('span-int-max', [ap(0, [INT_MIN, 0]), ap(0, [-1, 0]), {'call': 'bbox', 'a': 0}])
    script('span-overflow', [ap(0, [INT_MIN, 0]), ap(0, [0, 0]), {'call': 'bbox', 'a': 0}], Undefined())
    script('cache-span-overflow', [ap(0, [INT_MAX, 0]), ap(0, [-1, 0]), {'call': 'generate_bbox_cache', 'a': 0}],
           Undefined())
    script('cached-after-wide-append', [ap(0, [INT_MAX, INT_MIN]), ap(0, [INT_MIN, INT_MAX]), {'call': 'cached_bbox', 'a': 0}])
    script('clearance-plus-width', [ap(0, [0, 0]), {'call': 'set_width', 'a': 0, 'width': INT_MAX},
                                    {'call': 'bbox', 'a': 0, 'clearance': 1}], Undefined())
    script('clearance-minus-width', [ap(0, [0, 0]), {'call': 'set_width', 'a': 0, 'width': INT_MAX},
                                     {'call': 'bbox', 'a': 0, 'clearance': -1}])
    script('deflate-to-zero', [ap(0, [0, 0]), ap(0, [10, 4]), {'call': 'bbox', 'a': 0, 'clearance': -100}])
    script('inflate-saturates', [ap(0, [INT_MAX - 2, INT_MIN + 2]), {'call': 'bbox', 'a': 0, 'clearance': 1000}])
    script('move-overflow', [ap(0, [INT_MAX, 0]), {'call': 'move', 'a': 0, 'vector': [1, 0]}], Undefined())
    script('move-empty-box', [{'call': 'move', 'a': 0, 'vector': [INT_MAX, INT_MIN]}, {'call': 'cached_bbox', 'a': 0}])
    script('move-box-overflow', [{'call': 'move', 'a': 0, 'vector': [INT_MAX, 0]},
                                 {'call': 'move', 'a': 0, 'vector': [1, 0]}], Undefined())
    # The cache inflated to the int limit, then moved: GetRight wraps and Merge's SetEnd stores a
    # negative size, which the next Merge normalises.
    script('move-wraps-right', [ap(0, [0, 0]), ap(0, [INT_MAX - 10, 0]), {'call': 'set_width', 'a': 0, 'width': 100},
                                {'call': 'generate_bbox_cache', 'a': 0}, {'call': 'move', 'a': 0, 'vector': [5, 0]},
                                {'call': 'cached_bbox', 'a': 0}, ap(0, [5, 5]), {'call': 'cached_bbox', 'a': 0},
                                ap(0, [6, -7]), {'call': 'cached_bbox', 'a': 0}])
    script('cpoint-wraps', [ap(0, [1, 1]), ap(0, [2, 2]), ap(0, [3, 3])] +
           [{'call': 'cpoint', 'a': 0, 'index': i} for i in (-3, -1, 0, 2, 3, 5)])
    script('cpoint-low', [ap(0, [1, 1]), {'call': 'cpoint', 'a': 0, 'index': -2}], Undefined())
    script('cpoint-high', [ap(0, [1, 1]), {'call': 'get_point', 'a': 0, 'index': 2}], Undefined())
    script('cpoint-int-min', [ap(0, [1, 1]), {'call': 'cpoint', 'a': 0, 'index': INT_MIN}], Undefined())
    script('cpoint-int-max', [ap(0, [1, 1]), {'call': 'cpoint', 'a': 0, 'index': INT_MAX}], Undefined())
    script('clast-empty', [{'call': 'clast_point', 'a': 0}], Undefined())
    script('clear-keeps-width-and-box', [ap(0, [3, 4]), {'call': 'set_width', 'a': 0, 'width': 9},
                                         {'call': 'clear', 'a': 0}, {'call': 'width', 'a': 0},
                                         {'call': 'cached_bbox', 'a': 0}, ap(0, [-7, 2]), {'call': 'cached_bbox', 'a': 0}])
    script('duplicates', [ap(0, [1, 1]), ap(0, [1, 1]), ap(0, [1, 1], allow_duplication=True),
                          ap(0, [1, 1], allow_duplication=False), {'call': 'append_xy', 'a': 0, 'x': 1, 'y': 1,
                                                                    'allow_duplication': True},
                          {'call': 'cpoints', 'a': 0}, {'call': 'segment_count', 'a': 0}])
    script('move-assign-leaves-empty', [ap(1, [5, 6]), ap(1, [7, 8]), {'call': 'set_width', 'a': 1, 'width': 3},
                                        {'call': 'move_assign', 'a': 0, 'src': 1}])
    script('self-assign', [ap(0, [5, 6]), {'call': 'assign', 'a': 0, 'src': 0},
                           {'call': 'move_assign', 'a': 0, 'src': 0}, {'call': 'copy', 'a': 0, 'src': 0}])
    script('ne', [ap(0, [1, 1]), ap(1, [1, 1]), {'call': 'ne', 'a': 0, 'src': 1}, ap(1, [2, 2]),
                  {'call': 'ne', 'a': 0, 'src': 1}, ap(0, [3, 3]), {'call': 'ne', 'a': 0, 'src': 1}])
    for size in (0, RESERVE_DOMAIN, POINTS_MAX_SIZE + 1, 2**64 - 1):
        script(f'reserve-{size}', [{'call': 'reserve_points', 'a': 0, 'size': size}])
    for size in (RESERVE_DOMAIN + 1, POINTS_MAX_SIZE):
        script(f'reserve-{size}', [{'call': 'reserve_points', 'a': 0, 'size': size}], Outside())
    script('closed-one', [ap(0, [1, 1]), {'call': 'set_closed', 'a': 0, 'closed': True}],
           Deferred('merge', P001))
    script('distance-empty', [{'call': 'distance', 'a': 0, 'point': [0, 0], 'outline_only': True}],
           UndefinedAfter('SHAPE_LINE_CHAIN_BASE::SquaredDistance', P001))
    script('distance-one', [ap(0, [3, 4]), {'call': 'distance', 'a': 0, 'point': [0, 0], 'outline_only': False}],
           UndefinedAfter('SHAPE_LINE_CHAIN_BASE::SquaredDistance', P001))
    script('distance-two', [ap(0, [3, 4]), ap(0, [5, 4]), {'call': 'distance', 'a': 0, 'point': [0, 0],
                                                            'outline_only': False}], Deferred('d', P001))
    script('arc-index-empty', [{'call': 'arc_index', 'a': 0, 'segment': 0}],
           UndefinedAfter('SHAPE_LINE_CHAIN::IsSharedPt', P002))
    script('arc-empty', [{'call': 'arc', 'a': 0, 'index': 0}], Undefined())


# CLIPPER_Z_VALUE, INTERSECTION, compareOriginDistance --------------------------------------------------

def gen_misc(corpus, rng):
    corpus.case('clipper_z_value-default', {'op': 'clipper_z_value'})
    corpus.case('intersection', {'op': 'intersection'})
    values = [I64_MIN, I64_MIN + 1, -2, -1, 0, 1, 2, 1000, I64_MAX - 1, I64_MAX]
    n = 0
    for first in values:
        for second in values:
            for offset in (None, 0, 1, -1, I64_MAX, I64_MIN, 12345):
                inp = {'op': 'clipper_z_value', 'indices': [first, second]}
                if offset is not None:
                    inp['offset'] = offset

                def replay(first=first, second=second, offset=offset):
                    for v in (first, second):
                        if v >= 0:
                            L(v + (offset or 0))
                corpus.classify(f'clipper_z_value-{n}', inp, replay)
                n += 1
    for k in range(400):
        origin, a, b = rand_point(rng), rand_point(rng), rand_point(rng)
        if k % 5 == 0:
            b = list(a)

        def replay(origin=origin, a=a, b=b):
            for p in (a, b):
                I(origin[0] - p[0]), I(origin[1] - p[1])
        corpus.classify(f'compare_origin_distance-{k}',
                        {'op': 'compare_origin_distance', 'origin': origin, 'a': a, 'b': b}, replay)


# Items --------------------------------------------------------------------------------------------------

def rand_shape(rng, small=True):
    scale = 1000 if small else rng.choice([DOMAIN, 1 << 28, INT_MAX])
    if rng.random() < 0.6:
        return {'kind': 'circle', 'center': pt(rng, scale), 'radius': rng.randint(0, max(1, scale // 10))}
    spec = {'kind': 'chain', 'points': [pt(rng, scale) for _ in range(rng.randint(1, 5))]}
    if rng.random() < 0.4:
        spec['width'] = rng.randint(0, 50)
    return spec


def rand_items(rng, count, small=True):
    items = []
    for _ in range(count):
        r = rng.random()
        shape = None if r < 0.1 else rand_shape(rng, small)
        layers = rng.choice([[-1, 0], [0], [-1], [1, 0], []])
        items.append({'shape': shape, 'layers': layers})
    return items


def item_shape(items, ref, layer):
    if ref is None:
        return None
    item = items[ref]
    if layer not in item['layers']:
        return None
    return spec_shape(item['shape'])


def item_box(items, ref, layer):
    shape = item_shape(items, ref, layer)
    return Box() if shape is None else shape_bbox(shape)


def rand_ref(rng, items):
    return None if rng.random() < 0.1 or not items else rng.randrange(len(items))


# SHAPE_INDEX scripts ---------------------------------------------------------------------------------------

INDEX_CALLS = ['add', 'add', 'add', 'add', 'add_bbox', 'remove', 'remove_all', 'size', 'range', 'accept',
               'query', 'query', 'iterate', 'clone', 'destroy', 'new', 'bulk_load', 'reindex',
               'shape_functor', 'bounding_box', 'collide', 'query_callback']


def gen_index(corpus, rng):
    for n in range(700):
        clean = rng.random() < 0.85
        items = rand_items(rng, rng.randint(1, 10), clean)
        layers = [None, None, None]
        steps = [{'call': 'new', 'a': 0, 'layer': rng.choice([0, -1, 1])}]
        layers[0] = steps[0]['layer']
        outcome = None
        for _ in range(rng.choice([2, 5, 10, 20])):
            call = rng.choice(INDEX_CALLS)
            a = rng.randrange(3)
            step = {'call': call}
            try:
                if call in ('shape_functor', 'bounding_box'):
                    step['item'] = rand_ref(rng, items)
                    step['layer'] = rng.choice([0, -1, 1, 2])
                    if call == 'bounding_box':
                        item_box(items, step['item'], step['layer'])
                elif call == 'collide':
                    step['item'] = rand_ref(rng, items)
                    step['layer'] = rng.choice([0, -1, 1])
                    step['min_distance'] = rng.choice([0, 1, 5, 100, -3, DOMAIN, DOMAIN + 1])
                    kind = rng.choice(['seg', 'seg', 'point', 'shape'])
                    if kind == 'seg':
                        step['target'] = {'seg': [pt(rng, 1200), pt(rng, 1200)]}
                    elif kind == 'point':
                        step['target'] = {'point': pt(rng, 1200)}
                    else:
                        step['target'] = {'shape': rand_shape(rng)}
                    shape = item_shape(items, step['item'], step['layer'])
                    if shape is not None:
                        if kind == 'shape':
                            other = spec_shape(step['target']['shape'])
                            if (shape[0] != 'circle' or other[0] != 'circle'
                                    or not 0 <= step['min_distance'] <= DOMAIN
                                    or not shape_within(shape) or not shape_within(other)):
                                raise Outside()
                            raise Deferred('SHAPE::Collide', COLLISIONS)
                        if not shape_within(shape) or abs(step['min_distance']) > DOMAIN:
                            raise Outside()
                        if shape[0] == 'chain':
                            raise Deferred('SHAPE_LINE_CHAIN::Collide', P001)
                elif call == 'query_callback':
                    step['item'] = rand_ref(rng, items)
                else:
                    step['a'] = a
                    if call == 'new':
                        step['layer'] = rng.choice([0, -1, 1])
                        layers[a] = step['layer']
                    elif layers[a] is None:
                        continue
                    elif call in ('add', 'remove'):
                        step['item'] = rand_ref(rng, items)
                        b = item_box(items, step['item'], layers[a])
                        if call == 'add':
                            tree_domain(b)
                    elif call == 'add_bbox':
                        step['item'] = rand_ref(rng, items)
                        step['pos'] = pt(rng, rng.choice([1000, TREE_DOMAIN]))
                        step['size'] = pt(rng, rng.choice([100, 1000]))
                        b = Box(step['pos'], step['size'], True).normalized()
                        tree_domain(b)
                    elif call == 'query':
                        step['shape'] = rand_shape(rng, clean or rng.random() < 0.5)
                        step['min_distance'] = rng.choice([0, 1, 10, 500, -5, 100000])
                        step['stop'] = rng.choice([1, 2, 3, 100])
                        shape_bbox(spec_shape(step['shape']))
                    elif call == 'iterate':
                        step['ops'] = [rng.choice(['deref', 'inc', 'inc_post', 'is_null', 'is_not_null', 'next'])
                                       for _ in range(rng.randint(1, 12))]
                    elif call == 'clone':
                        src = rng.randrange(3)
                        if layers[src] is None:
                            continue
                        step['src'] = src
                        layers[a] = layers[src]
                    elif call == 'destroy':
                        layers[a] = None
                    elif call == 'bulk_load':
                        step['entries'] = [{'item': rand_ref(rng, items), 'pos': pt(rng, 1000),
                                            'size': pt(rng, 200)} for _ in range(rng.randint(0, 40))]
                    elif call == 'reindex':
                        if not clean:
                            continue
                        # Every item an index of this layer may hold, re-added under its own box.
                        for ref in [None] + list(range(len(items))):
                            tree_domain(item_box(items, ref, layers[a]))
                steps.append(step)
            except Exception as e:  # noqa: BLE001 - classification
                steps.append(step)
                outcome = e
                break
        inp = {'op': 'index', 'items': items, 'steps': steps}

        def replay(outcome=outcome):
            if outcome:
                raise outcome
        corpus.classify(f'index-{n}', inp, replay)


def gen_index_edges(corpus):
    def script(name, items, steps, outcome=None):
        def replay():
            if outcome:
                raise outcome
        corpus.classify(f'index-edge-{name}', {'op': 'index', 'items': items, 'steps': steps}, replay)

    circle = lambda c, r: {'kind': 'circle', 'center': c, 'radius': r}
    items = [{'shape': circle([0, 0], 5), 'layers': [-1, 0]}, {'shape': circle([100, 0], 5), 'layers': [0]},
             {'shape': None, 'layers': [0]}, {'shape': circle([50, 50], 1), 'layers': [1]}]
    script('asserts', items, [{'call': 'new', 'a': 0, 'layer': 0}, {'call': 'add', 'a': 0, 'item': None},
                              {'call': 'add', 'a': 0, 'item': 2}, {'call': 'add', 'a': 0, 'item': 3},
                              {'call': 'size', 'a': 0}, {'call': 'remove', 'a': 0, 'item': None},
                              {'call': 'range', 'a': 0}, {'call': 'reindex', 'a': 0}, {'call': 'accept', 'a': 0}])
    script('empty-iterate', items, [{'call': 'new', 'a': 0, 'layer': 0},
                                    {'call': 'iterate', 'a': 0, 'ops': ['deref', 'is_null', 'inc', 'next', 'deref']}])
    many = [{'shape': circle([i * 7 % 300, i * 13 % 300], i % 4), 'layers': [0]} for i in range(60)]
    script('split', many, [{'call': 'new', 'a': 0, 'layer': 0}] + [{'call': 'add', 'a': 0, 'item': i} for i in range(60)]
           + [{'call': 'size', 'a': 0}, {'call': 'range', 'a': 0},
              {'call': 'query', 'a': 0, 'shape': circle([150, 150], 10), 'min_distance': 20, 'stop': 1000},
              {'call': 'query', 'a': 0, 'shape': circle([150, 150], 10), 'min_distance': 200, 'stop': 3},
              {'call': 'clone', 'a': 1, 'src': 0}] + [{'call': 'remove', 'a': 0, 'item': i} for i in range(0, 60, 2)]
           + [{'call': 'size', 'a': 0}, {'call': 'size', 'a': 1}, {'call': 'range', 'a': 0}, {'call': 'range', 'a': 1},
              {'call': 'reindex', 'a': 1}, {'call': 'iterate', 'a': 1, 'ops': ['next'] * 64 + ['deref', 'is_null']}])
    script('bulk', many, [{'call': 'new', 'a': 0, 'layer': 0},
                          {'call': 'bulk_load', 'a': 0, 'entries': [{'item': i, 'pos': [i, -i], 'size': [3, 3]}
                                                                    for i in range(60)]},
                          {'call': 'range', 'a': 0}, {'call': 'bulk_load', 'a': 0, 'entries': []}, {'call': 'size', 'a': 0}])
    big = [{'shape': circle([TREE_DOMAIN, 0], 5), 'layers': [0]}, {'shape': circle([INT_MIN, 0], 1), 'layers': [0]},
           {'shape': circle([INT_MAX, 0], 1), 'layers': [0]}]
    script('tree-domain', big, [{'call': 'new', 'a': 0, 'layer': 0}, {'call': 'add', 'a': 0, 'item': 0}], Outside())
    script('bbox-overflow', big, [{'call': 'new', 'a': 0, 'layer': 0}, {'call': 'add', 'a': 0, 'item': 1}], Undefined())
    script('functor-overflow', big, [{'call': 'bounding_box', 'item': 1, 'layer': 0}], Undefined())
    script('remove-big', big, [{'call': 'new', 'a': 0, 'layer': 0}, {'call': 'remove', 'a': 0, 'item': 0},
                               {'call': 'bounding_box', 'item': 2, 'layer': 0}])
    script('collide-no-shape', items, [{'call': 'collide', 'item': 3, 'layer': 0, 'min_distance': 0,
                                        'target': {'shape': circle([0, 0], 1)}},
                                       {'call': 'collide', 'item': None, 'layer': 0, 'min_distance': 0,
                                        'target': {'point': [0, 0]}}])
    script('collide-circle-seg', items, [{'call': 'collide', 'item': 0, 'layer': 0, 'min_distance': 5,
                                          'target': {'seg': [[0, 9], [1, 9]]}},
                                         {'call': 'collide', 'item': 0, 'layer': 0, 'min_distance': 2,
                                          'target': {'seg': [[0, 9], [1, 9]]}},
                                         {'call': 'collide', 'item': 0, 'layer': -1, 'min_distance': 0,
                                          'target': {'point': [3, 4]}}])
    script('collide-shape', items, [{'call': 'collide', 'item': 0, 'layer': 0, 'min_distance': 0,
                                     'target': {'shape': circle([9, 0], 5)}}], Deferred('c', COLLISIONS))


# SHAPE_INDEX_LIST scripts --------------------------------------------------------------------------------------

def gen_list(corpus, rng):
    for n in range(1400):
        items = rand_items(rng, rng.randint(1, 10))
        for item in items:
            # The list's functor asks for layer -1.
            if rng.random() < 0.8 and -1 not in item['layers']:
                item['layers'] = item['layers'] + [-1]
        entries = []
        steps = []
        outcome = None
        for _ in range(rng.choice([2, 5, 10, 20])):
            call = rng.choice(['add', 'add', 'add', 'remove', 'size', 'clear', 'query', 'query', 'iterate',
                               'qiterate', 'qiterate'])
            step = {'call': call}
            steps.append(step)
            try:
                if call in ('add', 'remove'):
                    valid = [i for i in range(len(items)) if item_shape(items, i, -1) is not None]
                    ref = rng.choice(valid) if valid and rng.random() < 0.92 else rand_ref(rng, items)
                    step['item'] = ref
                    if call == 'remove':
                        for i, e in enumerate(entries):
                            if e[0] == ref:
                                del entries[i]
                                break
                    else:
                        shape = item_shape(items, ref, -1)
                        if shape is None:
                            raise Undefined()
                        entries.append((ref, shape, shape_bbox(shape)))
                elif call == 'clear':
                    entries = []
                elif call == 'query':
                    shape = rand_shape(rng)
                    step['shape'] = shape
                    step['min_distance'] = rng.choice([0, 1, 10, 100, 500, 3000])
                    step['stop'] = rng.choice([1, 2, 100])
                    exact = rng.random() < 0.3
                    if exact or rng.random() < 0.5:
                        step['exact'] = exact
                    exact = step.get('exact', True)
                    model = spec_shape(shape)
                    ref_box = shape_bbox(model)
                    m2 = step['min_distance'] ** 2
                    calls = 0
                    passing = [e for e in entries if box_squared_distance(ref_box, e[2]) <= m2]
                    if exact and passing:
                        # The oracle tests every passing entry until the visitor stops, which
                        # depends on collision results: refuse at the first, else drop when a
                        # later one is outside the domain.
                        domain = [e[1][0] == 'circle' and model[0] == 'circle' and shape_within(e[1])
                                  and shape_within(model) for e in passing]
                        if not domain[0]:
                            raise Outside()
                        if not all(domain):
                            raise Drop()
                        raise Deferred('SHAPE::Collide', COLLISIONS)
                    for e in passing:
                        calls += 1
                        if calls >= step['stop']:
                            break
                elif call == 'iterate':
                    ops = [rng.choice(['deref', 'inc', 'inc_post', 'eq_end', 'ne_end', 'eq_null', 'copy'])
                           for _ in range(rng.randint(1, 2 * len(entries) + 2))]
                    step['ops'] = ops
                    pos = 0
                    for k, op in enumerate(ops):
                        if op in ('deref', 'inc', 'inc_post') and pos >= len(entries) and rng.random() < 0.9:
                            ops[k] = op = 'eq_end'
                        if op in ('deref', 'inc', 'inc_post'):
                            if pos >= len(entries):
                                raise Undefined()
                            if op != 'deref':
                                pos += 1
                elif call == 'qiterate':
                    shape = None if rng.random() < 0.15 else rand_shape(rng)
                    step['shape'] = shape
                    step['min_distance'] = rng.choice([0, 10, 100, 1000, 3000])
                    step['exact'] = rng.random() < 0.25
                    ops = [rng.choice(['deref', 'inc', 'inc_post', 'eq_end', 'ne_end', 'copy'])
                           for _ in range(rng.randint(1, 4))]
                    if step['exact'] and rng.random() < 0.6:
                        ops = [rng.choice(['eq_end', 'ne_end']) for _ in ops]
                    if rng.random() < 0.05:
                        ops.append('copy_null')
                    step['ops'] = ops
                    model = spec_shape(shape)

                    def in_domain(e):
                        return (model is not None and e[1][0] == 'circle' and model[0] == 'circle'
                                and shape_within(e[1]) and shape_within(model)
                                and 0 <= step['min_distance'] <= DOMAIN)

                    def next_pos(pos):
                        while pos < len(entries):
                            e = entries[pos]
                            if box_distance(ref_box, e[2]) <= step['min_distance']:
                                if not step['exact']:
                                    return pos
                                if not in_domain(e):
                                    raise Outside()
                                # The oracle continues from collision results the model does not
                                # know: every later passing entry must be inside the domain and
                                # the remaining operations must be defined at any position.
                                later = [f for f in entries[pos + 1:]
                                         if box_distance(ref_box, f[2]) <= step['min_distance']]
                                if not all(in_domain(f) for f in later):
                                    raise Drop()
                                if any(op not in ('eq_end', 'ne_end') for op in step['ops']):
                                    raise Drop()
                                raise Deferred('SHAPE::Collide', COLLISIONS)
                            pos += 1
                        return pos
                    if model is None:
                        ref_box = Box()
                        pos = 0
                    else:
                        ref_box = shape_bbox(model)
                        pos = next_pos(0)
                    for k, op in enumerate(ops):
                        if op in ('deref', 'inc', 'inc_post') and pos >= len(entries) and rng.random() < 0.9:
                            ops[k] = op = 'ne_end'
                        if op == 'copy_null':
                            raise Undefined()
                        if op in ('deref', 'inc', 'inc_post'):
                            if pos >= len(entries):
                                raise Undefined()
                            if op != 'deref':
                                pos = next_pos(pos + 1)
            except Exception as e:  # noqa: BLE001 - classification
                outcome = e
                break
        inp = {'op': 'index_list', 'items': items, 'steps': steps}

        def replay(outcome=outcome):
            if outcome:
                raise outcome
        corpus.classify(f'list-{n}', inp, replay)


def gen_list_edges(corpus):
    def script(name, items, steps, outcome=None):
        def replay():
            if outcome:
                raise outcome
        corpus.classify(f'list-edge-{name}', {'op': 'index_list', 'items': items, 'steps': steps}, replay)

    circle = lambda c, r: {'kind': 'circle', 'center': c, 'radius': r}
    items = [{'shape': circle([0, 0], 5), 'layers': [-1]},
             {'shape': {'kind': 'chain', 'points': [[8, 0], [9, 1]]}, 'layers': [-1]},
             {'shape': circle([100, 0], 5), 'layers': [0]}, {'shape': None, 'layers': [-1]}]
    script('null-iterators', items, [{'call': 'iterate', 'ops': ['eq_end', 'eq_null', 'ne_end']},
                                     {'call': 'qiterate', 'shape': None, 'min_distance': 0, 'exact': False,
                                      'ops': ['eq_end']},
                                     {'call': 'add', 'item': 0}, {'call': 'clear'},
                                     {'call': 'iterate', 'ops': ['eq_end', 'eq_null']}])
    script('deref-unallocated', items, [{'call': 'iterate', 'ops': ['deref']}], Undefined())
    script('inc-end', items, [{'call': 'add', 'item': 0}, {'call': 'iterate', 'ops': ['inc', 'inc']}], Undefined())
    script('add-null', items, [{'call': 'add', 'item': None}], Undefined())
    script('add-no-shape', items, [{'call': 'add', 'item': 2}], Undefined())
    script('add-null-shape', items, [{'call': 'add', 'item': 3}], Undefined())
    script('copy-null', items, [{'call': 'qiterate', 'shape': None, 'min_distance': 0, 'exact': False,
                                 'ops': ['copy_null']}], Undefined())
    script('query', items, [{'call': 'add', 'item': 0}, {'call': 'add', 'item': 1}, {'call': 'add', 'item': 0},
                            {'call': 'query', 'shape': circle([0, 0], 1), 'min_distance': 6, 'stop': 100, 'exact': False},
                            {'call': 'query', 'shape': circle([0, 0], 1), 'min_distance': 7, 'stop': 2, 'exact': False},
                            {'call': 'query', 'shape': circle([500, 0], 1), 'min_distance': 6, 'stop': 100},
                            {'call': 'qiterate', 'shape': circle([0, 0], 1), 'min_distance': 7, 'exact': False,
                             'ops': ['deref', 'inc', 'deref', 'copy', 'inc_post', 'deref', 'inc', 'eq_end']},
                            {'call': 'remove', 'item': 0}, {'call': 'size'},
                            {'call': 'iterate', 'ops': ['deref', 'inc', 'deref', 'inc', 'eq_end']}])
    script('exact-chain', items, [{'call': 'add', 'item': 1},
                                  {'call': 'query', 'shape': circle([0, 0], 1), 'min_distance': 7, 'stop': 100}],
           Outside())
    script('exact-circle', items, [{'call': 'add', 'item': 0},
                                   {'call': 'query', 'shape': circle([0, 0], 1), 'min_distance': 7, 'stop': 100}],
           Deferred('c', COLLISIONS))
    huge = [{'shape': circle([INT_MAX - 10, INT_MAX - 10], 10), 'layers': [-1]},
            {'shape': circle([INT_MIN + 10, INT_MIN + 10], 10), 'layers': [-1]}]
    script('squared-distance-overflow', huge,
           [{'call': 'add', 'item': 0}, {'call': 'query', 'shape': circle([INT_MIN + 10, INT_MIN + 10], 10),
                                         'min_distance': 0, 'stop': 1, 'exact': False}], Undefined())


def gen_malformed(corpus):
    def bad(name, inp, message):
        corpus.control(f'malformed-{name}', inp, {'kind': 'invalid_input', 'message': message})

    bad('op', {'op': 'nope'}, 'unknown operation')
    bad('op-type', {'op': 1}, 'expected string: op')
    bad('extra', {'op': 'intersection', 'x': 1}, 'unexpected or missing field')
    bad('offset-alone', {'op': 'clipper_z_value', 'offset': 1}, 'unexpected or missing field')
    bad('indices', {'op': 'clipper_z_value', 'indices': [1]}, 'expected indices')
    bad('point', {'op': 'compare_origin_distance', 'origin': [1], 'a': [0, 0], 'b': [0, 0]}, 'expected point')
    bad('int', {'op': 'compare_origin_distance', 'origin': [2**31, 0], 'a': [0, 0], 'b': [0, 0]},
        'integer out of range')
    bad('slot', {'op': 'chain', 'steps': [{'call': 'width', 'a': 4}]}, 'invalid slot')
    bad('call', {'op': 'chain', 'steps': [{'call': 'nope', 'a': 0}]}, 'unknown call')
    bad('steps', {'op': 'chain', 'steps': 3}, 'expected array: steps')
    bad('size', {'op': 'chain', 'steps': [{'call': 'reserve_points', 'a': 0, 'size': -1}]}, 'integer out of range')
    bad('shape', {'op': 'index_list', 'items': [{'shape': {'kind': 'square'}, 'layers': []}], 'steps': []},
        'unknown shape')
    bad('item', {'op': 'index_list', 'items': [], 'steps': [{'call': 'add', 'item': 0}]}, 'invalid item')
    bad('index-missing', {'op': 'index', 'items': [], 'steps': [{'call': 'size', 'a': 0}]}, 'missing index')
    bad('target', {'op': 'index', 'items': [], 'steps': [{'call': 'collide', 'item': None, 'layer': 0,
                                                          'min_distance': 0, 'target': {}}]}, 'expected one target')
    bad('iter-op', {'op': 'index_list', 'items': [], 'steps': [{'call': 'iterate', 'ops': ['nope']}]}, 'unknown op')


def generate():
    c = Corpus()
    rng = random.Random(SEED)
    gen_misc(c, rng)
    gen_chain_edges(c)
    gen_chains(c, rng)
    gen_index_edges(c)
    gen_index(c, rng)
    gen_list_edges(c)
    gen_list(c, rng)
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
    print(f'{len(c.cases)} public cases ({c.deferred} expected to defer), {len(c.controls)} controls')


if __name__ == '__main__':
    main()
