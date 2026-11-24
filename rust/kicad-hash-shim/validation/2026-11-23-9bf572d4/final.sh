#!/usr/bin/env bash
# Final validation of the clean integrated head on native WSL x86-64. Heavy steps hold the
# shared build lock (build.sh takes and releases its own); only our own lock is ever removed.
set -uo pipefail
. "$HOME/.cargo/env"
root="$HOME/kicad-port-hash51"
src="$root/src"
out="$root/final"
unit="libs.kimath.include.application.pure.p001"
lock="$HOME/kicad-port.build-lock"
rm -rf "$out"; mkdir -p "$out"
log() { echo "== $*" | tee -a "$out/summary.txt"; }

# 1. Clean own oracle build from the clean-commit export.
rm -rf "$root/work" "$root/oracle-stage"; mkdir -p "$root/oracle-stage"
cp /mnt/c/Users/KarlVonRichter/.t3/hash-wsl-msg/oracle-stage/reference.tar \
   /mnt/c/Users/KarlVonRichter/.t3/hash-wsl-msg/oracle-stage/source-manifest.json "$root/oracle-stage/"
log "pinned mmh3_hash.h CR lines: $(tar -xOf "$root/oracle-stage/reference.tar" libs/kimath/include/mmh3_hash.h | grep -c $'\r'); reference files with CR: $(mkdir -p /tmp/hash51-ref && tar -xf "$root/oracle-stage/reference.tar" -C /tmp/hash51-ref && grep -rlI $'\r' /tmp/hash51-ref | wc -l); source files with CR: $(grep -rlI $'\r' "$src/rust" "$src/port" | wc -l)"
rm -rf /tmp/hash51-ref
start=$(date +%s)
ORACLE_JOBS=8 bash "$src/port/oracle/build.sh" "$root/oracle-stage" "$src/port/oracle" "$root/work" > "$out/oracle-build.log" 2>&1
log "oracle build.sh exit $? in $(( $(date +%s) - start ))s; $(grep -E 'tests passed|tests failed' "$out/oracle-build.log")"
oracle="$root/work/oracle-build/kicad-oracle"
"$oracle" --provenance > "$out/oracle-provenance.json"
log "oracle provenance overlay: $(python3 -c 'import json,sys; p=json.load(open(sys.argv[1])); print({k: v for k, v in p.items() if "overlay" in k and not isinstance(v, (dict, list))})' "$out/oracle-provenance.json")"

# 2. Rust workspace under the build lock.
until mkdir "$lock" 2>/dev/null; do echo "lock busy; waiting" >> "$out/lock-wait.log"; sleep 20; done
trap 'rmdir "$lock" 2>/dev/null' EXIT
cd "$src/rust"
unset CARGO_TARGET_DIR   # the ci policy expects rust/target
cargo fmt --all -- --check > "$out/fmt.log" 2>&1; log "cargo fmt --check exit $?"
cargo clippy --locked --workspace --all-targets -j8 -- -D warnings > "$out/clippy.log" 2>&1; log "cargo clippy --workspace -D warnings exit $?"
cargo test --locked --workspace -j8 > "$out/test.log" 2>&1; log "cargo test --workspace exit $?; $(grep -c '^test result: ok' "$out/test.log") ok suites, $(grep -E '^test result' "$out/test.log" | awk '{p+=$4; f+=$6} END {print p" passed, "f" failed"}')"
cargo build --locked -j8 --workspace --bins > "$out/build-debug.log" 2>&1; log "debug build (every workspace binary, incl. all unit shims) exit $?"
cargo build --locked -j8 --release -p kicad-hash-shim > "$out/build-release.log" 2>&1; log "release build exit $?"
cd "$src"
python3 port/tools/check_goldens_policy.py --wasm > "$out/wasm-policy.log" 2>&1; log "check_goldens_policy --wasm exit $?: $(tail -n 1 "$out/wasm-policy.log")"
digest=$(cd rust && cargo run --locked -q -p kicad-hash --example wasm_parity 2>>"$out/wasm-native.log")
log "native wasm_parity: $digest"
hex=$(echo "$digest" | awk '{print $NF}')
(cd rust && KICAD_HASH_EXPECTED_DIGEST="$hex" cargo build --locked -j8 --release -p kicad-hash --example wasm_parity \
    --target wasm32-unknown-unknown > "$out/wasm-build.log" 2>&1); log "wasm example build exit $?"
node rust/kicad-hash/wasm-test.mjs rust/target/wasm32-unknown-unknown/release/examples/wasm_parity.wasm > "$out/wasm-run.log" 2>&1
log "node wasm run exit $?: $(cat "$out/wasm-run.log")"
python3 port/tools/check_goldens_policy.py --ci --require-phase0 --runner rust/target/debug/kicad-difftest \
    --oracle "$oracle" --reports "$out/ci-reports" > "$out/ci-phase0.log" 2>&1
log "check_goldens_policy --ci --require-phase0 exit $?: $(tail -n 2 "$out/ci-phase0.log" | tr '\n' ' ')"
rmdir "$lock"; trap - EXIT

