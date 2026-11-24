# SPDX-License-Identifier: GPL-3.0-or-later
"""Binary-free checks of the execution-mapping extension (elfexec.py); the 30 mapping controls on
the genuine oracles are elfexec_controls.py and need the external inputs."""
import json
import pathlib
import struct
import unittest

import elfexec
import objgate

LOCKS = pathlib.Path(elfexec.__file__).resolve().parent / 'locks'


class ElfExecTests(unittest.TestCase):
    def setUp(self):
        self.ext = json.loads((LOCKS / 'object-elfexec.lock.json').read_text(encoding='utf-8'))
        self.imports = json.loads((LOCKS / 'object.lock.json').read_text(encoding='utf-8'))['link']['imports']

    def test_extension_is_derived_and_pending(self):
        self.assertEqual(sorted(self.ext), sorted(elfexec.EXT_FIELDS))
        self.assertEqual(self.ext['mode'], elfexec.MODE)
        self.assertEqual(self.ext['status'], 'DERIVED-PENDING-REVIEW')
        self.assertNotIn('reviewer', self.ext)
        self.assertEqual(self.ext['derived_from'], '0bac756736379961a9c3fa34ae2d549615231561650d6d36af6b05a403c1b4db')
        self.assertEqual(sorted(self.ext['import_versions']), sorted(self.imports))

    def test_rows_are_in_the_object_inventory(self):
        elock = json.loads((LOCKS / 'eligibility.lock.json').read_bytes())
        olock = json.loads((LOCKS / 'object.lock.json').read_text(encoding='utf-8'))
        inv = objgate.inventory(elock, olock)
        for name in elfexec.ROWS:
            self.assertIn(('eligibility', name), inv)
        self.assertEqual(len(inv), len(set(inv)))

    def test_non_elf_and_headerless_inputs_fail_every_row(self):
        for data in (b'', b'not an elf' * 10):
            rows = elfexec.check(data, self.ext, self.imports)
            self.assertEqual([r[0] for r in rows], elfexec.ROWS)
            self.assertTrue(all(not ok for name, ok, det in rows[:-1]))
        # a 64-byte ELF64 header claiming no program or section headers
        hdr = bytearray(64)
        hdr[:16] = b'\x7fELF\x02\x01\x01\x03' + bytes(8)
        struct.pack_into('<HHIQQQIHHHHHH', hdr, 16, 3, 62, 1, 0, 0, 0, 0, 64, 56, 0, 64, 0, 0)
        rows = elfexec.check(bytes(hdr), self.ext, self.imports)
        self.assertTrue(all(not ok for name, ok, det in rows[:-1]))

    def test_extension_row_rejects_missing_or_altered_status(self):
        self.assertFalse(elfexec.ext_row(None)[1])
        self.assertFalse(elfexec.ext_row(dict(self.ext, status='REVIEWED'))[1])
        self.assertFalse(elfexec.ext_row({k: v for k, v in self.ext.items() if k != 'needed'})[1])
        self.assertTrue(elfexec.ext_row(self.ext)[1])

    def test_elf_hash(self):
        self.assertEqual(elfexec.elf_hash('GLIBC_2.2.5'), 0x09691A75)


if __name__ == '__main__':
    unittest.main()
