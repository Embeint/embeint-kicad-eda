#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""One-time, explicitly requested resistor golden migration, after live equality."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

UNIT = "pcb_calculator.application.pure.p001"
CASES_SHA256 = "58dd1186c9f10c8efb54bc90102c71508c8e852c0b8c08d437380b112fe3c597"
OLD_SHA256 = "8d1cc9e8c280b349294f9d512e450586c892a3170881afbba1c1b78de1a107a2"
# Exact reviewed output bytes, independently reconstructed from the original
# golden plus the genuine 33535308 SSE2 oracle's thirteen changed replies.
# A matching live run is still required; this never supplies oracle results.
NEW_SHA256 = "db61789166977ceab4ce628c252c6ed35771cd7a5bb84d0ad7ac0a3ae08e2e9e"
IMAGE_LOCK_SHA256 = "c4d1c9c0aca71985f07bd36ff759301daa88a43e4a17335d31e4446bbd6101d2"
CHANGED = {*(f"series-{i}" for i in range(9)), "series--1",
           "series--2147483648", "series-2147483647", "format-boundaries-3"}


def differences(old, new):
    if type(old) is not type(new):
        raise ValueError("migration changes a JSON value type")
    if isinstance(old, dict):
        if old.keys() != new.keys():
            raise ValueError("migration changes object fields")
        return [item for key in old for item in differences(old[key], new[key])]
    if isinstance(old, list):
        if len(old) != len(new):
            raise ValueError("migration changes array lengths")
        return [item for a, b in zip(old, new) for item in differences(a, b)]
    return [] if old == new else [(old, new)]


def check_replacement(proposal, old_text):
    if hashlib.sha256(old_text.encode('utf-8')).hexdigest() != OLD_SHA256:
        raise ValueError('actual original golden does not match the reviewed baseline')
    if not old_text.endswith('\n') or '\r' in old_text:
        raise ValueError('original golden must be canonical LF JSONL')
    lines = old_text.splitlines()
    seen = set()
    for change in proposal['changes']:
        index = change['old_index']
        if (type(index) is not int or type(change['new_index']) is not int
                or index != change['new_index'] or index < 0 or index >= len(lines)
                or index in seen or lines[index] != change['old']
                or not isinstance(change['new'], str) or '\n' in change['new']
                or '\r' in change['new']):
            raise ValueError('replacement does not match actual original lines and positions')
        seen.add(index)
        lines[index] = change['new']
    digest = hashlib.sha256(('\n'.join(lines)+'\n').encode('utf-8')).hexdigest()
    if digest != NEW_SHA256 or proposal['new_sha256'] != digest:
        raise ValueError('replacement paths or values differ from the exact authorized output')


def approve(proposal, runtime, runtime_sha, image_identity_sha, old_text):
    if (proposal["unit"] != UNIT or proposal["cases_sha256"] != CASES_SHA256
            or proposal["old_sha256"] != OLD_SHA256):
        raise ValueError("unexpected unit or changed corpus")
    changes = proposal["changes"]
    if len(changes) != 13 or {x["case"] for x in changes} != CHANGED:
        raise ValueError("migration must change exactly the thirteen reviewed cases")
    if any(type(x["old_index"]) is not int or type(x["new_index"]) is not int
           or x["old_index"] != x["new_index"]
           or not isinstance(x["old"], str) or not isinstance(x["new"], str) for x in changes):
        raise ValueError("migration must preserve all case identities and positions")
    delta = [item for x in changes for item in differences(json.loads(x["old"]), json.loads(x["new"]))]
    if (sum(type(a) is int for a, _ in delta), sum(type(a) is str for a, _ in delta), len(delta)) != (317, 3, 320):
        raise ValueError("unexpected field changes outside the reviewed migration")
    if runtime["oracle_sha256"] != proposal["oracle"]["oracle_sha256"]:
        raise ValueError("runtime preflight belongs to a different oracle")
    check_replacement(proposal, old_text)
    proposal["reason"] = (
        "User-authorized resistor reference migration to pinned glibc 2.39 SSE2. "
        "Runtime pow(10,126) is 0x5a17a2ecc414a040 rather than the historical "
        "FMA/table value 0x5a17a2ecc414a03f. Exactly thirteen existing cases "
        "change (317 numeric bit fields and three engineering-format strings); "
        "cases, comparison and all historical evidence are unchanged. "
        f"Native measurement image lock SHA-256: {IMAGE_LOCK_SHA256}; "
        f"installed image identity SHA-256: {image_identity_sha}. "
        "Historical failing reference run: 45323785738. "
        f"Fresh matching-oracle runtime preflight SHA-256: {runtime_sha}. "
        "See port/findings/resistors-sse2-reference-migration.md."
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--golden-root", type=Path,
                        help="separate cases/goldens copy; leaves sealed source unchanged")
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root, oracle, out = args.root.resolve(), args.oracle.resolve(strict=True), args.out.resolve()
    golden_root = args.golden_root.resolve() if args.golden_root else root
    out.mkdir(parents=True, exist_ok=True)
    # Golden generation is confined to the reviewed measurement image. The
    # ordinary host-CI preflight intentionally has a narrower runtime scope.
    image_lock = Path('/opt/refenv/inputs.lock.json').read_bytes()
    if hashlib.sha256(image_lock).hexdigest() != IMAGE_LOCK_SHA256:
        raise ValueError('golden generation requires the selected measurement-image lock')
    image_identity = Path('/opt/refenv/image-identity.json').read_bytes()
    (out/'image-identity.json').write_bytes(image_identity)
    subprocess.run([sys.executable, str(root/'port/oracle/check_resistor_reference.py'),
                    '--oracle', str(oracle), '--report', str(out/'runtime.json')], check=True)
    provenance = subprocess.check_output([str(oracle), '--provenance'], timeout=30)
    (out/'oracle-provenance.json').write_bytes(provenance)
    runner, shim = [root/'rust/target/debug'/name for name in ('kicad-difftest', 'kicad-resistors-shim')]
    common = ['--root', str(golden_root), '--unit', UNIT, '--oracle', str(oracle), '--rust', str(shim)]
    blessing = ['--provenance', str(out/'oracle-provenance.json'), '--approval', str(out/'approval.json')]
    subprocess.run([str(runner), 'propose', *common, *blessing, '--report', str(out/'propose.json')], check=True)
    approval = json.loads((out/'approval.json').read_bytes())
    runtime_bytes = (out/'runtime.json').read_bytes()
    old_text = (golden_root/'port/goldens'/f'{UNIT}.jsonl').read_bytes().decode('utf-8')
    approve(approval, json.loads(runtime_bytes), hashlib.sha256(runtime_bytes).hexdigest(),
            hashlib.sha256(image_identity).hexdigest(), old_text)
    (out/'approval.json').write_text(json.dumps(approval, indent=2)+'\n', encoding='utf-8')
    evidence = golden_root/'port/goldens/evidence'/f'{UNIT}-glibc239-sse2-v1.json'
    evidence.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(runner), 'bless', *common, *blessing, '--evidence', str(evidence),
                    '--report', str(out/'bless.json')], check=True)
    subprocess.run([str(runner), 'check', *common, '--report', str(out/'check.json')], check=True)


if __name__ == '__main__':
    main()
