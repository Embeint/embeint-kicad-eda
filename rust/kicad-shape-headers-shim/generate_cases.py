#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus and controls for the seg.h / shape header unit; inputs only.

Unit libs.kimath.include.geometry.application.ui.p003. Python integers are exact, so the mirror
below replays the pinned integer arithmetic of every owned inline body (and, through the seg and
circle generators' mirrors, of the accepted seg.cpp and circle.cpp bodies they call) in
evaluation order. It only classifies a candidate; it never writes a result:

* defined: no undefined behaviour and inside every verified domain. The request enters the
  public corpus. The mirror records the first call the Rust port defers, so the expected
  deferral count is known.
* undefined: signed overflow or an out-of-range floating-to-integer conversion. The request is
  a control that the oracle refuses ("undefined p003 operation"). When a Rust deferral comes
  first, the control carries "rust_unported" with that owner.
* outside a verified domain: a call into an out-of-line body that this unit does not port and
  whose undefined domain the adapter does not replay (SHAPE_ARC's three-point constructor,
  shape_collisions.cpp, the out-of-line SHAPE_COMPOUND and SHAPE_ARC members). Both programs
  refuse such a request with "outside the verified domain"; it is a control too.

--check verifies the committed files equal the generator output.
"""
import argparse
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.geometry.application.ui.p003'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'shape-headers-controls.jsonl'
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined p003 operation'}
OUTSIDE = {'kind': 'invalid_input', 'message': 'outside the verified domain'}
SEED = 0x5EA9E003
INT_MIN, INT_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1
# The verified domain of the out-of-line calls: every coordinate, radius, width and clearance
# within this bound (radii and clearances also non-negative).
DOMAIN = 2**20

ARC_UNIT = 'libs.kimath.src.geometry.shape_arc.application.ui.p001'
COMPOUND_UNIT = 'libs.kimath.src.geometry.shape_compound.application.ui.p001'
ELLIPSE_UNIT = 'libs.kimath.src.geometry.shape_ellipse.application.ui.p001'
COLLISIONS_UNIT = 'libs.kimath.src.geometry.shape_collisions.application.ui.p001'
UTIL_UNIT = 'libs.kimath.src.math.util.application.ui.p001'
DEFERRALS = {
    'arc_ctor': ('SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const VECTOR2I&, int)',
                 ARC_UNIT),
    'arc_copy': ('SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)', ARC_UNIT),
    'arc_bbox': ('SHAPE_ARC::BBox', ARC_UNIT),
    'ellipse_default': ('SHAPE_ELLIPSE::SHAPE_ELLIPSE()', ELLIPSE_UNIT),
    'ellipse_radii': ('SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, int, int, const EDA_ANGLE&)',
                      ELLIPSE_UNIT),
    'ellipse_arc_radii': ('SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, int, int, const EDA_ANGLE&, '
                          'const EDA_ANGLE&, const EDA_ANGLE&)', ELLIPSE_UNIT),
    'collide_shape': ('SHAPE::Collide(const SHAPE*, int, int*, VECTOR2I*)', COLLISIONS_UNIT),
    'compound_clone': ('SHAPE_COMPOUND::Clone', COMPOUND_UNIT),
    'compound_bbox': ('SHAPE_COMPOUND::BBox', COMPOUND_UNIT),
    'compound_collide': ('SHAPE_COMPOUND::Collide(const SEG&, int, int*, VECTOR2I*)', COMPOUND_UNIT),
    'compound_is_solid': ('SHAPE_COMPOUND::IsSolid', COMPOUND_UNIT),
    'compound_move': ('SHAPE_COMPOUND::Move', COMPOUND_UNIT),
    'compound_rotate': ('SHAPE_COMPOUND::Rotate', COMPOUND_UNIT),
    'log': ('kimathLogOverflow', UTIL_UNIT),
}
SHAPE_TYPES = ['SH_RECT', 'SH_SEGMENT', 'SH_LINE_CHAIN', 'SH_CIRCLE', 'SH_SIMPLE', 'SH_POLY_SET',
               'SH_COMPOUND', 'SH_ARC', 'SH_NULL', 'SH_POLY_SET_TRIANGLE', 'SH_ELLIPSE']


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SEGGEN = load('seg_generate_cases', ROOT / 'rust/kicad-seg-shim/generate_cases.py')
CIRCGEN = load('circle_generate_cases', ROOT / 'rust/kicad-circle-shim/generate_cases.py')


class Undefined(Exception):
    pass


class Outside(Exception):
    pass


UNDEFINED_ERRORS = (Undefined, SEGGEN.Undefined, CIRCGEN.Undefined)


def bits(v):
    return '%016x' % struct.unpack('<Q', struct.pack('<d', v))[0]


def I(v):
    if not INT_MIN <= v <= INT_MAX:
        raise Undefined
    return v


def L(v):
    if not I64_MIN <= v <= I64_MAX:
        raise Undefined
    return v


def subI(a, b):
    return (I(a[0] - b[0]), I(a[1] - b[1]))


def addI(a, b):
    return (I(a[0] + b[0]), I(a[1] + b[1]))


def sq_norm(v):
    return min(v[0] * v[0] + v[1] * v[1], I64_MAX)


def kiround_half(v):
    """KiROUND( x / 2.0 ): halves round away from zero; always in range."""
    q = abs(v) / 2.0
    r = math.floor(q + 0.5)
    return int(r) if v >= 0 else -int(r)


class Run:
    """One pinned evaluation. events: the calls the Rust port defers, in order."""

    def __init__(self):
        self.events = []
        self.labels = set()
        # Set after a SHAPE_CIRCLE::Rotate the replay could not follow (a non-cardinal angle);
        # such a rotation must end its script.
        self.unknown_centre = False

    def defer(self, name):
        self.events.append(name)

    def absorb(self, run):
        """Takes over the deferrals a seg generator mirror run recorded (only a KiROUND
        overflow's kimathLogOverflow can occur on these paths)."""
        for event in run.events:
            assert event == 'kimathLogOverflow', event
            self.defer('log')

    def sqdist(self, s, p):
        r = SEGGEN.Run()
        v = r.squared_distance_point(s, p)
        self.absorb(r)
        return v

    def nearest(self, s, p):
        r = SEGGEN.Run()
        v = r.nearest_point_point(s, p)
        self.absorb(r)
        return v

    # seg.h ------------------------------------------------------------------------------------
    @staticmethod
    def canonical(s):
        (ax, ay), (bx, by) = s
        qa, qb = ay - by, bx - ax
        qc = L(L(-qa * ax) - L(qb * ay))
        return qa, qb, qc

    def collinear(self, s, o):
        qa, qb, qc = self.canonical(s)
        out = []
        for p in o:
            v = L(L(L(p[0] * qa) + L(p[1] * qb)) + qc)
            if v == I64_MIN:
                raise Undefined
            out.append(abs(v))
        self.labels.add('collinear-true' if out[0] <= 1 and out[1] <= 1 else 'collinear-false')
        return out[0] <= 1 and out[1] <= 1

    def contains_point(self, s, p, threshold=3):
        return self.sqdist(s, p) <= threshold

    def approx_collinear(self, s, o, threshold):
        """seg.cpp ApproxCollinear, through the seg generator's guard and an exact value."""
        r = SEGGEN.Run()
        r.approx(s, o, threshold, True)
        self.absorb(r)
        la, lb = sq_norm(subI(s[0], s[1])), sq_norm(subI(o[0], o[1]))
        a, b = (o, s) if la < lb else (s, o)
        p, q, rr, l = SEGGEN.Run.canonical(a)
        if l == 0:
            return False
        sg = lambda v: (v > 0) - (v < 0)
        ds = []
        for pt in b:
            det = SEGGEN.Run.det(p, q, rr, pt)
            ds.append(sg(det) * SEGGEN.rescale(det, det, l, SEGGEN.wrap64))
        t = threshold * threshold
        return abs(ds[0]) <= t and abs(ds[1]) <= t

    def overlaps(self, s, o):
        if o[0] == o[1]:
            if s[0] == o[0] or s[1] == o[0]:
                self.labels.add('overlaps-point-end')
                return False
            self.labels.add('overlaps-point')
            return self.contains_point(s, o[0])
        if not self.collinear(s, o):
            return False
        if self.contains_point(s, o[0]) or self.contains_point(s, o[1]):
            self.labels.add('overlaps-other-end-inside')
            return True
        if self.contains_point(o, s[0]) or self.contains_point(o, s[1]):
            self.labels.add('overlaps-this-end-inside')
            return True
        self.labels.add('overlaps-disjoint')
        return False

    def approx_overlaps(self, s, o, threshold):
        if s[0] == s[1]:
            if o[0] == s[0] or o[1] == s[0]:
                self.labels.add('approx-overlaps-point-end')
                return False
            self.labels.add('approx-overlaps-point')
            return self.contains_point(o, s[0], threshold)
        if not self.approx_collinear(o, s, threshold):
            self.labels.add('approx-overlaps-not-collinear')
            return False
        if self.contains_point(o, s[0], threshold) or self.contains_point(o, s[1], threshold):
            self.labels.add('approx-overlaps-this-end-inside')
            return True
        if self.contains_point(s, o[0], threshold) or self.contains_point(s, o[1], threshold):
            self.labels.add('approx-overlaps-other-end-inside')
            return True
        self.labels.add('approx-overlaps-disjoint')
        return False

    def contains_seg(self, s, o):
        if o[0] == o[1]:
            self.labels.add('contains-seg-point')
            return self.contains_point(s, o[0])
        if not self.collinear(s, o):
            return False
        r = self.contains_point(s, o[0]) and self.contains_point(s, o[1])
        self.labels.add('contains-seg-true' if r else 'contains-seg-false')
        return r

    def center(self, s):
        d = subI(s[1], s[0])
        return addI(s[0], (kiround_half(d[0]), kiround_half(d[1])))

    def seg_op(self, inp):
        op = inp['op']
        s = tuple(tuple(p) for p in inp['seg']) if 'seg' in inp else None
        o = tuple(tuple(p) for p in inp['other']) if 'other' in inp else None
        p = tuple(inp['point']) if 'point' in inp else None
        if op == 'seg_side':
            d = subI(s[1], s[0])
            subI(p, s[0])
        elif op == 'seg_canonical':
            self.canonical(s)
        elif op == 'seg_collinear':
            self.collinear(s, o)
        elif op == 'seg_overlaps':
            self.overlaps(s, o)
        elif op == 'seg_approx_overlaps':
            self.approx_overlaps(s, o, inp.get('threshold', 3))
        elif op == 'seg_contains_seg':
            self.contains_seg(s, o)
        elif op in ('seg_length', 'seg_squared_length'):
            subI(s[0], s[1])
        elif op == 'seg_tcoef':
            d = subI(s[1], s[0])
            q = subI(p, s[0])
            L(d[0] * q[0] + d[1] * q[1])
        elif op == 'seg_center':
            self.center(s)
        elif op == 'seg_intersect_lines':
            r = SEGGEN.Run()
            r.intersect(s, o, False, True)
            self.absorb(r)
            for label in r.labels:
                if label.startswith('lines-') or label in ('intersects-parallel',
                                                           'intersects-crossing-point'):
                    self.labels.add('intersect-lines-' + label)
        self.labels.add(op)

    # Shapes -----------------------------------------------------------------------------------
    def build(self, spec, depth=0):
        """Returns a mirror state: ('circle', [c, r]), ('arc', [...]), ('compound', [...]),
        ('ellipse', None)."""
        kind = spec['kind']
        if kind == 'circle':
            self.labels.add('circle-ctor-' + spec['ctor'])
            if spec['ctor'] == 'default':
                return ['circle', (0, 0), 0]
            return ['circle', tuple(spec['center']), spec['radius']]
        if kind == 'arc':
            self.labels.add('arc-ctor-' + spec['ctor'])
            if spec['ctor'] == 'default':
                return ['arc', ((0, 0), (0, 0), (0, 0)), 0, 'default']
            pts = (tuple(spec['start']), tuple(spec['mid']), tuple(spec['end']))
            arc_domain(pts, spec['width'])
            self.defer('arc_ctor')
            return ['arc', pts, spec['width'], 'three_point']
        if kind == 'ellipse':
            self.labels.add('ellipse-ctor-' + spec['ctor'])
            self.defer('ellipse_' + spec['ctor'])
            return ['ellipse']
        assert kind == 'compound'
        members = []
        for item in spec['add']:
            self.add_shape(members, item, depth)
        self.labels.add('compound-ctor')
        return ['compound', members]

    def clone(self, state):
        if state[0] == 'arc':
            self.defer('arc_copy')
        if state[0] == 'compound':
            self.defer('compound_clone')
            # SHAPE_COMPOUND's copy constructor calls shape->Clone() on every entry.
            if any(m[0] == 'null' for m in state[1]):
                raise Undefined
        return [state[0]] + list(state[1:])

    def add_shape(self, members, item, depth):
        if item['shape'] is None:
            # AddShape( SHAPE* ) stores a null pointer; the shared overload dereferences it.
            if item['shared']:
                raise Undefined
            self.labels.add('add-null')
            members.append(['null'])
            return
        sub = self.build(item['shape'], depth + 1)
        if sub[0] == 'compound':
            self.labels.add('add-flatten-shared' if item['shared'] else 'add-flatten')
            for m in sub[1]:
                # subshape->Clone() dereferences a null entry.
                if m[0] == 'null':
                    raise Undefined
                members.append(self.clone(m))
        elif item['shared']:
            self.labels.add('add-shared-clone')
            members.append(self.clone(sub))
        else:
            self.labels.add('add-owned')
            members.append(sub)

    def collide_circle(self, state, seg, clearance, want_actual, want_location):
        _, c, r = state
        md = I(clearance + r)
        pn = self.nearest(seg, c)
        d = subI(pn, c)
        dist_sq = sq_norm(d)
        if not (dist_sq == 0 or dist_sq < md * md):
            self.labels.add('circle-collide-clear')
            return
        self.labels.add('circle-collide-zero' if dist_sq == 0 else 'circle-collide-within')
        if want_location:
            pts, logged = CIRCGEN.intersect_line(c, r, seg[0], seg[1])
            if logged:
                self.defer('log')
            kept = [p for p in pts if self.sqdist(seg, p) <= 3]
            if kept and dist_sq == 0:
                self.labels.add('circle-collide-location-intersection')
                CIRCGEN.intersect_line(c, r, seg[0], seg[1])
            else:
                self.labels.add('circle-collide-location-nearest')
        if want_actual:
            root = math.trunc(math.sqrt(float(dist_sq)))
            if root >= 2**31:
                raise Undefined
            v = I(root - r)
            self.labels.add('circle-collide-actual-negative' if v < 0 else 'circle-collide-actual')

    def call(self, state, c):
        name = c['call']
        kind = state[0]
        assert not self.unknown_centre, c
        if 'circle-rotate' in self.labels:
            self.labels.add('circle-after-rotate')
        self.labels.add(f'{kind}-{name}')
        if name in ('type', 'type_name', 'is_null', 'has_indexable_subshapes',
                    'indexable_subshape_count', 'indexable_subshapes', 'get_start', 'get_end',
                    'get_width', 'get_radius', 'get_center', 'get_circle', 'get_p0', 'get_p1',
                    'get_arc_mid', 'get_chord', 'empty', 'size', 'unique_subshape', 'shapes',
                    'get_major_radius', 'get_minor_radius', 'get_rotation', 'get_start_angle',
                    'get_end_angle', 'is_arc'):
            return
        if name == 'set_width':
            if kind == 'arc':
                state[2] = c['width']
            return
        if name == 'set_radius':
            state[2] = c['radius']
            return
        if name == 'set_center':
            state[1] = tuple(c['center'])
            return
        if name == 'assign':
            if kind == 'circle':
                src = self.build(c['from'])
                state[1], state[2] = src[1], src[2]
            else:
                self.build(c['from'])
            return
        if name == 'equals':
            self.build(c['other'])
            return
        if name in ('is_ccw', 'is_clockwise'):
            start, mid, end = state[1]
            v1 = (end[0] - mid[0], end[1] - mid[1])
            v2 = (start[0] - mid[0], start[1] - mid[1])
            L(L(v1[0] * v2[1]) - L(v1[1] * v2[0]))
            return
        if name == 'clone':
            self.clone(state)
            return
        if name == 'is_solid':
            if kind == 'compound':
                compound_domain(state)
                self.defer('compound_is_solid')
            return
        if name == 'add_shape':
            self.add_shape(state[1], c, 0)
            return
        if name in ('bbox', 'centre'):
            clearance = c.get('clearance', 0)
            if kind == 'circle':
                r = I(state[2] + clearance)
                subI(state[1], (r, r))
                I(2 * r)
            elif kind == 'arc':
                if state[3] != 'default' or not within(clearance) or not within(state[2]):
                    raise Outside
                self.defer('arc_bbox')
            else:
                compound_domain(state)
                if not within(clearance):
                    raise Outside
                self.defer('compound_bbox')
            return
        if name in ('collide_seg', 'collide_point'):
            seg = (tuple(c['seg'][0]), tuple(c['seg'][1])) if name == 'collide_seg' else \
                (tuple(c['point']), tuple(c['point']))
            clearance = c.get('clearance', 0)
            if kind == 'circle':
                self.collide_circle(state, seg, clearance, 'actual' in c, 'location' in c)
            else:
                compound_domain(state)
                if not (within(clearance) and all(within(v) for p in seg for v in p)):
                    raise Outside
                self.defer('compound_collide')
            return
        if name == 'collide_shape':
            other = self.build(c['other'])
            clearance = c.get('clearance', 0)
            if kind == 'circle':
                circle_domain(state)
            else:
                compound_domain(state)
            if other[0] != 'circle' or not within(clearance):
                raise Outside
            circle_domain(other)
            self.defer('collide_shape')
            return
        if name == 'move':
            if kind == 'circle':
                state[1] = addI(state[1], tuple(c['vector']))
            else:
                compound_domain(state)
                if not all(within(v) for v in c['vector']):
                    raise Outside
                self.defer('compound_move')
            return
        if name == 'rotate':
            center = tuple(c.get('center', (0, 0)))
            if kind == 'circle':
                ox, oy = subI(state[1], center)
                # Only the cardinal branches negate; the generator keeps every rotation within
                # 2^29 of a centre within 2^29, so the final sums cannot overflow. The trigo.h
                # wrapper is ported (libs.kimath.include.application.ui.p001). A cardinal
                # rotation is replayed exactly, so later calls are classified on the rotated
                # centre; any other angle needs sin/cos and must end the script.
                assert all(abs(v) <= 2**29 for v in (ox, oy) + center), c
                self.labels.add('circle-rotate')
                turn = cardinal_turn(c['angle'])
                if turn is None:
                    self.unknown_centre = True
                else:
                    rotated = [(ox, oy), (oy, -ox), (-ox, -oy), (-oy, ox)][turn]
                    state[1] = addI(rotated, center)
            else:
                compound_domain(state)
                if not all(within(v) for v in center):
                    raise Outside
                self.defer('compound_rotate')
            return
        raise ValueError(name)


def within(v):
    return -DOMAIN <= v <= DOMAIN


def cardinal_turn(angle_bits):
    """The quarter turn RotatePoint( int*, int*, const EDA_ANGLE& ) takes for an angle in
    degrees whose EDA_ANGLE::Normalize is 0, 90, 180 or 270 (an integral multiple of 90, or a
    non-finite value, which foldToTwoPeriods makes 0), or None."""
    a = struct.unpack('>d', bytes.fromhex(angle_bits))[0]
    if a != a or a in (float('inf'), float('-inf')):
        return 0
    if a != int(a) or int(a) % 90:
        return None
    return (int(a) % 360) // 90


def circle_domain(state):
    if state[0] != 'circle' or not all(within(v) for v in state[1]) \
            or not 0 <= state[2] <= DOMAIN:
        raise Outside


def compound_domain(state):
    for m in state[1]:
        circle_domain(m)


def arc_domain(pts, width):
    """The three-point SHAPE_ARC constructor's verified domain: coordinates within 2^20, distinct
    points, and an angle at the mid point whose sine is at least 1/8."""
    if not all(within(v) for p in pts for v in p) or not within(width):
        raise Outside
    s, m, e = pts
    d1 = (m[0] - s[0], m[1] - s[1])
    d2 = (e[0] - m[0], e[1] - m[1])
    n1, n2 = d1[0]**2 + d1[1]**2, d2[0]**2 + d2[1]**2
    cross = d1[0] * d2[1] - d1[1] * d2[0]
    if s == e or n1 == 0 or n2 == 0 or cross * cross * 64 < n1 * n2:
        raise Outside


def evaluate(inp):
    run = Run()
    try:
        if inp['op'] == 'shape':
            state = run.build(inp['shape'])
            for c in inp['calls']:
                run.call(state, c)
        elif inp['op'].startswith('seg_') and inp['op'] not in ('seg_new', 'seg_copy',
                                                                'seg_compare', 'seg_square',
                                                                'seg_reverse', 'seg_reversed',
                                                                'seg_stream'):
            run.seg_op(inp)
        elif inp['op'] == 'shape_type_as_string':
            # SHAPE_TYPE's values are 0 to 15 (no fixed underlying type, enumerators 0 to 10);
            # static_cast of anything else is undefined. 11 to 15 are unnamed.
            if not 0 <= inp['value'] <= 15:
                raise Undefined
            run.labels.add('shape-type-unnamed' if inp['value'] > 10 else 'shape-type-named')
        else:
            run.labels.add(inp['op'])
    except UNDEFINED_ERRORS:
        return 'undefined', run.labels, run.events
    except Outside:
        return 'outside', run.labels, run.events
    return 'defined', run.labels, run.events


# Inputs ---------------------------------------------------------------------------------------
EXTREMES = [INT_MIN, INT_MIN + 1, -2**30, -46341, -46340, -1, 0, 1, 46340, 46341, 2**30,
            INT_MAX - 1, INT_MAX]
HAND_SEGS = SEGGEN.HAND_SEGS
HAND_POINTS = SEGGEN.HAND_POINTS
SCALES = [12, 1000, 10**6, 2**30, 'full', 'extreme']


def rand_point(rng, scale):
    return SEGGEN.rand_point(rng, scale)


def rand_seg(rng, scale):
    return SEGGEN.rand_seg(rng, scale)


SEG_PAIR_OPS = ['seg_compare', 'seg_collinear', 'seg_overlaps', 'seg_approx_overlaps',
                'seg_contains_seg', 'seg_intersect_lines']
SEG_ONE_OPS = ['seg_canonical', 'seg_length', 'seg_squared_length', 'seg_center', 'seg_stream',
               'seg_reversed']
SEG_POINT_OPS = ['seg_side', 'seg_tcoef']


def seg_candidates(rng):
    out = []
    out.append(('seg_new-default', {'op': 'seg_new', 'ctor': 'default'}))
    for i, s in enumerate(HAND_SEGS):
        out.append((f'seg_new-coords-{i}', {'op': 'seg_new', 'ctor': 'coords',
                                           'coords': s[0] + s[1]}))
        out.append((f'seg_new-points-{i}', {'op': 'seg_new', 'ctor': 'points', 'seg': s}))
        out.append((f'seg_new-indexed-{i}', {'op': 'seg_new', 'ctor': 'indexed', 'seg': s,
                                            'index': rng.choice([-1, 0, 7, INT_MIN, INT_MAX])}))
        out.append((f'seg_copy-{i}', {'op': 'seg_copy', 'seg': s, 'index': i - 3}))
        out.append((f'seg_assign-{i}', {'op': 'seg_copy', 'seg': s, 'index': 5 * i,
                                       'assign_to': {'seg': HAND_SEGS[-1 - i], 'index': -i}}))
        out.append((f'seg_reverse-{i}', {'op': 'seg_reverse', 'seg': s, 'index': i}))
    for v in EXTREMES + [2, -2, 3, 12345, -99999]:
        out.append((f'seg_square-{v}', {'op': 'seg_square', 'value': v}))
    for t in range(16):
        out.append((f'shape_type_as_string-{t}', {'op': 'shape_type_as_string', 'value': t}))
    for name, t in [('below', -1), ('above', 16), ('int-min', INT_MIN), ('int-max', INT_MAX)]:
        out.append((f'shape_type_as_string-{name}', {'op': 'shape_type_as_string', 'value': t}))
    out.append(('constants', {'op': 'constants'}))
    for si, s in enumerate(HAND_SEGS):
        for op in SEG_ONE_OPS:
            out.append((f'{op}-hand-{si}', {'op': op, 'seg': s}))
        for pi, p in enumerate(HAND_POINTS):
            for op in SEG_POINT_OPS:
                if (si + pi) % 2 == 0:
                    out.append((f'{op}-hand-{si}-{pi}', {'op': op, 'seg': s, 'point': p}))
        for oi, o in enumerate(HAND_SEGS):
            for op in SEG_PAIR_OPS:
                if (si * 3 + oi) % 4:
                    continue
                inp = {'op': op, 'seg': s, 'other': o}
                if op == 'seg_approx_overlaps' and (si + oi) % 2:
                    inp['threshold'] = rng.choice([0, 1, 3, 10, -3, 46341, INT_MAX])
                out.append((f'{op}-hand-{si}-{oi}', inp))
    for scale in SCALES:
        name = scale if isinstance(scale, str) else f's{scale}'
        for op in SEG_ONE_OPS:
            for n in range(12):
                s = rand_seg(rng, scale)
                if n % 5 == 0:
                    s[1] = list(s[0])
                out.append((f'{op}-{name}-{n}', {'op': op, 'seg': s}))
        for op in SEG_POINT_OPS:
            for n in range(30):
                out.append((f'{op}-{name}-{n}', {'op': op, 'seg': rand_seg(rng, scale),
                                                 'point': rand_point(rng, scale)}))
        for op in SEG_PAIR_OPS:
            for n in range(40):
                if n % 2:
                    s, o = SEGGEN.related_segs(rng, scale)
                    s, o = SEGGEN.clamp_seg(s), SEGGEN.clamp_seg(o)
                    fam = 'related'
                else:
                    s, o = rand_seg(rng, scale), rand_seg(rng, scale)
                    fam = 'random'
                inp = {'op': op, 'seg': s, 'other': o}
                if op == 'seg_approx_overlaps' and n % 3:
                    inp['threshold'] = rng.choice([0, 1, 2, 3, 10, 1000, -3, 46341, INT_MAX])
                out.append((f'{op}-{name}-{fam}-{n}', inp))
    # Collinear chains and overlaps with small offsets reach every containment branch.
    for n in range(60):
        a = [rng.randint(-1000, 1000), rng.randint(-1000, 1000)]
        d = [rng.randint(-50, 50), rng.randint(-50, 50)]
        k = [rng.randint(-3, 4) for _ in range(4)]
        off = rng.choice([[0, 0], [0, 1], [1, 0], [1, 1], [0, 2], [3, 0]])
        s = [[a[0] + k[0] * d[0], a[1] + k[0] * d[1]], [a[0] + k[1] * d[0], a[1] + k[1] * d[1]]]
        o = [[a[0] + k[2] * d[0] + off[0], a[1] + k[2] * d[1] + off[1]],
             [a[0] + k[3] * d[0] + off[0], a[1] + k[3] * d[1] + off[1]]]
        for op in ('seg_overlaps', 'seg_approx_overlaps', 'seg_contains_seg', 'seg_collinear'):
            inp = {'op': op, 'seg': s, 'other': o}
            if op == 'seg_approx_overlaps':
                inp['threshold'] = rng.choice([1, 3, 5])
            out.append((f'{op}-chain-{n}', inp))
    out.extend(stream_state_candidates())
    return out


def state(flags, width=0, fill=' ', precision=6):
    return {'flags': flags, 'precision': precision, 'width': width, 'fill': fill}


DEFAULT_FLAGS = ['skipws', 'dec']
STREAM_SEGS = [[[10, 11], [12, 13]], [[0, -1], [INT_MIN, INT_MAX]], [[-1, 0], [8, 9]],
               [[-255, 4096], [65535, -65536]], [[0, 0], [0, 0]]]
STREAM_STATES = [
    ('default', state(DEFAULT_FLAGS)),
    ('no-flags', state([])),
    ('hex-showbase-upper-fill-width', state(['skipws', 'hex', 'showbase', 'uppercase'], 5, '_')),
    ('hex', state(['hex'])),
    ('hex-showbase', state(['hex', 'showbase'])),
    ('hex-upper', state(['hex', 'uppercase'], 2, '0')),
    ('oct-showbase', state(['oct', 'showbase'])),
    ('oct', state(['oct'], 9, '#')),
    ('dec-and-hex', state(['dec', 'hex', 'showbase'])),
    ('hex-and-oct', state(['hex', 'oct', 'showbase'])),
    ('showpos', state(['dec', 'showpos'])),
    ('showpos-hex', state(['hex', 'showpos', 'showbase'])),
    ('left-width', state(['dec', 'left', 'showpos'], 4)),
    ('right-width', state(['dec', 'right'], 7, '.')),
    ('internal-width', state(['hex', 'showbase', 'internal'], 12, '*')),
    ('left-and-right', state(['left', 'right'], 6, '-')),
    ('width-exact', state(DEFAULT_FLAGS, 2, 'x')),
    ('width-one', state(DEFAULT_FLAGS, 1, 'x')),
    ('width-negative', state(DEFAULT_FLAGS, -5, 'x')),
    ('width-max', state(DEFAULT_FLAGS, 512, '~')),
    ('float-flags', state(['fixed', 'scientific', 'showpoint', 'boolalpha', 'uppercase'], 3,
                          ' ', 17)),
    ('precision-negative', state(['scientific'], 0, ' ', -64)),
    ('every-flag', state(['boolalpha', 'dec', 'fixed', 'hex', 'internal', 'left', 'oct', 'right',
                          'scientific', 'showbase', 'showpoint', 'showpos', 'skipws', 'unitbuf',
                          'uppercase'], 8, '=')),
]


def stream_state_candidates():
    """operator<<( std::ostream&, const SEG& ) into a caller's stream: flags reach every
    coordinate, the width pads only the first insertion and is 0 afterwards."""
    out = []
    for si, s in enumerate(STREAM_SEGS):
        for name, st in STREAM_STATES:
            out.append((f'seg_stream-state-{name}-{si}', {'op': 'seg_stream', 'seg': s,
                                                           'state': st}))
    return out


def circle_spec(rng, scale, ctor=None):
    ctor = ctor or rng.choice(['default', 'center_radius', 'circle', 'copy'])
    if ctor == 'default':
        return {'kind': 'circle', 'ctor': 'default'}
    if scale == 'domain':
        c, r = [rng.randint(-DOMAIN, DOMAIN), rng.randint(-DOMAIN, DOMAIN)], rng.randint(0, DOMAIN)
    elif scale == 'small':
        c, r = [rng.randint(-100, 100), rng.randint(-100, 100)], rng.randint(0, 60)
    else:
        c, r = rand_point(rng, scale), rng.choice([0, 1, 5, 100, 46341, 2**30, INT_MAX, -1,
                                                   INT_MIN, rng.randint(0, 10**6)])
    return {'kind': 'circle', 'ctor': ctor, 'center': c, 'radius': r}


def well_conditioned_arc(rng, bound):
    while True:
        s = [rng.randint(-bound, bound), rng.randint(-bound, bound)]
        m = [rng.randint(-bound, bound), rng.randint(-bound, bound)]
        e = [rng.randint(-bound, bound), rng.randint(-bound, bound)]
        try:
            arc_domain((tuple(s), tuple(m), tuple(e)), 0)
            return s, m, e
        except Outside:
            continue


def arc_spec(rng, ctor='default'):
    if ctor == 'default':
        return {'kind': 'arc', 'ctor': 'default'}
    s, m, e = well_conditioned_arc(rng, rng.choice([20, 1000, DOMAIN]))
    return {'kind': 'arc', 'ctor': 'three_point', 'start': s, 'mid': m, 'end': e,
            'width': rng.choice([0, 1, 250, DOMAIN])}


ANGLES = [0.0, -0.0, 30.0, 90.0, 180.0, 270.0, -90.0, 360.0, 400.5, -720.0, 1e-300, 45.0]


def ellipse_spec(rng, ctor):
    if ctor == 'default':
        return {'kind': 'ellipse', 'ctor': 'default'}
    spec = {'kind': 'ellipse', 'ctor': ctor, 'center': rand_point(rng, rng.choice([100, 'extreme'])),
            'major': rng.choice([0, 1, 10, -5, 300, INT_MAX, INT_MIN]),
            'minor': rng.choice([0, 1, 20, -5, 300, INT_MAX]),
            'rotation': bits(rng.choice(ANGLES))}
    if ctor == 'arc_radii':
        spec['start'] = bits(rng.choice(ANGLES))
        spec['end'] = bits(rng.choice(ANGLES))
    return spec


def collide_args(rng, call, scale, small):
    c = {'call': call}
    if call == 'collide_seg':
        c['seg'] = [rand_point(rng, scale), rand_point(rng, scale)] if not small else \
            [[rng.randint(-200, 200), rng.randint(-200, 200)] for _ in range(2)]
        if rng.random() < 0.3:
            c['seg'][1] = list(c['seg'][0])
    else:
        c['point'] = rand_point(rng, scale) if not small else \
            [rng.randint(-200, 200), rng.randint(-200, 200)]
    if rng.random() < 0.8:
        c['clearance'] = rng.choice([0, 1, 3, 10, 50, -5, INT_MAX, INT_MIN, 46341]) if not small \
            else rng.choice([0, 1, 3, 10, 50])
    if rng.random() < 0.6:
        c['actual'] = rng.choice([0, -7, 12345])
    if rng.random() < 0.6:
        c['location'] = [rng.randint(-9, 9), rng.randint(-9, 9)]
    return c


GENERIC = [{'call': 'type'}, {'call': 'type_name'}, {'call': 'is_null'},
           {'call': 'has_indexable_subshapes'}, {'call': 'indexable_subshape_count'},
           {'call': 'get_start'}, {'call': 'get_end'}, {'call': 'get_width'}]


def circle_script(rng, scale):
    calls = []
    for _ in range(rng.randint(2, 7)):
        k = rng.randrange(12)
        if k == 0:
            calls.append(rng.choice(GENERIC))
        elif k == 1:
            calls.append({'call': 'indexable_subshapes', 'prefill': rng.choice([0, 1, 3])})
        elif k == 2:
            calls.append({'call': rng.choice(['get_radius', 'get_center', 'get_circle',
                                              'is_solid', 'clone'])})
        elif k == 3:
            calls.append({'call': 'set_radius', 'radius': rng.choice([0, 7, -3, 1000, INT_MAX])})
        elif k == 4:
            calls.append({'call': 'set_center', 'center': rand_point(rng, scale)})
        elif k == 5:
            c = {'call': 'bbox'}
            if rng.random() < 0.7:
                c['clearance'] = rng.choice([0, 1, -1, 10, -10, 1000, INT_MAX, INT_MIN])
            calls.append(c)
        elif k == 6:
            calls.append({'call': 'centre'})
        elif k in (7, 8):
            calls.append(collide_args(rng, rng.choice(['collide_seg', 'collide_point']), scale,
                                      False))
        elif k == 9:
            calls.append({'call': 'move', 'vector': rand_point(rng, rng.choice([10, scale]))})
        elif k == 10:
            calls.append({'call': 'assign', 'from': circle_spec(rng, scale)})
        else:
            calls.append({'call': 'set_width', 'width': rng.choice([0, 5, -1])})
    return calls


def shape_candidates(rng):
    out = []
    # Circles at every scale.
    for scale in [12, 1000, 10**6, 2**30, 'full', 'extreme']:
        name = scale if isinstance(scale, str) else f's{scale}'
        for n in range(70):
            out.append((f'shape-circle-{name}-{n}',
                        {'op': 'shape', 'shape': circle_spec(rng, scale),
                         'calls': circle_script(rng, scale)}))
    # Collisions near the circle: through the centre, tangent, within and outside the clearance.
    for n in range(120):
        c = [rng.randint(-1000, 1000), rng.randint(-1000, 1000)]
        r = rng.randint(0, 300)
        y = c[1] + rng.choice([0, r, r + 1, r + 3, r - 1, 2 * r + 5, -r, rng.randint(-r - 9, r + 9)])
        seg = [[c[0] - rng.randint(0, 2000), y], [c[0] + rng.randint(-50, 2000), y]]
        call = {'call': 'collide_seg', 'seg': seg, 'clearance': rng.choice([0, 1, 4, 20, -2]),
                'actual': -1, 'location': [5, 5]}
        if n % 4 == 0:
            del call['location']
        if n % 5 == 0:
            del call['actual']
        out.append((f'shape-circle-collide-{n}',
                    {'op': 'shape', 'shape': {'kind': 'circle', 'ctor': 'center_radius',
                                              'center': c, 'radius': r}, 'calls': [call]}))
    # The (int) sqrt( dist_sq ) boundary: minDist = INT_MIN admits dist_sq just below 2^62.
    for n, (dx, dy) in enumerate([(2147483524, 729778), (2147481629, 2944747), (2147483647, 0),
                                  (2147483647, 1), (2147483000, 0)]):
        for actual in (True, False):
            call = {'call': 'collide_point', 'point': [dx, dy], 'clearance': INT_MIN,
                    'location': [0, 0]}
            if actual:
                call['actual'] = 0
            out.append((f'shape-circle-root-{n}-{int(actual)}',
                        {'op': 'shape', 'shape': {'kind': 'circle', 'ctor': 'center_radius',
                                                  'center': [0, 0], 'radius': 0},
                         'calls': [call]}))
    # Rotations as the last call, within the safe range.
    for n in range(40):
        spec = circle_spec(rng, 10**6, 'center_radius')
        call = {'call': 'rotate', 'angle': bits(rng.choice(ANGLES + [rng.uniform(-400, 400)]))}
        if n % 2:
            call['center'] = rand_point(rng, 10**6)
        out.append((f'shape-circle-rotate-{n}', {'op': 'shape', 'shape': spec,
                                                 'calls': circle_script(rng, 1000)[:2] + [call]}))
    # Arcs: the default arc's inline members, and three-point arcs (deferred at construction).
    arc_calls = [{'call': c} for c in ['get_p0', 'get_p1', 'get_arc_mid', 'get_chord', 'is_ccw',
                                        'is_clockwise', 'is_solid', 'get_start', 'get_end',
                                        'get_width', 'type', 'type_name', 'is_null',
                                        'has_indexable_subshapes', 'indexable_subshape_count']]
    for n in range(30):
        calls = rng.sample(arc_calls, rng.randint(2, 8))
        if n % 2:
            calls.insert(rng.randrange(len(calls) + 1),
                         {'call': 'set_width', 'width': rng.choice([0, 3, -9, INT_MAX, INT_MIN])})
        if n % 3 == 0:
            other = {'call': 'equals', 'other': arc_spec(rng)}
            if n % 2:
                other['other_width'] = rng.choice([0, 3, -9])
            calls.append(other)
        if n % 5 == 0:
            calls.append({'call': 'indexable_subshapes', 'prefill': n % 3})
        if n % 7 == 1:
            calls.append({'call': rng.choice(['clone', 'bbox', 'centre'])})
        out.append((f'shape-arc-default-{n}', {'op': 'shape', 'shape': arc_spec(rng),
                                               'calls': calls}))
    for n in range(30):
        calls = rng.sample(arc_calls, rng.randint(2, 6))
        out.append((f'shape-arc-three-point-{n}', {'op': 'shape', 'shape': arc_spec(rng, 'three'),
                                                   'calls': calls}))
    # Compounds.
    def member(depth=0):
        k = rng.randrange(9 if depth < 2 else 6)
        if k < 4:
            return circle_spec(rng, rng.choice(['small', 'domain', 10**6, 'extreme']))
        if k < 6:
            return arc_spec(rng)
        return {'kind': 'compound',
                'add': [{'shape': member(depth + 1), 'shared': rng.random() < 0.5}
                        for _ in range(rng.randint(0, 3))]}
    comp_inline = [{'call': c} for c in ['empty', 'size', 'unique_subshape', 'shapes', 'type',
                                          'type_name', 'is_null', 'has_indexable_subshapes',
                                          'indexable_subshape_count', 'get_start', 'get_end',
                                          'get_width']]
    for n in range(160):
        spec = {'kind': 'compound', 'add': [{'shape': member(), 'shared': rng.random() < 0.5}
                                            for _ in range(rng.randint(0, 3))]}
        calls = []
        for _ in range(rng.randint(1, 6)):
            k = rng.randrange(6)
            if k < 3:
                calls.append(rng.choice(comp_inline))
            elif k == 3:
                calls.append({'call': 'add_shape', 'shape': member(), 'shared': rng.random() < 0.5})
            elif k == 4:
                calls.append({'call': 'indexable_subshapes', 'prefill': rng.choice([0, 2, 5])})
            else:
                calls.append({'call': 'set_width', 'width': 4})
        out.append((f'shape-compound-{n}', {'op': 'shape', 'shape': spec, 'calls': calls}))
    # Out-of-line compound members over in-domain circles (deferred in Rust).
    for n in range(40):
        spec = {'kind': 'compound', 'add': [{'shape': circle_spec(rng, rng.choice(['small', 'domain']),
                                                                  'center_radius'),
                                             'shared': rng.random() < 0.5}
                                            for _ in range(rng.randint(1, 3))]}
        k = n % 8
        if k == 0:
            last = {'call': 'bbox', 'clearance': rng.randint(0, 100)}
        elif k == 1:
            last = {'call': 'centre'}
        elif k == 2:
            last = collide_args(rng, 'collide_seg', None, True)
        elif k == 3:
            last = collide_args(rng, 'collide_point', None, True)
        elif k == 4:
            last = {'call': 'is_solid'}
        elif k == 5:
            last = {'call': 'move', 'vector': [rng.randint(-99, 99), rng.randint(-99, 99)]}
        elif k == 6:
            last = {'call': 'rotate', 'angle': bits(rng.choice(ANGLES))}
        else:
            last = {'call': 'clone'}
        out.append((f'shape-compound-out-of-line-{n}',
                    {'op': 'shape', 'shape': spec, 'calls': [{'call': 'size'}, last]}))
    # SHAPE::Collide( const SHAPE* ... ) from a circle or a compound of circles.
    for n in range(20):
        recv = circle_spec(rng, 'small', 'center_radius') if n % 2 else \
            {'kind': 'compound', 'add': [{'shape': circle_spec(rng, 'small', 'center_radius'),
                                          'shared': False}]}
        call = {'call': 'collide_shape', 'other': circle_spec(rng, 'small', 'center_radius'),
                'clearance': rng.randint(0, 20), 'actual': 0, 'location': [0, 0]}
        out.append((f'shape-collide-shape-{n}', {'op': 'shape', 'shape': recv, 'calls': [call]}))
    # Ellipses (every constructor here is out-of-line, so Rust defers at construction).
    ell_calls = [{'call': c} for c in ['get_center', 'get_major_radius', 'get_minor_radius',
                                        'get_rotation', 'get_start_angle', 'get_end_angle',
                                        'is_arc', 'is_solid', 'type_name', 'clone']]
    for n in range(24):
        ctor = ['default', 'radii', 'arc_radii'][n % 3]
        calls = rng.sample(ell_calls, rng.randint(2, 6))
        if n % 4 == 0:
            calls.append({'call': 'equals', 'other': ellipse_spec(rng, rng.choice(['radii',
                                                                                    'arc_radii']))})
        if n % 4 == 1:
            calls.append({'call': 'assign', 'from': ellipse_spec(rng, 'arc_radii')})
        out.append((f'shape-ellipse-{n}', {'op': 'shape', 'shape': ellipse_spec(rng, ctor),
                                           'calls': calls}))
    # Out-of-domain requests (controls).
    for n in range(6):
        s = [rng.randint(-DOMAIN, DOMAIN), rng.randint(-DOMAIN, DOMAIN)]
        out.append((f'shape-arc-collinear-{n}',
                    {'op': 'shape', 'shape': {'kind': 'arc', 'ctor': 'three_point', 'start': s,
                                              'mid': [s[0] + 1, s[1] + 1], 'end': [s[0] + 2, s[1] + 2],
                                              'width': 0}, 'calls': [{'call': 'get_p0'}]}))
    out.append(('shape-arc-huge', {'op': 'shape', 'shape': {
        'kind': 'arc', 'ctor': 'three_point', 'start': [INT_MIN, 0], 'mid': [0, INT_MAX],
        'end': [INT_MAX, 0], 'width': 0}, 'calls': []}))
    out.append(('shape-compound-huge-bbox', {'op': 'shape', 'shape': {
        'kind': 'compound', 'add': [{'shape': {'kind': 'circle', 'ctor': 'center_radius',
                                               'center': [INT_MAX, 0], 'radius': 5},
                                     'shared': False}]}, 'calls': [{'call': 'bbox'}]}))
    out.append(('shape-compound-arc-bbox', {'op': 'shape', 'shape': {
        'kind': 'compound', 'add': [{'shape': {'kind': 'arc', 'ctor': 'default'},
                                     'shared': False}]}, 'calls': [{'call': 'bbox'}]}))
    out.append(('shape-collide-shape-arc', {'op': 'shape', 'shape': circle_spec(rng, 'small', 'center_radius'),
                                            'calls': [{'call': 'collide_shape',
                                                       'other': {'kind': 'arc', 'ctor': 'default'}}]}))
    out.extend(null_entry_candidates())
    # Continuations after SHAPE_CIRCLE::Rotate, which the trigo.h port
    # (libs.kimath.include.application.ui.p001) made reachable: cardinal rotations, replayed
    # exactly, followed by further calls on the rotated circle. Their own seed leaves the earlier
    # cases unchanged.
    crng = random.Random(SEED + 0x7419)
    turns = [0.0, -0.0, 90.0, 180.0, 270.0, -90.0, 360.0, 450.0, -720.0, 1170.0, 3.6e15 + 90.0]
    for n in range(40):
        spec = circle_spec(crng, 10**6, 'center_radius')
        call = {'call': 'rotate', 'angle': bits(crng.choice(turns))}
        if n % 2:
            call['center'] = rand_point(crng, 10**6)
        calls = circle_script(crng, 1000)[:2] + [call] + circle_script(crng, 1000)[:4]
        out.append((f'shape-circle-rotate-then-{n}', {'op': 'shape', 'shape': spec,
                                                      'calls': calls}))
    return out


def null_entry_candidates():
    """AddShape( (SHAPE*) nullptr ) stores a null entry, which the inline members report without
    dereferencing it. The shared overload with a null pointer, flattening a compound holding a
    null entry and copying one (Clone) dereference it: undefined, so controls."""
    null = {'shape': None, 'shared': False}
    circle = {'kind': 'circle', 'ctor': 'center_radius', 'center': [3, -4], 'radius': 5}
    arc = {'kind': 'arc', 'ctor': 'default'}
    inline = [{'call': c} for c in ['empty', 'size', 'unique_subshape', 'shapes', 'type',
                                     'type_name', 'is_null', 'has_indexable_subshapes',
                                     'indexable_subshape_count', 'get_start', 'get_end',
                                     'get_width']]
    def compound(*items):
        return {'kind': 'compound', 'add': list(items)}
    out = []
    def add(name, spec, calls):
        out.append((f'shape-compound-null-{name}', {'op': 'shape', 'shape': spec, 'calls': calls}))
    add('only', compound(null), inline + [{'call': 'indexable_subshapes', 'prefill': p}
                                          for p in (0, 2)])
    add('set-width', compound(null), [{'call': 'set_width', 'width': 7}, {'call': 'get_width'},
                                      {'call': 'size'}])
    add('mixed', compound({'shape': circle, 'shared': True}, null, {'shape': arc, 'shared': False},
                          null), inline + [{'call': 'indexable_subshapes', 'prefill': 1}])
    add('added-by-call', compound({'shape': circle, 'shared': True}),
        [{'call': 'unique_subshape'}, {'call': 'add_shape', 'shape': None, 'shared': False},
         {'call': 'size'}, {'call': 'unique_subshape'}, {'call': 'shapes'},
         {'call': 'add_shape', 'shape': circle, 'shared': False}, {'call': 'shapes'}])
    add('added-to-empty', compound(), [{'call': 'empty'},
                                       {'call': 'add_shape', 'shape': None, 'shared': False},
                                       {'call': 'empty'}, {'call': 'size'},
                                       {'call': 'unique_subshape'}])
    add('nested-flatten-without-null', compound(null, {'shape': compound({'shape': circle,
                                                                         'shared': False}),
                                                      'shared': False}),
        [{'call': 'shapes'}, {'call': 'size'}])
    # Out-of-line members check their verified domain (circles only) before the call.
    for call in [{'call': 'bbox'}, {'call': 'centre'}, {'call': 'is_solid'},
                 {'call': 'move', 'vector': [1, 1]}, {'call': 'collide_point', 'point': [0, 0]}]:
        add(f'out-of-line-{call["call"]}', compound(null), [{'call': 'size'}, call])
    # Undefined: these dereference the null entry.
    add('shared', compound({'shape': None, 'shared': True}), [{'call': 'size'}])
    add('shared-by-call', compound(), [{'call': 'add_shape', 'shape': None, 'shared': True}])
    for shared in (False, True):
        tag = 'shared' if shared else 'raw'
        add(f'flatten-{tag}', compound({'shape': compound(null), 'shared': shared}), [])
        add(f'flatten-after-circle-{tag}',
            compound({'shape': compound({'shape': circle, 'shared': False}, null),
                      'shared': shared}), [])
        add(f'flatten-after-arc-{tag}',
            compound({'shape': compound({'shape': arc, 'shared': False}, null),
                      'shared': shared}), [])
        add(f'flatten-by-call-{tag}', compound(),
            [{'call': 'add_shape', 'shape': compound(null), 'shared': shared}])
    add('clone', compound(null), [{'call': 'clone'}])
    return out


def build():
    rng = random.Random(SEED)
    corpus, controls = [], []
    stats, labels, deferrals = Counter(), Counter(), Counter()
    seen = set()
    for case, inp in seg_candidates(rng) + shape_candidates(rng):
        assert case not in seen, case
        seen.add(case)
        kind, lab, events = evaluate(inp)
        stats[kind] += 1
        request = {'case': case, 'input': inp, 'unit': UNIT}
        if kind == 'defined':
            for l in lab:
                labels[l] += 1
            deferrals[events[0] if events else 'none'] += 1
            corpus.append(request)
            continue
        control = {'error': UNDEFINED if kind == 'undefined' else OUTSIDE,
                   'request': {**request, 'case': f'control-{case}'}}
        if events:
            upstream, unit = DEFERRALS[events[0]]
            control['rust_unported'] = {'upstream': upstream, 'unit': unit}
        controls.append(control)
    return corpus, controls, stats, labels, deferrals


REQUIRED_LABELS = {
    'collinear-true', 'collinear-false', 'overlaps-point-end', 'overlaps-point',
    'overlaps-other-end-inside', 'overlaps-this-end-inside', 'overlaps-disjoint',
    'approx-overlaps-point-end', 'approx-overlaps-point', 'approx-overlaps-not-collinear',
    'approx-overlaps-this-end-inside', 'approx-overlaps-other-end-inside', 'approx-overlaps-disjoint',
    'contains-seg-point', 'contains-seg-true', 'contains-seg-false',
    'circle-collide-clear', 'circle-collide-zero', 'circle-collide-within',
    'circle-collide-location-intersection', 'circle-collide-location-nearest',
    'circle-collide-actual', 'circle-collide-actual-negative',
    'add-flatten', 'add-flatten-shared', 'add-shared-clone', 'add-owned',
    'circle-ctor-default', 'circle-ctor-center_radius', 'circle-ctor-circle', 'circle-ctor-copy',
    'arc-ctor-default', 'arc-ctor-three_point', 'ellipse-ctor-default', 'ellipse-ctor-radii',
    'ellipse-ctor-arc_radii', 'compound-ctor', 'add-null', 'shape-type-named',
    'shape-type-unnamed', 'circle-rotate', 'circle-after-rotate',
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
        ok = CASES.exists() and CONTROLS.exists() and \
            CASES.read_bytes() == text.encode() and CONTROLS.read_bytes() == ctext.encode()
        if not ok:
            sys.exit('generated corpus or controls differ from the committed files')
    else:
        CASES.write_bytes(text.encode())
        CONTROLS.write_bytes(ctext.encode())
    print(f'{len(corpus)} public cases, {len(controls)} controls '
          f'({stats["undefined"]} undefined, {stats["outside"]} outside the verified domain)')
    print('expected first Rust deferral per public case:', dict(sorted(deferrals.items())))
    print('branch labels:', len(labels))


if __name__ == '__main__':
    main()
