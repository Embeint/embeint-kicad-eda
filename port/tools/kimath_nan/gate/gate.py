"""PR50 NaN-site generator gate, context revision (PRIVATE EXPERIMENTAL; never a qualification).

usage:
  gate.py verify  --inputs DIR --locks DIR --out REPORT --phase {full,static} [--bypass-eligibility] [overrides]
  gate.py propose --inputs DIR --locks DIR --out PROPOSAL --phase {full,static}   (never writes a lock)

Separate decisions, evaluated in order (a later stage never repairs an earlier failure):
  1. lock schema: every field of both locks is known; safety fields are used and checked.
  2. eligibility: exact binary/toolchain/flags/source/adapter/callee identities (static phase:
     tool- or WSL-dependent checks are reported NOT-RUN, never as passing).
  3. context (finding 2): narrow reviewed binding of each window's entry context: the exact
     bytes and direct callees of every instruction on any CFG path from the function entry to
     the window, the op-string / request / KIND register bindings, the frame setup, no write
     through the request pointer, and the cited manual context review. Any change FAILS here,
     before arithmetic, even when every arithmetic fingerprint is unchanged.
  4. scope (finding 3): reviewed REQUIRED_SITES == eligibility locators == contexts == graph
     locks == successfully extracted report sites (nonempty).
  5. arithmetic: hardened extraction (frame bounds and CALL stack effects, finding 1), derived
     source expressions, canonical site contract (finding 4), finite proof, fingerprint lock.
  Emission is a separate tool (emit.py) that requires this report.
Static phase (resource pause): Listing is substituted by the SHA-bound retained objdump text
(RetainedListing); the decision is at best STATIC-PARTIAL, never PASS.
"""
import argparse
import collections
import hashlib
import json
import pathlib
import re
import struct
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from elfbin import Elf, ElfError, git_blob_id  # noqa: E402
from listing import Listing, ListingError, RetainedListing, may_write, writes_through  # noqa: E402
from symex import Machine, SymexError, CONTRACTS, contract_sha256  # noqa: E402
from x86strict import decode_one, decode_range, DecodeError  # noqa: E402
import contract  # noqa: E402
import graph  # noqa: E402
import handoff  # noqa: E402
import lockschema  # noqa: E402
import srcexpr  # noqa: E402

DOUBLEOPS = '_ZN6oracleL9DoubleOpsERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEERKN8nlohmann16json_abi_v3_11_310basic_jsonISt3mapSt6vectorS5_blmdSaNS9_14adl_serializerESC_IhSaIhEEvEENS_4KINDE'
STRICT = ['-O3', '-fno-fast-math', '-ffp-contract=off']
FORBIDDEN = re.compile(r'^(-Ofast|-ffast-math|-funsafe-math-optimizations|-ffinite-math-only|-fno-signed-zeros|'
                       r'-freciprocal-math|-fassociative-math|-ffp-contract=(fast|on)|-fcx-limited-range|'
                       r'-fno-trapping-math|-fno-rounding-math|-frounding-math|-m.*|-march=.*|-mtune=.*|-mfpmath=.*|-O[012s]|-Og)$')
FP_MODE_SYMBOLS = re.compile(r'^(fesetround|fesetenv|feupdateenv|feenableexcept|fedisableexcept|fesetexceptflag|'
                             r'feholdexcept|__fesetround|_FPU_SETCW|__fpu_control)(@.*)?$')
ADAPTER_SPANS = {
    'KIND': 'enum class KIND',
    'Double': 'static double Double( const JSON& aValue, const std::string& aName )',
    'Bits': 'static JSON Bits( double aValue )',
    'Elements': 'static std::array<T, N> Elements( const JSON& aValue, const std::string& aName,',
    'Element<double>': 'double Element<double>( const JSON& aValue, const std::string& aName )',
    'ElementJSON(double)': 'static JSON ElementJSON( double aValue )',
    'Vector2': 'static VECTOR2<T> Vector2( const JSON& aValue, const std::string& aName )',
    'Vector3': 'static VECTOR3<T> Vector3( const JSON& aValue, const std::string& aName )',
    'Matrix': 'static MATRIX3x3<T> Matrix( const JSON& aValue, const std::string& aName )',
    'MatrixReply': 'static JSON MatrixReply( const MATRIX3x3<T>& aMatrix )',
    'Vector2JSON': 'static JSON Vector2JSON( const VECTOR2<T>& aVector )',
    'Vector3JSON': 'static JSON Vector3JSON( const VECTOR3<T>& aVector )',
    'DoubleOps': 'static JSON DoubleOps( const std::string& aOp, const JSON& aInput, KIND aScalar )',
    'KimathMath': 'static JSON KimathMath( const JSON& aInput )',
}
class GateError(Exception):
    pass


def sha(b):
    return hashlib.sha256(b).hexdigest()


def span(text, start_line):
    i = text.find('\n' + start_line)
    if i < 0 or text.find('\n' + start_line, i + 1) >= 0:
        raise GateError(f'adapter definition {start_line!r} not unique')
    s = text.rfind('\ntemplate', 0, i)
    s = s + 1 if s >= 0 and text[s + 1:i].count('\n') == 0 else i + 1
    b = text.index('\n{', i) + 1
    depth = 0
    for k in range(b, len(text)):
        depth += text[k] == '{'
        depth -= text[k] == '}'
        if depth == 0:
            return text[s:k + 1]
    raise GateError('unbalanced adapter braces')


