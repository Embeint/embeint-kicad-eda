#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
build=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
unit=libs.kimath.include.geometry.rtree.application.pure.p001
python3 rust/kicad-rtree-shim/generate_cases.py --check | tee "$out/corpus.log"
cargo fmt --manifest-path rust/Cargo.toml -p kicad-rtree -p kicad-rtree-shim --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-rtree -p kicad-rtree-shim --all-targets -j2 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-rtree -p kicad-rtree-node -p kicad-rtree-shim -j2 | tee "$out/tests.log"
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-rtree-shim -j2
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-rtree-shim --release -j2
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-rtree --target wasm32-unknown-unknown -j2
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-rtree-shim --target wasm32-wasip1 -j2
cp "$build/oracle-provenance.json" "$out/provenance.json"
for profile in debug release; do
  rust/target/debug/kicad-difftest compare --partial --unit "$unit" --root "$PWD" \
    --oracle "$build/kicad-oracle" --rust "$PWD/rust/target/$profile/kicad-rtree-shim" --report "$out/compare-$profile.json"
  python3 rust/kicad-rtree-shim/test_inputs.py "$PWD/rust/target/$profile/kicad-rtree-shim" | tee "$out/guards-$profile.json"
done
python3 rust/kicad-rtree-shim/test_inputs.py "$build/kicad-oracle" | tee "$out/guards-oracle.json"
export RTREE_WASM="$PWD/rust/target/wasm32-wasip1/debug/kicad-rtree-shim.wasm"
export RTREE_WASI_RUNNER="$PWD/rust/kicad-rtree-shim/run_wasi.mjs"
cat > "$out/wasi-shim" <<'WRAPPER'
#!/usr/bin/env bash
exec node --disable-warning=ExperimentalWarning "$RTREE_WASI_RUNNER" "$RTREE_WASM"
WRAPPER
chmod +x "$out/wasi-shim"
rust/target/debug/kicad-difftest compare --partial --unit "$unit" --root "$PWD" \
  --oracle "$build/kicad-oracle" --rust "$out/wasi-shim" --report "$out/compare-wasi.json"
python3 rust/kicad-rtree-shim/test_inputs.py "$out/wasi-shim" | tee "$out/guards-wasi.json"
bash rust/kicad-rtree-shim/sanitized_validation.sh "$build/../oracle-src/reference" "$out/sanitized"
sha256sum "$build/kicad-oracle" rust/target/{debug,release}/kicad-rtree-shim "$RTREE_WASM" > "$out/binaries.sha256"
git rev-parse HEAD > "$out/revision.txt"
git rev-parse HEAD^ > "$out/parent.txt"
rustc --version --verbose > "$out/rustc.txt"
node --version > "$out/node.txt"
uname -a > "$out/platform.txt"
