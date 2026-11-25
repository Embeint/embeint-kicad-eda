#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain controls never enter the successful public corpus.

Each control observes a state whose GetArcAngle would call std::clamp with a NaN
(undefined, C++20 [alg.clamp]). Both programs must refuse that observation with the
recorded error at exactly the recorded steps, keep every other observation as a
value, and agree on the whole reply exactly (parsed JSON, IEEE values as bit strings).
"""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.arc_chord_params.application.pure.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/arc-chord-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
BITS = re.compile(r'[0-9a-f]{16}')
FIELDS = {'valid', 'chord_length', 'half_chord', 'chord_unit', 'normal_unit', 'sagitta', 'radius',
          'center_offset', 'chord_midpoint', 'raw', 'center', 'start_angle', 'end_angle', 'arc_angle'}


def is_value(v):
    if isinstance(v, bool):
        return True
    if isinstance(v, str):
        return BITS.fullmatch(v) is not None
    return isinstance(v, list) and all(isinstance(x, str) and BITS.fullmatch(x) for x in v)


def check_states(control, result):
    if control['request']['input']['op'] == 'default':
        states = [result]
    else:
        assert isinstance(result, list), result
        assert len(result) == len(control['request']['input']['computes']), result
        assert all(isinstance(step['returned'], bool) for step in result), result
        states = [step['state'] for step in result]
    for i, state in enumerate(states):
        assert set(state) == FIELDS, (control['request']['case'], i, state)
        for key, value in state.items():
            if key == 'arc_angle' and i in control['undefined_steps']:
                assert value == {'error': control['error']}, (control['request']['case'], i, value)
            else:
                assert is_value(value), (control['request']['case'], i, key, value)


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
    assert all(c['undefined_steps'] for c in controls)
    requests = ''.join(json.dumps(c['request']) + '\n' for c in controls)
    replies = {}
    for binary in (args.oracle, args.rust):
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=60)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            assert set(reply) == {'unit', 'case', 'result'}, (binary, control, reply)
            assert reply['unit'] == control['request']['unit'], (binary, reply)
            assert reply['case'] == control['request']['case'], (binary, reply)
            check_states(control, reply['result'])
        replies[binary] = out
    for control, oracle, rust in zip(controls, replies[args.oracle], replies[args.rust]):
        assert oracle == rust, (control['request']['case'], oracle, rust)
    steps = sum(len(c['undefined_steps']) for c in controls)
    print(f'{len(controls)} undefined-domain controls checked: {steps} refused arc-angle '
          'observations agree, every other observation equal on both sides')


if __name__ == '__main__':
    main()
