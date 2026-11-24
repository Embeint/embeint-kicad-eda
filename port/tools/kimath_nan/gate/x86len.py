# SPDX-License-Identifier: GPL-3.0-or-later
"""Minimal x86-64 instruction length decoder (no third-party modules).

Decodes the legacy/REX encodings GCC emits for x86-64 baseline code: prefixes, the one-byte
map, the 0F map, the 0F38/0F3A maps, ModRM/SIB/displacement and immediates. It reports, per
instruction, its length, a branch target (rel8/rel32) and the position and target of a
RIP-relative displacement. VEX/EVEX (C4/C5/62 as prefixes), 3DNow! and anything it does not
know raise DecodeError: the caller fails closed. Validated against GNU objdump listings.
"""
import struct


class DecodeError(Exception):
    pass


PREFIXES = {0xF0, 0xF2, 0xF3, 0x2E, 0x36, 0x3E, 0x26, 0x64, 0x65, 0x66, 0x67}


def _one_byte(op):
    """(has_modrm, imm) for the one-byte map; imm: 0, 1, 2, 'z' (4 or 2), 'v' (8/4/2), 'rel8',
    'rel32', 'moffs', 'enter', None = invalid."""
    if op < 0x40:
        lo = op & 7
        if op in (0x0F, 0x26, 0x2E, 0x36, 0x3E) or lo in (6, 7) and op not in (0x0F,):
            return None
        if lo < 4:
            return (True, 0)
        return (False, 1) if lo == 4 else (False, 'z')
    if 0x50 <= op <= 0x5F:
        return (False, 0)
    table = {0x63: (True, 0), 0x68: (False, 'z'), 0x69: (True, 'z'), 0x6A: (False, 1), 0x6B: (True, 1),
             0x6C: (False, 0), 0x6D: (False, 0), 0x6E: (False, 0), 0x6F: (False, 0),
             0x80: (True, 1), 0x81: (True, 'z'), 0x83: (True, 1),
             0xA8: (False, 1), 0xA9: (False, 'z'), 0xC0: (True, 1), 0xC1: (True, 1), 0xC2: (False, 2),
             0xC3: (False, 0), 0xC6: (True, 1), 0xC7: (True, 'z'), 0xC8: (False, 'enter'), 0xC9: (False, 0),
             0xCA: (False, 2), 0xCB: (False, 0), 0xCC: (False, 0), 0xCD: (False, 1), 0xCF: (False, 0),
             0xE8: (False, 'rel32'), 0xE9: (False, 'rel32'), 0xEB: (False, 'rel8'),
             0xF4: (False, 0), 0xF5: (False, 0), 0xF6: (True, 'f6'), 0xF7: (True, 'f7'),
             0xFE: (True, 0), 0xFF: (True, 0)}
    if op in table:
        return table[op]
    if 0x70 <= op <= 0x7F or 0xE0 <= op <= 0xE3:
        return (False, 'rel8')
    if 0x84 <= op <= 0x8F:
        return (True, 0)
    if 0x90 <= op <= 0x9F:
        return (False, 0)
    if 0xA0 <= op <= 0xA3:
        return (False, 'moffs')
    if 0xA4 <= op <= 0xAF:
        return (False, 0)
    if 0xB0 <= op <= 0xB7:
        return (False, 1)
    if 0xB8 <= op <= 0xBF:
        return (False, 'v')
    if 0xD0 <= op <= 0xD3 or 0xD8 <= op <= 0xDF:
        return (True, 0)
    if 0xE4 <= op <= 0xE7:
        return (False, 1)
    if 0xEC <= op <= 0xEF or 0xF8 <= op <= 0xFD:
        return (False, 0)
    return None


