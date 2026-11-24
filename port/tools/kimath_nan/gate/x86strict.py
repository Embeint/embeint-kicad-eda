"""Strict x86-64 decoder for the reviewed instruction forms only (private prototype tool).

Decodes raw bytes. Every byte sequence that is not one of the listed forms raises DecodeError:
there is no "harmless by default" class. In particular VEX/EVEX (vzeroall, vaddsd), LOCK,
segment/address-size prefixes, integer ALU forms other than the listed cmp/xor idiom
(add, sub, inc, and, or, ...), push/pop, xchg, jmp, jcc other than je/jne, indirect calls,
ret, sqrt/min/max/cvt and every other SSE opcode are rejected.

`fmt()` renders the operands in GNU objdump AT&T syntax so that every decoded instruction can
be cross-checked against an independent disassembler.
"""
import struct

GPR64 = ['rax', 'rcx', 'rdx', 'rbx', 'rsp', 'rbp', 'rsi', 'rdi',
         'r8', 'r9', 'r10', 'r11', 'r12', 'r13', 'r14', 'r15']
GPR32 = ['eax', 'ecx', 'edx', 'ebx', 'esp', 'ebp', 'esi', 'edi',
         'r8d', 'r9d', 'r10d', 'r11d', 'r12d', 'r13d', 'r14d', 'r15d']


class DecodeError(Exception):
    pass


class Op:
    """Operand: kind in {'xmm', 'gpr', 'mem', 'imm', 'rel'}."""

    def __init__(self, kind, **kw):
        self.kind = kind
        self.__dict__.update(kw)

    def fmt(self):
        if self.kind == 'xmm':
            return f'%xmm{self.n}'
        if self.kind == 'gpr':
            return '%' + (GPR64 if self.width == 8 else GPR32)[self.n]
        if self.kind == 'imm':
            return f'${self.value:#x}' if self.value >= 0 else f'$-{-self.value:#x}'
        if self.kind == 'rel':
            return f'{self.target:x}'
        if self.base == 'rip':
            return f'{self.disp:#x}(%rip)' if self.disp >= 0 else f'-{-self.disp:#x}(%rip)'
        inner = '%' + GPR64[self.base]
        if self.index is not None:
            inner += f',%{GPR64[self.index]},{self.scale}'
        if self.disp_size == 0:
            d = ''
        elif self.disp >= 0:
            d = f'{self.disp:#x}'
        else:
            d = f'-{-self.disp:#x}'
        return f'{d}({inner})'


class Insn:
    def __init__(self, addr, raw, mnem, ops):
        self.addr, self.raw, self.mnem, self.ops = addr, raw, mnem, ops
        self.length = len(raw)
        self.next = addr + len(raw)

    def fmt(self):
        return (self.mnem + ' ' + ','.join(o.fmt() for o in self.ops)).strip()


SSE_ARITH = {0x58: 'add', 0x59: 'mul', 0x5C: 'sub', 0x5E: 'div'}


