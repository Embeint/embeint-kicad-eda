# SPDX-License-Identifier: GPL-3.0-or-later
"""The documented baseline checksum command must work from a clean CI checkout."""
import hashlib
from pathlib import Path
import unittest

import objgate


class ValidationBundleTests(unittest.TestCase):
    def test_every_manifest_member_exists_with_its_recorded_bytes(self):
        root = objgate.HERE.parents[3]
        manifest = root / 'rust/kicad-math-shim/validation/2026-11-24-bb3262fc/SHA256SUMS'
        entries = manifest.read_text().splitlines()
        self.assertTrue(entries)
        names = set()
        for line in entries:
            digest, name = line.split('  ', 1)
            relative = Path(name)
            self.assertFalse(relative.is_absolute())
            self.assertNotIn('..', relative.parts)
            self.assertNotIn(name, names)
            names.add(name)
            self.assertEqual(hashlib.sha256((root / relative).read_bytes()).hexdigest(), digest, name)
