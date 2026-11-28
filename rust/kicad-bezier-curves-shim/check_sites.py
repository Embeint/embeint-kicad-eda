#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check that an oracle binary compiles the bound NaN-order sites as recorded.

The Rust port follows the operand order GCC 13.3 chose for the out-of-line BEZIER_POLY::eval,
BEZIER_POLY::subdivide, BEZIER_POLY::thirdControlPointDeviation and
TransformEllipseToBeziers<double> (port/findings/bezier-curves-port.md). This script disassembles
those functions from a kicad-oracle binary with objdump and compares their instructions, with
addresses, displacements and symbol names removed, to reference-sites.txt. Any difference (another
compiler, flag, header or adapter change) fails instead of being compared silently; the order must
then be re-derived and reviewed, never re-recorded to accept drift.

--write records the given binary's sites (only for a reviewed derivation).
"""
import argparse
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
RECORD = HERE / 'reference-sites.txt'
FUNCTIONS = [
    'BEZIER_POLY::eval(double)',
    'BEZIER_POLY::subdivide(double, BEZIER_POLY&, BEZIER_POLY&)',
    'BEZIER_POLY::thirdControlPointDeviation()',
    'void TransformEllipseToBeziers<double>(ELLIPSE<double> const&, '
    'std::vector<BEZIER<double>, std::allocator<BEZIER<double> > >&)',
]


def normalise(line):
    # "  59af24:\tmovsd  0x310414(%rip),%xmm1        # 8ab340 <...>"
    parts = line.split('\t')
    if len(parts) < 2 or not re.match(r'\s*[0-9a-f]+:$', parts[0]):
        return None
    text = parts[-1].split('#')[0].strip()
    text = re.sub(r'<[^>]*>', '<>', text)
    text = re.sub(r'\b[0-9a-f]{4,}\b', 'A', text)
    text = re.sub(r'-?0x[0-9a-f]+', 'D', text)
    return re.sub(r'\s+', ' ', text)


def sites(binary):
    out = []
    for function in FUNCTIONS:
        listing = subprocess.run(['objdump', '-d', '-C', '--no-show-raw-insn', f'--disassemble={function}',
                                  binary], check=True, capture_output=True, text=True).stdout
        body = [n for n in (normalise(l) for l in listing.splitlines()) if n]
        if not body:
            raise SystemExit(f'{function}: not found in {binary}')
        out.append(f'== {function}')
        out.extend(body)
    return '\n'.join(out) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    text = sites(args.binary)
    if args.write:
        RECORD.write_bytes(text.encode())
        print(f'recorded {text.count(chr(10))} lines')
        return
    expected = RECORD.read_text(encoding='utf-8')
    if text != expected:
        import difflib
        diff = difflib.unified_diff(expected.splitlines(), text.splitlines(), 'recorded', args.binary, lineterm='')
        print('\n'.join(list(diff)[:200]))
        raise SystemExit('the oracle compiles the bound sites differently; re-derive the order')
    print(f'{len(FUNCTIONS)} bound functions match the recorded instructions')


if __name__ == '__main__':
    main()
