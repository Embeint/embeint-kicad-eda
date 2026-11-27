#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the fixed-seed libs/sexpr corpora and undefined-domain controls.

Writes port/cases/libs.sexpr.include.sexpr.application.pure.p001.jsonl (header unit),
port/cases/libs.sexpr.application.pure.p001.jsonl (source unit, sexpr.cpp) and
port/oracle/fixtures/sexpr-controls.jsonl. With --check, regenerate in memory and fail when
a committed file differs.

Every public case is inside the pinned evaluation's defined domain and the adapters'
resource bounds; a request whose evaluation would reach undefined behaviour (an out-of-range
m_children[] access, a null child dereference, doScan's INT branch writing a double through
an int32_t*) or exceed an AsString bound is classified here by an exact replay of the
reference control flow and the adapters' guards, and goes to the controls instead, with the
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
                                        {'op': 'scan', 'tree': root, 'args': args}))
            continue
        corpus.add(f'scan-{count}', {'op': 'scan', 'tree': root, 'args': args})
        count += 1
    for i in range(160):
        root = tree(rng, max_nodes=16, root_list=True)
        corpus.add(f'add-children-{i}', {'op': 'add_children', 'tree': root,
                                         'args': rand_children_args(rng)})
    return corpus


def control(unit, name, input_, error=None):
    return {'error': error or UNDEFINED, 'request': {'unit': unit, 'case': name, 'input': input_}}


# --- Source unit (sexpr.cpp) --------------------------------------------------------------

# std::string::max_size() of the reference build (GCC 13.3 libstdc++ headers, C++20, LP64:
# ( size_t( -1 ) - 1 ) / 2) and the AsString request bounds (identical in both adapters).
# GCC 14 and later headers give 2^63 - 2, which moves only the levels whose indentation is
# 2^63 - 2 beside the newline (port/findings/sexpr-string-max-size-toolchain.md).
STRING_MAX = 2**63 - 1
MAX_INDENT = 1 << 20
MAX_OUTPUT = 64 << 20
MAX_WORK = 1 << 30
TOO_LARGE = {'kind': 'invalid_input', 'message': 'as_string output too large'}
MAX_SOURCE_CONTROLS = 30

SPECIAL_FLOAT_BITS = [
    0x00000000, 0x80000000, 0x7f800000, 0xff800000, 0x7fc00000, 0xffc00000, 0x7f800001,
    0x7fa00000, 0xff800001, 0x7fffffff, 0xffffffff, 0x00000001, 0x80000001, 0x007fffff,
    0x00800000, 0x7f7fffff, 0xff7fffff, 0x3f800000, 0xbf800000, 0x3eaaaaab, 0x4b800001,
]

# AsString levels: the default argument (None), small levels, the indentation bound, and
# levels whose size_t indentation wraps or exceeds max_size() (length_error). 2^62 - 1 indents
# by max_size() - 1, the largest append accepts beside the newline; 2^62 throws.
SPECIAL_LEVELS = [None, 0, 1, 2, 3, 7, 100, 2**19, 2**61, 2**62 - 2, 2**62 - 1, 2**62,
                  2**63 - 1, 2**63, 2**63 + 1, 2**63 + 2**61, 2**63 + 2**62, 2**64 - 1]
# The same boundary reached through the wrapping size_t product (2^63 + 2^62 throws). Only the
# fixed shapes use it, so the random cases' level draws are unchanged.
WRAPPED_BOUNDARY_LEVELS = [2**63 + 2**62 - 1]


def float_bits(pattern):
    return f'{pattern:08x}'


def rand_float(rng):
    choice = rng.random()
    if choice < 0.3:
        return float_bits(rng.choice(SPECIAL_FLOAT_BITS))
    if choice < 0.6:
        return float_bits(rng.getrandbits(32))
    return struct.pack('>f', struct.unpack('>f', struct.pack('>f', rng.uniform(-1e6, 1e6)))[0]).hex()


