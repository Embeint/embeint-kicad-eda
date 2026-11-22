#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/pilot-md5"
source_root=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-b1e44652-8ce837179fbd65f3
rsync -a --exclude=.git --exclude=target --exclude=.port-deps --exclude=.port-venv --exclude='.md5*' --exclude=__pycache__ "$source_root/" "$work/repo/"
lock="$HOME/kicad-port.build-lock"
if ! mkdir "$lock"; then
    echo 'Shared lock busy; integration gates did not start' >&2
    exit 3
fi
trap 'rmdir "$lock"' EXIT
source "$HOME/.cargo/env"
cd "$work/repo"
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-md5 --target wasm32-unknown-unknown -j8
python3 port/tools/check_goldens_policy.py --ci \
    --runner "$work/repo/rust/target/debug/kicad-difftest" \
    --oracle "$work/oracle-build/kicad-oracle" \
    --reports "$work/integration-reports"
echo 'Registered unit integration gates complete; strict phase-zero gate unchanged'
