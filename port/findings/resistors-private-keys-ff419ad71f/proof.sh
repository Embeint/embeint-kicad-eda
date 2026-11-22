#!/usr/bin/env bash
# Clean proof of PR #44 private-marker-key adoption at an exact committed runtime SHA.
set -euo pipefail
sha=${1:?runtime sha}
tarball=${2:?clean LF git archive of sha}
stage=${3:?oracle stage prepared at sha}
root="$HOME/kicad-port/pilot-resistors-private-keys"
work="$root/proof-${sha:0:10}${4:+-$4}"
out="$work/evidence"
export PATH="$HOME/.cargo/bin:$PATH"
export RUST_TEST_THREADS=8
if [[ -e "$work" ]]; then echo "refusing existing $work" >&2; exit 2; fi
mkdir -p "$work/src" "$out"
tar -xf "$tarball" -C "$work/src"
sha256sum "$tarball" "$stage/reference.tar" "$stage/source-manifest.json" > "$out/inputs.sha256"
cp -a "$root/iter2/src/rust/target" "$work/src/rust/target"
while true; do
    set +e
    ORACLE_JOBS=8 bash "$work/src/port/oracle/build.sh" "$stage" "$work/src/port/oracle" "$work"
    status=$?
    set -e
    if [[ $status == 0 ]]; then break; fi
    if [[ $status != 3 ]]; then exit "$status"; fi
    sleep 30
done
lock="$HOME/kicad-port.build-lock"
until mkdir "$lock" 2>/dev/null; do sleep 30; done
trap 'rmdir "$lock"' EXIT
manifest="$work/src/rust/Cargo.toml"
oracle="$work/oracle-build/kicad-oracle"
debug="$work/src/rust/target/debug/kicad-resistors-shim"
release="$work/src/rust/target/release/kicad-resistors-shim"
echo '== fmt/clippy/tests'
cargo fmt --manifest-path "$manifest" --all --check
cargo clippy --locked --manifest-path "$manifest" --workspace --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path "$manifest" --workspace -j8
cargo test --locked --manifest-path "$manifest" -p kicad-resistors-shim -p kicad-difftest --release -j8
echo '== source validator and portable wasm builds'
python3 "$work/src/port/tools/validate_units.py" --root "$work/src" --json > "$out/source-validation.json"
python3 "$work/src/port/tools/check_goldens_policy.py" --root "$work/src" --wasm
echo '== native builds'
cargo build --locked --manifest-path "$manifest" -p kicad-resistors-shim -p kicad-difftest -j8
cargo build --locked --manifest-path "$manifest" -p kicad-resistors-shim -p kicad-difftest --release -j8
echo '== complete request guards'
python3 "$work/src/port/oracle/test_resistor_private_keys.py" --byte-identical "$oracle"
python3 "$work/src/port/oracle/test_resistor_private_keys.py" "$debug" "$release"
prior="$HOME/kicad-port/pilot-resistors/src/rust/target"
for old in "$prior/debug/kicad-resistors-shim" "$prior/release/kicad-resistors-shim"; do
    # Negative control: the retained pre-#48 shims (read only) must fail the new guard.
    sha256sum "$old" >> "$out/negative-control.txt"
    if python3 "$work/src/port/oracle/test_resistor_private_keys.py" "$old" >> "$out/negative-control.txt" 2>&1; then
        echo "pre-fix shim unexpectedly passed: $old" >&2; exit 1
    fi
    echo "expected failure: $old" >> "$out/negative-control.txt"
done
python3 "$work/src/port/oracle/test_resistor_collector_inputs.py" "$oracle" "$debug" "$release"
python3 "$work/src/rust/kicad-resistors-shim/tests/requests.py" --oracle "$oracle" --rust "$debug" --rust "$release"
requests="$work/src/port/oracle/fixtures/resistor-private-keys.requests.jsonl"
"$oracle" < "$requests" > "$out/private-keys-cpp.jsonl"
"$debug" < "$requests" > "$out/private-keys-rust-debug.jsonl"
"$release" < "$requests" > "$out/private-keys-rust-release.jsonl"
cmp "$out/private-keys-cpp.jsonl" "$work/src/port/oracle/fixtures/resistor-private-keys.replies.jsonl"
cmp "$out/private-keys-rust-debug.jsonl" "$out/private-keys-rust-release.jsonl"
echo '== registered live 1050 checks'
python3 "$work/src/port/tools/check_goldens_policy.py" --root "$work/src" --ci \
    --runner "$work/src/rust/target/debug/kicad-difftest" --oracle "$oracle" --reports "$out/registered-reports"
