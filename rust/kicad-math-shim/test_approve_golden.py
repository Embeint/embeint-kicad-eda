# SPDX-License-Identifier: GPL-3.0-or-later
"""Small synthetic authorization controls; no golden or eligibility is produced."""
import copy
import json
import unittest
from unittest.mock import patch

import approve_golden as a


class ApprovalTests(unittest.TestCase):
    def setUp(self):
        self.requests = b'{"case":"a"}\n{"case":"b"}\n'
        lines = [json.dumps({'unit': a.UNIT, 'case': c, 'result': {'bits': str(i)}})
                 for i, c in enumerate(('a', 'b'))]
        self.proposal = {'format': 1, 'unit': a.UNIT, 'old_sha256': a.EMPTY,
                         'cases_sha256': a.sha(self.requests),
                         'new_sha256': a.sha(('\n'.join(lines) + '\n').encode()),
                         'changes': [{'case': c, 'old_index': None, 'old': None,
                                      'new_index': i, 'new': line}
                                     for i, (c, line) in enumerate(zip(('a', 'b'), lines))],
                         'oracle': {'oracle_sha256': '1' * 64}}
        self.gate = {'mode': 'object-v1', 'phase': 'full', 'decision': 'PASS (synthetic)',
                     'failures': [], 'checks': [{'status': 'pass'}], 'emission': a.GENERATED,
                     'inputs': {'oracle': '1' * 64}}
        self.patch = patch.multiple(a, CASES=a.sha(self.requests), COUNT=2,
                                   OUTPUT=self.proposal['new_sha256'])
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def test_exact_synthetic_initial_additions_accept(self):
        a.validate(self.proposal, self.requests, self.gate)

    def test_changed_answer_rejects_even_if_proposal_hash_is_changed(self):
        self.proposal['changes'][0]['new'] = self.proposal['changes'][0]['new'].replace('"0"', '"9"')
        self.proposal['new_sha256'] = a.sha(('\n'.join(c['new'] for c in self.proposal['changes']) + '\n').encode())
        with self.assertRaises(ValueError):
            a.validate(self.proposal, self.requests, self.gate)

    def test_missing_duplicate_moved_and_noninteger_indices_reject(self):
        changes = self.proposal['changes']
        for replacement in [changes[:1], [changes[0], changes[0]], list(reversed(changes)),
                            [dict(changes[0], new_index=0.0), changes[1]],
                            [dict(changes[0], old='invented'), changes[1]]]:
            p = dict(self.proposal, changes=replacement)
            with self.assertRaises(ValueError):
                a.validate(p, self.requests, self.gate)

    def test_gate_must_be_full_successful_emitted_and_same_oracle(self):
        for field, value in [('phase', 'static'), ('decision', 'FAIL'), ('failures', ['x']),
                             ('checks', [{'status': 'not-run'}]), ('checks', []),
                             ('emission', '0' * 64), ('inputs', {'oracle': '2' * 64})]:
            gate = dict(self.gate, **{field: value})
            with self.assertRaises(ValueError):
                a.validate(self.proposal, self.requests, gate)

    def test_corpus_and_existing_baseline_drift_reject(self):
        with self.assertRaises(ValueError):
            a.validate(self.proposal, self.requests + b'\n', self.gate)
        p = copy.deepcopy(self.proposal)
        p['old_sha256'] = '3' * 64
        with self.assertRaises(ValueError):
            a.validate(p, self.requests, self.gate)


if __name__ == '__main__':
    unittest.main()
