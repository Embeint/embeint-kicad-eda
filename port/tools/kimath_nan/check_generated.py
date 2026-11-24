# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline checks binding `rust/kicad-math/src/generated.rs` to its reviewed derivation.

Run from the repository root: `python -B port/tools/kimath_nan/check_generated.py`.
No oracle, compiler or Cargo is needed; numpy makes the bounded check take seconds
(without it the same check runs in pure Python, a few minutes).

1. `generated.rs` hashes to the reviewed SHA-256, and parsing its straight-line code gives
   exactly the reviewed operation graphs in `trees/` (operation kinds, ordered dst/src
   operands, constants, output mapping), so the shipped code is the reviewed graph.
2. Finite proof, commutativity only: each output graph equals the pinned C++ expression in
   `libs/kimath/include/math/matrix3x3.h` (written out below) up to swapping the two children
   of `+` and `*`. No associativity, no other rewriting. IEEE 754 `+` and `*` are exactly
   commutative for every non-NaN operand pair, so non-NaN results are the source's.
3. The 136 witnesses in `rust/kicad-math/tests/data/nan_priority_witnesses.txt` cover
   exactly the observable nodes; each reproduces its oracle output on the graph, and
   swapping only its node's priority reproduces its recorded flipped output.
4. The seven nodes without a witness are output-equivalent under a priority swap: checked
   exhaustively over two leaf domains (each leaf a NaN with its own payload, or a signed
   zero, infinity or finite), with a positive control that the same domains detect every
   observable node's swap. The written arguments are in README.md.
