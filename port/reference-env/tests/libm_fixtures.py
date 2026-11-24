# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic libm ELF files, with real x86-64 code, for the dispatch tests.

synthetic_libm() builds a small libm and debug file: for every checked
function an SSE2 and an FMA implementation, an IFUNC resolver, and for the
wrapped functions a wrapper that reaches its SSE2 implementation through an
exact .plt.sec stub and an IRELATIVE slot. It returns a lock whose libm
entries name these files, the facts derived from them and one valid traced
record per function. Nothing here is a runtime observation.
"""

import copy
import hashlib
import struct
from types import SimpleNamespace

WRAPPED = ("atan2", "exp", "log", "pow")  # FUNC wrappers in glibc 2.39 x86-64
TAIL_CALL = ("log",)                      # wrappers that jump (not call) to the stub
PLT_START = 0x10000
TEXT_START = 0x20000
GOT_START = 0xE0000
BUILD_ID = "5e1f0000c0de0000aa55aa55aa55aa55aa55aa55"
RETURN = "!/out/bin/primitive_probe+0x1234"
# endbr64; mov %rax,%rax; ret
IMPLEMENTATION = bytes.fromhex("f30f1efa" "4889c0" "c3")
FUNC, IFUNC = 2, 10


def probe_elf():
    """Synthetic caller with executable bytes at RETURN, not at the ELF header."""
    return build_elf([{"name": ".text", "type": 1, "flags": 6, "addr": 0x1200,
                       "data": b"\x90" * 0x40}])


def hx(value):
    return f"0x{value:x}"


def libm_sha256(lock):
    return next(f["sha256"] for f in lock["toolchain_files"] if f["resolved_path"] == lock["libm"]["path"])


def implementation_name(function, isa):
    return f"__ieee754_{function}_{isa}" if function in WRAPPED else f"__{function}_{isa}"


def synthetic_libm(real_lock, functions):
    """A synthetic libm, its debug file, a matching lock, derived facts and valid traced records."""
    import libm_elf  # the probe directory is on sys.path in every test module

    text = bytearray()
    layout = {}

    def place(code, align=0x20):
        while len(text) % align:
            text.append(0xCC)
        address = TEXT_START + len(text)
        text.extend(code)
        return address

    debug_symbols = []
    dynamic = []
    stubs = []
    relocations = []
    for index, function in enumerate(functions):
        sse2 = place(IMPLEMENTATION)
        fma = place(IMPLEMENTATION)
        resolver = place(b"\xc3")
        dispatcher = implementation_name(function, "sse2")[:-len("_sse2")]
        debug_symbols += [(implementation_name(function, "sse2"), sse2, len(IMPLEMENTATION), FUNC, 0, 2),
                          (implementation_name(function, "fma"), fma, len(IMPLEMENTATION), FUNC, 0, 2),
                          (dispatcher, resolver, 1, IFUNC, 0, 2)]
        entry = {"sse2": sse2, "fma": fma, "resolver": resolver}
        if function in WRAPPED:
            stub = PLT_START + 16 * len(stubs)
            slot = GOT_START + 8 * len(stubs)
            stubs.append((stub, slot))
            relocations.append((slot, 37, 0, resolver))
            wrapper_at = TEXT_START + len(text) + (-len(text)) % 0x20
            if function in TAIL_CALL:
                code = bytes.fromhex("f30f1efa") + b"\xe9" + struct.pack("<i", stub - (wrapper_at + 9))
            else:
                code = bytes.fromhex("f30f1efa") + b"\xe8" + struct.pack("<i", stub - (wrapper_at + 9)) + b"\xc3"
            wrapper = place(code)
            assert wrapper == wrapper_at
            compat = place(code)  # never entered by a valid trace
            debug_symbols += [("__" + function, wrapper, len(code), FUNC, 0, 2),
                              (function, wrapper, len(code), FUNC, 1, 2),
                              ("__" + function + "_compat", compat, len(code), FUNC, 0, 2)]
            dynamic += [(function, compat, len(code), FUNC, 2 | 0x8000), (function, wrapper, len(code), FUNC, 3)]
            entry.update(wrapper=wrapper, compat=compat, stub=stub, slot=slot, wrapper_size=len(code))
        else:
            dynamic.append((function, resolver, 1, IFUNC, 2))
        layout[function] = entry
    libm = libm_elf_bytes(BUILD_ID, dynamic, {2: "GLIBC_2.2.5", 3: "GLIBC_2.29"}, stubs, relocations,
                          text=(TEXT_START, bytes(text)), got=(GOT_START, bytes(8 * max(1, len(stubs)))))
    debug = debug_elf(BUILD_ID, debug_symbols)
    table, _excluded = libm_elf.multiarch_symbols(libm_elf.Elf64(debug))
    lock = copy.deepcopy(real_lock)
    lock["libm"].update(build_id=BUILD_ID, debug_file_sha256=hashlib.sha256(debug).hexdigest(),
                        multiarch_symbols=table)
    for item in lock["toolchain_files"]:
        if item["resolved_path"] == lock["libm"]["path"]:
            item["sha256"] = hashlib.sha256(libm).hexdigest()
            item["size"] = len(libm)
    facts = libm_elf.dispatch_facts(libm, debug, functions)
    records = [valid_record(function, layout[function], lock) for function in functions]
    return SimpleNamespace(libm=libm, debug=debug, lock=lock, facts=facts, records=records, layout=layout,
                           elf=libm_elf.Elf64(libm))


def valid_record(function, entry, lock):
    """The trace a correct run of the synthetic code produces."""
    sse2 = entry["sse2"]
    body = [hx(sse2), hx(sse2 + 4), hx(sse2 + 7)]
    record = {"function": function, "object": lock["libm"]["path"], "linked_object": lock["libm"]["path"],
              "inputs": ["3fe0000000000000", "0000000000000000"], "trace_status": "returned",
              "return_address": RETURN, "return_to": RETURN, "slot_reads": []}
    if "wrapper" in entry:
        wrapper, stub = entry["wrapper"], entry["stub"]
        trace = [hx(wrapper), hx(wrapper + 4), hx(stub), hx(stub + 4)] + body
        if function not in TAIL_CALL:
            trace.append(hx(wrapper + 9))
        record.update(offset=hx(wrapper), linked_offset=hx(wrapper), trace=trace,
                      slot_reads=[{"at": hx(stub + 4), "slot": hx(entry["slot"]), "value": hx(sse2)}])
    else:
        record.update(offset=hx(sse2), linked_offset=hx(sse2), trace=body)
    return record


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


def libm_elf_bytes(build_id, dynamic, versions, plt, relocations, text=None, got=None):
    """dynamic: (name, value, size, type, version index | 0x8000 hidden). versions: index -> name.

    plt: list of (stub, slot) or raw 16-byte entries at consecutive addresses from the first stub.
    relocations: (offset, type, symbol, addend). text and got: optional (address, bytes).
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
    ] + ([{"name": ".text", "type": 1, "data": text[1], "addr": text[0], "flags": 6}] if text else [])
      + ([{"name": ".got", "type": 1, "data": got[1], "addr": got[0], "flags": 3}] if got else []))
