"""Recursive typed schema for both locks (private prototype; finding R4).

Every object has an exact set of required and optional keys; every value has a type (nonblank
strings, fixed-length lowercase hex, integers that are not booleans, enumerations, constants,
bounded lists, exact-inventory maps). Dynamic maps carry an explicit reviewed key inventory
declared here (identity-specific inventories are marked so). Fields whose meaning the gate
checks are pinned to their canonical values (SEMANTIC); documentary fields are typed 'doc'
(nonblank text, never used for a decision). Validation proves record integrity and shape,
not authorship or the semantic quality of a review.
"""
import re

import contract

DOUBLEOPS = '_ZN6oracleL9DoubleOpsERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEERKN8nlohmann16json_abi_v3_11_310basic_jsonISt3mapSt6vectorS5_blmdSaNS9_14adl_serializerESC_IhSaIhEEvEENS_4KINDE'
INVERSE = '_ZNK9MATRIX3x3IdE7InverseEv'
ADAPTER_SPAN_KEYS = {'KIND', 'Double', 'Bits', 'Elements', 'Element<double>', 'ElementJSON(double)', 'Vector2', 'Vector3',
                     'Matrix', 'MatrixReply', 'Vector2JSON', 'Vector3JSON', 'DoubleOps', 'KimathMath'}
# Identity-specific reviewed inventories (binary 0bac7567...): any change needs a reviewed edit here.
REVIEWED_CALLEES = {'0x41d00', '0x50370', '0x449e0', '0x43e80', '0x44760', '0x44930', '0x1faa0', '0x31150', '0x51000',
                    '0x44f60', '0x47a40', '0x47f10', '0x474b0', '0x4fbb0'}
BRANCH_INVENTORY = {'determinant-doubleops': set(), 'inverse': set(), 'mul_matrix-doubleops': set(),
                    'mul_vector2-doubleops': set(), 'mul_vector3-doubleops': set(),
                    'mul_scalar-doubleops': {'0x495cf'}, 'scalar_mul-doubleops': {'0x481f1'}}
NOT_EXTRACTED_KEYS = {'mul_scalar/scalar_mul with an int scalar'}
ASSUMPTION_IDS = ['A1', 'A2', 'A3', 'A4', 'A5']
CANON_BINDINGS = {'r12': {'source_family': 'rdx', 'value': 'json_input'},
                  'r15': {'source_family': 'rcx', 'value': 'kind_arg'},
                  'op': {'source_family': 'rsi', 'value': 'aOp'}}
ANALYSE_ARGS = {'rdi': {'size': 72, 'access': 'write'}, 'rsi': {'size': 72, 'access': 'read'}}


def obj(keys, optional=()):
    return ('obj', keys, dict(optional))


def mp(inventory, value):
    return ('map', inventory, value)


def lst(item, lo, hi=None):
    return ('list', item, lo, hi)


STR, DOC, ADDR = ('str',), ('doc',), ('re', r'0x[0-9a-f]+')
HEX = lambda n: ('re', r'[0-9a-f]{%d}' % n)
INT = lambda lo: ('int', lo)
ENUM = lambda *v: ('enum', set(v))
CONST = lambda v: ('const', v)

BINDING = obj({'binding_insn': ADDR, 'binding_text': STR, 'source_family': ENUM('rdx', 'rcx', 'rsi'),
               'value': ENUM('json_input', 'kind_arg', 'aOp'), 'meaning': DOC})
CALLEE = obj({'symbol': STR, 'sha256': HEX(64), 'effect': STR, 'contract_sha256': HEX(64), 'rationale': DOC},
             optional={'padding': INT(0),
                       'args': mp({'rdi', 'rsi'}, obj({'size': INT(1), 'access': ENUM('read', 'write')}))})
BRANCH = obj({'mnem': ENUM('je', 'jne'), 'target': ADDR, 'flag_value': lst(ENUM('kind_arg'), 1, 1),
              'flag_imm': INT(-128), 'expect': ENUM('fallthrough', 'taken'), 'meaning': DOC})
RUST = obj({'name': STR, 'params': STR, 'ret': STR, 'prelude': STR, 'leaves': mp(None, STR), 'return': STR})


def site_spec(sid):
    return obj({'op': ENUM(*contract.OPS), 'function': CONST(DOUBLEOPS), 'start': ADDR, 'stop': ADDR,
                'consumer': CONST(contract.OPS[contract.REQUIRED_SITES[sid]]['consumer']),
                'roles': mp(None, STR), 'branches': mp(BRANCH_INVENTORY[sid], BRANCH),
                'source': ENUM(*contract.OPS), 'output_order': lst(STR, 1), 'label': STR,
                'dispatch': obj({'literal': STR, 'kind': ENUM('jne', 'je')}), 'rust': RUST})


