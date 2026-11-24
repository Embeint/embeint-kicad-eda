# SPDX-License-Identifier: GPL-3.0-or-later
"""Capture controls use dummy build files, never a runtime or oracle substitute."""
import json
from pathlib import Path
import shlex
import tempfile
import unittest

import capture


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='capture fixture ')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.root = self.base / 'build root'
        self.root.mkdir()
        names = list(capture.SOURCE_FILES.values()) + ['oracle-build/kicad-oracle',
                 'oracle-build/config.h', 'oracle-build/oracle_provenance.h',
                 'oracle-build/CMakeFiles/kicad-oracle.dir/main.cpp.o',
                 'oracle-build/CMakeFiles/kicad-oracle.dir/shims/kimath_math.cpp.o',
                 'oracle-build/reader-generated/generated.h']
        for name in names:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('dummy capture fixture ' + name)
        self.identity = self.base / 'identity.json'
        self.identity.write_text('dummy capture identity')
        self.output = self.base / 'captured'
        objects = [self.root / name for name in names if name.endswith('.o')]
        self.command = ': && /usr/bin/g++ ' + ' '.join(shlex.quote(str(p)) for p in objects)
        self.command += ' -o kicad-oracle -lm && :\n'

    def test_actual_order_bytes_and_include_tree_are_captured(self):
        capture.capture(self.root, self.output, self.command, self.identity)
        record = json.loads((self.output / 'capture.json').read_text())
        self.assertEqual(record['provider_order'], ['main.cpp.o', 'kimath_math.cpp.o'])
        self.assertEqual((self.output / 'providers/link-order.txt').read_text(), 'main.cpp.o\nkimath_math.cpp.o\n')
        self.assertEqual((self.output / 'tree/oracle-build/config.h').read_bytes(),
                         (self.root / 'oracle-build/config.h').read_bytes())
        self.assertTrue(record['sha256'])

    def test_absent_ambiguous_or_response_file_link_order_rejects(self):
        for command in ('', self.command * 2, self.command.replace('-lm', '@objects.rsp'),
                        self.command.replace('kimath_math.cpp.o', 'other.cpp.o')):
            with self.subTest(command=command), self.assertRaises((ValueError, FileNotFoundError)):
                capture.capture(self.root, self.output, command, self.identity)
            self.assertFalse(self.output.exists())

    def test_linked_include_file_refuses_and_leaves_no_partial_capture(self):
        path = self.root / 'oracle-src/overlay/alias'
        path.symlink_to(self.identity)
        with self.assertRaises(ValueError):
            capture.capture(self.root, self.output, self.command, self.identity)
        self.assertFalse(self.output.exists())
        self.assertFalse(list(self.base.glob('.object-capture-*')))

    def test_existing_evidence_is_preserved(self):
        self.output.mkdir()
        keep = self.output / 'keep'
        keep.write_text('original evidence')
        with self.assertRaises(ValueError):
            capture.capture(self.root, self.output, self.command, self.identity)
        self.assertEqual(keep.read_text(), 'original evidence')
