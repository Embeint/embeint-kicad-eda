"""Gate -> emitter handoff (private prototype; finding R3).

The gate records `bindings`: SHA-256 of the exact lock bytes it parsed, of every input file it
consumed (paths recorded), of the oracle bytes (and whether they were an in-memory fixture), of
the generation contract (canonical contract.py content) and of the tool sources. At accepted
emission, `validate_full_pass` re-checks a report for internal consistency and freshness:

- mode verify, phase full, no eligibility bypass, no listing patches, no retained/substitute
  listing, decision PASS, every stage decision exactly 'pass', failures == [];
- every check row well-formed with status exactly 'pass'; the multiset of (category, check) pairs equals the
  gate's full-phase inventory for the CURRENT locks; per-category counts recomputed from the
  rows equal the reported counts (integers, not booleans);
- the report's binding digests equal digests recomputed NOW from the current lock files, the
  recorded input files, the current generation contract and the current tool sources;
- the current locks pass the schema and scope checks again; every required site is present
  with the current locked fingerprint, a true finite proof and trees.
This rejects contradictions and stale handoffs. It does not authenticate arbitrary JSON: a
fabricated but internally consistent report is outside what a digest check can detect.
"""
import collections
import hashlib
import json
import pathlib

import contract

TOOL_SOURCES = ['gate.py', 'symex.py', 'contract.py', 'listing.py', 'x86strict.py', 'elfbin.py', 'srcexpr.py',
                'graph.py', 'lockschema.py', 'handoff.py', 'emit.py']
STAGES = ('schema', 'eligibility', 'context', 'scope', 'arithmetic')
HERE = pathlib.Path(__file__).resolve().parent


def sha(b):
    return hashlib.sha256(b).hexdigest()


def generation_contract_sha256():
    doc = {'REQUIRED_SITES': contract.REQUIRED_SITES, 'EMIT_ORDER': contract.EMIT_ORDER, 'ROLE_RUST': contract.ROLE_RUST,
           'OPS': contract.OPS, 'canonical_rust': {op: contract.canonical_rust(op) for op in contract.OPS}}
    return sha(json.dumps(doc, sort_keys=True, separators=(',', ':')).encode())


def tool_sources():
    return {n: sha((HERE / n).read_bytes()) for n in TOOL_SOURCES}


def bindings(elock_path, elock_bytes, glock_path, glock_bytes, inp):
    return {
        'eligibility_lock': {'path': str(pathlib.Path(elock_path).resolve()), 'sha256': sha(elock_bytes)},
        'graphs_lock': None if glock_bytes is None else {'path': str(pathlib.Path(glock_path).resolve()),
                                                         'sha256': sha(glock_bytes)},
        'inputs': {k: {'path': str(p.resolve()), 'sha256': sha(inp.bytes[k])} for k, p in inp.files.items()},
        'oracle': {'path': None if inp.oracle_in_memory else str(inp.oracle_path.resolve()), 'sha256': sha(inp.oracle),
                   'in_memory_fixture': inp.oracle_in_memory},
        'generation_contract_sha256': generation_contract_sha256(),
        'tool_sources_sha256': tool_sources(),
    }


