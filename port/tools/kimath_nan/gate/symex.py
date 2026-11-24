"""Hardened symbolic executor for oracle arithmetic windows (private prototype tool).

Executes strictly decoded instructions (x86strict) of one reviewed window, from the op's
dispatch target to its output consumer call, with explicit effects for every decoded form:

- registers hold tracked values: frame addresses ('addr', 'frame', off), the bound request
  JSON ('json_input',), the bound scalar kind ('kind_arg',), read-only data pointers, std::string
  and JSON object parts, integers, binary64 expressions ('leaf'|'const'|'op'), or undefined;
- memory is byte-granular per base; loads must reassemble exactly one stored value, otherwise
  they are undefined; every memory operand needs a tracked base address (no index registers);
  %rsp and entry-bound pointer registers are immutable;
- frame accesses must lie inside the reviewed frame [0, frame_size) above the window's %rsp
  (CONTEXT-1, finding 1): memory below %rsp belongs to callees and is never accessible;
- every CALL performs its hardware stack effect: the return address is written to [rsp-8]
  (a poison value) and, when the callee returns, all memory below %rsp is clobbered;
- arithmetic accepts only binary64 expressions; RIP constants are read from the ELF bytes of a
  read-only section;
- calls are allowed only to reviewed bindings (exact address, symbol and callee byte SHA-256)
  whose lock entry states exactly the implemented CONTRACTS entry (preconditions, memory
  written/read, return representation, stack ownership, exceptional behaviour, ABI); the
  implemented effect is applied explicitly and afterwards every xmm register and every
  caller-saved GPR is clobbered (SysV) except the declared return value;
- conditional branches need a reviewed disposition and a bound flag source; every other
  control transfer (jmp, ret outside an analysed callee, indirect calls) is undecodable or
  rejected.

Any violation raises SymexError: nothing is skipped. A contract is a reviewed trust boundary
bound to exact callee bytes, not a machine proof of the callee.
"""
import hashlib

from x86strict import GPR64, decode_one, DecodeError

UNDEF = ('undef', 'entry')
CLOB = ('clobbered', 'call')
CALLER_SAVED = [0, 1, 2, 6, 7, 8, 9, 10, 11]
RSP = 4


class SymexError(Exception):
    pass