"$work/src/rust/target/release/kicad-difftest" compare --unit pcb_calculator.application.pure.p001 \
    --oracle "$oracle" --rust "$release" --root "$work/src" --report "$out/compare-release.json"
"$oracle" --provenance > "$out/oracle-provenance.json"
cp "$work/oracle-src/reference/source-manifest.json" "$out/source-manifest.json"
cp "$work/oracle-build/binary.sha256" "$out/binary.sha256"
python3 - "$work" "$sha" <<'PY'
from pathlib import Path
import hashlib, json, subprocess, sys
work = Path(sys.argv[1]); sha = sys.argv[2]; root = work / 'src'; out = work / 'evidence'
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
provenance = json.loads((out / 'oracle-provenance.json').read_text())
assert provenance['overlay_revision'] == sha, provenance['overlay_revision']
assert provenance['overlay_dirty_at_export'] is False
assert provenance['reference_file_count'] == 913
assert provenance == json.loads((work / 'oracle-build/oracle-provenance.json').read_text())
unit = 'pcb_calculator.application.pure.p001'
report = json.loads((out / f'registered-reports/{unit}.json').read_text())
assert report['cases'] == 1050 and report['deferred'] == 0 and report['status'] == 'green', report
assert report['oracle_sha256'] == digest(work / 'oracle-build/kicad-oracle')
assert report['rust_sha256'] == digest(root / 'rust/target/debug/kicad-resistors-shim')
release = json.loads((out / 'compare-release.json').read_text())
assert release['status'] == 'green' and release['cases'] == 1050 and release['deferred'] == 0, release
fixtures = root / 'port/oracle/fixtures'
result = {
    'runtime_source': sha,
    'rust_dependency': '1a2fb924d539d63a68f5696191907dc2aa6d6370',
    'pr48_runtime_fix': '50e3f24874ea430770d59e10aa3db095dec4f757',
    'prior_pr44_head': '084115f06cb1cb73fe6bc04f90f715d3384cfb3e',
    'corrected_dependency': '19035296e129cf8640e89331bc9e8ad84f0fe38c',
    'collector_fix': '1dd31a7493189ca694bc09020a3a84bf9fb6082e',
    'case_count': report['cases'],
    'cases_sha256': digest(root / f'port/cases/{unit}.jsonl'),
    'golden_sha256': digest(root / f'port/goldens/{unit}.jsonl'),
    'private_key_requests_sha256': digest(fixtures / 'resistor-private-keys.requests.jsonl'),
    'private_key_replies_sha256': digest(fixtures / 'resistor-private-keys.replies.jsonl'),
    'private_key_rust_reply_sha256': digest(out / 'private-keys-rust-debug.jsonl'),
    'oracle_sha256': report['oracle_sha256'],
    'rust_debug_sha256': report['rust_sha256'],
    'rust_release_sha256': digest(root / 'rust/target/release/kicad-resistors-shim'),
    'difftest_debug_sha256': digest(root / 'rust/target/debug/kicad-difftest'),
    'difftest_release_sha256': digest(root / 'rust/target/release/kicad-difftest'),
    'release_compare': {key: release[key] for key in ('status', 'cases', 'deferred', 'oracle_output_sha256', 'rust_output_sha256', 'rust_sha256')},
    'registered_outputs': {key: report.get(key) for key in ('oracle_output_sha256', 'rust_output_sha256')},
    'oracle_provenance_sha256': digest(out / 'oracle-provenance.json'),
    'source_manifest_sha256': digest(out / 'source-manifest.json'),
    'compilers': provenance.get('compilers'),
    'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
    'status': 'claimed; draft; final #39 graph acceptance and independent review pending',
    'compiler_qualification': 'GCC 13/15 Release x86-64; GCC O0 and Clang 18 NaN payload equivalence not claimed',
    'phase0_required': False,
    'goldens_reblessed': False,
}
(out / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
PY
echo "private-key-proof-complete $sha"
