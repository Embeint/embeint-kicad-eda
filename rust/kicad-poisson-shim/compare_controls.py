#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Refusal controls and exact-limit probes, outside the public corpus.

1. Every request of port/oracle/fixtures/poisson-disk-controls.jsonl goes to each program in
   one process, in order. Each refusal must be exactly its recorded invalid_input reply on both
   sides, and the continuation after it must be a successful tile, so a refusal leaves the
   process answering. The two programs' replies must be identical.
2. Exact sample limits: for fixed tiles, the oracle's count N is read with ample limits; then
   max_samples = N must give the same full tile on both sides and N - 1 the same "sample limit"
   refusal (the port stops at the limit, the reference adapter refuses from its count).
"""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.src.geometry.poisson_disk.application.pure.p001'
FIXTURE = ROOT / 'port/oracle/fixtures/poisson-disk-controls.jsonl'
CASES = ROOT / 'port/cases' / f'{UNIT}.jsonl'
PROBES = [('3fe0000000000000', 5), ('3fd0000000000000', 6), ('3fb999999999999a', 7),
          ('3f947ae147ae147b', 8)]


def run(binary, requests):
    text = ''.join(json.dumps(r, sort_keys=True) + '\n' for r in requests)
    r = subprocess.run([binary], input=text, text=True, capture_output=True, check=True,
                       timeout=600)
    out = [json.loads(s) for s in r.stdout.splitlines()]
    assert len(out) == len(requests), (binary, len(out), len(requests))
    for request, reply in zip(requests, out):
        assert reply.get('unit') == UNIT and reply.get('case') == request['case'], (binary, reply)
    return out


def tile(bits, seed, samples):
    return {'op': 'tile', 'min_dist': bits, 'seed': seed, 'max_grid_cells': 4194304,
            'max_samples': samples, 'emit': 'samples'}


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
    requests = [c['request'] for c in controls]
    replies = {b: run(b, requests) for b in (args.oracle, args.rust)}
    refused = 0
    for i, control in enumerate(controls):
        oracle, rust = replies[args.oracle][i], replies[args.rust][i]
        assert oracle == rust, (control['request']['case'], oracle, rust)
        if control['expect'] == 'result':
            assert set(oracle) == {'unit', 'case', 'result'}, oracle
            assert oracle['result']['count'] > 0, oracle
        else:
            assert oracle == {'unit': UNIT, 'case': control['request']['case'],
                              'error': control['expect']}, oracle
            refused += 1

    counts = run(args.oracle, [{'unit': UNIT, 'case': f'probe-{i}', 'input': tile(b, s, 4194304)}
                               for i, (b, s) in enumerate(PROBES)])
    probes = []
    for i, ((b, s), reply) in enumerate(zip(PROBES, counts)):
        n = reply['result']['count']
        assert n >= 2, reply
        probes.append({'unit': UNIT, 'case': f'probe-{i}-exact', 'input': tile(b, s, n)})
        probes.append({'unit': UNIT, 'case': f'probe-{i}-short', 'input': tile(b, s, n - 1)})
    pairs = {b: run(b, probes) for b in (args.oracle, args.rust)}
    for i, request in enumerate(probes):
        oracle, rust = pairs[args.oracle][i], pairs[args.rust][i]
        assert oracle == rust, (request['case'], oracle, rust)
        if request['case'].endswith('-exact'):
            assert oracle['result'] == counts[i // 2]['result'], request['case']
        else:
            assert oracle['error'] == {'kind': 'invalid_input', 'message': 'sample limit'}, oracle
    print(f'{len(controls)} control requests: {refused} refusals identical, '
          f'{len(controls) - refused} continuations succeed and agree; '
          f'{len(probes)} exact-limit probes agree')


if __name__ == '__main__':
    main()
