#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public fixed-seed corpus for libs/core/wx_stl_compat.cpp; inputs only, no expected answers.

Operations (schema in README.md):

    less_point    std::less<wxPoint>::operator()( a, b )
    point_map     std::map<wxPoint, int>::emplace of each entry, then the map in order
    to_wx_string  TowxString( view ) with the C library's LC_CTYPE set to "C" or "C.UTF-8"
    insert_size   operator<<( std::ostream&, const wxSize& ) on a configured stream
    insert_point  operator<<( std::ostream&, const wxPoint& ) on a configured stream

The corpus crosses 32-bit boundary values, every single byte and the boundary,
overlong, surrogate, truncated and NUL-split forms of the C library's decoders, and the
stream states, flags, widths, fills, groupings, short buffers, exception masks and
unitbuf/sync combinations that change the insertions' observable result. After those come
the defined paths the first corpus left out: the destructor flush that terminates the
reference process (and the failing first writes that unwind past it), states the mask
intersects (setup's exception caught, then the insertion), and scripted host operations: a
buffer overflow, a sync() or the num_put facet's do_put throwing std::runtime_error at a
given call, and a tied stream with its own state, mask, unitbuf and sync. After those, from
their own generators again: the operator called while another exception propagates, a
facet that changes the stream it formats (width, precision and flags, its own fill and long
value, zero or two stock calls), a host operation that cancels the insertion's thread
(forced unwind), and null buffers for the stream and its tie. Requests whose answer differs
between libstdc++ runtimes (the unitbuf destructor) are ordinary cases: the reference
runtime decides them.

Malformed and out-of-contract requests are written to a separate fixture with the exact
refusal both programs must give; they never enter the public corpus.
port/oracle/test_wx_stl_compat_inputs.py follows each with a successful request.
"""
import argparse
import json
from pathlib import Path
import random
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.core.application.thin.replacement.p001'
CASES = ROOT / 'port' / 'cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port' / 'oracle' / 'fixtures' / 'wx-stl-compat-controls.jsonl'
SEED = 0x57C0A7
INT_MIN, INT_MAX = -2 ** 31, 2 ** 31 - 1
I64_MIN, I64_MAX = -2 ** 63, 2 ** 63 - 1
EDGES = [INT_MIN, INT_MIN + 1, -1, 0, 1, INT_MAX - 1, INT_MAX]
EDGES5 = [INT_MIN, -1, 0, 1, INT_MAX]
CTYPES = ['C', 'C.UTF-8']
MAX_CAPACITY = 1 << 20


def stream(**changes):
    base = {'state': [], 'exceptions': [], 'flags': ['dec', 'skipws'], 'width': 0, 'precision': 6,
            'fill': 32, 'thousands_sep': 44, 'grouping': [], 'capacity': None, 'sync_fails': False}
    if 'host' in changes:
        base['host'] = None
    unknown = set(changes) - set(base)
    if unknown:
        raise ValueError(unknown)
    base.update(changes)
    return base


def utf8(code):
    """UTF-8-style encoding of any value below 2**31 in the shortest one- to six-byte form."""
    if code < 0x80:
        return bytes([code])
    for length, limit in ((2, 0x800), (3, 0x10000), (4, 0x200000), (5, 0x4000000), (6, 0x80000000)):
        if code < limit:
            out = []
            for _ in range(length - 1):
                out.append(0x80 | (code & 0x3F))
                code >>= 6
            lead = (0xFF << (8 - length)) & 0xFF
            return bytes([lead | code] + out[::-1])
    raise ValueError(code)


def overlong(code, length):
    out = []
    for _ in range(length - 1):
        out.append(0x80 | (code & 0x3F))
        code >>= 6
    lead = (0xFF << (8 - length)) & 0xFF
    return bytes([lead | code] + out[::-1])


class Corpus:
    def __init__(self):
        self.rows = []

    def add(self, case, value_input):
        self.rows.append({'unit': UNIT, 'case': case, 'input': value_input})


def less_cases(c, rng):
    points = [(x, y) for x in EDGES5 for y in EDGES5]
    for i, a in enumerate(points):
        for j, b in enumerate(points):
            c.add(f'less-edge-{i:02d}-{j:02d}', {'op': 'less_point', 'a': list(a), 'b': list(b)})
    for k in range(48):
        small = k % 2 == 0
        draw = (lambda: rng.randint(-3, 3)) if small else (lambda: rng.randint(INT_MIN, INT_MAX))
        a = [draw(), draw()]
        b = [a[0], draw()] if k % 3 == 0 else [draw(), draw()]
        c.add(f'less-random-{k:02d}', {'op': 'less_point', 'a': a, 'b': b})


def map_cases(c, rng):
    c.add('map-empty', {'op': 'point_map', 'entries': []})
    c.add('map-single', {'op': 'point_map', 'entries': [[[INT_MIN, INT_MAX], -1]]})
    c.add('map-first-value-wins', {'op': 'point_map', 'entries': [
        [[1, 2], 10], [[1, 2], 20], [[0, 5], 30], [[1, -3], 40], [[0, 5], 50]]})
    edge_points = [[x, y] for x in EDGES for y in EDGES]
    shuffled = edge_points[:]
    rng.shuffle(shuffled)
    c.add('map-edge-cross', {'op': 'point_map',
                             'entries': [[p, i - 24] for i, p in enumerate(shuffled)]})
    for k in range(4):
        span = 3 if k < 2 else 1000
        entries = [[[rng.randint(-span, span), rng.randint(-span, span)], rng.randint(INT_MIN, INT_MAX)]
                   for _ in range(256)]
        c.add(f'map-random-{k}', {'op': 'point_map', 'entries': entries})
    wide = [[[rng.randint(INT_MIN, INT_MAX), rng.randint(INT_MIN, INT_MAX)], k] for k in range(256)]
    c.add('map-random-wide', {'op': 'point_map', 'entries': wide})


def view_strings():
    named = [
        ('ascii', b'hello, world'), ('nul-middle', b'ab\0cd'), ('nul-trailing', b'ab\0'),
        ('nul-only', b'\0'), ('nul-two', b'\0\0'), ('nul-leading', b'\0a'),
        ('nul-double-middle', b'a\0\0b'), ('nul-many', b'\0x\0\0y\0'), ('del', b'\x7f'),
        ('controls', bytes(range(1, 32))),
        ('u0080', utf8(0x80)), ('u07ff', utf8(0x7FF)), ('u0800', utf8(0x800)),
        ('ud7ff', utf8(0xD7FF)), ('ue000', utf8(0xE000)), ('ufffd', utf8(0xFFFD)),
        ('ufffe', utf8(0xFFFE)), ('uffff', utf8(0xFFFF)), ('u10000', utf8(0x10000)),
        ('u10ffff', utf8(0x10FFFF)), ('above-unicode', utf8(0x110000)),
        ('four-byte-max', utf8(0x1FFFFF)), ('five-byte-min', utf8(0x200000)),
        ('five-byte-max', utf8(0x3FFFFFF)), ('six-byte-min', utf8(0x4000000)),
        ('six-byte-max', utf8(0x7FFFFFFF)), ('f5-lead', b'\xf5\x80\x80\x80'),
        ('f7-lead', b'\xf7\xbf\xbf\xbf'), ('bom', b'\xef\xbb\xbf'),
        ('euro-text', 'Größe 5 €'.encode()), ('emoji', '\U0001F600'.encode()),
        ('surrogate-low', b'\xed\xa0\x80'), ('surrogate-high', b'\xed\xbf\xbf'),
        ('surrogate-pair', b'\xed\xa0\xbd\xed\xb8\x80'),
        ('lead-c0', b'\xc0\x80'), ('lead-c1', b'\xc1\xbf'), ('lead-fe', b'\xfe'), ('lead-ff', b'\xff'),
        ('lead-fe-cont', b'\xfe\xbf'), ('lone-80', b'\x80'), ('lone-bf', b'\xbf'),
        ('truncated-2', b'\xc3'), ('truncated-3', b'\xe2\x82'), ('truncated-4', b'\xf0\x9f\x98'),
        ('truncated-5', b'\xf8\x88\x80\x80'), ('truncated-6', b'\xfc\x84\x80\x80\x80'),
        ('truncated-after-ascii', b'a\xc3'), ('cut-by-nul-2', b'\xc3\0\xa9'),
        ('cut-by-nul-3', b'\xe2\x82\0\xac'), ('bad-cont-2', b'\xc3\x28'), ('bad-cont-3', b'\xe2\x28\xa1'),
        ('bad-cont-3b', b'\xe2\x82\x28'), ('bad-cont-6', b'\xfd\xbf\xbf\x7f\xbf\xbf'),
        ('valid-nul-valid', b'\xc3\xa9\0\xc3\xa9'), ('valid-then-invalid-chunk', b'ok\0\xff'),
        ('invalid-then-valid-chunk', b'\xff\0ok'), ('multibyte-trailing-nul', b'\xe2\x82\xac\0'),
        ('extra-continuation', b'\xc3\xa9\xa9'),
    ]
    for length in range(3, 7):
        named.append((f'overlong-{length}-zero', overlong(0, length)))
        named.append((f'overlong-{length}-max', overlong((1 << (5 * length - 4)) - 1, length)))
    named.append(('overlong-2-max', overlong(0x7F, 2)))
    return named


def string_cases(c, rng):
    for ctype in CTYPES:
        tag = 'c' if ctype == 'C' else 'utf8'
        c.add(f'str-{tag}-null', {'op': 'to_wx_string', 'ctype': ctype, 'view': None})
        c.add(f'str-{tag}-empty', {'op': 'to_wx_string', 'ctype': ctype, 'view': ''})
        for b in range(256):
            c.add(f'str-{tag}-byte-{b:02x}', {'op': 'to_wx_string', 'ctype': ctype,
                                              'view': bytes([b]).hex()})
            c.add(f'str-{tag}-pair-a{b:02x}', {'op': 'to_wx_string', 'ctype': ctype,
                                               'view': bytes([0x61, b]).hex()})
        for name, data in view_strings():
            c.add(f'str-{tag}-{name}', {'op': 'to_wx_string', 'ctype': ctype, 'view': data.hex()})
        for k in range(32):
            data = bytes(rng.randrange(256) for _ in range(rng.randint(1, 48)))
            c.add(f'str-{tag}-random-bytes-{k:02d}', {'op': 'to_wx_string', 'ctype': ctype, 'view': data.hex()})
        for k in range(32):
            codes = []
            for _ in range(rng.randint(1, 24)):
                pick = rng.randrange(6)
                codes.append([0, rng.randint(1, 0x7F), rng.randint(0x80, 0xD7FF), rng.randint(0xE000, 0x10FFFF),
                              rng.randint(0x110000, 0x3FFFFFF), rng.randint(0x4000000, 0x7FFFFFFF)][pick])
            data = b''.join(utf8(code) for code in codes)
            c.add(f'str-{tag}-random-utf8-{k:02d}', {'op': 'to_wx_string', 'ctype': ctype, 'view': data.hex()})
        c.add(f'str-{tag}-long-ascii', {'op': 'to_wx_string', 'ctype': ctype,
                                        'view': (b'abcdefgh' * 512).hex()})
        c.add(f'str-{tag}-long-utf8', {'op': 'to_wx_string', 'ctype': ctype,
                                       'view': (b'\xc3\xa9' * 2048).hex()})
        c.add(f'str-{tag}-long-utf8-bad-end', {'op': 'to_wx_string', 'ctype': ctype,
                                               'view': (b'\xc3\xa9' * 2047 + b'\xc3\x00').hex()})
        c.add(f'str-{tag}-long-nuls', {'op': 'to_wx_string', 'ctype': ctype, 'view': (b'\0' * 4096).hex()})


def insert(c, case, op, value, **changes):
    c.add(case, {'op': op, 'value': list(value), 'stream': stream(**changes)})


def insert_cases(c, rng):
    ops = [('size', 'insert_size'), ('point', 'insert_point')]
    for tag, op in ops:
        for i, x in enumerate(EDGES):
            for j, y in enumerate(EDGES):
                insert(c, f'{tag}-edge-{i}-{j}', op, (x, y))
        for k in range(8):
            insert(c, f'{tag}-random-{k}', op, (rng.randint(INT_MIN, INT_MAX), rng.randint(-999, 999)))

    values = [(INT_MIN, INT_MAX), (0, -1), (255, -4096)]
    flag_sets = {
        'none': [], 'hex': ['hex'], 'oct': ['oct'], 'hex-showbase': ['hex', 'showbase'],
        'hex-showbase-upper': ['hex', 'showbase', 'uppercase'], 'hex-upper': ['hex', 'uppercase'],
        'oct-showbase': ['oct', 'showbase'], 'showpos': ['dec', 'showpos'],
        'hex-oct': ['hex', 'oct'], 'dec-hex': ['dec', 'hex'], 'dec-oct': ['dec', 'oct'],
        'all-bases': ['dec', 'hex', 'oct', 'showbase'], 'hex-showpos': ['hex', 'showpos'],
        'dec-showbase': ['dec', 'showbase', 'uppercase'], 'float-flags': ['boolalpha', 'fixed', 'scientific',
                                                                          'showpoint', 'dec'],
        'unitbuf': ['dec', 'unitbuf'], 'skipws-only': ['skipws'],
        'every-flag': ['boolalpha', 'dec', 'fixed', 'hex', 'internal', 'left', 'oct', 'right', 'scientific',
                       'showbase', 'showpoint', 'showpos', 'skipws', 'unitbuf', 'uppercase'],
    }
    for tag, op in ops:
        for name, flags in flag_sets.items():
            for v, value in enumerate(values):
                insert(c, f'{tag}-flags-{name}-{v}', op, value, flags=flags)

    adjust = {'none': [], 'left': ['left'], 'right': ['right'], 'internal': ['internal'],
              'left-right': ['left', 'right'], 'left-internal': ['left', 'internal']}
    for tag, op in ops:
        for name, extra in adjust.items():
            for width in [I64_MIN, -1, 0, 3, 4, 5, 7, 8, 9, 10, 40]:
                for fill in [0x20, 0x00, 0x2A, 0x80, 0xFF]:
                    if fill != 0x2A and width not in (9, 40):
                        continue
                    insert(c, f'{tag}-pad-{name}-w{width}-f{fill:02x}', op, (-12, 34),
                           flags=['dec', 'skipws'] + extra, width=width, fill=fill)
            insert(c, f'{tag}-pad-{name}-huge-bounded', op, (5, 6), flags=['dec'] + extra,
                   width=I64_MAX, fill=0x2D, capacity=64)
            insert(c, f'{tag}-pad-{name}-wide-unbounded', op, (5, 6), flags=['dec'] + extra,
                   width=4096, fill=0x2E)
        # The widest padding an unbounded buffer accepts (the port and adapter bound).
        if tag == 'size':
            insert(c, 'size-pad-limit-unbounded', op, (5, 6), width=MAX_CAPACITY, fill=0x2E)
        for p in [I64_MIN, -1, 0, 17, I64_MAX]:
            insert(c, f'{tag}-precision-{p}', op, (1, 2), precision=p)

    groupings = {
        'three': ([3], 0x2C), 'one-ff': ([1], 0xFF), 'one-two': ([1, 2], 0x2E), 'two-one-three-nul': ([2, 1, 3], 0x00),
        'zero': ([0], 0x2C), 'char-max': ([127], 0x2C), 'negative': ([255], 0x2C), 'minus-128': ([128], 0x2C),
        'three-zero': ([3, 0], 0x2C), 'three-max': ([3, 127], 0x20), 'three-negative': ([3, 255], 0x2C),
        'one-max': ([1, 127], 0x5F), 'nine': ([9], 0x27), 'ten': ([10], 0x27), 'fours': ([4] * 16, 0x2C),
        'ones': ([1] * 16, 0x80), 'mixed': ([1, 2, 3, 4], 0x7C),
        # The num_put<char> twin facet ends the grouping at its first NUL byte.
        'two-nul-one': ([2, 0, 1], 0x2C), 'one-nul-five': ([1, 0, 5], 0x2C), 'nul-three': ([0, 3], 0x2C),
    }
    group_values = [(INT_MIN, INT_MAX), (0, -1), (1000, -1000), (999, 12345678)]
    group_flags = {'dec': ['dec'], 'showpos': ['dec', 'showpos'], 'hex-showbase': ['hex', 'showbase'],
                   'oct-showbase': ['oct', 'showbase'], 'hex-upper': ['hex', 'uppercase']}
    for tag, op in ops:
        for name, (grouping, sep) in groupings.items():
            for fname, flags in group_flags.items():
                for v, value in enumerate(group_values):
                    if fname != 'dec' and v not in (0, 2):
                        continue
                    insert(c, f'{tag}-group-{name}-{fname}-{v}', op, value, flags=flags,
                           grouping=grouping, thousands_sep=sep)
        insert(c, f'{tag}-group-three-padded', op, (-1234567, 7), grouping=[3], width=30, fill=0x30,
               flags=['dec', 'internal'])

    states = {'eof': ['eof'], 'fail': ['fail'], 'bad': ['bad'], 'eof-fail': ['eof', 'fail'],
              'bad-eof': ['bad', 'eof'], 'all': ['bad', 'eof', 'fail']}
    masks = {'none': [], 'bad': ['bad'], 'eof': ['eof'], 'fail': ['fail'], 'bad-fail': ['bad', 'fail'],
             'all': ['bad', 'eof', 'fail']}
    for tag, op in ops:
        for sname, state in states.items():
            for mname, mask in masks.items():
                if set(state) & set(mask):
                    continue
                insert(c, f'{tag}-state-{sname}-mask-{mname}', op, (3, 4), state=state, exceptions=mask,
                       width=12)
        for mname, mask in masks.items():
            insert(c, f'{tag}-good-mask-{mname}', op, (3, 4), exceptions=mask, width=12)

    for tag, op in ops:
        full = len((' width="12" height="-345"' if tag == 'size' else ' x="12" y="-345"').encode())
        for capacity in range(0, full + 2):
            for mname, mask in [('none', []), ('bad', ['bad']), ('fail', ['fail']), ('eof', ['eof'])]:
                if mname in ('fail', 'eof') and capacity % 3 != 0:
                    continue
                insert(c, f'{tag}-capacity-{capacity:02d}-mask-{mname}', op, (12, -345), capacity=capacity,
                       exceptions=mask)
        for capacity in range(0, 16):
            for name, extra in [('right', []), ('left', ['left'])]:
                insert(c, f'{tag}-capacity-fill-{name}-{capacity:02d}', op, (1, 2), capacity=capacity, width=12,
                       fill=0x23, flags=['dec'] + extra)
                insert(c, f'{tag}-capacity-fill-{name}-{capacity:02d}-mask-bad', op, (1, 2), capacity=capacity,
                       width=12, fill=0x23, flags=['dec'] + extra, exceptions=['bad'])

    sync_states = {'good': [], 'eof': ['eof'], 'fail': ['fail'], 'bad': ['bad'], 'eof-fail': ['eof', 'fail']}
    sync_masks = {'none': [], 'fail': ['fail'], 'eof': ['eof']}
    for tag, op in ops:
        for sname, state in sync_states.items():
            for mname, mask in sync_masks.items():
                if set(state) & set(mask):
                    continue
                for sync_fails in (False, True):
                    for capacity in (None, 5):
                        name = f'{tag}-unitbuf-{sname}-mask-{mname}-sync-{"fails" if sync_fails else "ok"}'
                        name += '-unbounded' if capacity is None else f'-cap{capacity}'
                        insert(c, name, op, (7, -8), state=state, exceptions=mask, sync_fails=sync_fails,
                               capacity=capacity, flags=['dec', 'unitbuf'])
        insert(c, f'{tag}-no-unitbuf-sync-fails', op, (7, -8), sync_fails=True)
        insert(c, f'{tag}-no-unitbuf-sync-fails-mask-bad', op, (7, -8), sync_fails=True, exceptions=['bad'])


LITERAL = {'size': len(b' width="'), 'point': len(b' x="')}
OPS = [('size', 'insert_size'), ('point', 'insert_point')]


def host(overflow_throw=None, sync_throw=None, num_put=None, tie=None):
    return {'overflow_throw': overflow_throw, 'sync_throw': sync_throw, 'num_put': num_put, 'tie': tie}


def tie_stream(state=(), exceptions=(), flags=(), sync_fails=False, sync_throw=None):
    return {'state': list(state), 'exceptions': list(exceptions), 'flags': list(flags),
            'sync_fails': sync_fails, 'sync_throw': sync_throw}


def terminate_boundary_cases(c):
    """Good, unitbuf, a failing sync and badbit in the mask. The reference terminates in the
    first destructor that runs without unwinding; a first write that does not fit throws
    ios_base::failure first, and the unwinding destructor skips the flush. The boundary is
    max(width, literal length) bytes of capacity."""
    for tag, op in OPS:
        length = LITERAL[tag]
        for name, extra in [('right', []), ('left', ['left']), ('internal', ['internal'])]:
            for width in sorted({0, length - 1, length, length + 1, length + 3}):
                for capacity in range(0, max(width, length) + 2):
                    insert(c, f'{tag}-terminate-{name}-w{width}-cap{capacity:02d}', op, (12, -345),
                           flags=['dec', 'unitbuf'] + extra, width=width, fill=0x2A, capacity=capacity,
                           sync_fails=True, exceptions=['bad'])
        for capacity in (None, 3):
            insert(c, f'{tag}-terminate-unbounded-mask-bad-fail-{capacity}', op, (1, 2),
                   flags=['dec', 'unitbuf'], capacity=capacity, sync_fails=True, exceptions=['bad', 'fail'])
        for sname, state in [('eof', ['eof']), ('fail', ['fail']), ('eof-fail', ['eof', 'fail'])]:
            for capacity in (None, 5):
                insert(c, f'{tag}-terminate-{sname}-mask-bad-{capacity}', op, (7, -8), state=state,
                       flags=['dec', 'unitbuf'], capacity=capacity, sync_fails=True, exceptions=['bad'])


def setup_failure_cases(c):
    """A state that intersects the mask: exceptions( mask ) throws during setup, is caught,
    and the insertion then runs on the stream with that mask."""
    states = {'eof': ['eof'], 'fail': ['fail'], 'bad': ['bad'], 'eof-fail': ['eof', 'fail'],
              'bad-eof': ['bad', 'eof'], 'all': ['bad', 'eof', 'fail']}
    masks = {'eof': ['eof'], 'fail': ['fail'], 'bad': ['bad'], 'bad-fail': ['bad', 'fail'],
             'all': ['bad', 'eof', 'fail']}
    flush = {'plain': (['dec'], False), 'unitbuf': (['dec', 'unitbuf'], False),
             'unitbuf-sync-fails': (['dec', 'unitbuf'], True)}
    for tag, op in OPS:
        for sname, state in states.items():
            for mname, mask in masks.items():
                if not set(state) & set(mask):
                    continue
                for fname, (flags, sync_fails) in flush.items():
                    insert(c, f'{tag}-setup-{sname}-mask-{mname}-{fname}', op, (3, 4), state=state,
                           exceptions=mask, flags=flags, sync_fails=sync_fails, width=12)


def overflow_throw_cases(c):
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail'], 'eof': ['eof']}
    for tag, op in OPS:
        full = len((' width="12" height="-345"' if tag == 'size' else ' x="12" y="-345"').encode())
        for k in range(0, full + 1):
            for mname, mask in masks.items():
                insert(c, f'{tag}-overflow-throw-{k:02d}-mask-{mname}', op, (12, -345), exceptions=mask,
                       host=host(overflow_throw=k))
        for name, extra in [('right', []), ('left', ['left'])]:
            for k in (0, 1, 5, 7, 8, 11, 12, 13, 15):
                for mname in ('none', 'bad'):
                    insert(c, f'{tag}-overflow-throw-pad-{name}-{k:02d}-mask-{mname}', op, (1, 2),
                           flags=['dec'] + extra, width=12, fill=0x23, exceptions=masks[mname],
                           host=host(overflow_throw=k))
        for k in (0, 4, 9):
            for capacity in (0, 4, 9):
                insert(c, f'{tag}-overflow-throw-{k}-cap{capacity}-mask-bad', op, (123, 4), capacity=capacity,
                       exceptions=['bad'], host=host(overflow_throw=k))
        for k in (0, 3, 6, 10):
            for mname in ('none', 'bad'):
                insert(c, f'{tag}-overflow-throw-{k:02d}-unitbuf-mask-{mname}', op, (12, -345),
                       flags=['dec', 'unitbuf'], exceptions=masks[mname], host=host(overflow_throw=k))
        insert(c, f'{tag}-overflow-throw-never', op, (12, -345), host=host(overflow_throw=MAX_CAPACITY))


def num_put_cases(c):
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail']}
    for tag, op in OPS:
        for call in (0, 1, 2):
            for when in ('before', 'after'):
                for mname, mask in masks.items():
                    insert(c, f'{tag}-num-put-{call}-{when}-mask-{mname}', op, (-1234, 56), exceptions=mask,
                           host=host(num_put={'call': call, 'throw': when}))
        for when in ('before', 'after'):
            # The stock body grouped and short, then the facet's throw.
            insert(c, f'{tag}-num-put-0-{when}-grouped-hex', op, (65535, 1), flags=['hex', 'showbase'],
                   grouping=[2], thousands_sep=0x5F, exceptions=['bad'],
                   host=host(num_put={'call': 0, 'throw': when}))
            for capacity in (LITERAL[tag], LITERAL[tag] + 2):
                for mname in ('none', 'bad'):
                    insert(c, f'{tag}-num-put-0-{when}-cap{capacity}-mask-{mname}', op, (-1234, 56),
                           capacity=capacity, exceptions=masks[mname],
                           host=host(num_put={'call': 0, 'throw': when}))
            # The buffer throws inside the stock body, before the facet's own throw.
            insert(c, f'{tag}-num-put-0-{when}-overflow-inside', op, (-1234, 56), exceptions=['bad'],
                   host=host(overflow_throw=LITERAL[tag] + 2, num_put={'call': 0, 'throw': when}))
            insert(c, f'{tag}-num-put-1-{when}-unitbuf', op, (-1234, 56), flags=['dec', 'unitbuf'],
                   host=host(num_put={'call': 1, 'throw': when}))
        insert(c, f'{tag}-num-put-never', op, (-1234, 56), host=host(num_put={'call': 64, 'throw': 'before'}))


def sync_throw_cases(c):
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail']}
    for tag, op in OPS:
        for k in range(0, 6):
            for mname, mask in masks.items():
                for sync_fails in (False, True):
                    for sname, state in [('good', []), ('eof', ['eof'])]:
                        name = f'{tag}-sync-throw-{k}-mask-{mname}-{"fails" if sync_fails else "ok"}-{sname}'
                        insert(c, name, op, (7, -8), state=state, exceptions=mask, flags=['dec', 'unitbuf'],
                               sync_fails=sync_fails, host=host(sync_throw=k))
        insert(c, f'{tag}-sync-throw-no-unitbuf', op, (7, -8), host=host(sync_throw=0))


def tie_cases(c):
    states = {'good': [], 'eof': ['eof'], 'bad': ['bad'], 'fail': ['fail']}
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail']}
    syncs = {'ok': (False, None), 'fails': (True, None), 'throw0': (False, 0), 'throw1': (False, 1)}
    for tag, op in OPS:
        for sname, state in states.items():
            for mname, mask in masks.items():
                for yname, (sync_fails, sync_throw) in syncs.items():
                    for unitbuf in (False, True):
                        for main_mask in ([], ['bad']):
                            name = (f'{tag}-tie-{sname}-mask-{mname}-sync-{yname}'
                                    f'{"-unitbuf" if unitbuf else ""}{"-main-bad" if main_mask else ""}')
                            tie = tie_stream(state, mask, ['unitbuf'] if unitbuf else [], sync_fails, sync_throw)
                            insert(c, name, op, (5, -6), exceptions=main_mask, host=host(tie=tie))
        # A main stream that is not good never flushes its tie.
        insert(c, f'{tag}-tie-main-eof', op, (5, -6), state=['eof'],
               host=host(tie=tie_stream(sync_fails=True, exceptions=['bad'])))
        # Ties with a main stream that also flushes itself.
        for yname, (sync_fails, sync_throw) in syncs.items():
            insert(c, f'{tag}-tie-sync-{yname}-main-unitbuf', op, (5, -6), flags=['dec', 'unitbuf'],
                   host=host(tie=tie_stream(flags=['unitbuf'], sync_fails=sync_fails, sync_throw=sync_throw)))
        # A short main buffer with a tie: the tie is flushed by every good sentry only.
        insert(c, f'{tag}-tie-short-buffer', op, (5, -6), capacity=LITERAL[tag] + 1,
               host=host(tie=tie_stream()))


def random_host_cases(c, rng):
    names = ['bad', 'eof', 'fail']
    for tag, op in OPS:
        for k in range(48):
            pick = lambda: [n for n in names if rng.random() < 0.3]
            flags = ['dec'] + (['unitbuf'] if rng.random() < 0.5 else []) + (['left'] if rng.random() < 0.3 else [])
            put = None
            if rng.random() < 0.4:
                put = {'call': rng.randint(0, 2), 'throw': rng.choice(['before', 'after'])}
            tie = None
            if rng.random() < 0.5:
                tie = tie_stream(pick(), pick(), ['unitbuf'] if rng.random() < 0.5 else [], rng.random() < 0.3,
                                 rng.choice([None, None, 0, 1, 2]))
            insert(c, f'{tag}-host-random-{k:02d}', op, (rng.randint(-99999, 99999), rng.randint(-99, 99)),
                   state=pick() if rng.random() < 0.3 else [], exceptions=pick(), flags=flags,
                   width=rng.choice([0, 3, 9, 14]), fill=0x2E, capacity=rng.choice([None, None, 4, 9, 14, 20]),
                   sync_fails=rng.random() < 0.4, grouping=rng.choice([[], [3], [1, 2]]),
                   host=host(rng.choice([None, None, rng.randint(0, 24)]), rng.choice([None, None, rng.randint(0, 4)]),
                             put, tie))


def put_script(call=0, throw='none', set_before=None, set_after=None, fill=None, value=None, delegate=None):
    script = {'call': call, 'throw': throw}
    for key, item in (('set_before', set_before), ('set_after', set_after), ('fill', fill), ('value', value),
                      ('delegate', delegate)):
        if item is not None:
            script[key] = item
    return script


def change(width=None, precision=None, flags=None):
    return {'width': width, 'precision': precision, 'flags': flags}


def scripted(overflow_throw=None, sync_throw=None, num_put=None, tie=None, null_rdbuf=None, cancel=None):
    value = host(overflow_throw, sync_throw, num_put, tie)
    if null_rdbuf is not None:
        value['null_rdbuf'] = null_rdbuf
    if cancel is not None:
        value['cancel'] = cancel
    return value


def insert_unwinding(c, case, op, value, unwinding=True, **changes):
    c.add(case, {'op': op, 'value': list(value), 'stream': stream(**changes), 'unwinding': unwinding})


def unwinding_cases(c):
    """The operator called from a destructor while std::runtime_error( "outer" ) propagates:
    every ~sentry, the tied stream's included, does nothing, so nothing a destructor would
    flush (or terminate in) happens; exceptions from the operator still reach the caller."""
    states = {'good': [], 'eof': ['eof'], 'bad': ['bad']}
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail']}
    flush = {'plain': (['dec'], False), 'unitbuf': (['dec', 'unitbuf'], False),
             'unitbuf-sync-fails': (['dec', 'unitbuf'], True)}
    for tag, op in OPS:
        for sname, state in states.items():
            for mname, mask in masks.items():
                for fname, (flags, sync_fails) in flush.items():
                    insert_unwinding(c, f'{tag}-unwinding-{sname}-mask-{mname}-{fname}', op, (12, -345),
                                     state=state, exceptions=mask, flags=flags, sync_fails=sync_fails, width=12)
        # The reference terminates after the first literal here without the outer exception.
        for width in (0, 3, 9):
            insert_unwinding(c, f'{tag}-unwinding-would-terminate-w{width}', op, (1, 2),
                             flags=['dec', 'unitbuf'], sync_fails=True, exceptions=['bad'], width=width)
            insert_unwinding(c, f'{tag}-unwinding-false-would-terminate-w{width}', op, (1, 2), unwinding=False,
                             flags=['dec', 'unitbuf'], sync_fails=True, exceptions=['bad'], width=width)
        for capacity in (0, LITERAL[tag], LITERAL[tag] + 1, 30):
            for mname in ('none', 'bad'):
                insert_unwinding(c, f'{tag}-unwinding-cap{capacity:02d}-mask-{mname}', op, (12, -345),
                                 capacity=capacity, flags=['dec', 'unitbuf'], sync_fails=True,
                                 exceptions=masks[mname])
        for k in range(0, 4):
            for mname in ('none', 'bad'):
                insert_unwinding(c, f'{tag}-unwinding-sync-throw-{k}-mask-{mname}', op, (7, -8),
                                 flags=['dec', 'unitbuf'], exceptions=masks[mname], host=host(sync_throw=k))
        for k in (0, 5, 9):
            for mname in ('none', 'bad'):
                insert_unwinding(c, f'{tag}-unwinding-overflow-throw-{k}-mask-{mname}', op, (12, -345),
                                 flags=['dec', 'unitbuf'], exceptions=masks[mname], host=host(overflow_throw=k))
        for when in ('before', 'after'):
            insert_unwinding(c, f'{tag}-unwinding-num-put-{when}', op, (-1234, 56), exceptions=['bad'],
                             flags=['dec', 'unitbuf'], host=host(num_put={'call': 1, 'throw': when}))
        ties = {'ok': (False, None), 'fails': (True, None), 'throw0': (False, 0), 'throw2': (False, 2)}
        for yname, (sync_fails, sync_throw) in ties.items():
            for mname in ('none', 'bad'):
                insert_unwinding(c, f'{tag}-unwinding-tie-unitbuf-sync-{yname}-mask-{mname}', op, (5, -6),
                                 flags=['dec', 'unitbuf'], sync_fails=True,
                                 host=host(tie=tie_stream(flags=['unitbuf'], exceptions=masks[mname],
                                                          sync_fails=sync_fails, sync_throw=sync_throw)))


ADJUST = {'none': [], 'left': ['left'], 'right': ['right'], 'internal': ['internal'],
          'left-internal': ['left', 'internal']}


def facet_cases(c):
    """A num_put facet that changes the stream it formats (its ios_base&) before or after
    delegating to the stock body, which then pads to the width it finds and resets it, with
    the base, sign and prefix flags it finds; the facet may also pass its own fill and long
    value, or delegate zero or two times."""
    probe = put_script(set_before=change(6, 42, ['dec', 'skipws', 'showpos']))
    for tag, op in OPS:
        insert(c, f'{tag}-facet-probe', op, (7, 8), fill=0x2A, host=host(num_put=probe))
        for aname, extra in ADJUST.items():
            for width in (-1, 0, 1, 5, 6, 12, 40):
                for vname, value, base in [('neg', (-12, 34), ['dec']), ('pos', (5, 0), ['dec', 'showpos']),
                                           ('hex', (255, 0), ['hex', 'showbase']),
                                           ('hex-upper', (48879, 1), ['hex', 'showbase', 'uppercase']),
                                           ('oct', (8, 0), ['oct', 'showbase'])]:
                    if width in (1, 5) and vname not in ('neg', 'hex'):
                        continue
                    script = put_script(set_before=change(width, None, base + extra))
                    insert(c, f'{tag}-facet-pad-{aname}-w{width}-{vname}', op, value, fill=0x2A,
                           host=host(num_put=script))
        insert(c, f'{tag}-facet-pad-wide', op, (5, 6), fill=0x2E,
               host=host(num_put=put_script(set_before=change(4096, None, None))))
        for fname, fill in [('nul', 0x00), ('ff', 0xFF), ('same', None)]:
            insert(c, f'{tag}-facet-fill-{fname}', op, (-3, 4), fill=0x2A,
                   host=host(num_put=put_script(set_before=change(7, None, ['dec', 'internal']), fill=fill)))
        # Flags changed from decimal: the value converted for decimal is formatted in another
        # base, its long two's complement when negative.
        for fname, flags in [('hex', ['hex']), ('oct', ['oct']), ('hex-showbase-upper', ['hex', 'showbase', 'uppercase']),
                             ('oct-showbase', ['oct', 'showbase']), ('none', []), ('every', [
                                 'boolalpha', 'dec', 'fixed', 'hex', 'internal', 'left', 'oct', 'right', 'scientific',
                                 'showbase', 'showpoint', 'showpos', 'skipws', 'unitbuf', 'uppercase'])]:
            insert(c, f'{tag}-facet-flags-{fname}', op, (-1, INT_MAX),
                   host=host(num_put=put_script(set_before=change(None, None, flags))))
        # Flags changed to decimal from hex: the value was converted through unsigned int.
        insert(c, f'{tag}-facet-flags-hex-to-dec', op, (-1, -2), flags=['hex'],
               host=host(num_put=put_script(set_before=change(None, None, ['dec', 'showpos']))))
        # The facet's own long values: the full 64-bit range in every base.
        for vname, value in [('min', I64_MIN), ('max', I64_MAX), ('minus-one', -1), ('zero', 0),
                             ('big', 0x123456789ABCDEF)]:
            for fname, flags in [('dec', ['dec', 'showpos']), ('hex', ['hex', 'showbase']),
                                 ('oct', ['oct', 'showbase'])]:
                for gname, grouping in [('none', []), ('one', [1]), ('three', [3])]:
                    if gname != 'none' and vname in ('zero', 'big'):
                        continue
                    insert(c, f'{tag}-facet-value-{vname}-{fname}-group-{gname}', op, (1, 2), grouping=grouping,
                           thousands_sep=0x27,
                           host=host(num_put=put_script(set_before=change(None, None, flags), value=value)))
            insert(c, f'{tag}-facet-value-{vname}-padded', op, (1, 2), fill=0x23,
                   host=host(num_put=put_script(set_before=change(30, None, ['dec', 'internal']), value=value)))
        # Precision is carried, not read.
        for p in (I64_MIN, -1, 0, I64_MAX):
            insert(c, f'{tag}-facet-precision-{p}', op, (1, 2),
                   host=host(num_put=put_script(set_after=change(None, p, None))))
        # A width set after delegating survives into the next literal's padding.
        for aname in ('none', 'left'):
            for width in (0, 3, 10):
                insert(c, f'{tag}-facet-after-width-{aname}-{width}', op, (1, 2), fill=0x2D,
                       flags=['dec'] + ADJUST[aname], host=host(num_put=put_script(set_after=change(width, None, None))))
        insert(c, f'{tag}-facet-after-flags-unitbuf', op, (1, 2), sync_fails=True,
               host=host(num_put=put_script(set_after=change(None, None, ['dec', 'unitbuf']))))
        # Zero or two stock calls; a short buffer fails the iterator, so the second writes nothing.
        for delegate in (0, 2):
            for capacity in (None, LITERAL[tag] + 1, LITERAL[tag] + 3):
                cap = 'unbounded' if capacity is None else f'cap{capacity:02d}'
                insert(c, f'{tag}-facet-delegate-{delegate}-{cap}', op, (123, 4), capacity=capacity,
                       host=host(num_put=put_script(delegate=delegate, set_before=change(5, None, None))))
        # The changes made before a throw survive it: the exception skips the stock reset.
        for when in ('before', 'after'):
            for mname, mask in [('none', []), ('bad', ['bad'])]:
                insert(c, f'{tag}-facet-throw-{when}-mask-{mname}', op, (-1234, 56), exceptions=mask,
                       host=host(num_put=put_script(0, when, set_before=change(9, 3, ['hex', 'showbase']),
                                                    set_after=change(11, 4, None))))
        # A padded field that the buffer cuts short or throws inside.
        for capacity in (LITERAL[tag], LITERAL[tag] + 3, LITERAL[tag] + 8):
            for mname, mask in [('none', []), ('bad', ['bad'])]:
                insert(c, f'{tag}-facet-padded-cap{capacity:02d}-mask-{mname}', op, (-5, 6), capacity=capacity,
                       exceptions=mask, fill=0x2A,
                       host=host(num_put=put_script(set_before=change(8, None, ['dec', 'internal']))))
        for k in (LITERAL[tag], LITERAL[tag] + 4):
            insert(c, f'{tag}-facet-padded-overflow-throw-{k}', op, (-5, 6), exceptions=['bad'], fill=0x2A,
                   host=host(overflow_throw=k, num_put=put_script(set_before=change(8, None, None))))


def facet_random_cases(c, rng):
    names = ['boolalpha', 'dec', 'fixed', 'hex', 'internal', 'left', 'oct', 'right', 'scientific', 'showbase',
             'showpoint', 'showpos', 'skipws', 'unitbuf', 'uppercase']
    for tag, op in OPS:
        for k in range(32):
            def ios():
                if rng.random() < 0.3:
                    return None
                return change(rng.choice([None, -5, 0, 4, 9, 17, 25]), rng.choice([None, 0, 13]),
                              rng.choice([None, [n for n in names if rng.random() < 0.25]]))
            script = put_script(rng.randint(0, 2), rng.choice(['before', 'after', 'none', 'none']),
                                set_before=ios(), set_after=ios(),
                                fill=rng.choice([None, None, 0x2B, 0x00]),
                                value=rng.choice([None, None, rng.randint(I64_MIN, I64_MAX), rng.randint(-999, 999)]),
                                delegate=rng.choice([None, None, 0, 2]))
            insert(c, f'{tag}-facet-random-{k:02d}', op, (rng.randint(INT_MIN, INT_MAX), rng.randint(-99, 99)),
                   exceptions=rng.choice([[], [], ['bad']]), flags=rng.choice([['dec'], ['hex'], ['oct', 'showbase']]),
                   fill=0x2E, capacity=rng.choice([None, None, 12, 30]), grouping=rng.choice([[], [3], [1, 2]]),
                   host=host(num_put=script))


def forced_unwind_cases(c):
    """A host operation cancels the insertion's thread: libstdc++ catches the forced unwind
    first, sets badbit and always rethrows it, whatever the mask; the rethrown unwind is
    uncaught for every destructor it passes. A forced unwind out of a destructor's own sync
    terminates the reference (and aborts a GCC 15 runtime, whose destructor swallows it)."""
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail']}
    for tag, op in OPS:
        full = len((' width="12" height="-345"' if tag == 'size' else ' x="12" y="-345"').encode())
        for k in range(0, full + 1):
            for mname, mask in masks.items():
                if mname == 'fail' and k % 4 != 0:
                    continue
                insert(c, f'{tag}-cancel-overflow-{k:02d}-mask-{mname}', op, (12, -345), exceptions=mask, width=0,
                       host=scripted(cancel={'op': 'overflow', 'call': k}))
        for name, extra in [('right', []), ('left', ['left'])]:
            for k in (0, 7, 8, 11, 12):
                insert(c, f'{tag}-cancel-overflow-pad-{name}-{k:02d}', op, (1, 2), flags=['dec'] + extra, width=12,
                       fill=0x23, host=scripted(cancel={'op': 'overflow', 'call': k}))
        # unitbuf, a sync that would throw: the destructors the unwind passes do nothing.
        for k in (0, 3, LITERAL[tag] + 1):
            for mname in ('none', 'bad'):
                insert(c, f'{tag}-cancel-overflow-{k:02d}-unitbuf-sync-throw-mask-{mname}', op, (12, -345),
                       flags=['dec', 'unitbuf'], exceptions=masks[mname],
                       host=scripted(sync_throw=0, cancel={'op': 'overflow', 'call': k}))
            insert(c, f'{tag}-cancel-overflow-{k:02d}-unitbuf-sync-fails', op, (12, -345), flags=['dec', 'unitbuf'],
                   sync_fails=True, exceptions=['bad'], host=scripted(cancel={'op': 'overflow', 'call': k}))
        # The cancellation precedes a runtime_error scripted for the same call.
        insert(c, f'{tag}-cancel-overflow-before-throw', op, (12, -345), exceptions=['bad'],
               host=scripted(overflow_throw=2, cancel={'op': 'overflow', 'call': 2}))
        insert(c, f'{tag}-cancel-overflow-never', op, (12, -345),
               host=scripted(cancel={'op': 'overflow', 'call': MAX_CAPACITY}))
        insert(c, f'{tag}-cancel-overflow-cap', op, (12, -345), capacity=LITERAL[tag] + 1,
               host=scripted(cancel={'op': 'overflow', 'call': LITERAL[tag] + 1}))
        # A cancelled destructor sync.
        for k in range(0, 6):
            for mname, mask in masks.items():
                for state_name, state in [('good', []), ('eof', ['eof'])]:
                    insert(c, f'{tag}-cancel-sync-{k}-mask-{mname}-{state_name}', op, (7, -8), state=state,
                           exceptions=mask, flags=['dec', 'unitbuf'], host=scripted(cancel={'op': 'sync', 'call': k}))
        insert(c, f'{tag}-cancel-sync-no-unitbuf', op, (7, -8), host=scripted(cancel={'op': 'sync', 'call': 0}))
        # A cancelled facet call, before its stock body.
        for call in (0, 1):
            for mname, mask in masks.items():
                insert(c, f'{tag}-cancel-num-put-{call}-mask-{mname}', op, (-1234, 56), exceptions=mask,
                       flags=['dec', 'unitbuf'],
                       host=scripted(num_put=put_script(set_before=change(9, None, None)),
                                     cancel={'op': 'num_put', 'call': call}))
        insert(c, f'{tag}-cancel-num-put-without-facet', op, (-1234, 56),
               host=scripted(cancel={'op': 'num_put', 'call': 0}))
        # A cancelled tied flush: the tied stream sets badbit and rethrows; there is no main
        # sentry yet.
        for k in range(0, 4):
            for tname, tflags in [('plain', []), ('unitbuf', ['unitbuf'])]:
                for mname in ('none', 'bad'):
                    insert(c, f'{tag}-cancel-tie-sync-{k}-{tname}-mask-{mname}', op, (5, -6),
                           flags=['dec', 'unitbuf'],
                           host=scripted(tie=tie_stream(flags=tflags, exceptions=masks[mname]),
                                         cancel={'op': 'tie_sync', 'call': k}))


def null_rdbuf_cases(c):
    """A stream constructed on a null rdbuf() is bad from the start, and clear() keeps badbit,
    so every sentry fails and nothing is written; the destructor never syncs and flush() of a
    tied stream without a buffer does nothing."""
    states = {'good': [], 'eof': ['eof'], 'fail': ['fail']}
    masks = {'none': [], 'bad': ['bad'], 'fail': ['fail'], 'eof': ['eof']}
    for tag, op in OPS:
        for sname, state in states.items():
            for mname, mask in masks.items():
                for fname, flags in [('plain', ['dec']), ('unitbuf', ['dec', 'unitbuf'])]:
                    insert(c, f'{tag}-null-rdbuf-{sname}-mask-{mname}-{fname}', op, (3, 4), state=state,
                           exceptions=mask, flags=flags, width=9, sync_fails=True, host=scripted(null_rdbuf=True))
        insert(c, f'{tag}-null-rdbuf-false', op, (3, 4), host=scripted(null_rdbuf=False))
        insert(c, f'{tag}-null-rdbuf-with-tie', op, (3, 4), flags=['dec', 'unitbuf'],
               host=scripted(null_rdbuf=True, tie=tie_stream(sync_fails=True, exceptions=['bad'])))
        insert(c, f'{tag}-null-rdbuf-with-facet-and-cancel', op, (3, 4),
               host=scripted(null_rdbuf=True, num_put=put_script(throw='before'),
                             cancel={'op': 'overflow', 'call': 0}))
        # A tied stream without a buffer: never flushed, whatever its scripts.
        for tsname, tstate in [('good', []), ('eof', ['eof'])]:
            for tmname, tmask in [('none', []), ('bad', ['bad']), ('fail', ['fail'])]:
                for tfname, tflags in [('plain', []), ('unitbuf', ['unitbuf'])]:
                    tie = dict(tie_stream(tstate, tmask, tflags, True, 0), null_rdbuf=True)
                    insert(c, f'{tag}-tie-null-rdbuf-{tsname}-mask-{tmname}-{tfname}', op, (5, -6),
                           flags=['dec', 'unitbuf'], host=scripted(tie=tie))
        insert(c, f'{tag}-tie-null-rdbuf-false', op, (5, -6),
               host=scripted(tie=dict(tie_stream(sync_fails=True), null_rdbuf=False)))
        insert(c, f'{tag}-tie-null-rdbuf-cancel', op, (5, -6),
               host=scripted(tie=dict(tie_stream(), null_rdbuf=True), cancel={'op': 'tie_sync', 'call': 0}))
        insert(c, f'{tag}-null-rdbuf-both', op, (5, -6), flags=['dec', 'unitbuf'],
               host=scripted(null_rdbuf=True, tie=dict(tie_stream(), null_rdbuf=True)))
        insert_unwinding(c, f'{tag}-null-rdbuf-unwinding', op, (5, -6), flags=['dec', 'unitbuf'],
                         exceptions=['fail'], host=scripted(null_rdbuf=True))


def generate_cases():
    rng = random.Random(SEED)
    corpus = Corpus()
    less_cases(corpus, rng)
    map_cases(corpus, rng)
    string_cases(corpus, rng)
    insert_cases(corpus, rng)
    # Defined stream paths added after the first corpus, from their own generator so the
    # earlier requests are unchanged.
    terminate_boundary_cases(corpus)
    setup_failure_cases(corpus)
    overflow_throw_cases(corpus)
    num_put_cases(corpus)
    sync_throw_cases(corpus)
    tie_cases(corpus)
    random_host_cases(corpus, random.Random(SEED + 1))
    # The call context, mutable facet state, forced unwinding and null buffers, after every
    # earlier request.
    unwinding_cases(corpus)
    facet_cases(corpus)
    facet_random_cases(corpus, random.Random(SEED + 2))
    forced_unwind_cases(corpus)
    null_rdbuf_cases(corpus)
    return corpus.rows


def invalid(message):
    return {'kind': 'invalid_input', 'message': message}


def generate_controls():
    """Malformed and out-of-contract requests with the exact refusal of each, in the adapters'
    check order."""
    rows = []

    def add(name, value_input, message):
        rows.append({'request': {'unit': UNIT, 'case': f'control-{len(rows):03d}-{name}',
                                 'input': value_input},
                     'error': invalid(message)})

    add('missing-op', {'a': [1, 2], 'b': [3, 4]}, 'expected string: op')
    add('numeric-op', {'op': 3}, 'expected string: op')
    add('unknown-op', {'op': 'greater_point'}, 'unknown operation')
    add('case-op', {'op': 'TowxString'}, 'unknown operation')
    add('less-extra', {'op': 'less_point', 'a': [1, 2], 'b': [3, 4], 'c': 1}, 'unexpected or missing field')
    add('less-missing', {'op': 'less_point', 'a': [1, 2]}, 'unexpected or missing field')
    for name, value in [('number', 5), ('short', [1]), ('long', [1, 2, 3]), ('object', {}), ('null', None)]:
        add(f'less-shape-{name}', {'op': 'less_point', 'a': value, 'b': [0, 0]}, 'expected point')
    for name, value in [('float', [1.0, 2]), ('string', [1, '2']), ('bool', [True, 0]),
                        ('above', [INT_MAX + 1, 0]), ('below', [0, INT_MIN - 1]), ('u64', [2 ** 64 - 1, 0])]:
        add(f'less-component-{name}', {'op': 'less_point', 'a': [0, 0], 'b': value}, 'component out of range')
    add('map-missing', {'op': 'point_map'}, 'unexpected or missing field')
    add('map-not-array', {'op': 'point_map', 'entries': {}}, 'expected entries')
    add('map-too-many', {'op': 'point_map', 'entries': [[[0, k], k] for k in range(257)]}, 'expected entries')
    add('map-entry-short', {'op': 'point_map', 'entries': [[[0, 0]]]}, 'expected entry')
    add('map-entry-object', {'op': 'point_map', 'entries': [{'k': 1}]}, 'expected entry')
    add('map-key', {'op': 'point_map', 'entries': [[[0], 1]]}, 'expected point')
    add('map-value-float', {'op': 'point_map', 'entries': [[[0, 0], 1.5]]}, 'value out of range')
    add('map-value-range', {'op': 'point_map', 'entries': [[[0, 0], INT_MAX + 1]]}, 'value out of range')
    add('str-missing-view', {'op': 'to_wx_string', 'ctype': 'C'}, 'unexpected or missing field')
    for name, ctype in [('posix', 'POSIX'), ('utf8-lower', 'C.utf8'), ('en', 'en_US.UTF-8'), ('null', None),
                        ('number', 0)]:
        add(f'str-ctype-{name}', {'op': 'to_wx_string', 'ctype': ctype, 'view': '61'}, 'expected ctype')
    for name, view in [('odd', '616'), ('upper', '4A'), ('nonhex', 'zz'), ('array', [97]), ('number', 97),
                       ('long', '61' * 4097)]:
        add(f'str-view-{name}', {'op': 'to_wx_string', 'ctype': 'C.UTF-8', 'view': view}, 'expected view')

    good = stream()
    add('insert-missing-stream', {'op': 'insert_size', 'value': [1, 2]}, 'unexpected or missing field')
    add('insert-bad-value', {'op': 'insert_point', 'value': [1], 'stream': good}, 'expected point')
    add('insert-stream-not-object', {'op': 'insert_size', 'value': [1, 2], 'stream': []},
        'unexpected or missing stream field')
    missing = dict(good)
    del missing['fill']
    add('insert-stream-missing', {'op': 'insert_size', 'value': [1, 2], 'stream': missing},
        'unexpected or missing stream field')
    add('insert-stream-extra', {'op': 'insert_size', 'value': [1, 2], 'stream': dict(good, tie=None)},
        'unexpected or missing stream field')
    bad_fields = [
        ('state-name', {'state': ['good']}, 'expected iostate'),
        ('state-dup', {'state': ['eof', 'eof']}, 'expected iostate'),
        ('state-string', {'state': 'eof'}, 'expected iostate'),
        ('mask-name', {'exceptions': ['failbit']}, 'expected iostate'),
        ('flags-name', {'flags': ['hexadecimal']}, 'expected fmtflags'),
        ('flags-dup', {'flags': ['dec', 'dec']}, 'expected fmtflags'),
        ('width-float', {'width': 1.5}, 'expected streamsize'),
        ('width-big', {'width': 2 ** 63}, 'expected streamsize'),
        ('precision-string', {'precision': '6'}, 'expected streamsize'),
        ('fill-big', {'fill': 256}, 'expected byte'),
        ('fill-negative', {'fill': -1}, 'expected byte'),
        ('sep-null', {'thousands_sep': None}, 'expected byte'),
        ('grouping-long', {'grouping': [3] * 17}, 'expected grouping'),
        ('grouping-byte', {'grouping': [256]}, 'expected grouping'),
        ('grouping-string', {'grouping': '\u0003'}, 'expected grouping'),
        ('capacity-negative', {'capacity': -1}, 'expected capacity'),
        ('capacity-big', {'capacity': MAX_CAPACITY + 1}, 'expected capacity'),
        ('capacity-float', {'capacity': 4.0}, 'expected capacity'),
        ('sync-number', {'sync_fails': 0}, 'expected sync_fails'),
        ('unbounded-width', {'width': MAX_CAPACITY + 1}, 'unbounded width exceeds limit'),
        ('unbounded-width-max', {'width': I64_MAX}, 'unbounded width exceeds limit'),
    ]
    for name, changes, message in bad_fields:
        for op in ('insert_size', 'insert_point'):
            add(f'{op}-{name}', {'op': op, 'value': [1, 2], 'stream': dict(good, **changes)}, message)

    # The optional host scripts.
    ok_tie = tie_stream()
    bad_hosts = [
        ('host-number', 5, 'unexpected or missing host field'),
        ('host-null', None, 'unexpected or missing host field'),
        ('host-missing', {'overflow_throw': None, 'sync_throw': None, 'num_put': None},
         'unexpected or missing host field'),
        ('host-extra', dict(host(), when=1), 'unexpected or missing host field'),
        ('overflow-negative', host(overflow_throw=-1), 'expected overflow_throw'),
        ('overflow-float', host(overflow_throw=1.0), 'expected overflow_throw'),
        ('overflow-string', host(overflow_throw='1'), 'expected overflow_throw'),
        ('overflow-big', host(overflow_throw=MAX_CAPACITY + 1), 'expected overflow_throw'),
        ('sync-big', host(sync_throw=65), 'expected sync_throw'),
        ('sync-negative', host(sync_throw=-1), 'expected sync_throw'),
        ('put-number', host(num_put=1), 'expected num_put'),
        ('put-empty', host(num_put={}), 'expected num_put'),
        ('put-missing', host(num_put={'call': 0}), 'expected num_put'),
        ('put-when', host(num_put={'call': 0, 'throw': 'during'}), 'expected num_put'),
        ('put-call-big', host(num_put={'call': 65, 'throw': 'before'}), 'expected num_put'),
        ('put-call-null', host(num_put={'call': None, 'throw': 'after'}), 'expected num_put'),
        ('tie-number', host(tie=1), 'unexpected or missing tie field'),
        ('tie-empty', host(tie={}), 'unexpected or missing tie field'),
        ('tie-extra', host(tie=dict(ok_tie, width=0)), 'unexpected or missing tie field'),
        ('tie-state', host(tie=dict(ok_tie, state=['good'])), 'expected iostate'),
        ('tie-mask', host(tie=dict(ok_tie, exceptions='bad')), 'expected iostate'),
        ('tie-flags', host(tie=dict(ok_tie, flags=['flush'])), 'expected fmtflags'),
        ('tie-sync-fails', host(tie=dict(ok_tie, sync_fails=None)), 'expected sync_fails'),
        ('tie-sync-throw', host(tie=dict(ok_tie, sync_throw=65)), 'expected sync_throw'),
    ]
    for name, value, message in bad_hosts:
        for op in ('insert_size', 'insert_point'):
            add(f'{op}-{name}', {'op': op, 'value': [1, 2], 'stream': dict(good, host=value)}, message)
    add('insert-host-unbounded-width', {'op': 'insert_point', 'value': [1, 2],
                                        'stream': dict(good, width=MAX_CAPACITY + 1, host=host(overflow_throw=0))},
        'unbounded width exceeds limit')
    # The earliest failing check decides.
    add('insert-order-value-first', {'op': 'insert_size', 'value': 7, 'stream': dict(good, fill=999)},
        'expected point')
    add('insert-order-fields-first', {'op': 'insert_size', 'value': [1, 2],
                                      'stream': dict(good, fill=999, state=['x'])}, 'expected iostate')
    add('insert-order-parse-before-domain', {'op': 'insert_size', 'value': [1, 2],
                                             'stream': dict(good, state=['eof'], exceptions=['eof'],
                                                            sync_fails=1)}, 'expected sync_fails')
    add('insert-order-sync-before-host', {'op': 'insert_size', 'value': [1, 2],
                                          'stream': dict(good, sync_fails=1, host=5)}, 'expected sync_fails')
    add('insert-order-host-before-width', {'op': 'insert_size', 'value': [1, 2],
                                           'stream': dict(good, width=MAX_CAPACITY + 1, host=host(sync_throw=99))},
        'expected sync_throw')
    add('insert-order-host-fields', {'op': 'insert_point', 'value': [1, 2],
                                     'stream': dict(good, host=host(overflow_throw=-1, sync_throw=99, num_put=1,
                                                                    tie=1))}, 'expected overflow_throw')
    add('insert-order-tie-fields', {'op': 'insert_point', 'value': [1, 2],
                                    'stream': dict(good, host=host(tie=dict(ok_tie, state=['x'], flags=['y'])))},
        'expected iostate')
    # The call context, the facet's changes, forced unwinding and null buffers.
    for name, value, message in [('number', 1, 'expected unwinding'), ('null', None, 'expected unwinding'),
                                 ('string', 'true', 'expected unwinding')]:
        add(f'insert-unwinding-{name}', {'op': 'insert_point', 'value': [1, 2], 'stream': good, 'unwinding': value},
            message)
    add('insert-context-field', {'op': 'insert_point', 'value': [1, 2], 'stream': good, 'context': True},
        'unexpected or missing field')
    ok_put = {'call': 0, 'throw': 'none'}
    ok_change = {'width': None, 'precision': None, 'flags': None}
    new_hosts = [
        ('null-rdbuf-number', dict(host(), null_rdbuf=1), 'expected null_rdbuf'),
        ('null-rdbuf-null', dict(host(), null_rdbuf=None), 'expected null_rdbuf'),
        ('cancel-number', dict(host(), cancel=1), 'expected cancel'),
        ('cancel-null', dict(host(), cancel=None), 'expected cancel'),
        ('cancel-missing', dict(host(), cancel={'op': 'overflow'}), 'expected cancel'),
        ('cancel-extra', dict(host(), cancel={'op': 'overflow', 'call': 0, 'when': 1}), 'expected cancel'),
        ('cancel-op', dict(host(), cancel={'op': 'xsputn', 'call': 0}), 'expected cancel'),
        ('cancel-op-number', dict(host(), cancel={'op': 1, 'call': 0}), 'expected cancel'),
        ('cancel-call-null', dict(host(), cancel={'op': 'sync', 'call': None}), 'expected cancel'),
        ('cancel-call-negative', dict(host(), cancel={'op': 'overflow', 'call': -1}), 'expected cancel'),
        ('cancel-sync-big', dict(host(), cancel={'op': 'sync', 'call': 65}), 'expected cancel'),
        ('cancel-num-put-big', dict(host(), cancel={'op': 'num_put', 'call': 65}), 'expected cancel'),
        ('cancel-overflow-big', dict(host(), cancel={'op': 'overflow', 'call': MAX_CAPACITY + 1}),
         'expected cancel'),
        ('put-throw-never', host(num_put={'call': 0, 'throw': 'never'}), 'expected num_put'),
        ('put-extra', host(num_put=dict(ok_put, width=3)), 'expected num_put'),
        ('put-set-number', host(num_put=dict(ok_put, set_before=5)), 'expected num_put'),
        ('put-set-null', host(num_put=dict(ok_put, set_after=None)), 'expected num_put'),
        ('put-set-missing', host(num_put=dict(ok_put, set_before={'width': 1})), 'expected num_put'),
        ('put-set-extra', host(num_put=dict(ok_put, set_after=dict(ok_change, fill=1))), 'expected num_put'),
        ('put-set-width-big', host(num_put=dict(ok_put, set_before=dict(ok_change, width=4097))),
         'expected num_put'),
        ('put-set-width-float', host(num_put=dict(ok_put, set_before=dict(ok_change, width=1.5))),
         'expected num_put'),
        ('put-set-precision-big', host(num_put=dict(ok_put, set_after=dict(ok_change, precision=2 ** 63))),
         'expected num_put'),
        ('put-set-flags-name', host(num_put=dict(ok_put, set_before=dict(ok_change, flags=['hexadecimal']))),
         'expected num_put'),
        ('put-set-flags-string', host(num_put=dict(ok_put, set_before=dict(ok_change, flags='hex'))),
         'expected num_put'),
        ('put-fill-big', host(num_put=dict(ok_put, fill=256)), 'expected num_put'),
        ('put-fill-null', host(num_put=dict(ok_put, fill=None)), 'expected num_put'),
        ('put-value-big', host(num_put=dict(ok_put, value=2 ** 63)), 'expected num_put'),
        ('put-value-float', host(num_put=dict(ok_put, value=1.0)), 'expected num_put'),
        ('put-delegate-big', host(num_put=dict(ok_put, delegate=3)), 'expected num_put'),
        ('put-delegate-negative', host(num_put=dict(ok_put, delegate=-1)), 'expected num_put'),
        ('tie-null-rdbuf', host(tie=dict(ok_tie, null_rdbuf='yes')), 'expected null_rdbuf'),
        ('tie-cancel-field', host(tie=dict(ok_tie, cancel={'op': 'sync', 'call': 0})),
         'unexpected or missing tie field'),
    ]
    for name, value, message in new_hosts:
        for op in ('insert_size', 'insert_point'):
            add(f'{op}-{name}', {'op': op, 'value': [1, 2], 'stream': dict(good, host=value)}, message)
    cancel = dict(host(), cancel={'op': 'overflow', 'call': 0})
    add('insert-unwinding-excludes-cancel', {'op': 'insert_size', 'value': [1, 2], 'unwinding': True,
                                             'stream': dict(good, host=cancel)}, 'unwinding excludes cancel')
    add('insert-unwinding-false-with-cancel-ok-then-width', {
        'op': 'insert_size', 'value': [1, 2], 'unwinding': False,
        'stream': dict(good, width=MAX_CAPACITY + 1, host=cancel)}, 'unbounded width exceeds limit')
    add('insert-order-unwinding-before-stream', {'op': 'insert_size', 'value': [1, 2], 'unwinding': 1,
                                                 'stream': dict(good, fill=999)}, 'expected unwinding')
    add('insert-order-value-before-unwinding', {'op': 'insert_size', 'value': [1], 'unwinding': 1,
                                                'stream': good}, 'expected point')
    add('insert-order-cancel-conflict-before-width', {
        'op': 'insert_point', 'value': [1, 2], 'unwinding': True,
        'stream': dict(good, width=MAX_CAPACITY + 1, host=cancel)}, 'unwinding excludes cancel')
    add('insert-order-host-fields-before-null-rdbuf', {
        'op': 'insert_point', 'value': [1, 2], 'stream': dict(good, host=dict(host(num_put=1), null_rdbuf=1))},
        'expected num_put')
    add('insert-order-null-rdbuf-before-cancel', {
        'op': 'insert_point', 'value': [1, 2], 'stream': dict(good, host=dict(host(), null_rdbuf=1, cancel=1))},
        'expected null_rdbuf')
    add('insert-order-tie-before-null-rdbuf', {
        'op': 'insert_point', 'value': [1, 2],
        'stream': dict(good, host=dict(host(tie=dict(ok_tie, sync_throw=65, null_rdbuf='x')), null_rdbuf=1))},
        'expected sync_throw')
    add('insert-order-put-call-before-changes', {
        'op': 'insert_point', 'value': [1, 2],
        'stream': dict(good, host=host(num_put={'call': 99, 'throw': 'none', 'set_before': 5}))},
        'expected num_put')
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
