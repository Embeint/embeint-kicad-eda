# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic libm ELF files, dispatch facts and traces for the dispatch tests.

Offsets of the multiarch implementations are the lock's real ones; wrappers,
PLT stubs, GOT slots and resolvers are synthetic. Nothing here is a runtime
observation.
"""

import struct

WRAPPED = ("atan2", "exp", "log", "pow")  # FUNC wrappers in glibc 2.39 x86-64
PLT_START = 0x10000
WRAPPER_START = 0x90000
RESOLVER_START = 0xA0000
SLOT_START = 0xE0000
IMPLEMENTATION_SIZE = 0x10


def hx(value):
    return f"0x{value:x}"


def expected(function, lock):
    table = lock["libm"]["multiarch_symbols"]
    wanted = {f"__{function}_sse2", f"__ieee754_{function}_sse2"}
    offset = next(o for o, names in table.items() if wanted.intersection(names))
    name = next(n for n in table[offset] if n in wanted)
    return int(offset, 16), name


def libm_sha256(lock):
    return next(f["sha256"] for f in lock["toolchain_files"] if f["resolved_path"] == lock["libm"]["path"])


def facts_and_records(lock, functions):
    """Facts consistent with the lock and one passing traced record per function."""
    table = lock["libm"]["multiarch_symbols"]
    facts = {
        "schema": 1, "libm_sha256": libm_sha256(lock), "libm_build_id": lock["libm"]["build_id"],
        "debug_sha256": lock["libm"]["debug_file_sha256"], "debug_build_id": lock["libm"]["build_id"],
        "definitions": {}, "plt_sec": {"start": hx(PLT_START), "end": hx(PLT_START + 16 * len(functions))},
        "plt_stubs": [], "ifuncs": {}, "functions": [], "multiarch_symbols": table,
        "excluded_suffix_matches": [],
    }
    for offset, names in table.items():
        facts["functions"].append([offset, IMPLEMENTATION_SIZE, names])
    records = []
    for index, function in enumerate(functions):
        target, target_name = expected(function, lock)
        resolver = RESOLVER_START + 0x100 * index
        facts["ifuncs"][hx(resolver)] = [target_name[:-len("_sse2")]]
        common = {"function": function, "object": lock["libm"]["path"], "linked_object": lock["libm"]["path"],
                  "inputs": ["3fe0000000000000", "0000000000000000"], "trace_status": "returned"}
        if function in WRAPPED:
            wrapper = WRAPPER_START + 0x100 * index
            stub = PLT_START + 16 * index
            slot = SLOT_START + 8 * index
            facts["definitions"][function] = [
                {"value": hx(wrapper + 0x80), "size": 0x40, "type": "FUNC", "version": "GLIBC_2.2.5",
                 "default": False},
                {"value": hx(wrapper), "size": 0x40, "type": "FUNC", "version": "GLIBC_2.29", "default": True}]
            facts["functions"].append([hx(wrapper), 0x40, ["__" + function, function]])
            facts["functions"].append([hx(wrapper + 0x80), 0x40, ["__" + function + "_compat"]])
            facts["plt_stubs"].append({
                "stub": hx(stub), "template": True, "slot": hx(slot), "bytes": "synthetic",
                "relocations": [{"type": "R_X86_64_IRELATIVE", "symbol": 0, "addend": hx(resolver)}]})
            trace = [hx(wrapper), hx(wrapper + 4), hx(stub), hx(stub + 4), hx(target), hx(target + 4),
                     hx(wrapper + 9)]
            records.append({**common, "offset": hx(wrapper), "linked_offset": hx(wrapper), "trace": trace})
        else:
            facts["definitions"][function] = [
                {"value": hx(resolver), "size": 0x30, "type": "IFUNC", "version": "GLIBC_2.2.5", "default": True}]
            facts["plt_stubs"].append({
                "stub": hx(PLT_START + 16 * index), "template": True, "slot": hx(SLOT_START + 8 * index),
                "bytes": "synthetic", "relocations": [
                    {"type": "R_X86_64_IRELATIVE", "symbol": 0, "addend": hx(resolver)}]})
            trace = [hx(target), hx(target + 4), hx(target + 8)]
            records.append({**common, "offset": hx(target), "linked_offset": hx(target), "trace": trace})
    return facts, records


# --- synthetic ELF files -------------------------------------------------------------------

def strings(names):
    table = b"\0"
    offsets = {}
    for name in names:
        if name not in offsets:
            offsets[name] = len(table)
            table += name.encode() + b"\0"
    return table, offsets


def symbol_table(symbols):
    """symbols: (name, value, size, type, bind, shndx). Returns (symtab bytes, strtab bytes)."""
    table, offsets = strings([s[0] for s in symbols])
    data = b"\0" * 24
    for name, value, size, kind, bind, shndx in symbols:
        data += struct.pack("<IBBHQQ", offsets[name], (bind << 4) | kind, 0, shndx, value, size)
    return data, table


def build_id_note(build_id):
    desc = bytes.fromhex(build_id)
    return struct.pack("<III", 4, len(desc), 3) + b"GNU\0" + desc


def build_elf(sections):
    """sections: dicts name type data [addr link info entsize flags]; link names a section."""
    names = [s["name"] for s in sections] + [".shstrtab"]
    shstrtab, name_offsets = strings(names)
    sections = sections + [{"name": ".shstrtab", "type": 3, "data": shstrtab}]
    index = {s["name"]: i + 1 for i, s in enumerate(sections)}
    body = b""
    placed = []
    for section in sections:
        while (64 + len(body)) % 8:
            body += b"\0"
        placed.append(64 + len(body))
        body += section["data"]
    while (64 + len(body)) % 8:
        body += b"\0"
    shoff = 64 + len(body)
    headers = b"\0" * 64
    for section, offset in zip(sections, placed):
        headers += struct.pack("<IIQQQQIIQQ", name_offsets[section["name"]], section["type"],
                               section.get("flags", 0), section.get("addr", 0), offset, len(section["data"]),
                               index[section["link"]] if section.get("link") else 0, section.get("info", 0),
                               8, section.get("entsize", 0))
    header = b"\x7fELF\x02\x01\x01" + b"\0" * 9 + struct.pack(
        "<HHIQQQIHHHHHH", 3, 62, 1, 0, 0, shoff, 0, 64, 0, 0, 64, len(sections) + 1, index[".shstrtab"])
    return header + body + headers


def debug_elf(build_id, symbols):
    symtab, strtab = symbol_table(symbols)
    return build_elf([
        {"name": ".note.gnu.build-id", "type": 7, "data": build_id_note(build_id)},
        {"name": ".strtab", "type": 3, "data": strtab},
        {"name": ".symtab", "type": 2, "data": symtab, "link": ".strtab", "entsize": 24},
    ])


def plt_sec_entry(stub, slot):
    return bytes.fromhex("f30f1efaff25") + struct.pack("<i", slot - (stub + 10)) + bytes.fromhex("660f1f440000")


def bnd_plt_sec_entry(stub, slot):
    """The BND form of an IBT stub; valid x86 but not the locked libm's form, so refused."""
    return bytes.fromhex("f30f1efaf2ff25") + struct.pack("<i", slot - (stub + 11)) + bytes.fromhex("0f1f440000")


