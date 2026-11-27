#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the fixed-seed libs/sexpr corpora and undefined-domain controls.

Writes port/cases/libs.sexpr.include.sexpr.application.pure.p001.jsonl (header unit) and
port/oracle/fixtures/sexpr-controls.jsonl. With --check, regenerate in memory and fail when
a committed file differs.

Every public case is inside the pinned evaluation's defined domain; a request whose
evaluation would reach undefined behaviour (an out-of-range m_children[] access, a null child
dereference, doScan's INT branch writing a double through an int32_t*) is classified here by
an exact replay of the reference control flow and goes to the controls instead, with the
refusal both adapters must give. Only generated data is used.
"""
import argparse
import json
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
HEADER = 'libs.sexpr.include.sexpr.application.pure.p001'
SOURCE = 'libs.sexpr.application.pure.p001'
CASES = ROOT / 'port/cases'
CONTROLS = ROOT / 'port/oracle/fixtures/sexpr-controls.jsonl'
SEED = 0x5E59_2026
UNDEFINED = {'kind': 'invalid_input', 'message': 'undefined sexpr operation'}
MAX_NODES = 20000
MAX_DEPTH = 4000
MAX_ARGS = 16
INT32 = (-2**31, 2**31 - 1)
INT64 = (-2**63, 2**63 - 1)
# The header unit's Scan/AddChildren/variadic constructor reach doScan/doAddChildren, which
# the source unit owns; in this revision the Rust port defers them.
DEFERRED = {
    'scan': {'upstream': 'SEXPR::SEXPR_LIST::doScan', 'unit': SOURCE},
    'add_children': {'upstream': 'SEXPR::SEXPR_LIST::doAddChildren', 'unit': SOURCE},
}


def hexs(data):
    return data.hex()


def bits(value):
    return struct.pack('>d', value).hex()


def bits_of(pattern):
    return f'{pattern:016x}'


def double_value(text):
    return struct.unpack('>d', bytes.fromhex(text))[0]


# --- Values ---------------------------------------------------------------------------

SPECIAL_DOUBLE_BITS = [
    0x0000000000000000, 0x8000000000000000, 0x7ff0000000000000, 0xfff0000000000000,
    0x7ff8000000000000, 0xfff8000000000000, 0x7ff0000000000001, 0x7ff4000000000000,
    0xfff0000000000001, 0x7fffffffffffffff, 0xffffffffffffffff, 0x7ff8000000000001,
    0x7ff00000e0000000, 0x7ff7ffffe0000000, 0xfff7ffffffffffff, 0x0000000000000001,
    0x8000000000000001, 0x000fffffffffffff, 0x0010000000000000, 0x7fefffffffffffff,
    0xffefffffffffffff, 0x3ff0000000000000, 0xbff0000000000000, 0x47efffffe0000000,
    0x47efffffefffffff, 0x47effffff0000000, 0x47effffff0000001, 0x36a0000000000000,
    0x3690000000000000, 0x3690000000000001, 0x380fffffffffffff,
]

SPECIAL_DOUBLES = [
    1.0, -1.0, 0.5, 1.5, 2.5, 3.14, 0.1, 0.2, 0.3, 42.42, 1e10, -1e10, 9999999999.0,
    9999999999.5, 9999999999.499999, 99999999995.0, 1e-4, -1e-4, 9.99999999995e-5,
    9.9999999995e-05, 0.00010000000005, 1234567890.5, 1234567891.5, 123456789.05,
    0.12345678905, 1e300, -1e300, 1e-300, 1e-5, 1e-10, 1e-11, 5e-11, 4.9999999999e-11,
    5.0000000001e-11, -5e-11, 2.0**53, 2.0**63, -2.0**63, 2.0**64, 1e22, 1e23, 5e-324,
    1.7976931348623157e308, 2.2250738585072014e-308, 123.456, 1e9, 1e9 + 0.5, 999999999.95,
    0.0001234, 0.00012345678905, 10000000000.000488281, 10000000000.00048828125,
    12345678901.000244140625, 1e15 + 0.25, 2.0**-14, 2.0**-11, 3.0517578125e-05,
    100.0, 7.0, 0.0625, 1.0 / 3.0, 2.0 / 3.0, 6.02214076e23, 1.602176634e-19,
]

SPECIAL_INTS = [0, 1, -1, 2, 42, -42, INT32[0], INT32[1], INT32[0] - 1, INT32[1] + 1,
                2**32, 2**32 + 5, -2**32 - 5, INT64[0], INT64[1], INT64[0] + 1, INT64[1] - 1,
                1000000, -123456789, 2**53 + 1, -(2**53) - 1, 0x7fffffff00000000]

SPECIAL_STRINGS = [b'', b'a', b'symbol', b'string', b'with space', b'"quoted"', b'back\\slash',
                   b'line\nbreak', b'tab\there', b'\x00', b'a\x00b', b'\x00lead', b'tail\x00',
                   b'\xff\xfe\x80', b'\xc3\xa9t\xc3\xa9', b'(paren)', b'nan', b'1e10', b'-0',
                   b'0.5', b'#comment', b';', b'\r\n', b'\x7f', bytes(range(256))]


def rand_double(rng):
    choice = rng.random()
    if choice < 0.15:
        return bits_of(rng.choice(SPECIAL_DOUBLE_BITS))
    if choice < 0.35:
        return bits(rng.choice(SPECIAL_DOUBLES))
    if choice < 0.5:
        return bits_of(rng.getrandbits(64))
    if choice < 0.65:
        digits = rng.randint(0, 12)
        return bits(round(rng.uniform(-1e6, 1e6), digits))
    if choice < 0.75:
        return bits(float(rng.randint(-10**12, 10**12)))
    if choice < 0.85:
        mantissa = rng.randint(1, 2**20) * rng.choice((1, -1))
        return bits(mantissa * 2.0 ** rng.randint(-80, 80))
    if choice < 0.95:
        exponent = rng.randint(-20, 20)
        value = 10.0 ** exponent * rng.choice((1, -1))
        pattern = struct.unpack('>Q', struct.pack('>d', value))[0] + rng.randint(-3, 3)
        return bits_of(pattern & 0xffffffffffffffff)
    # A tie at the tenth significant digit or the tenth decimal: k + 0.5 units.
    scale = 10.0 ** rng.randint(-9, 9)
    return bits((rng.randint(10**9, 10**10 - 1) + 0.5) * scale)


def rand_int(rng):
    choice = rng.random()
    if choice < 0.3:
        return rng.choice(SPECIAL_INTS)
    if choice < 0.6:
        return rng.randint(-1000, 1000)
    if choice < 0.8:
        return rng.randint(*INT32)
    return rng.randint(*INT64)


def rand_int32(rng):
    choice = rng.random()
    if choice < 0.3:
        return rng.choice([0, 1, -1, 7, INT32[0], INT32[1], -100, 100000])
    if choice < 0.7:
        return rng.randint(-1000, 1000)
    return rng.randint(*INT32)


def rand_bytes(rng, long=False):
    choice = rng.random()
    if choice < 0.35:
        return rng.choice(SPECIAL_STRINGS)
    if choice < 0.6:
        return bytes(rng.choice(b'abcdefghijklmnopqrstuvwxyz_-.0123456789')
                     for _ in range(rng.randint(1, 12)))
    if long and choice < 0.62:
        return bytes(rng.getrandbits(8) for _ in range(rng.choice((1024, 4096))))
    return bytes(rng.getrandbits(8) for _ in range(rng.randint(0, 40)))


def rand_line(rng):
    choice = rng.random()
    if choice < 0.45:
        return None
    if choice < 0.6:
        return rng.choice([0, 1, -1, INT32[0], INT32[1], 2, 100])
    return rng.randint(-5, 100000)


def atom(rng, kind=None):
    kind = kind or rng.choice(('integer', 'double', 'string', 'symbol'))
    if kind == 'integer':
        return ['integer', rand_int(rng), rand_line(rng)]
    if kind == 'double':
        return ['double', rand_double(rng), rand_line(rng)]
    return [kind, hexs(rand_bytes(rng)), rand_line(rng)]


def tree(rng, max_nodes=40, max_depth=6, nulls=True, root_list=False):
    """A random pre-order tree with at most about max_nodes nodes."""
    budget = [max_nodes]

    def node(depth, root):
        budget[0] -= 1
        if not root and nulls and rng.random() < 0.06:
            return [['null']]
        if (root and root_list) or (depth < max_depth and budget[0] > 0 and rng.random() < 0.35):
            count = rng.choice((0, 1, 1, 2, 2, 3, 4, 5, 8)) if budget[0] > 0 else 0
            count = min(count, max(budget[0], 0))
            out = [['list', rand_line(rng), count]]
            for _ in range(count):
                out += node(depth + 1, False)
            return out
        return [atom(rng)]

    return node(1, True)


def chain(depth, leaf=('integer', 0, None)):
    return [['list', None, 1] for _ in range(depth - 1)] + [list(leaf)]


def wide(rng, count):
    return [['list', None, count]] + [atom(rng) for _ in range(count)]


def children(nodes):
    """Split a list-root tree into its root entry and each child's subtree."""
    out = []
    i = 1
    while i < len(nodes):
        start, pending = i, 1
        while pending:
            if nodes[i][0] == 'list':
                pending += nodes[i][2]
            pending -= 1
            i += 1
        out.append(nodes[start:i])
    return out


def kind_of(subtree):
    return subtree[0][0]


def value_of(subtree):
    return subtree[0][1]


# --- Scan and children arguments --------------------------------------------------------

def scan_arg_for(rng, child, match):
    """A scan argument that reads (match) or deliberately fails on this child."""
    kind = kind_of(child) if child else None
    options = []
    if match:
        if kind in ('integer', 'double'):
            options.append({'kind': 'double', 'initial': rand_double(rng)})
        if kind == 'integer':
            options.append({'kind': 'long_int', 'initial': rand_int(rng)})
        if kind in ('symbol', 'string'):
            options.append({'kind': 'in_string', 'symbol': kind == 'symbol',
                            'initial': hexs(rand_bytes(rng))})
            text = bytes.fromhex(value_of(child))
            options.append({'kind': rng.choice(('const_string', 'string_value')),
                            'value': hexs(text)})
            if b'\x00' not in text:
                options.append({'kind': 'c_str', 'value': hexs(text + rng.choice((b'', b'\x00z')))})
        options.append({'kind': 'string', 'initial': hexs(rand_bytes(rng))})
        if kind in ('list', None) or kind in ('integer', 'double'):
            options.append({'kind': rng.choice(('const_string', 'string_value', 'c_str')),
                            'value': hexs(rand_bytes(rng))})
    else:
        if kind not in ('integer', 'double'):
            options.append({'kind': 'double', 'initial': rand_double(rng)})
        if kind != 'integer':
            options.append({'kind': 'long_int', 'initial': rand_int(rng)})
            options.append({'kind': 'int', 'initial': rand_int32(rng)})
        options.append({'kind': 'in_string', 'symbol': kind != 'symbol',
                        'initial': hexs(rand_bytes(rng))})
        if kind in ('symbol', 'string'):
            other = bytes.fromhex(value_of(child)) + b'!'
            options.append({'kind': rng.choice(('const_string', 'string_value')),
                            'value': hexs(other)})
    return rng.choice(options)


def rand_scan_arg(rng):
    kind = rng.choice(('int', 'long_int', 'double', 'string', 'in_string', 'const_string',
                       'string_value', 'c_str'))
    if kind == 'int':
        return {'kind': kind, 'initial': rand_int32(rng)}
    if kind == 'long_int':
        return {'kind': kind, 'initial': rand_int(rng)}
    if kind == 'double':
        return {'kind': kind, 'initial': rand_double(rng)}
    if kind == 'string':
        return {'kind': kind, 'initial': hexs(rand_bytes(rng))}
    if kind == 'in_string':
        return {'kind': kind, 'symbol': rng.random() < 0.5, 'initial': hexs(rand_bytes(rng))}
    return {'kind': kind, 'value': hexs(rand_bytes(rng))}


def scan_outcome(root, args):
    """Replay doScan over the decoded list: ('ok', returned) or ('undefined', None)."""
    kids = children(root)
    for i, arg in enumerate(args):
        if i >= len(kids) or kind_of(kids[i]) == 'null':
            return 'undefined', None
        child, kind = kids[i], kind_of(kids[i])
        if arg['kind'] == 'double':
            if kind not in ('double', 'integer'):
                return 'ok', i
        elif arg['kind'] == 'int':
            if kind == 'integer':
                return 'undefined', None
            return 'ok', i
        elif arg['kind'] == 'long_int':
            if kind != 'integer':
                return 'ok', i
        elif arg['kind'] == 'in_string':
            if kind != ('symbol' if arg['symbol'] else 'string'):
                return 'ok', i
        elif arg['kind'] in ('const_string', 'string_value', 'c_str'):
            compared = bytes.fromhex(arg['value'])
            if arg['kind'] == 'c_str':
                compared = compared.split(b'\x00')[0]
            if kind in ('symbol', 'string') and bytes.fromhex(value_of(child)) != compared:
                return 'ok', i
    return 'ok', len(args)


def scan_request(rng):
    """A list and arguments that read some prefix and may stop at a mismatch."""
    root = tree(rng, max_nodes=24, max_depth=3, root_list=True)
    kids = children(root)
    count = rng.randint(1, MAX_ARGS)
    args = []
    for i in range(count):
        roll = rng.random()
        if i < len(kids) and kind_of(kids[i]) != 'null' and roll < 0.8:
            args.append(scan_arg_for(rng, kids[i], match=roll < 0.7))
        else:
            args.append(rand_scan_arg(rng))
    return root, args


def rand_children_arg(rng, depth=0):
    kind = rng.choice(('int', 'long_int', 'double', 'string', 'c_str', 'out_string', 'sexpr'))
    if kind == 'int':
        return {'kind': kind, 'value': rand_int32(rng)}
    if kind == 'long_int':
        return {'kind': kind, 'value': rand_int(rng)}
    if kind == 'double':
        return {'kind': kind, 'value': rand_double(rng)}
    if kind in ('string', 'c_str'):
        return {'kind': kind, 'value': hexs(rand_bytes(rng))}
    if kind == 'out_string':
        return {'kind': kind, 'symbol': rng.random() < 0.5, 'value': hexs(rand_bytes(rng))}
    return {'kind': kind, 'tree': None if rng.random() < 0.15 else tree(rng, max_nodes=12)}


def rand_children_args(rng):
    return [rand_children_arg(rng) for _ in range(rng.randint(1, MAX_ARGS))]


# --- Corpora ----------------------------------------------------------------------------

class Corpus:
    def __init__(self, unit):
        self.unit = unit
        self.records = []
        self.names = set()

    def add(self, name, input_):
        assert name not in self.names, name
        self.names.add(name)
        self.records.append({'unit': self.unit, 'case': name, 'input': input_})


def header_cases(rng, controls):
    corpus = Corpus(HEADER)
    # Constructors, predicates and line numbers of every node type.
    for kind in ('integer', 'double', 'string', 'symbol'):
        for line in (None, 0, 1, -1, 2, INT32[0], INT32[1], 12345):
            corpus.add(f'atom-{kind}-line-{line}', {'op': 'predicates', 'tree': [
                [kind, {'integer': 7, 'double': bits(2.5)}.get(kind, hexs(b'v')), line]]})
    for i, pattern in enumerate(SPECIAL_DOUBLE_BITS):
        corpus.add(f'atom-double-bits-{i}', {'op': 'predicates', 'tree': [
            ['double', bits_of(pattern), None]]})
    for i, value in enumerate(SPECIAL_INTS):
        corpus.add(f'atom-integer-{i}', {'op': 'predicates', 'tree': [['integer', value, 3]]})
    for i, text in enumerate(SPECIAL_STRINGS):
        for kind in ('string', 'symbol'):
            corpus.add(f'atom-{kind}-{i}', {'op': 'predicates', 'tree': [[kind, hexs(text), None]]})
    for line in (None, 0, -1, INT32[0], INT32[1]):
        corpus.add(f'list-empty-line-{line}', {'op': 'predicates', 'tree': [['list', line, 0]]})
    corpus.add('list-with-nulls', {'op': 'predicates', 'tree': [
        ['list', None, 4], ['null'], ['list', 5, 2], ['null'], ['null'], ['integer', 1, None],
        ['null']]})
    corpus.add('chain-max-depth', {'op': 'predicates', 'tree': chain(MAX_DEPTH)})
    corpus.add('chain-max-depth-null-leaf', {'op': 'predicates', 'tree':
                                             chain(MAX_DEPTH - 1, ('list', 9, 1)) + [['null']]})
    corpus.add('wide-2000', {'op': 'predicates', 'tree': wide(rng, 2000)})
    for i in range(400):
        corpus.add(f'predicates-random-{i}', {'op': 'predicates', 'tree': tree(rng)})
    long_tree = [['list', None, 3], ['string', hexs(bytes(range(256)) * 256), -7],
                 ['symbol', hexs(b'\x00' * 1000), None], ['double', bits_of(0x7ff4000000000123), 1]]
    corpus.add('long-strings', {'op': 'predicates', 'tree': long_tree})

    # SEXPR_LIST constructors.
    corpus.add('construct-default', {'op': 'construct_list', 'ctor': 'default'})
    for line in (0, 1, -1, 2, 99, INT32[0], INT32[1], -12345):
        corpus.add(f'construct-line-{line}', {'op': 'construct_list', 'ctor': 'line', 'line': line})
    for i in range(160):
        corpus.add(f'construct-variadic-{i}', {'op': 'construct_list', 'ctor': 'variadic',
                                               'args': rand_children_args(rng)})

    # Exceptions.
    for cls in ('PARSE_EXCEPTION', 'INVALID_TYPE_EXCEPTION'):
        for i, text in enumerate(SPECIAL_STRINGS + [b'SEXPR is not a list type!',
                                                    b'x' * 5000, b'\x00\x00', b'%s%n']):
            corpus.add(f'exception-{cls}-{i}', {'op': 'exception', 'class': cls,
                                                 'message': hexs(text)})

    # ISEXPRABLE defaults.
    for i in range(40):
        corpus.add(f'isexprable-{i}', {'op': 'isexprable', 'tree': tree(rng, max_nodes=20)})

    # AsSymbol / AsString wrappers.
    for i, text in enumerate(SPECIAL_STRINGS):
        for which in ('symbol', 'string'):
            corpus.add(f'out-{which}-{i}', {'op': 'out_string', 'which': which,
                                            'value': hexs(text)})
            corpus.add(f'in-{which}-{i}', {'op': 'in_string', 'which': which, 'value': hexs(text),
                                           'assign': hexs(rand_bytes(rng))})

    # Scan and AddChildren templates.
    count = 0
    while count < 300:
        root, args = scan_request(rng)
        outcome, _ = scan_outcome(root, args)
        if outcome == 'undefined':
            if len(controls) < 40:
                controls.append(control(HEADER, f'control-scan-{len(controls)}',
                                        {'op': 'scan', 'tree': root, 'args': args},
                                        rust_unported=DEFERRED['scan']))
            continue
        corpus.add(f'scan-{count}', {'op': 'scan', 'tree': root, 'args': args})
        count += 1
    for i in range(160):
        root = tree(rng, max_nodes=16, root_list=True)
        corpus.add(f'add-children-{i}', {'op': 'add_children', 'tree': root,
                                         'args': rand_children_args(rng)})
    return corpus


def control(unit, name, input_, error=None, rust_unported=None):
    record = {'error': error or UNDEFINED, 'request': {'unit': unit, 'case': name, 'input': input_}}
    if rust_unported:
        record['rust_unported'] = rust_unported
    return record


def fixed_controls():
    """Refusals at the request bounds and each undefined scan branch, equal on both sides."""
    out = []
    deep = chain(MAX_DEPTH + 1)
    out.append(control(HEADER, 'control-tree-too-deep', {'op': 'predicates', 'tree': deep},
                       {'kind': 'invalid_input', 'message': 'tree too deep'}))
    big = [['list', None, MAX_NODES]] + [['integer', i, None] for i in range(MAX_NODES)]
    out.append(control(HEADER, 'control-tree-too-large', {'op': 'predicates', 'tree': big},
                       {'kind': 'invalid_input', 'message': 'tree too large'}))
    many = [{'kind': 'string', 'initial': ''} for _ in range(MAX_ARGS + 1)]
    out.append(control(HEADER, 'control-scan-17-args', {'op': 'scan', 'tree': [['list', None, 0]],
                                                        'args': many},
                       {'kind': 'invalid_input', 'message': 'argument count out of range'}))
    out.append(control(HEADER, 'control-add-0-args', {'op': 'add_children',
                                                      'tree': [['list', None, 0]], 'args': []},
                       {'kind': 'invalid_input', 'message': 'argument count out of range'}))
    out.append(control(HEADER, 'control-scan-past-end',
                       {'op': 'scan', 'tree': [['list', None, 1], ['symbol', '61', None]],
                        'args': [{'kind': 'string', 'initial': ''},
                                 {'kind': 'string', 'initial': ''}]},
                       rust_unported=DEFERRED['scan']))
    out.append(control(HEADER, 'control-scan-empty-list',
                       {'op': 'scan', 'tree': [['list', None, 0]],
                        'args': [{'kind': 'double', 'initial': bits(1.0)}]},
                       rust_unported=DEFERRED['scan']))
    out.append(control(HEADER, 'control-scan-null-child',
                       {'op': 'scan', 'tree': [['list', None, 2], ['integer', 1, None], ['null']],
                        'args': [{'kind': 'long_int', 'initial': 0},
                                 {'kind': 'string', 'initial': ''}]},
                       rust_unported=DEFERRED['scan']))
    out.append(control(HEADER, 'control-scan-int-writes-double',
                       {'op': 'scan', 'tree': [['list', None, 1], ['integer', 5, None]],
                        'args': [{'kind': 'int', 'initial': 0}]},
                       rust_unported=DEFERRED['scan']))
    return out


def lines(records):
    return ''.join(json.dumps(r, separators=(',', ':'), sort_keys=True) + '\n' for r in records)


def generate():
    rng = random.Random(SEED)
    controls = []
    header = header_cases(rng, controls)
    controls = fixed_controls() + controls
    return {
        CASES / f'{HEADER}.jsonl': lines(header.records),
        CONTROLS: lines(controls),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    outputs = generate()
    stale = []
    for path, text in outputs.items():
        if args.check:
            if not path.exists() or path.read_bytes() != text.encode('utf-8'):
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.write_bytes(text.encode('utf-8'))
            print(f'{path.relative_to(ROOT)}: {text.count(chr(10))} records')
    if stale:
        sys.exit('stale generated files: ' + ', '.join(stale))


if __name__ == '__main__':
    main()
