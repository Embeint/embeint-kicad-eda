#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for geometry/distribute.cpp; inputs only, no expected answers.

GetDeltasForDistributeByGaps and GetDeltasForDistributeByPoints compute in int, and a signed
int overflow is undefined (C++ [expr.pre]/4). The mirror below replays only their int
operations in source order to decide whether a call reaches such an overflow, and which
operation overflows first. It never computes a delta.

Python floats are IEEE binary64 with correctly rounded * and /. Every int and every item
index is exactly representable, so totalGap / double( n - 1 ) and i * perItemGap are the
same single rounded operations here as in C++; KiROUND is evaluated exactly with Fraction
(round half away from zero), which the overflow decision of the two int additions that use
it needs.

A request whose calls all stay defined goes to the public corpus. A call that overflows
goes to the separate undefined-domain controls fixture, inside a request that continues
with at least one defined call after the last refusal; both programs must refuse exactly
the recorded calls with the recorded operation and agree on every other call.
"""
import argparse
from collections import Counter
from fractions import Fraction
import json
import math
from pathlib import Path
import random
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.distribute.application.ui.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'distribute-controls.jsonl'
SEED = 0xD157B07E
INT_MIN, INT_MAX = -2 ** 31, 2 ** 31 - 1
MAX_CALLS = 8
MAX_ITEMS = 4096
# The upstream QA inputs (qa/tests/libs/kimath/geometry/test_distribute.cpp at the pin).
QA_GAPS = [[[0, 100], [200, 300], [400, 500]],
           [[0, 100], [149, 249], [400, 500]],
           [[-100, 0], [0, 100], [0, 100], [300, 400]]]
QA_POINTS = [[0, 100, 200, 300, 400], [0, 49, 200], [0, 0, 0, 100]]


class Overflow(Exception):
    pass


def fits(v):
    return INT_MIN <= v <= INT_MAX


def op(value, message):
    if not fits(value):
        raise Overflow(message)
    return value


def kiround(x):
    """KiROUND<double, int>; the overflow branch is unreachable for these arguments."""
    f = Fraction(x)
    r = math.floor(abs(f) + Fraction(1, 2))
    r = r if f >= 0 else -r
    assert fits(r), x
    return r


def gaps_labels(extents, labels):
    """Labels of the branches one gaps call reaches; raises Overflow at the first overflow."""
    n = len(extents)
    if n < 3:
        labels['gaps:fewer-than-three'] += 1
        return
    total_gap = op(extents[-1][0] - extents[0][1], 'signed overflow: gaps total space')
    for i in range(1, n - 1):
        start, end = extents[i]
        span = op(end - start, f'signed overflow: gaps item span at {i}')
        total_gap = op(total_gap - span, f'signed overflow: gaps total gap at {i}')
        if span < 0:
            labels['gaps:negative-span'] += 1
    per_item_gap = total_gap / float(n - 1)
    labels['gaps:negative-total-gap' if total_gap < 0 else 'gaps:total-gap'] += 1
    target_pos = extents[0][1]
    for i in range(1, n - 1):
        start, end = extents[i]
        accumulated = float(i) * per_item_gap
        rounded = kiround(accumulated)
        if Fraction(accumulated) - math.floor(Fraction(accumulated)) == Fraction(1, 2):
            labels['gaps:round-half'] += 1
        offset = op(target_pos - start, f'signed overflow: gaps target offset at {i}')
        delta = op(offset + rounded, f'signed overflow: gaps delta at {i}')
        if delta in (INT_MIN, INT_MAX) or offset in (INT_MIN, INT_MAX):
            labels['gaps:int-limit'] += 1
        target_pos = op(target_pos + (end - start), f'signed overflow: gaps target advance at {i}')
    labels['gaps:deltas'] += 1


def points_labels(positions, labels):
    n = len(positions)
    if n < 3:
        labels['points:fewer-than-three'] += 1
        return
    start_pos = positions[0]
    total_gaps = op(positions[-1] - start_pos, 'signed overflow: points total gaps')
    item_gap = total_gaps / float(n - 1)
    labels['points:negative-total' if total_gaps < 0 else 'points:total'] += 1
    for i in range(1, n - 1):
        x = float(i) * item_gap
        if Fraction(x) - math.floor(Fraction(x)) == Fraction(1, 2):
            labels['points:round-half'] += 1
        target_pos = op(start_pos + kiround(x), f'signed overflow: points target at {i}')
        delta = op(target_pos - positions[i], f'signed overflow: points delta at {i}')
        if delta in (INT_MIN, INT_MAX):
            labels['points:int-limit'] += 1
    labels['points:deltas'] += 1


def call_labels(call, labels):
    if call['fn'] == 'gaps':
        gaps_labels(call['extents'], labels)
    else:
        points_labels(call['positions'], labels)


def gaps(extents):
    return {'fn': 'gaps', 'extents': [list(e) for e in extents]}


def points(positions):
    return {'fn': 'points', 'positions': list(positions)}


class Corpus:
    def __init__(self):
        self.cases, self.controls = [], []
        self.labels, self.control_labels = Counter(), Counter()
        self.names = set()

    def add(self, name, calls):
        """A request of defined calls only; an overflowing call is a generator error."""
        assert 1 <= len(calls) <= MAX_CALLS and name not in self.names, name
        self.names.add(name)
        for call in calls:
            call_labels(call, self.labels)
        self.cases.append({'unit': UNIT, 'case': name, 'input': {'calls': calls}})

    def control(self, name, calls):
        """A request with at least one overflowing call and a defined call after the last."""
        assert 1 <= len(calls) <= MAX_CALLS and name not in self.names, name
        self.names.add(name)
        refused = {}
        for i, call in enumerate(calls):
            try:
                call_labels(call, self.control_labels)
            except Overflow as e:
                refused[str(i)] = str(e)
                self.control_labels[str(e).split(' at ')[0]] += 1
        assert refused and str(len(calls) - 1) not in refused, name
        self.controls.append({'request': {'unit': UNIT, 'case': name, 'input': {'calls': calls}},
                              'refused': refused})

    def either(self, name, call, continuation):
        """Route one generated call: public when defined, else a control with a continuation."""
        try:
            call_labels(call, Counter())
        except Overflow:
            self.control(f'control-{name}', [call, continuation])
            return
        self.add(name, [call])


def rand_int(rng, lo=INT_MIN, hi=INT_MAX):
    return rng.randint(lo, hi)


def generate():
    rng = random.Random(SEED)
    c = Corpus()
    qa_g = [gaps(e) for e in QA_GAPS]
    qa_p = [points(p) for p in QA_POINTS]
    for i, call in enumerate(qa_g + qa_p):
        c.add(f'qa-{i}', [call])
    c.add('qa-all', qa_g + qa_p[:MAX_CALLS - len(qa_g)])
    continuation = qa_p[2]

    # Fewer than three items: no arithmetic runs, whatever the values.
    extremes = [INT_MIN, INT_MIN + 1, -1, 0, 1, INT_MAX - 1, INT_MAX]
    c.add('empty', [gaps([]), points([])])
    for a in extremes:
        c.add(f'one-{a}', [gaps([(a, -a if a != INT_MIN else INT_MAX)]), points([a])])
        for b in extremes:
            c.add(f'two-{a}-{b}', [gaps([(a, b), (b, a)]), points([a, b])])

    # Rounding ties and half-way cases, both signs, every small gap count.
    for n in range(3, 12):
        for total in range(-2 * (n - 1), 2 * (n - 1) + 1):
            c.add(f'ties-points-{n}-{total}', [points([0] * (n - 1) + [total])])
            c.add(f'ties-gaps-{n}-{total}', [gaps([(0, 0)] * (n - 1) + [(total, total)])])

    # Small random layouts: sorted, unsorted, overlapping and negative-width items.
    for k in range(600):
        n = rng.randint(3, 16)
        span = rng.choice([10, 1000, 100000])
        if rng.random() < 0.5:
            starts = sorted(rng.randint(-span, span) for _ in range(n))
        else:
            starts = [rng.randint(-span, span) for _ in range(n)]
        widths = [rng.randint(-span // 10, span // 2) for _ in range(n)]
        c.add(f'small-gaps-{k}', [gaps([(s, s + w) for s, w in zip(starts, widths)])])
        c.add(f'small-points-{k}', [points(starts)])

    # Huge coordinates: any defined call is public, any overflowing call becomes a control.
    for k in range(400):
        n = rng.randint(3, 10)
        positions = [rand_int(rng) for _ in range(n)]
        c.either(f'huge-points-{k}', points(positions), continuation)
        half = [rand_int(rng, INT_MIN // 2, INT_MAX // 2) for _ in range(n)]
        c.either(f'half-points-{k}', points(half), continuation)
        extents = [(rand_int(rng), rand_int(rng)) for _ in range(n)]
        c.either(f'huge-gaps-{k}', gaps(extents), continuation)
        quarter = sorted(rand_int(rng, INT_MIN // 4, INT_MAX // 4) for _ in range(2 * n))
        c.either(f'quarter-gaps-{k}', gaps(list(zip(quarter[::2], quarter[1::2]))), continuation)

    # Exact int limits reached without overflow.
    c.add('limit-points-span', [points([INT_MIN, 0, INT_MIN + 7, -1]), points([-1, 5, INT_MAX - 1]),
                                points([INT_MIN, INT_MIN, INT_MIN]), points([INT_MAX] * 5)])
    c.add('limit-points-delta', [points([0, INT_MIN + 1, 0]), points([-1, INT_MAX - 1, 0]),
                                 points([0, INT_MAX, 0]), points([-1, INT_MIN, -1])])
    c.add('limit-gaps-space', [gaps([(0, INT_MIN), (0, 0), (-1, 0)]),
                               gaps([(0, -1), (0, 0), (INT_MAX - 1, 0)]),
                               gaps([(0, 0), (0, 0), (INT_MIN, 0)])])
    c.add('limit-gaps-span', [gaps([(0, -1), (INT_MIN, -1), (0, 0)]),
                              gaps([(0, 0), (-1, INT_MAX - 1), (0, 0)]),
                              gaps([(0, 0), (INT_MAX, 0), (0, 0)])])
    c.add('limit-gaps-total', [gaps([(0, 0), (0, INT_MAX), (0, 0)]),
                               gaps([(0, -1), (0, INT_MAX), (0, 1), (-1, 0)])])
    c.add('limit-gaps-offset', [gaps([(0, INT_MAX), (0, 0), (INT_MAX, 0)]),
                                gaps([(0, INT_MIN), (0, 0), (INT_MIN, 0)])])
    c.add('limit-gaps-delta', [gaps([(0, -1), (INT_MIN + 6, INT_MIN + 6), (10, 0)]),
                               gaps([(0, 0), (INT_MAX - 5, INT_MAX - 5), (-11, 0)])])
    c.add('limit-gaps-advance', [gaps([(0, INT_MAX - 1), (0, 1), (INT_MAX, INT_MAX)]),
                                 gaps([(0, INT_MIN + 1), (0, -1), (INT_MIN, INT_MIN)])])

    # Large item counts up to the adapter bound.
    for k, n in enumerate([100, 1000, 4095, MAX_ITEMS]):
        positions = sorted(rng.randint(-10 ** 8, 10 ** 8) for _ in range(n))
        c.add(f'many-points-{k}', [points(positions)])
        starts = sorted(rng.randint(-10 ** 8, 10 ** 8) for _ in range(n))
        c.add(f'many-gaps-{k}', [gaps([(s, s + rng.randint(0, 50000)) for s in starts])])
    c.add('many-limit-points', [points([INT_MIN] + [0] * (MAX_ITEMS - 2) + [-1])])
    c.add('many-limit-gaps', [gaps([(0, INT_MIN)] + [(0, 0)] * (MAX_ITEMS - 2) + [(-1, 0)])])

    # Requests of several independent calls.
    for k in range(60):
        calls = []
        for _ in range(rng.randint(2, MAX_CALLS)):
            n = rng.randint(0, 9)
            if rng.random() < 0.5:
                calls.append(points([rng.randint(-5000, 5000) for _ in range(n)]))
            else:
                calls.append(gaps([(s, s + rng.randint(-50, 500))
                                   for s in (rng.randint(-5000, 5000) for _ in range(n))]))
        c.add(f'multi-{k}', calls)

    # Undefined-domain controls: every reachable overflowing operation, both directions, at the
    # first and a later item, each followed by a defined call. PointsTarget is unreachable:
    # KiROUND( i * itemGap ) lies between 0 and totalGaps, so startPos + it lies between the
    # first and last positions.
    targeted = {
        'gaps-total-space': [gaps([(0, 1), (0, 0), (INT_MIN, 0)]),
                             gaps([(0, -1), (0, 0), (INT_MAX, 0)])],
        'gaps-item-span': [gaps([(0, 0), (INT_MIN, 1), (0, 0)]),
                           gaps([(0, 0), (INT_MAX, -2), (0, 0)]),
                           gaps([(0, 0), (0, 1), (2, 3), (INT_MIN, 0), (9, 9)])],
        'gaps-total-gap': [gaps([(0, 0), (0, 10), (INT_MIN + 5, 0)]),
                           gaps([(0, -1), (0, -5), (INT_MAX - 2, 0)]),
                           gaps([(0, 0), (0, INT_MAX), (0, 2), (0, 0)])],
        'gaps-target-offset': [gaps([(0, INT_MAX), (-1, -1), (INT_MAX, 0)]),
                               gaps([(0, INT_MIN), (1, 1), (INT_MIN, 0)]),
                               gaps([(0, 0), (0, 0), (INT_MIN, INT_MIN), (0, 0)])],
        'gaps-delta': [gaps([(0, -1), (INT_MIN, INT_MIN), (10, 0)]),
                       gaps([(0, 0), (INT_MAX, INT_MAX), (-11, 0)]),
                       gaps([(0, 0), (0, 0), (INT_MIN + 1, INT_MIN + 1), (30, 0)])],
        'gaps-target-advance': [gaps([(0, INT_MAX), (0, 1), (INT_MAX, INT_MAX)]),
                                gaps([(0, INT_MIN), (0, -1), (INT_MIN, INT_MIN)]),
                                gaps([(0, INT_MAX - 3), (0, 1), (5, 6), (0, 3), (INT_MAX, 0)])],
        'points-total-gaps': [points([INT_MIN, 0, 1]), points([INT_MAX, 0, -2])],
        'points-delta': [points([0, INT_MIN, 2]), points([0, INT_MAX, -4]),
                         points([0, 0, INT_MIN, 0, 8])],
    }
    for family, calls in targeted.items():
        for k, call in enumerate(calls):
            c.control(f'control-{family}-{k}', [call, continuation])
            c.control(f'control-{family}-{k}-between', [qa_g[1], call, qa_p[1], call, qa_g[2]])
    c.control('control-all-families', [calls[0] for calls in targeted.values()][:MAX_CALLS - 1]
              + [continuation])
    return c


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='verify the committed corpus')
    parser.add_argument('--branches', action='store_true', help='print branch evidence counts')
    args = parser.parse_args()
    c = generate()
    if args.branches:
        for title, counts in (('public corpus', c.labels), ('controls', c.control_labels)):
            print(f'# {title}')
            for label, count in sorted(counts.items()):
                print(f'{label}\t{count}')
    stale = False
    for path, rows, what in ((CASES, c.cases, 'cases'),
                             (CONTROLS, c.controls, 'undefined-domain controls')):
        text = ''.join(json.dumps(r, separators=(',', ':'), sort_keys=True) + '\n' for r in rows)
        if args.check:
            if not path.exists() or path.read_text(encoding='utf-8') != text:
                print(f'{path} is stale; rerun generate_cases.py', file=sys.stderr)
                stale = True
        else:
            path.write_text(text, encoding='utf-8', newline='\n')
        print(f'{len(rows)} {what}: {path.relative_to(ROOT).as_posix()}')
    return 1 if stale else 0


if __name__ == '__main__':
    sys.exit(main())