class Checks:
    def __init__(self):
        self.rows = []

    def add(self, category, name, status, detail=''):
        if status is True:
            status = 'pass'
        elif status is False:
            status = 'fail'
        self.rows.append({'category': category, 'check': name, 'status': status, 'detail': detail})

    def status(self, category):
        st = [r['status'] for r in self.rows if r['category'] == category]
        if not st:
            return 'none'
        if 'fail' in st:
            return 'fail'
        return 'partial' if 'not-run' in st else 'pass'

    def failures(self, category=None):
        return [f"{r['category']}: {r['check']}" for r in self.rows
                if r['status'] == 'fail' and (category is None or r['category'] == category)]


class Inputs:
    def __init__(self, a):
        d = pathlib.Path(a.inputs)
        p = lambda over, name: pathlib.Path(over) if over else d / name
        self.oracle_path = p(a.oracle, 'kicad-oracle')
        self.files = {
            'header': p(a.header, 'compiler-input-matrix3x3.h'),
            'vector2': p(a.vector2, 'compiler-input-vector2d.h'),
            'vector3': p(a.vector3, 'compiler-input-vector3.h'),
            'adapter': p(a.adapter, 'compiler-input-kimath_math.cpp'),
            'compile_commands': p(a.compile_commands, 'compile_commands.json'),
            'provenance': p(a.provenance, 'oracle-provenance.json'),
            'source_manifest': p(a.source_manifest, 'source.json'),
        }
        self.bytes = {k: v.read_bytes() for k, v in self.files.items()}
        self.oracle_in_memory = getattr(a, 'oracle_bytes', None) is not None
        self.oracle = a.oracle_bytes if self.oracle_in_memory else self.oracle_path.read_bytes()
        self.oracle_src = pathlib.Path(a.oracle_src) if a.oracle_src else None


# ---------------------------------------------------------------------------------------------
def schema(ck, elock, glock):
    """R4: recursive typed schema of both locks, one deterministic check per lock section."""
    S = 'schema'

    def top(name, lock, spec):
        keys, opt = spec[1], spec[2]
        if not isinstance(lock, dict):
            ck.add(S, f'{name} lock: top-level fields', False, 'not an object')
            for k in keys:
                ck.add(S, f'{name} lock: {k}', False, 'lock is not an object')
            return
        extra, missing = set(lock) - set(keys) - set(opt), set(keys) - set(lock)
        opt_errs = []
        for k, sub in opt.items():
            if k in lock:
                lockschema.validate(sub, lock[k], f'{name}.{k}', opt_errs)
        ck.add(S, f'{name} lock: top-level fields', not extra and not missing and not opt_errs,
               {'unknown': sorted(extra), 'missing': sorted(missing), 'optional_errors': opt_errs[:8]})
        for k, sub in keys.items():
            errs = []
            if k in lock:
                lockschema.validate(sub, lock[k], f'{name}.{k}', errs)
            else:
                errs = [f'{name}.{k}: missing']
            ck.add(S, f'{name} lock: {k}', not errs, errs[:8])
    top('eligibility', elock, lockschema.ELIGIBILITY)
    try:
        errs = lockschema.semantic(elock, CONTRACTS, contract_sha256) if isinstance(elock, dict) else ['not an object']
    except (TypeError, AttributeError, KeyError, ValueError) as e:
        errs = [f'semantic validation error {e!r}']
    ck.add(S, 'eligibility lock: canonical semantic fields', not errs, errs[:8])
    if glock is not None:
        top('graphs', glock, lockschema.GRAPHS)


