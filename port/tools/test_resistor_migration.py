#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Authorization regression controls, separate from the live comparison."""
import copy
import hashlib
import json
import unittest
from unittest.mock import patch

import migrate_resistor_reference as migration


class ReplacementTest(unittest.TestCase):
    def setUp(self):
        self.old = json.dumps({'case': 'a', 'result': {'125': 10, '126': 20, 'format': 'old'}})
        self.new = json.dumps({'case': 'a', 'result': {'125': 10, '126': 21, 'format': 'new'}})
        self.old_text = self.old+'\n'
        self.new_text = self.new+'\n'
        digest = lambda text: hashlib.sha256(text.encode()).hexdigest()
        self.old_patch = patch.object(migration, 'OLD_SHA256', digest(self.old_text))
        self.new_patch = patch.object(migration, 'NEW_SHA256', digest(self.new_text))
        self.old_patch.start()
        self.new_patch.start()
        self.addCleanup(self.old_patch.stop)
        self.addCleanup(self.new_patch.stop)
        self.proposal = {'new_sha256': digest(self.new_text), 'changes': [
            {'old_index': 0, 'new_index': 0, 'old': self.old, 'new': self.new}]}

    def test_exact_authorized_replacement_passes(self):
        migration.check_replacement(self.proposal, self.old_text)

    def test_counts_and_forged_hash_cannot_authorize_other_values_or_paths(self):
        for result in [
            {'125': 11, '126': 20, 'format': 'new'},
            {'125': 10, '126': -21, 'format': 'new'},
            {'125': 10, '126': 21, 'format': 'UNAUTHORIZED'},
        ]:
            for rehash in (False, True):
                proposal = copy.deepcopy(self.proposal)
                line = json.dumps({'case': 'a', 'result': result})
                proposal['changes'][0]['new'] = line
                if rehash:
                    proposal['new_sha256'] = hashlib.sha256((line+'\n').encode()).hexdigest()
                with self.subTest(result=result, rehash=rehash):
                    with self.assertRaises(ValueError):
                        migration.check_replacement(proposal, self.old_text)

    def test_original_lines_and_positions_must_match_actual_file(self):
        for key, value in [('old', 'forged'), ('old_index', False), ('new_index', False),
                           ('old_index', -1), ('old_index', 1), ('new', self.new+'\n')]:
            proposal = copy.deepcopy(self.proposal)
            proposal['changes'][0][key] = value
            with self.subTest(key=key, value=value):
                with self.assertRaises(ValueError):
                    migration.check_replacement(proposal, self.old_text)

    def test_metadata_digest_cannot_hide_a_different_original_file(self):
        with self.assertRaises(ValueError):
            migration.check_replacement(self.proposal, self.old_text+'\n')


if __name__ == '__main__':
    unittest.main()
