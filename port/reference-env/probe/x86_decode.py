#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail-closed x86-64 instruction length and control-flow decoder (standard library only).

Only what the dispatch proof needs: the length of one instruction and, for a
control transfer, its kind and direct target. It accepts an explicit subset of
the legacy, 0F, 0F38 and 0F3A opcode maps (general-purpose and SSE/SSE2
encodings as used by libm's baseline code) and raises Unsupported for anything
else: VEX/EVEX (AVX, FMA), x87, string instructions, system instructions,
far/indirect calls and any indirect jump other than RIP-relative memory.
tests/test_dispatch.py cross-checks every accepted instruction of the locked
libm against binutils' objdump inside the pinned image.
"""

import struct

LEGACY_PREFIXES = frozenset((0x66, 0x67, 0xF2, 0xF3, 0x2E, 0x36, 0x3E, 0x26, 0x64, 0x65, 0xF0))
MAX_LENGTH = 15

SEQ, JMP, JCC, CALL, RET, JMP_RIP_MEMORY = "seq", "jmp", "jcc", "call", "ret", "jmp_rip_memory"


class Unsupported(ValueError):
    """The bytes are not an instruction this decoder accepts."""


class Instruction:
    __slots__ = ("address", "length", "kind", "target", "memory")

    def __init__(self, address, length, kind, target=None, memory=None):
        self.address = address
        self.length = length
        self.kind = kind
        self.target = target      # direct branch/call target
        self.memory = memory      # RIP-relative memory operand address of JMP_RIP_MEMORY

    @property
    def following(self):
        return self.address + self.length


def _one_byte(opcode):
    """(has_modrm, immediate) for accepted one-byte opcodes; immediate: 0, 1, 2, 'z' or 'v'."""
    if opcode < 0x40 and opcode & 7 < 6 and opcode not in (0x26, 0x2E, 0x36, 0x3E):
        low = opcode & 7
        return (True, 0) if low < 4 else (False, 1 if low == 4 else "z")
    table = {
        0x63: (True, 0), 0x68: (False, "z"), 0x69: (True, "z"), 0x6A: (False, 1), 0x6B: (True, 1),
        0x80: (True, 1), 0x81: (True, "z"), 0x83: (True, 1), 0x90: (False, 0), 0x98: (False, 0),
        0x99: (False, 0), 0xA8: (False, 1), 0xA9: (False, "z"), 0xC0: (True, 1), 0xC1: (True, 1),
        0xC6: (True, 1), 0xC7: (True, "z"), 0xC9: (False, 0), 0xD0: (True, 0), 0xD1: (True, 0),
        0xD2: (True, 0), 0xD3: (True, 0),
    }
    if opcode in table:
        return table[opcode]
    if 0x50 <= opcode <= 0x5F or 0x91 <= opcode <= 0x97:
        return False, 0
    if 0x84 <= opcode <= 0x8B or opcode == 0x8D:
        return True, 0
    if 0xB0 <= opcode <= 0xB7:
        return False, 1
    if 0xB8 <= opcode <= 0xBF:
        return False, "v"
    return None


def _two_byte(opcode):
    """(has_modrm, immediate bytes) for accepted 0F opcodes."""
    if opcode in (0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17) or 0x18 <= opcode <= 0x1F:
        return True, 0
    if 0x28 <= opcode <= 0x2F or 0x40 <= opcode <= 0x4F or 0x50 <= opcode <= 0x6F:
        return True, 0
    if 0x70 <= opcode <= 0x73:
        return True, 1
    if 0x74 <= opcode <= 0x76 or 0x7C <= opcode <= 0x7F or 0x90 <= opcode <= 0x9F:
        return True, 0
    if opcode in (0xA3, 0xA5, 0xAB, 0xAD, 0xAE, 0xAF, 0xB0, 0xB1, 0xB6, 0xB7, 0xB8, 0xBB, 0xBC, 0xBD, 0xBE,
                  0xBF, 0xC0, 0xC1, 0xC3):
        return True, 0
    if opcode in (0xA4, 0xAC, 0xBA, 0xC2, 0xC4, 0xC5, 0xC6):
        return True, 1
    if 0xD0 <= opcode <= 0xFE:
        return True, 0
    if 0xC8 <= opcode <= 0xCF:
        return False, 0
    return None


def _modrm_length(code, offset):
    """Bytes taken by ModRM, SIB and displacement starting at code[offset]; RIP-relative flag."""
    if offset >= len(code):
        raise Unsupported("truncated ModRM")
    modrm = code[offset]
    mod, rm = modrm >> 6, modrm & 7
    if mod == 3:
        return 1, False
    length = 1
    if rm == 4:
        if offset + 1 >= len(code):
            raise Unsupported("truncated SIB")
        base = code[offset + 1] & 7
        length += 1
        if mod == 0 and base == 5:
            return length + 4, False
    elif mod == 0 and rm == 5:
        return length + 4, True
    return length + (1 if mod == 1 else 4 if mod == 2 else 0), False


def decode(code, address):
    """Decode one instruction from code (bytes starting at address)."""
    position = 0
    prefixes = set()
    while position < len(code) and code[position] in LEGACY_PREFIXES:
        if code[position] == 0xF0:
            raise Unsupported("LOCK prefix is outside the supported decoder subset")
        if code[position] in prefixes or len(prefixes) >= 4:
            raise Unsupported("repeated or excessive prefixes")
        prefixes.add(code[position])
        position += 1
    rex = 0
    if position < len(code) and 0x40 <= code[position] <= 0x4F:
        rex = code[position]
        position += 1
    if position >= len(code):
        raise Unsupported("truncated instruction")
    opcode = code[position]
    position += 1
    if opcode in (0xC4, 0xC5, 0x62, 0x8F):
        raise Unsupported("VEX/EVEX/XOP encoding")
    operand16 = 0x66 in prefixes and not rex & 8

    def finish(length, kind=SEQ, target=None, memory=None):
        if length > MAX_LENGTH or length > len(code):
            raise Unsupported("instruction exceeds the available bytes")
        return Instruction(address, length, kind, target, memory)

    def relative(size):
        if operand16:
            raise Unsupported("16-bit relative branch")
        if position + size > len(code):
            raise Unsupported("truncated displacement")
        value = struct.unpack_from("<b" if size == 1 else "<i", code, position)[0]
        return position + size, address + position + size + value

    if opcode == 0x0F:
        if position >= len(code):
            raise Unsupported("truncated 0F opcode")
        second = code[position]
        position += 1
        if 0x80 <= second <= 0x8F:
            end, target = relative(4)
            return finish(end, JCC, target)
        if second in (0x38, 0x3A):
            if position >= len(code):
                raise Unsupported("truncated three-byte opcode")
            # Only these two legacy SSE encodings are in this decoder's scope.
            # The rest of either map is not a generic ModRM instruction family.
            third = code[position]
            if prefixes != {0x66} or (second, third) not in ((0x38, 0x00), (0x3A, 0x0B)):
                raise Unsupported("unlisted three-byte opcode or prefix")
            position += 1
            used, _rip = _modrm_length(code, position)
            return finish(position + used + (1 if second == 0x3A else 0))
        if second == 0x50 and (prefixes - {0x66} or position >= len(code) or code[position] >> 6 != 3):
            raise Unsupported("MOVMSK requires a supported prefix and register source")
        entry = _two_byte(second)
        if entry is None:
            raise Unsupported(f"unsupported opcode 0f {second:02x}")
        has_modrm, immediate = entry
        if has_modrm:
            used, _rip = _modrm_length(code, position)
            position += used
        return finish(position + immediate)
    if 0x70 <= opcode <= 0x7F or 0xE0 <= opcode <= 0xE3:
        end, target = relative(1)
        return finish(end, JCC, target)
    if opcode == 0xEB:
        end, target = relative(1)
        return finish(end, JMP, target)
    if opcode == 0xE9:
        end, target = relative(4)
        return finish(end, JMP, target)
    if opcode == 0xE8:
        end, target = relative(4)
        return finish(end, CALL, target)
    if opcode == 0xC3:
        return finish(position, RET)
    if opcode == 0xC2:
        return finish(position + 2, RET)
    if opcode in (0xF6, 0xF7, 0xFE, 0xFF):
        if position >= len(code):
            raise Unsupported("truncated ModRM")
        group = (code[position] >> 3) & 7
        used, rip = _modrm_length(code, position)
        end = position + used
        if opcode in (0xF6, 0xF7):
            if group in (0, 1):
                end += 1 if opcode == 0xF6 else (2 if operand16 else 4)
            return finish(end)
        if opcode == 0xFE:
            if group > 1:
                raise Unsupported("unsupported FE group")
            return finish(end)
        if group in (0, 1, 6):
            return finish(end)
        if group == 4 and rip and code[position] >> 6 == 0:
            if end > len(code):
                raise Unsupported("truncated RIP-relative displacement")
            (displacement,) = struct.unpack_from("<i", code, position + 1)
            return finish(end, JMP_RIP_MEMORY, memory=address + end + displacement)
        raise Unsupported("unsupported indirect transfer or FF group")
    entry = _one_byte(opcode)
    if entry is None:
        raise Unsupported(f"unsupported opcode {opcode:02x}")
    has_modrm, immediate = entry
    if has_modrm:
        if position >= len(code):
            raise Unsupported("truncated ModRM")
        if opcode in (0xC6, 0xC7) and (code[position] >> 3) & 7:
            raise Unsupported("unsupported C6/C7 group (including XABORT/XBEGIN)")
        used, _rip = _modrm_length(code, position)
        position += used
    if immediate == "z":
        immediate = 2 if operand16 else 4
    elif immediate == "v":
        immediate = 8 if rex & 8 else 2 if operand16 else 4
    return finish(position + immediate)
