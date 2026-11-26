#!/usr/bin/env bash
# Split the replayed graph into a disposable copy of the current unit records.
set -eu
K="$HOME/kicad-port"
RUN="$K/shape-collision"
V5="$K/corrected-review-v5"
W=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-cc7d0fad-0bc9e64b3c386645
PY="$HOME/kicad-port-extractor-venv/bin/python"
FINAL="$RUN/final"
OUT="$FINAL/port/extractor/generated"
rm -rf "$FINAL"
mkdir -p "$OUT" "$FINAL/port/units" "$RUN/baseline-units"
rsync -a --delete "$W/port/units/" "$RUN/baseline-units/"
rsync -a "$W/port/units/" "$FINAL/port/units/"
cp "$RUN/replay/graph.json.gz" "$RUN/replay/inventory.json" "$OUT/"
cp "$K/source-manifest.json" "$OUT/source-manifest.json"
cp "$V5/inputs/generated-sources.json" "$V5/inputs/generated-recipes.json" "$OUT/"
rsync -a "$V5/inputs/sources/" "$OUT/sources/"
cd "$RUN/runner"
"$PY" port/tools/split_units.py --graph "$OUT/graph.json" --inventory "$OUT/inventory.json" \
  --units-dir "$FINAL/port/units" --report "$OUT/units.json" --source-root "$K/extract-source" > "$RUN/split.log" 2>&1
tail -3 "$RUN/split.log"
set +e
"$PY" port/tools/validate_units.py --root "$FINAL" --source-root "$K/extract-source" > "$RUN/validate.log" 2>&1
echo "validate exit $?"; tail -3 "$RUN/validate.log"
"$PY" port/tools/validate_units.py --root "$FINAL" --source-root "$K/extract-source" --require-complete > "$RUN/validate-strict.log" 2>&1
echo "strict exit $?"; tail -3 "$RUN/validate-strict.log"
