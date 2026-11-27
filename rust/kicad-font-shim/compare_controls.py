#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Controls never enter the successful public corpus.

Each control is a request whose pinned evaluation is undefined (a null FONT pointer, signed
overflow or an out-of-range conversion in faceSize), is malformed in a way the refusal order
exercises, or leaves the adapter's verified domain (a name conversion reaching wxFAIL_MSG in
the stream operator or NameAsToken, a unitbuf stream whose failing sync could throw from a
noexcept sentry destructor); generate_cases.py classifies them. The C++ adapter must refuse
each with exactly the recorded error, and Rust must give the same refusal. With --rust only,
the Rust side is checked alone.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'include.font.application.ui.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/font-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'


def expected(control):
    request = control['request']
    return {'unit': request['unit'], 'case': request['case'], 'error': control['error']}


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
    programs = [args.rust] + ([args.oracle] if args.oracle else [])
    for binary in programs:
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=120)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            assert reply == expected(control), (binary, control['request'], reply)
    ops = {}
    for c in controls:
        key = c['request']['input']['op'] + ': ' + c['error']['message']
        ops[key] = ops.get(key, 0) + 1
    print(f'{len(controls)} refusal controls checked on {len(programs)} program(s):',
          dict(sorted(ops.items())))


if __name__ == '__main__':
    main()