def parse(nodes):
    """The nested form of a pre-order tree: None, ['list', entry, children] or ['atom', entry]."""
    root = None
    stack = []
    for entry in nodes:
        if entry[0] == 'null':
            node = None
        elif entry[0] == 'list':
            node = ['list', entry, []]
        else:
            node = ['atom', entry]
        if stack:
            stack[-1][0][2].append(node)
            stack[-1][1] -= 1
        else:
            root = node
        if node is not None and node[0] == 'list' and entry[2] > 0:
            stack.append([node, entry[2]])
        while stack and stack[-1][1] == 0:
            stack.pop()
    return root


def located(root):
    """Every non-null node with its m_children path from the root, in pre-order."""
    out = []
    stack = [([], root)]
    while stack:
        path, node = stack.pop()
        out.append((path, node))
        if node[0] == 'list':
            for i in range(len(node[2]) - 1, -1, -1):
                if node[2][i] is not None:
                    stack.append((path + [i], node[2][i]))
    return out


def as_string_outcome(root, level):
    """Replay the adapters' AsString guard: 'ok', 'length_error' (thrown by the library),
    'undefined' (a null child) or 'too_large' (a resource bound)."""
    total = work = 0
    stack = [(root, level, 1, False)]
    while stack:
        node, at, depth, separated = stack.pop()
        size = 1 if separated else 0
        if node is None:
            return 'undefined'
        if node[0] == 'list':
            local = 1 if at else 0
            indent = (at * 2) % 2**64
            if STRING_MAX - local < indent:
                return 'length_error'
            if indent > MAX_INDENT:
                return 'too_large'
            size += indent + 3
            kids = node[2]
            for i in range(len(kids) - 1, -1, -1):
                stack.append((kids[i], (at + 1) % 2**64, depth + 1, i > 0))
        else:
            kind, value = node[1][0], node[1][1]
            if kind in ('string', 'symbol'):
                size += len(value) // 2 + 2
            else:
                size += 12 if kind == 'integer' else 400
        total += size
        work += size * depth
        if total > MAX_OUTPUT or work > MAX_WORK:
            return 'too_large'
    return 'ok'


def getter_error(root, path, method, extra):
    """The refusal a getter request must get, or None when its evaluation is defined."""
    node = None
    for candidate_path, candidate in located(root):
        if candidate_path == path:
            node = candidate
            break
    assert node is not None, path
    if method == 'GetChild' and node[0] == 'list' and extra['index'] >= len(node[2]):
        return UNDEFINED
    if method == 'AsString':
        outcome = as_string_outcome(node, extra['level'] or 0)
        if outcome == 'undefined':
            return UNDEFINED
        if outcome == 'too_large':
            return TOO_LARGE
    return None


def getter(nodes, path, method, **extra):
    return {'op': 'getter', 'tree': nodes, 'path': path, 'method': method, **extra}


GETTERS = ['GetChildren', 'GetChild', 'GetNumberOfChildren', 'GetLongInteger', 'GetInteger',
           'GetFloat', 'GetDouble', 'GetString', 'GetSymbol', 'GetList', 'AsString', 'AddChild']


def getter_extra(rng, node, method):
    if method == 'GetChild':
        if node[0] == 'list' and node[2] and rng.random() < 0.9:
            return {'index': rng.randrange(len(node[2]))}
        return {'index': rng.choice((0, 1, 5, 2**32, 2**64 - 1))}
    if method == 'AsString':
        return {'level': rng.choice(SPECIAL_LEVELS) if rng.random() < 0.3
                else rng.choice((None, 0, rng.randint(0, 40)))}
    if method == 'AddChild':
        roll = rng.random()
        return {'child': None if roll < 0.15 else chain(rng.randint(2, 30)) if roll < 0.25
                else tree(rng, max_nodes=12)}
    return {}


