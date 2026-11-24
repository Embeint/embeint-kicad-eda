"""Function-wide GNU objdump listing, coverage validation and a conservative CFG (private tool).

`Listing(elf, fn)` disassembles exactly the symbol range of `fn` with raw bytes and requires
every line in the range to parse, instructions to be contiguous from the symbol start to its
end, their bytes to equal the ELF bytes and no `(bad)` decoding. It is the independent
disassembler for cross-checking the strict decoder, and the input for the register-binding
proof (entry of each analysed window).
"""
import re
import subprocess

LINE = re.compile(r'^\s*([0-9a-f]+):\t((?:[0-9a-f]{2} )+)\s*\t(\S+)(?:\s+(.*?))?\s*$')
HEADER = re.compile(r'^[0-9a-f]{16} <.*>:$')


class ListingError(Exception):
    pass


def strip_comment(ops):
    ops = re.sub(r'\s*#.*$', '', ops or '')
    return re.sub(r'\s*<[^>]*>$', '', ops).strip()


class Listing:
    def __init__(self, path, elf, fn, objdump='objdump'):
        self.fn = fn
        start, end = fn['value'], fn['value'] + fn['size']
        out = subprocess.run([objdump, '-d', '-w', '--insn-width=16', '-M', 'att',
                              f'--start-address={start:#x}', f'--stop-address={end:#x}', path],
                             capture_output=True, text=True, check=True).stdout
        self.version = subprocess.run([objdump, '--version'], capture_output=True, text=True).stdout.splitlines()[0]
        self.insns, expect = [], start
        in_body = False
        for line in out.splitlines():
            if HEADER.match(line):
                in_body = True
                continue
            if not in_body or not line.strip() or line.startswith('Disassembly of section'):
                continue
            m = LINE.match(line)
            if not m:
                raise ListingError(f'unparsed disassembly line in {fn["name"]}: {line!r}')
            addr, raw, mnem, ops = int(m.group(1), 16), bytes.fromhex(m.group(2).replace(' ', '')), m.group(3), m.group(4) or ''
            if addr != expect:
                raise ListingError(f'gap or overlap at {addr:#x} (expected {expect:#x})')
            if '(bad)' in line or mnem.startswith('(bad'):
                raise ListingError(f'undecodable bytes at {addr:#x}')
            if raw != fn['bytes'][addr - start:addr - start + len(raw)]:
                raise ListingError(f'listing bytes differ from ELF bytes at {addr:#x}')
            self.insns.append({'addr': addr, 'len': len(raw), 'raw': raw, 'mnem': mnem,
                               'ops_text': strip_comment(ops), 'line': line})
            expect = addr + len(raw)
        if expect != end:
            raise ListingError(f'listing ends at {expect:#x}, symbol ends at {end:#x}')
        self.by_addr = {i['addr']: i for i in self.insns}

    def check_boundary(self, addr):
        if addr not in self.by_addr and addr != self.fn['value'] + self.fn['size']:
            raise ListingError(f'{addr:#x} is not an instruction boundary of {self.fn["name"]}')

    # --- conservative control flow ----------------------------------------------------------
    def successors(self, i):
        m, ops = i['mnem'], i['ops_text']
        nxt = i['addr'] + i['len']
        if m == 'ret' or m.startswith('ret'):
            return []
        if m.startswith('jmp') or m.startswith('notrack'):
            t = re.fullmatch(r'([0-9a-f]+)', ops)
            if not t:
                return None  # indirect: unknown successors
            return [int(t.group(1), 16)]
        if re.fullmatch(r'j[a-z]+', m):
            t = re.fullmatch(r'([0-9a-f]+)', ops)
            if not t:
                return None
            return [int(t.group(1), 16), nxt]
        if m in ('ud2', 'hlt'):
            return []
        return [nxt]

    def paths_to(self, target):
        """Instructions on some path from the function entry to `target` (target excluded)."""
        entry = self.fn['value']
        fwd, stack = set(), [entry]
        unknown = []
        while stack:
            a = stack.pop()
            if a in fwd or a == target:
                continue
            if a not in self.by_addr:
                continue  # leaves the function (tail call target outside): no path back
            fwd.add(a)
            s = self.successors(self.by_addr[a])
            if s is None:
                unknown.append(a)
                continue
            stack.extend(s)
        preds = {}
        for a in fwd:
            for s in self.successors(self.by_addr[a]) or []:
                preds.setdefault(s, []).append(a)
        back, stack = set(), list(preds.get(target, []))
        while stack:
            a = stack.pop()
            if a in back:
                continue
            back.add(a)
            stack.extend(preds.get(a, []))
        if unknown:
            # An indirect jump reachable from the entry could reach the window: refuse.
            raise ListingError(f'indirect control flow reachable from the entry at {unknown[0]:#x}')
        if not back and target != entry:
            raise ListingError('window entry unreachable from the function entry')
        return fwd & back


NONWRITING = re.compile(r'^(cmp[bwlq]?|test[bwlq]?|push[q]?|call[q]?|jmp|j[a-z]+|ucomisd|comisd|bt[lq]?|nop[wl]?|endbr64)$')
SWAPPING = re.compile(r'^(xchg|xadd|cmpxchg)')
FAMILY = {'r12': ['r12', 'r12d', 'r12w', 'r12b'], 'r15': ['r15', 'r15d', 'r15w', 'r15b'],
          'rdx': ['rdx', 'edx', 'dx', 'dl', 'dh'], 'rcx': ['rcx', 'ecx', 'cx', 'cl', 'ch'],
          'rsi': ['rsi', 'esi', 'si', 'sil'], 'rbx': ['rbx', 'ebx', 'bx', 'bl', 'bh']}


