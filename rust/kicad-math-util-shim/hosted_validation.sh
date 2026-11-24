#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Run from the repository root after the audited pinned oracle and debug/release Rust builds.
set -euo pipefail
oracle_dir=${1:?oracle build directory}
out=${2:?output directory}
unit=libs.kimath.src.math.util.application.ui.p001
oracle="$oracle_dir/kicad-oracle"
mkdir -p "$out"
python3 rust/kicad-math-util-shim/generate_cases.py --check > "$out/generator.txt"
"$oracle" --provenance > "$out/oracle-provenance.json"
# Require real source symbols in the linked oracle. util.cpp comes from the audited pin.
nm -C "$oracle" > "$out/symbols.txt"
for symbol in 'RoundedHypotWide(unsigned long, unsigned long)' 'rescale<int>' 'rescale<long>'; do
    grep -F "$symbol" "$out/symbols.txt" > /dev/null
done
"$oracle" < "port/cases/$unit.jsonl" > "$out/corpus-oracle.jsonl"
for kind in debug release; do
    shim="rust/target/$kind/kicad-math-util-shim"
    rust/target/debug/kicad-difftest compare --partial --unit "$unit" --oracle "$oracle" \
        --rust "$shim" --root . --report "$out/compare-$kind.json"
    python3 - "$out/compare-$kind.json" <<'PY'
import json, sys
report = json.load(open(sys.argv[1]))
assert [report[k] for k in ("cases", "compared", "failures", "deferred", "unprocessed")] == [10656, 10656, 0, 0, 0], report
PY
    python3 port/oracle/test_math_util_inputs.py "$oracle" "$shim" > "$out/controls-$kind.txt"
    "$shim" < "port/cases/$unit.jsonl" > "$out/corpus-$kind.jsonl"
    cmp "$out/corpus-oracle.jsonl" "$out/corpus-$kind.jsonl"
done
cmp "$out/corpus-debug.jsonl" "$out/corpus-release.jsonl"
sha256sum "$oracle" rust/target/{debug,release}/kicad-math-util-shim \
    "port/cases/$unit.jsonl" port/oracle/fixtures/math-util-controls.jsonl > "$out/sha256.txt"
{ uname -srm; gcc --version; rustc --version; } > "$out/platform.txt"
echo 'PASS: debug/release exact arithmetic comparisons, controls and byte-identical Rust profiles'