_STACK = 'CALL pushes the return address at caller [rsp-8]; the callee may use and clobber all memory below caller %rsp'
_EXC = 'may throw: unwinds out of DoubleOps, so this window produces no reply value (no output is derived)'
_ABI = 'SysV x86-64: preserves rbx, rbp, rsp, r12-r15; clobbers every xmm register and the other GPRs'
_TRUST = 'transitive callees trusted via exact whole-binary identity (eligibility) and manual review; not machine-proved'
CONTRACTS = {
    'string_from_cstr': {
        'pre': '%rdi = frame address p of 32 writable bytes; %rsi = read-only NUL-terminated string',
        'writes': 'std::string object [p, p+32) and heap only', 'reads': 'the string',
        'returns': 'nothing used', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'json_at_cstr': {
        'pre': '%rdi = bound request JSON (const); %rsi = frame address of a const char* to a read-only key',
        'writes': 'no caller memory', 'reads': 'request JSON and key',
        'returns': '%rax = const reference to request member <key>', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'parse_matrix_double': {
        'pre': '%rdi = frame sret p (72 bytes); %rsi = request field <key>; %rdx = frame std::string equal to <key>',
        'writes': 'MATRIX3x3<double> [p, p+72): m_data[i][j] = exact bits of field[i][j]; heap',
        'reads': 'the field and the name (const)', 'returns': '%rax = p', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'parse_vector2_double': {
        'pre': '%rdi = frame sret p (16 bytes); %rsi = request field <key>; %rdx = frame std::string equal to <key>',
        'writes': 'VECTOR2<double> x, y at p, p+8 (exact bits); heap', 'reads': 'field and name (const)',
        'returns': '%rax = p', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'parse_vector3_double': {
        'pre': '%rdi = frame sret p (24 bytes); %rsi = request field <key>; %rdx = frame std::string equal to <key>',
        'writes': 'VECTOR3<double> x, y, z at p, p+8, p+16 (exact bits); heap', 'reads': 'field and name (const)',
        'returns': '%rax = p', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'parse_double_isra': {
        'pre': '%rdi = request field <key>; %rsi/%rdx = data/length words of one frame std::string equal to <key>',
        'writes': 'no caller memory; heap', 'reads': 'field and name (const)',
        'returns': '%xmm0 low lane = exact bits of the field; high lane undefined',
        'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'string_dispose': {
        'pre': '%rdi = frame address of an intact std::string object',
        'writes': 'the object becomes dead; frees its heap buffer', 'reads': 'the object',
        'returns': 'nothing used', 'retains_pointers': 'none',
        'stack': _STACK, 'exceptional': 'does not throw', 'abi': _ABI, 'transitive': _TRUST},
    'json_from_literal': {
        'pre': '%rdi = frame address p of 16 writable bytes; %rsi = read-only string literal',
        'writes': 'JSON object [p, p+16) and heap only', 'reads': 'the literal',
        'returns': 'nothing used', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'bits_json': {
        'pre': '%rdi = frame sret p (16 bytes); %xmm0 low lane = binary64 value',
        'writes': 'JSON string [p, p+16) holding the 16-hex bit pattern; heap', 'reads': '%xmm0 low lane',
        'returns': '%rax = p', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'matrix_reply': {
        'pre': '%rdi = frame sret JSON; %rsi = frame MATRIX3x3<double>',
        'writes': 'the sret JSON and heap', 'reads': 'm_data[i][j] -> reply row i, column j (window output, read at the call)',
        'returns': 'reply JSON', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'vector2_json': {
        'pre': '%rdi = frame sret JSON; %rsi = frame VECTOR2<double>',
        'writes': 'the sret JSON and heap', 'reads': 'x, y (window output, read at the call)',
        'returns': 'reply JSON', 'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'json_array_init': {
        'pre': '%rdi = frame JSON; %rsi = frame json_ref[count]; %rdx = count; %ecx = 0',
        'writes': 'the JSON array and heap; moves from the owned elements',
        'reads': 'owned elements in order (window output, read at the call)', 'returns': 'nothing used',
        'retains_pointers': 'none to caller memory',
        'stack': _STACK, 'exceptional': _EXC, 'abi': _ABI, 'transitive': _TRUST},
    'analyse': {
        'pre': 'leaf function analysed instruction by instruction to its final ret; inputs are ONLY the declared '
               'argument registers, each a caller-frame region of declared size and access (read or write); all '
               'other GPRs and every xmm register are undefined on entry; declared regions must not overlap',
        'writes': 'only inside declared write regions', 'reads': 'only inside declared regions',
        'returns': '%rax as analysed', 'retains_pointers': 'n/a (analysed)',
        'stack': 'CALL pushes the return address at caller [rsp-8] and the callee %rsp is caller %rsp - 8; the body '
                 'must never read, copy, address through or write %rsp/%esp (any decoded use of that register '
                 'family rejects), so no callee-stack alias can arise; RET is modelled explicitly',
        'exceptional': 'no calls, cannot throw', 'abi': _ABI + '; callee-saved registers restored for the caller',
        'transitive': 'none (fully analysed)'},
}


def contract_sha256(effect):
    import json
    return hashlib.sha256(json.dumps(CONTRACTS[effect], sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def is_float(v):
    return v[0] in ('leaf', 'const', 'op')


class Machine:
    def __init__(self, elf, bindings, roles, branches, immutable=(RSP,), frame_size=None):
        if not isinstance(frame_size, int) or isinstance(frame_size, bool) or frame_size <= 0:
            raise SymexError('a reviewed positive frame size is required')
        self.elf, self.bindings, self.roles, self.branches = elf, bindings, roles, branches
        self.frame_size = frame_size
        self.gpr = {n: UNDEF for n in range(16)}
        self.gpr[RSP] = ('addr', 'frame', 0)
        self.xmm = {n: [UNDEF, UNDEF] for n in range(16)}
        self.mem = {}          # (base, off) -> (vid, byte index, size)
        self.vals = []
        self.flags = None
        self.immutable = set(immutable)
        self.path = []
        self.calls = []
        self.trace = []
        self.next_obj = 0
        self.callee_cache = {}
        self.in_callee = False
        self.callee_regions = []

    # ---- values and memory -------------------------------------------------------------------
    def check_frame(self, base, off, size, write=False):
        if self.in_callee:
            ok = [r for r in self.callee_regions if base == 'frame' and r[0] <= off and off + size <= r[1]]
            if not ok:
                raise SymexError(f'analysed callee access {base}{off:+#x}/{size} outside its declared argument regions')
            if write and ok[0][2] != 'write':
                raise SymexError(f'analysed callee writes read-only argument region {ok[0][3]}')
        if base != 'frame':
            return
        if off < 0:
            raise SymexError(f'access below %rsp (frame{off:+#x}/{size}): callee-owned stack is not reviewed')
        if off + size > self.frame_size:
            raise SymexError(f'access frame{off:+#x}/{size} outside the reviewed frame [0, {self.frame_size:#x})')

    def store(self, base, off, value, size, internal=False):
        if not internal:
            self.check_frame(base, off, size, write=True)
        vid = len(self.vals)
        self.vals.append(value)
        for k in range(size):
            self.mem[(base, off + k)] = (vid, k, size)

    def load(self, base, off, size, internal=False):
        if not internal:
            self.check_frame(base, off, size)
        cells = [self.mem.get((base, off + k)) for k in range(size)]
        if any(c is None for c in cells):
            return ('undef', f'unwritten {base}{off:+#x}/{size}')
        vid = cells[0][0]
        if all(c == (vid, k, size) for k, c in enumerate(cells)):
            return self.vals[vid]
        return ('undef', f'partial or overlapping {base}{off:+#x}/{size}')

    def kill(self, base, off, size, why):
        self.store(base, off, ('undef', why), size)

    def address(self, m):
        if m.base == 'rip':
            raise SymexError('RIP-relative operand used as a writable or pointer address')
        if m.index is not None:
            raise SymexError('indexed addressing is not a reviewed form')
        if self.in_callee and m.base == RSP:
            raise SymexError('stack access inside an analysed callee')
        b = self.gpr[m.base]
        if b[0] != 'addr':
            raise SymexError(f'memory through unbound base %{GPR64[m.base]} = {b}')
        return b[1], b[2] + m.disp

    def set_gpr(self, n, value):
        if n in self.immutable:
            raise SymexError(f'write to bound base register %{GPR64[n]}')
        self.gpr[n] = value

    def xread(self, o, width):
        """Lanes of an xmm register or memory operand (memory: `width` bytes, 8 or 16)."""
        if o.kind == 'xmm':
            return list(self.xmm[o.n])
        if o.kind != 'mem':
            raise SymexError('unexpected operand kind')
        if o.base == 'rip':
            raw = self.elf.read_const(o.rip_target, width)
            lanes = [('const', '0x' + raw[k:k + 8][::-1].hex()) for k in range(0, width, 8)]
            return lanes + [('undef', 'rip-hi')] * (2 - len(lanes))
        base, off = self.address(o)
        lanes = [self.load(base, off + k, 8) for k in range(0, width, 8)]
        return lanes + [('undef', 'mem-hi')] * (2 - len(lanes))

    def xwrite_mem(self, o, lanes):
        base, off = self.address(o)
        for k, v in enumerate(lanes):
            self.store(base, off + 8 * k, v, 8)

    # ---- execution ---------------------------------------------------------------------------
    def run(self, fn, start, stop, consumer):
        """Executes `fn` bytes from `start`; `stop` must be the consumer call instruction."""
        pc = start
        end = fn['value'] + fn['size']
        steps = 0
        while True:
            if not fn['value'] <= pc < end:
                raise SymexError(f'execution left {fn["name"]} at {pc:#x}')
            try:
                ins = decode_one(fn['bytes'], pc - fn['value'], pc)
            except DecodeError as e:
                raise SymexError(f'decode {pc:#x}: {e}') from e
            self.trace.append(ins)
            steps += 1
            if steps > 5000:
                raise SymexError('window too long')
            if pc == stop:
                if ins.mnem != 'call':
                    raise SymexError(f'window stop {stop:#x} is not a call')
                return self.consume(ins, consumer)
            pc = self.step(ins)

    def step(self, ins):
        m, ops = ins.mnem, ins.ops
        X = self.xmm
        if m == 'endbr64':
            return ins.next
        if m in ('je', 'jne'):
            return self.branch(ins)
        if m == 'call':
            self.call(ins)
            return ins.next
        if m == 'ret':
            raise SymexError(f'premature ret at {ins.addr:#x}')
        if m == 'mov' and ops[1].kind == 'gpr' and ops[0].kind == 'imm':
            self.set_gpr(ops[1].n, ('int', ops[0].value))
            return ins.next
        if m == 'xor':
            self.set_gpr(ops[1].n, ('int', 0))
            self.flags = ('opaque', 'xor')
            return ins.next
        if m == 'cmp':
            v = self.gpr[ops[1].n]
            self.flags = ('cmp32', v, ops[0].value)
            return ins.next
        if m == 'mov':
            src, dst = ops
            if src.kind == 'gpr' and dst.kind == 'gpr':
                self.set_gpr(dst.n, self.gpr[src.n])
            elif src.kind == 'mem' and dst.kind == 'gpr':
                self.set_gpr(dst.n, self.load(*self.address(src), 8))
            elif src.kind == 'gpr' and dst.kind == 'mem':
                base, off = self.address(dst)
                self.store(base, off, self.gpr[src.n], 8)
            else:
                raise SymexError(f'unreviewed mov form at {ins.addr:#x}')
            return ins.next
        if m == 'lea':
            src, dst = ops
            if src.base == 'rip':
                self.set_gpr(dst.n, ('rodata', src.rip_target))
            else:
                base, off = self.address(src)
                self.set_gpr(dst.n, ('addr', base, off))
            return ins.next
        if m in ('movq', 'movb') and ops[0].kind == 'imm':
            base, off = self.address(ops[1])
            size = 8 if m == 'movq' else 1
            self.store(base, off, ('int', ops[0].value), size)
            return ins.next
        if m == 'movq' and ops[0].kind == 'gpr':
            X[ops[1].n] = [self.gpr[ops[0].n], ('const', '0x0000000000000000')]
            return ins.next
        if m == 'movq' and ops[1].kind == 'gpr':
            self.set_gpr(ops[1].n, X[ops[0].n][0])
            return ins.next
        if m == 'movq' and ops[1].kind == 'xmm':
            X[ops[1].n] = [self.xread(ops[0], 8)[0], ('const', '0x0000000000000000')]
            return ins.next
        if m == 'movq' and ops[1].kind == 'mem':
            self.xwrite_mem(ops[1], [X[ops[0].n][0]])
            return ins.next
        if m == 'movsd':
            src, dst = ops
            if dst.kind == 'mem':
                self.xwrite_mem(dst, [X[src.n][0]])
            elif src.kind == 'mem':
                X[dst.n] = [self.xread(src, 8)[0], ('const', '0x0000000000000000')]
            else:
                X[dst.n] = [X[src.n][0], X[dst.n][1]]
            return ins.next
        if m in ('movupd', 'movapd', 'movaps', 'movups', 'movdqa', 'movdqu'):
            src, dst = ops
            if dst.kind == 'mem':
                self.xwrite_mem(dst, list(X[src.n]))
            else:
                X[dst.n] = self.xread(src, 16)
            return ins.next
        if m in ('movlpd', 'movlps', 'movhpd', 'movhps'):
            src, dst = ops
            hi = m.startswith('movh')
            if dst.kind == 'mem':
                self.xwrite_mem(dst, [X[src.n][1 if hi else 0]])
            else:
                v = self.xread(src, 8)[0]
                X[dst.n] = [X[dst.n][0], v] if hi else [v, X[dst.n][1]]
            return ins.next
        if m == 'unpcklpd':
            X[ops[1].n] = [X[ops[1].n][0], self.xread(ops[0], 16)[0]]
            return ins.next
        if m == 'unpckhpd':
            X[ops[1].n] = [X[ops[1].n][1], self.xread(ops[0], 16)[1]]
            return ins.next
        if m == 'shufpd':
            imm, src, dst = ops[0].value, self.xread(ops[1], 16), X[ops[2].n]
            X[ops[2].n] = [dst[imm & 1], src[(imm >> 1) & 1]]
            return ins.next
        if m[:3] in ('add', 'sub', 'mul', 'div') and m[3:] in ('sd', 'pd'):
            lanes = 2 if m.endswith('pd') else 1
            src, dst = self.xread(ops[0], 16 if lanes == 2 else 8), X[ops[1].n]
            out = list(dst)
            for k in range(lanes):
                if not (is_float(dst[k]) and is_float(src[k])):
                    raise SymexError(f'{m} at {ins.addr:#x} on a non-binary64 value: {dst[k]!r} / {src[k]!r}')
                out[k] = ('op', m[:3], dst[k], src[k])
            X[ops[1].n] = out
            return ins.next
        raise SymexError(f'no reviewed effect for {ins.fmt()} at {ins.addr:#x}')

    def branch(self, ins):
        b = self.branches.get(ins.addr)
        if b is None:
            raise SymexError(f'unreviewed conditional branch {ins.fmt()} at {ins.addr:#x}')
        if b['target'] != ins.ops[0].target or b['mnem'] != ins.mnem:
            raise SymexError(f'branch at {ins.addr:#x} differs from its reviewed form')
        f = self.flags
        if not f or f[0] != 'cmp32' or f[1] != tuple(b['flag_value']) or f[2] != b['flag_imm']:
            raise SymexError(f'branch at {ins.addr:#x} has an unbound flag source {f!r}')
        self.path.append({'addr': hex(ins.addr), 'condition': f'{b["flag_value"][0]} == {b["flag_imm"]}',
                          'jump_if_true': ins.mnem == 'je', 'disposition': b['expect']})
        if b['expect'] == 'fallthrough':
            return ins.next
        if b['expect'] == 'taken':
            return ins.ops[0].target
        raise SymexError('bad reviewed disposition')

    # ---- calls -------------------------------------------------------------------------------
    def bound_callee(self, target):
        b = self.bindings.get(target)
        if b is None:
            raise SymexError(f'call to unreviewed target {target:#x} {self.elf.functions_at(target)}')
        if target not in self.callee_cache:
            fn = self.elf.function(b['symbol'], target)
            if fn['value'] != target:
                raise SymexError(f'reviewed symbol {b["symbol"]} is not at {target:#x}')
            if fn['sha256'] != b['sha256']:
                raise SymexError(f'callee {b["symbol"]} bytes changed: {fn["sha256"]}')
            if b['effect'] not in CONTRACTS or b.get('contract_sha256') != contract_sha256(b['effect']):
                raise SymexError(f'callee {target:#x}: lock contract differs from the implemented {b["effect"]} contract')
            self.callee_cache[target] = fn
        return b, self.callee_cache[target]

    def frame_ptr(self, reg, what):
        v = self.gpr[reg]
        if v[0] != 'addr' or v[1] != 'frame':
            raise SymexError(f'{what}: %{GPR64[reg]} is not a frame address ({v!r})')
        return v[2]

    def cstr(self, v, what):
        if v[0] != 'rodata':
            raise SymexError(f'{what}: not a read-only string pointer ({v!r})')
        return self.elf.cstring(v[1])

    def string_obj(self, off, what):
        parts = [self.load('frame', off + 8 * k, 8) for k in range(4)]
        if not all(p[0] == 'str' for p in parts) or len({p[1] for p in parts}) != 1 or \
                [p[2] for p in parts] != [0, 1, 2, 3]:
            raise SymexError(f'{what}: no intact std::string object at frame{off:+#x}')
        return parts[0]

    def clobber(self, keep_rax=None):
        for n in CALLER_SAVED:
            self.gpr[n] = CLOB
        for n in self.xmm:
            self.xmm[n] = [CLOB, CLOB]
        if keep_rax is not None:
            self.gpr[0] = keep_rax
        self.flags = None

    def new_obj(self):
        self.next_obj += 1
        return self.next_obj

    # Hardware CALL/RET stack effects (finding 1).
    def push_return_address(self, ins):
        self.store('frame', -8, ('return-address', hex(ins.next)), 8, internal=True)

    def clobber_below_rsp(self):
        for key in [k for k in self.mem if k[0] == 'frame' and k[1] < 0]:
            self.mem[key] = (len(self.vals), 0, 1)
            self.vals.append(('undef', 'callee-owned stack clobbered by call'))

    def call(self, ins):
        target = ins.ops[0].target
        b, fn = self.bound_callee(target)
        self.push_return_address(ins)
        try:
            self._call_effect(ins, b, fn, b['effect'])
        finally:
            self.clobber_below_rsp()

    def _call_effect(self, ins, b, fn, e):
        rdi, rsi, rdx = self.gpr[7], self.gpr[6], self.gpr[2]
        rax = None
        self.calls.append({'addr': hex(ins.addr), 'effect': e})
        if e == 'string_from_cstr':
            p = self.frame_ptr(7, e)
            text = self.cstr(rsi, e)
            sid = self.new_obj()
            for k in range(4):
                self.store('frame', p + 8 * k, ('str', sid, k, text), 8)
        elif e == 'json_at_cstr':
            if rdi != ('json_input',):
                raise SymexError(f'{e}: object is not the bound request input ({rdi!r})')
            key = self.cstr(self.load('frame', self.frame_ptr(6, e), 8), e)
            rax = ('jfield', key)
        elif e in ('parse_matrix_double', 'parse_vector2_double', 'parse_vector3_double'):
            p = self.frame_ptr(7, e)
            if rsi[0] != 'jfield':
                raise SymexError(f'{e}: argument is not a request field ({rsi!r})')
            key = rsi[1]
            name = self.string_obj(self.frame_ptr(2, e), e)
            if name[3] != key:
                raise SymexError(f'{e}: name {name[3]!r} differs from field {key!r}')
            role = self.roles.get(key)
            if role is None:
                raise SymexError(f'{e}: field {key!r} has no reviewed role')
            if e == 'parse_matrix_double':
                for i in range(3):
                    for j in range(3):
                        self.store('frame', p + 8 * (3 * i + j), ('leaf', f'{role}{i}{j}'), 8)
            else:
                n = 2 if e == 'parse_vector2_double' else 3
                for k in range(n):
                    self.store('frame', p + 8 * k, ('leaf', f'{role}{"xyz"[k]}'), 8)
            rax = ('addr', 'frame', p)
        elif e == 'parse_double_isra':
            if rdi[0] != 'jfield':
                raise SymexError(f'{e}: argument is not a request field ({rdi!r})')
            if not (rsi[0] == 'str' and rdx[0] == 'str' and rsi[1] == rdx[1] and rsi[2] == 0 and rdx[2] == 1):
                raise SymexError(f'{e}: name is not the data/length pair of one std::string')
            if rsi[3] != rdi[1]:
                raise SymexError(f'{e}: name {rsi[3]!r} differs from field {rdi[1]!r}')
            role = self.roles.get(rdi[1])
            if role is None:
                raise SymexError(f'{e}: field {rdi[1]!r} has no reviewed role')
            self.clobber()
            self.xmm[0] = [('leaf', role), CLOB]
            return
        elif e == 'string_dispose':
            p = self.frame_ptr(7, e)
            self.string_obj(p, e)
            self.kill('frame', p, 32, 'disposed string')
        elif e == 'json_from_literal':
            p = self.frame_ptr(7, e)
            text = self.cstr(rsi, e)
            jid = self.new_obj()
            self.store('frame', p, ('jsonlit', jid, 0, text), 8)
            self.store('frame', p + 8, ('jsonlit', jid, 1, text), 8)
        elif e == 'bits_json':
            p = self.frame_ptr(7, e)
            v = self.xmm[0][0]
            if not is_float(v):
                raise SymexError(f'{e}: argument is not binary64 ({v!r})')
            jid = self.new_obj()
            self.store('frame', p, ('json', jid, 0, v), 8)
            self.store('frame', p + 8, ('json', jid, 1, v), 8)
        elif e == 'analyse':
            self.analyse_callee(fn, b)
            return
        else:
            raise SymexError(f'effect {e} is a consumer or unknown here (call at {ins.addr:#x})')
        self.clobber(rax)

    def analyse_callee(self, fn, b):
        """Executes a reviewed leaf callee from its entry to its final ret (R1 contract).

        Inputs are only the declared argument registers (`b['args']`: register -> {size, access}),
        each a non-overlapping caller-frame region; every other GPR and every xmm register is
        undefined inside the callee; any decoded use of the %rsp/%esp register family rejects.
        """
        args = b.get('args')
        if not isinstance(args, dict) or not args:
            raise SymexError('analysed callee needs declared argument regions')
        regs = {'rdi': 7, 'rsi': 6, 'rdx': 2, 'rcx': 1, 'r8': 8, 'r9': 9}
        regions = []
        for name, spec in sorted(args.items()):
            if name not in regs or spec.get('access') not in ('read', 'write') or not isinstance(spec.get('size'), int) \
                    or isinstance(spec.get('size'), bool) or spec['size'] <= 0:
                raise SymexError(f'bad declared argument {name!r}: {spec!r}')
            v = self.gpr[regs[name]]
            if v[0] != 'addr' or v[1] != 'frame':
                raise SymexError(f'declared argument %{name} is not a caller-frame address ({v!r})')
            self.check_frame('frame', v[2], spec['size'])
            regions.append((v[2], v[2] + spec['size'], spec['access'], name))
        for i, a in enumerate(regions):
            for c in regions[i + 1:]:
                if a[0] < c[1] and c[0] < a[1]:
                    raise SymexError(f'declared argument regions {a[3]} and {c[3]} overlap')
        saved_gpr, saved_immutable = dict(self.gpr), set(self.immutable)
        arg_regs = {regs[n] for n in args}
        for n in range(16):
            if n not in arg_regs:
                self.gpr[n] = ('undef', 'not a declared callee argument')
        self.gpr[RSP] = ('callee-rsp', 'caller rsp - 8 (never usable)')
        for n in self.xmm:
            self.xmm[n] = [('undef', 'callee entry'), ('undef', 'callee entry')]
        self.immutable = saved_immutable | arg_regs | {RSP}
        self.in_callee, self.callee_regions = True, regions
        pc, end = fn['value'], fn['value'] + fn['size']
        try:
            while True:
                try:
                    ins = decode_one(fn['bytes'], pc - fn['value'], pc)
                except DecodeError as e:
                    raise SymexError(f'decode callee {pc:#x}: {e}') from e
                self.trace.append(ins)
                if ins.mnem == 'ret':
                    if ins.next + b['padding'] != end:
                        raise SymexError(f'callee ret at {ins.addr:#x} is not its reviewed final instruction')
                    break
                if ins.mnem in ('call', 'je', 'jne'):
                    raise SymexError(f'control flow inside analysed callee at {ins.addr:#x}')
                if uses_stack_pointer(ins):
                    raise SymexError(f'analysed callee uses the %rsp/%esp register family at {ins.addr:#x}: {ins.fmt()}')
                pc = self.step(ins)
        finally:
            self.in_callee, self.callee_regions = False, []
            self.immutable = saved_immutable
        rax = self.gpr[0]
        for n in (3, 5, 12, 13, 14, 15, RSP):
            self.gpr[n] = saved_gpr[n]
        self.clobber(rax)

    def consume(self, ins, consumer):
        target = ins.ops[0].target
        b, _fn = self.bound_callee(target)
        e = consumer['effect']
        binding_for = {'matrix_reply': 'matrix_reply', 'vector2_json': 'vector2_json',
                       'bits_value': 'bits_json', 'json_array_init': 'json_array_init'}
        if b['effect'] != binding_for.get(e):
            raise SymexError(f'window consumer binding is {b["effect"]}, reviewed consumer {e}')
        outs = {}
        if e == 'matrix_reply':
            p = self.frame_ptr(6, e)
            for i in range(3):
                for j in range(3):
                    outs[f'r{i}{j}'] = self.load('frame', p + 8 * (3 * i + j), 8)
        elif e == 'vector2_json':
            p = self.frame_ptr(6, e)
            outs = {'x': self.load('frame', p, 8), 'y': self.load('frame', p + 8, 8)}
        elif e == 'bits_value':
            outs = {consumer['name']: self.xmm[0][0]}
        elif e == 'json_array_init':
            p = self.frame_ptr(6, e)
            if self.gpr[2] != ('int', consumer['count']) or self.gpr[1] != ('int', 0):
                raise SymexError(f'{e}: size/type-deduction arguments differ ({self.gpr[2]!r}, {self.gpr[1]!r})')
            for k, name in enumerate(consumer['names']):
                o = p + 0x18 * k
                lo, hi, ref = self.load('frame', o, 8), self.load('frame', o + 8, 8), self.load('frame', o + 16, 8)
                if not (lo[0] == 'json' and hi[0] == 'json' and lo[1] == hi[1] and (lo[2], hi[2]) == (0, 1)):
                    raise SymexError(f'{e}: element {k} is not an intact owned Bits() JSON value')
                if ref != ('int', 0):
                    raise SymexError(f'{e}: element {k} is not owned (value_ref {ref!r})')
                outs[name] = lo[3]
        else:
            raise SymexError(f'unknown consumer {e}')
        for n, v in outs.items():
            if not is_float(v):
                raise SymexError(f'output {n} is not a binary64 expression: {v!r}')
        self.calls.append({'addr': hex(ins.addr), 'effect': e, 'consumer': True})
        return outs


def uses_stack_pointer(ins):
    """True if any decoded operand reads, writes or addresses through the %rsp/%esp family."""
    for o in ins.ops:
        if o.kind == 'gpr' and o.n == RSP:
            return True
        if o.kind == 'mem' and o.base != 'rip' and (o.base == RSP or o.index == RSP):
            return True
    return False


def sha256(b):
    return hashlib.sha256(b).hexdigest()