def validate_full_pass(report, locks_dir, gate):
    """Returns a list of reasons the report cannot back accepted emission (empty = acceptable)."""
    e = []
    if not isinstance(report, dict):
        return ['report is not an object']
    want = {'mode': 'verify', 'phase': 'full', 'bypass_eligibility': False}
    for k, v in want.items():
        if report.get(k) != v or type(report.get(k)) is not type(v):
            e.append(f'report {k} must be {v!r}')
    if report.get('listing_patches') != []:
        e.append('report must carry an empty listing_patches list (no static fixtures)')
    lst = report.get('listing')
    if not isinstance(lst, dict) or len(lst) != 2 or any(not isinstance(x, dict) or not isinstance(x.get('decoder'), str)
                                                          or x['decoder'].startswith('RETAINED') for x in lst.values()):
        e.append('report listings must be two fresh (non-retained) decodings')
    if not isinstance(report.get('decision'), str) or not report['decision'].startswith('PASS'):
        e.append('report decision is not PASS')
    dec = report.get('decisions')
    if not isinstance(dec, dict) or any(dec.get(s) != 'pass' for s in STAGES):
        e.append(f'every stage decision must be exactly pass: {dec!r}')
    if report.get('failures') != []:
        e.append('report failures must be an empty list')
    rows = report.get('checks')
    if not isinstance(rows, list) or not rows:
        return e + ['report has no check rows']
    for r in rows:
        if not isinstance(r, dict) or set(r) != {'category', 'check', 'status', 'detail'} or r.get('category') not in STAGES \
                or not isinstance(r.get('check'), str):
            e.append(f'malformed check row {r!r:.120}')
        elif r['status'] != 'pass':
            e.append(f'check not passing: {r["category"]}: {r["check"]} = {r["status"]!r}')
    counts = report.get('counts')
    recomputed = {c: {s: sum(1 for r in rows if isinstance(r, dict) and r.get('category') == c and r.get('status') == s)
                      for s in ('pass', 'fail', 'not-run')} for c in STAGES}
    if not isinstance(counts, dict) or set(counts) != set(STAGES) or any(
            not isinstance(counts[c], dict) or set(counts[c]) != {'pass', 'fail', 'not-run'} or
            any(not isinstance(v, int) or isinstance(v, bool) for v in counts[c].values()) for c in counts):
        e.append('report counts are malformed')
    elif counts != recomputed:
        e.append('report counts differ from its check rows')
    locks = pathlib.Path(locks_dir)
    try:
        eb, gb = (locks / 'eligibility.lock.json').read_bytes(), (locks / 'graphs.lock.json').read_bytes()
        elock, glock = json.loads(eb), json.loads(gb)
    except (OSError, ValueError) as x:
        return e + [f'current locks unreadable: {x}']
    try:
        inv = collections.Counter(gate.check_inventory(elock, glock, 'full', with_categories=True))
    except (KeyError, TypeError, ValueError, AttributeError) as x:
        inv = None
        e.append(f'current locks do not define a check inventory: {x!r}')
    if inv is not None:
        got = collections.Counter((r.get('category'), r.get('check')) for r in rows if isinstance(r, dict))
        if got != inv:
            e.append(f'check categories/names differ from the full-phase inventory (missing {sorted((inv - got).elements(), key=repr)[:5]}, '
                     f'extra {sorted((got - inv).elements(), key=repr)[:5]})')
    b = report.get('bindings')
    if not isinstance(b, dict):
        return e + ['report has no bindings']
    if (b.get('eligibility_lock') or {}).get('sha256') != sha(eb) or (b.get('graphs_lock') or {}).get('sha256') != sha(gb):
        e.append('report was produced with different lock bytes than the current locks (stale handoff)')
    if b.get('generation_contract_sha256') != generation_contract_sha256():
        e.append('generation contract changed since the report')
    if b.get('tool_sources_sha256') != tool_sources():
        e.append('tool sources changed since the report')
    o = b.get('oracle') or {}
    if o.get('in_memory_fixture') is not False or not o.get('path'):
        e.append('oracle was an in-memory fixture, not a file')
    else:
        try:
            if sha(pathlib.Path(o['path']).read_bytes()) != o.get('sha256'):
                e.append('oracle file changed since the report')
        except OSError as x:
            e.append(f'oracle file unreadable: {x}')
    if o.get('sha256') != (elock.get('oracle') or {}).get('sha256'):
        e.append('report oracle differs from the current eligibility lock')
    ins = b.get('inputs')
    if not isinstance(ins, dict) or set(ins) != {'header', 'vector2', 'vector3', 'adapter', 'compile_commands',
                                                  'provenance', 'source_manifest'}:
        e.append('report input bindings incomplete')
    else:
        for k, v in ins.items():
            try:
                if sha(pathlib.Path(v['path']).read_bytes()) != v['sha256']:
                    e.append(f'input {k} changed since the report')
            except (OSError, KeyError, TypeError) as x:
                e.append(f'input {k} unreadable: {x!r}')
    ck = gate.Checks()
    gate.schema(ck, elock, glock)
    e += [f'current lock {f}' for f in ck.failures()]
    req = set(contract.REQUIRED_SITES)
    for name, s in (('eligibility sites', elock.get('sites')), ('contexts', elock.get('contexts')), ('graph sites', glock.get('sites'))):
        if not isinstance(s, dict) or set(s) != req:
            e.append(f'current {name} differ from the reviewed required sites')
    sites = report.get('sites')
    if not isinstance(sites, dict) or set(sites) != req:
        e.append('report sites differ from the reviewed required sites')
    else:
        for sid in req:
            r = sites[sid]
            if not isinstance(r, dict) or 'error' in r or r.get('fingerprint') != ((glock.get('sites') or {}).get(sid) or {}).get('fingerprint') \
                    or (r.get('finite_proof') or {}).get('ok') is not True or not isinstance(r.get('trees'), dict):
                e.append(f'report site {sid} is not a passing extraction of the current locked graph')
    return e
