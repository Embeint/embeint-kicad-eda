#!/usr/bin/env bash
set -euo pipefail
N="$HOME/kicad-port/md5-literal-escaped-da17dc54"
OLD="$HOME/kicad-port/md5-envelope-regressions"
S=/mnt/c/Users/KarlVonRichter/.t3/phase0-md5-literal-escaped
mkdir "$HOME/kicad-port.build-lock" || exit 3
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
mkdir "$N"
mkdir -p "$N/repo" "$N/runners"
tar -xf "$S/code.tar" -C "$N/repo"
cp -a --reflink=auto "$OLD/repo/.port-deps" "$N/repo/.port-deps"
source "$HOME/.cargo/env"
export CARGO_BUILD_JOBS=8 PYTHONDONTWRITEBYTECODE=1
cd "$N/repo"
cargo fmt --manifest-path rust/Cargo.toml --all --check > "$N/fmt.log" 2>&1
cargo clippy --offline --locked --manifest-path rust/Cargo.toml -p kicad-md5-shim --all-targets -j8 -- -D warnings > "$N/clippy.log" 2>&1
cargo test --offline --locked --manifest-path rust/Cargo.toml -p kicad-md5-shim -j8 > "$N/tests.log" 2>&1
cargo build --offline --locked --manifest-path rust/Cargo.toml -p kicad-md5-shim -j8 > "$N/build.log" 2>&1
cargo build --offline --locked --manifest-path rust/Cargo.toml -p kicad-md5 --target wasm32-unknown-unknown -j8 > "$N/wasm.log" 2>&1
cp rust/target/debug/kicad-md5-shim "$N/runners/kicad-md5-shim"
printf '%s  %s\n' ef081cadadaadc81a14b966e492d312872ef3429e20d02ee44ef51af98ead6d1 "$OLD/runners/kicad-oracle" | sha256sum -c -
printf '%s  %s\n' 5d07329ad8d10ff7363d83d8e72f570e336125eca0500b8cb842354660f3264c "$OLD/runners/kicad-difftest" | sha256sum -c -
"$OLD/runners/kicad-oracle" --provenance > "$N/oracle-provenance.json"
cmp "$N/oracle-provenance.json" "$OLD/oracle-provenance.json"
python3 port/oracle/test_md5_inputs.py "$OLD/runners/kicad-oracle" "$N/runners/kicad-md5-shim" > "$N/envelopes.log" 2>&1
python3 "$S/proof.py" "$N" > "$N/proof.log" 2>&1
"$OLD/runners/kicad-difftest" check --unit libs.kimath.src.md5_hash.application.pure.p001 --root "$N/repo" \
    --oracle "$OLD/runners/kicad-oracle" --rust "$N/runners/kicad-md5-shim" --report "$N/registered.json" > "$N/registered.log" 2>&1
date -u > "$N/completed"
cat "$N/envelopes.log" "$N/proof.log" "$N/registered.log"