def item(rng):
    """One operator<< right operand."""
    kind = rng.choice(('double', 'float', 'int64', 'int32', 'string', 'out_string',
                       'isexprable', 'list', 'sexpr'))
    if kind == 'double':
        return {'kind': kind, 'value': rand_double(rng)}
    if kind == 'float':
        return {'kind': kind, 'value': rand_float(rng)}
    if kind == 'int64':
        return {'kind': kind, 'value': rand_int(rng)}
    if kind == 'int32':
        return {'kind': kind, 'value': rand_int32(rng)}
    if kind == 'string':
        return {'kind': kind, 'value': hexs(rand_bytes(rng))}
    if kind == 'out_string':
        return {'kind': kind, 'symbol': rng.random() < 0.5, 'value': hexs(rand_bytes(rng))}
    if kind == 'isexprable':
        return {'kind': kind}
    if kind == 'list':
        return {'kind': kind, 'tree': None if rng.random() < 0.15
                else tree(rng, max_nodes=10, root_list=True)}
    return {'kind': kind, 'tree': None if rng.random() < 0.15 else tree(rng, max_nodes=10)}


def target(rng, kind=None):
    """One operator>> right operand with a random initial value."""
    kind = kind or rng.choice(('int32', 'int64', 'float', 'double', 'string', 'in_string',
                               'isexprable'))
    if kind == 'int32':
        return {'kind': kind, 'initial': rand_int32(rng)}
    if kind == 'int64':
        return {'kind': kind, 'initial': rand_int(rng)}
    if kind == 'float':
        return {'kind': kind, 'initial': rand_float(rng)}
    if kind == 'double':
        return {'kind': kind, 'initial': rand_double(rng)}
    if kind == 'string':
        return {'kind': kind, 'initial': hexs(rand_bytes(rng))}
    if kind == 'in_string':
        return {'kind': kind, 'symbol': rng.random() < 0.5, 'initial': hexs(rand_bytes(rng))}
    return {'kind': kind}


def target_for(rng, child_kind):
    """A target the operator for child_kind accepts."""
    options = {'integer': ['int32', 'int64'], 'double': ['float', 'double'],
               'string': ['string', 'in_string'], 'symbol': ['in_string']}.get(child_kind)
    if not options:
        return target(rng)
    out = target(rng, rng.choice(options))
    if out['kind'] == 'in_string':
        out['symbol'] = child_kind == 'symbol'
    return out


def extract_outcome(nodes, targets):
    """Replay the operator>> chain: 'undefined' when a dereferenced child is past the end or
    null, otherwise 'ok' (the chain may stop at an exception)."""
    kids = children(nodes)
    index = 0
    for t in targets:
        if t['kind'] == 'isexprable':
            continue
        if index >= len(kids) or kind_of(kids[index]) == 'null':
            return 'undefined'
        kind = kind_of(kids[index])
        accepted = {'int32': kind == 'integer', 'int64': kind == 'integer',
                    'float': kind == 'double', 'double': kind == 'double',
                    'string': kind == 'string',
                    'in_string': kind == ('symbol' if t.get('symbol') else 'string')}[t['kind']]
        if not accepted:
            return 'ok'
        index += 1
    return 'ok'


def extract_request(rng):
    count = rng.randint(0, 10)
    kids = []
    for _ in range(count):
        roll = rng.random()
        kids.append([['null']] if roll < 0.04 else
                    tree(rng, max_nodes=4, root_list=True) if roll < 0.12 else [atom(rng)])
    nodes = [['list', rand_line(rng), count]] + [n for k in kids for n in k]
    targets = []
    index = 0
    for _ in range(rng.randint(1, MAX_ARGS)):
        roll = rng.random()
        if roll < 0.08:
            targets.append({'kind': 'isexprable'})
            continue
        if index < count and roll < 0.85:
            targets.append(target_for(rng, kind_of(kids[index])))
        else:
            targets.append(target(rng))
        index += 1
    return nodes, targets


