#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain controls never enter the successful public corpus.

Each control reaches an operation the pinned C++ leaves undefined (signed int overflow in
Mirror or in GetPointAtAngle's final addition, a NaN or out-of-range double -> int conversion in
the major-axis constructor, std::max with a NaN radius). Both programs must refuse exactly that
observation with the recorded error, stop there, keep every earlier observation as a value, and
agree on the whole reply exactly (parsed JSON, IEEE values as bit strings). With --rust only,
the Rust side is checked alone.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.ellipse.application.ui.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/ellipse-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'


def is_error(v):
    return isinstance(v, dict) and 'error' in v


def check(control, reply, binary):
    name = control['request']['case']
    assert set(reply) == {'unit', 'case', 'result'}, (binary, name, reply)
    assert reply['unit'] == UNIT and reply['case'] == name, (binary, reply)
    result = reply['result']
    refusal = {'error': control['error']}
    if control['undefined'] == 'state':
        assert result == {'state': refusal, 'steps': []}, (binary, name, result)
        return
    index = control['undefined']
    steps = result['steps']
    assert not is_error(result['state']), (binary, name, result)
    assert len(steps) == index + 1, (binary, name, steps)
    assert steps[index] == refusal, (binary, name, steps)
    assert not any(is_error(s) for s in steps[:index]), (binary, name, steps)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--oracle')
    parser.add_argument('--rust', required=True)
    args = parser.parse_args()
    controls = [json.loads(s) for s in FIXTURE.read_text(encoding='utf-8').splitlines()]
    public = {json.loads(s)['case'] for s in CASES.read_text(encoding='utf-8').splitlines()}
    names = [c['request']['case'] for c in controls]
    assert len(set(names)) == len(names), 'duplicate control case'
    assert not public & set(names), sorted(public & set(names))
    requests = ''.join(json.dumps(c['request']) + '\n' for c in controls)
    binaries = [b for b in (args.oracle, args.rust) if b]
    replies = {}
    for binary in binaries:
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=120)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            check(control, reply, binary)
        replies[binary] = out
    if args.oracle:
        for control, oracle, rust in zip(controls, replies[args.oracle], replies[args.rust]):
            assert oracle == rust, (control['request']['case'], oracle, rust)
    kinds = {}
    for c in controls:
        key = 'constructor' if c['undefined'] == 'state' else \
            c['request']['input']['steps'][c['undefined']]['op']
        kinds[key] = kinds.get(key, 0) + 1
    print(f'{len(controls)} undefined-domain controls checked on {len(binaries)} program(s), '
          f'refused at {dict(sorted(kinds.items()))}; every earlier observation equal')


if __name__ == '__main__':
    main()
