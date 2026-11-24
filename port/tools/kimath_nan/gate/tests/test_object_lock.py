# SPDX-License-Identifier: GPL-3.0-or-later
"""Binary-free checks of the object-mode lock and its pending review (no oracle, no objects).

These guard the checkpoint's shape and honesty, not eligibility: the lock is derived and
unreviewed, the review row cannot pass, the reviewed locks are unchanged, and no private path
is a dependency. Machine eligibility needs objgate.py on genuine inputs (OBJECT_MODE.md).
"""
import hashlib
import json
import pathlib
import unittest

import objgate

GATE = pathlib.Path(objgate.__file__).resolve().parent
ROOT = GATE.parents[3]
LOCKS = GATE / 'locks'


def sha(b):
    return hashlib.sha256(b).hexdigest()


class ObjectLockTests(unittest.TestCase):
    def setUp(self):
        self.olock = json.loads((LOCKS / 'object.lock.json').read_text(encoding='utf-8'))
        self.elock = json.loads((LOCKS / 'eligibility.lock.json').read_bytes())
        self.glock = json.loads((LOCKS / 'graphs.lock.json').read_bytes())

    def test_reviewed_locks_unchanged(self):
        self.assertEqual(sha((LOCKS / 'eligibility.lock.json').read_bytes()),
                         '011613b6103d109254e4cfd23f5ce9dbea00b64aa82c55828129b311008a64ac')
        self.assertEqual(sha((LOCKS / 'graphs.lock.json').read_bytes()),
                         '2e972c598899483db242e020eeb698c371859a59f875e4771431ade59f0f91a9')

    def test_object_lock_has_its_own_recorded_context_review(self):
        o = self.olock
        self.assertEqual(sorted(o), sorted(objgate.OBJECT_LOCK_FIELDS))
        self.assertEqual(o['mode'], objgate.MODE)
        self.assertEqual(o['status'], 'REVIEWED-EXPERIMENTAL-UNQUALIFIED')
        rv = o['context_review']
        self.assertEqual(rv['status'], 'REVIEWED')
        self.assertEqual(rv['reviewer'], 'Codex (parent object-context review, 2026-11-24)')
        review = (GATE / 'review/object-context-review.md').read_bytes()
        self.assertIn(sha(review), rv['basis'])
        self.assertEqual(rv['object_sha256'], o['home_object']['sha256'])
        link = {k: o['link'][k] for k in ('home_sections', 'provider_units', 'imports', 'externals')}
        self.assertEqual(rv['link_digest'], sha(objgate.canon(link)))

    def test_emission_binding(self):
        want = self.glock['generated_rs_sha256']
        self.assertEqual(self.olock['emission']['generated_rs_sha256'], want)
        self.assertEqual(sha((ROOT / 'rust' / 'kicad-math' / 'src' / 'generated.rs').read_bytes()), want)
        self.assertEqual(sorted(self.olock['emission']['required_sites']), sorted(objgate.contract.REQUIRED_SITES))
        self.assertEqual(sorted(self.olock['context']), sorted(objgate.contract.REQUIRED_SITES))

    def test_inventory_has_the_review_row_and_no_duplicates(self):
        inv = objgate.inventory(self.elock, self.olock)
        self.assertEqual(len(inv), len(set(inv)))
        self.assertIn(('context', objgate.OBJECT_CONTEXT_REVIEW), inv)
        self.assertIn(('scope', objgate.OBJECT_INVENTORY), inv)

    def test_no_private_paths(self):
        for name in ('locks/object.lock.json', 'objcontrols.inputs.json', 'review/object-context-review.PENDING.md'):
            text = (GATE / name).read_text(encoding='utf-8')
            for marker in ('C:\\', 'C:/', '/home/', 'Users\\', '/mnt/'):
                self.assertNotIn(marker, text, name)

    def test_control_inputs_are_digest_bound(self):
        m = json.loads((GATE / 'objcontrols.inputs.json').read_text(encoding='utf-8'))
        self.assertTrue(m['files'])
        for rel, digest in m['files'].items():
            self.assertRegex(digest, r'^[0-9a-f]{64}$', rel)
        self.assertEqual(m['files']['fresh/oracle-build/CMakeFiles/kicad-oracle.dir/shims/kimath_math.cpp.o'],
                         self.olock['home_object']['sha256'])
        self.assertEqual(m['files']['reviewed/objects/kimath_math.cpp.o'], self.olock['home_object']['sha256'])
        self.assertEqual(m['files']['reviewed/kicad-oracle'], self.elock['oracle']['sha256'])


if __name__ == '__main__':
    unittest.main()
