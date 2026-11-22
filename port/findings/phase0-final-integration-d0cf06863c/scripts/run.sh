#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Phase zero integration proof for one exported revision: clean raw LF tree,
# fresh audited oracle with all three families, Rust workspace, wasm, policy,
# full source validator, complete envelopes on both actual adapters and the
# registered live check. Resumable: completed steps leave done-* markers.
set -euo pipefail
rev=${1:?revision}
win=/mnt/c/Users/KarlVonRichter/.t3/phase0-final
export_dir="$win/export-$rev"
work="$HOME/kicad-port/phase0-final-$rev"
seed="$HOME/kicad-port/md5-corrected-graph/repo"   # read only: solidcore checkout, Cargo cache
lock="$HOME/kicad-port.build-lock"
export CARGO_BUILD_JOBS=8 ORACLE_JOBS=8 PYTHONDONTWRITEBYTECODE=1
source "$HOME/.cargo/env"
mkdir -p "$work"
cd "$work"
stamp() { echo "== $(date -Is) $*"; }
done_step() { touch "$work/done-$1"; }
have() { [[ -f "$work/done-$1" ]]; }

if ! have prep; then
    stamp prep
    rm -rf "$work/repo" "$work/stage"
    mkdir "$work/repo" "$work/stage"
    python3 - "$export_dir" <<'PY'
import hashlib, json, pathlib, sys
d = pathlib.Path(sys.argv[1])
h = json.loads((d / 'head-identity.json').read_text())
assert hashlib.sha256((d / 'repo.tar').read_bytes()).hexdigest() == h['archive_sha256']
assert not h['missing'], h['missing']
p = json.loads((d / 'pin-identity.json').read_text())
m = d / 'stage/source-manifest.json'
assert hashlib.sha256(m.read_bytes()).hexdigest() == p['manifest_sha256']
assert not p['differ_from_blob'] and not p['not_exported'] and not p['extra_exported'], p
print('export identity: head', h['head'], 'files', h['exported_files'],
      'blob-differing', len(h['bytes_differ_from_blob']), 'pinned', p['manifest_files'])
PY
    tar -xf "$export_dir/repo.tar" -C "$work/repo"
    cp "$export_dir/stage/"* "$work/stage/"
    cp -a --reflink=auto "$seed/.port-deps" "$work/repo/.port-deps"
    test "$(git -C "$work/repo/.port-deps/solidcore" rev-parse HEAD)" = \
        "$(python3 -c 'import tomllib;print(tomllib.load(open("'"$work"'/repo/port/pins.toml","rb"))["solidcore"]["revision"])')"
    test -z "$(git -C "$work/repo/.port-deps/solidcore" status --porcelain)"
    cp -a --reflink=auto "$seed/rust/target" "$work/repo/rust/target"
    done_step prep
fi

cd "$work/repo"
if ! have oracle; then
    stamp oracle build, audit and ctest
    while true; do
        result=0
        bash port/oracle/build.sh "$work/stage" "$work/repo/port/oracle" "$work" || result=$?
        if [[ "$result" == 0 ]]; then break; fi
        if [[ "$result" != 3 ]]; then exit "$result"; fi
        sleep 30
    done
    done_step oracle
fi

stamp waiting for the shared lock
until mkdir "$lock" 2>/dev/null; do sleep 30; done
trap 'rmdir "$lock"' EXIT
stamp lock acquired
oracle="$work/oracle-build/kicad-oracle"
runner="$work/repo/rust/target/debug/kicad-difftest"
manifest=rust/Cargo.toml

if ! have rust; then
    stamp rust workspace as CI
    rustc --version; cargo --version; gcc --version | head -1; node --version
    cargo fmt --manifest-path $manifest --all --check
    cargo clippy --locked --manifest-path $manifest --workspace --all-targets -j8 -- -D warnings
    cargo test --locked --manifest-path $manifest --workspace -j8
    cargo build --locked --manifest-path $manifest -p kicad-difftest -j8
    done_step rust
fi
if ! have wasm; then
    stamp portable wasm builds
    python3 port/tools/check_goldens_policy.py --wasm
    cargo build --locked --manifest-path $manifest -p kicad-resistors --example nonfinite_parity \
        --target wasm32-unknown-unknown --release -j8
    node rust/kicad-resistors/wasm-test.mjs rust/target/wasm32-unknown-unknown/release/examples/nonfinite_parity.wasm
    done_step wasm
fi
if ! have policy; then
    stamp policy tests and full source validator
    python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
    python3 port/tools/validate_units.py --json > "$work/source-validation.json"
    cat "$work/source-validation.json"
    python3 port/tools/validate_units.py
    done_step policy
fi
if ! have corpus; then
    stamp corpus regeneration
    python3 rust/kicad-md5-shim/generate_cases.py --output "$work/md5-regenerated.jsonl"
    cmp "$work/md5-regenerated.jsonl" port/cases/libs.kimath.src.md5_hash.application.pure.p001.jsonl
    # rust/kicad-iec60664/cases.py reads the pin with git show; this export is not a
    # Git checkout, so check_iec_corpus.py runs it from Windows Git against the blob.
    python3 rust/kicad-resistors/cases.py --check
    (cd rust/kicad-resistors && python3 -m unittest test_cases -v)
    done_step corpus
