#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the fixed-seed public corpus and refusal controls of include.font.application.ui.p001.

Writes port/cases/include.font.application.ui.p001.jsonl (cases every program must evaluate)
and port/oracle/fixtures/font-controls.jsonl (requests the C++ adapter and the Rust shim must
refuse identically: undefined evaluations and requests outside the verified domain). The
classification mirrors port/oracle/shims/include_font_p001.cpp. With --check, the committed
files must equal the regenerated ones.
"""
import argparse
import json
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'include.font.application.ui.p001'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
CONTROLS = ROOT / 'port/oracle/fixtures/font-controls.jsonl'
SEED = 0x464F4E54  # "FONT"
UNDEFINED = 'undefined font operation'
OUTSIDE = 'outside the verified domain'
INT_MIN, INT_MAX = -2**31, 2**31 - 1
I64_MIN, I64_MAX = -2**63, 2**63 - 1

# Names and texts as wchar_t units. INT_MIN is 0x80000000, which the strict UTF-8 encoder
# masks to a NUL byte; 0x80000041 masks to 'A'.
NAMES = {
    'empty': [],
    'ascii': [ord(c) for c in 'KiCad Font'],
    'del': [0x7F, 0x41],
    'latin1': [0x80, 0xE9, 0xFF],
    'bmp': [0x41, 0x100, 0x7FF, 0x800, 0xFFFF],
    'surrogate': [0xD800, 0xDFFF],
    'astral': [0x10000, 0x10FFFF],
    'nul-byte': [0x41, INT_MIN, 0x42],
    'masked-a': [0x80000041 - 2**32, 0x42],
    'embedded-nul': [0x41, 0, 0x110000],
    'nul-first': [0, 0x42],
    'quote': [0x22, 0x5D, 0x5B],
    'mixed': [0x61, 0x3B1, 0x20AC, 0x1F600],
}
ASSERT_NAMES = {
    'beyond-unicode': [0x110000],
    'after-ascii': [0x41, 0x7FFFFFFF],
    'minus-one': [-1],
    'after-bmp': [0x100, 0x200000],
    'int-max': [INT_MAX],
    'before-nul': [0x40000000, 0],
}


def request(case, inp):
    return {'case': case, 'input': inp, 'unit': UNIT}


def bits(value):
    return struct.pack('>d', value).hex()


def base_font(name):
    return {'kind': 'base', 'name': name}


def scripted_font(name, s, o, b, i):
    return {'kind': 'scripted', 'name': name, 'stroke': s, 'outline': o, 'bold': b, 'italic': i}


def outline_font(name=(), file=(), face=None, fake_bold=False, fake_ital=False):
    return {'kind': 'outline', 'name': list(name), 'file': list(file),
            'face': None if face is None else {'style_flags': face},
            'fake_bold': fake_bold, 'fake_ital': fake_ital}


def reaches_assert(name):
    for unit in name:
        if unit == 0:
            return False
        if (unit & 0xFFFFFFFF) & 0x7FFFFFFF > 0x10FFFF:
            return True
    return False


def face_size_defined(size):
    scaled = size * 64
    if not INT_MIN <= scaled <= INT_MAX:
        return False
    value = float(scaled) * 1.4
    return -2147483649.0 < value < 2147483648.0


STATES = [{'bad': b, 'eof': e, 'fail': f} for b in (False, True) for e in (False, True)
          for f in (False, True)]
GOOD = STATES[0]


def stream(state=GOOD, exceptions=GOOD, flags=(), width=0, fill=32, buffer=True,
           capacity=None, sync_fails=False):
    return {'state': state, 'exceptions': exceptions, 'flags': list(flags), 'width': width,
            'fill': fill, 'buffer': {'capacity': capacity, 'sync_fails': sync_fails} if buffer else None}


def insert_refusal(inp):
    font, spec = inp['font'], inp['stream']
    if font is None:
        return UNDEFINED if inp['pointer'] else 'expected font'
    if reaches_assert(font['name']):
        return OUTSIDE
    mask = spec['exceptions']
    if ('unitbuf' in spec['flags'] and spec['buffer'] is not None and spec['buffer']['sync_fails']
            and any(mask.values())):
        return OUTSIDE
    return None


def generate():
    rng = random.Random(SEED)
    cases, controls = [], []

    def add(case, inp, refusal=None):
        req = request(case, inp)
        if refusal is None:
            cases.append(req)
        else:
            controls.append({'error': {'kind': 'invalid_input', 'message': refusal}, 'request': req})

    # Constants and flag predicates.
    add('text_style', {'op': 'text_style'})
    flag_values = list(range(64)) + [0xFFFFFFFF, 0x80000000, 0xFFFFFFC0, 0x7FFFFFFF]
    flag_values += [rng.getrandbits(32) for _ in range(120)]
    for index, flags in enumerate(flag_values):
        add(f'style_predicates-{index}', {'op': 'style_predicates', 'flags': flags})

    # FONT queries on every kind.
    fonts = {}
    for key, name in NAMES.items():
        fonts[f'base-{key}'] = base_font(name)
    for mask in range(16):
        flags = [bool(mask >> bit & 1) for bit in range(4)]
        fonts[f'scripted-{mask}'] = scripted_font(NAMES['ascii'], *flags)
    face_flags = [None, 0, 1, 2, 3, 4, -1, -2, -3, I64_MIN, I64_MAX, 0x100, 6, 7]
    for face in face_flags:
        for fake in range(4):
            fonts[f'outline-{face}-{fake}'] = outline_font(
                NAMES['ascii'], NAMES['bmp'], face, bool(fake & 1), bool(fake & 2))
    for key, name in NAMES.items():
        fonts[f'outline-name-{key}'] = outline_font(name, name[::-1], 2)
    for key, font in fonts.items():
        add(f'queries-{key}', {'op': 'queries', 'font': font})

    # linesCount on the base and outline kinds.
    texts = [[], [10], [10, 10], [0x61], [0x61, 10], [10, 0x61], [0x61, 10, 0x62],
             [10, 0, 10, 13], [13, 10], [0, 10], [10] * 40, [10] * 40 + [0x61],
             [-10, 10 + 2**31 - 2**32, 0x10000 + 10, 10]]
    alphabet = [10, 13, 0, 0x61, 0x80, -1, 0x0A0A]
    for _ in range(60):
        texts.append([rng.choice(alphabet) for _ in range(rng.randrange(0, 24))])
    texts.append([10] * 4096)
    for index, units in enumerate(texts):
        for kind, font in (('base', base_font(NAMES['ascii'])),
                           ('outline', outline_font(NAMES['ascii']))):
            add(f'lines_count-{kind}-{index}', {'op': 'lines_count', 'font': font, 'text': units})

    # NameAsToken: the returned pointer always dangles.
    for kind, font in (('base', base_font(NAMES['ascii'])), ('outline', outline_font([])),
                       ('scripted', scripted_font([], True, False, True, False))):
        add(f'name_as_token-{kind}', {'op': 'name_as_token', 'font': font}, UNDEFINED)

    # operator<< for both overloads.
    insert_index = 0

    def insert(font, spec, pointer=False, tag=''):
        nonlocal insert_index
        inp = {'op': 'insert', 'font': font, 'pointer': pointer, 'stream': spec}
        add(f'insert-{tag}-{insert_index}', inp, insert_refusal(inp))
        insert_index += 1

    for key, font in fonts.items():
        for pointer in (False, True):
            insert(font, stream(), pointer, f'font-{key}')
    for key, name in ASSERT_NAMES.items():
        insert(base_font(name), stream(), False, f'assert-{key}')
        insert(outline_font(name, [], 1), stream(state=STATES[4]), True, f'assert-{key}')
    insert(None, stream(), True, 'null-pointer')
    insert(None, stream(capacity=0), True, 'null-pointer')
    insert(None, stream(), False, 'null-reference')
    sample = [fonts['base-ascii'], fonts['scripted-15'], fonts['scripted-0'],
              fonts['outline-3-3'], fonts['outline-name-latin1'], fonts['base-nul-byte'],
              fonts['base-empty']]
    adjust = [(), ('left',), ('right',), ('internal',), ('left', 'right'), ('left', 'internal')]
    for font in sample:
        for width in (-5, 0, 1, 6, 7, 8, 9, 12, 40):
            for flags in adjust:
                insert(font, stream(width=width, flags=flags, fill=42), rng.random() < 0.5, 'width')
        for fill in (0, 255, 0x20, 0x80):
            insert(font, stream(width=10, fill=fill), False, 'fill')
        for state in STATES:
            for mask in STATES:
                insert(font, stream(state=state, exceptions=mask), rng.random() < 0.5, 'state')
        for capacity in list(range(0, 14)) + [20, 26, 27, 28, 40, 4096]:
            insert(font, stream(capacity=capacity), False, 'capacity')
            insert(font, stream(capacity=capacity, exceptions=STATES[4], width=9), True, 'capacity')
        for state in (GOOD, STATES[1], STATES[2], STATES[4]):
            for sync_fails in (False, True):
                for mask in (GOOD, STATES[1], STATES[4], STATES[7]):
                    insert(font, stream(state=state, exceptions=mask, flags=('unitbuf',),
                                        sync_fails=sync_fails, capacity=None), False, 'unitbuf')
                insert(font, stream(state=state, flags=('unitbuf',), sync_fails=sync_fails,
                                    capacity=5), True, 'unitbuf')
        for state in STATES:
            insert(font, stream(state=state, buffer=False, width=4), False, 'nobuf')
            insert(font, stream(state=state, buffer=False, exceptions=STATES[4],
                                flags=('unitbuf',)), True, 'nobuf')
    for _ in range(400):
        font = rng.choice(list(fonts.values()))
        spec = stream(state=rng.choice(STATES), exceptions=rng.choice(STATES),
                      flags=tuple(f for f in ('left', 'right', 'internal', 'unitbuf')
                                  if rng.random() < 0.25),
                      width=rng.choice([0, 0, 3, 9, 15, 33, -1]), fill=rng.randrange(256),
                      buffer=rng.random() < 0.9,
                      capacity=rng.choice([None, None, 0, 4, 9, 18, 30]),
                      sync_fails=rng.random() < 0.3)
        insert(font, spec, rng.random() < 0.5, 'random')

    # The cursorless Draw overload forwards to the out-of-line Draw (deferred in Rust).
    draw_index = 0
    for font in (fonts['base-ascii'], fonts['outline-3-0'], fonts['scripted-5']):
        for text, position, mouse, url in (([], [0, 0], None, None),
                                           ([0x41], [1, 2], None, None),
                                           ([0x41, 10, 0x42], [INT_MIN, INT_MAX], [3, 4], [0x75]),
                                           ([0x78], [-7, 9], None, [])):
            add(f'draw-{draw_index}', {'op': 'draw', 'font': font, 'text': text,
                                       'position': position, 'mouse_pos': mouse,
                                       'active_url': url})
            draw_index += 1

    # OUTLINE_FONT members.
    outline_fonts = {k: v for k, v in fonts.items() if v['kind'] == 'outline'}
    for key, font in outline_fonts.items():
        add(f'outline-{key}', {'op': 'outline', 'font': font})
    calls_list = [[], ['bold'], ['ital'], ['bold', 'ital'], ['ital', 'bold'], ['bold', 'bold'],
                  ['ital', 'ital', 'bold', 'bold', 'ital', 'bold', 'ital', 'bold']]
    for face in (None, 0, 1, 2, 3):
        for index, calls in enumerate(calls_list):
            add(f'set_fake-{face}-{index}', {'op': 'set_fake', 'font': outline_font(face=face),
                                            'calls': calls})
    em_values = [0.0, -0.0, 1.0, -1.0, 2.0, 0.5, 1e-310, -5e-324, 1.7976931348623157e308,
                 -1.7976931348623157e308, 1.2840787425410998e308, 1.2840787425410999e308,
                 float('inf'), float('-inf'), 2.2250738585072014e-308, 1e100, 3.0, 1 / 3]
    em_bits = [bits(v) for v in em_values]
    em_bits += ['7ff8000000000000', 'fff8000000000000', '7ff0000000000001', 'fff0000000000123',
                '7ff8dead0000beef', 'fff4000000000000', '7fffffffffffffff', 'ffffffffffffffff']
    em_bits += [f'{rng.getrandbits(64):016x}' for _ in range(150)]
    for index, value in enumerate(em_bits):
        font = outline_font(face=rng.choice([None, 1]))
        add(f'em_size-{index}', {'op': 'em_size', 'font': font, 'size': value})
    for index, flags in enumerate(flag_values):
        add(f'baseline_offset-{index}', {'op': 'baseline_offset', 'font': outline_font(),
                                         'flags': flags})
    sizes = [0, 1, -1, 16, -16, 2, 7, 23967451, 23967452, -23967451, -23967452, 33554431,
             33554432, -33554432, -33554433, INT_MAX, INT_MIN, 1000000, -1000000, 5, 14, 15]
    sizes += [rng.randrange(-30000000, 30000000) for _ in range(120)]
    sizes += [rng.randrange(INT_MIN, INT_MAX + 1) for _ in range(30)]
    for op in ('face_size', 'subscript_size'):
        for index, size in enumerate(sizes):
            add(f'{op}-{index}', {'op': op, 'font': outline_font(), 'size': size},
                None if face_size_defined(size) else UNDEFINED)
    return cases, controls


def dump(records):
    return ''.join(json.dumps(r, sort_keys=True, separators=(',', ':')) + '\n' for r in records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    cases, controls = generate()
    names = [r['case'] for r in cases] + [c['request']['case'] for c in controls]
    assert len(names) == len(set(names)), 'duplicate case id'
    outputs = {CASES: dump(cases), CONTROLS: dump(controls)}
    if args.check:
        stale = [str(p.relative_to(ROOT)) for p, text in outputs.items()
                 if not p.exists() or p.read_text(encoding='utf-8') != text]
        if stale:
            print('stale generated files: ' + ', '.join(stale), file=sys.stderr)
            return 1
    else:
        for path, text in outputs.items():
            path.write_bytes(text.encode('utf-8'))
    print(f'{len(cases)} cases, {len(controls)} controls')
    return 0


if __name__ == '__main__':
    sys.exit(main())
