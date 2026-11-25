#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for the out-of-line EDA_ANGLE bodies; inputs only, no answers.

Python floats are IEEE binary64 with correctly rounded +, -, fmod and copysign, so the
mirror below decides which source branch each request reaches. It labels branches for
the evidence table and checks that every branch is reached; it never writes a result.
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
UNIT = 'libs.kimath.src.geometry.eda_angle.application.pure.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
SEED = 0xEDA0C9F


def bits(x):
    return struct.pack('>d', x).hex()


def value(hexbits):
    return struct.unpack('>d', bytes.fromhex(hexbits))[0]


def fold(v, period):
    if not math.isfinite(v):
        return 0.0, 'fold-nonfinite'
    if abs(v) < 2.0 * period:
        return v, 'fold-direct'
    return math.fmod(v, period) + math.copysign(period, v), 'fold-fmod'


def keep_upright(v):
    """Branch label of EDA_ANGLE::KeepUpright (Normalize, then the comparisons)."""
    n, folded = fold(v, 360.0)
    while n < -0.0:
        n += 360.0
    while n >= 360.0:
        n -= 360.0
    if n <= 45:
        branch = 'le45'
    elif n >= 315:
        branch = 'ge315'
    elif n > 135 and n <= 225:
        branch = 'gt135-le225'
    else:
        branch = 'vertical'
    return [f'normalize-{folded}', f'upright-{branch}']


def is_cardinal(v):
    if not math.isfinite(v):
        return ['nonfinite']
    test, folded = fold(v, 90.0)
    raised = lowered = 0
    while test < 0.0:
        test += 90.0
        raised += 1
    while test >= 90.0:
        test -= 90.0
        lowered += 1
    return [folded, f'raise-{min(raised, 2)}', f'lower-{min(lowered, 2)}',
            'true' if test == 0.0 else 'false']


def is_cardinal90(v):
    if not math.isfinite(v):
        return ['nonfinite']
    test, folded = fold(abs(v), 180.0)
    lowered = 0
    while test >= 180.0:
        test -= 180.0
        lowered += 1
    return [folded, f'lower-{min(lowered, 2)}', 'true' if test == 90.0 else 'false']


MIRRORS = {'keep_upright': keep_upright, 'is_cardinal': is_cardinal,
           'is_cardinal90': is_cardinal90}
REQUIRED = {
    'keep_upright': {'normalize-fold-nonfinite', 'normalize-fold-direct', 'normalize-fold-fmod',
                     'upright-le45', 'upright-ge315', 'upright-gt135-le225', 'upright-vertical'},
    'is_cardinal': {'nonfinite', 'fold-direct', 'fold-fmod', 'raise-0', 'raise-1', 'lower-0',
                    'lower-1', 'true', 'false'},
    'is_cardinal90': {'nonfinite', 'fold-direct', 'fold-fmod', 'lower-0', 'lower-1', 'true',
                      'false'},
}


def inputs():
    """Ordered unique bit patterns: specials, every boundary and its neighbours, random."""
    out = []
    seen = set()

    def add(h):
        if h not in seen:
            seen.add(h)
            out.append(h)

    def around(x):
        for v in (math.nextafter(x, -math.inf), x, math.nextafter(x, math.inf)):
            add(bits(v))

    for h in ['0000000000000000', '8000000000000000', '0000000000000001', '8000000000000001',
              '000fffffffffffff', '800fffffffffffff', '0010000000000000', '8010000000000000',
              '7fefffffffffffff', 'ffefffffffffffff', '7ff0000000000000', 'fff0000000000000',
              '7ff8000000000000', 'fff8000000000000', '7ff8000000000042', 'fff8000000000042',
              '7ff0000000000001', 'fff0000000000042', '7fffffffffffffff']:
        add(h)
    for x in [1e-300, -1e-300, 1e-320, -1e-320, 1e-15, -1e-15, 0.5, -0.5, 1.0, -1.0,
              1e15, -1e15, 2.0 ** 52, -2.0 ** 52, 2.0 ** 53, -2.0 ** 53, 1e20, -1e20,
              1e300, -1e300]:
        add(bits(x))
    # KeepUpright boundaries in every nearby turn, and the Normalize fold thresholds.
    for k in range(-3, 4):
        for b in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]:
            around(b + 360.0 * k)
    for t in [720.0, -720.0, 1440.0, -1440.0]:
        around(t)
    # Every multiple of 90 within the direct and fmod fold ranges of both cardinal tests.
    for k in range(-16, 17):
        around(90.0 * k)
    for t in [180.0, -180.0, 360.0, -360.0]:
        around(t)
    # Exactly representable huge multiples of 90 (cardinal) and their odd neighbours.
    for e in [10, 30, 52, 53, 60, 100, 500, 1000, 1016]:
        for m in [90.0, 180.0, 270.0, 45.0]:
            x = m * 2.0 ** e
            if math.isfinite(x):
                around(x)
                around(-x)
    # Remainders that land a fold exactly on a period or a hair below one.
    for x in [90.0 - 2 ** -46, -(90.0 - 2 ** -46), 180.0 - 2 ** -45, 270.0 + 2 ** -44,
              -1e-17, -2 ** -1074, 1e-17, 89.99999999999999, -89.99999999999999]:
        add(bits(x))
    rng = random.Random(SEED)
    for _ in range(384):
        add(f'{rng.getrandbits(64):016x}')
    for _ in range(384):
        add(bits(rng.uniform(-1080.0, 1080.0)))
    for _ in range(256):
        add(bits(rng.randint(-4096, 4096) * 0.5))
    for _ in range(128):
        add(bits(rng.randint(-2 ** 40, 2 ** 40) * 90.0 * rng.choice([1.0, 2.0 ** 30, 2.0 ** 60])))
    return out


def generate():
    cases = []
    labels = {op: Counter() for op in MIRRORS}
    for i, h in enumerate(inputs()):
        for op, mirror in MIRRORS.items():
            cases.append({'unit': UNIT, 'case': f'{op}-{i}-{h}', 'input': {'op': op, 'a': h}})
            labels[op].update(mirror(value(h)))
    for op, required in REQUIRED.items():
        missing = required - set(labels[op])
        if missing:
            raise SystemExit(f'{op}: corpus misses branches {sorted(missing)}')
    return cases, labels


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='verify the committed corpus')
    parser.add_argument('--branches', action='store_true', help='print branch evidence counts')
    args = parser.parse_args()
    cases, labels = generate()
    text = ''.join(json.dumps(c, separators=(',', ':'), sort_keys=True) + '\n' for c in cases)
    if args.branches:
        for op in MIRRORS:
            for label, count in sorted(labels[op].items()):
                print(f'{op}\t{label}\t{count}')
    if args.check:
        if CASES.read_text(encoding='utf-8') != text:
            sys.exit(f'{CASES} differs from the deterministic generator')
        print(f'{len(cases)} cases match')
    elif not args.branches:
        CASES.write_text(text, encoding='utf-8', newline='\n')
        print(f'wrote {len(cases)} cases to {CASES}')


if __name__ == '__main__':
    main()
