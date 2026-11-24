#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
reference=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
root=$PWD
# Every header still comes from the verified unchanged source export. Only this
# separate diagnostic build adds sanitizer/coverage flags, never the oracle flags.
g++ -std=c++20 -O0 -g0 -fno-fast-math -ffp-contract=off \
  -fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all --coverage \
  -ffunction-sections -fdata-sections -Wl,--gc-sections \
  -I"$root/port/oracle" -I"$reference/libs/kimath/include" -I"$reference/thirdparty/nlohmann_json" \
  "$root/rust/kicad-rtree-shim/sanitize_main.cpp" "$root/port/oracle/protocol.cpp" \
  "$root/port/oracle/registry.cpp" "$root/port/oracle/shims/dynamic_rtree.cpp" -o "$out/rtree-sanitized"
rust/target/debug/kicad-difftest compare --partial \
  --unit libs.kimath.include.geometry.rtree.application.pure.p001 --root "$root" \
  --oracle "$out/rtree-sanitized" --rust "$root/rust/target/debug/kicad-rtree-shim" --report "$out/compare-sanitized.json"
python3 rust/kicad-rtree-shim/test_inputs.py "$out/rtree-sanitized" > "$out/guards-sanitized.json"
(cd "$out" && gcov --json-format --branch-counts --branch-probabilities rtree-sanitized-dynamic_rtree.gcno > gcov.log)
python3 rust/kicad-rtree-shim/summarize_coverage.py "$out/rtree-sanitized-dynamic_rtree.gcov.json.gz" "$out/coverage.json"
sha256sum "$out/rtree-sanitized" > "$out/sanitized.sha256"
