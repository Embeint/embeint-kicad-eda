#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for ARC_CHORD_PARAMS; inputs only, no expected answers.

Python floats are IEEE binary64 with correctly rounded +, -, *, / and sqrt, so the
mirror below follows Compute's member updates and decides which source branch each
request reaches (including the branch EDA_ANGLE( VECTOR2D ) takes for the start and
end directions). It labels branches for the evidence table and checks that every
reachable branch is hit; it never writes a result.
"""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import random
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.arc_chord_params.application.pure.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
SEED = 0xA4C0C9F
INT_MIN, INT_MAX = -2 ** 31, 2 ** 31 - 1
MAX_COMPUTES = 8


def div(a, b):
    try:
        return a / b
    except ZeroDivisionError:
        if a == 0 or math.isnan(a):
            return math.nan
        return math.copysign(math.inf, a) * math.copysign(1.0, b)


class Mirror:
    """Member-by-member model of the pinned class, for branch labels only."""

    def __init__(self):
        self.chord = self.half = self.ux = self.uy = self.nx = self.ny = 0.0
        self.sag = self.rad = self.off = self.midx = self.midy = 0.0
        self.valid = False

    def compute(self, s, m, e, labels):
        self.valid = False
        dx = float(e[0]) - s[0]
        dy = float(e[1]) - s[1]
        self.chord = math.sqrt(dx * dx + dy * dy)
        if self.chord <= 0.0:
            labels.add('compute-zero-chord')
            return
        mx = float(m[0]) - s[0]
        my = float(m[1]) - s[1]
        cross = mx * dy - my * dx
        if cross == 0.0:
            labels.add('compute-collinear')
            return
        self.sag = abs(cross) / self.chord
        if self.sag <= 0.0:
            labels.add('compute-sagitta-nonpositive')
            return
        self.half = self.chord / 2.0
        self.rad = (self.half * self.half + self.sag * self.sag) / (2.0 * self.sag)
        if self.rad <= 0.0:
            labels.add('compute-radius-nonpositive')
            return
        self.ux = dx / self.chord
        self.uy = dy / self.chord
        self.nx = -self.uy
        self.ny = self.ux
        labels.add('compute-valid')
        if cross < 0.0:
            self.nx = -self.nx
            self.ny = -self.ny
            labels.add('normal-flipped')
        else:
            labels.add('normal-kept')
        self.off = self.rad - self.sag
        self.midx = (float(s[0]) + e[0]) * 0.5
        self.midy = (float(s[1]) + e[1]) * 0.5
        self.valid = True

    def observe(self, labels):
        labels.add('observe-' + ('valid' if self.valid else 'invalid'))
        sin_half = div(self.half, self.rad)
        cos_half = div(self.off, self.rad)
        start = ((-sin_half) * self.ux - cos_half * self.nx, (-sin_half) * self.uy - cos_half * self.ny)
        end = (sin_half * self.ux - cos_half * self.nx, sin_half * self.uy - cos_half * self.ny)
        labels.add('start-' + vector_branch(*start))
        labels.add('end-' + vector_branch(*end))
        ratio = div(self.half, self.rad)
        if ratio < 0.0:
            labels.add('clamp-lower')
        elif 1.0 < ratio:
            labels.add('clamp-upper')
        elif math.isnan(ratio):
            labels.add('clamp-nan')
        else:
            labels.add('clamp-inside')
        labels.add('arc-reflex' if self.sag > self.rad else 'arc-minor')


def vector_branch(x, y):
    """Which EDA_ANGLE( const VECTOR2D& ) branch a direction takes."""
    if x == 0.0 and y == 0.0:
        return 'zero'
    if y == 0.0:
        return 'x-axis'
    if x == 0.0:
        return 'y-axis'
    if x == y:
        return 'diagonal'
    if x == -y:
        return 'antidiagonal'
    return 'atan2'


REQUIRED = {'compute-zero-chord', 'compute-collinear', 'compute-valid', 'normal-flipped',
            'normal-kept', 'observe-valid', 'observe-invalid', 'clamp-nan', 'clamp-inside',
            'clamp-upper', 'arc-reflex', 'arc-minor'}
REQUIRED |= {f'{w}-{b}' for w in ('start', 'end')
             for b in ('x-axis', 'y-axis', 'diagonal', 'antidiagonal', 'atan2')}
# For 32-bit integer points these source refusals cannot be reached: a nonzero cross
# product of integer-valued doubles is at least 1, so sagitta and radius are positive.
# A ratio is a non-negative quotient or NaN, so the lower clamp is unreachable too, and
# a valid unit direction is never the zero vector.
UNREACHABLE = {'compute-sagitta-nonpositive', 'compute-radius-nonpositive', 'clamp-lower'}


def generate():
    cases = []
    labels = Counter()

    def add(name, computes):
        if not 1 <= len(computes) <= MAX_COMPUTES:
            raise SystemExit(f'{name}: compute count')
        for triple in computes:
            for p in triple:
                if not all(INT_MIN <= c <= INT_MAX for c in p):
                    raise SystemExit(f'{name}: point out of range')
        mirror = Mirror()
        seen = set()
        for s, m, e in computes:
            mirror.compute(s, m, e, seen)
            mirror.observe(seen)
        labels.update(seen)
        cases.append({'unit': UNIT, 'case': name,
                      'input': {'op': 'compute', 'computes': [[list(p) for p in t] for t in computes]}})

    cases.append({'unit': UNIT, 'case': 'default-state', 'input': {'op': 'default'}})
    Mirror().observe(seen := set())
    labels.update(seen)

    # Degenerate: zero chord (any mid) and collinear mids, including mid at an endpoint.
    for i, (s, m) in enumerate([((0, 0), (0, 0)), ((0, 0), (5, 5)), ((7, -3), (1, 2)),
                                ((INT_MIN, INT_MIN), (INT_MAX, INT_MAX)),
                                ((INT_MAX, INT_MIN), (0, 0))]):
        add(f'zero-chord-{i}', [(s, m, s)])
    for i, (s, m, e) in enumerate([((0, 0), (5, 0), (10, 0)), ((0, 0), (0, 0), (10, 10)),
                                   ((0, 0), (10, 10), (10, 10)), ((0, 0), (-5, -5), (10, 10)),
                                   ((0, 0), (30, 0), (10, 0)), ((1, 2), (4, 8), (2, 4)),
                                   ((INT_MIN, INT_MIN), (0, 0), (INT_MAX, INT_MAX)),
                                   ((INT_MIN, 0), (0, 0), (INT_MAX, 0)),
                                   ((-3, 7), (-3, -100), (-3, 1000))]):
        add(f'collinear-{i}', [(s, m, e)])

    # Semicircles, quarter circles and reflex arcs on axis-aligned and diagonal chords,
    # in both turning directions (mid on either side), for every from-vector branch.
    radii = [1, 2, 3, 10, 1000, 2 ** 20, 2 ** 30 - 1]
    for r in radii:
        for k, (s, e) in enumerate([((-r, 0), (r, 0)), ((r, 0), (-r, 0)), ((0, -r), (0, r)),
                                    ((0, r), (0, -r)), ((-r, -r), (r, r)), ((r, r), (-r, -r)),
                                    ((-r, r), (r, -r)), ((r, -r), (-r, r))]):
            cx = (s[0] + e[0]) // 2
            cy = (s[1] + e[1]) // 2
            px, py = -(e[1] - s[1]) // 2, (e[0] - s[0]) // 2
            for side in (1, -1):
                for depth, tag in ((1, 'semi'), (3, 'reflex')):
                    m = (cx + side * px * depth, cy + side * py * depth)
                    if all(INT_MIN <= c <= INT_MAX for c in m):
                        add(f'axis-{tag}-{r}-{k}-{side}', [(s, m, e)])
            # Quarter arcs: start on one axis, end on the other, mid on the 45 degree ray.
            q = round(r * math.sqrt(0.5))
            add(f'quarter-{r}-{k}', [((r, 0), (q, q), (0, r))])

    # Shallow arcs (sagitta 1, huge radius) and arcs at the full 32-bit extent.
    for i, (s, m, e) in enumerate([
            ((0, 0), (500, 1), (1000, 0)), ((0, 0), (500, -1), (1000, 0)),
            ((-2 ** 30, 0), (0, 1), (2 ** 30, 0)), ((INT_MIN, 0), (0, 1), (INT_MAX, 0)),
            ((INT_MIN, INT_MIN), (INT_MIN, INT_MAX), (INT_MAX, INT_MAX)),
            ((INT_MIN, INT_MIN), (INT_MAX, INT_MIN), (INT_MAX, INT_MAX)),
            ((INT_MAX, INT_MAX), (INT_MIN, INT_MAX), (INT_MIN, INT_MIN)),
            ((INT_MIN, INT_MAX), (INT_MAX, INT_MAX), (INT_MAX, INT_MIN)),
            ((0, 0), (1, 1), (2, 0)), ((0, 0), (0, 1), (1, 0)), ((0, 0), (1, 0), (0, 1)),
            ((0, 0), (INT_MAX, 1), (1, 0)), ((0, 0), (-1, INT_MAX), (0, 1)),
            ((INT_MAX, 0), (0, INT_MAX), (INT_MIN, 0)), ((INT_MAX, 0), (0, INT_MIN), (INT_MIN, 0)),
            ((0, INT_MIN), (INT_MAX, 0), (0, INT_MAX)), ((0, INT_MIN), (INT_MIN, 0), (0, INT_MAX))]):
        add(f'extreme-{i}', [(s, m, e)])

    # Refused Compute after a valid one keeps the stale members it did not assign;
    # a valid Compute after a refusal or a reversal overwrites them.
    good = ((0, 0), (5, 5), (10, 0))
    reflex = ((-10, 0), (0, -30), (10, 0))
    zero = ((3, 3), (1, 1), (3, 3))
    line = ((0, 0), (2, 2), (4, 4))
    for i, seq in enumerate([[good, zero], [good, line], [zero, good], [line, good],
                             [good, reflex], [reflex, good], [good, line, zero],
                             [reflex, zero, line, good], [zero], [line], [zero, line],
                             [good, good, good, good, good, good, good, good]]):
        add(f'sequence-{i}', seq)

    rng = random.Random(SEED)
    # Near-semicircles search for the upper clamp: halfChord / radius rounding above 1.
    upper = 0
    for i in range(20000):
        if upper >= 24:
            break
        h = rng.randint(1, 2 ** 29)
        s, e = (-h, 0), (h, 0)
        m = (rng.randint(-2, 2), rng.choice((h, -h)) + rng.randint(-1, 1))
        seen = set()
        mirror = Mirror()
        mirror.compute(s, m, e, seen)
        mirror.observe(seen)
        if 'clamp-upper' in seen:
            upper += 1
            add(f'clamp-upper-{upper}', [(s, m, e)])
    for scale, tag, count in ((16, 'small', 384), (10 ** 6, 'medium', 384),
                              (INT_MAX, 'full', 384)):
        for i in range(count):
            lo, hi = (-scale, scale) if scale != INT_MAX else (INT_MIN, INT_MAX)
            pts = [(rng.randint(lo, hi), rng.randint(lo, hi)) for _ in range(3)]
            add(f'random-{tag}-{i}', [tuple(pts)])
    for i in range(128):
        seq = []
        for _ in range(rng.randint(2, 4)):
            pts = [(rng.randint(-8, 8), rng.randint(-8, 8)) for _ in range(3)]
            if rng.random() < 0.3:
                pts[2] = pts[0]
            seq.append(tuple(pts))
        add(f'random-sequence-{i}', seq)

    missing = REQUIRED - set(labels)
    if missing:
        raise SystemExit(f'corpus misses branches {sorted(missing)}')
    if UNREACHABLE & set(labels):
        raise SystemExit(f'reached a branch recorded as unreachable: {UNREACHABLE & set(labels)}')
    return cases, labels


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='verify the committed corpus')
    parser.add_argument('--branches', action='store_true', help='print branch evidence counts')
    args = parser.parse_args()
    cases, labels = generate()
    text = ''.join(json.dumps(c, separators=(',', ':'), sort_keys=True) + '\n' for c in cases)
    if args.branches:
        for label, count in sorted(labels.items()):
            print(f'{label}\t{count}')
    if args.check:
        if CASES.read_text(encoding='utf-8') != text:
            sys.exit(f'{CASES} differs from the deterministic generator')
        print(f'{len(cases)} cases match')
    elif not args.branches:
        CASES.write_text(text, encoding='utf-8', newline='\n')
        print(f'wrote {len(cases)} cases to {CASES}')


if __name__ == '__main__':
    main()
