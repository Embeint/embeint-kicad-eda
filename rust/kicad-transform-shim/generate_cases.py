#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the fixed-seed public corpus and undefined-domain controls for
libs.kimath.src.transform.application.ui.p001 (libs/kimath/src/transform.cpp).

Every request is classified here with exact Python integers: a request whose pinned C++
evaluation is defined goes to port/cases/<unit>.jsonl; one whose evaluation would overflow
a signed integer or divide by zero goes to port/oracle/fixtures/transform-controls.jsonl
with the refusal both programs must report. The classification follows the Rust body's
check order (it only matters for InverseTransform, where several undefined operations can
coincide). --check verifies the committed files are byte-identical to a fresh generation;
--branches prints how many public cases reach each source branch.
"""
import argparse
import json
import random
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.transform.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/transform-controls.jsonl'
SEED = 0x7472616E  # "tran"
I32_MIN, I32_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1
OVERFLOW = {'kind': 'invalid_input', 'message': 'undefined integer overflow'}
DIVISION = {'kind': 'invalid_input', 'message': 'undefined division by zero'}

# The eight symbol orientations KiCad builds (identity, rotations, X/Y mirrors and their
# combinations) followed by degenerate and non-unimodular matrices.
ORIENTATIONS = [
    [1, 0, 0, 1], [0, 1, -1, 0], [-1, 0, 0, -1], [0, -1, 1, 0],
    [1, 0, 0, -1], [-1, 0, 0, 1], [0, 1, 1, 0], [0, -1, -1, 0],
]
SPECIAL = [
    [0, 0, 0, 0], [1, 1, 1, 1], [2, 0, 0, 2], [3, 1, 2, 1], [5, 3, 3, 2], [7, 2, -3, 4],
    [-2, 0, 0, 3], [1, 2, 3, 4], [I32_MAX, 0, 0, 1], [I32_MIN, 0, 0, 1], [1, 0, 0, I32_MIN],
    [I32_MAX, I32_MAX, I32_MAX, I32_MAX], [I32_MIN, I32_MIN, I32_MIN, I32_MIN],
    [46340, 0, 0, 46340], [46341, 1, 1, 46341], [65536, 0, 0, 32768],
]
EDGE_INTS = [0, 1, -1, 2, -2, 7, -7, 46340, -46340, 46341, 65535, 65536, -65536,
             1073741823, 1073741824, -1073741824, I32_MAX - 1, I32_MAX, I32_MIN + 1, I32_MIN]


def i32(v):
    return I32_MIN <= v <= I32_MAX


def wrap32(v):
    v &= 0xFFFFFFFF
    return v - 2**32 if v >= 2**31 else v


def trunc_div(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def point_error(t, p):
    terms = [t[0] * p[0], t[1] * p[1], t[2] * p[0], t[3] * p[1]]
    if not all(map(i32, terms)) or not i32(terms[0] + terms[1]) or not i32(terms[2] + terms[3]):
        return OVERFLOW
    return None


def box_error(t, origin, size):
    if point_error(t, origin):
        return OVERFLOW
    right, bottom = origin[0] + size[0], origin[1] + size[1]
    if not (I64_MIN <= right <= I64_MAX and I64_MIN <= bottom <= I64_MAX):
        return OVERFLOW
    return point_error(t, [wrap32(right), wrap32(bottom)])


def inverse_error(t):
    x1, y1, x2, y2 = t
    a, b = x1 * y2, x2 * y1
    if not i32(a) or not i32(b) or not i32(a - b):
        return OVERFLOW
    det = a - b
    for numerator in (y2, -x2 if x2 != I32_MIN else None, -y1 if y1 != I32_MIN else None, x1):
        if numerator is None:
            return OVERFLOW
        if det == 0:
            return DIVISION
        if numerator == I32_MIN and det == -1:
            return OVERFLOW
    return None


def hexbits(x):
    return struct.pack('>d', x).hex()


def frombits(b):
    # Formatted directly so NaN payloads and signalling bits never pass through a float.
    return f'{b:016x}'


ANGLE_BITS = [
    hexbits(v) for v in [
        0.0, -0.0, 0.1, -0.1, 0.05, 1.0, 10.0, 30.0, 44.99999999999999, 45.0, 60.0, 89.9, 90.0,
        90.1, 135.0, 179.9, 180.0, 180.1, 225.0, 270.0, 315.0, 359.9, 359.99999999999994, 360.0,
        360.1, 450.0, 720.0, 721.0, -45.0, -90.0, -180.0, -270.0, -359.9, -360.0, -720.5,
        1e-300, 5e-324, 1e16, -1e16, 1e300, -1e300, 1.7976931348623157e308,
        float('inf'), float('-inf'),
    ]
] + [frombits(0x7ff8000000000000), frombits(0xfff8000000000000),
     frombits(0x7ff00000000000a5), frombits(0x7ff80000000000cd)]


def request(case, data):
    return {'unit': UNIT, 'case': case, 'input': data}


def generate():
    rng = random.Random(SEED)
    public, controls = [], []

    def add(case, data, error):
        if error is None:
            public.append(request(case, data))
        else:
            controls.append({'error': error, 'request': request(case, data)})

    def rand_t(scale):
        return [rng.randint(-scale, scale) for _ in range(4)]

    def full_t():
        return [rng.randint(I32_MIN, I32_MAX) for _ in range(4)]

    matrices = ORIENTATIONS + SPECIAL

    # operator==: every subset of differing members, on several bases, plus random pairs.
    bases = ORIENTATIONS[:4] + [[I32_MIN, I32_MAX, 0, -1], [I32_MAX, I32_MIN, I32_MIN, I32_MAX]]
    for bi, base in enumerate(bases):
        for mask in range(16):
            other = [(v ^ 1) if mask >> k & 1 else v for k, v in enumerate(base)]
            add(f'eq-mask-{bi}-{mask}', {'op': 'eq', 'a': base, 'b': other}, None)
    for i in range(64):
        a = full_t() if i % 2 else rand_t(3)
        b = list(a) if i % 4 == 0 else (full_t() if i % 2 else rand_t(3))
        add(f'eq-random-{i}', {'op': 'eq', 'a': a, 'b': b}, None)

    # std::hash<TRANSFORM>: every listed matrix, edge-value matrices and random matrices.
    for i, t in enumerate(matrices):
        add(f'hash-matrix-{i}', {'op': 'hash', 't': t}, None)
    for i, v in enumerate(EDGE_INTS):
        for k in range(4):
            t = [0, 0, 0, 0]
            t[k] = v
            add(f'hash-edge-{i}-{k}', {'op': 'hash', 't': t}, None)
    for i in range(96):
        add(f'hash-random-{i}', {'op': 'hash', 't': full_t()}, None)

    # TransformCoordinate( VECTOR2I ): orientations on edge points, products at the int
    # limits, random matrices on random points.
    edge_points = [[x, y] for x in EDGE_INTS for y in (0, 1, -1, I32_MAX, I32_MIN)]
    for ti, t in enumerate(matrices):
        for pi, p in enumerate(edge_points):
            add(f'point-edge-{ti}-{pi}', {'op': 'point', 't': t, 'p': p}, point_error(t, p))
    for i in range(400):
        mode = i % 4
        if mode == 0:
            t, p = rand_t(2), [rng.randint(I32_MIN, I32_MAX) for _ in range(2)]
        elif mode == 1:
            t, p = full_t(), [rng.randint(-1, 1) for _ in range(2)]
        elif mode == 2:
            t, p = rand_t(46341), [rng.randint(-46341, 46341) for _ in range(2)]
        else:
            t, p = full_t(), [rng.randint(I32_MIN, I32_MAX) for _ in range(2)]
        add(f'point-random-{i}', {'op': 'point', 't': t, 'p': p}, point_error(t, p))

    # TransformCoordinate( BOX2I ): orientations on normal, negative, empty, wrapping and
    # int64-extreme boxes.
    boxes = [
        ([0, 0], [0, 0]), ([10, 20], [5, 7]), ([10, 20], [-5, -7]), ([-100, 50], [300, -25]),
        ([I32_MAX, 0], [10, 0]), ([I32_MIN, I32_MIN], [-1, -1]), ([0, 0], [2**32, 2**32 + 5]),
        ([0, 0], [I32_MAX, I32_MAX]), ([1, 1], [I64_MAX - 1, I64_MAX - 1]),
        ([-1, -1], [I64_MIN + 1, I64_MIN + 1]), ([5, -5], [I64_MAX, 0]), ([-5, 5], [0, I64_MIN]),
        ([1, 0], [I64_MAX, 0]), ([0, -1], [0, I64_MIN]), ([I32_MIN, I32_MAX], [I64_MAX, I64_MIN]),
    ]
    for ti, t in enumerate(matrices):
        for bi, (origin, size) in enumerate(boxes):
            add(f'box-edge-{ti}-{bi}', {'op': 'box', 't': t, 'origin': origin, 'size': size},
                box_error(t, origin, size))
    for i in range(300):
        mode = i % 3
        t = rng.choice(ORIENTATIONS) if mode == 0 else rand_t(3)
        origin = [rng.randint(I32_MIN, I32_MAX) if mode == 2 else rng.randint(-10**6, 10**6)
                  for _ in range(2)]
        size = [rng.randint(I64_MIN, I64_MAX) if mode == 2 else rng.randint(-10**6, 10**6)
                for _ in range(2)]
        add(f'box-random-{i}', {'op': 'box', 't': t, 'origin': origin, 'size': size},
            box_error(t, origin, size))

    # InverseTransform: orientations, unimodular and general matrices (truncating
    # quotients), negative determinants, singular and overflowing matrices.
    for i, t in enumerate(matrices):
        add(f'inverse-matrix-{i}', {'op': 'inverse', 't': t}, inverse_error(t))
    inverse_specials = [
        [0, 1, 1, 0], [2, 1, 1, 1], [1, 1, 0, 1], [-3, 5, 2, -3], [10, 3, 7, 2], [4, 0, 0, 4],
        [I32_MIN, 0, 0, -1], [I32_MIN, 1, 0, -1], [1, 0, 0, -1], [-1, 0, 1, 1],
        [I32_MAX, 0, 1, 1], [1, I32_MAX, 0, 1], [1, 0, I32_MAX, 1], [0, I32_MIN, 1, 0],
        [0, 1, I32_MIN, 0], [1, 2, 2, 4], [0, 0, 1, 1], [I32_MAX, I32_MIN, 1, -1],
        [-1, 0, 0, 1], [I32_MIN, 0, 0, 1], [1, 0, 0, I32_MIN], [-1, 0, 0, I32_MIN],
    ]
    for i, t in enumerate(inverse_specials):
        add(f'inverse-special-{i}', {'op': 'inverse', 't': t}, inverse_error(t))
    for i in range(300):
        t = rand_t(4) if i % 3 == 0 else (rand_t(46341) if i % 3 == 1 else full_t())
        add(f'inverse-random-{i}', {'op': 'inverse', 't': t}, inverse_error(t))

    # MapAngles: every listed matrix on paired edge angles, random angles and matrices.
    pairs = [(a, b) for a in ANGLE_BITS[::3] for b in ANGLE_BITS[1::4]]
    for ti, t in enumerate(matrices):
        for pi, (a1, a2) in enumerate(pairs):
            add(f'map-edge-{ti}-{pi}', {'op': 'map_angles', 't': t, 'a1': a1, 'a2': a2}, None)
    for i in range(600):
        mode = i % 4
        t = rng.choice(ORIENTATIONS) if mode < 2 else (rand_t(5) if mode == 2 else full_t())
        if mode == 1:
            a1, a2 = (hexbits(rng.choice([0, 45, 90, 135, 180, 225, 270, 315, -90, 360, 405]) * 1.0)
                      for _ in range(2))
        elif mode == 3:
            a1, a2 = (frombits(rng.getrandbits(64)) for _ in range(2))
        else:
            a1, a2 = (hexbits(rng.uniform(-1080.0, 1080.0)) for _ in range(2))
        add(f'map-random-{i}', {'op': 'map_angles', 't': t, 'a1': a1, 'a2': a2}, None)

    return public, controls


def lines(items):
    return ''.join(json.dumps(v, sort_keys=True, separators=(',', ':')) + '\n' for v in items)


def branches(public):
    counts = {}

    def hit(name):
        counts[name] = counts.get(name, 0) + 1

    for r in public:
        d = r['input']
        op = d['op']
        if op == 'eq':
            a, b = d['a'], d['b']
            first = next((k for k in range(4) if a[k] != b[k]), None)
            hit(f'eq-{"equal" if first is None else "first-diff-" + "x1 y1 x2 y2".split()[first]}')
        elif op == 'box':
            o, s = d['origin'], d['size']
            end = [o[0] + s[0], o[1] + s[1]]
            hit('box-end-narrowing-wraps' if any(not i32(v) for v in end) else 'box-end-fits')
            hit('box-identity' if d['t'] == ORIENTATIONS[0] else 'box-non-identity')
        elif op == 'inverse':
            t = d['t']
            det = t[0] * t[3] - t[2] * t[1]
            hit('inverse-det-negative' if det < 0 else 'inverse-det-positive')
            hit('inverse-unimodular' if abs(det) == 1 else 'inverse-truncating')
        else:
            hit(op)
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--branches', action='store_true')
    args = parser.parse_args()
    public, controls = generate()
    names = [r['case'] for r in public] + [c['request']['case'] for c in controls]
    assert len(set(names)) == len(names), 'duplicate case id'
    outputs = {CASES: lines(public), CONTROLS: lines(controls)}
    if args.branches:
        for name, n in sorted(branches(public).items()):
            print(f'{n:6d} {name}')
    if args.check:
        stale = [str(p.relative_to(ROOT)) for p, text in outputs.items()
                 if not p.exists() or p.read_bytes() != text.encode()]
        if stale:
            sys.exit(f'stale generated files: {stale}')
        print(f'{len(public)} public cases and {len(controls)} controls are current')
        return
    for path, text in outputs.items():
        path.write_bytes(text.encode())
    print(f'wrote {len(public)} public cases and {len(controls)} controls')


if __name__ == '__main__':
    main()
