"""Minimal strict ELF64 little-endian reader (private prototype tool, no third-party modules).

Reads the actual bytes of the eligible binary: header, sections, symbols (.symtab), notes
(build ID, GNU property, ABI tag) and data at virtual addresses. Every malformed or unexpected
structure raises ElfError; nothing is guessed.
"""
import hashlib
import struct


class ElfError(Exception):
    pass


SHT_SYMTAB, SHT_NOTE, SHT_NOBITS = 2, 7, 8
SHF_WRITE, SHF_ALLOC, SHF_EXECINSTR = 1, 2, 4
STT_FUNC = 2


class Elf:
    def __init__(self, data):
        self.data = data
        self.sha256 = hashlib.sha256(data).hexdigest()
        if data[:4] != b'\x7fELF':
            raise ElfError('not an ELF file')
        ei_class, ei_data, ei_version, ei_osabi = data[4], data[5], data[6], data[7]
        if (ei_class, ei_data, ei_version) != (2, 1, 1):
            raise ElfError(f'unsupported ELF ident class={ei_class} data={ei_data} version={ei_version}')
        (self.e_type, self.e_machine, _v, self.e_entry, _phoff, self.e_shoff, _flags, _ehsize,
         _phentsize, _phnum, shentsize, shnum, shstrndx) = struct.unpack_from('<HHIQQQIHHHHHH', data, 16)
        self.osabi = ei_osabi
        if shentsize != 64 or shnum == 0 or shstrndx >= shnum:
            raise ElfError('bad section header table')
        raw = [struct.unpack_from('<IIQQQQIIQQ', data, self.e_shoff + i * 64) for i in range(shnum)]
        names = raw[shstrndx]
        self.sections = []
        for (name, typ, flags, addr, off, size, link, info, align, entsize) in raw:
            if typ != SHT_NOBITS and off + size > len(data):
                raise ElfError('section outside file')
            self.sections.append({'name': self._str(names[4], name), 'type': typ, 'flags': flags,
                                  'addr': addr, 'offset': off, 'size': size, 'link': link,
                                  'entsize': entsize})
        self.symbols = []
        for s in self.sections:
            if s['type'] == SHT_SYMTAB:
                strtab = self.sections[s['link']]
                if s['entsize'] != 24:
                    raise ElfError('bad symtab entry size')
                for i in range(s['size'] // 24):
                    name, info, other, shndx, value, size = struct.unpack_from('<IBBHQQ', data, s['offset'] + 24 * i)
                    self.symbols.append({'name': self._str(strtab['offset'], name), 'type': info & 0xf,
                                         'bind': info >> 4, 'shndx': shndx, 'value': value, 'size': size})
        if not self.symbols:
            raise ElfError('no .symtab: the binary is stripped')

    def _str(self, table_off, idx):
        end = self.data.index(b'\0', table_off + idx)
        return self.data[table_off + idx:end].decode('ascii')

    def section_at(self, va, length=1):
        hits = [s for s in self.sections if s['flags'] & SHF_ALLOC and s['type'] != SHT_NOBITS
                and s['addr'] <= va and va + length <= s['addr'] + s['size']]
        if len(hits) != 1:
            raise ElfError(f'address {va:#x}+{length} is not inside exactly one loaded section')
        return hits[0]

    def read(self, va, length):
        s = self.section_at(va, length)
        o = s['offset'] + va - s['addr']
        return self.data[o:o + length]

    def read_const(self, va, length):
        """Bytes of a read-only, non-executable loaded section (a constant pool)."""
        s = self.section_at(va, length)
        if s['flags'] & (SHF_WRITE | SHF_EXECINSTR):
            raise ElfError(f'constant {va:#x} is in writable or executable section {s["name"]}')
        return self.read(va, length)

    def cstring(self, va, limit=256):
        s = self.section_at(va)
        if s['flags'] & (SHF_WRITE | SHF_EXECINSTR):
            raise ElfError(f'string {va:#x} is not in a read-only data section')
        o = s['offset'] + va - s['addr']
        end = self.data.find(b'\0', o, min(o + limit, s['offset'] + s['size']))
        if end < 0:
            raise ElfError(f'unterminated string at {va:#x}')
        return self.data[o:end].decode('ascii')

    def function(self, name, value=None):
        """The unique FUNC symbol `name` (at `value`, for local clones that share a name),
        its executable bytes and their SHA-256."""
        hits = [s for s in self.symbols if s['name'] == name and s['type'] == STT_FUNC
                and (value is None or s['value'] == value)]
        if len(hits) != 1:
            raise ElfError(f'{len(hits)} FUNC symbols named {name}' + ('' if value is None else f' at {value:#x}'))
        sym = hits[0]
        if sym['size'] == 0:
            raise ElfError(f'{name} has no size')
        sec = self.section_at(sym['value'], sym['size'])
        if not sec['flags'] & SHF_EXECINSTR:
            raise ElfError(f'{name} is not in an executable section')
        code = self.read(sym['value'], sym['size'])
        return {'name': name, 'value': sym['value'], 'size': sym['size'], 'bytes': code,
                'sha256': hashlib.sha256(code).hexdigest(), 'section': sec['name']}

    def plt_import(self, va):
        """Imported symbol called through the .plt.sec stub at `va` (exact stub pattern only).

        Stub (the only reviewed form): f3 0f 1e fa (endbr64), ff 25 disp32 (jmp *disp(%rip)),
        66 0f 1f 44 00 00 (nopw). The GOT slot must carry an R_X86_64_JUMP_SLOT (7) relocation.
        """
        s = self.section_at(va, 16)
        if s['name'] != '.plt.sec':
            raise ElfError(f'{va:#x} is not in .plt.sec')
        stub = self.read(va, 16)
        if stub[:6] != bytes.fromhex('f30f1efaff25') or stub[10:] != bytes.fromhex('660f1f440000'):
            raise ElfError(f'unexpected PLT stub bytes at {va:#x}: {stub.hex()}')
        got = va + 10 + struct.unpack_from('<i', stub, 6)[0]
        rela = [x for x in self.sections if x['name'] == '.rela.plt']
        dynsym = [x for x in self.sections if x['name'] == '.dynsym']
        if len(rela) != 1 or len(dynsym) != 1:
            raise ElfError('missing .rela.plt or .dynsym')
        dynstr = self.sections[dynsym[0]['link']]
        hits = []
        for table in ('.rela.dyn', '.rela.plt'):
            tables = [x for x in self.sections if x['name'] == table]
            if len(tables) != 1:
                raise ElfError(f'missing or ambiguous {table}')
            r = tables[0]
            if r['entsize'] != 24 or r['size'] % 24:
                raise ElfError(f'invalid {table} entry size')
            for k in range(r['size'] // 24):
                off, info, add = struct.unpack_from('<QQq', self.data, r['offset'] + 24 * k)
                typ, idx = info & 0xffffffff, info >> 32
                if typ not in (1, 5, 6, 7, 8) or idx >= dynsym[0]['size'] // 24:
                    raise ElfError('unsupported runtime relocation write or symbol')
                width = struct.unpack_from('<Q', self.data, dynsym[0]['offset'] + 24 * idx + 16)[0] if typ == 5 else 8
                if width == 0 or off + width > 1 << 64:
                    raise ElfError('invalid runtime relocation write size')
                if off < got + 8 and got < off + width:
                    hits.append((table, off, typ, idx))
        if len(hits) != 1 or hits[0][:3] != ('.rela.plt', got, 7):
            raise ElfError(f'GOT {got:#x} does not have one unique JUMP_SLOT write: {hits}')
        idx = hits[0][3]
        name_off = struct.unpack_from('<I', self.data, dynsym[0]['offset'] + 24 * idx)[0]
        return {'import': self._str(dynstr['offset'], name_off), 'stub_sha256': hashlib.sha256(stub).hexdigest(),
                'got': hex(got)}

    def functions_at(self, va):
        return sorted({s['name'] for s in self.symbols if s['type'] == STT_FUNC and s['value'] == va})

    def notes(self):
        out = {}
        for s in self.sections:
            if s['type'] != SHT_NOTE:
                continue
            o, end = s['offset'], s['offset'] + s['size']
            while o < end:
                namesz, descsz, typ = struct.unpack_from('<III', self.data, o)
                name = self.data[o + 12:o + 12 + namesz].rstrip(b'\0').decode()
                d0 = o + 12 + ((namesz + 3) & ~3)
                desc = self.data[d0:d0 + descsz]
                out.setdefault((name, typ), []).append(desc)
                o = d0 + ((descsz + 3) & ~3)
        return out

    def identity(self):
        notes = self.notes()
        build_id = [d.hex() for d in notes.get(('GNU', 3), [])]
        abi = [struct.unpack('<IIII', d) for d in notes.get(('GNU', 1), [])]
        props = []
        for d in notes.get(('GNU', 5), []):
            o = 0
            while o + 8 <= len(d):
                ptype, psz = struct.unpack_from('<II', d, o)
                props.append({'type': hex(ptype), 'data': d[o + 8:o + 8 + psz].hex()})
                o += 8 + ((psz + 7) & ~7)
        return {'sha256': self.sha256, 'class': 'ELF64', 'data': 'LSB', 'machine': self.e_machine,
                'type': self.e_type, 'osabi': self.osabi, 'build_id': build_id,
                'abi_tag': [list(a) for a in abi], 'gnu_properties': props,
                'comment': self.comment()}

    def comment(self):
        s = [x for x in self.sections if x['name'] == '.comment']
        if len(s) != 1:
            return None
        raw = self.data[s[0]['offset']:s[0]['offset'] + s[0]['size']]
        return sorted({p.decode(errors='replace') for p in raw.split(b'\0') if p})


def git_blob_id(data):
    return hashlib.sha1(b'blob %d\0' % len(data) + data).hexdigest()