def split_ops(text):
    out, depth, cur = [], 0, ''
    for ch in text:
        if ch == ',' and depth == 0:
            out.append(cur.strip())
            cur = ''
            continue
        depth += ch == '('
        depth -= ch == ')'
        cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def may_write(i, reg):
    """Conservative: True unless the instruction provably does not write any part of `reg`."""
    names = FAMILY[reg]
    ops = split_ops(i['ops_text'])
    mentioned = any(re.search(r'%' + n + r'\b', o) for o in ops for n in names)
    if not mentioned:
        return False  # implicit-operand instructions never write r12/r15 (stated assumption)
    if SWAPPING.match(i['mnem']):
        return True
    if NONWRITING.match(i['mnem']):
        return False
    last = ops[-1] if ops else ''
    return any(last == '%' + n for n in names)


def writes_through(i, reg):
    """Conservative: True if the instruction may write memory addressed through `reg`."""
    names = FAMILY[reg]
    ops = split_ops(i['ops_text'])
    if not ops:
        return False
    if NONWRITING.match(i['mnem']):
        return False
    targets = ops if SWAPPING.match(i['mnem']) else ops[-1:]
    return any('(' in o and re.search(r'\(%(' + '|'.join(names) + r')\b', o) for o in targets)


RETAINED = re.compile(r'^\s*([0-9a-f]+):\s+(\S+)(?:\s+(.*))?$')


class RetainedListing(Listing):
    """STATIC-PHASE SUBSTITUTE for `Listing` (used only under the resource pause).

    Parses a retained GNU objdump text listing (no raw bytes; produced earlier by objdump 2.46
    with -C) whose SHA-256 is bound in the eligibility lock, derives instruction lengths from
    consecutive addresses, and takes raw bytes from the exact ELF. It does NOT re-decode the
    bytes with objdump: boundaries and mnemonics are the retained independent decoding. Byte
    patch fixtures may replace a range by the strict decoder's decoding of the patched bytes
    (`patch_range`), which is marked in each replaced line.
    """

    def __init__(self, text, elf, fn, version):
        self.fn, self.version, self.insns = fn, version, []
        start, end = fn['value'], fn['value'] + fn['size']
        rows = []
        for line in text.splitlines():
            m = RETAINED.match(line)
            if not m:
                continue
            a = int(m.group(1), 16)
            if start <= a < end:
                ops = (m.group(3) or '').split('#', 1)[0].split('<', 1)[0].strip()
                rows.append((a, m.group(2), ops, line))
        if not rows or rows[0][0] != start:
            raise ListingError('retained listing does not start at the symbol')
        for k, (a, mn, ops, line) in enumerate(rows):
            nxt = rows[k + 1][0] if k + 1 < len(rows) else end
            if not 0 < nxt - a <= 15:
                raise ListingError(f'retained listing gap or bad length at {a:#x}')
            if '(bad)' in line:
                raise ListingError(f'undecodable bytes at {a:#x}')
            self.insns.append({'addr': a, 'len': nxt - a, 'raw': elf.read(a, nxt - a), 'mnem': mn,
                               'ops_text': ops, 'line': line})
        self.by_addr = {i['addr']: i for i in self.insns}

    def patch_range(self, elf, start, end, decode_range, rows=None):
        """Replaces [start, end) by the strict decoding of the (patched) ELF bytes, or by
        fixture-supplied rows (addr, length, mnemonic, operands) for forms the strict decoder
        does not accept; such rows are marked and are NOT an independent decoding."""
        self.fn = elf.function(self.fn['name'], self.fn['value'])
        self.check_boundary(start)
        self.check_boundary(end)
        new = []
        if rows:
            a = start
            for addr, length, mnem, ops in rows:
                if addr != a:
                    raise ListingError('fixture rows must be contiguous')
                raw = elf.read(addr, length)
                new.append({'addr': addr, 'len': length, 'raw': raw, 'mnem': mnem, 'ops_text': ops,
                            'line': f'{addr:x}: {raw.hex()} {mnem} {ops} [PATCH: FIXTURE-SUPPLIED TEXT, NOT DECODED]'})
                a += length
            if a != end:
                raise ListingError('fixture rows must cover the patch range')
        else:
            for d in decode_range(self.fn['bytes'], self.fn['value'], start, end):
                f = d.fmt().split(' ', 1)
                new.append({'addr': d.addr, 'len': d.length, 'raw': d.raw, 'mnem': d.mnem,
                            'ops_text': f[1] if len(f) > 1 else '',
                            'line': f'{d.addr:x}: {d.raw.hex()} {d.fmt()} [PATCH: STRICT DECODE ONLY]'})
        self.insns = sorted([i for i in self.insns if not start <= i['addr'] < end] + new, key=lambda i: i['addr'])
        for i in self.insns:
            if start <= i['addr'] < end:
                continue
            i['raw'] = elf.read(i['addr'], i['len'])
        self.by_addr = {i['addr']: i for i in self.insns}
