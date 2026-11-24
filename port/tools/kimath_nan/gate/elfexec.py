# SPDX-License-Identifier: GPL-3.0-or-later
"""Strict ELF execution-mapping validation for the object-bound gate (current profile only).

The object gate reads code, constants, symbols and dynamic relocations through the SECTION
table (elfbin.Elf, linkproof.Linked). The kernel and the dynamic loader use the PROGRAM
headers and the PT_DYNAMIC table instead. This module proves, from the raw file bytes and
independently of elfbin, that for the reviewed build profile every section fact the gate uses
is the runtime fact:

- the ELF header is exactly the profile (ELF64 LSB, GNU/Linux OSABI, x86-64 PIE executable),
  with the program-header table where the loader reads it;
- the program headers are exactly the profile's sequence of types, flags and alignments; PT_LOAD
  mappings are congruent, ascending and page-disjoint, map in-file bytes, and only the writable
  segment has a zero-filled tail;
- every allocated section lies in exactly one PT_LOAD with file offset - p_offset equal to
  address - p_vaddr (so reading the section is reading the mapped bytes), with execute and
  write permission equal to its segment's; no unallocated section or the section-header table
  overlaps a mapped file range; allocated file ranges do not overlap each other;
- each executable mapping holds only executable sections and zero padding;
- the entry point is `_start` inside `.text`;
- PT_PHDR, PT_INTERP, the notes, PT_GNU_PROPERTY, PT_GNU_EH_FRAME, PT_GNU_STACK and PT_GNU_RELRO
  are exactly their sections; the interpreter is the profile's loader;
- PT_DYNAMIC is exactly `.dynamic`. Its tag sequence is the profile's, and every address and
  size the loader reads (string and symbol tables, RELA and JMPREL relocations, init/fini,
  GNU hash, version tables, PLTGOT) names exactly the section the gate parses, resolved
  through the program headers;
- dynamic relocations have only the profile's types, at writable mapped places, with
  RELACOUNT leading RELATIVE entries and valid symbol indices;
- symbol versioning is well formed (verneed chain, ELF hashes, versym count). Each import the
  unit uses binds the profile's library and version.

Unknown or ambiguous layouts are rejected rather than masked. Measured profile facts are bound
by a separately DERIVED, PENDING extension lock (locks/object-elfexec.lock.json), never by
this code or a reviewer field.

usage: elfexec.py derive --oracle REVIEWED_ORACLE --object-lock locks/object.lock.json --out EXT
       elfexec.py check  --oracle ORACLE --object-lock ... --ext EXT
"""
import argparse
import hashlib
import json
import pathlib
import re
import struct
import sys

MODE = 'object-elfexec-v1'
PT = {1: 'LOAD', 2: 'DYNAMIC', 3: 'INTERP', 4: 'NOTE', 6: 'PHDR', 0x6474E550: 'GNU_EH_FRAME',
      0x6474E551: 'GNU_STACK', 0x6474E552: 'GNU_RELRO', 0x6474E553: 'GNU_PROPERTY'}
DT = {0: 'NULL', 1: 'NEEDED', 2: 'PLTRELSZ', 3: 'PLTGOT', 5: 'STRTAB', 6: 'SYMTAB', 7: 'RELA', 8: 'RELASZ',
      9: 'RELAENT', 10: 'STRSZ', 11: 'SYMENT', 12: 'INIT', 13: 'FINI', 20: 'PLTREL', 21: 'DEBUG', 23: 'JMPREL',
      25: 'INIT_ARRAY', 26: 'FINI_ARRAY', 27: 'INIT_ARRAYSZ', 28: 'FINI_ARRAYSZ', 30: 'FLAGS',
      0x6FFFFEF5: 'GNU_HASH', 0x6FFFFFF0: 'VERSYM', 0x6FFFFFF9: 'RELACOUNT', 0x6FFFFFFB: 'FLAGS_1',
      0x6FFFFFFE: 'VERNEED', 0x6FFFFFFF: 'VERNEEDNUM'}
SHF_WRITE, SHF_ALLOC, SHF_EXECINSTR = 1, 2, 4
SHT_NOBITS = 8
PF_X, PF_W, PF_R = 1, 2, 4
EXT_FIELDS = ['note', 'mode', 'status', 'derived_from', 'header', 'program_headers', 'interpreter', 'dynamic_tags',
              'dynamic_flags', 'needed', 'section_segments', 'relocation_types', 'entry_symbol', 'import_versions']
