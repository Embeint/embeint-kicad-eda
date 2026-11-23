#!/usr/bin/env bash
# Build the focused ASan+UBSan adapter from the snapshot and replay cases/refusals.
set -euo pipefail
W="$HOME/kicad-port/rtree-p003"; R="${1:-$W/repo}"; REF="${2:-$W/oracle-src/reference}"
O="${3:-$W/oracle-build/kicad-oracle}"; OUT="${4:-/mnt/c/kb/rtree-p003-scripts/smoke}"
U=libs.kimath.include.geometry.rtree.application.pure.p003
S="$W/sanitized"; mkdir -p "$S"
cp /mnt/c/kb/rtree-p003-scripts/sanitize_driver.cpp "$S/"
bash /mnt/c/kb/rtree-p003-scripts/lock.sh g++ -std=c++20 -O1 -g1 -fno-fast-math -ffp-contract=off \
    -fsanitize=undefined,address -fno-sanitize-recover=all -fno-omit-frame-pointer \
    -I "$REF/libs/kimath/include" -I "$REF/thirdparty/nlohmann_json" -I "$R/port/oracle" \
    "$S/sanitize_driver.cpp" "$R/port/oracle/registry.cpp" "$R/port/oracle/protocol.cpp" \
    "$R/port/oracle/shims/rtree_node.cpp" -o "$S/rtree-adapter-sanitized"
sha256sum "$S/rtree-adapter-sanitized" | tee "$OUT/sanitize-binary.sha256"
status=0
for name in cases refusals; do
    if [ $name = cases ]; then in="$R/port/cases/$U.jsonl"; else in="$R/rust/kicad-rtree-node-shim/fixtures/refusals.requests.jsonl"; fi
    if ! "$S/rtree-adapter-sanitized" < "$in" > "$OUT/$name.sanitized.jsonl" 2> "$OUT/$name.sanitized.stderr"; then
        echo "sanitized run failed: $name"; status=1
    fi
    "$O" < "$in" > "$OUT/$name.plain.jsonl"
    if cmp -s "$OUT/$name.sanitized.jsonl" "$OUT/$name.plain.jsonl"; then echo "$name: sanitized output identical ($(wc -l < "$in") requests)"; else echo "$name: OUTPUT DIFFERS"; status=1; fi
    if [ -s "$OUT/$name.sanitized.stderr" ]; then echo "$name: sanitizer diagnostics:"; head -20 "$OUT/$name.sanitized.stderr"; status=1; fi
done
echo "sanitize status=$status"; exit $status
