#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Keep malformed target refusals outside real collector exception results."""
import json
import subprocess
import sys

UNIT = 'pcb_calculator.application.pure.p001'


def main():
    if len(sys.argv) < 2:
        raise SystemExit('usage: test_resistor_collector_inputs.py ADAPTER [ADAPTER ...]')
    requests, expected = [], []
    for index, target in enumerate([None, False, [], {}, '0', -1, 1.0]):
        case = 'collector-invalid-target-' + str(index)
        requests.append({'unit': UNIT, 'case': case,
                         'input': {'op': 'collector', 'target_bits': target, 'candidates': []}})
        expected.append({'unit': UNIT, 'case': case,
                         'error': {'kind': 'invalid_input', 'message': 'expected u64 IEEE754 bits'}})
    requests.append({'unit': UNIT, 'case': 'collector-empty',
                     'input': {'op': 'collector', 'target_bits': 0, 'candidates': []}})
    expected.append({'unit': UNIT, 'case': 'collector-empty',
                     'result': {'exception': 'Empty solution collector'}})
    wire = '\n'.join(json.dumps(request) for request in requests) + '\n'
    for binary in sys.argv[1:]:
        process = subprocess.run([binary], input=wire, capture_output=True, text=True,
                                 check=True, timeout=30)
        replies = [json.loads(line) for line in process.stdout.split('\n') if line]
        assert replies == expected, (binary, replies, expected)
    print('Seven complete collector refusals and the legitimate empty result passed')


if __name__ == '__main__':
    main()