def check_inventory(elock, glock, phase, *, with_categories=False):
    """Check names, or category/name pairs, produced by a fully passing verify run."""
    inventory = []
    names = ['eligibility lock: top-level fields', 'eligibility lock: canonical semantic fields',
             'graphs lock: top-level fields']
    names += [f'eligibility lock: {k}' for k in lockschema.ELIGIBILITY[1]]
    names += [f'graphs lock: {k}' for k in lockschema.GRAPHS[1]]
    inventory.extend(("schema", name) for name in names)
    names = []
    L = elock
    names += ['lock status is experimental, not qualified', 'oracle sha256', 'oracle build id', 'ELF64 LSB x86-64 executable',
              'x86 ISA needed: baseline only', 'ABI tag', '.comment names the reviewed compiler', 'compile_commands sha256',
              f'all {L["compile_count"]} compilations strict, no forbidden FP/ISA flags',
              'adapter compiled once with reviewed command', 'header include dir present in adapter command',
              'retained compiler-input tree provided (--oracle-src)',
              'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
              'retained compiler-input files equal the bound files',
              'no MXCSR/x87-control writes anywhere in .text (objdump scan)', 'candidate image identity (docker image inspect)',
              'oracle provenance sha256', 'compiler identity and target', 'provenance FP mode: contract off, no fast-math',
              'provenance pins', 'provenance libc', 'source manifest sha256', 'source manifest revision/tree/pin/clean',
              'manifest adapter hash equals compiler input', 'provenance overlay adapter hash equals compiler input',
              'adapter definitions equal the reviewed definitions', 'no FP-environment functions referenced',
              'runtime FP-mode contract recorded (measurement is runtime qualification)']
    names += [f'{k} is Git blob {b["blob"][:12]} ({b["git_path"]})' for k, b in L['source']['blobs'].items()]
    names += [f'reviewed callee {a} {b["effect"]} bytes' for a, b in L['callees'].items()]
    inventory.extend(("eligibility", name) for name in names)
    names = []
    f = L['frame']
    names += ['r12/r15 prologue bindings (straight-line, sources unwritten)', 'aOp -> %rbx prologue binding (mov %rsi,%rbx)',
              f'frame setup {f["setup_text"]!r} and reviewed frame size {f["size"]:#x}', CONTEXT_IDENTITY, CONTEXT_CITATIONS,
              CONTEXT_RECORD]
    for sid in contract.REQUIRED_SITES:
        names += [f'{sid}: r12 (aInput) and r15 (aScalar) unwritten on every path',
                  f'{sid}: no write through the request pointer %r12 on any path', f'{sid}: path instruction count',
                  f'{sid}: exact bytes of every pre-window path instruction',
                  f'{sid}: exact direct callees on pre-window paths (symbol and bytes)']
    inventory.extend(("context", name) for name in names)
    names = []
    names += [f'{k} == reviewed required sites ({len(contract.REQUIRED_SITES)})'
              for k in ('eligibility locators', 'context bindings', 'report sites extracted', 'graph locks')]
    inventory.extend(("scope", name) for name in names)
    names = []
    inventory.append(('scope', INVENTORY_CHECK))
    names += ['source definitions derived from the exact header', 'source data-member layouts']
    for sid in contract.REQUIRED_SITES:
        names += [f'{sid}: site fields equal the canonical contract', f'{sid}: finite proof',
                  f'{sid}: arithmetic graph fingerprint', f'{sid}: graph node count', f'{sid}: graph source',
                  f'{sid}: source definition hash']
    inventory.extend(('arithmetic', name) for name in names)
    return inventory if with_categories else [name for _category, name in inventory]


CONTEXT_IDENTITY = ('actual binary identity equals the context-reviewed identity '
                    '(a new identity needs a renewed manual context review)')
CONTEXT_CITATIONS = 'context review citations (>= 2) bound to the cited review bytes'
CONTEXT_RECORD = 'context review record non-vacuous (A1-A5 assumptions, renewal, reviewer)'
INVENTORY_CHECK = 'gate check inventory equals the full-phase inventory for these locks'


