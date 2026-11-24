# SPDX-License-Identifier: GPL-3.0-or-later
"""Execution-mapping controls for elfexec.py on the genuine reviewed and fresh oracles.

    python -B port/tools/kimath_nan/gate/elfexec_controls.py --inputs ROOT --out report.json

ROOT holds the inputs of objcontrols.inputs.json (both oracles are checked by SHA-256 there).
Both genuine oracles must pass every row. Each mutation changes one execution fact in a
temporary in-memory copy of the fresh oracle and must fail the named rows. No mutant is executed.
"""
import argparse
import hashlib
import json
import pathlib
import struct
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import elfexec  # noqa: E402

ORACLES = {'reviewed': 'reviewed/kicad-oracle', 'fresh': 'fresh/oracle-build/kicad-oracle'}
R = elfexec.ROWS


def sha(b):
    return hashlib.sha256(b).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--inputs', required=True)
    ap.add_argument('--out', required=True)
    a = ap.parse_args()
    root = pathlib.Path(a.inputs)
    manifest = json.loads((HERE / 'objcontrols.inputs.json').read_text(encoding='utf-8'))
    data = {}
    for k, rel in ORACLES.items():
        b = (root / rel).read_bytes()
        if sha(b) != manifest['files'][rel]:
            sys.exit(f'REFUSED: {rel} is not the recorded genuine input')
        data[k] = b
    ext = json.loads((HERE / 'locks' / 'object-elfexec.lock.json').read_text(encoding='utf-8'))
    imports = json.loads((HERE / 'locks' / 'object.lock.json').read_text(encoding='utf-8'))['link']['imports']
    results = []

    def failing(b, e=ext):
        return [n for n, ok, det in elfexec.check(b, e, imports) if not ok]

    for k, b in data.items():
        f = failing(b)
        results.append({'control': f'genuine {k} oracle accepted', 'failing': f, 'pass': not f})
    F = data['fresh']
    raw = elfexec.Raw(F)
    ph = lambda i, field: raw.e_phoff + 56 * i + {'type': 0, 'flags': 4, 'offset': 8, 'vaddr': 16, 'paddr': 24,
                                                   'filesz': 32, 'memsz': 40, 'align': 48}[field]
    idx = {p['type']: i for i, p in reversed(list(enumerate(raw.phs)))}
    load_i = [i for i, p in enumerate(raw.phs) if p['type'] == 1]
    dyn = raw.by['.dynamic']
    dyn_tags = [struct.unpack_from('<qQ', F, dyn['offset'] + 16 * k) for k in range(dyn['size'] // 16)]
    dt_at = lambda tag, n=0: dyn['offset'] + 16 * [k for k, (t, v) in enumerate(dyn_tags) if t == tag][n]
    shoff = lambda name, field: raw.e_shoff + 64 * raw.by[name]['index'] + {'offset': 24, 'addr': 16}[field]

    def mut(edits):
        b = bytearray(F)
        for off, fmt, val in edits:
            if callable(val):
                val = val(struct.unpack_from(fmt, b, off)[0])
            struct.pack_into(fmt, b, off, val)
        return bytes(b)

    def control(name, edits, expect):
        b = mut(edits)
        f = failing(b)
        ok = all(any(R[e] == n for n in f) for e in expect)
        results.append({'control': name, 'sha256': sha(b), 'expected_failing': [R[e] for e in expect], 'failing': f, 'pass': ok})

    rx, ro, rw = load_i[1], load_i[2], load_i[3]
    control('e_phnum = 0 (parent probe, sha 61c53989...)', [(56, '<H', 0)], [0])
    control('e_phoff moved by one entry', [(32, '<Q', raw.e_phoff + 56)], [0])
    control('executable LOAD loses PF_X', [(ph(rx, 'flags'), '<I', 4)], [1, 2])
    control('read-only data LOAD gains PF_X', [(ph(ro, 'flags'), '<I', 5)], [1, 2])
    control('executable LOAD maps another file offset', [(ph(rx, 'offset'), '<Q', lambda v: v + 0x1000)], [2])
    control('executable LOAD overlaps the next mapping', [(ph(rx, 'memsz'), '<Q', lambda v: v + 0x2000),
                                                           (ph(rx, 'filesz'), '<Q', lambda v: v + 0x2000)], [1])
    control('writable LOAD gains PF_X', [(ph(rw, 'flags'), '<I', 7)], [1, 2])
    control('.text section header points at other file bytes than the mapping', [(shoff('.text', 'offset'), '<Q', lambda v: v + 0x10)], [2])
    gap = raw.by['.init']['offset'] + raw.by['.init']['size']
    control('non-zero byte in executable padding', [(gap, '<B', 0xC3)], [3])
    other = raw.by['.text']['addr'] + 0x100
    control('entry point moved inside .text', [(24, '<Q', other)], [4])
    control('entry point moved outside the executable mapping', [(24, '<Q', raw.by['.rodata']['addr'])], [4])
    interp = raw.by['.interp']['offset']
    control('interpreter path changed', [(interp + 1, '<4s', b'tmp6')], [5])
    control('PT_INTERP redirected', [(ph(idx[3], 'offset'), '<Q', lambda v: v + 8), (ph(idx[3], 'vaddr'), '<Q', lambda v: v + 8),
                                     (ph(idx[3], 'paddr'), '<Q', lambda v: v + 8)], [5])
    control('PT_GNU_STACK executable', [(ph(idx[0x6474E551], 'flags'), '<I', 7)], [1, 5])
    control('PT_GNU_RELRO extended over .data', [(ph(idx[0x6474E552], 'memsz'), '<Q', lambda v: v + 0x2000),
                                                 (ph(idx[0x6474E552], 'filesz'), '<Q', lambda v: v + 0x2000)], [5])
    control('PT_DYNAMIC redirected to .data', [(ph(idx[2], 'offset'), '<Q', raw.by['.data']['offset']),
                                               (ph(idx[2], 'vaddr'), '<Q', raw.by['.data']['addr']),
                                               (ph(idx[2], 'paddr'), '<Q', raw.by['.data']['addr'])], [6])
    control('PT_DYNAMIC removed (type PT_NULL)', [(ph(idx[2], 'type'), '<I', 0)], [1, 6])
    control('DT_RELA/DT_RELASZ skip the first relocation', [(dt_at(7) + 8, '<Q', lambda v: v + 24), (dt_at(8) + 8, '<Q', lambda v: v - 24)], [6])
    control('DT_JMPREL redirected', [(dt_at(23) + 8, '<Q', lambda v: v + 24)], [6])
    control('DT_SYMTAB redirected', [(dt_at(6) + 8, '<Q', lambda v: v + 24)], [6])
    control('DT_STRTAB redirected', [(dt_at(5) + 8, '<Q', lambda v: v + 1)], [6])
    control('DT_NEEDED names another string', [(dt_at(1, 2) + 8, '<Q', lambda v: v + 3)], [6])
    control('DT_DEBUG replaced by DT_RPATH', [(dt_at(21), '<q', 15)], [6])
    control('DT_FLAGS_1 loses PIE/NOW', [(dt_at(0x6FFFFFFB) + 8, '<Q', 0)], [6])
    rel = raw.by['.rela.dyn']
    k = next(k for k in range(rel['size'] // 24) if struct.unpack_from('<QQq', F, rel['offset'] + 24 * k)[1] & 0xFFFFFFFF == 6)
    control('a GLOB_DAT relocation becomes IRELATIVE', [(rel['offset'] + 24 * k + 8, '<Q', lambda v: (v & ~0xFFFFFFFF) | 37)], [7])
    k2 = next(k for k in range(rel['size'] // 24) if struct.unpack_from('<QQq', F, rel['offset'] + 24 * k)[1] & 0xFFFFFFFF == 8)
    control('a RELATIVE relocation retargets read-only data', [(rel['offset'] + 24 * k2, '<Q', raw.by['.rodata']['addr'])], [7])
    ds, vs, dstr = raw.by['.dynsym'], raw.by['.gnu.version'], raw.by['.dynstr']
    names = {}
    for i in range(ds['size'] // 24):
        nm = struct.unpack_from('<I', F, ds['offset'] + 24 * i)[0]
        names[F[dstr['offset'] + nm:F.index(b'\0', dstr['offset'] + nm)].decode()] = i
    vm = struct.unpack_from('<H', F, vs['offset'] + 2 * names['memcpy'])[0]
    vo = struct.unpack_from('<H', F, vs['offset'] + 2 * names['strlen'])[0]
    if vm != vo:
        control('memcpy binds another symbol version', [(vs['offset'] + 2 * names['memcpy'], '<H', vo)], [8])
    results.append({'control': 'extension lock with a reviewer-free status other than DERIVED-PENDING-REVIEW',
                    'failing': failing(F, dict(ext, status='REVIEWED')), 'pass': R[9] in failing(F, dict(ext, status='REVIEWED'))})
    summary = {'total': len(results), 'as_expected': sum(1 for r in results if r['pass'])}
    for r in results:
        print('PASS' if r['pass'] else 'MISS', r['control'])
    pathlib.Path(a.out).write_text(json.dumps({'summary': summary, 'results': results}, indent=1), encoding='utf-8')
    print(summary)
    sys.exit(0 if summary['total'] == summary['as_expected'] else 1)


if __name__ == '__main__':
    main()
