#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# PROSPECTIVE private strict-gate validation of an unpublished candidate. This is
# not hosted CI and not a public gate activation.
#
# The candidate changes only the resistor record status/notes, the resistor README
# and the workflow flag relative to runtime 412b54a546, so the C++ oracle built
# and audited there is REUSED after identity checks: binary hash, live embedded
# provenance equal to its build record, every overlay file hash equal to the
# candidate's raw port/oracle bytes, and the candidate's 915 pinned reference
# hashes equal to the manifest that binary embeds. Its embedded overlay revision
# stays 412b54a546 and is not relabelled. The Rust runner and shims are rebuilt
# (incrementally, at most eight jobs) from sources identical to the tested
# runtimes, and each report binds the executable that ran. Never waits on the
# shared lock: a busy lock exits with status 3.
set -euo pipefail
rev=${1:?candidate revision}
win=/mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont
export_dir="$win/export-$rev"
work="$HOME/kicad-port/phase0-strict-candidate-$rev"
retained="$HOME/kicad-port/phase0-final-cont-412b54a546"   # own earlier root, read only
oracle_sha=945b7b606a09ca36ff516a4e6d25955d42f7dc5c5fdb714cd6f2e035e931cf0e
oracle_rev=412b54a54637b44966cb8babc4a7bd4fc6a4c4c1
lock="$HOME/kicad-port.build-lock"
export CARGO_BUILD_JOBS=8 PYTHONDONTWRITEBYTECODE=1
source "$HOME/.cargo/env"
mkdir -p "$work"
cd "$work"
stamp() { echo "== $(date -Is) $*"; }
done_step() { touch "$work/done-$1"; }
have() { [[ -f "$work/done-$1" ]]; }

if ! mkdir "$lock" 2>/dev/null; then stamp shared lock busy; exit 3; fi
trap 'rmdir "$lock"' EXIT
stamp lock acquired
free -g | head -2

if ! have prep; then
    stamp prep: candidate raw LF tree and retained oracle identity
    rm -rf "$work/repo" "$work/oracle"
    mkdir "$work/repo" "$work/oracle"
    tar -xf "$export_dir/repo.tar" -C "$work/repo"
    cp -a --reflink=auto "$retained/repo/.port-deps" "$work/repo/.port-deps"
    cp -a --reflink=auto "$retained/repo/rust/target" "$work/repo/rust/target"
    cp "$retained/oracle-build/kicad-oracle" "$work/oracle/kicad-oracle"
    printf '%s  %s\n' "$oracle_sha" "$work/oracle/kicad-oracle" | sha256sum -c -
    "$work/oracle/kicad-oracle" --provenance > "$work/oracle-provenance.json"
    cmp "$work/oracle-provenance.json" "$retained/oracle-build/oracle-provenance.json"
    python3 - "$work" "$export_dir" "$retained" "$oracle_rev" <<'PY'
import hashlib, json, pathlib, sys
work, export, retained = (pathlib.Path(arg) for arg in sys.argv[1:4])
oracle_rev = sys.argv[4]
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
h = json.loads((export / 'head-identity.json').read_text())
assert sha(export / 'repo.tar') == h['archive_sha256'] and not h['missing']
assert h['bytes_differ_from_blob'] == ['thirdparty/argparse/tools/build.bat'], h['bytes_differ_from_blob']
pin = json.loads((export / 'pin-identity.json').read_text())
assert pin['manifest_files'] == 915 and not pin['differ_from_blob'] and not pin['not_exported'] and not pin['extra_exported']
p = json.loads((work / 'oracle-provenance.json').read_text())
assert p['overlay_revision'] == oracle_rev and p['overlay_dirty_at_export'] is False
assert p['floating_point'] == {'contract': 'off', 'fast_math': False} and p['reference_file_count'] == 915
assert len(p['compile_commands']) == 75
overlay = work / 'repo/port/oracle'
mismatch = [name for name, digest in p['overlay_files_sha256'].items() if sha(overlay / name) != digest]
present = sorted(str(f.relative_to(overlay)) for f in overlay.rglob('*') if f.is_file() and '__pycache__' not in f.parts)
assert not mismatch, mismatch
assert present == sorted(p['overlay_files_sha256']), sorted(set(present) ^ set(p['overlay_files_sha256']))
built_manifest = retained / 'oracle-src/reference/source-manifest.json'
assert sha(built_manifest) == p['source_manifest_sha256']
candidate_files = json.loads((export / 'stage/source-manifest.json').read_text(encoding='utf-8'))['files']
built_files = json.loads(built_manifest.read_text(encoding='utf-8'))['files']
assert candidate_files == built_files, 'candidate pinned reference hashes differ from the oracle build'
report = {'oracle_sha256': sha(work / 'oracle/kicad-oracle'), 'oracle_overlay_revision': p['overlay_revision'],
          'overlay_files_checked': len(p['overlay_files_sha256']), 'overlay_files_equal_candidate': True,
          'reference_files_equal_candidate': len(candidate_files), 'source_manifest_sha256': p['source_manifest_sha256'],
          'candidate_revision': h['head'], 'candidate_archive_sha256': h['archive_sha256']}
