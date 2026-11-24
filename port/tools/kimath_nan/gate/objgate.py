# SPDX-License-Identifier: GPL-3.0-or-later
"""Object-bound eligibility mode for the MATRIX3x3<double> NaN-site gate (EXPERIMENTAL; pending review).

The reviewed gate (gate.py) binds eligibility and context to one whole linked oracle (sha256
0bac7567...): any rebuild changes it, because the binary embeds its own revision, paths and
compile database. This mode binds the code the unit executes instead:

- eligibility: the exact compiled matrix adapter object (sha256), its compiler inputs (Git
  blobs), its compile command (build root normalized) and strict flags, the toolchain closure
  identity (the reviewed image-identity.json bytes and the reviewed input lock), the linked
  oracle's ELF identity fields, a link proof (linkproof.py) that every reachable section and
  selected weak/COMDAT body is exactly the reviewed object bytes after independently verified
  relocations, with every body from another object bound by its reviewed digest, and the
  whole linked oracle's floating-point environment;
- context: the reviewed context bindings translated to the fresh layout through the proven
  units; path digests over the matrix object's own bytes (layout independent) and callee
  identities from the link proof; a context review record bound to the object and link digest
  (PENDING: no reviewer has signed it; the context stage cannot pass);
- arithmetic: the reviewed ordered graph extraction re-run on the fresh binary at translated
  addresses, compared with the reviewed graph lock (drift fails), and the finite proof;
- scope: the complete site inventory and the complete check inventory;
- emission: only from a PASS in the same run, and only if the emitted bytes hash to the graph
  lock's generated_rs_sha256.

usage:
  objgate.py derive --oracle L0 --object O --providers DIR ... --out object.lock.json
  objgate.py verify --oracle L --object O --providers DIR --compiler-inputs DIR
                    --toolchain-identity image-identity.json --locks DIR --out REPORT
                    [--phase full|static] [--oracle-src DIR] [--objdump PATH]
                    [--verify-closure] [--emit FILE]
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import struct
import subprocess
import sys
import tempfile
import types

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
import emit  # noqa: E402
import gate  # noqa: E402
import graph  # noqa: E402
import objschema  # noqa: E402
import srcexpr  # noqa: E402
from elfbin import Elf, ElfError, git_blob_id  # noqa: E402
from linkproof import Linked, LinkError, Prover, named  # noqa: E402
from listing import ListingError  # noqa: E402
from relobj import RelObj, ObjError, SHF_EXECINSTR  # noqa: E402
from symex import Machine, SymexError  # noqa: E402
from x86len import DecodeError, decode  # noqa: E402

MODE = 'object-v1'
HOME_FILE = 'kimath_math.cpp.o'
E, C, S, A = 'eligibility', 'context', 'scope', 'arithmetic'
FP_WRITES = {('0f', 0xAE): {1: 'fxrstor', 2: 'ldmxcsr', 5: 'xrstor'}, ('1', 0xD9): {4: 'fldenv', 5: 'fldcw'},
             ('1', 0xDD): {4: 'frstor'}}
OBJECT_CONTEXT_REVIEW = 'object context review signed by a named reviewer for this object and link digest'
OBJECT_INVENTORY = 'object gate check inventory equals the object-mode inventory for these locks'


def sha(b):
    return hashlib.sha256(b).hexdigest()


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(',', ':')).encode()


# -- inputs ---------------------------------------------------------------------------------
class Inputs:
    def __init__(self, a):
        self.oracle_path = pathlib.Path(a.oracle)
        self.oracle = self.oracle_path.read_bytes()
        self.object = pathlib.Path(a.object).read_bytes()
        pdir = pathlib.Path(a.providers)
        order = [l.strip() for l in (pdir / 'link-order.txt').read_text().splitlines() if l.strip()] \
            if (pdir / 'link-order.txt').is_file() else sorted(p.name for p in pdir.glob('*.o'))
        if HOME_FILE not in order:
            raise ValueError('link-order.txt must list the home object at its link position')
        self.order = order
        self.providers = {n: (pdir / n).read_bytes() for n in order if n != HOME_FILE}
        self.files, self.bytes = {}, {}
        if getattr(a, 'compiler_inputs', None):
            d = pathlib.Path(a.compiler_inputs)
            for k, n in (('header', 'compiler-input-matrix3x3.h'), ('vector2', 'compiler-input-vector2d.h'),
                         ('vector3', 'compiler-input-vector3.h'), ('adapter', 'compiler-input-kimath_math.cpp'),
                         ('compile_commands', 'compile_commands.json')):
                self.files[k] = d / n
                self.bytes[k] = (d / n).read_bytes()
        self.identity = pathlib.Path(a.toolchain_identity).read_bytes() if getattr(a, 'toolchain_identity', None) else None
        self.oracle_src = pathlib.Path(a.oracle_src) if getattr(a, 'oracle_src', None) else None


def dt_needed(elf):
    dyn = [s for s in elf.sections if s['name'] == '.dynamic']
    dynstr = [s for s in elf.sections if s['name'] == '.dynstr']
    if len(dyn) != 1 or len(dynstr) != 1:
        return None
    out = []
    for k in range(dyn[0]['size'] // 16):
        tag, val = struct.unpack_from('<qQ', elf.data, dyn[0]['offset'] + 16 * k)
        if tag == 1:
            out.append(elf._str(dynstr[0]['offset'], val))
        if tag == 0:
            break
    return out


def fp_sweep(elf):
    """Decodes every byte of every executable section (FUNC ranges, then the gaps between them)."""
    hits, errors, covered = [], [], 0
    funcs = sorted((s['value'], s['value'] + s['size']) for s in elf.symbols if s['type'] == 2 and s['size'])
    for sec in elf.sections:
        if not sec['flags'] & SHF_EXECINSTR or sec['type'] == 8:
            continue
        lo, hi = sec['addr'], sec['addr'] + sec['size']
        code = elf.data[sec['offset']:sec['offset'] + sec['size']]
        starts = sorted({lo} | {a for a, b in funcs if lo <= a < hi} | {b for a, b in funcs if lo < b < hi})
        for i, st in enumerate(starts):
            end = starts[i + 1] if i + 1 < len(starts) else hi
            pos = st - lo
            while pos < end - lo:
                try:
                    ins = decode(code, pos, lo + pos)
                except DecodeError as e:
                    errors.append(f'{sec["name"]}: {e}')
                    break
                names = FP_WRITES.get(ins['opcode'])
                if names and ins['modrm_reg'] in names and ins['modrm_mod'] != 3:
                    hits.append((hex(ins['va']), names[ins['modrm_reg']]))
                pos += ins['len']
                covered += ins['len']
    return hits, errors[:10], covered


# -- link layout and translation ------------------------------------------------------------
def unit_intervals(prover):
    """[(start, end, key)] of every proven unit in the linked oracle (key is layout independent)."""
    out = []
    for u, digest in prover.done.items():
        o = prover.objects[u[0]]
        s = o.sections[u[1]]
        base = prover.unit_base.get(u, prover.base.get((u[0], u[1])))
        if base is None or not s['size']:
            continue
        if u[0] == prover.home:
            out.append((base, base + s['size'], ('home', s['name'])))
        elif len(u) == 4:
            fn = [y['name'] for y in named(o, u[1]) if y['value'] == u[2]]
            out.append((base + u[2], base + u[3], ('provider', fn[0] if fn else f'{s["name"]}+{u[2]}', digest)))
        else:
            out.append((base, base + s['size'], ('provider', s['name'], digest)))
    return sorted(out)


def identify(L, intervals, T):
    plt = L.plt_name(T)
    if plt is not None:
        return ['plt', plt]
    hit = [(a, b, k) for a, b, k in intervals if a <= T < b]
    if len(hit) != 1:
        return ['unproven', hex(T)]
    a, _b, k = hit[0]
    return list(k) + [T - a]


class Translator:
    """Maps reviewed-layout addresses to the current layout through the proven units."""

    def __init__(self, ref, intervals):
        self.ref = [(r['start'], r['end'], tuple(r['key'])) for r in ref]
        self.cur = {k: a for a, _b, k in intervals}

    def __call__(self, a0):
        hit = [(s, k) for s, e, k in self.ref if s <= a0 < e]
        if len(hit) != 1 or hit[0][1] not in self.cur:
            raise LinkError(f'reviewed address {a0:#x} is not inside one proven unit of this link')
        s, k = hit[0]
        return self.cur[k] + (a0 - s)


def translated_lock(elock, tr, elf):
    """The reviewed lock with every address mapped through the proof; callee byte digests are
    recomputed only for callees inside proven units (tr refuses anything else)."""
    L = json.loads(json.dumps(elock))
    h = lambda v: hex(tr(int(v, 16)))
    for b in L['entry_registers'].values():
        b['binding_insn'] = h(b['binding_insn'])
    L['op_binding']['binding_insn'] = h(L['op_binding']['binding_insn'])
    L['frame']['setup_insn'] = h(L['frame']['setup_insn'])
    callees = {}
    for a0, b in elock['callees'].items():
        a = tr(int(a0, 16))
        fn = elf.function(b['symbol'], a)
        callees[hex(a)] = dict(b, sha256=fn['sha256'])
    L['callees'] = callees
    for s in L['sites'].values():
        s['start'], s['stop'] = h(s['start']), h(s['stop'])
        s['branches'] = {h(k): dict(v, target=h(v['target'])) for k, v in s['branches'].items()}
    return L


def dispatch_check_t(lst, elf, start, d, eq_addr):
    """gate.dispatch_check with the reviewed operator== call target translated (no other change)."""
    preds = [i for i in lst.insns if (lst.successors(i) or []) and start in (lst.successors(i) or [])
             and i['addr'] + i['len'] != start]
    if d['kind'] == 'jne':
        if len(preds) != 1 or preds[0]['mnem'] != 'jne':
            raise gate.GateError(f'{start:#x}: expected exactly one jne predecessor')
        j = preds[0]
    else:
        prev = [i for i in lst.insns if i['addr'] + i['len'] == start]
        if preds or len(prev) != 1 or prev[0]['mnem'] != 'je':
            raise gate.GateError(f'{start:#x}: expected fall-through after je and no jumps into it')
        j = prev[0]
    k = lst.insns.index(j)
    seq = lst.insns[k - 4:k + 1]
    shape = [f"{i['mnem']} {i['ops_text']}" for i in seq]
    if not (re.fullmatch(r'lea 0x[0-9a-f]+\(%rip\),%rsi', shape[0]) and shape[1] == 'mov %rbx,%rdi'
            and shape[2] == f'call {eq_addr:x}' and shape[3] == 'test %al,%al'):
        raise gate.GateError(f'dispatch sequence for {start:#x} differs: {shape}')
    lea = gate.decode_one(lst.fn['bytes'], seq[0]['addr'] - lst.fn['value'], seq[0]['addr'])
    literal = elf.cstring(lea.ops[0].rip_target)
    if literal != d['literal']:
        raise gate.GateError(f'dispatch literal {literal!r} differs from {d["literal"]!r}')
    return literal


def extract_t(elf, L, sid, listings, eq_addr):
    """gate.extract with dispatch_check_t (translated operator== address)."""
    s = L['sites'][sid]
    fn = elf.function(s['function'])
    dlst = listings[s['function']]
    start, stop = int(s['start'], 16), int(s['stop'], 16)
    dlst.check_boundary(start)
    dlst.check_boundary(stop)
    if not (fn['value'] <= start < stop < fn['value'] + fn['size']):
        raise gate.GateError('window outside its function')
    dispatch_check_t(dlst, elf, start, s['dispatch'], eq_addr)
    bindings = {int(a, 16): b for a, b in L['callees'].items()}
    branches = {int(a, 16): dict(b, target=int(b['target'], 16)) for a, b in s['branches'].items()}
    mach = Machine(elf, bindings, s['roles'], branches, frame_size=L['frame']['size'])
    mach.gpr[12] = ('json_input',)
    mach.gpr[15] = ('kind_arg',)
    outs = mach.run(fn, start, stop, s['consumer'])
    traced = gate.cross_check(mach.trace, list(listings.values()))
    if list(outs) != s['output_order']:
        raise gate.GateError(f'output names {list(outs)} differ from {s["output_order"]}')
    return outs, {'path': mach.path, 'calls': mach.calls, 'instructions': traced}


def object_path(lst, sid, start, obj, text_base, text_size, L, intervals, Lk):
    """Layout-independent context of one site: path instruction count, a digest of the path's
    bytes taken from the home OBJECT (bound by its SHA-256), callee identities from the proof."""
    rel = sorted(lst.paths_to(start))
    text = obj.section_bytes([s for s in obj.sections if s['name'] == '.text'][0]['index'])
    blob, calls = b'', {}
    for a in rel:
        i = lst.by_addr[a]
        if not text_base <= a < text_base + text_size:
            raise gate.GateError(f'{sid}: path instruction {a:#x} outside the home .text')
        off = a - text_base
        blob += struct.pack('<IB', a - lst.fn['value'], i['len']) + text[off:off + i['len']]
        if i['mnem'] == 'call':
            t = re.fullmatch(r'([0-9a-f]+)', i['ops_text'])
            calls[str(a - lst.fn['value'])] = identify(Lk, intervals, int(t.group(1), 16)) if t else ['indirect']
    return {'path_instructions': len(rel), 'path_object_bytes_sha256': sha(blob),
            'path_callees_sha256': sha(canon(calls)), 'unproven_callees': [v for v in calls.values() if v[0] == 'unproven']}


# -- inventory --------------------------------------------------------------------------------
def inventory(elock, olock):
    schema = gate.Checks()
    gate.schema(schema, elock, None)
    # The graph schema has the same fixed inventory as the reviewed gate's schema.
    inv = [(r['category'], r['check']) for r in schema.rows]
    inv += [('schema', 'graphs lock: top-level fields')]
    inv += [('schema', f'graphs lock: {k}') for k in gate.lockschema.GRAPHS[1]]
    e = ['object lock: top-level fields', 'object lock mode and status fields',
         'object lock: recursive typed fields',
         'home object sha256 equals the reviewed object', 'home object: x86-64 relocatable with the reviewed .comment']
    e += [f'{k} is Git blob {b["blob"][:12]} ({b["git_path"]})' for k, b in elock['source']['blobs'].items()]
    e += ['adapter compile command equals the reviewed command (build root normalized)',
          'every compilation strict, no forbidden FP/ISA flags; adapter compiled once',
          'header include dir present in adapter command',
          'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
          'compiler-input tree files equal the bound files',
          'adapter definitions equal the reviewed definitions',
          'toolchain identity record is the reviewed closure (sha256)',
          'toolchain identity names the reviewed input lock and compilers',
          'toolchain closure re-verified on this machine (verify_image.py)',
          'linked oracle: ELF64 x86-64 executable, baseline ISA, reviewed DT_NEEDED',
          'linked oracle .comment names the reviewed compiler',
          'link proof: reachable units are the relocated reviewed object bytes',
          'link proof: home sections equal the reviewed set',
          'link proof: other-object bodies equal the reviewed digests',
          'link proof: dynamic imports equal the reviewed set',
          'link proof: externals equal the reviewed bindings',
          'FP environment: no MXCSR/x87-control writes in any executable byte (decoder sweep)',
          'FP environment: no MXCSR/x87-control writes in .text (objdump scan)',
          'no FP-environment functions referenced',
          'runtime FP-mode contract recorded (measurement is runtime qualification)']
    inv += [(E, n) for n in e]
    c = ['reviewed addresses translated through proven units', 'complete function listings',
         'r12/r15 prologue bindings (straight-line, sources unwritten)', 'aOp -> %rbx prologue binding (mov %rsi,%rbx)',
         f'frame setup {elock["frame"]["setup_text"]!r} and reviewed frame size {elock["frame"]["size"]:#x}',
         OBJECT_CONTEXT_REVIEW]
    for sid in contract.REQUIRED_SITES:
        c += [f'{sid}: r12 (aInput) and r15 (aScalar) unwritten on every path',
              f'{sid}: no write through the request pointer %r12 on any path', f'{sid}: path instruction count',
              f'{sid}: path bytes equal the reviewed object path bytes',
              f'{sid}: path callees are the reviewed proven identities']
    inv += [(C, n) for n in c]
    inv += [(S, f'{k} == reviewed required sites ({len(contract.REQUIRED_SITES)})')
            for k in ('eligibility locators', 'object context bindings', 'report sites extracted', 'graph locks')]
    inv.append((S, OBJECT_INVENTORY))
    a = ['source definitions derived from the exact header', 'source data-member layouts']
    for sid in contract.REQUIRED_SITES:
        a += [f'{sid}: site fields equal the canonical contract', f'{sid}: finite proof',
              f'{sid}: arithmetic graph fingerprint', f'{sid}: graph node count', f'{sid}: graph source',
              f'{sid}: source definition hash']
    inv += [(A, n) for n in a]
    return inv


OBJECT_LOCK_FIELDS = ['note', 'mode', 'status', 'home_object', 'compile', 'toolchain', 'linked', 'link',
                      'reference_layout', 'context', 'context_review', 'eq_operator', 'emission']


# -- verify -----------------------------------------------------------------------------------
def normalize_command(cmd, root):
    return cmd.replace(root, '<ROOT>')


def build_root(adapter_file):
    suffix = '/oracle-src/overlay/shims/kimath_math.cpp'
    if not adapter_file.endswith(suffix):
        raise ValueError(f'adapter path {adapter_file} is not <root>{suffix}')
    return adapter_file[:-len(suffix)]


def verify_installed_closure(identity_path):
    """Run the repository's reviewed verifier; callers cannot substitute a success command.

    The identity's exact reviewed digest is checked separately. The installed input lock and
    candidate belong to the image, while the verifier is part of the sealed source checkout.
    """
    verifier = HERE.parents[3] / 'port/reference-env/image/verify_image.py'
    command = [sys.executable, str(verifier), '--lock', '/opt/refenv/inputs.lock.json',
               '--candidate', '/opt/refenv/candidate.toml', '--expect', str(identity_path)]
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=180)
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, {'command': command, 'error': str(error)}
    return result.returncode == 0, {'command': command, 'returncode': result.returncode,
                                    'output': (result.stdout + result.stderr)[-400:]}


def accept_emission(report, text, graph_hash, object_hash, output):
    """Only the current complete run can reach this writer; rejection preserves old output."""
    if not report['decision'].startswith('PASS'):
        report['emission'] = f'REFUSED: decision is {report["decision"]!r}'
        return
    digest = sha(text.encode())
    if digest != graph_hash or digest != object_hash:
        report['emission'] = f'REFUSED: emitted bytes {digest} differ from the reviewed hash'
        report['decision'] = 'FAIL'
        report['failures'].append('emission: generated bytes do not match both locks')
        return
    target = pathlib.Path(output)
    temp = None
    try:
        with tempfile.NamedTemporaryFile(dir=target.parent, prefix=target.name + '.', delete=False) as stream:
            temp = pathlib.Path(stream.name)
            stream.write(text.encode())
        os.replace(temp, target)
        report['emission'] = digest
    finally:
        if temp is not None:
            temp.unlink(missing_ok=True)


def run(a):
    ck = gate.Checks()
    locks = pathlib.Path(a.locks)
    raw = {n: (locks / n).read_bytes() for n in ('eligibility.lock.json', 'graphs.lock.json', 'object.lock.json')}
    elock, glock, olock = (json.loads(raw[n]) for n in ('eligibility.lock.json', 'graphs.lock.json', 'object.lock.json'))
    inp = Inputs(a)
    report = {'note': 'EXPERIMENTAL object-bound gate; not a qualification', 'mode': MODE, 'phase': a.phase,
              'bindings': {n: sha(b) for n, b in raw.items()},
              'inputs': {'oracle': sha(inp.oracle), 'object': sha(inp.object),
                         'providers': {n: sha(b) for n, b in inp.providers.items()},
                         'compiler_inputs': {k: sha(v) for k, v in inp.bytes.items()},
                         'toolchain_identity': sha(inp.identity) if inp.identity else None}}
    # schema
    gate.schema(ck, elock, glock)
    ck.add(E, 'object lock: top-level fields', sorted(olock) == sorted(OBJECT_LOCK_FIELDS), sorted(olock))
    ck.add(E, 'object lock mode and status fields', olock.get('mode') == MODE and isinstance(olock.get('status'), str))
    schema_errors = objschema.errors(olock, glock['generated_rs_sha256'])
    ck.add(E, 'object lock: recursive typed fields', not schema_errors, schema_errors[:8])
    # object and source
    obj = RelObj(inp.object)
    ho = olock['home_object']
    ck.add(E, 'home object sha256 equals the reviewed object', obj.sha256 == ho['sha256'], obj.sha256)
    ck.add(E, 'home object: x86-64 relocatable with the reviewed .comment', obj.comment() == ho['comment'], obj.comment())
    for k, b in elock['source']['blobs'].items():
        ok = k in inp.bytes and git_blob_id(inp.bytes[k]) == b['blob']
        ck.add(E, f'{k} is Git blob {b["blob"][:12]} ({b["git_path"]})', ok)
    adapter_cc = None
    try:
        cc = json.loads(inp.bytes['compile_commands'])
        ad = [c for c in cc if c['file'].endswith('/oracle-src/overlay/shims/kimath_math.cpp')
              and c['output'] == elock['adapter_compile']['output']]
        if len(ad) == 1:
            root = build_root(ad[0]['file'])
            adapter_cc = (ad[0], root)
        ck.add(E, 'adapter compile command equals the reviewed command (build root normalized)',
               adapter_cc is not None and normalize_command(ad[0]['command'], adapter_cc[1]) == olock['compile']['command'],
               adapter_cc and normalize_command(ad[0]['command'], adapter_cc[1]))
        bad = []
        for c in cc:
            args = c['command'].split()
            if args[0] != olock['compile']['compiler_path']:
                bad.append((c['file'], 'compiler', args[0]))
            if not all(f in args for f in gate.STRICT):
                bad.append((c['file'], 'missing strict flag'))
            bad += [(c['file'], 'forbidden flag', f) for f in args if gate.FORBIDDEN.match(f)]
        ck.add(E, 'every compilation strict, no forbidden FP/ISA flags; adapter compiled once',
               not bad and len(ad) == 1, {'bad': bad[:5], 'compilations': len(cc), 'adapter_entries': len(ad)})
        incs = [x[2:] for x in ad[0]['command'].split() if x.startswith('-I')] if adapter_cc else []
        kim = olock['compile']['header_include_dir'].replace('<ROOT>', adapter_cc[1]) if adapter_cc else None
        ck.add(E, 'header include dir present in adapter command', kim in incs)
        if inp.oracle_src is not None and adapter_cc:
            pre = incs[:incs.index(kim)] if kim in incs else incs
            shadows = [d for d in pre if d.startswith(adapter_cc[1] + '/') and any(
                (inp.oracle_src / d[len(adapter_cc[1]) + 1:] / 'math' / h).exists()
                for h in ('matrix3x3.h', 'vector2d.h', 'vector3.h'))]
            outside = [d for d in pre if not d.startswith(adapter_cc[1] + '/')]
            absent = [d for d in pre if d.startswith(adapter_cc[1] + '/')
                      and not (inp.oracle_src / d[len(adapter_cc[1]) + 1:]).is_dir()]
            ck.add(E, 'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
                   not shadows and not outside and not absent,
                   {'shadows': shadows, 'unchecked_outside_root': outside, 'absent_from_tree': absent})
            same = all((inp.oracle_src / rel).read_bytes() == inp.bytes[k]
                       for k, rel in elock['source']['compiler_input_paths'].items())
            ck.add(E, 'compiler-input tree files equal the bound files', same)
        else:
            for n in ('no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
                      'compiler-input tree files equal the bound files'):
                ck.add(E, n, 'not-run', 'compiler-input tree (--oracle-src) not provided')
    except (KeyError, ValueError, json.JSONDecodeError) as e:
        for n in ('adapter compile command equals the reviewed command (build root normalized)',
                  'every compilation strict, no forbidden FP/ISA flags; adapter compiled once',
                  'header include dir present in adapter command',
                  'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
                  'compiler-input tree files equal the bound files'):
            ck.add(E, n, False, repr(e))
    try:
        adapter = inp.bytes['adapter'].decode()
        spans = {k: sha(gate.span(adapter, v).encode()) for k, v in gate.ADAPTER_SPANS.items()}
        diff = sorted(k for k, v in spans.items() if elock['adapter_spans'].get(k) != v)
    except (KeyError, gate.GateError) as e:
        diff = [str(e)]
    ck.add(E, 'adapter definitions equal the reviewed definitions', not diff, diff)
    # toolchain closure
    tc = olock['toolchain']
    ident = json.loads(inp.identity) if inp.identity else {}
    ck.add(E, 'toolchain identity record is the reviewed closure (sha256)', bool(inp.identity) and sha(inp.identity) == tc['identity_sha256'],
           sha(inp.identity) if inp.identity else None)
    ck.add(E, 'toolchain identity names the reviewed input lock and compilers',
           ident.get('lock_sha256') == tc['input_lock_sha256'] and ident.get('compiler', {}).get('g++') == tc['gxx']
           and ident.get('compiler', {}).get('glibc') == tc['glibc']
           and ident.get('compiler', {}).get('libstdcxx_runtime') == tc['libstdcxx_runtime'])
    if a.verify_closure:
        if a.verify_closure is not True:
            ok, detail = False, 'verify_closure must be a boolean, not a command'
        else:
            ok, detail = verify_installed_closure(a.toolchain_identity)
        ck.add(E, 'toolchain closure re-verified on this machine (verify_image.py)', ok, detail)
    else:
        ck.add(E, 'toolchain closure re-verified on this machine (verify_image.py)', 'not-run',
               'run the gate inside the image with --verify-closure to recompute the installed closure')
    # linked oracle identity fields (not its whole-file hash)
    elf = Elf(inp.oracle)
    idn = elf.identity()
    isa = [q['data'] for q in idn['gnu_properties'] if q['type'] == '0xc0008002']
    ck.add(E, 'linked oracle: ELF64 x86-64 executable, baseline ISA, reviewed DT_NEEDED',
           (idn['machine'], idn['type']) == (62, 3) and isa == [elock['oracle']['x86_isa_needed_data']]
           and dt_needed(elf) == olock['linked']['needed'], {'isa': isa, 'needed': dt_needed(elf)})
    ck.add(E, 'linked oracle .comment names the reviewed compiler', idn['comment'] == olock['linked']['comment'], idn['comment'])
    # link proof
    Lk = Linked(elf)
    objects = {n: (obj if n == HOME_FILE else RelObj(inp.providers[n])) for n in inp.order}
    prover = Prover(Lk, objects, HOME_FILE, olock['link']['externals_spec'])
    proof = None
    try:
        proof = prover.prove()
        ck.add(E, 'link proof: reachable units are the relocated reviewed object bytes', True,
               {k: proof[k] for k in ('relocations', 'by_type', 'merge_items', 'relaxed', 'skipped')})
    except (LinkError, ObjError, ElfError, DecodeError) as e:
        ck.add(E, 'link proof: reachable units are the relocated reviewed object bytes', False, str(e)[:400])
    lk = olock['link']
    for name, key in (('link proof: home sections equal the reviewed set', 'home_sections'),
                      ('link proof: other-object bodies equal the reviewed digests', 'provider_units'),
                      ('link proof: dynamic imports equal the reviewed set', 'imports'),
                      ('link proof: externals equal the reviewed bindings', 'externals')):
        got = proof.get(key) if proof else None
        det = None
        if proof and got != lk[key] and isinstance(got, dict):
            det = {'missing': sorted(set(lk[key]) - set(got))[:5], 'extra': sorted(set(got) - set(lk[key]))[:5],
                   'changed': sorted(k for k in set(got) & set(lk[key]) if got[k] != lk[key][k])[:5]}
        ck.add(E, name, proof is not None and got == lk[key], det)
    report['link'] = proof and {k: v for k, v in proof.items() if k != 'home_bases'}
    # FP environment over the whole linked oracle
    hits, errors, covered = fp_sweep(elf)
    ck.add(E, 'FP environment: no MXCSR/x87-control writes in any executable byte (decoder sweep)',
           not hits and not errors, {'writes': hits[:5], 'decode_errors': errors, 'bytes_decoded': covered})
    if a.phase == 'full':
        out = subprocess.run([a.objdump, '-d', '-w', '--no-show-raw-insn', '-j', '.text', str(inp.oracle_path)],
                             capture_output=True, text=True, check=True).stdout
        fpw = sorted({l.split('\t')[-1].split()[0] for l in out.splitlines()
                      if re.search(r'\t(v?ldmxcsr|fldcw|fldenv|frstor|fxrstor|xrstor\w*|fnclex|fninit)\b', l)})
        ck.add(E, 'FP environment: no MXCSR/x87-control writes in .text (objdump scan)', not fpw, fpw)
    else:
        ck.add(E, 'FP environment: no MXCSR/x87-control writes in .text (objdump scan)', 'not-run', 'static phase: no objdump')
    fp_syms = sorted({s['name'] for s in elf.symbols if gate.FP_MODE_SYMBOLS.match(s['name'])})
    ck.add(E, 'no FP-environment functions referenced', not fp_syms, fp_syms)
    ck.add(E, 'runtime FP-mode contract recorded (measurement is runtime qualification)',
           elock['runtime_fp']['mxcsr'] == '0x1f80' and elock['runtime_fp']['tunables'] == 'glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F')
    # context through the proof
    sites, ctxs, listings, tlock = {}, {}, None, None
    intervals = unit_intervals(prover) if proof else []
    try:
        if proof is None:
            raise LinkError('no link proof')
        tr = Translator(olock['reference_layout'], intervals)
        tlock = translated_lock(elock, tr, elf)
        eq_addr = tr(int(olock['eq_operator'], 16))
        ck.add(C, 'reviewed addresses translated through proven units', True)
    except (LinkError, ElfError, KeyError) as e:
        ck.add(C, 'reviewed addresses translated through proven units', False, str(e))
    if tlock is not None:
        try:
            if a.phase == 'static':
                listings = gate.load_listings(types.SimpleNamespace(oracle_path=inp.oracle_path), elf, tlock, 'static', a.objdump)
            else:
                listings = gate.load_listings(types.SimpleNamespace(oracle_path=inp.oracle_path), elf, tlock, 'full', a.objdump)
            ck.add(C, 'complete function listings', True,
                   {n[:30]: {'instructions': len(l.insns), 'decoder': l.version} for n, l in listings.items()})
        except (ListingError, ElfError, KeyError, OSError) as e:
            ck.add(C, 'complete function listings', False, str(e))
    if listings is not None:
        dlst = listings[gate.DOUBLEOPS]
        try:
            for b in tlock['entry_registers'].values():
                gate.prologue_binding(dlst, b, b['source_family'])
            ck.add(C, 'r12/r15 prologue bindings (straight-line, sources unwritten)', True)
        except gate.GateError as e:
            ck.add(C, 'r12/r15 prologue bindings (straight-line, sources unwritten)', False, str(e))
        try:
            gate.prologue_binding(dlst, tlock['op_binding'], tlock['op_binding']['source_family'])
            ck.add(C, 'aOp -> %rbx prologue binding (mov %rsi,%rbx)', True)
        except gate.GateError as e:
            ck.add(C, 'aOp -> %rbx prologue binding (mov %rsi,%rbx)', False, str(e))
        f = tlock['frame']
        ins = dlst.by_addr.get(int(f['setup_insn'], 16))
        ck.add(C, f'frame setup {f["setup_text"]!r} and reviewed frame size {f["size"]:#x}',
               ins is not None and f"{ins['mnem']} {ins['ops_text']}" == f['setup_text'] == f'sub ${f["size"]:#x},%rsp')
        tb = proof['home_bases']['.text']
        tsize = [s for s in obj.sections if s['name'] == '.text'][0]['size']
        for sid in contract.REQUIRED_SITES:
            try:
                start = int(tlock['sites'][sid]['start'], 16)
                dlst.check_boundary(start)
                rel = sorted(dlst.paths_to(start))
                writers = {r: [hex(x) for x in rel if gate.may_write(dlst.by_addr[x], r)
                               and x not in {int(tlock['entry_registers'][q]['binding_insn'], 16) for q in ('r12', 'r15')}]
                           for r in ('r12', 'r15')}
                through = [hex(x) for x in rel if gate.writes_through(dlst.by_addr[x], 'r12')]
                ctx = object_path(dlst, sid, start, obj, tb, tsize, elf, intervals, Lk)
                want = olock['context'].get(sid, {})
                ck.add(C, f'{sid}: r12 (aInput) and r15 (aScalar) unwritten on every path', not writers['r12'] and not writers['r15'], writers)
                ck.add(C, f'{sid}: no write through the request pointer %r12 on any path', not through, through[:5])
                ck.add(C, f'{sid}: path instruction count', ctx['path_instructions'] == want.get('path_instructions'), ctx['path_instructions'])
                ck.add(C, f'{sid}: path bytes equal the reviewed object path bytes',
                       ctx['path_object_bytes_sha256'] == want.get('path_object_bytes_sha256'), ctx['path_object_bytes_sha256'])
                ck.add(C, f'{sid}: path callees are the reviewed proven identities',
                       not ctx['unproven_callees'] and ctx['path_callees_sha256'] == want.get('path_callees_sha256'),
                       {'sha256': ctx['path_callees_sha256'], 'unproven': ctx['unproven_callees'][:3]})
                ctxs[sid] = ctx
            except (ListingError, gate.GateError, KeyError, TypeError, ValueError) as e:
                ck.add(C, f'{sid}: context', False, str(e))
    rv = olock['context_review']
    link_digest = sha(canon({k: proof[k] for k in ('home_sections', 'provider_units', 'imports', 'externals')})) if proof else None
    ck.add(C, OBJECT_CONTEXT_REVIEW,
           rv.get('status') == 'REVIEWED' and isinstance(rv.get('reviewer'), str) and rv['reviewer'].strip() != ''
           and rv.get('object_sha256') == obj.sha256 and rv.get('link_digest') == link_digest,
           {'status': rv.get('status'), 'reviewer': rv.get('reviewer'), 'link_digest': link_digest})
    report['contexts'] = ctxs
    # arithmetic
    try:
        exprs, src_spans, layouts = srcexpr.derive(inp.bytes['header'].decode(), inp.bytes['vector2'].decode(),
                                                   inp.bytes['vector3'].decode())
        ck.add(A, 'source definitions derived from the exact header', True)
    except (srcexpr.SourceError, KeyError) as e:
        ck.add(A, 'source definitions derived from the exact header', False, str(e))
        exprs, src_spans, layouts = None, {}, None
    ck.add(A, 'source data-member layouts', glock['source_layouts'] == layouts)
    for sid in contract.REQUIRED_SITES:
        s = elock['sites'][sid]
        errs = contract.site_contract_errors(sid, s)
        ck.add(A, f'{sid}: site fields equal the canonical contract', not errs, errs)
        if listings is None:
            sites[sid] = {'error': 'no listings'}
            continue
        r = {}
        try:
            outs, info = extract_t(elf, tlock, sid, listings, eq_addr)
            fp, doc = graph.fingerprint(outs, s['output_order'])
            r.update(info, fingerprint=fp, nodes=len(doc['nodes']), trees={n: graph.to_json(outs[n]) for n in s['output_order']})
            if exprs is not None:
                r['finite_proof'] = graph.finite_proof(outs, exprs[contract.REQUIRED_SITES[sid]])
                ck.add(A, f'{sid}: finite proof', r['finite_proof']['ok'], r['finite_proof'])
            g = glock['sites'][sid]
            ck.add(A, f'{sid}: arithmetic graph fingerprint', g['fingerprint'] == fp, fp)
            ck.add(A, f'{sid}: graph node count', g['nodes'] == len(doc['nodes']))
            ck.add(A, f'{sid}: graph source', g['source'] == contract.REQUIRED_SITES[sid])
            ck.add(A, f'{sid}: source definition hash', g['source_definition_sha256'] == src_spans.get(g['source']))
        except (gate.GateError, SymexError, ElfError, ListingError, KeyError, TypeError, ValueError) as e:
            r['error'] = f'{type(e).__name__}: {e}'
            ck.add(A, f'{sid}: extraction', False, str(e))
        sites[sid] = r
    report['sites'] = {k: {x: v for x, v in r.items() if x in ('fingerprint', 'nodes', 'error', 'finite_proof')} for k, r in sites.items()}
    req = set(contract.REQUIRED_SITES)
    sets = {'eligibility locators': set(elock['sites']), 'object context bindings': set(olock['context']),
            'report sites extracted': {k for k, v in sites.items() if 'fingerprint' in v and 'error' not in v},
            'graph locks': set(glock['sites'])}
    for k, v in sets.items():
        ck.add(S, f'{k} == reviewed required sites ({len(req)})', v == req, {'missing': sorted(req - v), 'extra': sorted(v - req)})
    import collections
    want = collections.Counter(inventory(elock, olock))
    have = collections.Counter([(r['category'], r['check']) for r in ck.rows] + [(S, OBJECT_INVENTORY)])
    ck.add(S, OBJECT_INVENTORY, want == have,
           {'missing': sorted((want - have).elements())[:8], 'extra': sorted((have - want).elements())[:8]})
    report['checks'] = ck.rows
    report['counts'] = {c: {st: sum(1 for r in ck.rows if r['category'] == c and r['status'] == st) for st in ('pass', 'fail', 'not-run')}
                        for c in ('schema', E, C, S, A)}
    report['failures'] = ck.failures()
    allpass = all(r['status'] == 'pass' for r in ck.rows)
    if report['failures']:
        report['decision'] = 'FAIL'
    elif a.phase != 'full':
        report['decision'] = 'STATIC-PARTIAL (object mode; never a gate PASS)'
    else:
        report['decision'] = 'PASS (object mode, experimental, UNQUALIFIED)' if allpass else 'FAIL'
    # emission: only from a PASS of this same run
    report['emission'] = None
    if a.emit:
        if not report['decision'].startswith('PASS'):
            report['emission'] = f'REFUSED: decision is {report["decision"]!r}'
        else:
            text = '\n'.join(emit.HEADER) + '\n'
            for sid in contract.EMIT_ORDER:
                rust = contract.canonical_rust(contract.REQUIRED_SITES[sid])
                if elock['sites'][sid].get('rust') != rust or sites[sid].get('fingerprint') != glock['sites'][sid]['fingerprint']:
                    raise SystemExit(f'{sid}: emission contract mismatch')
                text += emit.site_text(sid, elock['sites'][sid]['label'], sites[sid]['trees'], rust)
            accept_emission(report, text, glock['generated_rs_sha256'],
                            olock['emission']['generated_rs_sha256'], a.emit)
    return report


# -- derive (measured once on the reviewed build) ---------------------------------------------
def derive(a):
    elock = json.loads((pathlib.Path(a.locks) / 'eligibility.lock.json').read_bytes())
    glock = json.loads((pathlib.Path(a.locks) / 'graphs.lock.json').read_bytes())
    inp = Inputs(a)
    elf = Elf(inp.oracle)
    if elf.sha256 != elock['oracle']['sha256']:
        raise SystemExit('derive needs the reviewed oracle the reviewed lock binds')
    obj = RelObj(inp.object)
    Lk = Linked(elf)
    objects = {n: (obj if n == HOME_FILE else RelObj(inp.providers[n])) for n in inp.order}
    ext = json.loads(pathlib.Path(a.externals).read_text())
    prover = Prover(Lk, objects, HOME_FILE, ext)
    proof = prover.prove()
    intervals = unit_intervals(prover)
    ref = [{'start': s, 'end': e, 'key': list(k)} for s, e, k in intervals]
    cc = json.loads(inp.bytes['compile_commands'])
    ad = [c for c in cc if c['file'] == elock['adapter_compile']['file'] and c['output'] == elock['adapter_compile']['output']]
    root = build_root(ad[0]['file'])
    ident = json.loads(inp.identity)
    tr = Translator(ref, intervals)
    assert tr(0x48010) == 0x48010  # identity on the reviewed layout
    listings = gate.load_listings(types.SimpleNamespace(oracle_path=inp.oracle_path), elf, elock, a.phase, a.objdump)
    dlst = listings[gate.DOUBLEOPS]
    tb = proof['home_bases']['.text']
    tsize = [s for s in obj.sections if s['name'] == '.text'][0]['size']
    ctx = {}
    for sid in contract.REQUIRED_SITES:
        c = object_path(dlst, sid, int(elock['sites'][sid]['start'], 16), obj, tb, tsize, elf, intervals, Lk)
        if c['unproven_callees']:
            raise SystemExit(f'{sid}: unproven callees {c["unproven_callees"]}')
        ctx[sid] = {k: c[k] for k in ('path_instructions', 'path_object_bytes_sha256', 'path_callees_sha256')}
    link = {k: proof[k] for k in ('home_sections', 'provider_units', 'imports', 'externals')}
    lock = {
        'note': ('EXPERIMENTAL object-bound eligibility lock, DERIVED by objgate.py derive from the reviewed build '
                 '(oracle 0bac7567..., object below); it is not reviewed and not a qualification'),
        'mode': MODE,
        'status': 'DERIVED-PENDING-REVIEW',
        'home_object': {'file': HOME_FILE, 'sha256': obj.sha256, 'comment': obj.comment()},
        'compile': {'command': normalize_command(ad[0]['command'], root), 'compiler_path': elock['compiler']['path'],
                    'header_include_dir': elock['adapter_compile']['header_include_dir'].replace(root, '<ROOT>')},
        'toolchain': {'identity_sha256': sha(inp.identity), 'input_lock_sha256': ident['lock_sha256'],
                      'gxx': ident['compiler']['g++'], 'glibc': ident['compiler']['glibc'],
                      'libstdcxx_runtime': ident['compiler']['libstdcxx_runtime']},
        'linked': {'needed': dt_needed(elf), 'comment': elf.identity()['comment']},
        'link': dict(link, externals_spec=ext),
        'reference_layout': ref,
        'context': ctx,
        'context_review': {'status': 'PENDING', 'reviewer': None, 'object_sha256': obj.sha256,
                           'link_digest': sha(canon(link)),
                           'basis': ('Re-derivation of the reviewed binary context (lock contexts and assumptions '
                                     'A1-A5) against the measured object; awaiting an independent reviewer. No '
                                     'reviewer identity is recorded until one signs.')},
        'eq_operator': '0x1f990',
        'emission': {'generated_rs_sha256': glock['generated_rs_sha256'], 'required_sites': sorted(contract.REQUIRED_SITES)},
    }
    lock['link'].pop('externals_spec')
    lock['link']['externals_spec'] = ext
    pathlib.Path(a.out).write_text(json.dumps(lock, indent=1, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({'object': obj.sha256, 'home_sections': len(link['home_sections']),
                      'provider_units': len(link['provider_units']), 'imports': len(link['imports']),
                      'link_digest': lock['context_review']['link_digest']}, indent=1))


def parser():
    p = argparse.ArgumentParser()
    p.add_argument('action', choices=['derive', 'verify'])
    p.add_argument('--oracle', required=True)
    p.add_argument('--object', required=True)
    p.add_argument('--providers', required=True)
    p.add_argument('--compiler-inputs')
    p.add_argument('--toolchain-identity')
    p.add_argument('--locks', required=True)
    p.add_argument('--out', required=True)
    p.add_argument('--externals')
    p.add_argument('--phase', choices=['full', 'static'], default='full')
    p.add_argument('--oracle-src')
    p.add_argument('--objdump', default='objdump')
    p.add_argument('--verify-closure', action='store_true',
                   help='run the fixed repository verifier against the installed image inputs')
    p.add_argument('--emit')
    return p


def main():
    a = parser().parse_args()
    if a.action == 'derive':
        derive(a)
        return
    rep = run(a)
    pathlib.Path(a.out).write_text(json.dumps(rep, indent=1, default=str), encoding='utf-8')
    print(rep['decision'])
    for f in rep['failures'][:30]:
        print('  -', f)
    nr = [r['check'] for r in rep['checks'] if r['status'] == 'not-run']
    for n in nr:
        print('  not-run:', n)
    sys.exit(0 if rep['decision'].startswith('PASS') else (3 if rep['decision'].startswith('STATIC') else 1))


if __name__ == '__main__':
    main()