fi
"$oracle" --provenance > "$work/oracle-provenance.json"
if ! have envelopes; then
    stamp complete envelopes on both actual adapters, selected package builds
    mkdir -p "$work/adapters"
    cargo build --locked --manifest-path $manifest -j8 -p kicad-md5-shim --bin kicad-md5-shim
    cp rust/target/debug/kicad-md5-shim "$work/adapters/"
    python3 port/oracle/test_md5_inputs.py "$oracle" "$work/adapters/kicad-md5-shim"
    cargo build --locked --manifest-path $manifest -j8 -p kicad-iec60664-shim --bin kicad-iec60664-shim
    cp rust/target/debug/kicad-iec60664-shim "$work/adapters/"
    python3 port/oracle/test_iec60664_inputs.py "$oracle" "$work/adapters/kicad-iec60664-shim"
    python3 rust/kicad-iec60664/test_shims.py "$oracle" "$work/adapters/kicad-iec60664-shim"
    cargo build --locked --manifest-path $manifest -j8 -p kicad-resistors-shim --bin kicad-resistors-shim
    cp rust/target/debug/kicad-resistors-shim "$work/adapters/"
    cargo build --locked --manifest-path $manifest -j8 --release -p kicad-resistors-shim --bin kicad-resistors-shim
    cp rust/target/release/kicad-resistors-shim "$work/adapters/kicad-resistors-shim-release"
    python3 port/oracle/test_resistor_collector_inputs.py "$oracle" "$work/adapters/kicad-resistors-shim" \
        "$work/adapters/kicad-resistors-shim-release"
    python3 rust/kicad-resistors-shim/tests/requests.py --oracle "$oracle" \
        --rust "$work/adapters/kicad-resistors-shim" --rust "$work/adapters/kicad-resistors-shim-release"
    sha256sum "$oracle" "$work/adapters/"* | tee "$work/adapters.sha256"
    done_step envelopes
fi
if ! have registered; then
    stamp registered live check, infrastructure mode
    rm -rf "$work/reports"
    start=$(date +%s)
    python3 port/tools/check_goldens_policy.py --ci --runner "$runner" --oracle "$oracle" \
        --reports "$work/reports"
    echo "registered check seconds: $(( $(date +%s) - start ))"
    sha256sum "$oracle" "$runner" rust/target/debug/kicad-md5-shim rust/target/debug/kicad-iec60664-shim \
        rust/target/debug/kicad-resistors-shim | tee "$work/registered-binaries.sha256"
    done_step registered
fi
stamp summary
python3 - "$work" "$rev" <<'PY'
import hashlib, json, pathlib, sys
work, rev = pathlib.Path(sys.argv[1]), sys.argv[2]
repo = work / 'repo'
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
p = json.loads((work / 'oracle-provenance.json').read_text())
built = json.loads((work / 'oracle-build/oracle-provenance.json').read_text())
assert p == built, 'live provenance differs from the build record'
assert p['overlay_revision'].startswith(rev) and p['overlay_dirty_at_export'] is False
assert p['floating_point'] == {'contract': 'off', 'fast_math': False}
assert sha(work / 'oracle-src/reference/source-manifest.json') == p['source_manifest_sha256']
binaries = dict(reversed(line.split()) for line in (work / 'registered-binaries.sha256').read_text().splitlines())
units = {}
for report in sorted((work / 'reports').glob('*.json')):
    if report.name == 'phase0.json':
        continue
    r = json.loads(report.read_text())
    unit = report.stem
    shim = {'libs.kimath.src.md5_hash.application.pure.p001': 'kicad-md5-shim',
            'pcb_calculator.calculator_panels.application.pure.p001': 'kicad-iec60664-shim',
            'pcb_calculator.application.pure.p001': 'kicad-resistors-shim'}[unit]
    assert r['rust_sha256'] == binaries[f'rust/target/debug/{shim}'], (unit, 'rust identity')
    assert r['oracle_sha256'] == binaries[str(work / 'oracle-build/kicad-oracle')], (unit, 'oracle identity')
    units[unit] = {k: r.get(k) for k in ('status', 'cases', 'deferred', 'failures', 'oracle_sha256',
                                         'rust_sha256', 'oracle_output_sha256', 'rust_output_sha256')}
    units[unit]['golden_sha256'] = sha(repo / f'port/goldens/{unit}.jsonl')
    units[unit]['cases_sha256'] = sha(repo / f'port/cases/{unit}.jsonl')
phase0 = json.loads((work / 'reports/phase0.json').read_text())
summary = {'tested_revision': p['overlay_revision'], 'reference_file_count': p['reference_file_count'],
           'compile_commands': len(p['compile_commands']), 'oracle_provenance_sha256': sha(work / 'oracle-provenance.json'),
           'source_manifest_sha256': p['source_manifest_sha256'], 'units': units,
           'total_cases': sum(u['cases'] or 0 for u in units.values()),
           'phase0': {k: phase0[k] for k in phase0 if k != 'extraction'},
           'source_validation': json.loads((work / 'source-validation.json').read_text()).get('valid')}
(work / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
PY
done_step all
stamp PHASE0_INTEGRATION_PROOF_COMPLETE