ELIGIBILITY = obj({
    'note': DOC,
    'status': CONST('EXPERIMENTAL-ELIGIBLE-UNQUALIFIED'),
    'image': obj({'id': ('re', r'sha256:[0-9a-f]{64}'), 'status': DOC}),
    'oracle': obj({'sha256': HEX(64), 'build_id': HEX(40), 'x86_isa_needed_data': HEX(8),
                   'abi_tag': lst(INT(0), 4, 4), 'comment': lst(STR, 1)}),
    'compiler': obj({'path': STR, 'version_line': STR, 'target': STR, 'libc': STR}),
    'compile_commands_sha256': HEX(64), 'compile_count': INT(1), 'provenance_sha256': HEX(64),
    'adapter_compile': obj({'file': STR, 'output': STR, 'header_include_dir': STR, 'command': STR}),
    'source': obj({'revision': HEX(40), 'tree': HEX(40), 'pin': HEX(40), 'manifest_sha256': HEX(64),
                   'blobs': mp({'header', 'vector2', 'vector3', 'adapter'}, obj({'blob': HEX(40), 'git_path': STR})),
                   'compiler_input_paths': mp({'header', 'vector2', 'vector3', 'adapter'}, STR)}),
    'adapter_spans': mp(ADAPTER_SPAN_KEYS, HEX(64)),
    'runtime_fp': obj({'mxcsr': CONST('0x1f80'), 'tunables': CONST('glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F'),
                       'evidence': DOC}),
    'analysed_functions': CONST([DOUBLEOPS, INVERSE]),
    'entry_registers': mp({'r12', 'r15'}, BINDING),
    'op_binding': BINDING,
    'frame': obj({'size': INT(1), 'setup_insn': ADDR, 'setup_text': STR}),
    'callees': mp(REVIEWED_CALLEES, CALLEE),
    'sites': ('sites',),
    'contexts': mp(set(contract.REQUIRED_SITES), obj({'path_instructions': INT(1), 'path_bytes_sha256': HEX(64),
                                                      'path_call_count': INT(1), 'path_calls_sha256': HEX(64)})),
    'context_review': obj({'binary_sha256': HEX(64), 'binary_build_id': HEX(40), 'reviewer': STR,
                           'citations': lst(obj({'path': STR, 'sha256': HEX(64), 'section': STR}), 2),
                           'assumptions': lst(STR, len(ASSUMPTION_IDS), len(ASSUMPTION_IDS)), 'renewal': STR}),
    'static_substitutes': obj({'listings': mp({DOUBLEOPS, INVERSE}, obj({'path': STR, 'sha256': HEX(64), 'decoder': STR}))}),
    'not_extracted': mp(NOT_EXTRACTED_KEYS, DOC),
})
GRAPHS = obj({
    'note': DOC, 'graph_format': CONST('pr50-nan-arith-graph-v1'), 'generated_rs_sha256': HEX(64),
    'source_layouts': mp({'MATRIX3x3', 'VECTOR2', 'VECTOR3'}, lst(STR, 1)),
    'sites': mp(set(contract.REQUIRED_SITES), obj({'fingerprint': HEX(64), 'nodes': INT(1), 'source': ENUM(*contract.OPS),
                                                   'source_definition_sha256': HEX(64),
                                                   'observable_flip_witnesses': INT(0), 'equivalence_proofs': INT(0)})),
})


