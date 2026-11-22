#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/md5-corrected-graph"
previous="$HOME/kicad-port/md5-request-parse"
windows=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-b1e44652-8ce837179fbd65f3
lock="$HOME/kicad-port.build-lock"
echo 'Waiting for actual shared lock before private preparation'
until mkdir "$lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$lock"' EXIT
cp -a --reflink=auto "$previous/repo/." "$work/repo/"
rsync -a --exclude=.git --exclude=target --exclude=.port-deps --exclude=.port-venv --exclude='.md5*' --exclude=__pycache__ "$windows/" "$work/repo/"
# Remove obsolete generated records only inside this new private copy.
rsync -a --delete --exclude=__pycache__ "$windows/port/" "$work/repo/port/"
mkdir -p "$work/stage"
cp "$windows/.md5-corrected-stage/"* "$work/stage/"
python3 - "$work/stage/source-manifest.json" <<'PY'
import json, pathlib, sys
record = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert record['overlay_revision'] == 'a5ed3948399da32b64f7fad93e0394c64615f8ff'
assert record['overlay_dirty_at_export'] is False
print('Clean exported integration source:', record['overlay_revision'])
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
python3 - "$work" <<'PY'
import importlib.util, json, pathlib, sys
work = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location('md5_inputs', 'port/oracle/test_md5_inputs.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
checks = module.regression_cases()
assert len(checks) == 154 and sum('error' in v for _,v in checks) == 153
(work/'raw-regression-requests.jsonl').write_text(''.join(text+'\n' for text,_ in checks))
(work/'regression-expected.jsonl').write_text(''.join(json.dumps(value,separators=(',',':'))+'\n' for _,value in checks))
print('Corrected graph integration complete: 153 complete refusals and signed-zero success per adapter, full registered live check')
PY
touch "$work/completed"