"""
import hashlib
import itertools
import json
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
GENERATED = ROOT / 'rust' / 'kicad-math' / 'src' / 'generated.rs'
WITNESSES = ROOT / 'rust' / 'kicad-math' / 'tests' / 'data' / 'nan_priority_witnesses.txt'
GENERATED_SHA256 = 'd24ff0f24a1fae55cc8e9eb85fa783282030083802d58eb0e332be331c6935af'
SITES = json.loads((HERE / 'sites.json').read_text(encoding='utf-8'))
Q, DEFAULT, EXP, MANT = 0x0008000000000000, 0xFFF8000000000000, 0x7FF0000000000000, (1 << 52) - 1
failures = []


def fail(msg):
    failures.append(msg)
    print('FAIL', msg)


# 1. generated.rs -> graphs, compared with the reviewed trees ------------------------------
def leaf_name(token):
    m = re.fullmatch(r'([mab])\[(\d)\]\[(\d)\]', token)
    if m:
        return ['leaf', f'{m.group(1)}{m.group(2)}{m.group(3)}']
    m = re.fullmatch(r'v\.([xyz])', token)
    if m:
        return ['leaf', f'v{m.group(1)}']
    if token == 'scalar':
        return ['leaf', 's']
    m = re.fullmatch(r'f64::from_bits\(0x([0-9a-f]{16})\)', token)
    if m:
        return ['const', f'0x{m.group(1)}']
    return None


def parse_generated(text):
    graphs = {}
    for m in re.finditer(r'pub fn (\w+)\([^)]*\) -> [^{]+\{\n(.*?)\n\}\n', text, re.S):
        env, outputs = {}, None
        for line in (l.strip() for l in m.group(2).split('\n')):
            a = re.fullmatch(r'let (v\d+) = (add|sub|mul|div)\((.+), (.+)\);', line)
            if a:
                dst, kind, x, y = a.groups()
                env[dst] = ['op', kind, env.get(x) or leaf_name(x), env.get(y) or leaf_name(y)]
                if None in env[dst]:
                    fail(f'{m.group(1)}: unknown operand in {line!r}')
                continue
            r = re.fullmatch(r'(?:Matrix3x3D::from_elements|Vector2::new|Vector3::new)\((.*)\)', line)
            if r:
                outputs = [env[t.strip()] for t in r.group(1).split(',')]
            elif re.fullmatch(r'v\d+', line):
                outputs = [env[line]]
            elif line not in ('let m = &matrix.data;', 'let (a, b) = (&a.data, &b.data);'):
                fail(f'{m.group(1)}: unparsed line {line!r}')
        graphs[m.group(1)] = outputs
    return graphs


raw = GENERATED.read_bytes()
if hashlib.sha256(raw).hexdigest() != GENERATED_SHA256:
    fail(f'generated.rs sha256 {hashlib.sha256(raw).hexdigest()} != reviewed {GENERATED_SHA256}')
graphs = parse_generated(raw.decode('utf-8'))
trees = {}
for site, info in SITES.items():
    tree_bytes = (HERE / 'trees' / f'{site}.trees.json').read_bytes()
    if hashlib.sha256(tree_bytes).hexdigest() != info['trees_sha256']:
        fail(f'{site}: trees file hash changed')
    trees[site] = json.loads(tree_bytes)
    got = graphs.get(info['function'])
    want = [o['tree'] for o in trees[site]['outputs'].values()]
    if got != want:
        fail(f'{site}: generated.rs graph differs from the reviewed trees')
if sorted(graphs) != sorted(i['function'] for i in SITES.values()):
    fail(f'generated.rs functions {sorted(graphs)} differ from sites.json')


# 2. finite proof modulo commutativity --------------------------------------------------
def L(n):
    return ['leaf', n]


def op(k, a, b):
    return ['op', k, a, b]


def det(p='m'):
    e = lambda i, j: L(f'{p}{i}{j}')
    return op('add', op('sub', op('mul', e(0, 0), op('sub', op('mul', e(1, 1), e(2, 2)), op('mul', e(1, 2), e(2, 1)))),
                       op('mul', e(0, 1), op('sub', op('mul', e(1, 0), e(2, 2)), op('mul', e(1, 2), e(2, 0))))),
              op('mul', e(0, 2), op('sub', op('mul', e(1, 0), e(2, 1)), op('mul', e(1, 1), e(2, 0)))))


def source_outputs(site):
    m = lambda i, j: L(f'm{i}{j}')
    if site == 'determinant-doubleops':
        return [det()]
    if site == 'inverse':
        cof = lambda a, b, c, d: op('sub', op('mul', m(*a), m(*b)), op('mul', m(*c), m(*d)))
        rows = [cof((1, 1), (2, 2), (2, 1), (1, 2)), cof((0, 2), (2, 1), (2, 2), (0, 1)),
                cof((0, 1), (1, 2), (1, 1), (0, 2)), cof((1, 2), (2, 0), (2, 2), (1, 0)),
                cof((0, 0), (2, 2), (2, 0), (0, 2)), cof((0, 2), (1, 0), (1, 2), (0, 0)),
                cof((1, 0), (2, 1), (2, 0), (1, 1)), cof((0, 1), (2, 0), (2, 1), (0, 0)),
                cof((0, 0), (1, 1), (1, 0), (0, 1))]
        recip = op('div', ['const', '0x3ff0000000000000'], det())
        return [op('mul', r, recip) for r in rows]  # result * ( 1.0 / Determinant() )
    if site == 'mul_matrix-doubleops':
        a = lambda i, j: L(f'a{i}{j}')
        b = lambda i, j: L(f'b{i}{j}')
        return [op('add', op('add', op('mul', a(i, 0), b(0, j)), op('mul', a(i, 1), b(1, j))), op('mul', a(i, 2), b(2, j)))
                for i in range(3) for j in range(3)]
    if site == 'mul_vector2-doubleops':
        return [op('add', op('add', op('mul', m(i, 0), L('vx')), op('mul', m(i, 1), L('vy'))), m(i, 2)) for i in range(2)]
    if site == 'mul_vector3-doubleops':
        return [op('add', op('add', op('mul', m(i, 0), L('vx')), op('mul', m(i, 1), L('vy'))), op('mul', m(i, 2), L('vz')))
                for i in range(3)]
    # operator*( M, S ) and operator*( S, M ), which returns aMatrix * aScalar
    return [op('mul', m(i, j), L('s')) for i in range(3) for j in range(3)]


def canon(t):
    if t[0] != 'op':
        return json.dumps(t)
    a, b = canon(t[2]), canon(t[3])
    if t[1] in ('add', 'mul') and b < a:
        a, b = b, a
    return f'({t[1]} {a} {b})'


for site in SITES:
    want = [canon(t) for t in source_outputs(site)]
    got = [canon(o['tree']) for o in trees[site]['outputs'].values()]
    if got != want:
        fail(f'{site}: graph is not the C++ expression modulo commutativity of + and *')


# Bit-exact SSE2 evaluator over interned nodes (same node numbering as the review) ------------
def intern_site(site):
    pool, nodes = {}, []

    def intern(e):
        key = json.dumps(e, separators=(',', ':'))
        if key not in pool:
            node = ('op', e[1], intern(e[2]), intern(e[3])) if e[0] == 'op' else tuple(e)
            pool[key] = len(nodes)
            nodes.append(node)
        return pool[key]
    roots = [intern(o['tree']) for o in trees[site]['outputs'].values()]
    return nodes, roots, [i for i, n in enumerate(nodes) if n[0] == 'op']


try:
    import numpy as np
except ImportError:
    np = None


def make_ops(xp):
    if xp is None:
        import math
        import struct
        f = lambda w: struct.unpack('<d', struct.pack('<Q', w))[0]
        g = lambda x: struct.unpack('<Q', struct.pack('<d', x))[0]

        def isnan(w):
            return (w & EXP) == EXP and (w & MANT) != 0

        def apply(kind, first, second, a, c):
            if isnan(first):
                return first | Q
            if isnan(second):
                return second | Q
            x, y = f(a), f(c)
            if kind == 'div' and y == 0.0:
                return DEFAULT if x == 0.0 else ((a ^ c) & (1 << 63)) | EXP
            z = {'add': x + y, 'sub': x - y, 'mul': x * y, 'div': x / y if y else 0.0}[kind]
            return DEFAULT if math.isnan(z) else g(z)
        return apply

    def isnan(w):
        return ((w & np.uint64(EXP)) == np.uint64(EXP)) & ((w & np.uint64(MANT)) != 0)

    def apply(kind, first, second, a, c):
        x, y = a.view(np.float64), c.view(np.float64)
        with np.errstate(all='ignore'):
            z = {'add': np.add, 'sub': np.subtract, 'mul': np.multiply, 'div': np.divide}[kind](x, y)
        zb = z.view(np.uint64)
        zb = np.where(isnan(zb), np.uint64(DEFAULT), zb)
        return np.where(isnan(first), first | np.uint64(Q), np.where(isnan(second), second | np.uint64(Q), zb))
    return apply


APPLY = make_ops(np)


def evaluate(nodes, roots, leaves, flip=None):
    """leaves: name -> word (int) or numpy uint64 array; flip: node index whose priority swaps."""
    v = []
    for i, n in enumerate(nodes):
        if n[0] == 'leaf':
            v.append(leaves[n[1]])
        elif n[0] == 'const':
            c = int(n[1], 16)
            v.append(np.full_like(next(iter(leaves.values())), c) if np is not None and not isinstance(
                next(iter(leaves.values())), int) else c)
        else:
            a, c = v[n[2]], v[n[3]]
            first, second = (c, a) if i == flip else (a, c)
            v.append(APPLY(n[1], first, second, a, c))
    return [v[r] for r in roots]


# 3. witnesses ------------------------------------------------------------------------
apply_scalar = make_ops(None)


def evaluate_scalar(nodes, roots, leaves, flip=None):
    v = []
    for i, n in enumerate(nodes):
        if n[0] == 'leaf':
            v.append(leaves[n[1]])
        elif n[0] == 'const':
            v.append(int(n[1], 16))
        else:
            a, c = v[n[2]], v[n[3]]
            first, second = (c, a) if i == flip else (a, c)
            v.append(apply_scalar(n[1], first, second, a, c))
    return [v[r] for r in roots]


seen = {site: set() for site in SITES}
for line in WITNESSES.read_text(encoding='ascii').splitlines():
    if line.startswith('#'):
        continue
    head, ins, out, mut = line.split(' | ')
    site, node, case = head.split()
    nodes, roots, arith = intern_site(site)
    leaves = dict(zip(SITES[site]['inputs'], (int(x, 16) for x in ins.split())))
    k = int(node)
    if evaluate_scalar(nodes, roots, leaves) != [int(x, 16) for x in out.split()]:
        fail(f'{site} node {k} ({case}): graph does not give the oracle output')
    if evaluate_scalar(nodes, roots, leaves, arith[k]) != [int(x, 16) for x in mut.split()]:
        fail(f'{site} node {k} ({case}): flipped graph does not give the recorded flipped output')
    seen[site].add(k)
for site, info in SITES.items():
    nodes, roots, arith = intern_site(site)
    observable = set(range(len(arith))) - set(info['equivalent_nodes'])
    if seen[site] != observable:
        fail(f'{site}: witnessed nodes {sorted(seen[site])} != observable nodes {sorted(observable)}')


# 4. bounded exhaustive equivalence of the seven unwitnessed nodes ---------------------------
def domains(names):
    """Two exhaustive leaf domains; each leaf's NaN has its own payload (and sign/quietness)."""
    out = []
    for flavour in (0, 1):
        per_leaf = []
        for idx, _ in enumerate(names):
            if flavour == 0:
                nan = 0x7FF8000000000000 | (idx + 1)          # positive quiet NaN
                per_leaf.append([nan, 0x0000000000000000, 0x7FF0000000000000, 0x3FF8000000000000])
            else:
                nan = 0xFFF0000000000000 | (idx + 1)          # negative signalling NaN
                per_leaf.append([nan, 0x8000000000000000, 0xFFF0000000000000, 0xC000000000000000])
        out.append(per_leaf)
    return out


