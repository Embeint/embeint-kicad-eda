#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Harness controls never enter the public corpus.

Each control is a malformed request. Both programs must refuse it before any pinned call with
exactly the recorded error, and agree on the whole reply. No valid input of this unit leaves the
defined C++ domain, so there are no undefined-domain controls (see generate_cases.py controls()).
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.arc_construction.application.ui.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/arc-construction-controls.jsonl'
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
                           timeout=60)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            expected = {'unit': UNIT, 'case': control['request']['case'], 'error': control['error']}
            assert reply == expected, (binary, reply, expected)
        replies[binary] = out
    assert replies[args.oracle] == replies[args.rust]
    print(f'{len(controls)} harness controls refused identically by both programs')


if __name__ == '__main__':
    main()
