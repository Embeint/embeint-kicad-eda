#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Live validation of unit libs.kimath.include.application.pure.p001 against a freshly built
# oracle, for hosted CI or the qualified reference image. Run from the repository root after the
# oracle build (port/oracle/build.sh) and the Rust builds (debug workspace, release shim):
#
#   bash rust/kicad-hash-shim/hosted_validation.sh ORACLE_BUILD_DIR OUT_DIR
#
# ORACLE_BUILD_DIR holds kicad-oracle and CMakeFiles/kicad-oracle.dir/shims/
# kimath_header_hash.cpp.o. Every step runs; the exit status is non-zero if any fails:
#   1. reference bindings (BEZIER<double> sites, libstdc++ grouping runtime);
#   2. kicad-difftest compare, debug and release shims: all 2641 corpus cases compared exactly;
#   3. check_guards.py (441 refusal/guard requests incl. the 15 MMH3 and 325 BEZIER<int>
#      undefined-evaluation fixtures), oracle with each shim;
#   4. the supplementary sensitive requests, oracle versus each shim;
#   5. debug and release corpus replies byte-identical;
#   6. wasm32-unknown-unknown: every corpus reply reproduces the native digest (needs node).
# Nothing here proposes or blesses a golden.
set -uo pipefail
oracle_dir=${1:?oracle build directory}
out=${2:?output directory}
unit=libs.kimath.include.application.pure.p001
oracle="$oracle_dir/kicad-oracle"
object="$oracle_dir/CMakeFiles/kicad-oracle.dir/shims/kimath_header_hash.cpp.o"
runner=rust/target/debug/kicad-difftest
debug=rust/target/debug/kicad-hash-shim
release=rust/target/release/kicad-hash-shim
sensitive=rust/kicad-hash-shim/supplementary/sensitive.requests.jsonl
mkdir -p "$out"
status=0
step() { echo "== $*" | tee -a "$out/summary.txt"; }
fail() { step "FAIL: $*"; status=1; }

python3 rust/kicad-hash-shim/check_reference_bindings.py --object "$object" --oracle "$oracle" \
    > "$out/bindings.txt" 2>&1 || fail "reference bindings (see bindings.txt)"
step "bindings: $(tail -n 1 "$out/bindings.txt")"

for kind in debug release; do
    shim="rust/target/$kind/kicad-hash-shim"
    "$runner" compare --unit "$unit" --oracle "$oracle" --rust "$shim" --root . \
        --report "$out/compare-$kind.json" > "$out/compare-$kind.txt" 2>&1
    verdict=$(python3 - "$out/compare-$kind.json" <<'EOF'
import json, sys
r = json.load(open(sys.argv[1]))
ok = (r.get("cases") == 2641 and r.get("compared") == 2641 and r.get("failures") == 0
      and r.get("deferred") == 0 and r.get("unprocessed") == 0)
print(("ok " if ok else "BAD ") + str({k: r.get(k) for k in ("status", "cases", "compared",
      "deferred", "failures", "unprocessed")}))
EOF
)
    case "$verdict" in ok*) ;; *) fail "compare $kind";; esac
    step "compare $kind: $verdict"

    python3 rust/kicad-hash-shim/check_guards.py "$oracle" "$shim" > "$out/guards-$kind.txt" 2>&1 \
        || fail "guards $kind"
    step "guards $kind: $(tail -n 1 "$out/guards-$kind.txt")"
done

"$oracle" < "$sensitive" > "$out/sensitive-oracle.jsonl"
for kind in debug release; do
    "rust/target/$kind/kicad-hash-shim" < "$sensitive" > "$out/sensitive-$kind.jsonl"
    python3 rust/kicad-hash-shim/compare_replies.py "$sensitive" "$out/sensitive-oracle.jsonl" \
        "$out/sensitive-$kind.jsonl" "$out/sensitive-$kind.json" > "$out/sensitive-$kind.txt" 2>&1 \
        || fail "sensitive $kind"
    step "sensitive $kind: $(cat "$out/sensitive-$kind.txt")"
done

"$oracle" < "port/cases/$unit.jsonl" > "$out/corpus-oracle.jsonl"
"$debug" < "port/cases/$unit.jsonl" > "$out/corpus-debug.jsonl"
"$release" < "port/cases/$unit.jsonl" > "$out/corpus-release.jsonl"
cmp -s "$out/corpus-debug.jsonl" "$out/corpus-release.jsonl" || fail "debug and release replies differ"
step "debug/release corpus replies identical: $(cmp -s "$out/corpus-debug.jsonl" "$out/corpus-release.jsonl" && echo yes || echo no)"

digest=$(cd rust && cargo run --locked -q -p kicad-hash --example wasm_parity 2> "$out/wasm-native.log")
hex=$(echo "$digest" | awk '{print $NF}')
step "native wasm_parity: $digest"
if (cd rust && KICAD_HASH_EXPECTED_DIGEST="$hex" cargo build --locked --release -p kicad-hash \
        --example wasm_parity --target wasm32-unknown-unknown > "$out/wasm-build.log" 2>&1) \
   && node rust/kicad-hash/wasm-test.mjs \
        rust/target/wasm32-unknown-unknown/release/examples/wasm_parity.wasm > "$out/wasm-run.log" 2>&1; then
    step "wasm: $(cat "$out/wasm-run.log")"
else
    fail "wasm parity (see wasm-build.log, wasm-run.log)"
fi

sha256sum "$oracle" "$object" "$debug" "$release" \
    rust/target/wasm32-unknown-unknown/release/examples/wasm_parity.wasm \
    "port/cases/$unit.jsonl" "$sensitive" "$out"/corpus-*.jsonl "$out"/sensitive-*.jsonl \
    > "$out/sha256.txt" 2>/dev/null
{ uname -srm; gcc --version | head -n 1; rustc --version; node --version
  ldd "$oracle" | grep libstdc++; } > "$out/platform.txt" 2>&1
step "result: $([ $status -eq 0 ] && echo PASS || echo FAIL)"
exit $status
