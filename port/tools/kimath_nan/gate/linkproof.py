# SPDX-License-Identifier: GPL-3.0-or-later
"""Link proof: the code and constants a compiled object reaches in the linked oracle are
exactly compiler-output bytes after independently verified relocation fixups.

Inputs: the linked oracle, the home object (the matrix adapter), and the provider objects of
the same link (the other objects and the archive members). Nothing is masked and no target
fact is supplied by the caller.

Reachability starts at the home object's global entry points (and constructor arrays) and
follows every relocation, across objects:

- a home-object section is verified whole;
- a reference to a globally named symbol (also one defined in the home object: weak/COMDAT
  definitions are resolved by the linker) is followed to the definition the linker selected.
  The provider is the object whose relocated bytes are exactly the linked bytes at that
  symbol's linked address. A COMDAT section is verified whole; any other provider section only
  over the referenced function's symbol range, with intra-section branches found by the
  validated length decoder and followed the same way;
- a local symbol stays in its own object;
- merged strings/constants: the linked bytes at the target equal the object's item;
- a name no provider defines: an exact .plt.sec stub with a JUMP_SLOT for exactly that name,
  a GOT slot with GLOB_DAT/R_X86_64_64, a copy relocation, or an entry of `externals` (a
  reviewed leaf body hash, or an address-only data handle).

Placement of a section: from its named symbols in the linked symbol table (locals within the
object's own FILE group), else from the addresses all references to it imply, which must agree;
the whole section is then verified there. Relocation fields are rebuilt from the target:
PC32/PLT32 = S + A - P; GOTPCRELX unrelaxed (slot identity checked) or GNU ld's local
relaxation `mov` 0x8b to `lea` 0x8d; R_X86_64_64 = an R_X86_64_RELATIVE dynamic relocation
with addend S + A (the place holds the same value), or R_X86_64_64/GLOB_DAT naming the
undefined symbol (the place holds zero). Anything else fails. .eh_frame is not verified.

Every non-home unit gets a digest: SHA-256 over its exact object bytes and its relocations
with symbolic targets, independent of addresses and object names. A reviewed lock binds the
home object's SHA-256 and every provider digest; see objgate.py.
"""
import hashlib
import json
import struct

from elfbin import ElfError
from relobj import (R_NAMES, SHF_ALLOC, SHF_EXECINSTR, SHF_MERGE, SHF_STRINGS, SHN_ABS, SHN_UNDEF,
                    SHT_GROUP, SHT_NOBITS, STB_LOCAL, STT_FILE, STT_FUNC, STT_SECTION)
from x86len import DecodeError, decode

R_X86_64_COPY, R_X86_64_GLOB_DAT, R_X86_64_JUMP_SLOT, R_X86_64_RELATIVE = 5, 6, 7, 8
SKIP_SECTIONS = {'.eh_frame'}


class LinkError(Exception):
    pass


