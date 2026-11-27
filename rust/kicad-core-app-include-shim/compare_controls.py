#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Refusal controls never enter the successful public corpus.

Each control is a request both adapters must refuse: an evaluation the pinned headers leave
undefined (signed overflow in MIRRORVAL or in THROTTLE's chrono arithmetic, a KICAD_T or
FLIP_DIRECTION conversion outside the type's values), a callable that cannot be stored (a
48-byte callable in the default 32-byte INPLACE_FUNCTION does not compile), or a malformed
or out-of-lifetime harness request. Both programs must reply with exactly the recorded error
and agree on the whole reply. With --rust only, the Rust side is checked alone.
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'include.core.application.pure.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/core-app-include-p001-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'


def continuation(control):
    return {'unit': UNIT, 'case': 'after-' + control['request']['case'],
            'input': {'op': 'enumerators'}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--oracle')
    parser.add_argument('--rust')
    parser.add_argument('--write-requests', type=Path,
                        help='export controls interleaved with defined continuation requests')
    args = parser.parse_args()
    controls = [json.loads(s) for s in FIXTURE.read_text(encoding='utf-8').splitlines()]
    public = {json.loads(s)['case'] for s in CASES.read_text(encoding='utf-8').splitlines()}
    names = [c['request']['case'] for c in controls]
    assert len(set(names)) == len(names), 'duplicate control case'
    assert not public & set(names), sorted(public & set(names))
    requests = ''.join(json.dumps(request, ensure_ascii=False) + '\n'
                       for c in controls for request in (c['request'], continuation(c)))
    if args.write_requests:
        args.write_requests.write_text(requests, encoding='utf-8', newline='\n')
    if not args.rust:
        if not args.write_requests:
            parser.error('--rust or --write-requests is required')
        return
    binaries = [b for b in (args.oracle, args.rust) if b]
    replies = {}
    for binary in binaries:
        r = subprocess.run([binary], input=requests.encode(), capture_output=True, check=True,
                           timeout=120)
        out = [json.loads(s) for s in r.stdout.decode().splitlines()]
        assert len(out) == 2 * len(controls), (binary, len(out))
        continued_result = None
        for control, reply, continued in zip(controls, out[::2], out[1::2]):
            expected = {'unit': UNIT, 'case': control['request']['case'], 'error': control['error']}
            assert reply == expected, (binary, reply, expected)
            assert set(continued) == {'unit', 'case', 'result'}, (binary, continued)
            assert continued['unit'] == UNIT and continued['case'] == continuation(control)['case']
            if continued_result is None:
                continued_result = continued['result']
            assert continued['result'] == continued_result, (binary, continued)
        replies[binary] = out
    if args.oracle:
        assert replies[args.oracle] == replies[args.rust], 'oracle/Rust continuation replies differ'
    kinds = {}
    for c in controls:
        key = str(c['request']['input'].get('op', 'malformed'))
        kinds[key] = kinds.get(key, 0) + 1
    print(f'{len(controls)} refusal controls checked on {len(binaries)} program(s), by operation '
          f'{dict(sorted(kinds.items()))}; every refusal matches and {len(controls)} defined '
          'continuations succeed')


if __name__ == '__main__':
    main()
