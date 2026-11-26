#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for math/vector2wx.h; inputs only, no expected answers.

Each request converts one value with one of the six owned conversions:

    {"op": OP, "value": [x, y] | null}

`null` converts a default-constructed source (wxPoint(), wxSize() or VECTOR2I()).
Integer results are {"x": int, "y": int}; VECTOR2D results carry each component as
its exact IEEE binary64 bits (16 lowercase hex digits).

The corpus crosses the 32-bit boundary/sign set on both components (every
direction and quadrant, INT_MIN and INT_MAX), adds the default source and fixed-seed
uniform and small-magnitude values. The malformed-request controls are written to a
separate fixture with the exact refusal both programs must give; they never enter
the public corpus. port/oracle/test_vector2wx_inputs.py follows each one with a
successful conversion.
"""
import argparse
import json
from pathlib import Path
import random
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.math.application.thin.replacement.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'vector2wx-controls.jsonl'
SEED = 0x7EC2A3
INT_MIN, INT_MAX = -2 ** 31, 2 ** 31 - 1
OPS = ['to_vector2i_size', 'to_vector2i_point', 'to_vector2d_point', 'to_vector2d_size',
       'to_wx_point', 'to_wx_size']
EDGES = [INT_MIN, INT_MIN + 1, -1, 0, 1, INT_MAX - 1, INT_MAX]
RANDOM_PER_OP = 16


def request(case, op, value):
    return {'unit': UNIT, 'case': case, 'input': {'op': op, 'value': value}}


def generate_cases():
    rng = random.Random(SEED)
    cases = []
    for op in OPS:
        cases.append(request(f'{op}-default', op, None))
        for i, x in enumerate(EDGES):
            for j, y in enumerate(EDGES):
                cases.append(request(f'{op}-edge-{i}-{j}', op, [x, y]))
        for k in range(RANDOM_PER_OP):
            # Half uniform over the whole int range, half small signed magnitudes.
            if k % 2 == 0:
                value = [rng.randint(INT_MIN, INT_MAX), rng.randint(INT_MIN, INT_MAX)]
            else:
                value = [rng.randint(-1000, 1000), rng.randint(-1000, 1000)]
            cases.append(request(f'{op}-random-{k:02d}', op, value))
    return cases


def invalid(message):
    return {'kind': 'invalid_input', 'message': message}


def generate_controls():
    """Malformed requests and the exact refusal of each, in the adapters' check order:
    op string, known op, exact fields, point shape, then each component's range."""
    rows = []

    def add(name, value_input, message):
        rows.append({'request': {'unit': UNIT, 'case': f'control-{len(rows):02d}-{name}',
                                 'input': value_input},
                     'error': invalid(message)})

    add('missing-op', {'value': [1, 2]}, 'expected string: op')
    add('numeric-op', {'op': 1, 'value': [1, 2]}, 'expected string: op')
    add('unknown-op', {'op': 'to_vector3d', 'value': [1, 2]}, 'unknown operation')
    add('case-op', {'op': 'ToWxPoint', 'value': [1, 2]}, 'unknown operation')
    for op in OPS:
        add(f'{op}-missing-value', {'op': op}, 'unexpected or missing field')
        add(f'{op}-extra-field', {'op': op, 'value': [1, 2], 'z': 3}, 'unexpected or missing field')
    for name, value in [('number', 5), ('string', '1,2'), ('object', {}), ('bool', True),
                        ('empty', []), ('short', [1]), ('long', [1, 2, 3])]:
        add(f'shape-{name}', {'op': 'to_wx_size', 'value': value}, 'expected point')
    for name, value in [('float', [1.0, 2]), ('float-y', [0, -2.5]), ('string', [1, '2']),
                        ('bool', [True, 0]), ('null', [None, 0]), ('nested', [[1], 2]),
                        ('above-max', [INT_MAX + 1, 0]), ('below-min', [0, INT_MIN - 1]),
                        ('i64-max', [2 ** 63 - 1, 0]), ('i64-min', [0, -2 ** 63]),
                        ('u64-max', [2 ** 64 - 1, 0])]:
        for op in ('to_vector2d_point', 'to_wx_point'):
            add(f'{op}-component-{name}', {'op': op, 'value': value}, 'component out of range')
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='verify the committed files')
    args = parser.parse_args()
    outputs = [(CASES, generate_cases(), 'cases'), (CONTROLS, generate_controls(), 'controls')]
    for path, rows, what in outputs:
        names = [r.get('case') or r['request']['case'] for r in rows]
        if len(set(names)) != len(names):
            sys.exit(f'duplicate case id in {what}')
        text = ''.join(json.dumps(r, separators=(',', ':'), sort_keys=True) + '\n' for r in rows)
        if args.check:
            if path.read_text(encoding='utf-8') != text:
                sys.exit(f'{path} differs from the deterministic generator')
            print(f'{len(rows)} {what} match')
        else:
            path.write_text(text, encoding='utf-8', newline='\n')
            print(f'wrote {len(rows)} {what} to {path}')


if __name__ == '__main__':
    main()