def _two_byte(op):
    if op in (0x05, 0x06, 0x07, 0x08, 0x09, 0x0B, 0x31, 0x77, 0xA0, 0xA1, 0xA2, 0xA8, 0xA9) or 0xC8 <= op <= 0xCF:
        return (False, 0)
    if 0x80 <= op <= 0x8F:
        return (False, 'rel32')
    if op in (0x70, 0x71, 0x72, 0x73, 0xA4, 0xAC, 0xBA, 0xC2, 0xC4, 0xC5, 0xC6):
        return (True, 1)
    if op in (0x00, 0x01, 0x0D, 0xA3, 0xA5, 0xAB, 0xAD, 0xAE, 0xAF) or 0x10 <= op <= 0x1F or 0x28 <= op <= 0x2F \
            or 0x40 <= op <= 0x6F or 0x74 <= op <= 0x76 or 0x7C <= op <= 0x7F or 0x90 <= op <= 0x9F \
            or 0xB0 <= op <= 0xB9 or 0xBB <= op <= 0xC1 or op in (0xC3, 0xC7) or 0xD0 <= op <= 0xFE:
        return (True, 0)
    return None


def decode(buf, pos, va):
    """Decodes one instruction at buf[pos] (virtual address va). Returns a dict."""
    start = pos
    opsize16 = addr32 = False
    rex = 0
    while pos < len(buf) and buf[pos] in PREFIXES:
        if buf[pos] == 0x66:
            opsize16 = True
        if buf[pos] == 0x67:
            addr32 = True
        pos += 1
    prefix_end = pos
    if pos < len(buf) and 0x40 <= buf[pos] <= 0x4F:
        rex = buf[pos]
        pos += 1
    if pos >= len(buf):
        raise DecodeError(f'truncated at {va:#x}')
    op = buf[pos]
    pos += 1
    if op in (0xC4, 0xC5, 0x62):
        raise DecodeError(f'VEX/EVEX at {va:#x} not supported')
    if op == 0x0F:
        op2 = buf[pos]
        pos += 1
        if op2 == 0x38:
            pos += 1
            info, opmap = (True, 0), '0f38'
        elif op2 == 0x3A:
            pos += 1
            info, opmap = (True, 1), '0f3a'
        else:
            info, opmap = _two_byte(op2), '0f'
        opcode = (opmap, op2)
    else:
        info, opmap, opcode = _one_byte(op), '1', ('1', op)
    if info is None:
        raise DecodeError(f'unknown opcode {opcode} at {va:#x}')
    has_modrm, imm = info
    rip_disp_at = None
    modrm_reg = None
    mod = None
    if has_modrm:
        modrm = buf[pos]
        pos += 1
        mod, modrm_reg, rm = modrm >> 6, (modrm >> 3) & 7, modrm & 7
        if mod != 3:
            if rm == 4:
                sib = buf[pos]
                pos += 1
                if mod == 0 and (sib & 7) == 5:
                    pos += 4
            elif mod == 0 and rm == 5:
                rip_disp_at = pos - start
                pos += 4
            if mod == 1:
                pos += 1
            elif mod == 2:
                pos += 4
    size = 0
    branch = None
    if imm == 'f6':
        size = 1 if modrm_reg in (0, 1) else 0
    elif imm == 'f7':
        size = (2 if opsize16 else 4) if modrm_reg in (0, 1) else 0
    elif imm == 'z':
        size = 2 if opsize16 else 4
    elif imm == 'v':
        size = 8 if rex & 8 else (2 if opsize16 else 4)
    elif imm == 'moffs':
        size = 4 if addr32 else 8
    elif imm == 'enter':
        size = 3
    elif imm == 'rel8':
        size = 1
        branch = 1
    elif imm == 'rel32':
        size = 4
        branch = 4
    else:
        size = imm
    imm_at = pos - start
    pos += size
    length = pos - start
    if pos > len(buf):
        raise DecodeError(f'truncated at {va:#x}')
    out = {'va': va, 'len': length, 'bytes': bytes(buf[start:pos]), 'opcode': opcode, 'modrm_reg': modrm_reg,
           'modrm_mod': mod,
           'rex_w': bool(rex & 8), 'prefix_66': opsize16,
           'prefixes': bytes(buf[start:prefix_end])}
    if branch:
        rel = struct.unpack_from('<b' if branch == 1 else '<i', buf, start + imm_at)[0]
        out['branch_target'] = va + length + rel
        out['branch_field'] = (imm_at, branch)
    if rip_disp_at is not None:
        disp = struct.unpack_from('<i', buf, start + rip_disp_at)[0]
        out['rip_target'] = va + length + disp
        out['rip_field'] = (rip_disp_at, 4)
    return out
