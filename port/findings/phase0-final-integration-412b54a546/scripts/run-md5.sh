#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Affected-unit proof after merging the corrected #42 head into the integration:
# clean raw LF tree, fresh audited oracle with its eight default CTests, MD5 shim
# fmt/clippy/tests and native+wasm builds, the 203-reply default guard on both
# actual adapters, byte comparison of fresh stdout with #42's committed
# transcripts, #42's payload checksums, and the registered MD5 unit (5973 cases)
# at the default per-entry budget. Never waits on the shared lock: a busy lock
# exits with status 3 and the run resumes from its done-* markers.
set -euo pipefail
rev=${1:?revision}
win=/mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont
export_dir="$win/export-$rev"
work="$HOME/kicad-port/phase0-final-cont-$rev"
cache="$HOME/kicad-port/phase0-final-cont-7207de24ed/repo"   # own earlier root, read only
lock="$HOME/kicad-port.build-lock"
validation=rust/kicad-md5-shim/validation/2026-11-22-da17dc54
unit=libs.kimath.src.md5_hash.application.pure.p001
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
assert h['bytes_differ_from_blob'] == ['thirdparty/argparse/tools/build.bat'], h['bytes_differ_from_blob']
p = json.loads((d / 'pin-identity.json').read_text())
assert hashlib.sha256((d / 'stage/source-manifest.json').read_bytes()).hexdigest() == p['manifest_sha256']
assert p['manifest_files'] == 915
assert not p['differ_from_blob'] and not p['not_exported'] and not p['extra_exported'], p
print('export identity: head', h['head'], 'files', h['exported_files'], 'pinned', p['manifest_files'])
PY
    tar -xf "$export_dir/repo.tar" -C "$work/repo"
    cp "$export_dir/stage/"* "$work/stage/"
    cp -a --reflink=auto "$cache/.port-deps" "$work/repo/.port-deps"
    test "$(git -C "$work/repo/.port-deps/solidcore" rev-parse HEAD)" = \
        "$(python3 -c 'import tomllib;print(tomllib.load(open("'"$work"'/repo/port/pins.toml","rb"))["solidcore"]["revision"])')"
    test -z "$(git -C "$work/repo/.port-deps/solidcore" status --porcelain)"
    cp -a --reflink=auto "$cache/rust/target" "$work/repo/rust/target"
    done_step prep
fi

cd "$work/repo"
if ! have oracle; then
    stamp oracle build, audit and default ctest
    result=0
    bash port/oracle/build.sh "$work/stage" "$work/repo/port/oracle" "$work" || result=$?
    if [[ "$result" == 3 ]]; then stamp shared lock busy during oracle build; exit 3; fi
    if [[ "$result" != 0 ]]; then exit "$result"; fi
    done_step oracle
fi

if ! mkdir "$lock" 2>/dev/null; then stamp shared lock busy; exit 3; fi
trap 'rmdir "$lock"' EXIT
stamp lock acquired
oracle="$work/oracle-build/kicad-oracle"
manifest=rust/Cargo.toml

if ! have md5shim; then
    stamp md5 shim fmt, clippy, tests, native and wasm builds
    rustc --version; gcc --version | head -1
    cargo fmt --manifest-path $manifest --all --check
    cargo clippy --locked --manifest-path $manifest -p kicad-md5-shim --all-targets -j8 -- -D warnings
    cargo test --locked --manifest-path $manifest -p kicad-md5-shim -j8
    cargo build --locked --manifest-path $manifest -p kicad-md5-shim -p kicad-difftest -j8
    cargo build --locked --manifest-path $manifest -p kicad-md5 --target wasm32-unknown-unknown -j8
    mkdir -p "$work/adapters"
    cp rust/target/debug/kicad-md5-shim "$work/adapters/"
    done_step md5shim
fi
"$oracle" --provenance > "$work/oracle-provenance.json"
if ! have guard; then
    stamp default md5 guard on both actual adapters, transcripts and checksums
    python3 port/oracle/test_md5_inputs.py "$oracle" "$work/adapters/kicad-md5-shim"
    python3 - "$work" "$oracle" "$work/adapters/kicad-md5-shim" "$validation" <<'PY'
