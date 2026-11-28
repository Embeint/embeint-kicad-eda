#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Controls never enter the successful public corpus.

Each control is a request whose pinned evaluation reaches undefined behaviour (signed overflow,
an index outside a vector, an end or null iterator, an out-of-range conversion) or leaves the verified
domain of an out-of-line call (classified by generate_cases.py). The C++ adapter must refuse it
with exactly the recorded error. Rust must give the same refusal, except for a control marked
"rust_unported": there the refusal follows a call the Rust port defers, and Rust must defer to
exactly that owner instead. With --rust only, the Rust side is checked alone.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.geometry.application.ui.p004'
FIXTURE = ROOT / 'port/oracle/fixtures/shape-line-chain-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'


def expected(control, is_rust):
    base = {'unit': control['request']['unit'], 'case': control['request']['case']}
    if is_rust and 'rust_unported' in control:
        return {**base, 'error': {'kind': 'unported', **control['rust_unported']}}
    return {**base, 'error': control['error']}


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
    programs = [(args.rust, True)] + ([(args.oracle, False)] if args.oracle else [])
    replies = {}
    for binary, is_rust in programs:
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=120)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            assert reply == expected(control, is_rust), (binary, control['request'], reply)
        replies[binary] = out
    if args.oracle:
        for control, oracle, rust in zip(controls, replies[args.oracle], replies[args.rust]):
            if 'rust_unported' not in control:
                assert oracle == rust, (control['request']['case'], oracle, rust)
    ops = {}
    for c in controls:
        key = str(c['request']['input']['op']) + (' (after Rust deferral)' if 'rust_unported' in c else '')
        ops[key] = ops.get(key, 0) + 1
    print(f'{len(controls)} undefined-domain controls checked on {len(programs)} program(s):',
          dict(sorted(ops.items())))


if __name__ == '__main__':
    main()
