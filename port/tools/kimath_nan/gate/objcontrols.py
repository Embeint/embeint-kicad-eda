# SPDX-License-Identifier: GPL-3.0-or-later
"""Controls for the object-bound gate (objgate.py), run from the repository.

    python -B port/tools/kimath_nan/gate/objcontrols.py --inputs ROOT --objdump objdump --out report.json

ROOT holds the external inputs named in objcontrols.inputs.json, which are not in the repository
(ELF binaries, compiler objects, the reviewed include tree). Every file is checked against its
recorded SHA-256 before anything runs, and the script refuses on any difference or absence.
Nothing is synthesized: without the genuine inputs there is no control run.

Each control mutates one input in a temporary copy and runs the real gate (full phase).
Expectation is stated against the genuine baseline of the same run:

- 'same': the set of failing/not-run rows equals the baseline (binding unaffected);
- a list of row-name substrings that must newly FAIL.

No control is a review. One control writes a synthetic REVIEWED record into a TEMPORARY lock
only, to show that the review row gates exactly itself.
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import struct
import sys
import tempfile
import types

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import objgate  # noqa: E402
from elfbin import Elf  # noqa: E402
from linkproof import Linked, Prover  # noqa: E402
from relobj import RelObj  # noqa: E402
from x86len import decode  # noqa: E402

LOCKS = HERE / 'locks'
DOUBLEOPS, INV = objgate.gate.DOUBLEOPS, '_ZNK9MATRIX3x3IdE7InverseEv'
DESTROY = ('_ZN8nlohmann16json_abi_v3_11_310basic_jsonISt3mapSt6vectorNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE'
           'blmdSaNS0_14adl_serializerES3_IhSaIhEEvE10json_value7destroyENS0_6detail7value_tE')


def sha(b):
    return hashlib.sha256(b).hexdigest()


def tree_digest(root):
    rows = sorted((str(p.relative_to(root)).replace('\\', '/'), sha(p.read_bytes())) for p in root.rglob('*') if p.is_file())
    return sha(json.dumps(rows, separators=(',', ':')).encode()), len(rows)


def check_inputs(root, manifest):
    bad = []
    for rel, want in manifest['files'].items():
        p = root / rel
        if not p.is_file():
            bad.append(f'missing {rel}')
        elif sha(p.read_bytes()) != want:
            bad.append(f'{rel}: sha256 differs')
    for rel, want in manifest['trees'].items():
        p = root / rel
        if not p.is_dir() or tree_digest(p) != (want['sha256'], want['files']):
            bad.append(f'tree {rel}: digest differs or absent')
    if bad:
        sys.exit('REFUSED: control inputs are not the recorded genuine inputs:\n  ' + '\n  '.join(bad))


class Ctl:
    def __init__(self, root, objdump, manifest):
        self.root, self.objdump = root, objdump
        self.tmp = pathlib.Path(tempfile.mkdtemp(prefix='objctl-'))
        prov = self.tmp / 'providers'
        prov.mkdir()
        for n in manifest['link_order']:
            shutil.copyfile(root / 'reviewed' / 'objects' / n, prov / n)
        (prov / 'link-order.txt').write_text('\n'.join(manifest['link_order']) + '\n')
        F = root / 'fresh'
        self.fresh_ci = self.compiler_inputs('ci-fresh', F / 'oracle-src', F / 'oracle-build' / 'compile_commands.json')
        self.rev_ci = self.compiler_inputs('ci-reviewed', root / 'reviewed' / 'tree' / 'oracle-src',
                                           root / 'reviewed' / 'compile_commands.json')
        self.base = {
            'fresh': dict(oracle=F / 'oracle-build' / 'kicad-oracle',
                          object=F / 'oracle-build/CMakeFiles/kicad-oracle.dir/shims/kimath_math.cpp.o',
                          providers=prov, compiler_inputs=self.fresh_ci, toolchain_identity=None, oracle_src=F),
            'reviewed': dict(oracle=root / 'reviewed' / 'kicad-oracle', object=root / 'reviewed/objects/kimath_math.cpp.o',
                             providers=prov, compiler_inputs=self.rev_ci,
                             toolchain_identity=root / 'reviewed' / 'image-identity.json', oracle_src=root / 'reviewed' / 'tree'),
        }
        self.results = []

    def compiler_inputs(self, name, src, cc):
        d = self.tmp / name
        d.mkdir()
        m = src / 'reference' / 'libs' / 'kimath' / 'include' / 'math'
        for f, n in (('matrix3x3.h', 'compiler-input-matrix3x3.h'), ('vector2d.h', 'compiler-input-vector2d.h'),
                     ('vector3.h', 'compiler-input-vector3.h')):
            shutil.copyfile(m / f, d / n)
        shutil.copyfile(src / 'overlay' / 'shims' / 'kimath_math.cpp', d / 'compiler-input-kimath_math.cpp')
        shutil.copyfile(cc, d / 'compile_commands.json')
        return d

    def run(self, cfg, locks=LOCKS, emit=None, phase='full'):
        a = types.SimpleNamespace(oracle=str(cfg['oracle']), object=str(cfg['object']), providers=str(cfg['providers']),
                                  compiler_inputs=str(cfg['compiler_inputs']),
                                  toolchain_identity=str(cfg['toolchain_identity']) if cfg['toolchain_identity'] else None,
                                  oracle_src=str(cfg['oracle_src']), locks=str(locks), phase=phase, objdump=self.objdump,
                                  verify_closure=None, emit=emit)
        return objgate.run(a)

    def tmpfile(self, name, data):
        p = self.tmp / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        return p

    def patched(self, cfg, edits, name):
        d = bytearray(pathlib.Path(cfg['oracle']).read_bytes())
        e = Elf(bytes(d))
        for va, new, old in edits:
            s = e.section_at(va, len(new))
            o = s['offset'] + va - s['addr']
            if old is not None and bytes(d[o:o + len(old)]) != old:
                raise SystemExit(f'{name}: unexpected original bytes at {va:#x}')
            d[o:o + len(new)] = new
        return self.tmpfile(f'{name}/kicad-oracle', bytes(d))

    def control(self, group, name, rep, base_np, expect):
        np_ = nonpass(rep)
        if expect == 'same':
            ok = np_ == base_np
            got = 'same' if ok else sorted(np_ ^ base_np)[:6]
        else:
            names = [n for c, n, s in np_ - base_np if s == 'fail']
            ok = all(any(x in n for n in names) for x in expect) and not rep['decision'].startswith('PASS')
            got = sorted(names)[:8]
        self.results.append({'group': group, 'control': name, 'expected': expect, 'observed': got,
                             'decision': rep['decision'], 'emission': rep.get('emission'), 'pass': bool(ok)})
        print(('PASS ' if ok else 'MISS ') + group + ': ' + name, flush=True)


def nonpass(rep):
    return {(r['category'], r['check'], r['status']) for r in rep['checks'] if r['status'] != 'pass'}


def decode_fn(elf, sym):
    code = elf.read(sym['value'], sym['size'])
    out, pos = [], 0
    while pos < len(code):
        i = decode(code, pos, sym['value'] + pos)
        out.append(i)
        pos += i['len']
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--inputs', required=True, help='root holding the inputs of objcontrols.inputs.json')
    ap.add_argument('--objdump', default='objdump')
    ap.add_argument('--out', required=True)
    a = ap.parse_args()
    root = pathlib.Path(a.inputs).resolve()
    manifest = json.loads((HERE / 'objcontrols.inputs.json').read_text())
    check_inputs(root, manifest)
    k = Ctl(root, a.objdump, manifest)
    fresh, reviewed = k.base['fresh'], k.base['reviewed']
    base = {n: k.run(v) for n, v in k.base.items()}
    base_np = {n: nonpass(v) for n, v in base.items()}
    k.results.append({'group': 'baseline', 'control': 'genuine reviewed and fresh oracles', 'pass': True,
                      **{n: {'decision': v['decision'], 'counts': v['counts'], 'nonpass': sorted(base_np[n])}
                         for n, v in base.items()}})
    elf = Elf(pathlib.Path(fresh['oracle']).read_bytes())
    L = Linked(elf)
    obj = RelObj(pathlib.Path(fresh['object']).read_bytes())
    objs = {n: (obj if n == objgate.HOME_FILE else RelObj((fresh['providers'] / n).read_bytes())) for n in manifest['link_order']}
    olock = json.loads((LOCKS / 'object.lock.json').read_text())
    pv = Prover(L, objs, objgate.HOME_FILE, olock['link']['externals_spec'])
    pv.prove()
    home_base = {objs[objgate.HOME_FILE].sections[x[1]]['name']: v for x, v in pv.base.items() if x[0] == objgate.HOME_FILE}
    fn = lambda name: [s for s in elf.symbols if s['name'] == name and s['type'] == 2][0]
    data = pathlib.Path(fresh['oracle']).read_bytes()
    va_of = lambda off: [s['addr'] + off - s['offset'] for s in elf.sections if s['offset'] <= off < s['offset'] + s['size']
                         and s['addr']][0]

    # separable: unrelated changes leave the binding unchanged
    bid = [s for s in elf.sections if s['name'] == '.note.gnu.build-id'][0]
    k.control('separable', 'build ID rewritten', k.run(dict(fresh, oracle=k.patched(fresh, [(bid['addr'] + 16, bytes(20), None)], 'bid'))),
              base_np['fresh'], 'same')
    key = b'"overlay_revision":"'
    at = data.find(key) + len(key)
    old = data[at:at + 1]
    k.control('separable', 'embedded provenance revision changed',
              k.run(dict(fresh, oracle=k.patched(fresh, [(va_of(at), b'0' if old != b'0' else b'1', old)], 'prov'))),
              base_np['fresh'], 'same')
    victim = [s for s in elf.symbols if s['type'] == 2 and 'IEC60664' in s['name'] and s['size'] > 64][0]
    vo = victim['value'] + 40
    k.control('separable', 'byte in an unrelated unit function (IEC 60664)',
              k.run(dict(fresh, oracle=k.patched(fresh, [(vo, bytes([elf.read(vo, 1)[0] ^ 1]), None)], 'unrel'))),
              base_np['fresh'], 'same')

    # prior counterexamples and mutations
    lit = b'matrix_default\0'
    k.control('op-routing', 'Sol R2 op literal matrix_default -> mul_matrix',
              k.run(dict(fresh, oracle=k.patched(fresh, [(va_of(data.find(lit)), b'mul_matrix\0' + bytes(4), lit)], 'r2'))),
              base_np['fresh'], ['link proof: reachable units'])
    inv = fn(INV)
    insns = decode_fn(elf, inv)
    one = [i['rip_target'] for i in insns if 'rip_target' in i and elf.read(i['rip_target'], 8) == bytes.fromhex('000000000000f03f')][0]
    k.control('constants', 'Inverse 1.0 literal -> 1.0000000000000002',
              k.run(dict(fresh, oracle=k.patched(fresh, [(one, bytes.fromhex('010000000000f03f'), bytes.fromhex('000000000000f03f'))], 'one'))),
              base_np['fresh'], ['link proof: reachable units'])
    mul = [i for i in insns if i['opcode'] == ('0f', 0x59) and i['bytes'][-1] >> 6 == 3][0]
    m = mul['bytes'][-1]
    sw = (m & 0xC0) | ((m & 7) << 3) | ((m >> 3) & 7)
    k.control('operand-order', 'Inverse mulsd register operands swapped (NaN priority only)',
              k.run(dict(fresh, oracle=k.patched(fresh, [(mul['va'] + mul['len'] - 1, bytes([sw]), bytes([m]))], 'swap'))),
              base_np['fresh'], ['link proof: reachable units'])
    k.control('call-stack (R1)', 'Inverse endbr64 -> lea (%rsp),%r8',
              k.run(dict(fresh, oracle=k.patched(fresh, [(inv['value'], bytes.fromhex('4c8d0424'), bytes.fromhex('f30f1efa'))], 'r1'))),
              base_np['fresh'], ['link proof: reachable units'])
    call_inv = [i for i in decode_fn(elf, fn(DOUBLEOPS)) if i['opcode'] == ('1', 0xE8) and i.get('branch_target') == inv['value']]
    other = [s for s in elf.symbols if s['type'] == 2 and s['size'] > 32 and s['value'] != inv['value']][5]
    if call_inv:
        ci = call_inv[0]
        k.control('callees', 'DoubleOps call of Inverse retargeted',
                  k.run(dict(fresh, oracle=k.patched(fresh, [(ci['va'] + 1, struct.pack('<i', other['value'] - (ci['va'] + 5)), None)], 'retarget'))),
                  base_np['fresh'], ['link proof: reachable units'])
    cold = home_base['.text.unlikely'] + 8
    k.control('cold path', 'byte in the home .text.unlikely (DoubleOps cold part)',
              k.run(dict(fresh, oracle=k.patched(fresh, [(cold, bytes([elf.read(cold, 1)[0] ^ 1]), None)], 'cold'))),
              base_np['fresh'], ['link proof: reachable units'])
    dd = [s for s in elf.symbols if s['name'] == DESTROY][0]
    k.control('other-object bodies', 'byte in json_value::destroy (selected from main.cpp.o)',
              k.run(dict(fresh, oracle=k.patched(fresh, [(dd['value'] + 30, bytes([elf.read(dd['value'] + 30, 1)[0] ^ 1]), None)], 'destroy'))),
              base_np['fresh'], ['link proof: reachable units'])
    plt = [s for s in elf.sections if s['name'] == '.plt.sec'][0]
    stubs = {}
    for j in range(plt['size'] // 16):
        try:
            stubs[elf.plt_import(plt['addr'] + 16 * j)['import']] = plt['addr'] + 16 * j
        except Exception:
            pass
    tb = home_base['.text']
    tsym = {'value': tb, 'size': [s for s in obj.sections if s['name'] == '.text'][0]['size']}
    site = [i for i in decode_fn(elf, tsym) if i['opcode'] == ('1', 0xE8) and i.get('branch_target') == stubs.get('memcpy')]
    if site and 'memmove' in stubs:
        s0 = site[0]
        k.control('imports', 'home call of memcpy@plt retargeted to memmove@plt',
                  k.run(dict(fresh, oracle=k.patched(fresh, [(s0['va'] + 1, struct.pack('<i', stubs['memmove'] - (s0['va'] + 5)), None)], 'plt'))),
                  base_np['fresh'], ['link proof: reachable units'])
    vcode = elf.read(victim['value'], 32)
    pos = 0
    while pos < 7:
        pos += decode(vcode, pos, victim['value'] + pos)['len']
    plant = bytes.fromhex('0fae1500000000') + b'\x90' * (pos - 7)
    k.control('FP environment', 'ldmxcsr planted in an unrelated unit function',
              k.run(dict(fresh, oracle=k.patched(fresh, [(victim['value'], plant, None)], 'ldmxcsr'))),
              base_np['fresh'], ['decoder sweep', 'objdump scan'])

    # object and inputs
    ob = bytearray(pathlib.Path(fresh['object']).read_bytes())
    ob[-1] ^= 1
    k.control('object', 'home object last byte flipped', k.run(dict(fresh, object=k.tmpfile('obj/kimath_math.cpp.o', bytes(ob)))),
              base_np['fresh'], ['home object sha256'])
    ci = k.tmp / 'ci-header'
    shutil.copytree(fresh['compiler_inputs'], ci)
    h = (ci / 'compiler-input-matrix3x3.h').read_bytes().replace(b'm_data[1][2] * m_data[2][1]', b'm_data[2][1] * m_data[1][2]', 1)
    (ci / 'compiler-input-matrix3x3.h').write_bytes(h)
    k.control('source', 'matrix3x3.h operand order edited', k.run(dict(fresh, compiler_inputs=ci)), base_np['fresh'], ['header is Git blob'])
    ci2 = k.tmp / 'ci-flags'
    shutil.copytree(fresh['compiler_inputs'], ci2)
    cc = json.loads((ci2 / 'compile_commands.json').read_text())
    for c in cc:
        if c['file'].endswith('shims/kimath_math.cpp'):
            c['command'] = c['command'].replace(' -O3 ', ' -O2 ')
    (ci2 / 'compile_commands.json').write_text(json.dumps(cc))
    k.control('compile flags', 'adapter -O3 -> -O2 in the compile database', k.run(dict(fresh, compiler_inputs=ci2)),
              base_np['fresh'], ['adapter compile command', 'every compilation strict'])
    ident = json.loads(pathlib.Path(reviewed['toolchain_identity']).read_text())
    ident['compiler']['g++']['version'] = '13.3.1'
    k.control('toolchain', 'reviewed build with an altered toolchain identity record',
              k.run(dict(reviewed, toolchain_identity=k.tmpfile('id/image-identity.json', (json.dumps(ident, indent=2, sort_keys=True) + '\n').encode()))),
              base_np['reviewed'], ['toolchain identity record', 'toolchain identity names'])

    # locks (temporary copies only)
    def lockdir(name, mutate):
        d = k.tmp / ('locks-' + name)
        shutil.copytree(LOCKS, d)
        o = json.loads((d / 'object.lock.json').read_text())
        g = json.loads((d / 'graphs.lock.json').read_bytes())
        g0 = json.dumps(g, sort_keys=True)
        mutate(o, g)
        (d / 'object.lock.json').write_text(json.dumps(o, indent=1, sort_keys=True) + '\n')
        if json.dumps(g, sort_keys=True) != g0:
            (d / 'graphs.lock.json').write_text(json.dumps(g, indent=1))
        return d
    k0 = sorted(olock['link']['provider_units'])[0]
    k.control('locks', 'object lock: one other-object digest tampered',
              k.run(fresh, locks=lockdir('digest', lambda o, g: o['link']['provider_units'].update({k0: '0' * 64}))),
              base_np['fresh'], ['other-object bodies equal'])
    k.control('locks', 'object lock: one context path digest tampered',
              k.run(fresh, locks=lockdir('ctx', lambda o, g: o['context']['inverse'].update(path_object_bytes_sha256='0' * 64))),
              base_np['fresh'], ['inverse: path bytes equal'])
    k.control('graph drift', 'graphs lock: inverse fingerprint differs (ordered-graph drift)',
              k.run(fresh, locks=lockdir('drift', lambda o, g: g['sites']['inverse'].update(fingerprint='0' * 64)), emit=str(k.tmp / 'e.rs')),
              base_np['fresh'], ['inverse: arithmetic graph fingerprint'])
    rv = objgate.OBJECT_CONTEXT_REVIEW
    k.control('review record', 'synthetic REVIEWED record with the right link digest clears only that row (temporary fixture, not a review)',
              k.run(fresh, locks=lockdir('rv', lambda o, g: o['context_review'].update(status='REVIEWED', reviewer='SYNTHETIC-FIXTURE'))),
              base_np['fresh'] - {('context', rv, 'fail')}, 'same')
    k.control('review record', 'REVIEWED record bound to another link digest',
              k.run(fresh, locks=lockdir('rv2', lambda o, g: o['context_review'].update(status='REVIEWED', reviewer='SYNTHETIC-FIXTURE', link_digest='0' * 64))),
              base_np['fresh'] - {('context', rv, 'fail')}, ['object context review'])
    st = k.run(reviewed, phase='static', emit=str(k.tmp / 's.rs'))
    k.results.append({'group': 'emission', 'control': 'static object report never emits or reads as a pass',
                      'decision': st['decision'], 'emission': st['emission'],
                      'pass': not st['decision'].startswith('PASS') and str(st['emission']).startswith('REFUSED')
                      and not (k.tmp / 's.rs').exists()})
    print(('PASS ' if k.results[-1]['pass'] else 'MISS ') + 'emission: static report refused', flush=True)
    summary = {'total': len(k.results) - 1, 'as_expected': sum(1 for r in k.results[1:] if r['pass'])}
    pathlib.Path(a.out).write_text(json.dumps({'summary': summary, 'results': k.results}, indent=1, default=str))
    print(summary)
    shutil.rmtree(k.tmp, ignore_errors=True)
    sys.exit(0 if summary['total'] == summary['as_expected'] else 1)


if __name__ == '__main__':
    main()
