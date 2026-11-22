#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/pilot-md5"
source "$HOME/.cargo/env"
export CARGO_BUILD_JOBS=8
export ORACLE_JOBS=8
cd "$work/repo"
date -u
python3 - <<'PY'
import json
from pathlib import Path
manifest = json.loads(Path('../stage/source-manifest.json').read_text())
assert manifest['overlay_revision'] == 'a295db8bcadb8de7ae084178c2e1705fcf99d762'
assert manifest['overlay_dirty_at_export'] is False
print('Clean overlay export:', manifest['overlay_revision'], flush=True)
PY
bash port/oracle/build.sh "$work/stage" "$work/repo/port/oracle" "$work"
lock="$HOME/kicad-port.build-lock"
if ! mkdir "$lock"; then
    echo 'Shared lock busy before Cargo gates; resume after its owner releases it' >&2
    exit 3
fi
trap 'rmdir "$lock"' EXIT
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml --workspace --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml --workspace -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-md5-shim -p kicad-difftest -j8
python3 port/tools/check_goldens_policy.py --wasm
python3 -m unittest discover -s rust/kicad-difftest/tests -p test_policy.py -v
python3 rust/kicad-md5-shim/generate_cases.py --output "$work/regenerated.jsonl"
cmp "$work/regenerated.jsonl" port/cases/libs.kimath.src.md5_hash.application.pure.p001.jsonl
python3 "$work/measure.py"
rust/target/debug/kicad-difftest compare \
    --unit libs.kimath.src.md5_hash.application.pure.p001 \
    --oracle "$work/oracle-build/kicad-oracle" \
    --rust "$work/repo/rust/target/debug/kicad-md5-shim" \
    --report "$work/compare.json"
date -u
echo 'MD5 build, workspace, wasm, guards, measured runtimes and live compare complete'