ROWS = [
    'ELF execution: header is the profile with in-file program headers',
    'ELF execution: program headers are the profile (types, flags, alignment, congruent disjoint loads)',
    'ELF execution: every allocated section is the loader-mapped bytes of one PT_LOAD with its permissions',
    'ELF execution: executable mappings hold only executable sections and zero padding',
    'ELF execution: entry point is _start inside .text in an executable mapping',
    'ELF execution: PT_PHDR, PT_INTERP, notes and GNU segments are their sections; reviewed interpreter',
    'ELF execution: PT_DYNAMIC is .dynamic and every DT_* the loader reads names the parsed sections',
    'ELF execution: dynamic relocations have reviewed types at writable mapped places',
    'ELF execution: symbol versions are well formed and the unit imports bind the reviewed versions',
    'ELF execution extension lock: derived, pending, complete fields',
]


class ExecError(Exception):
    pass


def elf_hash(name):
    h = 0
    for c in name.encode():
        h = ((h << 4) + c) & 0xFFFFFFFF
        g = h & 0xF0000000
        if g:
            h ^= g >> 24
        h &= ~g & 0xFFFFFFFF
    return h


class Raw:
    """Independent raw parse (does not use elfbin)."""

    def __init__(self, data):
        self.data = data
        if len(data) < 64 or data[:4] != b'\x7fELF':
            raise ExecError('not an ELF file')
        self.ident = data[:16]
        (self.e_type, self.e_machine, self.e_version, self.e_entry, self.e_phoff, self.e_shoff, self.e_flags,
         self.e_ehsize, self.e_phentsize, self.e_phnum, self.e_shentsize, self.e_shnum,
         self.e_shstrndx) = struct.unpack_from('<HHIQQQIHHHHHH', data, 16)
        self.phs = []
        if self.e_phentsize == 56 and 0 < self.e_phnum < 0xFFFF and self.e_phoff + 56 * self.e_phnum <= len(data):
            for i in range(self.e_phnum):
                t, f, off, va, pa, fs, ms, al = struct.unpack_from('<IIQQQQQQ', data, self.e_phoff + 56 * i)
                self.phs.append({'type': t, 'flags': f, 'offset': off, 'vaddr': va, 'paddr': pa, 'filesz': fs,
                                 'memsz': ms, 'align': al})
        if self.e_shentsize != 64 or not 0 < self.e_shstrndx < self.e_shnum or self.e_shoff + 64 * self.e_shnum > len(data):
            raise ExecError('bad section header table')
        raw = [struct.unpack_from('<IIQQQQIIQQ', data, self.e_shoff + 64 * i) for i in range(self.e_shnum)]
        so, ss = raw[self.e_shstrndx][4], raw[self.e_shstrndx][5]
        if so + ss > len(data):
            raise ExecError('section name table outside the file')
        self.secs = []
        for i, (nm, typ, fl, addr, off, size, link, info, align, entsize) in enumerate(raw):
            end = data.index(b'\0', so + nm)
            if typ != SHT_NOBITS and off + size > len(data):
                raise ExecError(f'section {i} outside the file')
            self.secs.append({'index': i, 'name': data[so + nm:end].decode('ascii'), 'type': typ, 'flags': fl,
                              'addr': addr, 'offset': off, 'size': size, 'link': link, 'info': info,
                              'entsize': entsize})
        names = [s['name'] for s in self.secs if s['flags'] & SHF_ALLOC]
        if len(names) != len(set(names)):
            raise ExecError('duplicate allocated section names')
        self.by = {s['name']: s for s in self.secs}

    def loads(self):
        return [p for p in self.phs if p['type'] == 1]

    def va_to_off(self, va, length=1):
        hits = [p for p in self.loads() if p['vaddr'] <= va and va + length <= p['vaddr'] + p['filesz']]
        if len(hits) != 1:
            raise ExecError(f'address {va:#x}+{length} is not in exactly one mapped file range')
        return hits[0]['offset'] + va - hits[0]['vaddr']

    def read_va(self, va, length):
        o = self.va_to_off(va, length)
        return self.data[o:o + length]

    def cstr_va(self, va):
        o = self.va_to_off(va)
        end = self.data.index(b'\0', o)
        return self.data[o:end].decode('ascii')

    def dynamic(self):
        d = [p for p in self.phs if p['type'] == 2]
        if len(d) != 1:
            raise ExecError('not exactly one PT_DYNAMIC')
        out = []
        for k in range(d[0]['filesz'] // 16):
            tag, val = struct.unpack_from('<qQ', self.data, d[0]['offset'] + 16 * k)
            out.append((tag & 0xFFFFFFFFFFFFFFFF if tag >= 0 else tag, val))
        return d[0], out


def check(data, ext, unit_imports):
    """Returns [(row, ok, detail)] in ROWS order; never raises for malformed input."""
    if not ext_row(ext)[1]:
        return [(n, False, 'invalid extension schema') for n in ROWS]
    res = {}

    def row(i, fn):
        try:
            ok, det = fn()
        except (ExecError, struct.error, ValueError, KeyError, IndexError, UnicodeDecodeError, StopIteration) as e:
            ok, det = False, f'{type(e).__name__}: {e}'
        res[ROWS[i]] = (bool(ok), det)

    try:
        raw = Raw(data)
    except (ExecError, struct.error, ValueError, IndexError, UnicodeDecodeError) as e:
        return [(n, False, f'unparseable: {e}') for n in ROWS[:-1]] + [ext_row(ext)]
    prof = ext or {}

    def header():
        h = prof['header']
        bad = []
        if raw.ident[4:7] != b'\x02\x01\x01' or raw.ident[7] != h['osabi'] or raw.ident[8] != h['abiversion'] \
                or raw.ident[9:16] != bytes(7):
            bad.append('ident')
        for k in ('type', 'machine', 'flags', 'ehsize', 'phentsize', 'shentsize'):
            if getattr(raw, 'e_' + k) != h[k]:
                bad.append(k)
        if raw.e_version != 1:
            bad.append('version')
        if raw.e_phoff != h['ehsize']:
            bad.append('program headers do not follow the header')
        if raw.e_phnum != len(prof['program_headers']) or not raw.phs:
            bad.append(f'phnum {raw.e_phnum}')
        return not bad, bad

    def program_headers():
        bad = []
        seq = [[p['type'], p['flags'], p['align']] for p in raw.phs]
        if seq != prof['program_headers']:
            bad.append('type/flags/align sequence differs')
        loads = raw.loads()
        for p in raw.phs:
            if p['paddr'] != p['vaddr'] or p['filesz'] > p['memsz'] or p['offset'] + p['filesz'] > len(data):
                bad.append(f'{PT.get(p["type"], p["type"])} bounds')
        for i, p in enumerate(loads):
            if p['align'] != 0x1000 or p['offset'] % 0x1000 != p['vaddr'] % 0x1000:
                bad.append(f'LOAD {i} not congruent')
            if p['memsz'] != p['filesz'] and not (p['flags'] == PF_R | PF_W and i == len(loads) - 1):
                bad.append(f'LOAD {i} zero-filled tail')
            if i and (loads[i - 1]['vaddr'] + loads[i - 1]['memsz'] + 0xFFF) // 0x1000 > p['vaddr'] // 0x1000:
                bad.append(f'LOAD {i} overlaps the previous mapping')
            if i and loads[i - 1]['offset'] + loads[i - 1]['filesz'] > p['offset']:
                bad.append(f'LOAD {i} file range overlaps')
        if not loads or loads[0]['offset'] != 0:
            bad.append('first LOAD does not map the header')
        return not bad, bad

    def sections():
        bad, segs = [], {}
        loads = raw.loads()
        alloc = [s for s in raw.secs if s['flags'] & SHF_ALLOC]
        for s in alloc:
            hit = [i for i, p in enumerate(loads) if p['vaddr'] <= s['addr'] and s['addr'] + s['size'] <= p['vaddr'] + p['memsz']]
            if len(hit) != 1:
                bad.append(f'{s["name"]}: in {len(hit)} loads')
                continue
            p = loads[hit[0]]
            segs[s['name']] = hit[0]
            if s['type'] == SHT_NOBITS:
                if s['addr'] < p['vaddr'] + p['filesz']:
                    bad.append(f'{s["name"]}: NOBITS inside the file-backed range')
            elif not (s['offset'] - p['offset'] == s['addr'] - p['vaddr'] and s['addr'] + s['size'] <= p['vaddr'] + p['filesz']):
                bad.append(f'{s["name"]}: section bytes are not the mapped bytes')
            if bool(s['flags'] & SHF_EXECINSTR) != bool(p['flags'] & PF_X) or bool(s['flags'] & SHF_WRITE) != bool(p['flags'] & PF_W):
                bad.append(f'{s["name"]}: permissions differ from its segment')
        rng = sorted((s['offset'], s['offset'] + s['size'], s['name']) for s in alloc if s['type'] != SHT_NOBITS and s['size'])
        for (a0, a1, n0), (b0, b1, n1) in zip(rng, rng[1:]):
            if b0 < a1:
                bad.append(f'{n0} and {n1} overlap in the file')
        mapped = [(p['offset'], p['offset'] + p['filesz']) for p in loads]
        for s in raw.secs:
            if not s['flags'] & SHF_ALLOC and s['type'] not in (0, SHT_NOBITS) and s['size']:
                if any(s['offset'] < b and a < s['offset'] + s['size'] for a, b in mapped):
                    bad.append(f'unallocated {s["name"]} overlaps a mapped file range')
        sh0, sh1 = raw.e_shoff, raw.e_shoff + 64 * raw.e_shnum
        if any(sh0 < b and a < sh1 for a, b in mapped):
            bad.append('section header table is mapped')
        if segs != prof['section_segments']:
            bad.append('section-to-segment assignment differs from the profile')
        return not bad, bad

    def exec_mapping():
        bad = []
        if len([p for p in raw.loads() if p['flags'] & PF_X]) != 1:
            bad.append('not exactly one executable PT_LOAD')
        for p in raw.loads():
            if not p['flags'] & PF_X:
                continue
            cur, end = p['vaddr'], p['vaddr'] + p['filesz']
            for s in sorted((s for s in raw.secs if s['flags'] & SHF_ALLOC and p['vaddr'] <= s['addr'] < end), key=lambda s: s['addr']):
                if not s['flags'] & SHF_EXECINSTR:
                    bad.append(f'non-executable {s["name"]} in an executable mapping')
                if s['addr'] > cur:
                    gap = raw.read_va(cur, s['addr'] - cur)
                    if gap.strip(b'\0'):
                        bad.append(f'non-zero bytes before {s["name"]}')
                cur = max(cur, s['addr'] + s['size'])
            if cur < end and raw.read_va(cur, end - cur).strip(b'\0'):
                bad.append('non-zero bytes after the last executable section')
        return not bad, bad

    def entry():
        st = raw.by['.symtab']
        strt = raw.secs[st['link']]
        hits = []
        for k in range(st['size'] // 24):
            nm, info, other, shndx, value, size = struct.unpack_from('<IBBHQQ', data, st['offset'] + 24 * k)
            name = data[strt['offset'] + nm:data.index(b'\0', strt['offset'] + nm)]
            if name == prof['entry_symbol'].encode() and info == 0x12 and shndx != 0:
                hits.append(value)
        t = raw.by['.text']
        x = [p for p in raw.loads() if p['flags'] & PF_X and p['vaddr'] <= raw.e_entry < p['vaddr'] + p['filesz']]
        ok = hits == [raw.e_entry] and t['addr'] <= raw.e_entry < t['addr'] + t['size'] and len(x) == 1
        return ok, {'entry': hex(raw.e_entry), 'start_symbols': [hex(v) for v in hits]}

    def special():
        bad = []
        ph = {}
        for p in raw.phs:
            ph.setdefault(p['type'], []).append(p)
        eq = lambda p, s: (p['offset'], p['vaddr'], p['filesz'], p['memsz']) == (s['offset'], s['addr'], s['size'], s['size'])
        phdr = ph[6][0]
        if (phdr['offset'], phdr['vaddr'], phdr['filesz']) != (raw.e_phoff, raw.e_phoff, 56 * raw.e_phnum):
            bad.append('PT_PHDR is not the program-header table')
        if not eq(ph[3][0], raw.by['.interp']):
            bad.append('PT_INTERP is not .interp')
        if raw.read_va(ph[3][0]['vaddr'], ph[3][0]['filesz']) != prof['interpreter'].encode() + b'\0':
            bad.append('interpreter differs')
        notes = ph[4]
        prop, bid, abi = raw.by['.note.gnu.property'], raw.by['.note.gnu.build-id'], raw.by['.note.ABI-tag']
        if not eq(notes[0], prop) or not eq(ph[0x6474E553][0], prop):
            bad.append('property notes are not .note.gnu.property')
        if not (notes[1]['offset'], notes[1]['vaddr'], notes[1]['filesz']) == (bid['offset'], bid['addr'], abi['addr'] + abi['size'] - bid['addr']) \
                or abi['addr'] != bid['addr'] + bid['size']:
            bad.append('PT_NOTE is not .note.gnu.build-id + .note.ABI-tag')
        if not eq(ph[0x6474E550][0], raw.by['.eh_frame_hdr']):
            bad.append('PT_GNU_EH_FRAME is not .eh_frame_hdr')
        st = ph[0x6474E551][0]
        if (st['filesz'], st['memsz'], st['flags']) != (0, 0, PF_R | PF_W):
            bad.append('PT_GNU_STACK is not a non-executable stack')
        rel = ph[0x6474E552][0]
        rw = [p for p in raw.loads() if p['flags'] & PF_W]
        got, dat = raw.by['.got'], raw.by['.data']
        if not (len(rw) == 1 and rel['vaddr'] == rw[0]['vaddr'] and rel['offset'] == rw[0]['offset']
                and got['addr'] + got['size'] <= rel['vaddr'] + rel['memsz'] <= dat['addr']):
            bad.append('PT_GNU_RELRO does not cover exactly the relocated read-only data')
        return not bad, bad

    def dynamic():
        bad = []
        p, tags = raw.dynamic()
        d = raw.by['.dynamic']
        if (p['offset'], p['vaddr'], p['filesz'], p['memsz']) != (d['offset'], d['addr'], d['size'], d['size']):
            bad.append('PT_DYNAMIC is not .dynamic')
        n = next(i for i, (t, v) in enumerate(tags) if t == 0)
        if any(t or v for t, v in tags[n + 1:]):
            bad.append('entries after DT_NULL')
        seq = [DT.get(t, hex(t)) for t, v in tags[:n + 1]]
        if seq != prof['dynamic_tags']:
            bad.append({'tag sequence': seq})
        val = {}
        for t, v in tags[:n]:
            if t != 1:
                val[DT.get(t, t)] = v
        sec = raw.by
        want = {'STRTAB': sec['.dynstr']['addr'], 'STRSZ': sec['.dynstr']['size'], 'SYMTAB': sec['.dynsym']['addr'],
                'SYMENT': 24, 'RELA': sec['.rela.dyn']['addr'], 'RELASZ': sec['.rela.dyn']['size'], 'RELAENT': 24,
                'JMPREL': sec['.rela.plt']['addr'], 'PLTRELSZ': sec['.rela.plt']['size'], 'PLTREL': 7,
                'PLTGOT': sec['.got']['addr'], 'INIT': sec['.init']['addr'], 'FINI': sec['.fini']['addr'],
                'INIT_ARRAY': sec['.init_array']['addr'], 'INIT_ARRAYSZ': sec['.init_array']['size'],
                'FINI_ARRAY': sec['.fini_array']['addr'], 'FINI_ARRAYSZ': sec['.fini_array']['size'],
                'GNU_HASH': sec['.gnu.hash']['addr'], 'VERSYM': sec['.gnu.version']['addr'],
                'VERNEED': sec['.gnu.version_r']['addr'], 'DEBUG': 0,
                'FLAGS': prof['dynamic_flags']['FLAGS'], 'FLAGS_1': prof['dynamic_flags']['FLAGS_1']}
        for k, w in want.items():
            if val.get(k) != w:
                bad.append(f'DT_{k} {val.get(k)!r} is not {w:#x}')
        rel = sec['.rela.dyn']
        rc = 0
        for k in range(rel['size'] // 24):
            if struct.unpack_from('<QQq', data, rel['offset'] + 24 * k)[1] & 0xFFFFFFFF != 8:
                break
            rc += 1
        if val.get('RELACOUNT') != rc:
            bad.append(f'DT_RELACOUNT {val.get("RELACOUNT")} is not the {rc} leading RELATIVE relocations')
        vn = parse_verneed(raw, val.get('VERNEED', 0))
        if val.get('VERNEEDNUM') != len(vn):
            bad.append('DT_VERNEEDNUM differs from the verneed chain')
        needed = [raw.cstr_va(val.get('STRTAB', 0) + v) for t, v in tags[:n] if t == 1]
        if needed != prof['needed']:
            bad.append({'needed': needed})
        # the section links the gate relies on
        idx = {s['name']: s['index'] for s in raw.secs}
        for a, b in (('.dynsym', '.dynstr'), ('.rela.dyn', '.dynsym'), ('.rela.plt', '.dynsym'), ('.gnu.version', '.dynsym'),
                     ('.gnu.version_r', '.dynstr'), ('.dynamic', '.dynstr')):
            if sec[a]['link'] != idx[b]:
                bad.append(f'{a} sh_link is not {b}')
        if sec['.dynsym']['entsize'] != 24 or sec['.gnu.version']['size'] * 12 != sec['.dynsym']['size']:
            bad.append('dynsym/versym sizes disagree')
        return not bad, bad

    def relocations():
        bad, types = [], {}
        writes = relocation_writes(raw)
        reject_overlapping_writes(writes)
        widths = {(name, index): end - start for start, end, name, index in writes}
        nsym = raw.by['.dynsym']['size'] // 24
        w = [s for s in raw.secs if s['flags'] & SHF_ALLOC and s['flags'] & SHF_WRITE]
        got, bss = raw.by['.got'], raw.by['.bss']
        for name in ('.rela.dyn', '.rela.plt'):
            s = raw.by[name]
            allowed = set(prof['relocation_types'][name])
            seen = set()
            for k in range(s['size'] // 24):
                off, info, add = struct.unpack_from('<QQq', data, s['offset'] + 24 * k)
                t, sym = info & 0xFFFFFFFF, info >> 32
                seen.add(t)
                if t not in allowed:
                    bad.append(f'{name}: relocation type {t}')
                if sym >= nsym or (t == 8) != (sym == 0):
                    bad.append(f'{name}: symbol index {sym} for type {t}')
                width = widths[(name, k)]
                place = [x for x in w if x['addr'] <= off and off + width <= x['addr'] + x['size']]
                if len(place) != 1:
                    bad.append(f'{name}: place {off:#x} is not in one writable section')
                elif t == 7 and place[0]['name'] != '.got' or t == 5 and place[0]['name'] not in ('.bss', '.data.rel.ro'):
                    bad.append(f'{name}: type {t} at {place[0]["name"]}')
            types[name] = sorted(seen)
        return not bad, bad[:8] or types

    def versions():
        bad = []
        p, tags = raw.dynamic()
        val = {DT.get(t, t): v for t, v in tags if t not in (0, 1)}
        vn = parse_verneed(raw, val['VERNEED'])
        by_index = {}
        for lib, entries in vn:
            if lib not in prof['needed']:
                bad.append(f'verneed file {lib} is not DT_NEEDED')
            for e in entries:
                if e['hash'] != elf_hash(e['name']):
                    bad.append(f'{e["name"]}: hash differs')
                if e['other'] in by_index or e['other'] < 2:
                    bad.append(f'version index {e["other"]} reused')
                by_index[e['other']] = [lib, e['name']]
        ds, vs = raw.by['.dynsym'], raw.by['.gnu.version']
        dstr = raw.by['.dynstr']
        found, copies = {}, copy_symbols(raw)
        for i in range(ds['size'] // 24):
            nm, info, other, shndx, value, size = struct.unpack_from('<IBBHQQ', data, ds['offset'] + 24 * i)
            name = data[dstr['offset'] + nm:data.index(b'\0', dstr['offset'] + nm)].decode('ascii')
            v = struct.unpack_from('<H', data, vs['offset'] + 2 * i)[0] & 0x7FFF
            if v > 1 and v not in by_index:
                bad.append(f'{name}: version index {v} undefined')
            if name in unit_imports and (shndx == 0 or i in copies):
                found.setdefault(name, []).append(by_index.get(v, 'unversioned' if v <= 1 else None))
        got = {n: (v[0] if len(v) == 1 else v) for n, v in sorted(found.items())}
        missing = sorted(set(unit_imports) - set(found))
        if missing:
            bad.append({'imports without an undefined or copy-relocated dynamic symbol': missing[:5]})
        if got != prof['import_versions']:
            bad.append({'changed': sorted(k for k in set(got) | set(prof['import_versions'])
                                          if got.get(k) != prof['import_versions'].get(k))[:6]})
        return not bad, bad[:8] or {'imports': len(got)}

    for i, fn in enumerate((header, program_headers, sections, exec_mapping, entry, special, dynamic, relocations, versions)):
        row(i, fn)
    out = [(n,) + res[n] for n in ROWS[:-1]]
    return out + [ext_row(ext)]


def ext_row(ext):
    def text(v):
        return type(v) is str and bool(v.strip())

    def number(v):
        return type(v) is int and v >= 0

    def numbers(v):
        return type(v) is list and bool(v) and all(number(n) for n in v)

    def mapping(v, fields, test):
        return type(v) is dict and set(v) == set(fields) and all(test(x) for x in v.values())

    ok = type(ext) is dict and set(ext) == set(EXT_FIELDS)
    if ok:
        ok = (ext['mode'] == MODE and ext['status'] == 'DERIVED-PENDING-REVIEW'
              and text(ext['note']) and type(ext['derived_from']) is str
              and re.fullmatch('[0-9a-f]{64}', ext['derived_from']) is not None
              and mapping(ext['header'], ('osabi', 'abiversion', 'type', 'machine', 'flags', 'ehsize',
                                         'phentsize', 'shentsize'), number)
              and type(ext['program_headers']) is list and bool(ext['program_headers'])
              and all(numbers(p) and len(p) == 3 for p in ext['program_headers'])
              and text(ext['interpreter']) and ext['entry_symbol'] == '_start'
              and all(type(ext[k]) is list and bool(ext[k]) and all(text(n) for n in ext[k])
                      for k in ('dynamic_tags', 'needed'))
              and mapping(ext['dynamic_flags'], ('FLAGS', 'FLAGS_1'), number)
              and type(ext['section_segments']) is dict and bool(ext['section_segments'])
              and all(text(k) and number(v) for k, v in ext['section_segments'].items())
              and mapping(ext['relocation_types'], ('.rela.dyn', '.rela.plt'), numbers)
              and type(ext['import_versions']) is dict and bool(ext['import_versions'])
              and all(text(k) and type(v) is list and len(v) == 2 and all(text(x) for x in v)
                      for k, v in ext['import_versions'].items()))
    return (ROWS[-1], ok, None if ok else 'missing, incomplete or not the derived pending extension')


def copy_symbols(raw):
    s = raw.by['.rela.dyn']
    return {struct.unpack_from('<QQq', raw.data, s['offset'] + 24 * k)[1] >> 32 for k in range(s['size'] // 24)
            if struct.unpack_from('<QQq', raw.data, s['offset'] + 24 * k)[1] & 0xFFFFFFFF == 5}


def relocation_writes(raw):
    """Every loader write across both RELA tables, including full COPY symbol sizes."""
    ds = raw.by['.dynsym']
    writes = []
    for name in ('.rela.dyn', '.rela.plt'):
        s = raw.by[name]
        if s['type'] != 4 or s['entsize'] != 24 or s['size'] % 24:
            raise ExecError(f'{name}: invalid RELA table shape')
        for index in range(s['size'] // 24):
            off, info, _add = struct.unpack_from('<QQq', raw.data, s['offset'] + 24 * index)
            typ, sym = info & 0xFFFFFFFF, info >> 32
            if typ not in (1, 5, 6, 7, 8) or sym >= ds['size'] // 24:
                raise ExecError(f'{name}: unknown write type or symbol index')
            size = struct.unpack_from('<Q', raw.data, ds['offset'] + 24 * sym + 16)[0] if typ == 5 else 8
            if size == 0 or off + size > 1 << 64:
                raise ExecError(f'{name}: invalid relocation write size')
            writes.append((off, off + size, name, index))
    return writes


def reject_overlapping_writes(writes):
    ordered = sorted(writes)
    for previous, current in zip(ordered, ordered[1:]):
        if current[0] < previous[1]:
            raise ExecError(f'overlapping runtime relocation writes: {previous} and {current}')


def parse_verneed(raw, va):
    out, seen = [], 0
    s = raw.by['.gnu.version_r']
    if va != s['addr']:
        raise ExecError('DT_VERNEED is not .gnu.version_r')
    dstr = raw.by['.dynstr']
    def cstr(o):
        if not 0 <= o < dstr['size']:
            raise ExecError('version string outside .dynstr')
        start = dstr['offset'] + o
        end = raw.data.index(b'\0', start, dstr['offset'] + dstr['size'])
        return raw.data[start:end].decode('ascii')
    off = s['offset']
    while True:
        if not s['offset'] <= off <= s['offset'] + s['size'] - 16:
            raise ExecError('verneed entry outside .gnu.version_r')
        ver, cnt, file_, aux, nxt = struct.unpack_from('<HHIII', raw.data, off)
        if ver != 1 or cnt == 0 or aux < 16 or aux % 4:
            raise ExecError('invalid verneed version, count or auxiliary offset')
        entries, a = [], off + aux
        for index in range(cnt):
            if not s['offset'] <= a <= s['offset'] + s['size'] - 16:
                raise ExecError('vernaux entry outside .gnu.version_r')
            h, fl, other, name, anext = struct.unpack_from('<IHHII', raw.data, a)
            entries.append({'hash': h, 'flags': fl, 'other': other, 'name': cstr(name)})
            if index == cnt - 1:
                if anext != 0:
                    raise ExecError('vernaux chain continues beyond its recorded count')
            elif anext < 16 or anext % 4:
                raise ExecError('vernaux chain ends early or overlaps its previous entry')
            a += anext
        out.append((cstr(file_), entries))
        seen += 1
        if nxt == 0:
            break
        if seen >= 64 or nxt < 16 or nxt % 4:
            raise ExecError('invalid or overlong verneed chain')
        off += nxt
    return out


def derive(data, unit_imports, oracle_sha):
    raw = Raw(data)
    p, tags = raw.dynamic()
    n = next(i for i, (t, v) in enumerate(tags) if t == 0)
    loads = raw.loads()
    segs = {}
    for s in raw.secs:
        if s['flags'] & SHF_ALLOC:
            hit = [i for i, q in enumerate(loads) if q['vaddr'] <= s['addr'] and s['addr'] + s['size'] <= q['vaddr'] + q['memsz']]
            segs[s['name']] = hit[0]
    ext = {
        'note': ('EXPERIMENTAL execution-mapping extension of the object lock, DERIVED by elfexec.py derive from the '
                 'reviewed oracle; measured facts only, not reviewed'),
        'mode': MODE, 'status': 'DERIVED-PENDING-REVIEW', 'derived_from': oracle_sha,
        'header': {'osabi': raw.ident[7], 'abiversion': raw.ident[8], 'type': raw.e_type, 'machine': raw.e_machine,
                   'flags': raw.e_flags, 'ehsize': raw.e_ehsize, 'phentsize': raw.e_phentsize, 'shentsize': raw.e_shentsize},
        'program_headers': [[q['type'], q['flags'], q['align']] for q in raw.phs],
        'interpreter': raw.read_va([q for q in raw.phs if q['type'] == 3][0]['vaddr'],
                                   [q for q in raw.phs if q['type'] == 3][0]['filesz']).rstrip(b'\0').decode('ascii'),
        'dynamic_tags': [DT.get(t, hex(t)) for t, v in tags[:n + 1]],
        'dynamic_flags': {DT[t]: v for t, v in tags[:n] if t in (30, 0x6FFFFFFB)},
        'needed': [raw.cstr_va(dict((DT.get(t), v) for t, v in tags[:n])['STRTAB'] + v) for t, v in tags[:n] if t == 1],
        'section_segments': segs,
        'relocation_types': {},
        'entry_symbol': '_start',
        'import_versions': {},
    }
    for name in ('.rela.dyn', '.rela.plt'):
        s = raw.by[name]
        ext['relocation_types'][name] = sorted({struct.unpack_from('<QQq', data, s['offset'] + 24 * k)[1] & 0xFFFFFFFF
                                                for k in range(s['size'] // 24)})
    # import versions measured with the same parser the check uses
    vals = {DT.get(t, t): v for t, v in tags[:n] if t != 1}
    by_index = {e['other']: [lib, e['name']] for lib, es in parse_verneed(raw, vals['VERNEED']) for e in es}
    ds, vs, dstr = raw.by['.dynsym'], raw.by['.gnu.version'], raw.by['.dynstr']
    iv, copies = {}, copy_symbols(raw)
    for i in range(ds['size'] // 24):
        nm, info, other, shndx, value, size = struct.unpack_from('<IBBHQQ', data, ds['offset'] + 24 * i)
        name = data[dstr['offset'] + nm:data.index(b'\0', dstr['offset'] + nm)].decode('ascii')
        v = struct.unpack_from('<H', data, vs['offset'] + 2 * i)[0] & 0x7FFF
        if name in unit_imports and (shndx == 0 or i in copies):
            iv.setdefault(name, []).append(by_index.get(v, 'unversioned' if v <= 1 else None))
    ext['import_versions'] = {k: (v[0] if len(v) == 1 else v) for k, v in sorted(iv.items())}
    bad = [r for r in check(data, ext, unit_imports) if not r[1]]
    if bad:
        raise SystemExit(f'the reviewed oracle does not satisfy the structural rules: {bad}')
    return ext


def main():
    p = argparse.ArgumentParser()
    p.add_argument('action', choices=['derive', 'check'])
    p.add_argument('--oracle', required=True)
    p.add_argument('--object-lock', required=True)
    p.add_argument('--ext')
    p.add_argument('--out')
    a = p.parse_args()
    data = pathlib.Path(a.oracle).read_bytes()
    imports = json.loads(pathlib.Path(a.object_lock).read_text(encoding='utf-8'))['link']['imports']
    if a.action == 'derive':
        ext = derive(data, imports, hashlib.sha256(data).hexdigest())
        pathlib.Path(a.out).write_text(json.dumps(ext, indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
        print(a.out, hashlib.sha256(pathlib.Path(a.out).read_bytes()).hexdigest())
        return
    ext = json.loads(pathlib.Path(a.ext).read_text(encoding='utf-8'))
    rows = check(data, ext, imports)
    for n, ok, det in rows:
        print('PASS' if ok else 'FAIL', n, '' if ok else det)
    sys.exit(0 if all(r[1] for r in rows) else 1)


if __name__ == '__main__':
    main()