def decode_one(code, pos, addr):
    """Decodes one instruction of `code` at `pos` (virtual address `addr`)."""
    start = pos
    legacy = []
    while pos < len(code) and code[pos] in (0x66, 0xF2, 0xF3):
        legacy.append(code[pos])
        pos += 1
    if pos >= len(code):
        raise DecodeError('truncated prefixes')
    if code[pos] in (0xF0, 0x2E, 0x36, 0x3E, 0x26, 0x64, 0x65, 0x67, 0xC4, 0xC5, 0x62, 0x8F):
        raise DecodeError(f'rejected prefix/escape {code[pos]:#04x} (LOCK, segment, addr-size, VEX/EVEX/XOP)')
    if len(set(legacy)) != len(legacy) or (0xF2 in legacy and 0xF3 in legacy):
        raise DecodeError('conflicting or repeated legacy prefixes')
    rex = 0
    if 0x40 <= code[pos] <= 0x4F:
        rex = code[pos]
        pos += 1
    W, R, X, B = (rex >> 3) & 1, (rex >> 2) & 1, (rex >> 1) & 1, rex & 1
    p66, pF2, pF3 = 0x66 in legacy, 0xF2 in legacy, 0xF3 in legacy
    if p66 and (pF2 or pF3):
        # Hardware gives F2/F3 precedence as the mandatory prefix; no reviewed form mixes them.
        raise DecodeError('mixed 66 and F2/F3 mandatory prefixes are not a reviewed form')

    def need(n):
        if pos + n > len(code):
            raise DecodeError('truncated instruction')

    def modrm(width):
        nonlocal pos
        need(1)
        m = code[pos]
        pos += 1
        mod, reg, rm = m >> 6, ((m >> 3) & 7) | (R << 3), m & 7
        if mod == 3:
            return mod, reg, ('reg', rm | (B << 3))
        base, index, scale, disp, dsize = None, None, 1, 0, 0
        if rm == 4:
            need(1)
            sib = code[pos]
            pos += 1
            scale = 1 << (sib >> 6)
            idx = ((sib >> 3) & 7) | (X << 3)
            index = None if idx == 4 else idx
            b = sib & 7
            if b == 5 and mod == 0:
                raise DecodeError('SIB without base is not a reviewed form')
            base = b | (B << 3)
        elif rm == 5 and mod == 0:
            need(4)
            disp = struct.unpack_from('<i', code, pos)[0]
            pos += 4
            return mod, reg, ('mem', Op('mem', base='rip', index=None, scale=1, disp=disp,
                                         disp_size=4, width=width, rip_target=None))
        else:
            base = rm | (B << 3)
        if mod == 1:
            need(1)
            disp, dsize = struct.unpack_from('<b', code, pos)[0], 1
            pos += 1
        elif mod == 2:
            need(4)
            disp, dsize = struct.unpack_from('<i', code, pos)[0], 4
            pos += 4
        return mod, reg, ('mem', Op('mem', base=base, index=index, scale=scale, disp=disp,
                                     disp_size=dsize, width=width, rip_target=None))

    def xmm(n):
        return Op('xmm', n=n)

    def gpr(n, width):
        return Op('gpr', n=n, width=width)

    def rm_op(r, width, as_xmm):
        if r[0] == 'reg':
            return xmm(r[1]) if as_xmm else gpr(r[1], width)
        return r[1]

    def finish(mnem, ops, imm_bytes=0):
        nonlocal pos
        if imm_bytes:
            need(imm_bytes)
        raw = bytes(code[start:pos + imm_bytes])
        ins = Insn(addr, raw, mnem, ops)
        for o in ops:
            if o.kind == 'mem' and o.base == 'rip':
                o.rip_target = ins.next + o.disp
        return ins

    need(1)
    op = code[pos]
    pos += 1
    # endbr64
    if op == 0x0F and pF3 and not p66 and not rex and code[pos:pos + 2] == b'\x1e\xfa' and len(legacy) == 1:
        pos += 2
        return finish('endbr64', [])
    if legacy and op != 0x0F:
        raise DecodeError(f'legacy prefix on non-0F opcode {op:#04x}')
    if op == 0xC3 and not rex:
        return finish('ret', [])
    if op == 0xE8 and not rex:
        need(4)
        rel = struct.unpack_from('<i', code, pos)[0]
        pos += 4
        return finish('call', [Op('rel', target=addr + (pos - start) + rel)])
    if op in (0x74, 0x75) and not rex:
        need(1)
        rel = struct.unpack_from('<b', code, pos)[0]
        pos += 1
        return finish('je' if op == 0x74 else 'jne', [Op('rel', target=addr + (pos - start) + rel)])
    if op in (0x89, 0x8B) and W:
        mod, reg, r = modrm(8)
        a, b = gpr(reg, 8), rm_op(r, 8, False)
        return finish('mov', [a, b] if op == 0x89 else [b, a])
    if op == 0x8D and W:
        mod, reg, r = modrm(8)
        if r[0] != 'mem':
            raise DecodeError('lea with register operand')
        return finish('lea', [r[1], gpr(reg, 8)])
    if 0xB8 <= op <= 0xBF and not W:
        need(4)
        imm = struct.unpack_from('<I', code, pos)[0]
        pos += 4
        return finish('mov', [Op('imm', value=imm), gpr((op - 0xB8) | (B << 3), 4)])
    if op == 0x31 and not W:
        mod, reg, r = modrm(4)
        if r[0] != 'reg' or r[1] != reg:
            raise DecodeError('xor other than the 32-bit zeroing idiom')
        return finish('xor', [gpr(reg, 4), gpr(reg, 4)])
    if op == 0x83 and not W:
        mod, reg, r = modrm(4)
        if (reg & 7) != 7 or r[0] != 'reg':
            raise DecodeError('0x83 group form other than cmp $imm8,r32')
        need(1)
        imm = struct.unpack_from('<b', code, pos)[0]
        pos += 1
        return finish('cmp', [Op('imm', value=imm), gpr(r[1], 4)])
    if op == 0xC7 and W:
        mod, reg, r = modrm(8)
        if (reg & 7) != 0 or r[0] != 'mem':
            raise DecodeError('0xC7 form other than movq $imm32,m64')
        need(4)
        imm = struct.unpack_from('<i', code, pos)[0]
        pos += 4
        ins = finish('movq', [Op('imm', value=imm), r[1]])
        return ins
    if op == 0xC6 and not W:
        mod, reg, r = modrm(1)
        if (reg & 7) != 0 or r[0] != 'mem':
            raise DecodeError('0xC6 form other than movb $imm8,m8')
        need(1)
        imm = code[pos]
        pos += 1
        return finish('movb', [Op('imm', value=imm), r[1]])
    if op != 0x0F:
        raise DecodeError(f'unreviewed opcode {op:#04x}')
    need(1)
    op2 = code[pos]
    pos += 1
    if op2 in (0x10, 0x11):
        name = 'movupd' if p66 else 'movsd' if pF2 else None if pF3 else 'movups'
        if name is None or W:
            raise DecodeError('unreviewed 0F 10/11 form (movss or REX.W)')
        width = 8 if name == 'movsd' else 16
        mod, reg, r = modrm(width)
        if name == 'movups' and op2 == 0x10:
            raise DecodeError('movups load is not a reviewed form')
        src, dst = (rm_op(r, width, True), xmm(reg)) if op2 == 0x10 else (xmm(reg), rm_op(r, width, True))
        return finish(name, [src, dst])
    if op2 in (0x12, 0x13, 0x16, 0x17):
        name = ('movlpd' if op2 in (0x12, 0x13) else 'movhpd') if p66 else \
               (('movlps' if op2 in (0x12, 0x13) else 'movhps') if not (pF2 or pF3) else None)
        if name is None or W:
            raise DecodeError('unreviewed 0F 12/13/16/17 form')
        mod, reg, r = modrm(8)
        if r[0] != 'mem':
            raise DecodeError(f'{name} register form (movhlps/movlhps) is not reviewed')
        return finish(name, [r[1], xmm(reg)] if op2 in (0x12, 0x16) else [xmm(reg), r[1]])
    if op2 in (0x14, 0x15) and p66 and not (pF2 or pF3 or W):
        mod, reg, r = modrm(16)
        return finish('unpcklpd' if op2 == 0x14 else 'unpckhpd', [rm_op(r, 16, True), xmm(reg)])
    if op2 in (0x28, 0x29) and not (pF2 or pF3 or W):
        name = 'movapd' if p66 else 'movaps'
        mod, reg, r = modrm(16)
        return finish(name, [rm_op(r, 16, True), xmm(reg)] if op2 == 0x28 else [xmm(reg), rm_op(r, 16, True)])
    if op2 in SSE_ARITH and (p66 or pF2) and not (pF3 or W):
        name = SSE_ARITH[op2] + ('pd' if p66 else 'sd')
        mod, reg, r = modrm(16 if p66 else 8)
        return finish(name, [rm_op(r, 16 if p66 else 8, True), xmm(reg)])
    if op2 == 0xC6 and p66 and not (pF2 or pF3 or W):
        mod, reg, r = modrm(16)
        need(1)
        imm = code[pos]
        pos += 1
        return finish('shufpd', [Op('imm', value=imm), rm_op(r, 16, True), xmm(reg)])
    if op2 in (0x6F, 0x7F) and (p66 or pF3) and not (pF2 or W):
        name = 'movdqa' if p66 else 'movdqu'
        mod, reg, r = modrm(16)
        return finish(name, [rm_op(r, 16, True), xmm(reg)] if op2 == 0x6F else [xmm(reg), rm_op(r, 16, True)])
    if op2 in (0x6E, 0x7E) and p66 and W and not (pF2 or pF3):
        mod, reg, r = modrm(8)
        g = rm_op(r, 8, False)
        return finish('movq', [g, xmm(reg)] if op2 == 0x6E else [xmm(reg), g])
    if op2 == 0x7E and pF3 and not (p66 or pF2 or W):
        mod, reg, r = modrm(8)
        return finish('movq', [rm_op(r, 8, True), xmm(reg)])
    if op2 == 0xD6 and p66 and not (pF2 or pF3 or W):
        mod, reg, r = modrm(8)
        return finish('movq', [xmm(reg), rm_op(r, 8, True)])
    if op2 == 0x84 and not (legacy or rex):
        need(4)
        rel = struct.unpack_from('<i', code, pos)[0]
        pos += 4
        return finish('je', [Op('rel', target=addr + (pos - start) + rel)])
    if op2 == 0x85 and not (legacy or rex):
        need(4)
        rel = struct.unpack_from('<i', code, pos)[0]
        pos += 4
        return finish('jne', [Op('rel', target=addr + (pos - start) + rel)])
    raise DecodeError(f'unreviewed opcode 0f {op2:02x}')


def decode_range(code, base, start, stop):
    """Decodes [start, stop) of a function whose bytes `code` start at `base`."""
    out, a = [], start
    while a < stop:
        ins = decode_one(code, a - base, a)
        if ins.next > stop:
            raise DecodeError(f'instruction at {a:#x} crosses the range end {stop:#x}')
        out.append(ins)
        a = ins.next
    return out
