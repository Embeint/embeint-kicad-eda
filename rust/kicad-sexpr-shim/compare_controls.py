#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Undefined-domain and bound controls never enter the successful public corpora.

Each control is a request the reference adapter must refuse with exactly the recorded error:
either a request bound (tree depth or size, argument count) or a request whose pinned
evaluation reaches undefined behaviour (classified by generate_cases.py's replay of the
reference control flow). Rust must give the same refusal, except for a control marked
"rust_unported": there the undefined operation lies inside a function the Rust port defers,
and Rust must defer to exactly that owner instead. With --rust only, the Rust side is
checked alone.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNITS = ('libs.sexpr.include.sexpr.application.pure.p001', 'libs.sexpr.application.pure.p001')
FIXTURE = ROOT / 'port/oracle/fixtures/sexpr-controls.jsonl'


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
    public = set()
    for unit in UNITS:
        path = ROOT / 'port/cases' / f'{unit}.jsonl'
        if path.exists():
            public |= {json.loads(s)['case'] for s in path.read_text(encoding='utf-8').splitlines()}
    names = [c['request']['case'] for c in controls]
    assert len(set(names)) == len(names), 'duplicate control case'
    assert not public & set(names), sorted(public & set(names))
    requests = ''.join(json.dumps(c['request']) + '\n' for c in controls)
    programs = [(args.rust, True)] + ([(args.oracle, False)] if args.oracle else [])
    replies = {}
    for binary, is_rust in programs:
        r = subprocess.run([binary], input=requests, text=True, capture_output=True, check=True,
                           timeout=300)
        out = [json.loads(s) for s in r.stdout.splitlines()]
        assert len(out) == len(controls), (binary, len(out))
        for control, reply in zip(controls, out):
            assert reply == expected(control, is_rust), (binary, control['request']['case'], reply)
        replies[binary] = out
    if args.oracle:
        for control, oracle, rust in zip(controls, replies[args.oracle], replies[args.rust]):
            if 'rust_unported' not in control:
                assert oracle == rust, (control['request']['case'], oracle, rust)
    ops = {}
    for c in controls:
        key = c['request']['input']['op'] + (' (after Rust deferral)' if 'rust_unported' in c else '')
        ops[key] = ops.get(key, 0) + 1
    print(f'{len(controls)} controls checked on {len(programs)} program(s):', dict(sorted(ops.items())))


if __name__ == '__main__':
    main()