import hashlib, importlib.util, json, pathlib, subprocess, sys
work, oracle, shim, validation = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3], pathlib.Path(sys.argv[4])
spec = importlib.util.spec_from_file_location('md5_inputs', 'port/oracle/test_md5_inputs.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
checks = module.regression_cases()
raw = ''.join(t + '\n' for t, _ in checks).encode()
expected = ''.join(json.dumps(v, separators=(',', ':')) + '\n' for _, v in checks).encode()
assert raw == (validation / 'requests.jsonl').read_bytes(), 'requests differ from the committed transcript'
assert expected == (validation / 'expected.jsonl').read_bytes(), 'expected replies differ from the committed transcript'
report = {'checks': len(checks), 'refusals': sum('error' in v for _, v in checks),
          'successes': sum('result' in v for _, v in checks), 'adapters': {}}
for name, exe, transcript in [('oracle', oracle, 'oracle.stdout.jsonl'), ('rust', shim, 'rust.stdout.jsonl')]:
    p = subprocess.run([exe], input=raw, capture_output=True, check=True, timeout=60)
    assert not p.stderr, p.stderr
    replies = [json.loads(l) for l in p.stdout.split(b'\n') if l]
    assert replies == [v for _, v in checks], name
    committed = (validation / transcript).read_bytes()
    report['adapters'][name] = {
        'sha256': hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest(),
        'complete_replies': len(replies),
        'stdout_sha256': hashlib.sha256(p.stdout).hexdigest(),
        'stdout_equals_committed_transcript': p.stdout == committed,
        'first_199_stdout_lines_equal_committed': p.stdout.splitlines()[:199] == committed.splitlines()[:199]}
    assert report['adapters'][name]['stdout_equals_committed_transcript'], name
(work / 'md5-guard.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
print(json.dumps(report, indent=2, sort_keys=True))
PY
    (cd "$validation" && sha256sum -c --quiet files.sha256 && echo "$validation/files.sha256: $(wc -l < files.sha256) entries verify")
    python3 port/tools/validate_units.py
    done_step guard
fi
if ! have registered; then
    stamp registered md5 unit at the default per-entry budget
    start=$(date +%s)
    rust/target/debug/kicad-difftest check --root "$PWD" --unit $unit --oracle "$oracle" \
        --rust "$work/adapters/kicad-md5-shim" --report "$work/registered-md5.json"
    echo "registered md5 seconds: $(( $(date +%s) - start ))"
    sha256sum "$oracle" "$work/adapters/kicad-md5-shim" rust/target/debug/kicad-difftest | tee "$work/binaries.sha256"
    done_step registered
fi
stamp summary
python3 - "$work" "$rev" "$unit" <<'PY'
import hashlib, json, pathlib, sys
work, rev, unit = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
sha = lambda p: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
p = json.loads((work / 'oracle-provenance.json').read_text())
assert p == json.loads((work / 'oracle-build/oracle-provenance.json').read_text())
assert p['overlay_revision'].startswith(rev) and p['overlay_dirty_at_export'] is False
assert p['floating_point'] == {'contract': 'off', 'fast_math': False}
assert sha(work / 'oracle-src/reference/source-manifest.json') == p['source_manifest_sha256']
r = json.loads((work / 'registered-md5.json').read_text())
binaries = {pathlib.Path(path).name: digest for digest, path in
            (line.split() for line in (work / 'binaries.sha256').read_text().splitlines())}
assert r['oracle_sha256'] == binaries['kicad-oracle'] and r['rust_sha256'] == binaries['kicad-md5-shim']
assert r['status'] == 'green' and r['cases'] == 5973 and r['deferred'] == 0 and not r.get('failures')
golden = sha(work / f'repo/port/goldens/{unit}.jsonl')
summary = {'tested_revision': p['overlay_revision'], 'reference_file_count': p['reference_file_count'],
           'compile_commands': len(p['compile_commands']), 'oracle_provenance_sha256': sha(work / 'oracle-provenance.json'),
           'source_manifest_sha256': p['source_manifest_sha256'],
           'guard': json.loads((work / 'md5-guard.json').read_text()),
           'registered_md5': {k: r.get(k) for k in ('status', 'cases', 'deferred', 'failures', 'oracle_sha256',
                                                    'rust_sha256', 'oracle_output_sha256', 'rust_output_sha256')},
           'golden_sha256': golden}
(work / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
PY
done_step all
stamp MD5_FOLLOWUP_PROOF_COMPLETE