(work / 'reuse-identity.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
PY
    done_step prep
fi

cd "$work/repo"
oracle="$work/oracle/kicad-oracle"
runner="$work/repo/rust/target/debug/kicad-difftest"
if ! have policy; then
    stamp policy tests, source validator and candidate statuses
    rustc --version; gcc --version | head -1
    python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
    python3 port/tools/validate_units.py --json > "$work/source-validation.json"
    cat "$work/source-validation.json"
    python3 - <<'PY'
import pathlib, tomllib
units = {p.stem: tomllib.loads(p.read_text(encoding='utf-8')) for p in sorted(pathlib.Path('port/units').glob('*.toml'))}
green = sorted(u for u, v in units.items() if v['status'] == 'green')
print('records', len(units), 'green', green, 'layers', [units[u]['layer'] for u in green])
assert green == ['libs.kimath.src.md5_hash.application.pure.p001', 'pcb_calculator.application.pure.p001',
                 'pcb_calculator.calculator_panels.application.pure.p001'], green
assert units['pcb_calculator.application.thin.replacement.p001']['status'] == 'todo'
flag = [l for l in pathlib.Path('.github/workflows/rust-port.yml').read_text().splitlines() if 'PHASE0_REQUIRED:' in l]
print('workflow', flag); assert flag == ['  PHASE0_REQUIRED: "true"']
PY
    done_step policy
fi
if ! have strict; then
    stamp prospective strict live check with --require-phase0, default per-entry budget
    cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -j8
    rm -rf "$work/reports"
    start=$(date +%s)
    python3 port/tools/check_goldens_policy.py --ci --require-phase0 --runner "$runner" --oracle "$oracle" \
        --reports "$work/reports"
    echo "strict check seconds: $(( $(date +%s) - start ))"
    sha256sum "$oracle" "$runner" rust/target/debug/kicad-md5-shim rust/target/debug/kicad-iec60664-shim \
        rust/target/debug/kicad-resistors-shim | tee "$work/binaries.sha256"
    done_step strict
fi
stamp summary
python3 - "$work" <<'PY'
import hashlib, json, pathlib, sys
work = pathlib.Path(sys.argv[1]); repo = work / 'repo'
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
binaries = {pathlib.Path(path).name: digest for digest, path in
            (line.split() for line in (work / 'binaries.sha256').read_text().splitlines())}
expected = {'libs.kimath.src.md5_hash.application.pure.p001': ('kicad-md5-shim', 5973, '9b8b9a76', '709084cd'),
            'pcb_calculator.calculator_panels.application.pure.p001': ('kicad-iec60664-shim', 14190, '3051d7e1', '819f74f4'),
            'pcb_calculator.application.pure.p001': ('kicad-resistors-shim', 1050, '8d1cc9e8', '5fb5dfb6')}
units = {}
for unit, (shim, cases, oracle_out, rust_out) in expected.items():
    r = json.loads((work / f'reports/{unit}.json').read_text())
    assert r['status'] == 'green' and r['cases'] == cases and r['deferred'] == 0 and not r.get('failures'), (unit, r)
    assert r['oracle_sha256'] == binaries['kicad-oracle'] and r['rust_sha256'] == binaries[shim], unit
    assert r['oracle_output_sha256'].startswith(oracle_out) and r['rust_output_sha256'].startswith(rust_out), unit
    assert sha(repo / f'port/goldens/{unit}.jsonl') == r['oracle_output_sha256'], unit
    units[unit] = {k: r.get(k) for k in ('status', 'cases', 'deferred', 'failures', 'oracle_sha256', 'rust_sha256',
                                         'oracle_output_sha256', 'rust_output_sha256')}
phase0 = json.loads((work / 'reports/phase0.json').read_text())
assert phase0['phase0'] == 'COMPLETE' and phase0['strict_gate_enabled'] is True and phase0['live_green_units'] == 3
summary = {'kind': 'PROSPECTIVE private strict-gate validation; not hosted CI, not public activation',
           'reuse': json.loads((work / 'reuse-identity.json').read_text()), 'units': units,
           'total_cases': sum(u['cases'] for u in units.values()),
           'phase0': {k: phase0[k] for k in phase0 if k != 'extraction'},
           'extraction_coverage': {k: (len(v) if isinstance(v, list) else v) for k, v in phase0['extraction'].items()
                                   if k in ('inventory_complete', 'parse_complete', 'compile_database_complete',
                                            'all_configurations_complete', 'gate_complete', 'unresolved_call_count',
                                            'parse_failures', 'uncompiled_application_sources')},
           'source_validation': json.loads((work / 'source-validation.json').read_text())}
(work / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
PY
done_step all
stamp PROSPECTIVE_STRICT_CANDIDATE_PROOF_COMPLETE
