#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/resistor-escaped-fix-ec81fa0b-r2"
prior="$HOME/kicad-port/pilot-resistors-private-keys/proof-ff419ad71f-r2"
foundation="$HOME/kicad-port/md5-corrected-graph/repo"
if [[ -e "$work" ]]; then echo "Refusing existing validation root: $work" >&2; exit 2; fi
mkdir -p "$work/repo" "$work/oracle"
exec > >(tee "$work/validation.log") 2>&1
echo 'Waiting for shared lock; validation changes tests only and reuses the qualified ff419ad71f C++ executable.'
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do sleep 5; done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
tar -xf /mnt/c/Users/KarlVonRichter/.t3/resistor-escaped-fix/source.tar -C "$work/repo"
cp -a "$prior/src/rust/target" "$work/repo/rust/target"
rsync -a --exclude=target/ "$foundation/.port-deps/" "$work/repo/.port-deps/"
rsync -a "$foundation/.port-venv/" "$work/repo/.port-venv/"
rsync -a "$foundation/.west/" "$work/repo/.west/"
cp "$prior/oracle-build/kicad-oracle" "$work/oracle/"
cp "$prior/oracle-build/oracle-provenance.json" "$work/oracle/"
oracle="$work/oracle/kicad-oracle"
printf '%s  %s\n' b0b167cb40bfcb3b2f8fa6791f36742bdab8a560d791acccb875b105ce9b632f "$oracle" | sha256sum -c -
"$oracle" --provenance > "$work/live-oracle-provenance.json"
cmp "$work/live-oracle-provenance.json" "$prior/oracle-build/oracle-provenance.json"
cd "$work/repo"
export PATH="$HOME/.cargo/bin:$PATH"
export PYTHONDONTWRITEBYTECODE=1
python3 port/tools/bootstrap_west.py --check
cargo fmt --manifest-path rust/Cargo.toml -p kicad-resistors-shim --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-resistors-shim --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-resistors-shim --test private_keys -j8
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-resistors-shim --test private_keys --release -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-resistors-shim -j8
debug="$work/repo/rust/target/debug/kicad-resistors-shim"
release="$work/repo/rust/target/release/kicad-resistors-shim"
runner="$work/repo/rust/target/debug/kicad-difftest"
python3 port/oracle/generate_resistor_escaped_keys.py --check
python3 port/oracle/test_resistor_private_keys.py --byte-identical "$oracle"
python3 port/oracle/test_resistor_private_keys.py "$debug" "$release"
python3 rust/kicad-resistors-shim/tests/requests.py --oracle "$oracle" --rust "$debug" --rust "$release"
"$runner" check --root "$PWD" --unit pcb_calculator.application.pure.p001 --oracle "$oracle" --rust "$debug" --report "$work/registered-debug.json"
"$runner" compare --root "$PWD" --unit pcb_calculator.application.pure.p001 --oracle "$oracle" --rust "$release" --report "$work/registered-release.json"
sha256sum "$oracle" "$debug" "$release" "$runner" > "$work/binary.sha256"
printf 'Escaped-key default guards, debug/release Rust tests, all 1159 envelopes and both 1050-case runs passed.\n'
