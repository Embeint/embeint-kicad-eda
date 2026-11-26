#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus and refusal controls for POISSON_DISK::ToroidalUnitTile.

Inputs only, no expected answers. Python floats are IEEE binary64 with correctly rounded
division, and math.ceil is exact, so `grid_n` below is the reference's
std::max( 3, (int) std::ceil( std::sqrt( 2.0 ) / aMinDist ) ) wherever that is defined. It is
used to place radii on grid-size boundaries and to set exact storage limits; it never predicts a
sample.

The public corpus holds only requests both programs answer with a result: the early-return
domain (empty tiles), tiles from the largest radius down to digested tiles of ~10^5 samples, grid
transitions, the exact grid limit, and direct draws through the Boost.Random instantiations the
function uses. Requests the contract refuses (undefined int conversion, grid or sample limit,
malformed input) are controls in port/oracle/fixtures/poisson-disk-controls.jsonl, each followed
by a successful continuation in the same process; rust/kicad-poisson-shim/compare_controls.py runs
them on both programs.
"""
import argparse
import json
import math
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.poisson_disk.application.pure.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'poisson-disk-controls.jsonl'
SEED = 0x9015_50D1
SQRT2 = math.sqrt(2.0)
MAX_LIMIT = 4194304
# Tiles at or above this radius list every sample; smaller ones report a digest and both ends.
LIST_RADIUS = 0.03
SEEDS = [0, 1, 2, 5489, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF]


def bits(x):
    return format(struct.unpack('<Q', struct.pack('<d', x))[0], '016x')


def from_bits(text):
    return struct.unpack('<d', struct.pack('<Q', int(text, 16)))[0]


def grid_n(r):
    """None outside the defined conversion; the reference grid size otherwise."""
    if r <= 0.0 or r > 0.5:
        return 0
    q = math.ceil(SQRT2 / r) if math.isfinite(SQRT2 / r) else math.inf
    if not q < 2 ** 31:
        return None
    return max(3, q)


def tile(r, seed, cells=MAX_LIMIT, samples=MAX_LIMIT, emit=None):
    if emit is None:
        dense = isinstance(r, float) and 0.0 < r < LIST_RADIUS
        emit = 'digest' if dense else 'samples'
    return {'op': 'tile', 'min_dist': bits(r) if isinstance(r, float) else r, 'seed': seed,
            'max_grid_cells': cells, 'max_samples': samples, 'emit': emit}


def request(case, inp):
    return {'unit': UNIT, 'case': case, 'input': inp}


def public_cases():
    rng = random.Random(SEED)
    out = []

    def add(case, inp):
        out.append(request(case, inp))

    # Early-return domain: empty tiles, whatever the limits (they are not consulted).
    empty = [0.0, -0.0, -5e-324, -1e-300, -0.5, -1.0, -math.inf, math.inf,
             math.nextafter(0.5, 1.0), 0.75, 1.0, 1e300, 1.7976931348623157e308]
    for i, r in enumerate(empty):
        for seed in (0, 0xFFFFFFFF):
            add(f'empty-{i}-{seed:x}', tile(r, seed))
        add(f'empty-{i}-zero-limits', tile(r, 7, cells=0, samples=0))
    # Negative bit patterns that are not NaN (NaN is a control: it passes the guard).
    for i, payload in enumerate(['8000000000000000', 'fff0000000000000', 'bff0000000000000',
                                 '800fffffffffffff']):
        add(f'empty-bits-{i}', tile(payload, 3))

    # The largest radius (gridN 3): small tiles, so sample counts vary with the seed.
    for seed in sorted(set(range(64)) | set(SEEDS)):
        add(f'half-{seed:x}', tile(0.5, seed))
    add('half-below', tile(math.nextafter(0.5, 0.0), 11))

    # Grid-size transitions: sqrt(2)/k and its neighbours change ceil(sqrt(2)/r) by one.
    for k in range(3, 41):
        base = SQRT2 / k
        for tag, r in (('at', base), ('up', math.nextafter(base, 1.0)),
                       ('down', math.nextafter(base, 0.0))):
            if 0.0 < r <= 0.5:
                add(f'grid-{k}-{tag}', tile(r, k * 7919))

    # The exact grid limit succeeds (gridN * gridN cells); one fewer is a control.
    for r in (0.5, 0.25, 0.1, 0.05, 0.02):
        n = grid_n(r)
        add(f'grid-limit-exact-{r}', tile(r, 99, cells=n * n))

    # Seed sweep at moderate radii, every sample listed.
    for r in (0.3, 0.2, 0.15, 0.1, 0.07, 0.05):
        for seed in SEEDS:
            add(f'seed-{r}-{seed:x}', tile(r, seed))

    # Log-uniform radius sweep with fixed-seed random seeds.
    for i in range(160):
        r = math.exp(rng.uniform(math.log(0.012), math.log(0.5)))
        add(f'sweep-{i}', tile(r, rng.getrandbits(32)))

    # Dense tiles, digested: up to ~1.5e5 samples.
    for r in (0.01, 0.005, 0.003, 0.002):
        for seed in (0, 5489, 0xFFFFFFFF):
            add(f'dense-{r}-{seed:x}', tile(r, seed))
    add('dense-listed', tile(0.02, 12345, emit='samples'))
    add('half-digest', tile(0.5, 12345, emit='digest'))

    # Direct draws through the pinned Boost instantiations, from one engine per request.
    u64max = 2 ** 64 - 1
    int_ranges = [(0, 0), (5, 5), (0, 1), (0, 2), (3, 9), (0, 1000), (0, 2 ** 31),
                  (0, 2 ** 31 + 1), (0, 2 ** 32 - 3), (0, 2 ** 32 - 2), (0, 2 ** 32 - 1),
                  (7, 7 + 2 ** 32 - 1), (0, 2 ** 32), (0, 2 ** 32 + 1), (1, 2 ** 33),
                  (0, 2 ** 40), (0, 2 ** 63), (0, u64max - 1), (0, u64max), (1, u64max),
                  (2 ** 63, u64max), (u64max, u64max), (0, 3 * 2 ** 62)]
    for i, (lo, hi) in enumerate(int_ranges):
        add(f'rng-int-{i}', {'op': 'rng', 'seed': 1000 + i, 'draws': [
            {'kind': 'int', 'min': str(lo), 'max': str(hi), 'count': 200}]})
    dmax = 1.7976931348623157e308
    real_ranges = [(0.0, 1.0), (-1.0, 1.0), (1.0, math.nextafter(1.0, 2.0)), (0.0, 5e-324),
                   (-dmax, dmax), (-dmax, 0.0), (1e300, dmax), (-3.5, 1e-300), (2.0, 3.0)]
    for i, (lo, hi) in enumerate(real_ranges):
        add(f'rng-real-{i}', {'op': 'rng', 'seed': 2000 + i, 'draws': [
            {'kind': 'real', 'min': bits(lo), 'max': bits(hi), 'count': 200}]})
    for seed in SEEDS:
        add(f'rng-u32-{seed:x}', {'op': 'rng', 'seed': seed, 'draws': [
            {'kind': 'u32', 'count': 1300}]})
    # The sequence ToroidalUnitTile draws: reals, then size_t picks, interleaved on one engine.
    for i in range(12):
        draws = []
        for _ in range(rng.randrange(4, 20)):
            kind = rng.choice(['u32', 'int', 'real'])
            count = rng.randrange(1, 40)
            if kind == 'u32':
                draws.append({'kind': 'u32', 'count': count})
            elif kind == 'int':
                hi = rng.choice([0, 1, rng.randrange(2, 5000), rng.getrandbits(32),
                                 rng.getrandbits(64)])
                draws.append({'kind': 'int', 'min': '0', 'max': str(hi), 'count': count})
            else:
                draws.append({'kind': 'real', 'min': bits(0.0), 'max': bits(1.0), 'count': count})
        add(f'rng-mixed-{i}', {'op': 'rng', 'seed': rng.getrandbits(32), 'draws': draws})
    add('rng-max-draws', {'op': 'rng', 'seed': 77, 'draws': [{'kind': 'u32', 'count': 65536}]})
    return out


def controls():
    """(request, expected refusal) pairs; each refusal is followed by a continuation."""
    out = []
    n = 0

    def refuse(inp, message):
        nonlocal n
        n += 1
        out.append({'request': request(f'control-{n}', inp),
                    'expect': {'kind': 'invalid_input', 'message': message}})
        out.append({'request': request(f'control-{n}-continuation', tile(0.2, n)),
                    'expect': 'result'})

    # Undefined (int) conversion: NaN payloads pass the domain guard, tiny radii overflow int.
    for payload in ['7ff8000000000000', '7ff0000000000001', '7fffffffffffffff',
                    'fff8000000000000', 'fff0000000000001']:
        refuse(tile(payload, 1), 'undefined grid size')
    r = SQRT2 / (2 ** 31 - 1)
    while grid_n(r) is None:
        r = math.nextafter(r, 1.0)
    while grid_n(math.nextafter(r, 0.0)) is not None:
        r = math.nextafter(r, 0.0)
    smallest_defined = r
    assert grid_n(smallest_defined) == 2 ** 31 - 1, grid_n(smallest_defined)
    for r in (5e-324, 2.2250738585072014e-308, 1e-300, 1e-12,
              math.nextafter(smallest_defined, 0.0)):
        assert grid_n(r) is None, r
        refuse(tile(r, 2), 'undefined grid size')
    # Defined but far beyond any storage limit: the largest int grid, then ordinary radii.
    for r in (smallest_defined, 1e-6, 1e-4):
        assert grid_n(r) ** 2 > MAX_LIMIT, r
        refuse(tile(r, 3), 'grid limit')
    for r in (0.5, 0.25, 0.1, 0.05, 0.02):
        g = grid_n(r)
        refuse(tile(r, 99, cells=g * g - 1), 'grid limit')
    refuse(tile(0.5, 4, cells=0), 'grid limit')
    refuse(tile(0.5, 4, samples=0), 'sample limit')
    refuse(tile(0.1, 4, samples=1), 'sample limit')
    # Malformed requests.
    refuse({'op': 'tile'}, 'unexpected or missing field')
    refuse(dict(tile(0.2, 1), extra=1), 'unexpected or missing field')
    refuse(dict(tile(0.2, 1), min_dist=0.2), 'expected string: min_dist')
    refuse(dict(tile(0.2, 1), min_dist='3FC999999999999A'), 'expected bits: min_dist')
    refuse(dict(tile(0.2, 1), min_dist='3fc99999999999'), 'expected bits: min_dist')
    for seed in (-1, 2 ** 32, 1.0, '1', None):
        refuse(dict(tile(0.2, 1), seed=seed), 'seed out of range')
    for name in ('max_grid_cells', 'max_samples'):
        for value in (-1, MAX_LIMIT + 1, 1.5, '4'):
            refuse(dict(tile(0.2, 1), **{name: value}), f'integer out of range: {name}')
    refuse(dict(tile(0.2, 1), emit='all'), 'unknown emit')
    refuse({'op': 'nope'}, 'unknown operation')
    refuse({'op': 1}, 'expected string: op')
    good = {'kind': 'u32', 'count': 1}
    refuse({'op': 'rng', 'seed': 1, 'draws': []}, 'draws out of range')
    refuse({'op': 'rng', 'seed': 1, 'draws': [good] * 65}, 'draws out of range')
    refuse({'op': 'rng', 'seed': 1, 'draws': [{'kind': 'u32', 'count': 65536}, good]},
           'draws out of range')
    refuse({'op': 'rng', 'seed': 1, 'draws': [{'count': 1}]}, 'expected draw')
    refuse({'op': 'rng', 'seed': 1, 'draws': [{'kind': 'u64', 'count': 1}]}, 'unknown draw')
    refuse({'op': 'rng', 'seed': 1, 'draws': [{'kind': 'u32', 'count': 0}]},
           'integer out of range: count')
    refuse({'op': 'rng', 'seed': 1, 'draws': [{'kind': 'u32', 'count': 1, 'min': '0'}]},
           'unexpected or missing field')
    refuse({'op': 'rng', 'seed': 1, 'draws': [
        {'kind': 'int', 'count': 1, 'min': '01', 'max': 'x'}]}, 'expected decimal: min')
    refuse({'op': 'rng', 'seed': 1, 'draws': [
        {'kind': 'int', 'count': 1, 'min': '0', 'max': str(2 ** 64)}]}, 'expected decimal: max')
    refuse({'op': 'rng', 'seed': 1, 'draws': [
        {'kind': 'int', 'count': 1, 'min': '6', 'max': '5'}]}, 'invalid int range')
    for lo, hi in ((1.0, 1.0), (2.0, 1.0), (math.nan, 1.0), (0.0, math.inf), (-math.inf, 0.0)):
        refuse({'op': 'rng', 'seed': 1, 'draws': [
            {'kind': 'real', 'count': 1, 'min': bits(lo), 'max': bits(hi)}]},
            'invalid real range')
    return out


def render(records):
    return ''.join(json.dumps(r, sort_keys=True, separators=(',', ':')) + '\n' for r in records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='fail if the files differ')
    args = parser.parse_args()
    files = {CASES: render(public_cases()), CONTROLS: render(controls())}
    names = [json.loads(s)['case'] for s in files[CASES].splitlines()]
    names += [json.loads(s)['request']['case'] for s in files[CONTROLS].splitlines()]
    assert len(set(names)) == len(names), 'duplicate case id'
    for path, text in files.items():
        if args.check:
            if path.read_text(encoding='utf-8') != text:
                sys.exit(f'{path} is stale; rerun generate_cases.py')
        else:
            path.write_bytes(text.encode('utf-8'))
    print(f'{len(files[CASES].splitlines())} public cases, '
          f'{len(files[CONTROLS].splitlines())} control requests')


if __name__ == '__main__':
    main()
