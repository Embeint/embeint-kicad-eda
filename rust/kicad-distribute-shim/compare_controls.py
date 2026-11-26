#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain controls never enter the successful public corpus.

Each control request holds at least one call whose pinned int arithmetic overflows (undefined,
C++ [expr.pre]/4) and ends with a defined call. Both programs must refuse exactly the recorded
calls with the recorded first overflowing operation, return deltas for every other call
(including the continuation after the last refusal), and agree on the whole reply exactly.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.distribute.application.ui.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/distribute-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'


def check(control, result):
    calls = control['request']['input']['calls']
    assert isinstance(result, list) and len(result) == len(calls), result
    for i, (call, reply) in enumerate(zip(calls, result)):
        message = control['refused'].get(str(i))
        if message is None:
            items = call.get('extents', call.get('positions'))
            assert set(reply) == {'deltas'}, (control['request']['case'], i, reply)
            assert len(reply['deltas']) == len(items), (control['request']['case'], i)
            assert all(isinstance(d, int) for d in reply['deltas']), reply
        else:
            expected = {'refused': {'kind': 'signed_overflow', 'message': message}}
            assert reply == expected, (control['request']['case'], i, reply)


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
    for c in controls:
        last = len(c['request']['input']['calls']) - 1
        assert c['refused'] and str(last) not in c['refused'], c['request']['case']
    requests = ''.join(json.dumps(c['request']) + '\n' for c in controls)
    replies = {}
    for binary in (args.oracle, args.rust):
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=120)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            assert set(reply) == {'unit', 'case', 'result'}, (binary, control['request'], reply)
            assert reply['unit'] == UNIT and reply['case'] == control['request']['case'], reply
            check(control, reply['result'])
        replies[binary] = out
    for control, oracle, rust in zip(controls, replies[args.oracle], replies[args.rust]):
        assert oracle == rust, (control['request']['case'], oracle, rust)
    refused = sum(len(c['refused']) for c in controls)
    continued = sum(len(c['request']['input']['calls']) - len(c['refused']) for c in controls)
    print(f'{len(controls)} undefined-domain controls checked: {refused} refused calls and '
          f'{continued} defined continuation calls agree on both sides')


if __name__ == '__main__':
    main()
