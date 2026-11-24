# SPDX-License-Identifier: GPL-3.0-or-later
"""Approve only the initial, exact 7460-case math baseline after a full object gate.

No answers are supplied here. Propose and bless must independently obtain live
agreement; this guard also pins the complete output to the previously audited
live corpus, and binds that proposal to this run's eligible oracle.
"""
import argparse
import hashlib
import json
from pathlib import Path

UNIT = 'libs.kimath.include.math.application.pure.p001'
CASES = 'c82021245968c5f9cdbf23b2c1b681257d4ba68836ad1d884b41bb77dc6ac287'
OUTPUT = '5b3e42d65eb6b6126acbbf5d45cfbdcc27d461c25558179a84aa0b1c5dd4525b'
COUNT = 7460
EMPTY = hashlib.sha256(b'').hexdigest()
GENERATED = 'd24ff0f24a1fae55cc8e9eb85fa783282030083802d58eb0e332be331c6935af'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def validate(proposal, requests, gate):
    if (proposal['format'] != 1 or proposal['unit'] != UNIT or proposal['old_sha256'] != EMPTY
            or sha(requests) != CASES or proposal['cases_sha256'] != CASES):
        raise ValueError('not the authorized initial corpus/baseline')
    ids = [json.loads(line)['case'] for line in requests.splitlines()]
    if len(ids) != COUNT or len(set(ids)) != COUNT or len(proposal['changes']) != COUNT:
        raise ValueError('missing, duplicate or extra baseline cases')
    lines = []
    for i, change in enumerate(proposal['changes']):
        if (set(change) != {'case', 'old_index', 'new_index', 'old', 'new'}
                or change['old_index'] is not None or change['old'] is not None
                or type(change['new_index']) is not int or change['new_index'] != i
                or change['case'] != ids[i] or type(change['new']) is not str
                or '\n' in change['new'] or '\r' in change['new']):
            raise ValueError('baseline is not exactly the ordered additions')
        value = json.loads(change['new'])
        if set(value) != {'unit', 'case', 'result'} or value['unit'] != UNIT or value['case'] != ids[i]:
            raise ValueError('invalid baseline reply')
        lines.append(change['new'])
    if sha(('\n'.join(lines) + '\n').encode()) != OUTPUT or proposal['new_sha256'] != OUTPUT:
        raise ValueError('output differs from the audited exact live baseline')
    if (gate['mode'] != 'object-v1' or gate['phase'] != 'full' or not gate['decision'].startswith('PASS')
            or gate['failures'] or not gate['checks'] or any(c['status'] != 'pass' for c in gate['checks'])
            or gate['emission'] != GENERATED
            or gate['inputs']['oracle'] != proposal['oracle']['oracle_sha256']):
        raise ValueError('full eligible gate/emission does not bind this live oracle')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--approval', type=Path, required=True)
    p.add_argument('--gate', type=Path, required=True)
    p.add_argument('--run', required=True)
    a = p.parse_args()
    if (a.root / 'port/goldens' / f'{UNIT}.jsonl').exists():
        raise ValueError('initial baseline already exists; never replace a later golden')
    proposal = json.loads(a.approval.read_bytes())
    validate(proposal, (a.root / 'port/cases' / f'{UNIT}.jsonl').read_bytes(), json.loads(a.gate.read_bytes()))
    proposal['reason'] = (
        f'Initial 7460-case math baseline from live native run {a.run}, after the full object-mode '
        'gate, installed GCC13.3/glibc2.39 closure, reviewed matrix context and exact generated '
        'NaN-site emission passed on this same oracle. Debug/release live equality and refusal '
        'checks precede blessing. The selected reference uses baseline SSE2 libm dispatch and '
        'the measured libstdc++ stream behavior. All 3501 mixed-NaN cases and ten dispatch-sensitive '
        f'angles remain; complete output is pinned to SHA256 {OUTPUT}. The unchanged runner '
        'rechecks live equality, binary hashes and provenance during blessing. No tolerance, '
        'defined-input exclusion or fabricated output. This is a scoped partial-unit baseline, '
        'not universal reference adoption, other-locale/rounding-mode or second-host qualification.')
    a.approval.write_text(json.dumps(proposal, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
