#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain controls never enter the successful public corpus.

Each control is a TRANSFORM request whose pinned evaluation would overflow a signed
integer or divide by zero (undefined behaviour). The C++ adapter refuses it before calling
the reference body; the Rust port refuses it itself. Both programs must reply with exactly
the recorded error, and their whole replies must be equal.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.transform.application.ui.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/transform-controls.jsonl'
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
            expected = {'unit': UNIT, 'case': control['request']['case'], 'error': control['error']}
            assert reply == expected, (binary, reply, expected)
        replies[binary] = out
    assert replies[args.oracle] == replies[args.rust]
    kinds = {}
    for c in controls:
        key = (c['request']['input']['op'], c['error']['message'])
        kinds[key] = kinds.get(key, 0) + 1
    detail = ', '.join(f'{op} {msg}: {n}' for (op, msg), n in sorted(kinds.items()))
    print(f'{len(controls)} undefined-domain controls refused identically ({detail})')


if __name__ == '__main__':
    main()
