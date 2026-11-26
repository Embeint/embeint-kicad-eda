#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public cases and refusal controls for unit include.core.application.pure.p001.

Writes port/cases/<unit>.jsonl (requests inside the pinned headers' defined domain only)
and port/oracle/fixtures/core-app-include-p001-controls.jsonl (requests both adapters must
refuse, with the expected error). No result is computed here: the generator only decides
which requests are defined, using exact integer arithmetic for the signed-overflow
boundaries of MIRRORVAL and THROTTLE and the slot lifetimes of its own scripts.

--check verifies that the committed files equal the generator output.
"""
import argparse
import json
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'include.core.application.pure.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/core-app-include-p001-controls.jsonl'
SEED = 20261127
I32 = (-2**31, 2**31 - 1)
I64 = (-2**63, 2**63 - 1)
UNDEFINED = 'undefined: signed overflow'
CHRONO = 'undefined: chrono overflow'


def bits(v):
    return struct.pack('>d', v).hex()


DOUBLES = [
    '0000000000000000', '8000000000000000', '3ff0000000000000', 'bff8000000000000',
    '0000000000000001', '8000000000000001', '000fffffffffffff', '0010000000000000',
    '7fefffffffffffff', 'ffefffffffffffff', '7ff0000000000000', 'fff0000000000000',
    '7ff8000000000000', 'fff8000000000000', '7ff8000000000123', 'fff4000000000abc',
    '7ff0000000000001', '4415af1d78b58c40', '3fb999999999999a', 'c1dfffffffc00000',
    '41dfffffffc00000', '3ca0000000000000',
]


def defined_mirror(point, ref, lo, hi):
    difference = point - ref
    if not lo <= difference <= hi or difference == lo:
        return False
    return lo <= -difference + ref <= hi


class Out:
    def __init__(self):
        self.cases = []
        self.controls = []
        self.names = set()

    def _name(self, name):
        assert name not in self.names, name
        self.names.add(name)

    def case(self, name, payload):
        self._name(name)
        self.cases.append({'unit': UNIT, 'case': name, 'input': payload})

    def control(self, name, payload, message, kind='invalid_input'):
        self._name(name)
        self.controls.append({'request': {'unit': UNIT, 'case': name, 'input': payload},
                              'error': {'kind': kind, 'message': message}})


def typeinfo(out):
    out.case('enumerators', {'op': 'enumerators'})
    for value in range(-128, 128):
        out.case(f'typeinfo-{value}', {'op': 'typeinfo', 'value': value})
    for i, value in enumerate([128, -129, 255, 2**31 - 1, -2**31, 2**63 - 1, 2**64 - 1]):
        out.control(f'typeinfo-range-{i}', {'op': 'typeinfo', 'value': value},
                    'integer out of range: value')
    out.control('typeinfo-float', {'op': 'typeinfo', 'value': 1.0}, 'integer out of range: value')
    out.control('typeinfo-string', {'op': 'typeinfo', 'value': '5'}, 'integer out of range: value')
    out.control('typeinfo-extra', {'op': 'typeinfo', 'value': 5, 'x': 1}, 'unexpected or missing field')
    out.control('enumerators-extra', {'op': 'enumerators', 'value': 5}, 'unexpected or missing field')


def integer_values(lo, hi, rng):
    edges = [lo, lo + 1, lo // 2, -2**20, -7, -1, 0, 1, 7, 2**20, hi // 2, hi - 1, hi]
    return edges + [rng.randint(lo, hi) for _ in range(6)] + [rng.randint(-1000, 1000) for _ in range(4)]


def mirror(out, rng):
    for op in ('mirror_val', 'mirror'):
        for type_name, (lo, hi) in (('int', I32), ('int64_t', I64)):
            values = integer_values(lo, hi, rng)
            n = 0
            refused = 0
            for p in values:
                for r in values:
                    payload = {'op': op, 'type': type_name, 'point': p, 'ref': r}
                    if defined_mirror(p, r, lo, hi):
                        out.case(f'{op}-{type_name}-{n}', payload)
                    elif refused < 40:
                        out.control(f'{op}-{type_name}-overflow-{refused}', payload, UNDEFINED)
                        refused += 1
                    n += 1
        n = 0
        for p in DOUBLES:
            for r in DOUBLES:
                if op == 'mirror' and rng.random() < 0.6:
                    continue
                out.case(f'{op}-double-{n}', {'op': op, 'type': 'double', 'point': p, 'ref': r})
                n += 1
        for i in range(60):
            p, r = bits(rng.uniform(-1e12, 1e12)), bits(rng.uniform(-1e3, 1e3))
            out.case(f'{op}-double-random-{i}', {'op': op, 'type': 'double', 'point': p, 'ref': r})
        out.control(f'{op}-unknown-type', {'op': op, 'type': 'float', 'point': 1, 'ref': 2}, 'unknown type')
        out.control(f'{op}-int-range', {'op': op, 'type': 'int', 'point': 2**31, 'ref': 0},
                    'integer out of range: point')
        out.control(f'{op}-double-hex', {'op': op, 'type': 'double', 'point': '3FF0000000000000',
                                         'ref': '0000000000000000'}, 'expected hex: point')
        out.control(f'{op}-missing', {'op': op, 'type': 'int', 'point': 1}, 'unexpected or missing field')

    directions = [0, 1, 2, -1, 42, I32[0], I32[1]]
    for type_name, (lo, hi) in (('VECTOR2I', I32), ('VECTOR2L', I64)):
        values = [lo, -5, 0, 3, hi]
        n = 0
        refused = 0
        for d in directions:
            for px in values:
                for py in values:
                    for r in ([0, 0], [lo, hi], [-3, 9]):
                        payload = {'op': 'mirror_vector', 'type': type_name, 'point': [px, py],
                                   'ref': r, 'direction': d}
                        p, q = (px, r[0]) if d == 0 else (py, r[1])
                        if defined_mirror(p, q, lo, hi):
                            out.case(f'mirror_vector-{type_name}-{n}', payload)
                        elif refused < 30:
                            out.control(f'mirror_vector-{type_name}-overflow-{refused}', payload, UNDEFINED)
                            refused += 1
                        n += 1
    n = 0
    for d in directions:
        for p in DOUBLES[::3]:
            for r in DOUBLES[1::4]:
                out.case(f'mirror_vector-VECTOR2D-{n}', {'op': 'mirror_vector', 'type': 'VECTOR2D',
                                                         'point': [p, DOUBLES[(n * 7) % len(DOUBLES)]],
                                                         'ref': [r, p], 'direction': d})
                n += 1
    out.control('mirror_vector-direction-range', {'op': 'mirror_vector', 'type': 'VECTOR2I',
                                                  'point': [1, 2], 'ref': [0, 0], 'direction': 2**31},
                'integer out of range: direction')
    out.control('mirror_vector-point-shape', {'op': 'mirror_vector', 'type': 'VECTOR2I',
                                              'point': [1], 'ref': [0, 0], 'direction': 0},
                'expected point: point')
    out.control('mirror_vector-unknown-type', {'op': 'mirror_vector', 'type': 'VECTOR2F',
                                               'point': [1, 2], 'ref': [0, 0], 'direction': 0},
                'unknown type')
    for d in directions + [rng.randint(*I32) for _ in range(8)]:
        out.case(f'mirror_axis-{d}', {'op': 'mirror_axis', 'direction': d})
    out.control('mirror_axis-range', {'op': 'mirror_axis', 'direction': -2**31 - 1},
                'integer out of range: direction')


def throttle_defined(interval, clock):
    """Whether THROTTLE( interval ) with this clock script evaluates only defined arithmetic."""
    lo, hi = I64
    ns = interval * 1000000
    if not lo <= ns <= hi:
        return False
    last = clock[0] - ns
    if not lo <= last <= hi:
        return False
    for now in clock[1:]:
        elapsed = now - last
        if not lo <= elapsed <= hi:
            return False
        if elapsed >= ns:
            last = now
    return True


def throttle(out, rng):
    lo, hi = I64
    scripts = [
        (500, [0, 1]),
        (200, [0, 0, 1]),
        (50, [0, 0, 60000000]),
        (100, [5, 5, 99999999, 100000004, 100000005, 200000005]),
        (0, [0, 0, 0, -1, 7]),
        (-5, [0, -6000000, -4000000, -10000000]),
        (1, [10, 9, 8, 1000010, 1000009]),
        (9223372036854, [0, 1, 2]),
        (-9223372036854, [0, 1]),
        (100, [hi, hi, hi]),
        (100, [lo + 100000000, lo + 100000000, lo + 200000000]),
        (0, [hi, lo + hi, 0]),
        (3, []),
    ]
    scripts[-1] = (3, [rng.randint(-2**40, 2**40)])
    for i in range(40):
        interval = rng.choice([0, 1, 16, 100, 1000, rng.randint(-10**6, 10**6)])
        t = rng.randint(-2**50, 2**50)
        clock = [t]
        for _ in range(rng.randint(1, 30)):
            t += rng.choice([0, 1, 999999, 1000000, 15999999, 16000000, rng.randint(0, 2 * 10**9)])
            clock.append(t)
        scripts.append((interval, clock))
    n = 0
    for interval, clock in scripts:
        payload = {'op': 'throttle', 'interval_ms': interval, 'clock': clock}
        assert throttle_defined(interval, clock), payload
        out.case(f'throttle-{n}', payload)
        n += 1
    refused = [
        (9223372036855, [0, 1]),
        (-9223372036855, [0]),
        (1, [lo, lo]),
        (-1, [hi]),
        (0, [lo, hi]),
        (1000, [hi, hi, lo]),
        (0, [0, 1, lo]),
    ]
    for i, (interval, clock) in enumerate(refused):
        assert not throttle_defined(interval, clock)
        out.control(f'throttle-overflow-{i}', {'op': 'throttle', 'interval_ms': interval, 'clock': clock}, CHRONO)
    out.control('throttle-empty', {'op': 'throttle', 'interval_ms': 1, 'clock': []}, 'expected array: clock')
    out.control('throttle-long', {'op': 'throttle', 'interval_ms': 1, 'clock': [0] * 4097}, 'array too long: clock')
    out.control('throttle-value', {'op': 'throttle', 'interval_ms': 1, 'clock': [0, 2**63]}, 'integer out of range: clock')


LEAK_VALUES = {
    'int': lambda rng: rng.choice([0, -1, 7, I32[0], I32[1], rng.randint(*I32)]),
    'string': lambda rng: rng.choice(['', 'a', 'leak', 'Ω≈ç', 'x' * 300, str(rng.random())]),
    'vector_int': lambda rng: [rng.randint(*I32) for _ in range(rng.randint(0, 5))],
    'map_string_int': lambda rng: [[rng.choice(['b', 'a', 'c', '', 'Z', 'ä']), rng.randint(-9, 9)]
                                   for _ in range(rng.randint(0, 5))],
}
VIAS = ['get', 'deref', 'arrow', 'convert']


def leak(out, rng):
    fixed = {
        'shared_string': [
            [{'step': 'construct', 'slot': 0}, {'step': 'created', 'slot': 0}, {'step': 'use_count'},
             {'step': 'read', 'slot': 0, 'via': 'get'}, {'step': 'created', 'slot': 0},
             {'step': 'share', 'slot': 0, 'via': 'arrow'}, {'step': 'use_count'},
             {'step': 'read', 'slot': 0, 'via': 'deref'}, {'step': 'destroy', 'slot': 0}, {'step': 'use_count'}],
            [{'step': 'construct', 'slot': 0}, {'step': 'construct', 'slot': 1},
             {'step': 'share', 'slot': 0, 'via': 'convert'}, {'step': 'share', 'slot': 1, 'via': 'get'},
             {'step': 'share', 'slot': 1, 'via': 'deref'}, {'step': 'use_count'},
             {'step': 'destroy', 'slot': 1}, {'step': 'destroy', 'slot': 0}, {'step': 'use_count'},
             {'step': 'construct', 'slot': 0}, {'step': 'read', 'slot': 0, 'via': 'arrow'}, {'step': 'use_count'}],
            [{'step': 'construct', 'slot': 1}, {'step': 'destroy', 'slot': 1}, {'step': 'use_count'}],
        ],
        'int': [
            [{'step': 'construct', 'slot': 0}, {'step': 'created', 'slot': 0},
             {'step': 'identity', 'slot': 0}, {'step': 'created', 'slot': 0},
             {'step': 'read', 'slot': 0, 'via': 'convert'}],
            [{'step': 'construct', 'slot': 0}, {'step': 'write', 'slot': 0, 'via': 'deref', 'value': 5},
             {'step': 'construct', 'slot': 1}, {'step': 'created', 'slot': 1},
             {'step': 'read', 'slot': 1, 'via': 'get'}, {'step': 'read', 'slot': 0, 'via': 'arrow'},
             {'step': 'destroy', 'slot': 0}, {'step': 'construct', 'slot': 0},
             {'step': 'created', 'slot': 0}, {'step': 'read', 'slot': 0, 'via': 'get'}],
        ],
    }
    n = 0
    for type_name, scripts in fixed.items():
        for steps in scripts:
            out.case(f'leak-{type_name}-{n}', {'op': 'leak_at_exit', 'type': type_name, 'steps': steps})
            n += 1
    for type_name, make in LEAK_VALUES.items():
        for _ in range(12):
            live = [False, False]
            steps = []
            for _ in range(rng.randint(1, 24)):
                slot = rng.randrange(2)
                if not live[slot]:
                    steps.append({'step': 'construct', 'slot': slot})
                    live[slot] = True
                    continue
                kind = rng.choice(['created', 'identity', 'read', 'write', 'write', 'size', 'destroy'])
                if kind == 'size' and type_name == 'int':
                    kind = 'read'
                step = {'step': kind, 'slot': slot}
                if kind in ('read', 'write'):
                    step['via'] = rng.choice(VIAS)
                if kind == 'write':
                    step['value'] = make(rng)
                if kind == 'destroy':
                    live[slot] = False
                steps.append(step)
            out.case(f'leak-{type_name}-{n}', {'op': 'leak_at_exit', 'type': type_name, 'steps': steps})
            n += 1
    bad = [
        ('unknown-type', {'type': 'float', 'steps': []}, 'unknown type'),
        ('not-constructed', {'type': 'int', 'steps': [{'step': 'created', 'slot': 0}]}, 'slot not constructed'),
        ('twice', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 1}, {'step': 'construct', 'slot': 1}]},
         'slot already constructed'),
        ('slot-range', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 2}]}, 'integer out of range: slot'),
        ('use-count-type', {'type': 'string', 'steps': [{'step': 'use_count'}]}, 'use_count needs shared_string'),
        ('share-type', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 0},
                                                 {'step': 'share', 'slot': 0, 'via': 'get'}]},
         'share needs shared_string'),
        ('size-int', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 0}, {'step': 'size', 'slot': 0}]},
         'size needs a container'),
        ('size-shared', {'type': 'shared_string', 'steps': [{'step': 'construct', 'slot': 0},
                                                            {'step': 'size', 'slot': 0}]},
         'size needs a container'),
        ('write-shared', {'type': 'shared_string', 'steps': [{'step': 'construct', 'slot': 0},
                                                             {'step': 'write', 'slot': 0, 'via': 'get',
                                                              'value': 'x'}]},
         'shared values are set by share'),
        ('accessor', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 0},
                                               {'step': 'read', 'slot': 0, 'via': 'star'}]}, 'unknown accessor'),
        ('write-accessor', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 0},
                                                     {'step': 'write', 'slot': 0, 'via': 'star', 'value': 1}]},
         'unknown accessor'),
        ('write-value', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 0},
                                                  {'step': 'write', 'slot': 0, 'via': 'get', 'value': 'x'}]},
         'integer out of range: value'),
        ('map-entry', {'type': 'map_string_int', 'steps': [{'step': 'construct', 'slot': 0},
                                                           {'step': 'write', 'slot': 0, 'via': 'get',
                                                            'value': [['a']]}]}, 'expected entry: value'),
        ('step-object', {'type': 'int', 'steps': [{'slot': 0}]}, 'expected step object'),
        ('unknown-step', {'type': 'int', 'steps': [{'step': 'construct', 'slot': 0}, {'step': 'free', 'slot': 0}]},
         'unknown step'),
        ('missing-slot', {'type': 'int', 'steps': [{'step': 'free'}]}, 'unexpected or missing field'),
    ]
    for name, payload, message in bad:
        out.control(f'leak-{name}', {'op': 'leak_at_exit', **payload}, message)


SIGNATURES = {
    'i64(i32)': (['add', 'mul', 'shared', 'plain', 'big'], 1),
    'i64(i32,i32)': (['add', 'mul', 'shared', 'plain', 'big'], 2),
    'i64()': (['add', 'shared', 'plain', 'big'], 0),
    'string(string)': (['add', 'shared', 'plain'], 1),
}


def callable_spec(signature, kind, rng):
    spec = {'kind': kind}
    if kind in ('add', 'mul') or (kind == 'shared' and signature in ('i64(i32)', 'i64()')):
        if signature == 'string(string)':
            spec['s'] = rng.choice(['', '!', '-tail', 'Ω'])
        else:
            spec['k'] = rng.choice([0, 1, -1, I32[0], I32[1], rng.randint(*I32)])
    if kind == 'big':
        spec['values'] = [rng.randint(*I32) for _ in range(12)]
    return spec


def args_for(signature, rng):
    if signature == 'string(string)':
        return [rng.choice(['', 'abc', 'Ω≈', 'y' * 70])]
    count = SIGNATURES[signature][1]
    return [rng.choice([0, 1, -1, I32[0], I32[1], rng.randint(*I32)]) for _ in range(count)]


def token_for(signature, rng):
    if signature == 'string(string)':
        return rng.choice(['', 'tok-', 'ƒ'])
    return rng.choice([0, 1, -(2**40), 2**40, rng.randint(-2**40, 2**40)])


def function(out, rng):
    qa = [
        # The pinned qa/tests/common/test_inplace_function.cpp scenarios, on these signatures.
        ('i64()', 'default', 3, [
            {'step': 'construct_empty', 'slot': 0}, {'step': 'bool', 'slot': 0},
            {'step': 'construct', 'slot': 1, 'callable': {'kind': 'shared', 'k': 3}},
            {'step': 'move_assign', 'slot': 0, 'from': 1}, {'step': 'bool', 'slot': 0},
            {'step': 'invoke', 'slot': 0, 'args': []},
            {'step': 'construct', 'slot': 2, 'callable': {'kind': 'add', 'k': 10}},
            {'step': 'destroy', 'slot': 1},
            {'step': 'move_construct', 'slot': 1, 'from': 2},
            {'step': 'move_assign', 'slot': 0, 'from': 1}, {'step': 'use_count'},
            {'step': 'invoke', 'slot': 0, 'args': []}]),
        ('i64()', 'default', 7, [
            {'step': 'construct', 'slot': 0, 'callable': {'kind': 'shared', 'k': 0}},
            {'step': 'copy_construct', 'slot': 1, 'from': 0}, {'step': 'use_count'},
            {'step': 'invoke', 'slot': 1, 'args': []}, {'step': 'invoke', 'slot': 0, 'args': []}]),
        ('i64()', 'default', 9, [
            {'step': 'construct', 'slot': 0, 'callable': {'kind': 'shared', 'k': 0}},
            {'step': 'move_construct', 'slot': 1, 'from': 0}, {'step': 'use_count'},
            {'step': 'invoke', 'slot': 1, 'args': []}, {'step': 'bool', 'slot': 0},
            {'step': 'invoke', 'slot': 0, 'args': []}]),
        ('i64()', 'default', 11, [
            {'step': 'construct', 'slot': 0, 'callable': {'kind': 'shared', 'k': 0}},
            {'step': 'copy_assign', 'slot': 0, 'from': 0}, {'step': 'use_count'},
            {'step': 'invoke', 'slot': 0, 'args': []},
            {'step': 'move_assign', 'slot': 0, 'from': 0}, {'step': 'use_count'},
            {'step': 'invoke', 'slot': 0, 'args': []}]),
        ('i64()', 'default', 0, [
            {'step': 'construct_empty', 'slot': 0}, {'step': 'invoke', 'slot': 0, 'args': []},
            {'step': 'copy_construct', 'slot': 1, 'from': 0}, {'step': 'bool', 'slot': 1},
            {'step': 'copy_assign', 'slot': 0, 'from': 1}, {'step': 'move_assign', 'slot': 1, 'from': 0}]),
        ('string(string)', 'default', 'ok', [
            {'step': 'construct', 'slot': 0, 'callable': {'kind': 'shared'}},
            {'step': 'invoke', 'slot': 0, 'args': ['']}]),
        ('i64(i32)', 64, 2, [
            {'step': 'construct', 'slot': 0, 'callable': {'kind': 'big', 'values': list(range(12))}},
            {'step': 'invoke', 'slot': 0, 'args': [5]},
            {'step': 'copy_construct', 'slot': 1, 'from': 0},
            {'step': 'assign', 'slot': 0, 'callable': {'kind': 'shared', 'k': 1}},
            {'step': 'use_count'}, {'step': 'invoke', 'slot': 0, 'args': [5]},
            {'step': 'invoke', 'slot': 1, 'args': [5]}]),
    ]
    n = 0
    for signature, capacity, token, steps in qa:
        out.case(f'inplace-qa-{n}', {'op': 'inplace_function', 'signature': signature,
                                     'capacity': capacity, 'token': token, 'steps': steps})
        n += 1
    for signature, (kinds, _) in SIGNATURES.items():
        for capacity in ('default', 64):
            for _ in range(18):
                live = [False] * 4
                steps = []
                for _ in range(rng.randint(1, 30)):
                    slot = rng.randrange(4)
                    available = [k for k in kinds if k != 'big' or capacity == 64]
                    if not live[slot]:
                        choice = rng.choice(['construct', 'construct', 'construct_empty', 'copy', 'move'])
                        sources = [i for i in range(4) if live[i]]
                        if choice in ('copy', 'move') and sources:
                            steps.append({'step': f'{choice}_construct', 'slot': slot, 'from': rng.choice(sources)})
                        elif choice == 'construct_empty':
                            steps.append({'step': 'construct_empty', 'slot': slot})
                        else:
                            steps.append({'step': 'construct', 'slot': slot,
                                          'callable': callable_spec(signature, rng.choice(available), rng)})
                        live[slot] = True
                        continue
                    kind = rng.choice(['bool', 'invoke', 'invoke', 'assign', 'copy_assign', 'move_assign',
                                       'destroy', 'use_count'])
                    if kind == 'use_count':
                        steps.append({'step': 'use_count'})
                    elif kind in ('bool', 'destroy'):
                        steps.append({'step': kind, 'slot': slot})
                        if kind == 'destroy':
                            live[slot] = False
                    elif kind == 'invoke':
                        steps.append({'step': 'invoke', 'slot': slot, 'args': args_for(signature, rng)})
                    elif kind == 'assign':
                        steps.append({'step': 'assign', 'slot': slot,
                                      'callable': callable_spec(signature, rng.choice(available), rng)})
                    else:
                        sources = [i for i in range(4) if live[i]]
                        steps.append({'step': kind, 'slot': slot, 'from': rng.choice(sources)})
                out.case(f'inplace-{n}', {'op': 'inplace_function', 'signature': signature,
                                          'capacity': capacity, 'token': token_for(signature, rng),
                                          'steps': steps})
                n += 1

    def fn(signature='i64(i32)', capacity='default', token=0, steps=()):
        return {'op': 'inplace_function', 'signature': signature, 'capacity': capacity,
                'token': token, 'steps': list(steps)}

    big = {'kind': 'big', 'values': [0] * 12}
    bad = [
        ('big-default', fn(steps=[{'step': 'construct', 'slot': 0, 'callable': big}]),
         'callable too large for capacity'),
        ('big-assign-default', fn('i64()', steps=[{'step': 'construct_empty', 'slot': 0},
                                                  {'step': 'assign', 'slot': 0, 'callable': big}]),
         'callable too large for capacity'),
        ('big-string', fn('string(string)', 64, 'x', [{'step': 'construct', 'slot': 0, 'callable': big}]),
         'unknown callable'),
        ('mul-i64', fn('i64()', steps=[{'step': 'construct', 'slot': 0, 'callable': {'kind': 'mul', 'k': 1}}]),
         'unknown callable'),
        ('callable-object', fn(steps=[{'step': 'construct', 'slot': 0, 'callable': 'add'}]),
         'expected callable object'),
        ('callable-fields', fn(steps=[{'step': 'construct', 'slot': 0, 'callable': {'kind': 'add'}}]),
         'unexpected or missing field'),
        ('not-constructed', fn(steps=[{'step': 'bool', 'slot': 3}]), 'slot not constructed'),
        ('twice', fn(steps=[{'step': 'construct_empty', 'slot': 0}, {'step': 'construct_empty', 'slot': 0}]),
         'slot already constructed'),
        ('move-into-live', fn(steps=[{'step': 'construct_empty', 'slot': 0},
                                     {'step': 'move_construct', 'slot': 0, 'from': 0}]),
         'slot already constructed'),
        ('copy-from-dead', fn(steps=[{'step': 'copy_construct', 'slot': 0, 'from': 1}]), 'slot not constructed'),
        ('slot-range', fn(steps=[{'step': 'construct_empty', 'slot': 4}]), 'integer out of range: slot'),
        ('arg-count', fn(steps=[{'step': 'construct_empty', 'slot': 0},
                                {'step': 'invoke', 'slot': 0, 'args': []}]), 'wrong argument count'),
        ('arg-range', fn(steps=[{'step': 'construct_empty', 'slot': 0},
                                {'step': 'invoke', 'slot': 0, 'args': [2**31]}]), 'integer out of range: args'),
        ('arg-string', fn('string(string)', token='t', steps=[{'step': 'construct_empty', 'slot': 0},
                                                              {'step': 'invoke', 'slot': 0, 'args': [1]}]),
         'wrong argument count'),
        ('token-range', fn(token=2**40 + 1), 'integer out of range: token'),
        ('token-string', fn('string(string)', token=1), 'expected string: token'),
        ('capacity', fn(capacity=32), 'unknown capacity'),
        ('signature', fn('i32(i32)'), 'unknown signature'),
        ('unknown-step', fn(steps=[{'step': 'call', 'slot': 0}]), 'unknown step'),
        ('step-object', fn(steps=[[]]), 'expected step object'),
        ('use-count-fields', fn(steps=[{'step': 'use_count', 'slot': 0}]), 'unexpected or missing field'),
    ]
    for name, payload, message in bad:
        out.control(f'inplace-{name}', payload, message)


def general(out):
    out.control('unknown-op', {'op': 'typeid'}, 'unknown operation')
    out.control('op-type', {'op': 3}, 'expected string: op')
    out.control('no-op', {'value': 3}, 'unexpected or missing field')


def generate():
    rng = random.Random(SEED)
    out = Out()
    typeinfo(out)
    mirror(out, rng)
    throttle(out, rng)
    leak(out, rng)
    function(out, rng)
    general(out)
    dump = lambda rows: ''.join(json.dumps(r, ensure_ascii=False, separators=(',', ':')) + '\n' for r in rows)
    return {CASES: dump(out.cases), CONTROLS: dump(out.controls)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for path, text in generate().items():
        if args.check:
            if path.read_bytes() != text.encode():
                raise SystemExit(f'{path} differs from the generator output')
        else:
            path.write_bytes(text.encode())
    print('checked' if args.check else 'written', len(generate()[CASES].splitlines()), 'cases,',
          len(generate()[CONTROLS].splitlines()), 'controls')


if __name__ == '__main__':
    main()