def source_cases(rng, controls):
    corpus = Corpus(SOURCE)

    def add(name, input_, error=None):
        if error is None:
            corpus.add(name, input_)
        elif len(controls) < MAX_SOURCE_CONTROLS:
            controls.append(control(SOURCE, f'control-source-{name}', input_, error))

    # Typed accessors and AsString on every atom type, with each special value.
    lines_ = [None, 0, 1, -1, INT32[0], INT32[1], 77]
    for i, value in enumerate(SPECIAL_INTS):
        nodes = [['integer', value, rng.choice(lines_)]]
        for method in ('GetLongInteger', 'GetInteger', 'GetDouble', 'GetFloat'):
            add(f'integer-{i}-{method}', getter(nodes, [], method))
        add(f'integer-{i}-AsString', getter(nodes, [], 'AsString', level=None))
    doubles = [bits_of(b) for b in SPECIAL_DOUBLE_BITS] + [bits(v) for v in SPECIAL_DOUBLES]
    # The general/fixed decision points of {:.10g}: a few ulps around each power of ten.
    for exponent in range(-12, 13):
        base = struct.unpack('>Q', struct.pack('>d', 10.0 ** exponent))[0]
        doubles += [bits_of(base + delta) for delta in (-2, -1, 0, 1, 2)]
    for i, value in enumerate(doubles):
        nodes = [['double', value, rng.choice(lines_)]]
        for method in ('GetDouble', 'GetFloat'):
            add(f'double-{i}-{method}', getter(nodes, [], method))
        add(f'double-{i}-AsString', getter(nodes, [], 'AsString', level=None))
    for i in range(1500):
        add(f'double-random-{i}-AsString',
            getter([['double', rand_double(rng), None]], [], 'AsString',
                   level=rng.choice((None, 0, 3))))
    for i in range(300):
        add(f'double-random-{i}-GetFloat', getter([['double', rand_double(rng), None]], [],
                                                  'GetFloat'))
    for i, text in enumerate(SPECIAL_STRINGS):
        for kind in ('string', 'symbol'):
            nodes = [[kind, hexs(text), rng.choice(lines_)]]
            for method in ('GetString', 'GetSymbol', 'AsString'):
                extra = {'level': None} if method == 'AsString' else {}
                add(f'{kind}-{i}-{method}', getter(nodes, [], method, **extra))

    # Every member on every node type: the type checks and their messages (GetSymbol names
    # the stored line number).
    samples = {'list': [['list', None, 2], ['integer', 1, None], ['null']],
               'integer': [['integer', -3, None]], 'double': [['double', bits(0.5), None]],
               'string': [['string', hexs(b's'), None]], 'symbol': [['symbol', hexs(b'y'), None]]}
    for kind, nodes in samples.items():
        for line in (None, 0, -1, INT32[0], INT32[1], 4242):
            # The root with this line: a list's line is its second field, an atom's its third.
            first = nodes[0]
            entry = ['list', line, first[2]] if kind == 'list' else [first[0], first[1], line]
            tree_ = [entry] + nodes[1:]
            for method in GETTERS:
                extra = {'GetChild': {'index': 1}, 'AsString': {'level': 0},
                         'AddChild': {'child': [['symbol', hexs(b'c'), 9]]}}.get(method, {})
                add(f'type-{kind}-line-{line}-{method}', getter(tree_, [], method, **extra),
                    getter_error(parse(tree_), [], method, extra))

    # Random trees: a random member on a random node.
    for i in range(900):
        nodes = tree(rng, max_nodes=30, max_depth=5)
        root = parse(nodes)
        path, node = rng.choice(located(root))
        method = rng.choice(GETTERS)
        extra = getter_extra(rng, node, method)
        add(f'getter-random-{i}', getter(nodes, path, method, **extra),
            getter_error(root, path, method, extra))

    # AsString on lists: the upstream AsStringList example, nesting, separators, levels and
    # deep or wide trees within the bounds.
    upstream = [['list', None, 5], ['symbol', hexs(b'symbol'), None], ['list', None, 3],
                ['integer', 2, None], ['double', bits(42.42), None],
                ['string', hexs(b'substring'), None], ['integer', 1, None],
                ['double', bits(3.14), None], ['string', hexs(b'string'), None]]
    add('as-string-upstream-example', getter(upstream, [], 'AsString', level=None))
    add('as-string-upstream-example-sublist', getter(upstream, [1], 'AsString', level=None))
    shapes = {
        'empty': [['list', None, 0]],
        'nested-empty': [['list', None, 2], ['list', None, 0], ['list', None, 1], ['list', 3, 0]],
        'chain-500': chain(500),
        'chain-1000': chain(1000, ('string', hexs(b'leaf'), None)),
        'wide-2000': wide(rng, 2000),
        'long-strings': [['list', None, 2], ['string', hexs(bytes(range(256)) * 200), None],
                         ['symbol', hexs(b'\x00' * 4000), None]],
    }
    for name, nodes in shapes.items():
        for level in SPECIAL_LEVELS + WRAPPED_BOUNDARY_LEVELS:
            root = parse(nodes)
            extra = {'level': level}
            add(f'as-string-{name}-level-{level}', getter(nodes, [], 'AsString', **extra),
                getter_error(root, [], 'AsString', extra))
    for i in range(400):
        nodes = tree(rng, max_nodes=40, max_depth=6, nulls=False)
        root = parse(nodes)
        path, node = rng.choice(located(root))
        extra = {'level': rng.choice(SPECIAL_LEVELS) if rng.random() < 0.25
                 else rng.choice((None, 0, 1, rng.randint(0, 60)))}
        add(f'as-string-random-{i}', getter(nodes, path, 'AsString', **extra),
            getter_error(root, path, 'AsString', extra))

    # GetChild on every index of lists holding null children, AddChild of null and deep
    # children, GetChildren of a wide list.
    with_nulls = [['list', 5, 4], ['null'], ['integer', 1, None], ['null'], ['list', None, 0]]
    for index in range(4):
        add(f'get-child-nulls-{index}', getter(with_nulls, [], 'GetChild', index=index))
    for method in ('GetChildren', 'GetNumberOfChildren', 'GetList'):
        add(f'nulls-{method}', getter(with_nulls, [], method))
        add(f'wide-{method}', getter(wide(rng, 2000), [], method))
    for name, child in (('null', None), ('chain', chain(200)), ('atom', [['double', bits(1.5), 3]])):
        add(f'add-child-{name}', getter(with_nulls, [], 'AddChild', child=child))
        add(f'add-child-nested-{name}', getter(with_nulls, [3], 'AddChild', child=child))

    # operator<<: every overload, alone and chained, with special values.
    for kind, values in (('double', doubles[:40]), ('float', [float_bits(b) for b in SPECIAL_FLOAT_BITS]),
                         ('int64', SPECIAL_INTS), ('int32', [0, 1, -1, INT32[0], INT32[1]])):
        add(f'insert-{kind}-specials', {'op': 'insert', 'tree': [['list', None, 0]],
                                         'items': [{'kind': kind, 'value': v} for v in values[:16]]})
        if len(values) > 16:
            add(f'insert-{kind}-specials-2', {'op': 'insert', 'tree': [['list', None, 0]],
                                               'items': [{'kind': kind, 'value': v}
                                                         for v in values[16:32]]})
    for i in range(400):
        add(f'insert-random-{i}', {'op': 'insert', 'tree': tree(rng, max_nodes=10, root_list=True),
                                   'items': [item(rng) for _ in range(rng.randint(1, MAX_ARGS))]})

    # operator>>: each target kind against each child kind (accepting, refusing and the
    # string extraction of a symbol, which passes its test and then throws in GetString).
    child_kinds = {'integer': ['integer', 9, None], 'double': ['double', bits(-2.25), None],
                   'string': ['string', hexs(b'st'), None], 'symbol': ['symbol', hexs(b'sy'), 7],
                   'list': ['list', None, 0]}
    for child_name, child in child_kinds.items():
        for kind in ('int32', 'int64', 'float', 'double', 'string', 'in_string', 'isexprable'):
            for symbol in ((False, True) if kind == 'in_string' else (None,)):
                t = target(rng, kind)
                if symbol is not None:
                    t['symbol'] = symbol
                nodes = [['list', None, 2], child, ['integer', 1, None]]
                add(f'extract-{kind}-{symbol}-from-{child_name}',
                    {'op': 'extract', 'tree': nodes, 'targets': [t, target(rng, 'int64')]})
    for i in range(500):
        nodes, targets = extract_request(rng)
        error = UNDEFINED if extract_outcome(nodes, targets) == 'undefined' else None
        add(f'extract-random-{i}', {'op': 'extract', 'tree': nodes, 'targets': targets}, error)

    # doScan and doAddChildren, through Scan, AddChildren and the variadic constructor.
    count = attempt = 0
    while count < 300:
        nodes, args = scan_request(rng)
        outcome, _ = scan_outcome(nodes, args)
        attempt += 1
        if outcome == 'undefined':
            add(f'scan-undefined-{attempt}', {'op': 'scan', 'tree': nodes, 'args': args},
                UNDEFINED)
            continue
        add(f'scan-{count}', {'op': 'scan', 'tree': nodes, 'args': args})
        count += 1
    for i in range(160):
        add(f'add-children-{i}', {'op': 'add_children', 'tree': tree(rng, max_nodes=16, root_list=True),
                                  'args': rand_children_args(rng)})
    for i in range(80):
        add(f'construct-variadic-{i}', {'op': 'construct_list', 'ctor': 'variadic',
                                        'args': rand_children_args(rng)})

    # The list members on random list nodes (random paths mostly reach atoms).
    for i in range(300):
        nodes = tree(rng, max_nodes=30, max_depth=5, root_list=True)
        root = parse(nodes)
        path, node = rng.choice([(p, n) for p, n in located(root) if n[0] == 'list'])
        method = rng.choice(('GetChildren', 'GetChild', 'GetNumberOfChildren', 'GetList',
                             'AddChild', 'AsString'))
        extra = getter_extra(rng, node, method)
        add(f'getter-list-{i}', getter(nodes, path, method, **extra),
            getter_error(root, path, method, extra))

    # Extraction chains whose every target accepts its child, ending at the last child.
    for i in range(200):
        count = rng.randint(1, 12)
        kids = [atom(rng) for _ in range(count)]
        targets = []
        for kid in kids:
            if rng.random() < 0.1:
                targets.append({'kind': 'isexprable'})
            targets.append(target_for(rng, kid[0]))
        targets = targets[:MAX_ARGS]
        nodes = [['list', rand_line(rng), count]] + kids
        error = UNDEFINED if extract_outcome(nodes, targets) == 'undefined' else None
        add(f'extract-accepted-{i}', {'op': 'extract', 'tree': nodes, 'targets': targets}, error)
    return corpus


