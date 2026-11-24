# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic loader-write controls; no ELF or native code is executed."""
import struct
import types
import unittest

from elfbin import Elf, ElfError
from elfexec import ExecError, reject_overlapping_writes, relocation_writes

GOT = 0x2000


def fixture(plt, dynamic=(), copy_size=32):
    data = bytearray(0x300)
    data[:16] = bytes.fromhex('f30f1efaff25') + struct.pack('<i', GOT - 0x100a) + bytes.fromhex('660f1f440000')
    for offset, records in ((0x40, plt), (0xc0, dynamic)):
        for i, (place, typ, sym) in enumerate(records):
            struct.pack_into('<QQq', data, offset + i * 24, place, (sym << 32) | typ, 0)
    strings = b'\0strlen\0sincos\0copy\0'
    data[0x200:0x200 + len(strings)] = strings
    for i, name in enumerate((0, 1, 8, 15)):
        struct.pack_into('<IBBHQQ', data, 0x180 + i * 24, name, 0, 0, 0, 0, copy_size if i == 3 else 0)
    sections = [
        {'name': '.plt.sec', 'offset': 0, 'addr': 0x1000, 'size': 16, 'flags': 6, 'type': 1},
        {'name': '.rela.plt', 'offset': 0x40, 'size': len(plt) * 24, 'type': 4, 'entsize': 24},
        {'name': '.rela.dyn', 'offset': 0xc0, 'size': len(dynamic) * 24, 'type': 4, 'entsize': 24},
        {'name': '.dynsym', 'offset': 0x180, 'size': 96, 'link': 4},
        {'name': '.dynstr', 'offset': 0x200, 'size': len(strings)},
    ]
    for s in sections:
        s.setdefault('flags', 0)
        s.setdefault('type', 1)
        s.setdefault('addr', 0)
    elf = Elf.__new__(Elf)
    elf.data, elf.sections = bytes(data), sections
    raw = types.SimpleNamespace(data=elf.data, by={s['name']: s for s in sections})
    return elf, raw


class RelocationWriteTests(unittest.TestCase):
    def test_unique_slot_and_adjacent_writes_are_valid(self):
        elf, raw = fixture([(GOT, 7, 1), (GOT + 8, 7, 2)], [(GOT - 32, 5, 3)])
        self.assertEqual(elf.plt_import(0x1000)['import'], 'strlen')
        reject_overlapping_writes(relocation_writes(raw))

    def test_duplicate_plt_writes_reject_even_when_symbols_match(self):
        for symbol in (1, 2):
            elf, raw = fixture([(GOT, 7, 1), (GOT, 7, symbol)])
            with self.assertRaises(ElfError):
                elf.plt_import(0x1000)
            with self.assertRaises(ExecError):
                reject_overlapping_writes(relocation_writes(raw))

    def test_cross_table_exact_and_partial_overlaps_reject(self):
        for place in (GOT - 7, GOT, GOT + 7):
            elf, raw = fixture([(GOT, 7, 1)], [(place, 6, 2)])
            with self.assertRaises(ElfError):
                elf.plt_import(0x1000)
            with self.assertRaises(ExecError):
                reject_overlapping_writes(relocation_writes(raw))

    def test_copy_extent_includes_all_symbol_bytes(self):
        elf, raw = fixture([(GOT, 7, 1)], [(GOT - 16, 5, 3)])
        writes = relocation_writes(raw)
        self.assertIn((GOT - 16, GOT + 16, '.rela.dyn', 0), writes)
        with self.assertRaises(ElfError):
            elf.plt_import(0x1000)
        with self.assertRaises(ExecError):
            reject_overlapping_writes(writes)

    def test_invalid_copy_size_symbol_and_table_shape_reject(self):
        for kwargs in ({'dynamic': [(GOT, 5, 3)], 'copy_size': 0},
                       {'dynamic': [(GOT, 6, 99)]}):
            elf, raw = fixture([(GOT, 7, 1)], **kwargs)
            with self.assertRaises(ElfError):
                elf.plt_import(0x1000)
            with self.assertRaises(ExecError):
                relocation_writes(raw)
        elf, raw = fixture([(GOT, 7, 1)])
        raw.by['.rela.plt']['size'] += 1
        with self.assertRaises(ExecError):
            relocation_writes(raw)
        with self.assertRaises(ElfError):
            elf.plt_import(0x1000)


if __name__ == '__main__':
    unittest.main()
