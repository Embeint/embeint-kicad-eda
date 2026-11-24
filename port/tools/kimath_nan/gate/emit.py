"""Generated-Rust emitter with an enforced generation contract (private prototype; finding 4).

usage:
  emit.py candidate --report REPORT --locks DIR --out-dir DIR     proposal output, never accepted
  emit.py accept    --report REPORT --locks DIR --accepted FILE   accepted output, atomic

Both modes derive every Rust leaf, parameter, prelude, return and function name ONLY from the
canonical contract (contract.py); the lock's `rust` and `label` fields are cross-checked (the
label is part of the emitted bytes). The emitted text must hash to graphs.lock.json
`generated_rs_sha256`. Exit codes: 0 ok; 2 report/contract/scope refused; 3 emitted-code drift.
`accept` additionally requires a full-phase gate PASS without eligibility bypass, writes a
temporary file in the destination directory and replaces the accepted file only after every
check passed (os.replace); on any failure the existing accepted file is left untouched.
`candidate` writes `generated.rs.candidate` and `candidate.json` (never a file named like the
accepted output) so a proposal can be reviewed without being usable as accepted code.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import contract  # noqa: E402
import gate  # noqa: E402
import handoff  # noqa: E402

HEADER = ['// SPDX-License-Identifier: GPL-3.0-or-later',
          '//! GENERATED from the pinned oracle disassembly by scripts/emu.py + scripts/gen_rust.py.',
          '//! Do not edit; regenerate with scripts/gen_sites.sh.',
          '#![allow(clippy::let_and_return)]',
          'use crate::sse2::{add, div, mul, sub};',
          'use kicad_math::{Matrix3x3D, Vector2, Vector3};', '']


class Refused(Exception):
    pass


def tup(e):
    return tuple(tup(x) if isinstance(x, list) else x for x in e)


def site_text(site_id, label, trees, rust):
    names, lines = {}, []

    def emit(e):
        if e[0] == 'leaf':
            if e[1] not in rust['leaves']:
                raise Refused(f'{site_id}: leaf {e[1]} has no canonical Rust mapping')
            return rust['leaves'][e[1]]
        if e[0] == 'const':
            if not re.fullmatch(r'0x[0-9a-f]{16}', e[1]):
                raise Refused(f'{site_id}: malformed constant {e[1]!r}')
            return f'f64::from_bits({e[1]})'
        if e[0] != 'op' or e[1] not in ('add', 'sub', 'mul', 'div'):
            raise Refused(f'{site_id}: non-arithmetic node {e!r}')
        key = repr(e)
        if key not in names:
            a, b = emit(e[2]), emit(e[3])
            names[key] = f'v{len(names)}'
            lines.append(f'    let {names[key]} = {e[1]}({a}, {b});')
        return names[key]
    outs = {n: emit(tup(trees[n])) for n in contract.OPS[contract.REQUIRED_SITES[site_id]]['outputs']}
    text = f'/// GENERATED from `sites/{site_id}.trees.json`: site `{label}`.\n'
    text += '/// Each call is one instruction lane, `op(dst, src)`, in the pinned binary operand order.\n'
    text += f'pub fn {rust["name"]}({rust["params"]}) -> {rust["ret"]} {{\n'
    text += f'    {rust["prelude"]}\n'
    text += '\n'.join(lines) + '\n'
    text += '    ' + rust['return'].format(**outs) + '\n'
    text += '}\n\n'
    return text


def generate(report, elock, glock, accept):
    if report.get('mode') != 'verify':
        raise Refused('report is not a gate verify report')
    if report.get('bypass_eligibility'):
        raise Refused('report was produced with the eligibility bypass (diagnostic only)')
    d = report.get('decision', '')
    if accept:
        if not (report.get('phase') == 'full' and d.startswith('PASS')):
            raise Refused(f'accepted output needs a full-phase gate PASS, report decision is {d!r}')
    elif not (d.startswith('PASS') or d.startswith('STATIC-PARTIAL')):
        raise Refused(f'candidate output needs a gate PASS or STATIC-PARTIAL report, decision is {d!r}')
    for c in ('schema', 'context', 'scope', 'arithmetic'):
        if report.get('decisions', {}).get(c) != 'pass':
            raise Refused(f'report decision {c} is {report.get("decisions", {}).get(c)!r}')
    req = set(contract.REQUIRED_SITES)
    if set(report['sites']) != req or set(elock['sites']) != req or set(glock['sites']) != req:
        raise Refused('report, eligibility lock and graph lock sites must equal the reviewed required sites')
    text = '\n'.join(HEADER) + '\n'
    for sid in contract.EMIT_ORDER:
        r, s = report['sites'][sid], elock['sites'][sid]
        if r.get('fingerprint') != glock['sites'][sid]['fingerprint'] or 'error' in r:
            raise Refused(f'{sid}: report fingerprint does not equal the reviewed graph lock')
        rust = contract.canonical_rust(contract.REQUIRED_SITES[sid])
        if s.get('rust') != rust:
            raise Refused(f'{sid}: lock rust mapping differs from the canonical generation contract')
        text += site_text(sid, s['label'], r['trees'], rust)
    fns = re.findall(r'^pub fn (\w+)\(', text, re.M)
    want = sorted(contract.OPS[contract.REQUIRED_SITES[s]]['name'] for s in contract.EMIT_ORDER)
    if sorted(fns) != want or len(fns) != len(set(fns)):
        raise Refused(f'generated functions {fns} differ from the reviewed set')
    return text


def main():
    p = argparse.ArgumentParser()
    p.add_argument('mode', choices=['candidate', 'accept'])
    p.add_argument('--report', required=True)
    p.add_argument('--locks', required=True)
    p.add_argument('--out-dir')
    p.add_argument('--accepted')
    a = p.parse_args()
    locks = pathlib.Path(a.locks)
    try:
        report = json.loads(pathlib.Path(a.report).read_text(encoding='utf-8'))
        elock = json.loads((locks / 'eligibility.lock.json').read_text(encoding='utf-8'))
        glock = json.loads((locks / 'graphs.lock.json').read_text(encoding='utf-8'))
    except (OSError, ValueError) as e:
        print(f'REFUSED: unreadable report or locks: {e}')
        sys.exit(2)
    if a.mode == 'accept' and not a.accepted:
        print('REFUSED: --accepted is required')
        sys.exit(2)
    if a.mode == 'candidate' and (not a.out_dir or a.accepted):
        print('REFUSED: candidate mode writes only --out-dir and never an accepted file')
        sys.exit(2)
    if a.mode == 'accept':
        # R3: one full-PASS validator at the accepted-emission boundary, before any side effect.
        errs = handoff.validate_full_pass(report, locks, gate)
        if errs:
            print('REFUSED: report cannot back accepted emission:')
            for x in errs[:20]:
                print('  -', x)
            sys.exit(2)
    try:
        text = generate(report, elock, glock, a.mode == 'accept')
    except (Refused, KeyError, TypeError, ValueError) as e:
        print(f'REFUSED: {type(e).__name__}: {e}')
        sys.exit(2)
    data = text.encode('utf-8')
    got, want = hashlib.sha256(data).hexdigest(), glock.get('generated_rs_sha256')
    if a.mode == 'candidate':
        out = pathlib.Path(a.out_dir)
        out.mkdir(parents=True, exist_ok=True)
        (out / 'generated.rs.candidate').write_bytes(data)
        (out / 'candidate.json').write_text(json.dumps({'candidate_sha256': got, 'locked_sha256': want,
                                                        'matches_locked': got == want,
                                                        'status': 'CANDIDATE ONLY (not accepted output)'}, indent=1),
                                            encoding='utf-8')
        print(f'candidate {got} {"matches" if got == want else "DIFFERS FROM"} locked {want}')
        sys.exit(0 if got == want else 3)
    if got != want:
        print(f'DRIFT: emitted {got} != locked {want}; accepted output left untouched')
        sys.exit(3)
    dest = pathlib.Path(a.accepted)
    tmp = dest.with_name(dest.name + '.tmp-emit')
    tmp.write_bytes(data)
    if hashlib.sha256(tmp.read_bytes()).hexdigest() != want:
        tmp.unlink()
        print('DRIFT after write; accepted output left untouched')
        sys.exit(3)
    os.replace(tmp, dest)
    print(f'accepted {dest} {got}')


if __name__ == '__main__':
    main()
