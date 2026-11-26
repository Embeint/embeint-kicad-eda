#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and undefined-domain controls for the ELLIPSE source unit.

Writes port/cases/<unit>.jsonl (defined operations only) and
port/oracle/fixtures/ellipse-controls.jsonl (requests whose pinned evaluation reaches an
undefined operation at a recorded observation). No result is computed here: the split only
uses exact integer arithmetic and single IEEE binary64 products (Python floats), never the
host libm. Integer GetPointAtAngle cases whose final addition is not provably in range are
not generated; the controls use angles and rotations whose offsets are exact.

--check verifies the committed files equal the generator output.
"""
import argparse
import json
import math
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.ellipse.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/ellipse-controls.jsonl'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
MAX_STEPS = 8


def bits(v):
    return struct.pack('>d', v).hex()


def raw(h):
    """A double given by its exact IEEE bits (NaN payloads, signalling NaNs)."""
    return struct.unpack('>d', bytes.fromhex(h))[0]


QNAN = '7ff8000000000000'
NEG_QNAN_PAYLOAD = 'fff8000000000123'
SNAN = '7ff4000000000abc'
QNAN_PAYLOAD = '7ff80000000000aa'
NANS = [QNAN, NEG_QNAN_PAYLOAD, SNAN, QNAN_PAYLOAD]

ROTATIONS = [0.0, -0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0, 360.0, -45.0, -90.0, -180.0,
             -270.0, 720.0, 1080.0 + 45.0, 30.0, 12.345678, -123.456, 1e-300, 359.99999999999994,
             1e17, -1e300, math.inf, -math.inf]
ANGLES = [0.0, -0.0, 30.0, 45.0, 60.0, 90.0, 120.0, 180.0, 210.0, 270.0, 300.0, 359.0, 360.0,
          -90.0, -360.0, 540.0, 0.1, 1e-200, 5e-324, 89.99999999999999, 1e10, 1e300, math.inf,
          -math.inf]
F64_POOL = [0.0, -0.0, 1.0, -1.0, 0.5, 3.0, -7.25, 1e-310, 5e-324, -5e-324, 1e300, -1e300,
            1.7976931348623157e308, 123456.789, -98765.4321, 2.0**53, 2.0**31, math.inf, -math.inf]


def fits(v):
    return INT_MIN <= v <= INT_MAX


def rounded_hypot(x, y):
    # RoundedHypot: floor root rounded to nearest, then EuclideanNorm's INT_MAX saturation.
    n = x * x + y * y
    f = math.isqrt(n)
    r = f + (1 if n - f * f > f else 0)
    return min(r, INT_MAX)


def trunc_fits(product):
    if math.isnan(product) or math.isinf(product):
        return False
    t = math.trunc(product)
    return -2**31 <= t < 2**31


class Model:
    """Tracks only what the split needs: int centres and radii (exact arithmetic)."""

    def __init__(self, center, major, minor, start=0.0, end=360.0):
        self.center = list(center)
        self.major = major
        self.minor = minor
        # Only finiteness of the stored arc angles is used (a non-finite angle makes
        # std::cos NaN, so KiROUND reaches its overflow logging); Mirror keeps it.
        self.finite_angles = [math.isfinite(start), math.isfinite(end)]


def construct_model(kind, c):
    if kind == 'f64':
        return None
    if c['form'] == 'default':
        return Model((0, 0), 0, 0)
    start = raw(c['start']) if 'start' in c else 0.0
    end = raw(c['end']) if 'end' in c else 360.0
    if c['form'] == 'radii':
        return Model(c['center'], c['major'], c['minor'], start, end)
    norm = rounded_hypot(*c['major'])
    product = float(norm) * raw(c['ratio'])
    if not trunc_fits(product):
        return 'undefined'
    return Model(c['center'], norm, math.trunc(product), start, end)


def classify(kind, construct, steps):
    """Returns None for a defined request, 'state' or the step index of the first undefined
    observation, or 'unknown' when integer bounds cannot prove either."""
    model = construct_model(kind, construct)
    if model == 'undefined':
        return 'state'
    # A control must reach its undefined observation without an earlier KiROUND overflow,
    # whose logging callee defers in Rust; a request that may defer first is not a control.
    may_defer = False
    for i, s in enumerate(steps):
        op = s['op']
        if kind == 'f64':
            if op == 'angle_at' and model_nan_radius(construct, steps[:i]):
                return i
            continue
        if op == 'mirror':
            axis = 0 if s['direction'] == 'left_right' else 1
            twice = 2 * s['ref'][axis]
            if not fits(twice) or not fits(twice - model.center[axis]):
                return 'unknown' if may_defer else i
            model.center[axis] = twice - model.center[axis]
            model.finite_angles.reverse()
        elif op in ('point', 'start_point', 'end_point'):
            if op == 'point':
                finite = math.isfinite(raw(s['angle']))
            else:
                finite = model.finite_angles[0 if op == 'start_point' else 1]
            may_defer = may_defer or not finite
            exact = s.get('_exact')
            if exact is not None:
                ox, oy = exact
                if not fits(model.center[0] + ox) or not fits(model.center[1] + oy):
                    return 'unknown' if may_defer else i
                continue
            bound = abs(model.major) + abs(model.minor) + 4
            may_defer = may_defer or bound > INT_MAX
            for axis in (0, 1):
                c = model.center[axis]
                if c != 0 and abs(c) + bound > INT_MAX:
                    return 'unknown'
    return None


def model_nan_radius(construct, _earlier):
    # Mirror never changes radii, so the constructed radii decide.
    if construct['form'] == 'radii':
        return math.isnan(raw(construct['major'])) or math.isnan(raw(construct['minor']))
    if construct['form'] == 'major':
        x, y = (raw(v) for v in construct['major'])
        ratio = raw(construct['ratio'])
        if abs(x) == abs(y):
            norm = abs(x) * 1.4142135623730951  # M_SQRT2
        elif x == 0:
            norm = abs(y)
        elif y == 0:
            norm = abs(x)
        else:
            # Full hypot now runs here. Only the NaN classification is needed;
            # no ellipse result or numeric norm is generated by this model.
            def signalling(value):
                word = int(bits(value), 16)
                return ((word & 0x7ff0000000000000) == 0x7ff0000000000000
                        and (word & 0xfffffffffffff) != 0
                        and (word & 0x8000000000000) == 0)
            if math.isinf(x) or math.isinf(y):
                if signalling(x) or signalling(y):
                    return True
                return math.isnan(math.inf * ratio)
            if math.isnan(x) or math.isnan(y):
                return True
            # A finite, nonzero general vector has a positive norm. Only a zero
            # ratio combined with an overflowing norm could add a NaN here.
            # Every generated finite general vector is safely below that bound.
            if ratio == 0.0:
                assert max(abs(x), abs(y)) <= float.fromhex('0x1.fffffffffffffp+1022')
            return math.isnan(ratio)
        return math.isnan(norm) or math.isnan(norm * ratio)
    return False


def strip(steps):
    return [{k: v for k, v in s.items() if not k.startswith('_')} for s in steps]


class Corpus:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.names = set()

    def add(self, name, kind, construct, steps):
        assert name not in self.names, name
        assert len(steps) <= MAX_STEPS, name
        self.names.add(name)
        where = classify(kind, construct, steps)
        if where == 'unknown':
            return
        request = {'unit': UNIT, 'case': name,
                   'input': {'op': 'ellipse', 'type': kind, 'construct': construct,
                             'steps': strip(steps)}}
        if where is None:
            self.cases.append(request)
        else:
            self.controls.append({'request': request, 'undefined': where,
                                  'error': {'kind': 'invalid_input',
                                            'message': 'undefined ellipse operation'}})


def enc(kind, v):
    return bits(v) if kind == 'f64' else v


def pt(kind, x, y):
    return [enc(kind, x), enc(kind, y)]


def angles_of(start=None, end=None):
    out = {}
    if start is not None:
        out['start'] = bits(start)
    if end is not None:
        out['end'] = bits(end)
    return out


def observe_all(kind, rng, n_points=2, n_angles=1):
    steps = [{'op': 'subtended'}, {'op': 'start_point'}, {'op': 'end_point'}]
    for _ in range(n_points):
        steps.append({'op': 'point', 'angle': bits(rng.choice(ANGLES))})
    for _ in range(n_angles):
        if kind == 'i32':
            steps.append({'op': 'angle_at', 'point': [rng.randint(-1000, 1000), rng.randint(-1000, 1000)]})
        else:
            steps.append({'op': 'angle_at', 'point': pt(kind, rng.uniform(-1e3, 1e3), rng.uniform(-1e3, 1e3))})
    return steps


def generate():
    rng = random.Random(0x0E111B5E)
    c = Corpus()

    # Default ELLIPSE and every observation on it.
    for kind in ('i32', 'f64'):
        zero = 0 if kind == 'i32' else 0.0
        for i, a in enumerate(ANGLES):
            c.add(f'default-{kind}-point-{i}', kind, {'form': 'default'},
                  [{'op': 'point', 'angle': bits(a)}, {'op': 'start_point'}, {'op': 'end_point'},
                   {'op': 'subtended'}])
        for i, (x, y) in enumerate([(0, 0), (1, 0), (0, 1), (-1, -1), (5, -3)]):
            c.add(f'default-{kind}-angle-at-{i}', kind, {'form': 'default'},
                  [{'op': 'angle_at', 'point': pt(kind, type(zero)(x), type(zero)(y))}])
        for d in ('left_right', 'top_bottom'):
            c.add(f'default-{kind}-mirror-{d}', kind, {'form': 'default'},
                  [{'op': 'mirror', 'ref': pt(kind, type(zero)(3), type(zero)(-4)), 'direction': d},
                   {'op': 'subtended'}, {'op': 'start_point'}, {'op': 'end_point'}])

    # Radii constructor, all three default-argument forms, special rotations and angles.
    for kind in ('i32', 'f64'):
        for i, rot in enumerate(ROTATIONS):
            for form, extra in [('defaults', {}), ('start', angles_of(rng.choice(ANGLES))),
                                ('both', angles_of(rng.choice(ANGLES), rng.choice(ANGLES)))]:
                if kind == 'i32':
                    center = [rng.randint(-10**6, 10**6), rng.randint(-10**6, 10**6)]
                    radii = (rng.randint(0, 10**6), rng.randint(-10, 10**6))
                else:
                    center = pt(kind, rng.uniform(-1e6, 1e6), rng.uniform(-1e6, 1e6))
                    radii = (bits(rng.uniform(0, 1e6)), bits(rng.uniform(-10, 1e6)))
                construct = {'form': 'radii', 'center': center, 'major': radii[0],
                             'minor': radii[1], 'rotation': bits(rot), **extra}
                c.add(f'radii-{kind}-rot{i}-{form}', kind, construct, observe_all(kind, rng, 3, 2))

    # Every pair of start/end angles for the subtended normalization and endpoints.
    for kind in ('i32', 'f64'):
        for i, s in enumerate(ANGLES):
            for j, e in enumerate(ANGLES[::3]):
                center = [100, -200] if kind == 'i32' else pt(kind, 100.0, -200.0)
                major = 5000 if kind == 'i32' else bits(5000.0)
                minor = 2500 if kind == 'i32' else bits(2500.0)
                c.add(f'arc-{kind}-{i}-{j}', kind,
                      {'form': 'radii', 'center': center, 'major': major, 'minor': minor,
                       'rotation': bits(30.0), **angles_of(s, e)},
                      [{'op': 'subtended'}, {'op': 'start_point'}, {'op': 'end_point'}])

    # Fixed-seed integer ellipses across magnitudes, with mirror sequences.
    for i in range(600):
        scale = rng.choice([10, 1000, 10**6, 10**8, 10**9, 2**31 - 1])
        center = [rng.randint(-scale, scale), rng.randint(-scale, scale)]
        if rng.random() < 0.2:
            center[rng.randint(0, 1)] = 0
        rscale = rng.choice([1, 100, 10**5, 10**8, 10**9, 2**31 - 1])
        major = rng.randint(-rscale // 10, rscale)
        minor = rng.randint(-rscale // 10, rscale)
        rot = rng.choice(ROTATIONS + [rng.uniform(-720, 720) for _ in range(4)])
        construct = {'form': 'radii', 'center': center, 'major': major, 'minor': minor,
                     'rotation': bits(rot), **angles_of(rng.uniform(-400, 400), rng.uniform(-400, 400))}
        steps = []
        for _ in range(rng.randint(1, MAX_STEPS)):
            r = rng.random()
            if r < 0.25:
                ref_scale = rng.choice([10, 10**6, 2**30 - 1])
                steps.append({'op': 'mirror', 'ref': [rng.randint(-ref_scale, ref_scale),
                                                      rng.randint(-ref_scale, ref_scale)],
                              'direction': rng.choice(['left_right', 'top_bottom'])})
            elif r < 0.5:
                steps.append({'op': 'point', 'angle': bits(rng.choice([rng.uniform(-720, 720)] + ANGLES))})
            elif r < 0.65:
                steps.append({'op': rng.choice(['start_point', 'end_point'])})
            elif r < 0.8:
                steps.append({'op': 'subtended'})
            else:
                steps.append({'op': 'angle_at', 'point': [rng.randint(INT_MIN, INT_MAX),
                                                          rng.randint(INT_MIN, INT_MAX)]})
        c.add(f'random-i32-{i}', 'i32', construct, steps)

    # Fixed-seed double ellipses, including non-finite and NaN-payload members.
    def f64_value():
        r = rng.random()
        if r < 0.1:
            return raw(rng.choice(NANS))
        if r < 0.3:
            return rng.choice(F64_POOL)
        return rng.uniform(-1e7, 1e7) * rng.choice([1e-9, 1.0, 1e3])

    for i in range(600):
        nan_radius = True
        while nan_radius:
            major, minor = f64_value(), f64_value()
            nan_radius = math.isnan(major) or math.isnan(minor)
            if rng.random() < 0.15:
                break  # Keep some NaN-radius ellipses for every other observation.
        center = [f64_value(), f64_value()]
        rot = rng.choice(ROTATIONS + [rng.uniform(-720, 720)])
        construct = {'form': 'radii', 'center': pt('f64', *center), 'major': bits(major),
                     'minor': bits(minor), 'rotation': bits(rot),
                     **angles_of(rng.choice(ANGLES + [rng.uniform(-400, 400)]),
                                 rng.choice(ANGLES + [rng.uniform(-400, 400)]))}
        has_nan = math.isnan(major) or math.isnan(minor)
        steps = []
        for _ in range(rng.randint(1, MAX_STEPS)):
            r = rng.random()
            if r < 0.25:
                steps.append({'op': 'mirror', 'ref': pt('f64', f64_value(), f64_value()),
                              'direction': rng.choice(['left_right', 'top_bottom'])})
            elif r < 0.5:
                steps.append({'op': 'point', 'angle': bits(rng.choice([rng.uniform(-720, 720)] + ANGLES))})
            elif r < 0.65:
                steps.append({'op': rng.choice(['start_point', 'end_point'])})
            elif r < 0.8:
                steps.append({'op': 'subtended'})
            elif not has_nan:
                steps.append({'op': 'angle_at', 'point': pt('f64', f64_value(), f64_value())})
        c.add(f'random-f64-{i}', 'f64', construct, steps)

    # GetAngleAtPoint radius floors: radii below, at and above 1, negative and zero.
    for kind in ('i32', 'f64'):
        for i, (a, b) in enumerate([(0, 0), (1, 1), (2, 0), (0, 2), (-5, 3), (3, -5), (1, 100),
                                    (100, 1), (INT_MAX, 1), (1, INT_MAX), (INT_MIN, INT_MIN)]):
            for j, (x, y) in enumerate([(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1), (3, 4),
                                        (INT_MAX, INT_MIN), (INT_MIN, INT_MAX)]):
                if kind == 'i32':
                    construct = {'form': 'radii', 'center': [0, 0], 'major': a, 'minor': b,
                                 'rotation': bits(rng.choice(ROTATIONS))}
                    point = [x, y]
                else:
                    construct = {'form': 'radii', 'center': pt(kind, 0.0, 0.0),
                                 'major': bits(float(a) / 7.0), 'minor': bits(float(b) / 3.0),
                                 'rotation': bits(rng.choice(ROTATIONS))}
                    point = pt(kind, float(x), float(y))
                c.add(f'angle-at-{kind}-{i}-{j}', kind, construct,
                      [{'op': 'angle_at', 'point': point}])

    # Major-axis constructor: zero, axis, diagonal, saturated and general vectors.
    majors_i32 = [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1), (3, 4), (-3, 4), (5, 5), (-7, 7),
                  (INT_MAX, 0), (INT_MIN, 0), (0, INT_MIN), (INT_MAX, INT_MAX), (INT_MIN, INT_MIN),
                  (INT_MIN, INT_MAX), (1, 1), (2, 1), (10**9, 10**9 + 1), (123456, -654321)]
    ratios = [0.0, -0.0, 0.5, 1.0, 2.0, -1.0, 1e-300, 0.999999999, 1.0000000000000002, 1e300,
              -1e300, math.inf, -math.inf, raw(QNAN), raw(SNAN), 0.4999999999999999]
    for i, m in enumerate(majors_i32):
        for j, ratio in enumerate(ratios):
            extra = [{}, angles_of(10.0), angles_of(-30.0, 200.0)][(i + j) % 3]
            c.add(f'major-i32-{i}-{j}', 'i32',
                  {'form': 'major', 'center': [rng.randint(-1000, 1000), rng.randint(-1000, 1000)],
                   'major': list(m), 'ratio': bits(ratio), **extra},
                  [{'op': 'subtended'}, {'op': 'point', 'angle': bits(45.0)},
                   {'op': 'angle_at', 'point': [7, -9]}])
    for _ in range(100):
        m = (rng.randint(-2**31, 2**31 - 1), rng.randint(-2**31, 2**31 - 1))
        ratio = rng.choice([rng.random(), rng.uniform(-3, 3), rng.choice(ratios)])
        c.add(f'major-i32-random-{len(c.names)}', 'i32',
              {'form': 'major', 'center': [0, 0], 'major': list(m), 'ratio': bits(ratio)},
              [{'op': 'start_point'}, {'op': 'end_point'}])
    majors_f64 = [(0.0, 0.0), (-0.0, 0.0), (2.0, 0.0), (0.0, -3.0), (-0.0, 4.5), (5.0, 5.0),
                  (-5.0, 5.0), (1e-310, -1e-310), (1e300, 1e300), (math.inf, -math.inf),
                  (math.inf, 0.0), (0.0, raw(QNAN)), (raw(NEG_QNAN_PAYLOAD), 0.0), (3.0, 4.0),
                  (1.0, 2.0), (raw(QNAN), raw(QNAN)), (-1e-3, 7.0)]
    for i, m in enumerate(majors_f64):
        for j, ratio in enumerate(ratios[::2] + [raw(QNAN_PAYLOAD)]):
            c.add(f'major-f64-{i}-{j}', 'f64',
                  {'form': 'major', 'center': pt('f64', 1.5, -2.5), 'major': pt('f64', *m),
                   'ratio': bits(ratio), **([{}, angles_of(90.0), angles_of(-0.0, 1e300)][(i + j) % 3])},
                  [{'op': 'subtended'}, {'op': 'start_point'}, {'op': 'end_point'},
                   {'op': 'angle_at', 'point': pt('f64', 3.0, 4.0)}])

    # Mirror boundaries: exact int results at the limits (defined) beside the controls below.
    for i, (ref, center) in enumerate([(2**30 - 1, 0), (-2**30, 0), (2**30 - 1, -1), (-2**30, -1),
                                       (0, INT_MIN + 1), (0, INT_MAX), (2**30 - 1, INT_MAX),
                                       (-2**30, INT_MIN), (5, 10)]):
        for d in ('left_right', 'top_bottom'):
            ctr = [center, center]
            c.add(f'mirror-i32-limit-{i}-{d}', 'i32',
                  {'form': 'radii', 'center': ctr, 'major': 3, 'minor': 2, 'rotation': bits(60.0),
                   **angles_of(15.0, 75.0)},
                  [{'op': 'mirror', 'ref': [ref, ref], 'direction': d}, {'op': 'subtended'},
                   {'op': 'mirror', 'ref': [ref, ref], 'direction': d}])

    # KiROUND saturation at the origin (defined in C++; the logging callee defers in Rust).
    for i, (rot, ang) in enumerate([(45.0, 45.0), (45.0, 0.0), (0.0, 0.0), (180.0, 0.0),
                                    (30.0, 77.0), (math.inf, 0.0)]):
        c.add(f'kiround-i32-{i}', 'i32',
              {'form': 'radii', 'center': [0, 0], 'major': INT_MAX, 'minor': INT_MAX,
               'rotation': bits(rot)},
              [{'op': 'point', 'angle': bits(ang)}])
        c.add(f'kiround-i32-neg-{i}', 'i32',
              {'form': 'radii', 'center': [0, 0], 'major': INT_MIN, 'minor': INT_MIN,
               'rotation': bits(rot)},
              [{'op': 'point', 'angle': bits(ang)}])

    # Exact-offset end points at the int limits: rotation 0 and angle 0 give (Major, 0);
    # angle 180 gives (-Major, 0). Defined when the sum fits, controls when it does not.
    for i, (cx, major) in enumerate([(INT_MAX - 10, 10), (INT_MAX - 10, 11), (INT_MAX, 0),
                                     (INT_MAX, 1), (0, INT_MAX), (1, INT_MAX), (INT_MIN + 5, 5),
                                     (INT_MIN + 5, 6), (INT_MIN, 0), (INT_MIN, 1), (-1, INT_MAX)]):
        for ang, sign in ((0.0, 1), (180.0, -1)):
            c.add(f'limit-point-i32-{i}-{int(ang)}', 'i32',
                  {'form': 'radii', 'center': [cx, 0], 'major': major, 'minor': 1,
                   'rotation': bits(0.0), **angles_of(ang)},
                  [{'op': 'subtended'},
                   {'op': 'point', 'angle': bits(ang), '_exact': (sign * major, 0)},
                   {'op': 'start_point', '_exact': (sign * major, 0)},
                   {'op': 'end_point', '_exact': (major, 0)}])

    # Undefined mirror controls (int overflow of 2 * aRef or of the subtraction).
    for i, (ref, center) in enumerate([(2**30, 0), (-2**30 - 1, 0), (2**30 - 1, -2),
                                       (-2**30, 1), (0, INT_MIN), (INT_MAX, 0), (INT_MIN, 0)]):
        for d in ('left_right', 'top_bottom'):
            c.add(f'control-mirror-i32-{i}-{d}', 'i32',
                  {'form': 'radii', 'center': [center, center], 'major': 3, 'minor': 2,
                   'rotation': bits(60.0)},
                  [{'op': 'subtended'}, {'op': 'mirror', 'ref': [ref, ref], 'direction': d},
                   {'op': 'subtended'}])

    # Undefined NaN-radius GetAngleAtPoint controls (double only).
    for i, (a, b) in enumerate([(raw(QNAN), 1.0), (1.0, raw(SNAN)), (raw(NEG_QNAN_PAYLOAD), raw(QNAN))]):
        c.add(f'control-angle-at-f64-{i}', 'f64',
              {'form': 'radii', 'center': pt('f64', 0.0, 0.0), 'major': bits(a), 'minor': bits(b),
               'rotation': bits(10.0)},
              [{'op': 'point', 'angle': bits(10.0)}, {'op': 'angle_at', 'point': pt('f64', 1.0, 1.0)},
               {'op': 'subtended'}])
    return c


def lines(items):
    return ''.join(json.dumps(x, separators=(',', ':')) + '\n' for x in items)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    c = generate()
    outputs = {CASES: lines(c.cases), CONTROLS: lines(c.controls)}
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
