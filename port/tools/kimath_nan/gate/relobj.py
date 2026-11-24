# SPDX-License-Identifier: GPL-3.0-or-later
"""Strict ELF64 x86-64 relocatable-object reader (standard library only).

Parses the section headers, the symbol table, every SHT_RELA section and COMDAT groups of
one compiler output object. Every malformed or unexpected structure raises ObjError; nothing
is guessed.
"""
import hashlib
import struct


class ObjError(Exception):
    pass


SHT_PROGBITS, SHT_SYMTAB, SHT_STRTAB, SHT_RELA, SHT_NOBITS, SHT_GROUP = 1, 2, 3, 4, 8, 17
SHF_WRITE, SHF_ALLOC, SHF_EXECINSTR, SHF_MERGE, SHF_STRINGS, SHF_GROUP = 0x1, 0x2, 0x4, 0x10, 0x20, 0x200
STB_LOCAL, STB_GLOBAL, STB_WEAK = 0, 1, 2
STT_NOTYPE, STT_OBJECT, STT_FUNC, STT_SECTION, STT_FILE = 0, 1, 2, 3, 4
SHN_UNDEF, SHN_ABS, SHN_COMMON = 0, 0xFFF1, 0xFFF2
# x86-64 relocation types this reader accepts; anything else is refused by the link proof.
R_NAMES = {1: 'R_X86_64_64', 2: 'R_X86_64_PC32', 4: 'R_X86_64_PLT32', 9: 'R_X86_64_GOTPCREL',
           41: 'R_X86_64_GOTPCRELX', 42: 'R_X86_64_REX_GOTPCRELX'}


class RelObj:
    def __init__(self, data):
        self.data = data
        self.sha256 = hashlib.sha256(data).hexdigest()
        if data[:4] != b'\x7fELF' or data[4:7] != b'\x02\x01\x01':
            raise ObjError('not an ELF64 LSB version-1 file')
        (e_type, e_machine, _v, _entry, _phoff, shoff, _flags, _ehsize, _phentsize, phnum, shentsize,
         shnum, shstrndx) = struct.unpack_from('<HHIQQQIHHHHHH', data, 16)
        if (e_type, e_machine) != (1, 62) or phnum != 0:
            raise ObjError(f'not an x86-64 relocatable object (type {e_type}, machine {e_machine})')
        if shentsize != 64 or not 0 < shstrndx < shnum or shoff + 64 * shnum > len(data):
            raise ObjError('bad section header table')
        raw = [struct.unpack_from('<IIQQQQIIQQ', data, shoff + 64 * i) for i in range(shnum)]
        names = raw[shstrndx]
        self.sections = []
        for i, (name, typ, flags, addr, off, size, link, info, align, entsize) in enumerate(raw):
            if typ != SHT_NOBITS and off + size > len(data):
                raise ObjError(f'section {i} outside the file')
            if addr != 0:
                raise ObjError(f'section {i} has a non-zero address in a relocatable object')
            self.sections.append({'index': i, 'name': self._str(names[4], name), 'type': typ, 'flags': flags,
                                  'offset': off, 'size': size, 'link': link, 'info': info, 'align': align,
                                  'entsize': entsize})
        symtabs = [s for s in self.sections if s['type'] == SHT_SYMTAB]
        if len(symtabs) != 1 or symtabs[0]['entsize'] != 24:
            raise ObjError('expected exactly one symbol table')
        st = symtabs[0]
        strtab = self.sections[st['link']]
        self.symbols = []
        for i in range(st['size'] // 24):
            name, info, other, shndx, value, size = struct.unpack_from('<IBBHQQ', data, st['offset'] + 24 * i)
            sym = {'index': i, 'name': self._str(strtab['offset'], name), 'bind': info >> 4, 'type': info & 0xF,
                   'other': other, 'shndx': shndx, 'value': value, 'size': size}
            if sym['type'] == STT_SECTION and shndx < len(self.sections):
                sym['name'] = sym['name'] or self.sections[shndx]['name']
            self.symbols.append(sym)
        self.relocs = {}
        for s in self.sections:
            if s['type'] != SHT_RELA:
                continue
            if s['entsize'] != 24 or s['link'] != st['index'] or s['size'] % 24:
                raise ObjError(f'bad relocation section {s["name"]}')
            target = s['info']
            out = []
            for k in range(s['size'] // 24):
                off, info, addend = struct.unpack_from('<QQq', data, s['offset'] + 24 * k)
                typ, symi = info & 0xFFFFFFFF, info >> 32
                if symi >= len(self.symbols):
                    raise ObjError(f'{s["name"]}: relocation symbol index out of range')
                if off + (8 if typ == 1 else 4) > self.sections[target]['size']:
                    raise ObjError(f'{s["name"]}: relocation outside its section')
                out.append({'offset': off, 'type': typ, 'symbol': symi, 'addend': addend})
            if target in self.relocs:
                raise ObjError(f'two relocation sections for section {target}')
            self.relocs[target] = sorted(out, key=lambda r: r['offset'])
        self.groups = {}
        for s in self.sections:
            if s['type'] == SHT_GROUP:
                words = struct.unpack_from(f'<{s["size"] // 4}I', data, s['offset'])
                sig = self.symbols[s['info']]['name']
                self.groups[s['index']] = {'signature': sig, 'flags': words[0], 'members': list(words[1:])}

    def _str(self, off, idx):
        end = self.data.index(b'\0', off + idx)
        return self.data[off + idx:end].decode('ascii')

    def section_bytes(self, index):
        s = self.sections[index]
        if s['type'] == SHT_NOBITS:
            return bytes(s['size'])
        return self.data[s['offset']:s['offset'] + s['size']]

    def group_of(self, index):
        hits = [g for g in self.groups.values() if index in g['members']]
        if len(hits) > 1:
            raise ObjError(f'section {index} in several groups')
        return hits[0] if hits else None

    def comment(self):
        s = [x for x in self.sections if x['name'] == '.comment']
        if len(s) != 1:
            return None
        return sorted({p.decode(errors='replace') for p in self.section_bytes(s[0]['index']).split(b'\0') if p})