# 3. Comparisons (no build).
runner="$src/rust/target/debug/kicad-difftest"
debug="$src/rust/target/debug/kicad-hash-shim"
release="$src/rust/target/release/kicad-hash-shim"
python3 rust/kicad-hash-shim/check_guards.py "$oracle" "$debug" > "$out/guards-debug.txt" 2>&1; log "guards debug exit $?: $(tail -n 1 "$out/guards-debug.txt")"
python3 rust/kicad-hash-shim/check_guards.py "$oracle" "$release" > "$out/guards-release.txt" 2>&1; log "guards release exit $?: $(tail -n 1 "$out/guards-release.txt")"
for kind in debug release; do
    shim="$src/rust/target/$kind/kicad-hash-shim"
    "$runner" compare --unit "$unit" --oracle "$oracle" --rust "$shim" --root "$src" --report "$out/compare-$kind.json" > "$out/compare-$kind.txt" 2>&1
    log "compare $kind exit $?: $(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print({k: r.get(k) for k in ("status","cases","compared","deferred","failures","unprocessed")})' "$out/compare-$kind.json")"
done
python3 /mnt/c/Users/KarlVonRichter/.t3/hash-wsl-msg/failures.py "$out/compare-debug.json" > "$out/failures.txt"
"$runner" check --partial --unit "$unit" --oracle "$oracle" --rust "$debug" --root "$src" --report "$out/check-partial.json" > "$out/check-partial.txt" 2>&1
log "check --partial (committed golden) exit $?: $(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r.get("status"), r.get("golden", {}).get("status") if isinstance(r.get("golden"), dict) else r.get("golden"))' "$out/check-partial.json")"
for other in libs.kimath.include.geometry.rtree.application.pure.p003 libs.kimath.src.md5_hash.application.pure.p001 \
             pcb_calculator.application.pure.p001 pcb_calculator.calculator_panels.application.pure.p001; do
    [ -f "port/cases/$other.jsonl" ] || { log "$other: no cases"; continue; }
    status=$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["status"])' "port/units/$other.toml")
    bin=$(python3 -c 'import tomllib,sys; print(tomllib.load(open(sys.argv[1],"rb"))["difftest"]["rust_binary"])' "port/units/$other.toml")
    flag=""; [ "$status" = partial ] && flag="--partial"
    "$runner" check $flag --unit "$other" --oracle "$oracle" --rust "$src/rust/target/debug/$bin" --root "$src" --report "$out/other-$other.json" > "$out/other-$other.txt" 2>&1
    log "$other ($status) check $flag exit $?: $(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print({k: r.get(k) for k in ("status","cases","compared","deferred","failures")})' "$out/other-$other.json")"
done
python3 port/tools/validate_units.py > "$out/validate-units.log" 2>&1; log "validate_units exit $?: $(tail -n 1 "$out/validate-units.log")"
cp port/cases/$unit.jsonl "$out/cases-before.jsonl"
cp rust/kicad-hash-shim/fixtures/bezier-int-undefined.requests.jsonl "$out/bezier-before.jsonl"
python3 rust/kicad-hash-shim/generate_cases.py > "$out/generate.log" 2>&1
cmp -s "$out/cases-before.jsonl" port/cases/$unit.jsonl && cmp -s "$out/bezier-before.jsonl" rust/kicad-hash-shim/fixtures/bezier-int-undefined.requests.jsonl
log "generator reproduces corpus and fixtures byte for byte: exit $? ($(cat "$out/generate.log"))"
python3 port/findings/kimath-header-hash-mmh3-audit/audit.py "$oracle" \
    /mnt/c/Users/KarlVonRichter/.t3/hash-wsl-msg/baseline-cases.jsonl /mnt/c/Users/KarlVonRichter/.t3/hash-wsl-msg/baseline-golden.jsonl \
    "$out/audit.json" > "$out/audit.txt" 2>&1; log "mmh3 audit exit $?: $(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["summary"]["classes"])' "$out/audit.json")"
"$oracle" < port/cases/$unit.jsonl > "$out/oracle-replies.jsonl"
"$debug" < port/cases/$unit.jsonl > "$out/rust-debug-replies.jsonl"
"$release" < port/cases/$unit.jsonl > "$out/rust-release-replies.jsonl"
cmp -s "$out/rust-debug-replies.jsonl" "$out/rust-release-replies.jsonl"; log "debug and release replies identical: exit $?"
sha256sum "$oracle" "$debug" "$release" "$runner" rust/target/wasm32-unknown-unknown/release/examples/wasm_parity.wasm \
    "$out/oracle-replies.jsonl" "$out/rust-debug-replies.jsonl" port/cases/$unit.jsonl | sed "s|$root/||" > "$out/sha256.txt"
{ uname -srm; . /etc/os-release; echo "$PRETTY_NAME"; gcc --version | head -n1; rustc --version; cargo --version; node --version; python3 --version; dpkg -l libstdc++6 | tail -n1 | awk '{print $2, $3}'; } > "$out/platform.txt"
log "done"
