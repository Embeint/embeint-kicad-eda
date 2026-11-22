#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Reserved serde key envelope regressions: clean LF export of the runtime commit,
# fresh oracle build, both actual adapters and the registered live check.
set -euo pipefail
runtime=65d5b0a9732551db35a9348d69d7b2ccfa86a1fb
work="$HOME/kicad-port/md5-envelope-regressions"
previous="$HOME/kicad-port/md5-corrected-graph"
exports=/mnt/c/Users/KarlVonRichter/.t3/md5-envelope-export
stage_source=/mnt/c/Users/KarlVonRichter/.t3/md5-envelope-stage
lock="$HOME/kicad-port.build-lock"
evidence=rust/kicad-md5-shim/validation/2026-11-22-a5ed3948

echo 'Waiting for actual shared lock before private preparation'
until mkdir "$lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$lock"' EXIT
mkdir -p "$work"
test ! -e "$work/repo"
mkdir "$work/repo"
tar -xf "$exports/repo.tar" -C "$work/repo"
# Read-only reuse of the prior private root: the solidcore path dependency and Cargo cache.
cp -a --reflink=auto "$previous/repo/.port-deps" "$work/repo/.port-deps"
cp -a --reflink=auto "$previous/repo/rust/target" "$work/repo/rust/target"
mkdir -p "$work/stage"
cp "$stage_source/"* "$work/stage/"
sha256sum "$exports/repo.tar" "$work/stage/"* | tee "$work/export-sha256.txt"
python3 - "$work" "$runtime" <<'PY'
import hashlib, json, pathlib, sys
work, runtime = pathlib.Path(sys.argv[1]), sys.argv[2]
record = json.loads((work / 'stage/source-manifest.json').read_text())
assert record['overlay_revision'] == runtime and record['overlay_dirty_at_export'] is False
assert record['archive_sha256'] == '941b5a743e033a0d2743d3d0df58c7ef48fbb47f2b10f1050ba55aa9f941ed96'
repo = work / 'repo'
sha = lambda p: hashlib.sha256((repo / p).read_bytes()).hexdigest()
unit = 'libs.kimath.src.md5_hash.application.pure.p001'
assert sha(f'port/cases/{unit}.jsonl') == '0d2232c3b758cb67acdc486cbe420350394502fdf2df591e356280468df00795'
assert sha(f'port/goldens/{unit}.jsonl') == '9b8b9a761911d19c39e45136b28d17eb47ef50dc582102b4b4e538ba909034cb'
for path in ['port/oracle/test_md5_inputs.py', 'rust/kicad-md5-shim/src/main.rs',
             'rust/kicad-difftest/src/request.rs', f'port/cases/{unit}.jsonl',
             f'port/goldens/{unit}.jsonl']:
    assert b'\r' not in (repo / path).read_bytes(), path
print('Clean LF export of runtime', runtime, 'with unchanged MD5 corpus and golden bytes')
PY
rmdir "$lock"
trap - EXIT

source "$HOME/.cargo/env"
export CARGO_BUILD_JOBS=8 ORACLE_JOBS=8
cd "$work/repo"
while true; do
    if bash port/oracle/build.sh "$work/stage" "$work/repo/port/oracle" "$work"; then
        break
    else
        result=$?
        if [[ "$result" != 3 ]]; then exit "$result"; fi
    fi
    sleep 10
done
until mkdir "$lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$lock"' EXIT
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim -p kicad-difftest --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim -p kicad-difftest -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-md5 --target wasm32-unknown-unknown -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-md5-shim -p kicad-difftest -j8
mkdir -p "$work/runners"
cp rust/target/debug/kicad-md5-shim "$work/runners/kicad-md5-shim"
cp rust/target/debug/kicad-difftest "$work/runners/kicad-difftest"
cp "$work/oracle-build/kicad-oracle" "$work/runners/kicad-oracle"
"$work/runners/kicad-oracle" --provenance > "$work/oracle-provenance.json"
python3 port/oracle/test_md5_inputs.py "$work/runners/kicad-oracle" "$work/runners/kicad-md5-shim"
python3 rust/kicad-md5-shim/generate_cases.py --output "$work/regenerated.jsonl"
cmp "$work/regenerated.jsonl" port/cases/libs.kimath.src.md5_hash.application.pure.p001.jsonl
python3 port/tools/check_goldens_policy.py --ci \
    --runner "$work/runners/kicad-difftest" --oracle "$work/runners/kicad-oracle" \
    --reports "$work/reports"
python3 - "$work" "$evidence" <<'PY'
import hashlib, importlib.util, json, pathlib, subprocess, sys
work, evidence = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
spec = importlib.util.spec_from_file_location('md5_inputs', 'port/oracle/test_md5_inputs.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
checks = module.regression_cases()
assert len(checks) == 199 and sum('error' in v for _, v in checks) == 194
raw = ''.join(text + '\n' for text, _ in checks).encode()
expected = ''.join(json.dumps(v, separators=(',', ':')) + '\n' for _, v in checks).encode()
assert raw.startswith((evidence / 'raw-regression-requests.jsonl').read_bytes())
assert expected.startswith((evidence / 'regression-expected.jsonl').read_bytes())
(work / 'raw-regression-requests.jsonl').write_bytes(raw)
(work / 'regression-expected.jsonl').write_bytes(expected)
# Retain each actual adapter's complete stdout for the full fixture stream.
outputs = {}
for name in ['kicad-oracle', 'kicad-md5-shim']:
    process = subprocess.run([str(work / 'runners' / name)], input=raw, capture_output=True,
                             check=True, timeout=60)
    assert not process.stderr
    (work / f'{name}-regressions.stdout.jsonl').write_bytes(process.stdout)
    replies = [json.loads(line) for line in process.stdout.split(b'\n') if line]
    assert replies == [v for _, v in checks], name
    outputs[name] = hashlib.sha256(process.stdout).hexdigest()
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
identity = {name: sha(work / 'runners' / name) for name in ['kicad-oracle', 'kicad-md5-shim', 'kicad-difftest']}
identity.update({'raw_regression_requests_sha256': hashlib.sha256(raw).hexdigest(),
                 'regression_expected_sha256': hashlib.sha256(expected).hexdigest(),
                 'adapter_stdout_sha256': outputs,
                 'oracle_provenance_sha256': sha(work / 'oracle-provenance.json')})
(work / 'identities.json').write_text(json.dumps(identity, indent=2, sort_keys=True) + '\n')
print(json.dumps(identity, indent=2, sort_keys=True))
print('Reserved key integration complete: 194 complete refusals and 5 complete successes per adapter, full registered live check')
PY
touch "$work/completed"