def validate(spec, v, path, errs):
    k = spec[0]
    if k in ('str', 'doc'):
        if not isinstance(v, str) or not v.strip():
            errs.append(f'{path}: nonblank string required')
    elif k == 're':
        if not isinstance(v, str) or not re.fullmatch(spec[1], v):
            errs.append(f'{path}: must match {spec[1]}')
    elif k == 'int':
        if not isinstance(v, int) or isinstance(v, bool) or v < spec[1]:
            errs.append(f'{path}: integer >= {spec[1]} required (booleans rejected)')
    elif k == 'enum':
        if not isinstance(v, str) or v not in spec[1]:
            errs.append(f'{path}: must be one of {sorted(spec[1])}')
    elif k == 'const':
        if not contract.typed_equal(v, spec[1]):
            errs.append(f'{path}: must equal the reviewed constant')
    elif k == 'list':
        if not isinstance(v, list) or len(v) < spec[2] or (spec[3] is not None and len(v) > spec[3]):
            errs.append(f'{path}: list of {spec[2]}..{spec[3]} items required')
        else:
            for i, x in enumerate(v):
                validate(spec[1], x, f'{path}[{i}]', errs)
    elif k == 'obj':
        if not isinstance(v, dict):
            errs.append(f'{path}: object required')
            return
        keys, opt = spec[1], spec[2]
        extra, missing = set(v) - set(keys) - set(opt), set(keys) - set(v)
        if extra:
            errs.append(f'{path}: unknown fields {sorted(extra)}')
        if missing:
            errs.append(f'{path}: missing fields {sorted(missing)}')
        for key, sub in {**keys, **opt}.items():
            if key in v:
                validate(sub, v[key], f'{path}.{key}', errs)
    elif k == 'map':
        if not isinstance(v, dict):
            errs.append(f'{path}: object required')
            return
        if spec[1] is not None and set(v) != set(spec[1]):
            errs.append(f'{path}: keys must equal the reviewed inventory (missing {sorted(set(spec[1]) - set(v))}, '
                        f'extra {sorted(set(v) - set(spec[1]))})')
        for key, x in v.items():
            if not isinstance(key, str) or not key:
                errs.append(f'{path}: blank key')
            validate(spec[2], x, f'{path}.{key}', errs)
    elif k == 'sites':
        if not isinstance(v, dict):
            errs.append(f'{path}: object required')
            return
        for sid, s in v.items():
            if sid in BRANCH_INVENTORY:
                validate(site_spec(sid), s, f'{path}.{sid}', errs)
            # unknown or missing sites are reported by the scope stage against REQUIRED_SITES
    else:
        errs.append(f'{path}: unknown schema kind {k}')


def semantic(elock, contracts, contract_sha):
    """Checked semantic fields pinned to the implemented/canonical contracts."""
    errs = []
    eg = elock.get('entry_registers', {})
    for name, want in (('r12', CANON_BINDINGS['r12']), ('r15', CANON_BINDINGS['r15'])):
        b = eg.get(name, {}) if isinstance(eg, dict) else {}
        for f, val in want.items():
            if not isinstance(b, dict) or b.get(f) != val:
                errs.append(f'entry_registers.{name}.{f} must be {val!r}')
    ob = elock.get('op_binding', {})
    for f, val in CANON_BINDINGS['op'].items():
        if not isinstance(ob, dict) or ob.get(f) != val:
            errs.append(f'op_binding.{f} must be {val!r}')
    fr = elock.get('frame', {})
    if isinstance(fr, dict) and isinstance(fr.get('size'), int) and fr.get('setup_text') != f'sub ${fr["size"]:#x},%rsp':
        errs.append('frame.setup_text must be the setup of frame.size')
    for a, c in (elock.get('callees') or {}).items():
        if not isinstance(c, dict):
            continue
        e = c.get('effect')
        if e not in contracts:
            errs.append(f'callees.{a}.effect {e!r} is not an implemented contract')
            continue
        if c.get('contract_sha256') != contract_sha(e):
            errs.append(f'callees.{a}.contract_sha256 differs from the implemented {e} contract')
        if e == 'analyse':
            if not contract.typed_equal(c.get('args'), ANALYSE_ARGS) or not isinstance(c.get('padding'), int) or isinstance(c.get('padding'), bool) \
                    or c['padding'] < 0:
                errs.append(f'callees.{a}: analyse needs the reviewed args {ANALYSE_ARGS} and an integer padding')
        elif 'args' in c or 'padding' in c:
            errs.append(f'callees.{a}: args/padding only apply to analysed callees')
    cr = elock.get('context_review', {})
    if isinstance(cr, dict) and isinstance(cr.get('assumptions'), list):
        ids = [x.split(' ', 1)[0] if isinstance(x, str) else None for x in cr['assumptions']]
        if ids != ASSUMPTION_IDS:
            errs.append(f'context_review.assumptions must be exactly {ASSUMPTION_IDS} in order')
        for x in cr['assumptions']:
            if not isinstance(x, str) or len(x.split(' ', 1)) < 2 or not x.split(' ', 1)[1].strip():
                errs.append('context_review.assumptions: every assumption needs nonblank text after its ID')
    if isinstance(cr, dict) and isinstance(cr.get('citations'), list):
        paths = [c.get('path') for c in cr['citations'] if isinstance(c, dict)]
        if len(set(paths)) != len(paths):
            errs.append('context_review.citations: duplicate citation paths')
    return errs