def source_controls():
    """The source unit's undefined evaluations and resource bounds, equal on both sides."""
    out = []

    def add(name, input_, error=UNDEFINED):
        out.append(control(SOURCE, f'control-source-{name}', input_, error))

    two = [['list', None, 2], ['integer', 1, None], ['symbol', hexs(b'b'), None]]
    add('get-child-empty', getter([['list', None, 0]], [], 'GetChild', index=0))
    add('get-child-past-end', getter(two, [], 'GetChild', index=2))
    add('get-child-max-index', getter(two, [], 'GetChild', index=2**64 - 1))
    add('as-string-null-root-child', getter([['list', None, 1], ['null']], [], 'AsString',
                                            level=None))
    add('as-string-null-after-siblings',
        getter([['list', None, 3], ['integer', 1, None], ['list', None, 1], ['null'],
                ['string', hexs(b'x'), None]], [], 'AsString', level=0))
    add('as-string-chain-max-depth', getter(chain(MAX_DEPTH), [], 'AsString', level=None),
        TOO_LARGE)
    add('as-string-chain-1300', getter(chain(1300), [], 'AsString', level=None), TOO_LARGE)
    add('as-string-indent-above-bound', getter([['list', None, 0]], [], 'AsString',
                                               level=2**19 + 1), TOO_LARGE)
    add('as-string-child-indent-above-bound', getter([['list', None, 1], ['list', None, 0]], [],
                                                     'AsString', level=2**19), TOO_LARGE)
    add('as-string-unallocatable-indent', getter([['list', None, 0]], [], 'AsString',
                                                 level=2**62 - 1), TOO_LARGE)
    add('as-string-unallocatable-wrapped-indent',
        getter([['list', None, 0]], [], 'AsString', level=2**63 + 2**62 - 1), TOO_LARGE)
    add('extract-empty', {'op': 'extract', 'tree': [['list', None, 0]],
                          'targets': [{'kind': 'int32', 'initial': 0}]})
    add('extract-past-end', {'op': 'extract', 'tree': two,
                             'targets': [{'kind': 'int64', 'initial': 0},
                                         {'kind': 'in_string', 'symbol': True, 'initial': ''},
                                         {'kind': 'isexprable'},
                                         {'kind': 'double', 'initial': bits(0.0)}]})
    add('extract-null-child', {'op': 'extract', 'tree': [['list', None, 1], ['null']],
                               'targets': [{'kind': 'string', 'initial': ''}]})
    add('scan-int-writes-double', {'op': 'scan', 'tree': [['list', None, 1], ['integer', -1, None]],
                                   'args': [{'kind': 'int', 'initial': 3}]})
    add('scan-past-end', {'op': 'scan', 'tree': [['list', None, 0]],
                          'args': [{'kind': 'string', 'initial': ''}]})
    add('getter-path-through-atom', getter(two, [0, 0], 'GetList'),
        {'kind': 'invalid_input', 'message': 'invalid path'})
    add('getter-path-to-null', getter([['list', None, 1], ['null']], [0], 'GetList'),
        {'kind': 'invalid_input', 'message': 'invalid path'})
    add('getter-unknown-method', getter(two, [], 'GetParent'),
        {'kind': 'invalid_input', 'message': 'unknown method'})
    add('insert-17-items', {'op': 'insert', 'tree': [['list', None, 0]],
                            'items': [{'kind': 'isexprable'}] * (MAX_ARGS + 1)},
        {'kind': 'invalid_input', 'message': 'argument count out of range'})
    return out


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
                                 {'kind': 'string', 'initial': ''}]}))
    out.append(control(HEADER, 'control-scan-empty-list',
                       {'op': 'scan', 'tree': [['list', None, 0]],
                        'args': [{'kind': 'double', 'initial': bits(1.0)}]}))
    out.append(control(HEADER, 'control-scan-null-child',
                       {'op': 'scan', 'tree': [['list', None, 2], ['integer', 1, None], ['null']],
                        'args': [{'kind': 'long_int', 'initial': 0},
                                 {'kind': 'string', 'initial': ''}]}))
    out.append(control(HEADER, 'control-scan-int-writes-double',
                       {'op': 'scan', 'tree': [['list', None, 1], ['integer', 5, None]],
                        'args': [{'kind': 'int', 'initial': 0}]}))
    return out


def lines(records):
    return ''.join(json.dumps(r, separators=(',', ':'), sort_keys=True) + '\n' for r in records)


def generate():
    rng = random.Random(SEED)
    controls = []
    header = header_cases(rng, controls)
    controls = fixed_controls() + controls
    # The source corpus continues the same generator after the header corpus, so the header
    # corpus is unchanged by it.
    generated = []
    source = source_cases(rng, generated)
    controls += source_controls() + generated
    return {
        CASES / f'{HEADER}.jsonl': lines(header.records),
        CASES / f'{SOURCE}.jsonl': lines(source.records),
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
