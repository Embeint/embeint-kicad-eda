#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Read libm's ELF symbols for the dispatch contract (standard library only).

One classifier names libm's multiarch implementations for both the lock
generator (tools/lock_inputs.py, from the snapshot .deb) and the runtime
collector (collect_environment.py, from the installed debug file), so the two
cannot disagree.
"""

import re
import struct

STT_FUNC = 2
STT_GNU_IFUNC = 10
SHN_UNDEF = 0
SYMBOL_TYPES = {0: "NOTYPE", 1: "OBJECT", 2: "FUNC", 10: "IFUNC"}

# A multiarch implementation is a defined FUNC symbol named __<stem>_<isa>
# with a non-empty stem. An IFUNC symbol is a dispatcher (its value is the
# resolver), and a bare name such as __fma is the public function itself.
VARIANT_ISAS = ("sse2", "sse41", "sse4_1", "avx", "avx2", "fma", "fma4", "avx512", "evex")
VARIANT = re.compile(r"^__.+_(" + "|".join(VARIANT_ISAS) + r")$")


def hex_offset(value):
    return f"0x{value:x}"


def classify(name, kind, shndx=1):
    """Return (is_variant, reason) for one symbol.

    reason is None for a variant and for names without an ISA suffix; otherwise
    it says why a name ending in an ISA suffix is not an implementation.
    """
    if not (name.startswith("__") and name.endswith(tuple("_" + isa for isa in VARIANT_ISAS))):
        return False, None
    reasons = []
    if not VARIANT.match(name):
        reasons.append("bare public name without a function stem")
    if shndx == SHN_UNDEF:
        reasons.append("undefined symbol")
    if kind == STT_GNU_IFUNC:
        reasons.append("IFUNC dispatcher symbol (its value is the resolver)")
    elif kind != STT_FUNC:
        reasons.append("not a function symbol")
    if reasons:
        return False, "; ".join(reasons) + ": not a multiarch implementation"
    return True, None


class Elf64:
    """Minimal little-endian ELF64 reader: sections, symbols and the build ID."""

    def __init__(self, data):
        if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
            raise ValueError("Not a little-endian ELF64 file")
        self.data = data
        (self.shoff,) = struct.unpack_from("<Q", data, 0x28)
        self.shentsize, self.shnum, self.shstrndx = struct.unpack_from("<HHH", data, 0x3A)
        self.sections = []
        for index in range(self.shnum):
            fields = struct.unpack_from("<IIQQQQIIQQ", data, self.shoff + index * self.shentsize)
            self.sections.append(fields)
        names = self.sections[self.shstrndx]
        self.names = [self._string(names[4], s[0]) for s in self.sections]

    def _string(self, offset, index):
        end = self.data.index(b"\0", offset + index)
        return self.data[offset + index:end].decode()

    def section(self, name):
        for section_name, fields in zip(self.names, self.sections):
            if section_name == name:
                return fields
        return None

    def build_id(self):
        note = self.section(".note.gnu.build-id")
        if note is None:
            return None
        offset = note[4]
        namesz, descsz, kind = struct.unpack_from("<III", self.data, offset)
        if kind != 3:
            raise ValueError("Unexpected build-id note type")
        start = offset + 12 + ((namesz + 3) & ~3)
        return self.data[start:start + descsz].hex()

    def symbols(self, table=".symtab"):
        """Yield dicts (index name value size type bind shndx) for every named symbol."""
        symtab = self.section(table)
        if symtab is None:
            return
        strtab = self.sections[symtab[6]]
        entsize = symtab[9] or 24
        for index, offset in enumerate(range(symtab[4], symtab[4] + symtab[5], entsize)):
            name_index, info, _other, shndx, value, size = struct.unpack_from("<IBBHQQ", self.data, offset)
            if not name_index:
                continue
            yield {"index": index, "name": self._string(strtab[4], name_index), "value": value,
                   "size": size, "type": info & 0xF, "bind": info >> 4, "shndx": shndx}

    def function_symbols(self):
        """Yield (name, value, binding) for FUNC/IFUNC symbols in .symtab."""
        for symbol in self.symbols():
            if symbol["type"] in (STT_FUNC, STT_GNU_IFUNC):
                yield symbol["name"], symbol["value"], symbol["bind"]


def multiarch_symbols(elf):
    """Return (table offset -> sorted names, excluded suffix matches) from .symtab."""
    table = {}
    excluded = {}
    for symbol in elf.symbols():
        variant, reason = classify(symbol["name"], symbol["type"], symbol["shndx"])
        if variant:
            table.setdefault(hex_offset(symbol["value"]), set()).add(symbol["name"])
        elif reason is not None:
            key = (symbol["name"], hex_offset(symbol["value"]), SYMBOL_TYPES.get(symbol["type"],
                                                                                str(symbol["type"])), reason)
            excluded[key] = None
    ordered = {offset: sorted(names) for offset, names in sorted(table.items(), key=lambda i: int(i[0], 16))}
    return ordered, [dict(zip(("name", "offset", "type", "reason"), key)) for key in sorted(excluded)]

