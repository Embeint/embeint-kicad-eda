# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic libm ELF files for the classifier tests. Nothing here is a runtime observation."""

import struct


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