class Linked:
    """Index of the linked ELF: FILE groups of local symbols, global symbols, dynamic relocations."""

    def __init__(self, elf):
        self.elf = elf
        self.file_groups, self.globals_ = [], {}
        cur = None
        for s in elf.symbols:
            if s['type'] == STT_FILE:
                cur = {'file': s['name'], 'locals': {}}
                self.file_groups.append(cur)
                continue
            if s['bind'] == STB_LOCAL:
                if s['shndx'] != SHN_UNDEF and s['name'] and cur is not None:
                    cur['locals'].setdefault(s['name'], []).append(s)
            elif s['shndx'] != SHN_UNDEF:
                self.globals_.setdefault(s['name'], []).append(s)
        self.sec = {s['name']: s for s in elf.sections}
        self.dyn = {}
        dynsym = self.sec.get('.dynsym')
        names = []
        if dynsym:
            dynstr = elf.sections[dynsym['link']]
            for i in range(dynsym['size'] // 24):
                n = struct.unpack_from('<I', elf.data, dynsym['offset'] + 24 * i)[0]
                names.append(elf._str(dynstr['offset'], n))
        for rname in ('.rela.dyn', '.rela.plt'):
            r = self.sec.get(rname)
            if not r:
                continue
            for k in range(r['size'] // 24):
                off, info, add = struct.unpack_from('<QQq', elf.data, r['offset'] + 24 * k)
                typ, idx = info & 0xFFFFFFFF, info >> 32
                self.dyn.setdefault(off, []).append({'type': typ, 'name': names[idx] if idx else None, 'addend': add})

    def global_addr(self, name):
        hits = self.globals_.get(name, [])
        return hits[0]['value'] if len(hits) == 1 else None

    def plt_name(self, va):
        try:
            s = self.elf.section_at(va, 16)
        except ElfError:
            return None
        if s['name'] == '.plt.got':
            # endbr64; jmp *disp32(%rip); nopw: the GOT slot must carry GLOB_DAT for one name
            stub = self.elf.read(va, 16)
            if (va - s['addr']) % 16 or stub[:6] != bytes.fromhex('f30f1efaff25') or stub[10:] != bytes.fromhex('660f1f440000'):
                raise LinkError(f'unexpected .plt.got stub at {va:#x}: {stub.hex()}')
            rel = self.dyn_at(va + 10 + struct.unpack_from('<i', stub, 6)[0])
            if not (rel and rel['type'] == R_X86_64_GLOB_DAT and rel['name']):
                raise LinkError(f'.plt.got stub at {va:#x} has no GLOB_DAT slot: {rel}')
            return rel['name']
        if s['name'] != '.plt.sec':
            return None
        try:
            return self.elf.plt_import(va)['import']
        except ElfError as e:
            raise LinkError(f'PLT stub at {va:#x}: {e}')

    def dyn_at(self, place):
        rel = self.dyn.get(place, [])
        return rel[0] if len(rel) == 1 else None

    def any_local(self, name, va):
        return any(s['value'] == va for g in self.file_groups for s in g['locals'].get(name, []))


def s32(v):
    return struct.unpack('<i', struct.pack('<I', v & 0xFFFFFFFF))[0]


def named(obj, idx):
    return [y for y in obj.symbols if y['shndx'] == idx and y['type'] not in (STT_SECTION, STT_FILE)
            and y['name'] and not y['name'].startswith('.L')]


class Prover:
    def __init__(self, linked, objects, home, externals=None):
        """objects: {name: RelObj} in link order (home included); externals: {symbol: spec}."""
        self.L, self.elf = linked, linked.elf
        self.objects, self.home = objects, home
        self.externals = externals or {}
        self.base = {}         # (obj, sec) -> linked section address
        self.derived = {}      # (obj, sec) -> set of implied addresses
        self.reached = set()   # units: (obj, sec) whole, or (obj, sec, start, end) function range
        self.done = {}         # unit -> digest (None for home units)
        self.unit_base = {}    # function unit -> where its section's offset 0 would be
        self.facts = {'relocations': 0, 'by_type': {}, 'imports': set(), 'externals': {}, 'merge_items': 0,
                      'relaxed': 0, 'providers': {}}
        self.fgroup = {}
        self.defs = {}         # global name -> [(obj, sec, value, size, is_group)]
        for on, o in objects.items():
            for y in o.symbols:
                if y['bind'] != STB_LOCAL and y['shndx'] not in (SHN_UNDEF, SHN_ABS) and y['shndx'] < len(o.sections):
                    self.defs.setdefault(y['name'], []).append((on, y['shndx'], y['value'], y['size'],
                                                                o.group_of(y['shndx']) is not None))

    # -- placement -------------------------------------------------------------------------
    def file_group(self, on):
        if on in self.fgroup:
            return self.fgroup[on]
        o = self.objects[on]
        fname = next((y['name'] for y in o.symbols if y['type'] == STT_FILE), None)
        locs = {y['name'] for y in o.symbols if y['bind'] == STB_LOCAL and y['type'] == STT_FUNC
                and y['shndx'] not in (SHN_UNDEF, SHN_ABS) and o.group_of(y['shndx']) is None
                and not y['name'].startswith('.L')}
        cands = [g for g in self.L.file_groups if g['file'] == fname]
        best = [g for g in cands if all(n in g['locals'] for n in locs)]
        self.fgroup[on] = best[0] if len(best) == 1 else (cands[0] if len(cands) == 1 and not locs else None)
        return self.fgroup[on]

    def place(self, on, idx):
        key = (on, idx)
        if key in self.base:
            return self.base[key]
        o = self.objects[on]
        s = o.sections[idx]
        cands = set()
        for y in named(o, idx):
            if y['bind'] == STB_LOCAL:
                g = self.file_group(on)
                hits = g['locals'].get(y['name'], []) if g else []
                if o.group_of(idx) is not None and len(hits) != 1:
                    continue  # a COMDAT copy's locals exist only for the selected copy
            else:
                hits = self.L.globals_.get(y['name'], [])
                if o.group_of(idx) is not None or y['bind'] == 2:
                    continue  # weak/COMDAT definitions may be another object's; resolved by name
            if len(hits) != 1:
                raise LinkError(f'{on}:{s["name"]}: {y["name"]} has {len(hits)} linked definitions')
            cands.add(hits[0]['value'] - y['value'])
        if len(cands) > 1:
            raise LinkError(f'{on}:{s["name"]}: symbols disagree on the section address')
        if cands:
            self.base[key] = cands.pop()
            return self.base[key]
        return None

    def is_prov_code(self, on, idx):
        o = self.objects[on]
        s = o.sections[idx]
        return on != self.home and o.group_of(idx) is None and bool(s['flags'] & SHF_EXECINSTR)

    def fn_at(self, on, idx, off):
        fn = [z for z in named(self.objects[on], idx) if z['type'] == STT_FUNC and z['value'] <= off < z['value'] + z['size']]
        if len(fn) != 1:
            raise LinkError(f'{on}: {self.objects[on].sections[idx]["name"]}+{off:#x} is not inside one function')
        return fn[0]

    def fn_addr(self, on, fn):
        """Linked address of one FUNC symbol of a provider object (locals within its FILE group)."""
        if fn['bind'] == STB_LOCAL:
            g = self.file_group(on)
            hits = g['locals'].get(fn['name'], []) if g else []
        else:
            hits = self.L.globals_.get(fn['name'], [])
        if len(hits) != 1:
            raise LinkError(f'{on}: {fn["name"]} has {len(hits)} linked definitions')
        return hits[0]['value']

    # -- relocation targets ------------------------------------------------------------------
    def resolve_name(self, name, T, kind):
        """The linked target T of a globally named reference: verify its identity, reach it."""
        if kind == 'pc':
            plt = self.L.plt_name(T)
            if plt is not None:
                if plt != name:
                    raise LinkError(f'reference to {name} reaches the PLT stub of {plt}')
                self.facts['imports'].add(name)
                return
        addr = self.L.global_addr(name)
        provs = []
        for on, idx, value, size, is_group in self.defs.get(name, []):
            provs.append((on, idx, value, size, is_group))
        if addr is not None and addr == T and provs:
            for on, idx, value, size, is_group in provs:
                o = self.objects[on]
                if not is_group and on != self.home and o.sections[idx]['flags'] & SHF_EXECINSTR:
                    unit = (on, idx, value, value + size)
                elif not is_group and on != self.home:
                    unit = (on, idx)   # data defined by another object: verified whole
                else:
                    unit = (on, idx)
                if self.try_unit(unit, T - value if (is_group or on == self.home) else None):
                    self.facts['providers'].setdefault(name, on)
                    return
            raise LinkError(f'{name}: no provided object\'s relocated bytes are the linked body at {T:#x}')
        rel = self.L.dyn_at(T)
        if rel and rel['type'] == R_X86_64_COPY and rel['name'] == name:
            self.facts['imports'].add(name)
            return
        spec = self.externals.get(name)
        if spec is not None:
            self.check_external(name, spec, T)
            return
        raise LinkError(f'reference to {name} reaches {T:#x}, which is not a verified definition of {name}')

    def check_external(self, name, spec, T):
        hits = [s for s in self.L.globals_.get(name, []) if s['value'] == T]
        hits += [s for g in self.L.file_groups for s in g['locals'].get(name, []) if s['value'] == T]
        if len(hits) != 1:
            raise LinkError(f'external {name}: {len(hits)} linked symbols at {T:#x}')
        sym = hits[0]
        if spec['kind'] == 'address-only':
            got = 'object' if sym['type'] == 1 else 'notype' if sym['type'] == 0 else 'func'
            if got != spec['symbol_type']:
                raise LinkError(f'external {name} is a {got}, reviewed as {spec["symbol_type"]}')
            self.facts['externals'][name] = {'kind': 'address-only', 'symbol_type': got}
        elif spec['kind'] == 'leaf':
            code = self.elf.read(sym['value'], sym['size'])
            pos = 0
            while pos < len(code):
                ins = decode(code, pos, sym['value'] + pos)
                if 'rip_field' in ins or ('branch_target' in ins and not
                                         sym['value'] <= ins['branch_target'] < sym['value'] + sym['size']):
                    raise LinkError(f'external {name} is not a leaf (reference at {ins["va"]:#x})')
                pos += ins['len']
            digest = hashlib.sha256(code).hexdigest()
            if digest != spec['sha256']:
                raise LinkError(f'external leaf {name} body {digest} is not the reviewed {spec["sha256"]}')
            self.facts['externals'][name] = {'kind': 'leaf', 'sha256': digest}
        else:
            raise LinkError(f'external {name}: unknown kind {spec["kind"]}')

    def reach_defined(self, on, y, P, F, A, kind, pending, k=None):
        """Target address for a relocation against a symbol defined in object `on` (k: distance
        from the relocated field to the end of its instruction, for section-symbol targets)."""
        o = self.objects[on]
        if y['bind'] != STB_LOCAL and y['type'] != STT_SECTION:
            T = (P + F - A) if kind == 'pc' else None
            pending.append(('name', y['name'], T, kind))
            return ('name',)
        t = o.sections[y['shndx']]
        if t['flags'] & SHF_MERGE:
            if y['type'] == STT_SECTION and kind == 'pc':
                raise LinkError(f'PC-relative section-symbol reference into merged {t["name"]}')
            return ('merge', t, y['value'] + (A if y['type'] == STT_SECTION else 0))
        if self.is_prov_code(on, y['shndx']):
            # other-object code outside COMDAT: function units, placed by their own symbols
            if y['type'] == STT_SECTION:
                if kind == 'pc' and k is None:
                    raise LinkError(f'{on}: section-relative code reference without an instruction context')
                off = A + (k if kind == 'pc' else 0)
            else:
                off = y['value']
            fn = self.fn_at(on, y['shndx'], off)
            pending.append(('unit', (on, y['shndx'], fn['value'], fn['value'] + fn['size'])))
            virt = self.fn_addr(on, fn) - fn['value']   # where offset 0 of the section would be
            return ('addr', virt + y['value'])
        b = self.place(on, y['shndx'])
        pending.append(('unit', (on, y['shndx'])))
        if b is not None:
            return ('addr', b + y['value'])
        if kind == 'pc':
            pending.append(('derive', (on, y['shndx']), P + F - A - y['value']))
            return ('derived',)
        return ('unplaced', y['shndx'], y['value'])

    def check_merge(self, o, t, item_off, S):
        data = o.section_bytes(t['index'])
        if t['flags'] & SHF_STRINGS:
            item = data[item_off:data.index(b'\0', item_off) + 1]
        else:
            e = t['entsize']
            if item_off % e:
                raise LinkError(f'unaligned constant reference into {t["name"]}')
            item = data[item_off:item_off + e]
        if self.elf.read(S, len(item)) != item:
            raise LinkError(f'merged item {t["name"]}+{item_off:#x} differs at {S:#x}')
        self.facts['merge_items'] += 1
        return item

    # -- units -------------------------------------------------------------------------------
    def unit_relocs(self, unit):
        on, idx = unit[0], unit[1]
        o = self.objects[on]
        rs = o.relocs.get(idx, [])
        if len(unit) == 4:
            rs = [r for r in rs if unit[2] <= r['offset'] < unit[3]]
        return o, rs

    def expected(self, unit, sbase, pending, sym_desc):
        o, rs = self.unit_relocs(unit)
        on, idx = unit[0], unit[1]
        start, end = (unit[2], unit[3]) if len(unit) == 4 else (0, o.sections[idx]['size'])
        data = bytearray(o.section_bytes(idx)[start:end])
        ends, intra = {}, []
        if o.sections[idx]['flags'] & SHF_EXECINSTR:
            code = o.section_bytes(idx)
            relocated = {r['offset'] for r in rs}
            pos = start
            while pos < end:
                try:
                    ins = decode(code, pos, pos)
                except DecodeError as e:
                    raise LinkError(f'{on}: {e}')
                for f in ('branch_field', 'rip_field'):
                    if f in ins:
                        fo = pos + ins[f][0]
                        ends[fo] = pos + ins['len']
                        tgt = ins['branch_target'] if f == 'branch_field' else ins['rip_target']
                        if fo not in relocated and not start <= tgt < end:
                            if len(unit) != 4:
                                raise LinkError(f'{on}:{o.sections[idx]["name"]}: unrelocated reference leaves the section')
                            intra.append((fo, ins[f][1], pos + ins['len'], tgt))
                pos += ins['len']
            if pos != end:
                raise LinkError(f'{on}: decoding overruns the function range')
        for fo, size, iend, tgt in intra:
            # an unrelocated branch to another function of the same section: the linked target must
            # be that named function (+offset), wherever this link placed it
            fn = self.fn_at(on, idx, tgt)
            T = self.fn_addr(on, fn) + (tgt - fn['value'])
            P_end = sbase + iend
            if size != 4:
                raise LinkError(f'{on}: short branch leaves its function at +{fo:#x}')
            struct.pack_into('<i', data, fo - start, s32(T - P_end))
            sym_desc.append([fo - start, 'intra', 0, ['intra', fn['name'], tgt - fn['value']]])
            pending.append(('unit', (on, idx, fn['value'], fn['value'] + fn['size'])))
        for r in rs:
            typ, off, A = r['type'], r['offset'], r['addend']
            y = o.symbols[r['symbol']]
            P = sbase + off
            w = off - start
            self.facts['relocations'] += 1
            self.facts['by_type'][R_NAMES.get(typ, typ)] = self.facts['by_type'].get(R_NAMES.get(typ, typ), 0) + 1
            desc = None
            if typ in (2, 4):
                F = s32(struct.unpack('<I', self.elf.read(P, 4))[0])
                if y['shndx'] == SHN_UNDEF:
                    pending.append(('name', y['name'], P + F - A, 'pc'))
                    desc = ['name', y['name']]
                else:
                    tgt = self.reach_defined(on, y, P, F, A, 'pc', pending, ends.get(off, None) and ends[off] - off)
                    if tgt[0] == 'addr':
                        F = tgt[1] + A - P
                        desc = ['local', o.sections[y['shndx']]['name'], y['name'], y['value']]
                    elif tgt[0] == 'merge':
                        desc = ['merge', self.check_merge(o, tgt[1], tgt[2], P + F - A).hex()]
                    elif tgt[0] == 'name':
                        desc = ['name', y['name']]
                    else:
                        desc = ['local', o.sections[y['shndx']]['name'], y['name'], y['value']]
                struct.pack_into('<i', data, w, s32(F))
            elif typ in (9, 41, 42):
                F = s32(struct.unpack('<I', self.elf.read(P, 4))[0])
                op_o, op_l = data[w - 2], self.elf.read(P - 2, 1)[0]
                T = P + F - A
                if op_l == op_o:
                    rel = self.L.dyn_at(T)
                    if y['shndx'] == SHN_UNDEF:
                        if not (rel and rel['type'] in (R_X86_64_GLOB_DAT, 1) and rel['name'] == y['name']):
                            raise LinkError(f'GOT slot {T:#x} is not the slot of {y["name"]}: {rel}')
                        self.facts['imports'].add(y['name'])
                        desc = ['got', y['name']]
                    else:
                        raise LinkError(f'unrelaxed GOT reference to defined {y["name"]} not reviewed')
                elif typ in (41, 42) and op_o == 0x8B and op_l == 0x8D:
                    self.facts['relaxed'] += 1
                    data[w - 2] = 0x8D
                    if y['shndx'] == SHN_UNDEF:
                        pending.append(('name', y['name'], T, 'abs'))
                        desc = ['relaxed', y['name']]
                    else:
                        tgt = self.reach_defined(on, y, P, F, A, 'pc', pending)
                        if tgt[0] == 'addr' and tgt[1] != T:
                            raise LinkError(f'relaxed reference to {y["name"]} does not reach it')
                        desc = ['relaxed', y['name']]
                else:
                    raise LinkError(f'unsupported GOTPCRELX rewrite {op_o:#x}->{op_l:#x} at {P:#x}')
                struct.pack_into('<i', data, w, F)
            elif typ == 1:
                rel = self.L.dyn_at(P)
                if y['shndx'] == SHN_UNDEF:
                    if not (rel and rel['type'] in (1, R_X86_64_GLOB_DAT) and rel['name'] == y['name']
                            and rel['addend'] == A):
                        raise LinkError(f'64-bit reference to {y["name"]} at {P:#x}: dynamic relocation {rel}')
                    self.facts['imports'].add(y['name'])
                    struct.pack_into('<Q', data, w, 0)
                    desc = ['abs-import', y['name']]
                else:
                    if not (rel and rel['type'] == R_X86_64_RELATIVE):
                        raise LinkError(f'64-bit reference at {P:#x} has no RELATIVE relocation: {rel}')
                    t = o.sections[y['shndx']]
                    if t['flags'] & SHF_MERGE:
                        desc = ['merge', self.check_merge(o, t, y['value'] + A, rel['addend']).hex()]
                    elif y['bind'] != STB_LOCAL and y['type'] != STT_SECTION:
                        pending.append(('name', y['name'], rel['addend'] - A, 'abs'))
                        desc = ['name', y['name']]
                    else:
                        tgt = self.reach_defined(on, y, P, 0, A, 'abs', pending)
                        if tgt[0] == 'addr':
                            if rel['addend'] != tgt[1] + A:
                                raise LinkError(f'64-bit reference at {P:#x} is RELATIVE to {rel["addend"]:#x}, '
                                                f'not {tgt[1] + A:#x}')
                        else:
                            pending.append(('derive', (on, y['shndx']), rel['addend'] - A - y['value']))
                        desc = ['local', t['name'], y['name'], y['value']]
                    struct.pack_into('<Q', data, w, rel['addend'] & 0xFFFFFFFFFFFFFFFF)
            else:
                raise LinkError(f'relocation type {typ} in {on}:{o.sections[idx]["name"]} not supported')
            sym_desc.append([off - start, typ, A, desc])
        return bytes(data)

    def intra_edges(self, unit, sbase):
        """Function-range units: branch/RIP targets inside the same section, outside the range."""
        on, idx, start, end = unit
        o = self.objects[on]
        code = self.elf.read(sbase + start, end - start)
        relocated = {r['offset'] - start for r in o.relocs.get(idx, []) if start <= r['offset'] < end}
        out, pos = [], 0
        while pos < len(code):
            try:
                ins = decode(code, pos, sbase + start + pos)
            except DecodeError as e:
                raise LinkError(f'{on}: {e}')
            for f in ('branch_field', 'rip_field'):
                if f in ins and pos + ins[f][0] not in relocated:
                    t = ins['branch_target'] if f == 'branch_field' else ins['rip_target']
                    if not sbase + start <= t < sbase + end:
                        off = t - sbase
                        fn = [y for y in named(o, idx) if y['type'] == STT_FUNC and y['value'] <= off < y['value'] + y['size']]
                        if len(fn) != 1:
                            raise LinkError(f'{on}: unrelocated reference at {ins["va"]:#x} leaves its function')
                        out.append((on, idx, fn[0]['value'], fn[0]['value'] + fn[0]['size']))
            pos += ins['len']
        return out

    def try_unit(self, unit, implied_base=None):
        """Verify a unit; commit its side effects only on success. Returns True/False."""
        if unit in self.done:
            return True
        on, idx = unit[0], unit[1]
        o = self.objects[on]
        s = o.sections[idx]
        if s['flags'] & SHF_MERGE or s['name'] in SKIP_SECTIONS:
            self.done[unit] = None
            return True
        if len(unit) == 4 and self.is_prov_code(on, idx):
            sbase = self.fn_addr(on, self.fn_at(on, idx, unit[2])) - unit[2]
        else:
            sbase = self.place(on, idx) if implied_base is None else implied_base
        if sbase is None:
            b = self.derived.get((on, idx))
            if not b:
                return False
            if len(b) != 1:
                raise LinkError(f'{on}:{s["name"]}: references disagree on its address')
            sbase = next(iter(b))
        pending, desc = [], []
        try:
            exp = self.expected(unit, sbase, pending, desc)
            start = unit[2] if len(unit) == 4 else 0
            if s['type'] != SHT_NOBITS:
                got = self.elf.read(sbase + start, len(exp)) if exp else b''
                if got != exp:
                    return False
        except (ElfError, LinkError):
            if implied_base is not None:
                return False
            raise
        if len(unit) == 4 and self.is_prov_code(on, idx):
            self.unit_base[unit] = sbase
        else:
            self.base[(on, idx)] = sbase
        digest = None
        if on != self.home:
            start, end = (unit[2], unit[3]) if len(unit) == 4 else (0, s['size'])
            canon = {'section': s['name'], 'range': [start, end] if len(unit) == 4 else None,
                     'bytes_sha256': hashlib.sha256(o.section_bytes(idx)[start:end]).hexdigest(),
                     'relocations': desc}
            digest = hashlib.sha256(json.dumps(canon, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        self.done[unit] = digest
        for p in pending:
            if p[0] == 'unit':
                self.reached.add(p[1])
            elif p[0] == 'derive':
                self.derived.setdefault(p[1], set()).add(p[2])
                self.reached.add(p[1])
            else:
                self.resolve_name(p[1], p[2], p[3])
        return True

    def prove(self):
        o = self.objects[self.home]
        roots = sorted({y['shndx'] for y in o.symbols if y['bind'] != STB_LOCAL and y['shndx'] not in (SHN_UNDEF, SHN_ABS)
                        and o.group_of(y['shndx']) is None})
        roots += [s['index'] for s in o.sections if s['name'] in ('.init_array', '.preinit_array', '.fini_array')]
        self.reached.update((self.home, r) for r in roots)
        while True:
            progress = False
            for unit in sorted(self.reached - set(self.done), key=str):
                if self.try_unit(unit):
                    progress = True
                elif self.place(unit[0], unit[1]) is not None or self.derived.get((unit[0], unit[1])):
                    raise LinkError(f'{unit[0]}:{self.objects[unit[0]].sections[unit[1]]["name"]}: '
                                    'linked bytes are not the relocated object bytes')
            if not progress:
                break
        missing = [(u[0], self.objects[u[0]].sections[u[1]]['name']) for u in self.reached - set(self.done)]
        if missing:
            raise LinkError(f'referenced sections never placed: {missing[:5]}')
        for key, b in self.derived.items():
            if len(b) != 1 or self.base.get(key) != next(iter(b)):
                raise LinkError(f'{key[0]}:{self.objects[key[0]].sections[key[1]]["name"]}: inconsistent derived address')
        home_units = sorted(self.objects[self.home].sections[u[1]]['name'] for u in self.done if u[0] == self.home)
        provider = {}
        for u, d in self.done.items():
            if u[0] != self.home and d is not None:
                s = self.objects[u[0]].sections[u[1]]
                fn = [y['name'] for y in named(self.objects[u[0]], u[1]) if len(u) == 4 and y['value'] == u[2]]
                key = fn[0] if fn else s['name']
                provider[key] = d
        f = self.facts
        return {'home_sections': home_units, 'provider_units': provider,
                'provider_objects': sorted(set(f['providers'].values()) - {self.home}),
                'imports': sorted(f['imports']), 'externals': f['externals'],
                'relocations': f['relocations'], 'by_type': f['by_type'], 'merge_items': f['merge_items'],
                'relaxed': f['relaxed'], 'skipped': sorted(SKIP_SECTIONS),
                'home_bases': {self.objects[self.home].sections[k[1]]['name']: v for k, v in self.base.items()
                               if k[0] == self.home}}