def eligibility(ck, inp, elf, L, phase, objdump_path):
    E = 'eligibility'
    ident = elf.identity()
    ck.add(E, 'lock status is experimental, not qualified', L['status'] == 'EXPERIMENTAL-ELIGIBLE-UNQUALIFIED', L['status'])
    ck.add(E, 'oracle sha256', ident['sha256'] == L['oracle']['sha256'], ident['sha256'])
    ck.add(E, 'oracle build id', ident['build_id'] == [L['oracle']['build_id']], ident['build_id'])
    ck.add(E, 'ELF64 LSB x86-64 executable', (ident['machine'], ident['type']) == (62, 3))
    isa = [q for q in ident['gnu_properties'] if q['type'] == '0xc0008002']
    ck.add(E, 'x86 ISA needed: baseline only', [q['data'] for q in isa] == [L['oracle']['x86_isa_needed_data']], isa)
    ck.add(E, 'ABI tag', ident['abi_tag'] == [L['oracle']['abi_tag']], ident['abi_tag'])
    ck.add(E, '.comment names the reviewed compiler', ident['comment'] == L['oracle']['comment'], ident['comment'])
    cc = json.loads(inp.bytes['compile_commands'])
    ck.add(E, 'compile_commands sha256', sha(inp.bytes['compile_commands']) == L['compile_commands_sha256'])
    bad = []
    for c in cc:
        args = c['command'].split()
        if args[0] != L['compiler']['path']:
            bad.append((c['file'], 'compiler', args[0]))
        if not all(f in args for f in STRICT):
            bad.append((c['file'], 'missing strict flag'))
        bad += [(c['file'], 'forbidden flag', f) for f in args if FORBIDDEN.match(f)]
    ck.add(E, f'all {len(cc)} compilations strict, no forbidden FP/ISA flags', not bad and len(cc) == L['compile_count'], bad[:5])
    ad = [c for c in cc if c['file'] == L['adapter_compile']['file'] and c['output'] == L['adapter_compile']['output']]
    ck.add(E, 'adapter compiled once with reviewed command', len(ad) == 1 and ad[0]['command'] == L['adapter_compile']['command'])
    incs = [x[2:] for x in ad[0]['command'].split() if x.startswith('-I')] if ad else []
    ck.add(E, 'header include dir present in adapter command', L['adapter_compile']['header_include_dir'] in incs)
    if phase == 'static':
        for name in ('retained compiler-input tree provided (--oracle-src)',
                     'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
                     'retained compiler-input files equal the bound files',
                     'no MXCSR/x87-control writes anywhere in .text (objdump scan)',
                     'candidate image identity (docker image inspect)'):
            ck.add(E, name, 'not-run', 'resource pause: WSL/objdump/engine unavailable')
    else:
        ck.add(E, 'retained compiler-input tree provided (--oracle-src)', inp.oracle_src is not None and inp.oracle_src.is_dir())
        if inp.oracle_src and ad:
            kim = L['adapter_compile']['header_include_dir']
            shadows = [d for d in (incs[:incs.index(kim)] if kim in incs else incs)
                       if any((inp.oracle_src / d.replace('/work/kicad-port/', '', 1) / 'math' / h).exists()
                              for h in ('matrix3x3.h', 'vector2d.h', 'vector3.h'))]
            ck.add(E, 'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h', not shadows, shadows)
            same = all((inp.oracle_src / rel).read_bytes() == inp.bytes[k] for k, rel in L['source']['compiler_input_paths'].items())
            ck.add(E, 'retained compiler-input files equal the bound files', same)
        out = subprocess.run([objdump_path, '-d', '-w', '--no-show-raw-insn', '-j', '.text', str(inp.oracle_path)],
                             capture_output=True, text=True, check=True).stdout
        fpw = sorted({l.split('\t')[-1].split()[0] for l in out.splitlines()
                      if re.search(r'\t(v?ldmxcsr|fldcw|fldenv|frstor|fxrstor|xrstor\w*|fnclex|fninit)\b', l)})
        ck.add(E, 'no MXCSR/x87-control writes anywhere in .text (objdump scan)', not fpw, fpw)
        r = subprocess.run(['docker', 'image', 'inspect', '--format', '{{.Id}}', L['image']['id']], capture_output=True, text=True)
        ck.add(E, 'candidate image identity (docker image inspect)', r.stdout.strip() == L['image']['id'])
    prov = json.loads(inp.bytes['provenance'])
    ck.add(E, 'oracle provenance sha256', sha(inp.bytes['provenance']) == L['provenance_sha256'])
    ck.add(E, 'compiler identity and target', prov['compilers'].get(L['compiler']['path'], '').startswith(L['compiler']['version_line'])
           and prov['compiler_targets'].get(L['compiler']['path']) == L['compiler']['target'])
    ck.add(E, 'provenance FP mode: contract off, no fast-math', prov['floating_point'] == {'contract': 'off', 'fast_math': False})
    ck.add(E, 'provenance pins', (prov['kicad_pin'], prov['overlay_revision']) == (L['source']['pin'], L['source']['revision']))
    ck.add(E, 'provenance libc', prov['system_libc_version'] == L['compiler']['libc'])
    for key in ('header', 'vector2', 'vector3', 'adapter'):
        b = L['source']['blobs'][key]
        ck.add(E, f'{key} is Git blob {b["blob"][:12]} ({b["git_path"]})', git_blob_id(inp.bytes[key]) == b['blob'])
    man = json.loads(inp.bytes['source_manifest'])
    ck.add(E, 'source manifest sha256', sha(inp.bytes['source_manifest']) == L['source']['manifest_sha256'])
    ck.add(E, 'source manifest revision/tree/pin/clean',
           (man['revision'], man['tree'], man['pin'], man['source_clean']) == (L['source']['revision'], L['source']['tree'], L['source']['pin'], True))
    ck.add(E, 'manifest adapter hash equals compiler input', man['files'].get('port/oracle/shims/kimath_math.cpp', {}).get('sha256') == sha(inp.bytes['adapter']))
    ck.add(E, 'provenance overlay adapter hash equals compiler input', prov['overlay_files_sha256'].get('shims/kimath_math.cpp') == sha(inp.bytes['adapter']))
    adapter = inp.bytes['adapter'].decode()
    try:
        spans = {k: sha(span(adapter, v).encode()) for k, v in ADAPTER_SPANS.items()}
        diff = sorted(k for k, v in spans.items() if L['adapter_spans'].get(k) != v)
    except GateError as e:
        diff = [str(e)]
    ck.add(E, 'adapter definitions equal the reviewed definitions', not diff and set(L['adapter_spans']) == set(ADAPTER_SPANS), diff)
    fp_syms = sorted({s['name'] for s in elf.symbols if FP_MODE_SYMBOLS.match(s['name'])})
    ck.add(E, 'no FP-environment functions referenced', not fp_syms, fp_syms)
    ck.add(E, 'runtime FP-mode contract recorded (measurement is runtime qualification)',
           L['runtime_fp']['mxcsr'] == '0x1f80' and L['runtime_fp']['tunables'] == 'glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F')
    for addr, b in L['callees'].items():
        try:
            fn = elf.function(b['symbol'], int(addr, 16))
            ok, det = fn['sha256'] == b['sha256'], fn['sha256']
        except ElfError as e:
            ok, det = False, str(e)
        ck.add(E, f'reviewed callee {addr} {b["effect"]} bytes', ok, det)
    return ident


