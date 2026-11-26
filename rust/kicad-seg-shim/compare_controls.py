#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain controls never enter the successful public corpus.

Each control is a request whose pinned SEG evaluation reaches signed overflow, negation of
the minimum or an out-of-range floating-to-integer cast before any call the Rust port defers
(classified by generate_cases.py's exact mirror). The C++ adapter's guard and the Rust port
must both refuse it with exactly the recorded error, and the two replies must be identical.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.seg.application.ui.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/seg-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--oracle', required=True)
    parser.add_argument('--rust', required=True)
    args = parser.parse_args()
    controls = [json.loads(s) for s in FIXTURE.read_text(encoding='utf-8').splitlines()]
    public = {json.loads(s)['case'] for s in CASES.read_text(encoding='utf-8').splitlines()}
    names = [c['request']['case'] for c in controls]
    assert len(set(names)) == len(names), 'duplicate control case'
    assert not public & set(names), sorted(public & set(names))
    requests = ''.join(json.dumps(c['request']) + '\n' for c in controls)
    replies = {}
    for binary in (args.oracle, args.rust):
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=120)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            expected = {'unit': control['request']['unit'], 'case': control['request']['case'],
                        'error': control['error']}
            assert reply == expected, (binary, control['request'], reply)
        replies[binary] = out
    ops = {}
    for control, oracle, rust in zip(controls, replies[args.oracle], replies[args.rust]):
        assert oracle == rust, (control['request']['case'], oracle, rust)
        op = control['request']['input']['op']
        ops[op] = ops.get(op, 0) + 1
    print(f'{len(controls)} undefined-domain controls refused identically by both programs:',
          dict(sorted(ops.items())))


if __name__ == '__main__':
    main()