def libm_elf_bytes(build_id, dynamic, versions, plt, relocations):
    """dynamic: (name, value, size, type, version index | 0x8000 hidden). versions: index -> name.

    plt: list of (stub, slot) or raw 16-byte entries at consecutive addresses from the first stub.
    relocations: (offset, type, symbol, addend).
    """
    symbols = [(name, value, size, kind, 1, 5) for name, value, size, kind, _v in dynamic]
    dynsym, dynstr = symbol_table(symbols)
    version_names = list(versions.values())
    dynstr_extra, extra = strings(version_names)
    # Append version names after the symbol names in one .dynstr.
    base = len(dynstr)
    dynstr += dynstr_extra[1:]
    name_offset = {name: base + extra[name] - 1 for name in version_names}
    versym = struct.pack("<H", 0) + b"".join(struct.pack("<H", v) for *_rest, v in dynamic)
    verdef = b""
    items = sorted(versions.items())
    for position, (ndx, name) in enumerate(items):
        following = 28 if position + 1 < len(items) else 0
        verdef += struct.pack("<HHHHIII", 1, 0, ndx, 1, 0, 20, following)
        verdef += struct.pack("<II", name_offset[name], 0)
    rela = b"".join(struct.pack("<QQq", offset, (symbol << 32) | kind, addend)
                    for offset, kind, symbol, addend in relocations)
    first = plt[0][0] if plt and isinstance(plt[0], tuple) else 0x1000
    code = b"".join(entry if isinstance(entry, bytes) else plt_sec_entry(*entry) for entry in plt)
    return build_elf([
        {"name": ".note.gnu.build-id", "type": 7, "data": build_id_note(build_id)},
        {"name": ".dynstr", "type": 3, "data": dynstr},
        {"name": ".dynsym", "type": 11, "data": dynsym, "link": ".dynstr", "entsize": 24, "info": 1},
        {"name": ".gnu.version", "type": 0x6FFFFFFF, "data": versym, "link": ".dynsym", "entsize": 2},
        {"name": ".gnu.version_d", "type": 0x6FFFFFFD, "data": verdef, "link": ".dynstr", "info": len(items)},
        {"name": ".rela.plt", "type": 4, "data": rela, "link": ".dynsym", "entsize": 24},
        {"name": ".plt.sec", "type": 1, "data": code, "addr": first, "flags": 6},
    ])