# ---------------------------------------------------------------------------------------------
def load_listings(inp, elf, L, phase, objdump):
    out = {}
    for name in L['analysed_functions']:
        fn = elf.function(name)
        if phase == 'static':
            sub = L['static_substitutes']['listings'][name]
            text = pathlib.Path(sub['path']).read_bytes()
            if sha(text) != sub['sha256']:
                raise ListingError(f'retained listing {sub["path"]} differs from its bound SHA-256')
            out[name] = RetainedListing(text.decode(), elf, fn, 'RETAINED ' + sub['decoder'])
        else:
            out[name] = Listing(str(inp.oracle_path), elf, fn, objdump)
    return out


def prologue_binding(lst, b, source_family):
    bind = int(b['binding_insn'], 16)
    ins = lst.by_addr.get(bind)
    if ins is None or f"{ins['mnem']} {ins['ops_text']}" != b['binding_text']:
        raise GateError(f'binding instruction changed at {bind:#x} (reviewed {b["binding_text"]!r})')
    a = lst.fn['value']
    while a < bind:
        i = lst.by_addr[a]
        if not re.fullmatch(r'push %r\w+|mov %r\w+,%r\w+|mov %e\w+,%r\w+', f"{i['mnem']} {i['ops_text']}") or \
                may_write(i, source_family):
            raise GateError(f'prologue instruction before {b["binding_text"]!r} is not reviewed: {i["line"]}')
        a += i['len']
    targets = {t for i in lst.insns for t in (lst.successors(i) or []) if t != i['addr'] + i['len']}
    if any(lst.fn['value'] < t <= bind for t in targets):
        raise GateError(f'a jump enters the prologue before {b["binding_text"]!r}')
    return bind


def context_global(ck, lst, L, elf):
    C = 'context'
    try:
        for reg, b in L['entry_registers'].items():
            prologue_binding(lst, b, b['source_family'])
        ck.add(C, 'r12/r15 prologue bindings (straight-line, sources unwritten)', True)
    except GateError as e:
        ck.add(C, 'r12/r15 prologue bindings (straight-line, sources unwritten)', False, str(e))
    try:
        prologue_binding(lst, L['op_binding'], L['op_binding']['source_family'])
        ck.add(C, 'aOp -> %rbx prologue binding (mov %rsi,%rbx)', True)
    except GateError as e:
        ck.add(C, 'aOp -> %rbx prologue binding (mov %rsi,%rbx)', False, str(e))
    f = L['frame']
    ins = lst.by_addr.get(int(f['setup_insn'], 16))
    ck.add(C, f'frame setup {f["setup_text"]!r} and reviewed frame size {f["size"]:#x}',
           ins is not None and f"{ins['mnem']} {ins['ops_text']}" == f['setup_text'] and f['setup_text'] == f'sub ${f["size"]:#x},%rsp',
           ins and ins['line'])
    r = L['context_review']
    ident = elf.identity()
    # R2: compared with the ACTUAL binary inside the context stage, independently of eligibility.
    ck.add(C, CONTEXT_IDENTITY, ident['sha256'] == r['binary_sha256'] and ident['build_id'] == [r['binary_build_id']],
           {'actual_sha256': ident['sha256'], 'actual_build_id': ident['build_id']})
    cites = r['citations'] if isinstance(r.get('citations'), list) else []
    ok = len(cites) >= 2
    for ref in cites:
        p = pathlib.Path(ref['path'])
        ok = ok and p.is_file() and sha(p.read_bytes()) == ref['sha256']
    ck.add(C, CONTEXT_CITATIONS, ok, len(cites))
    asm = r.get('assumptions')
    ok = isinstance(asm, list) and [x.split(' ', 1)[0] for x in asm if isinstance(x, str)] == lockschema.ASSUMPTION_IDS and \
        all(isinstance(x, str) and len(x.split(' ', 1)) == 2 and x.split(' ', 1)[1].strip() for x in asm) and \
        isinstance(r.get('renewal'), str) and r['renewal'].strip() and isinstance(r.get('reviewer'), str) and r['reviewer'].strip()
    ck.add(C, CONTEXT_RECORD, bool(ok))


