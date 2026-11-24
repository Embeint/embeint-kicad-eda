# SPDX-License-Identifier: GPL-3.0-or-later
"""Malformed lock and emitter controls; these do not constitute reference acceptance."""
import copy
import json
import pathlib
import tempfile
import unittest

import objgate
import objschema


class ObjectSchemaTests(unittest.TestCase):
    def setUp(self):
        self.lock = json.loads((objgate.HERE / 'locks/object.lock.json').read_bytes())
        self.digest = self.lock['emission']['generated_rs_sha256']

    def test_genuine_pending_shape_and_review_transition(self):
        self.lock['status'] = 'DERIVED-PENDING-REVIEW'
        self.lock['context_review'].update(status='PENDING', reviewer=None)
        self.assertEqual(objschema.errors(self.lock, self.digest), [])
        # Synthetic shape only: no gate or review is passed by this fixture.
        self.lock['context_review'].update(status='REVIEWED', reviewer='test fixture')
        self.assertTrue(objschema.errors(self.lock, self.digest))
        self.lock['status'] = 'REVIEWED-EXPERIMENTAL-UNQUALIFIED'
        self.assertEqual(objschema.errors(self.lock, self.digest), [])

    def test_equal_floats_booleans_unknown_fields_and_missing_scope_reject(self):
        mutations = [
            lambda o: o['context']['inverse'].update(path_instructions=753.0),
            lambda o: o['context']['inverse'].update(path_instructions=True),
            lambda o: o['reference_layout'][0].update(start=float(o['reference_layout'][0]['start'])),
            lambda o: o['reference_layout'][0].update(unrecognized=1),
            lambda o: o['toolchain'].update(unrecognized=1),
            lambda o: o['context_review'].update(reviewer=False),
            lambda o: o['context_review'].update(reviewer=' '),
            lambda o: o['emission']['required_sites'].pop(),
            lambda o: o['emission'].update(generated_rs_sha256='0' * 64),
        ]
        for mutate in mutations:
            obj = copy.deepcopy(self.lock)
            mutate(obj)
            with self.subTest(lock=obj):
                self.assertTrue(objschema.errors(obj, self.digest))

    def test_original_recursive_schemas_are_in_the_object_inventory(self):
        elock = json.loads((objgate.HERE / 'locks/eligibility.lock.json').read_bytes())
        inv = objgate.inventory(elock, self.lock)
        elock['sites']['mul_vector3-doubleops']['consumer']['count'] = 3.0
        glock = json.loads((objgate.HERE / 'locks/graphs.lock.json').read_bytes())
        checks = objgate.gate.Checks()
        objgate.gate.schema(checks, elock, glock)
        self.assertTrue(checks.failures())
        self.assertTrue(all((r['category'], r['check']) in inv for r in checks.rows))


class ObjectEmissionTests(unittest.TestCase):
    def test_refusal_fails_run_and_preserves_accepted_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / 'generated.rs'
            path.write_bytes(b'prior accepted output')
            report = {'decision': 'PASS (synthetic unit fixture)', 'failures': []}
            objgate.accept_emission(report, 'candidate output', '0' * 64, '0' * 64, path)
            self.assertEqual(report['decision'], 'FAIL')
            self.assertTrue(report['failures'])
            self.assertEqual(path.read_bytes(), b'prior accepted output')
            self.assertEqual(list(path.parent.iterdir()), [path])

    def test_static_refuses_and_valid_full_fixture_replaces_atomically(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / 'generated.rs'
            report = {'decision': 'STATIC-PARTIAL', 'failures': []}
            digest = objgate.sha(b'candidate output')
            objgate.accept_emission(report, 'candidate output', digest, digest, path)
            self.assertFalse(path.exists())
            report['decision'] = 'PASS (synthetic writer fixture, not eligibility)'
            objgate.accept_emission(report, 'candidate output', digest, digest, path)
            self.assertEqual(path.read_bytes(), b'candidate output')
            self.assertEqual(report['emission'], digest)
            self.assertEqual(list(path.parent.iterdir()), [path])
