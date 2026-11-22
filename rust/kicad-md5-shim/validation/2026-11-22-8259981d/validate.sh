#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/md5-request-parse"
previous="$HOME/kicad-port/md5-input-fix"
windows=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-b1e44652-8ce837179fbd65f3
lock="$HOME/kicad-port.build-lock"
echo 'Waiting for real shared lock; no compilation or prior-tree mutation while extractor owns it'
until mkdir "$lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$lock"' EXIT
echo 'Acquired preparation lock'
cp -a --reflink=auto "$previous/repo/." "$work/repo/"
rsync -a --exclude=.git --exclude=target --exclude=.port-deps --exclude=.port-venv --exclude='.md5*' --exclude=__pycache__ "$windows/" "$work/repo/"
mkdir -p "$work/stage"
cp "$windows/.md5-parser-stage/"* "$work/stage/"
python3 - "$work/stage/source-manifest.json" <<'PY'
import json, pathlib, sys
record = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert record['overlay_revision'] == '8259981dde8c9632c3b1194e8cc9362fe87a45b9'
assert record['overlay_dirty_at_export'] is False
print('Actual clean exported source:', record['overlay_revision'])
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
python3 port/tools/validate_units.py --json > "$work/units.json"
python3 rust/kicad-md5-shim/generate_cases.py --output "$work/regenerated.jsonl"
cmp "$work/regenerated.jsonl" port/cases/libs.kimath.src.md5_hash.application.pure.p001.jsonl
"$work/runners/kicad-difftest" check --root "$work/repo" \
    --unit libs.kimath.src.md5_hash.application.pure.p001 \
    --oracle "$work/runners/kicad-oracle" --rust "$work/runners/kicad-md5-shim" \
    --report "$work/live-check.json"
python3 - "$work" <<'PY'
import importlib.util, json, pathlib, sys
work = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location('md5_inputs', 'port/oracle/test_md5_inputs.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
checks = module.regression_cases()
(work/'raw-regression-requests.jsonl').write_text(''.join(text+'\n' for text, _ in checks))
(work/'regression-expected.jsonl').write_text(''.join(json.dumps(reply,separators=(',',':'))+'\n' for _, reply in checks))
assert len(checks) == 79 and sum('error' in value for _,value in checks) == 78
assert json.loads((work/'units.json').read_text())['valid'] is True
print('All parser adoption gates complete: 78 full refusals plus signed-zero success per adapter; 5973 live cases')
PY
touch "$work/completed"