def context_site(ck, elf, lst, sid, start, L):
    C = 'context'
    ctx = L.get('contexts', {}).get(sid)
    if ctx is None:
        ck.add(C, f'{sid}: reviewed context binding present', False)
        ctx = {'path_instructions': None, 'path_bytes_sha256': None, 'path_call_count': None, 'path_calls_sha256': None}
    try:
        rel = sorted(lst.paths_to(start))
    except ListingError as e:
        ck.add(C, f'{sid}: CFG paths to the window', False, str(e))
        return
    blob = b''.join(struct.pack('<IB', a - lst.fn['value'], lst.by_addr[a]['len']) + lst.by_addr[a]['raw'] for a in rel)
    calls = {}
    for a in rel:
        i = lst.by_addr[a]
        if i['mnem'] == 'call':
            t = re.fullmatch(r'([0-9a-f]+)', i['ops_text'])
            if not t:
                calls[hex(a)] = {'symbol': 'INDIRECT', 'sha256': None}
                continue
            target = int(t.group(1), 16)
            syms = elf.functions_at(target)
            if syms:
                fn = elf.function(syms[0], target)
                calls[hex(target)] = {'symbol': syms[0], 'sha256': fn['sha256']}
            else:
                try:
                    imp = elf.plt_import(target)
                    calls[hex(target)] = {'symbol': 'PLT:' + imp['import'], 'sha256': imp['stub_sha256']}
                except ElfError as e:
                    calls[hex(target)] = {'symbol': 'UNRESOLVED', 'sha256': None}
                    ck.add(C, f'{sid}: call target {target:#x} resolved', False, str(e))
    writers = {'r12': [hex(a) for a in rel if may_write(lst.by_addr[a], 'r12')],
               'r15': [hex(a) for a in rel if may_write(lst.by_addr[a], 'r15')]}
    bound = {int(L['entry_registers'][r]['binding_insn'], 16) for r in ('r12', 'r15')}
    writers = {r: [w for w in v if int(w, 16) not in bound] for r, v in writers.items()}
    through = [hex(a) for a in rel if writes_through(lst.by_addr[a], 'r12')]
    ck.add(C, f'{sid}: r12 (aInput) and r15 (aScalar) unwritten on every path', not writers['r12'] and not writers['r15'], writers)
    ck.add(C, f'{sid}: no write through the request pointer %r12 on any path', not through, through[:5])
    ck.add(C, f'{sid}: path instruction count', len(rel) == ctx['path_instructions'], len(rel))
    ck.add(C, f'{sid}: exact bytes of every pre-window path instruction', sha(blob) == ctx['path_bytes_sha256'], sha(blob))
    calls_sha = sha(json.dumps(calls, sort_keys=True, separators=(',', ':')).encode())
    ck.add(C, f'{sid}: exact direct callees on pre-window paths (symbol and bytes)',
           len(calls) == ctx['path_call_count'] and calls_sha == ctx['path_calls_sha256'], calls_sha)
    return {'path_instructions': len(rel), 'path_bytes_sha256': sha(blob), 'path_call_count': len(calls),
            'path_calls_sha256': calls_sha, 'path_calls': calls}


def dispatch_check(lst, elf, start, d):
    preds = [i for i in lst.insns if (lst.successors(i) or []) and start in (lst.successors(i) or [])
             and i['addr'] + i['len'] != start]
    if d['kind'] == 'jne':
        if len(preds) != 1 or preds[0]['mnem'] != 'jne':
            raise GateError(f'{start:#x}: expected exactly one jne predecessor')
        j = preds[0]
    else:
        prev = [i for i in lst.insns if i['addr'] + i['len'] == start]
        if preds or len(prev) != 1 or prev[0]['mnem'] != 'je':
            raise GateError(f'{start:#x}: expected fall-through after je and no jumps into it')
        j = prev[0]
    k = lst.insns.index(j)
    seq = lst.insns[k - 4:k + 1]
    shape = [f"{i['mnem']} {i['ops_text']}" for i in seq]
    if not (re.fullmatch(r'lea 0x[0-9a-f]+\(%rip\),%rsi', shape[0]) and shape[1] == 'mov %rbx,%rdi'
            and shape[2] == 'call 1f990' and shape[3] == 'test %al,%al'):
        raise GateError(f'dispatch sequence for {start:#x} differs: {shape}')
    lea = decode_one(lst.fn['bytes'], seq[0]['addr'] - lst.fn['value'], seq[0]['addr'])
    literal = elf.cstring(lea.ops[0].rip_target)
    if literal != d['literal']:
        raise GateError(f'dispatch literal {literal!r} differs from {d["literal"]!r}')
    return literal


def cross_check(trace, listings):
    n = 0
    for ins in trace:
        lst = next((l for l in listings if l.fn['value'] <= ins.addr < l.fn['value'] + l.fn['size']), None)
        if lst is None:
            raise GateError(f'traced {ins.addr:#x} outside the listed functions')
        o = lst.by_addr.get(ins.addr)
        if o is None or o['len'] != ins.length or o['raw'] != ins.raw:
            raise GateError(f'decoder/listing boundary mismatch at {ins.addr:#x}')
        if f"{o['mnem']} {o['ops_text']}".strip() != ins.fmt():
            raise GateError(f'decoder/listing text mismatch at {ins.addr:#x}: {ins.fmt()!r} vs {o["line"]!r}')
        n += 1
    return n


def extract(elf, L, sid, listings):
    s = L['sites'][sid]
    fn = elf.function(s['function'])
    dlst = listings[s['function']]
    start, stop = int(s['start'], 16), int(s['stop'], 16)
    dlst.check_boundary(start)
    dlst.check_boundary(stop)
    if not (fn['value'] <= start < stop < fn['value'] + fn['size']):
        raise GateError('window outside its function')
    dispatch_check(dlst, elf, start, s['dispatch'])
    bindings = {int(a, 16): b for a, b in L['callees'].items()}
    branches = {int(a, 16): dict(b, target=int(b['target'], 16)) for a, b in s['branches'].items()}
    mach = Machine(elf, bindings, s['roles'], branches, frame_size=L['frame']['size'])
    mach.gpr[12] = ('json_input',)
    mach.gpr[15] = ('kind_arg',)
    outs = mach.run(fn, start, stop, s['consumer'])
    traced = cross_check(mach.trace, list(listings.values()))
    if list(outs) != s['output_order']:
        raise GateError(f'output names {list(outs)} differ from {s["output_order"]}')
    return outs, {'path': mach.path, 'calls': mach.calls, 'instructions': traced}


