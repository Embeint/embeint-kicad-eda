#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reproduce the independent escaped-key requests without decoding wire escapes."""
import argparse
from pathlib import Path
import sys


def requests():
    lines = []
    for key in ('$serde_json::private::Number', '$serde_json::private::RawValue'):
        spellings = [key.replace('$', chr(92) + 'u0024'),
                     key.replace(':', chr(92) + 'u003a'),
                     ''.join(chr(92) + 'u' + format(ord(char), '04x') for char in key)]
        for encoded in spellings:
            for value in ['"0"', '"[0]"', '"bad"', '"1e1000"', 'null', 'false', '[]', '{}']:
                for template in [
                    '{"unit":"pcb_calculator.application.pure.p001","case":"escaped","input":{"op":"format","values_bits":[{"@":#}]}}',
                    '{"unit":"pcb_calculator.application.pure.p001","case":"escaped","input":{"op":"collector","target_bits":{"@":#},"candidates":[]}}',
                    '{"unit":{"@":#},"case":"escaped","input":{}}',
                    '{"unit":"unknown","case":"escaped","input":{"@":#}}',
                ]:
                    lines.append(template.replace('@', encoded).replace('#', value))
            lines.append('{"unit":"pcb_calculator.application.pure.p001","case":"escaped-dup",'
                         '"input":{"op":"format","values_bits":[{"' + key + '":"0","' + encoded + '":"1"}]}}')
    lines.append('{"unit":"pcb_calculator.application.pure.p001","case":"recovered",'
                 '"input":{"op":"format","values_bits":[0,4607182418800017408]}}')
    return ('\n'.join(lines) + '\n').encode('ascii')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    data = requests()
    if args.check:
        path = Path(__file__).with_name('fixtures') / 'resistor-private-keys-escaped.requests.jsonl'
        if path.read_bytes() != data:
            raise ValueError('Escaped-key request fixture differs from its literal generator')
        print('199 literal escaped-key requests reproduce exactly')
    else:
        sys.stdout.buffer.write(data)


if __name__ == '__main__':
    main()