def exhaustive(site, flips):
    """Returns {flip: number of inputs whose outputs change} over both domains."""
    nodes, roots, arith = intern_site(site)
    names = SITES[site]['inputs']
    counts = {k: 0 for k in flips}
    for per_leaf in domains(names):
        if np is not None:
            grids = np.meshgrid(*[np.array(v, dtype=np.uint64) for v in per_leaf], indexing='ij')
            leaves = {n: g.reshape(-1) for n, g in zip(names, grids)}
            base = evaluate(nodes, roots, leaves)
            for k in flips:
                alt = evaluate(nodes, roots, leaves, arith[k])
                diff = np.zeros(base[0].shape, dtype=bool)
                for x, y in zip(base, alt):
                    diff |= x != y
                counts[k] += int(diff.sum())
        else:
            for combo in itertools.product(*per_leaf):
                leaves = dict(zip(names, combo))
                base = evaluate_scalar(nodes, roots, leaves)
                for k in flips:
                    counts[k] += evaluate_scalar(nodes, roots, leaves, arith[k]) != base
    return counts


report = {'numpy': np is not None, 'equivalent': {}, 'observable_detected': {}}
for site in ('determinant-doubleops', 'inverse'):
    info = SITES[site]
    nodes, roots, arith = intern_site(site)
    counts = exhaustive(site, range(len(arith)))
    for k in range(len(arith)):
        if k in info['equivalent_nodes']:
            report['equivalent'][f'{site}#{k}'] = counts[k]
            if counts[k]:
                fail(f'{site} node {k}: claimed equivalent but {counts[k]} inputs differ')
        else:
            report['observable_detected'][f'{site}#{k}'] = counts[k]
            if not counts[k]:
                fail(f'{site} node {k}: positive control, the domain does not detect this swap')
print(json.dumps(report, indent=1))
if failures:
    sys.exit(f'{len(failures)} check(s) failed')
print('OK: generated.rs is the reviewed graph; finite results equal the C++ modulo commutativity; '
      '136 witnesses reproduce; 7 unwitnessed swaps are output-equivalent over the bounded domains')
