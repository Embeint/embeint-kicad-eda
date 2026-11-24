#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Read libm's ELF facts for the dispatch contract (standard library only).

One classifier names libm's multiarch implementations for both the lock
generator (tools/lock_inputs.py, from the snapshot .deb) and the runtime
collector (collect_environment.py, from the installed debug file), so the two
cannot disagree. dispatch_facts() records the static half of the dispatch
chain proof that run_probe.dispatch_report() checks against a runtime trace:
public symbol versions, the PLT stubs and their GOT relocations, IFUNC
resolvers and every function symbol's address range.
"""

import hashlib
import re
import struct

SHT_SYMTAB = 2
SHT_RELA = 4
SHT_DYNSYM = 11
SHT_GNU_VERDEF = 0x6FFFFFFD
SHT_GNU_VERSYM = 0x6FFFFFFF
STT_FUNC = 2
STT_GNU_IFUNC = 10
SHN_UNDEF = 0
SHF_EXECINSTR = 4
SYMBOL_TYPES = {0: "NOTYPE", 1: "OBJECT", 2: "FUNC", 10: "IFUNC"}
RELOCATION_TYPES = {6: "R_X86_64_GLOB_DAT", 7: "R_X86_64_JUMP_SLOT", 8: "R_X86_64_RELATIVE",
                    37: "R_X86_64_IRELATIVE"}
R_X86_64_IRELATIVE = 37

# A multiarch implementation is a defined FUNC symbol named __<stem>_<isa>
# with a non-empty stem. An IFUNC symbol is a dispatcher (its value is the
# resolver), and a bare name such as __fma is the public function itself.
VARIANT_ISAS = ("sse2", "sse41", "sse4_1", "avx", "avx2", "fma", "fma4", "avx512", "evex")
VARIANT = re.compile(r"^__.+_(" + "|".join(VARIANT_ISAS) + r")$")

# IBT .plt.sec entry as the locked libm has it (binutils 2.42, no BND prefix):
# endbr64; jmp *rel32(%rip); nopw 0x0(%rax,%rax,1). Only this exact 16-byte
# form is accepted; the GOT slot is the jump's memory operand. Any other form
# (BND, lazy .plt, different padding) leaves the stub unrecognised.
PLT_SEC_PREFIX = bytes.fromhex("f30f1efaff25")
PLT_SEC_SUFFIX = bytes.fromhex("660f1f440000")
PLT_SEC_ENTRY = 16
PLT_SEC_JUMP = 4  # offset of the indirect jump inside the entry
PLT_SEC_JUMP_END = 10  # the rel32 is relative to the end of the jump


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
    """Minimal little-endian ELF64 reader: sections, symbols, versions, relocations."""

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

    def sections_of_type(self, kind):
        return [fields for fields in self.sections if fields[1] == kind]

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

    def version_definitions(self):
        """Map version index -> name from .gnu.version_d."""
        sections = self.sections_of_type(SHT_GNU_VERDEF)
        if not sections:
            return {}
        verdef = sections[0]
        strtab = self.sections[verdef[6]]
        names = {}
        offset = verdef[4]
        for _ in range(verdef[7]):
            _version, _flags, index, count, _hash, aux, following = struct.unpack_from("<HHHHIII", self.data,
                                                                                        offset)
            if count:
                (name,) = struct.unpack_from("<I", self.data, offset + aux)
                names[index] = self._string(strtab[4], name)
            if not following:
                break
            offset += following
        return names

    def dynamic_definitions(self):
        """Yield defined .dynsym symbols with their version name and default flag."""
        versym = self.sections_of_type(SHT_GNU_VERSYM)
        versions = self.version_definitions()
        for symbol in self.symbols(".dynsym"):
            if symbol["shndx"] == SHN_UNDEF:
                continue
            version, default = None, True
            if versym:
                (raw,) = struct.unpack_from("<H", self.data, versym[0][4] + 2 * symbol["index"])
                default = not raw & 0x8000
                version = versions.get(raw & 0x7FFF)
            yield {**symbol, "version": version, "default": default}

    def relocations(self):
        """Yield (r_offset, type, symbol index, addend) from every RELA section."""
        for section in self.sections_of_type(SHT_RELA):
            entsize = section[9] or 24
            for offset in range(section[4], section[4] + section[5], entsize):
                r_offset, info, addend = struct.unpack_from("<QQq", self.data, offset)
                yield r_offset, info & 0xFFFFFFFF, info >> 32, addend

    def code(self, address, maximum=15):
        """Up to maximum bytes from address inside one executable PROGBITS section."""
        for fields in self.sections:
            start, length = fields[3], fields[5]
            if fields[1] == 1 and fields[2] & SHF_EXECINSTR and start <= address < start + length:
                offset = fields[4] + address - start
                return self.data[offset:offset + min(maximum, start + length - address)]
        raise ValueError(f"Address {address:#x} is not in executable code")

    def read(self, address, size):
        """Bytes at a virtual address inside one PROGBITS section."""
        for fields in self.sections:
            start, length = fields[3], fields[5]
            if fields[1] == 1 and start <= address and address + size <= start + length:
                offset = fields[4] + address - start
                return self.data[offset:offset + size]
        raise ValueError(f"Address {address:#x} is not inside one file-backed section")


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


def plt_sec_stubs(libm):
    """Every .plt.sec entry: its exact-template status and the GOT slot it jumps through."""
    section = libm.section(".plt.sec")
    if section is None:
        return None, []
    start, size = section[3], section[5]
    stubs = []
    for stub in range(start, start + size - size % PLT_SEC_ENTRY, PLT_SEC_ENTRY):
        code = libm.read(stub, PLT_SEC_ENTRY)
        template = code[:6] == PLT_SEC_PREFIX and code[PLT_SEC_JUMP_END:] == PLT_SEC_SUFFIX
        slot = None
        if template:
            (displacement,) = struct.unpack_from("<i", code, 6)
            slot = hex_offset(stub + PLT_SEC_JUMP_END + displacement)
        stubs.append({"stub": hex_offset(stub), "template": template, "slot": slot, "bytes": code.hex()})
    return {"start": hex_offset(start), "end": hex_offset(start + size)}, stubs


def dispatch_facts(libm_bytes, debug_bytes, functions):
    """Static facts of libm's dispatch chain for the named public functions."""
    libm = Elf64(libm_bytes)
    debug = Elf64(debug_bytes)
    definitions = {name: [] for name in functions}
    for symbol in libm.dynamic_definitions():
        if symbol["name"] in definitions:
            definitions[symbol["name"]].append({
                "value": hex_offset(symbol["value"]), "size": symbol["size"],
                "type": SYMBOL_TYPES.get(symbol["type"], str(symbol["type"])),
                "version": symbol["version"], "default": symbol["default"]})
    for rows in definitions.values():
        rows.sort(key=lambda r: (int(r["value"], 16), r["version"] or ""))
    relocations = {}
    for r_offset, kind, symbol, addend in libm.relocations():
        relocations.setdefault(r_offset, []).append({
            "type": RELOCATION_TYPES.get(kind, str(kind)), "symbol": symbol, "addend": hex_offset(addend)
            if addend >= 0 else "-" + hex_offset(-addend)})
    plt_sec, stubs = plt_sec_stubs(libm)
    for stub in stubs:
        stub["relocations"] = relocations.get(int(stub["slot"], 16), []) if stub["slot"] else []
    ifuncs = {}
    ranges = {}
    for symbol in debug.symbols():
        if symbol["shndx"] == SHN_UNDEF:
            continue
        if symbol["type"] == STT_GNU_IFUNC:
            ifuncs.setdefault(hex_offset(symbol["value"]), set()).add(symbol["name"])
        elif symbol["type"] == STT_FUNC and symbol["size"]:
            ranges.setdefault((symbol["value"], symbol["size"]), set()).add(symbol["name"])
    variants, excluded = multiarch_symbols(debug)
    return {
        "schema": 1,
        "libm_sha256": hashlib.sha256(libm_bytes).hexdigest(),
        "libm_build_id": libm.build_id(),
        "debug_sha256": hashlib.sha256(debug_bytes).hexdigest(),
        "debug_build_id": debug.build_id(),
        "definitions": definitions,
        "plt_sec": plt_sec,
        "plt_stubs": stubs,
        "ifuncs": {offset: sorted(names) for offset, names in sorted(ifuncs.items(), key=lambda i: int(i[0], 16))},
        "functions": [[hex_offset(value), size, sorted(names)] for (value, size), names in sorted(ranges.items())],
        "multiarch_symbols": variants,
        "excluded_suffix_matches": excluded,
    }