# ---------------------------------------------------------------------------------------------
def run(a):
    phase = a.phase
    ck = Checks()
    report = {'note': 'PRIVATE EXPERIMENTAL prototype gate (context revision); not a qualification',
              'mode': a.mode, 'phase': phase, 'bypass_eligibility': bool(a.bypass_eligibility)}
    locks = pathlib.Path(a.locks)
    elock_bytes = (locks / 'eligibility.lock.json').read_bytes()
    glock_bytes = (locks / 'graphs.lock.json').read_bytes() if a.mode == 'verify' else None
    elock = json.loads(elock_bytes)
    glock = json.loads(glock_bytes) if glock_bytes is not None else None
    schema(ck, elock, glock)
    inp = Inputs(a)
    report['bindings'] = handoff.bindings(locks / 'eligibility.lock.json', elock_bytes, locks / 'graphs.lock.json',
                                          glock_bytes, inp)
    try:
        elf = Elf(inp.oracle)
    except ElfError as e:
        report.update(decision='FAIL', failures=[f'ELF: {e}'])
        return report
    try:
        ident = eligibility(ck, inp, elf, elock, phase, a.objdump)
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as e:
        ck.add('eligibility', 'eligibility inputs and lock readable', False, repr(e))
        ident = elf.identity()
    report['provenance'] = {'oracle_sha256': ident['sha256'], 'build_id': ident['build_id']}
    try:
        listings = load_listings(inp, elf, elock, phase, a.objdump)
        # Static byte-patch fixtures: replace patched code ranges by the strict decoding of the
        # patched bytes (recorded in the report; only valid with the retained-listing substitute).
        for lp in (getattr(a, 'listing_patches', None) or []):
            if phase != 'static':
                raise ListingError('listing patches are a static-phase fixture mechanism only')
            try:
                listings[lp[0]].patch_range(elf, lp[1], lp[2], decode_range, lp[3] if len(lp) > 3 else None)
            except DecodeError as e:
                raise ListingError(f'patched range {lp[1]:#x} not decodable by the strict decoder: {e}')
        report['listing'] = {n[:40]: {'instructions': len(l.insns), 'decoder': l.version} for n, l in listings.items()}
        report['listing_patches'] = [[lp[0][:40], hex(lp[1]), hex(lp[2]), 'fixture-supplied text' if len(lp) > 3 else 'strict decode']
                                     for lp in (getattr(a, 'listing_patches', None) or [])]
    except (ListingError, ElfError, KeyError, DecodeError) as e:
        ck.add('context', 'complete function listings', False, str(e))
        listings = None
    # context prerequisite (finding 2)
    contexts = {}
    if listings is not None:
        dlst = listings[DOUBLEOPS]
        try:
            context_global(ck, dlst, elock, elf)
        except (KeyError, TypeError, ValueError) as e:
            ck.add('context', 'context bindings readable', False, repr(e))
        for sid in contract.REQUIRED_SITES:
            s = elock['sites'].get(sid)
            if s is None:
                continue
            try:
                dlst.check_boundary(int(s['start'], 16))
                contexts[sid] = context_site(ck, elf, dlst, sid, int(s['start'], 16), elock)
            except (ListingError, GateError, KeyError, TypeError, ValueError) as e:
                ck.add('context', f'{sid}: context', False, str(e))
    report['contexts'] = contexts
    # source
    try:
        exprs, src_spans, layouts = srcexpr.derive(inp.bytes['header'].decode(), inp.bytes['vector2'].decode(),
                                                   inp.bytes['vector3'].decode())
        ck.add('arithmetic', 'source definitions derived from the exact header', True)
    except srcexpr.SourceError as e:
        ck.add('arithmetic', 'source definitions derived from the exact header', False, str(e))
        exprs, src_spans, layouts = None, {}, None
    report['source'] = {'definition_sha256': src_spans, 'layouts': layouts}
    if glock is not None:
        ck.add('arithmetic', 'source data-member layouts', glock['source_layouts'] == layouts)
    # arithmetic per required site (finding 1 inside the executor, finding 4 contracts)
    sites = {}
    for sid in contract.REQUIRED_SITES:
        r = {}
        s = elock['sites'].get(sid)
        if s is None:
            r['error'] = 'no locator for a required site'
            ck.add('arithmetic', f'{sid}: locator present', False)
            sites[sid] = r
            continue
        errs = contract.site_contract_errors(sid, s)
        ck.add('arithmetic', f'{sid}: site fields equal the canonical contract', not errs, errs)
        if listings is None:
            sites[sid] = {'error': 'no listings'}
            continue
        try:
            outs, info = extract(elf, elock, sid, listings)
            fp, doc = graph.fingerprint(outs, s['output_order'])
            r.update(info, fingerprint=fp, nodes=len(doc['nodes']),
                     trees={n: graph.to_json(outs[n]) for n in s['output_order']})
            if exprs is not None:
                r['finite_proof'] = graph.finite_proof(outs, exprs[contract.REQUIRED_SITES[sid]])
                ck.add('arithmetic', f'{sid}: finite proof', r['finite_proof']['ok'], r['finite_proof'])
            if glock is not None:
                g = glock['sites'].get(sid)
                if g is None:
                    ck.add('arithmetic', f'{sid}: reviewed graph present', False)
                else:
                    ck.add('arithmetic', f'{sid}: arithmetic graph fingerprint', g['fingerprint'] == fp, fp)
                    ck.add('arithmetic', f'{sid}: graph node count', g['nodes'] == len(doc['nodes']))
                    ck.add('arithmetic', f'{sid}: graph source', g['source'] == contract.REQUIRED_SITES[sid])
                    ck.add('arithmetic', f'{sid}: source definition hash', g['source_definition_sha256'] == src_spans.get(g['source']))
        except (GateError, SymexError, ElfError, ListingError, KeyError, TypeError, ValueError) as e:
            r['error'] = f'{type(e).__name__}: {e}'
            ck.add('arithmetic', f'{sid}: extraction', False, str(e))
        sites[sid] = r
    report['sites'] = sites
    # scope (finding 3)
    req = set(contract.REQUIRED_SITES)
    sets = {'eligibility locators': set(elock.get('sites', {})), 'context bindings': set(elock.get('contexts', {})),
            'report sites extracted': {k for k, v in sites.items() if 'fingerprint' in v and 'error' not in v}}
    if glock is not None:
        sets['graph locks'] = set(glock['sites'])
    for k, v in sets.items():
        ck.add('scope', f'{k} == reviewed required sites ({len(req)})', bool(req) and v == req,
               {'missing': sorted(req - v), 'extra': sorted(v - req)})
    if a.mode == 'verify':
        try:
            want = collections.Counter(check_inventory(elock, glock, phase, with_categories=True))
            have = collections.Counter([(r['category'], r['check']) for r in ck.rows] + [('scope', INVENTORY_CHECK)])
            ok, det = want == have, {'missing': sorted((want - have).elements())[:8], 'extra': sorted((have - want).elements())[:8]}
        except (KeyError, TypeError, ValueError, AttributeError) as e:
            ok, det = False, repr(e)
        ck.add('scope', INVENTORY_CHECK, ok, det)
    # decisions
    report['checks'] = ck.rows
    st = {c: ck.status(c) for c in ('schema', 'eligibility', 'context', 'scope', 'arithmetic')}
    if st['context'] != 'pass' and st['arithmetic'] == 'pass':
        st['arithmetic'] = 'blocked: context prerequisite failed (fingerprints not usable)'
    st['runtime_qualification'] = 'NOT RUN'
    st['emission'] = 'separate tool (emit.py)'
    report['decisions'] = st
    report['counts'] = {c: {s: sum(1 for r in ck.rows if r['category'] == c and r['status'] == s) for s in ('pass', 'fail', 'not-run')}
                        for c in ('schema', 'eligibility', 'context', 'scope', 'arithmetic')}
    failures = ck.failures()
    if a.bypass_eligibility:
        failures = [f for f in failures if not f.startswith('eligibility')]
    report['failures'] = failures
    core = all(st[c] == 'pass' for c in ('schema', 'context', 'scope', 'arithmetic'))
    if a.mode == 'propose':
        report['decision'] = 'PROPOSAL-ONLY'
    elif a.bypass_eligibility:
        report['decision'] = ('INVALID-FOR-ACCEPTANCE (eligibility bypassed): ' +
                              ', '.join(f'{c} {st[c]}' for c in ('schema', 'context', 'scope', 'arithmetic')))
    elif failures:
        report['decision'] = 'FAIL'
    elif phase == 'static':
        report['decision'] = ('STATIC-PARTIAL (not a gate PASS): ' +
                              ', '.join(f'{c} {st[c]}' for c in ('schema', 'eligibility', 'context', 'scope', 'arithmetic')) +
                              '; runtime qualification NOT RUN')
    else:
        report['decision'] = 'PASS (experimental eligibility, UNQUALIFIED)' if core and st['eligibility'] == 'pass' else 'FAIL'
    return report


def parser():
    p = argparse.ArgumentParser()
    p.add_argument('mode', choices=['verify', 'propose'])
    p.add_argument('--inputs', required=True)
    p.add_argument('--locks', required=True)
    p.add_argument('--out', required=True)
    p.add_argument('--phase', choices=['full', 'static'], required=True)
    p.add_argument('--oracle-src')
    p.add_argument('--objdump', default='objdump')
    p.add_argument('--bypass-eligibility', action='store_true')
    for k in ('oracle', 'header', 'vector2', 'vector3', 'adapter', 'compile-commands', 'provenance', 'source-manifest'):
        p.add_argument('--' + k)
    return p


def main():
    a = parser().parse_args()
    rep = run(a)
    pathlib.Path(a.out).write_text(json.dumps(rep, indent=1, default=str), encoding='utf-8')
    print(rep['decision'])
    for f in rep.get('failures', [])[:20]:
        print('  -', f)
    ok = rep['decision'].startswith(('PASS', 'PROPOSAL', 'STATIC-PARTIAL'))
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
