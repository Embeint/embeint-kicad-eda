# SPDX-License-Identifier: GPL-3.0-or-later
"""Binary-free checks of the execution-mapping extension (elfexec.py); the 30 mapping controls on
the genuine oracles are elfexec_controls.py and need the external inputs."""
import json
import copy
import pathlib
import struct
import unittest
import types

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

    def test_recursive_extension_schema_rejects_equal_floats_and_unknown_fields(self):
        for path in [('header', 'machine'), ('dynamic_flags', 'FLAGS'),
                     ('program_headers', 0, 0), ('section_segments', '.text'),
                     ('relocation_types', '.rela.dyn', 0)]:
            e = copy.deepcopy(self.ext)
            parent = e
            for key in path[:-1]:
                parent = parent[key]
            parent[path[-1]] = float(parent[path[-1]])
            self.assertFalse(elfexec.ext_row(e)[1], path)
        e = copy.deepcopy(self.ext)
        e['header']['unknown'] = 1
        self.assertFalse(elfexec.ext_row(e)[1])
        self.assertFalse(elfexec.ext_row(dict(self.ext, import_versions={}))[1])

    def test_version_auxiliary_count_must_match_the_chain(self):
        data = bytearray(96)
        struct.pack_into('<HHIII', data, 0, 1, 1, 0, 16, 0)
        struct.pack_into('<IHHII', data, 16, elfexec.elf_hash('VER'), 0, 2, 4, 0)
        data[64:72] = b'lib\0VER\0'
        raw = types.SimpleNamespace(data=bytes(data), by={
            '.gnu.version_r': {'addr': 0, 'offset': 0, 'size': 32},
            '.dynstr': {'offset': 64, 'size': 8}})
        self.assertEqual(len(elfexec.parse_verneed(raw, 0)[0][1]), 1)
        for offset, fmt, value in [(2, '<H', 2), (28, '<I', 16), (24, '<I', 8)]:
            mutant = bytearray(data)
            struct.pack_into(fmt, mutant, offset, value)
            raw.data = bytes(mutant)
            with self.assertRaises((elfexec.ExecError, ValueError)):
                elfexec.parse_verneed(raw, 0)


if __name__ == '__main__':
    unittest.main()
