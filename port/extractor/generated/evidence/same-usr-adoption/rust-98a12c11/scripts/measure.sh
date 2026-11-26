#!/usr/bin/env bash
# Remeasure the adopted canonical graph over the unit records merged with accepted
# rust 98a12c1123. Own staging only; the earlier adoption root, V5 and the shared
# raw/reconciled outputs are read, never written.
set -euo pipefail
K="$HOME/kicad-port"
P="$K/canonical-adopt-4738a9c2"
N="$K/canonical-adopt-merge-98a12c11"
W=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-290c6870-81dcfa65b1526978
PY="$HOME/kicad-port-extractor-venv/bin/python"
export PYTHONDONTWRITEBYTECODE=1
until mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do
  echo "$(date -u +%FT%TZ) waiting for shared build lock"; sleep 30
done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
echo "$(date -u +%FT%TZ) acquired shared build lock"
rm -rf "$N"
mkdir -p "$N/final/port/extractor/generated" "$N/final/port/units" "$N/runner/port" "$N/logs"
G="$N/final/port/extractor/generated"
rsync -a --exclude __pycache__ "$P/runner/port/" "$N/runner/port/"
for f in "$W"/port/units/*.toml; do sed 's/\r$//' "$f" > "$N/final/port/units/$(basename "$f")"; done
for n in graph.json.gz inventory.json units.json measurement.json generated-sources.json source-manifest.json generated-recipes.json.gz; do cp "$W/port/extractor/generated/$n" "$G/$n"; done
cp "$W/port/extractor/generated/measurement.json" "$N/committed-measurement.json"
rsync -a "$W/port/extractor/generated/sources/" "$G/sources/"
# Runner tools must equal the committed tools (LF-normalized).
(cd "$N/runner" && find port -type f \( -name '*.py' \) | sort | while read -r f; do
  a=$(sha256sum < "$f" | cut -c1-64); b=$(sed 's/\r$//' "$W/$f" | sha256sum | cut -c1-64)
  test "$a" = "$b" || { echo "runner differs: $f"; exit 1; }; done) > "$N/logs/runner-check.log"
cmp "$P/replay/graph.json.gz" "$W/port/extractor/generated/replay-extraction/graph.json.gz"
cmp "$P/replay/inventory.json" "$W/port/extractor/generated/replay-extraction/inventory.json"
set +e
for label in default strict; do
  extra=""; test "$label" = strict && extra=--require-complete
  "$PY" "$N/runner/port/tools/validate_units.py" --root "$N/final" --source-root "$K/extract-source" --json $extra > "$N/logs/validate-$label.json" 2>&1
  echo "$?" > "$N/logs/validate-$label.exit"
done
set -e
"$PY" "$N/runner/port/tools/extraction_report.py" --raw-output "$K/raw-output" \
  --reconciled-output "$K/reconciled-output" --corrected-output "$P/replay" \
  --reconciliation "$K/reconciliation/reconciliation.json" --units-dir "$N/final/port/units" \
  --units-report "$G/units.json" --output "$G/measurement.json" > "$N/logs/measurement.log" 2>&1
sha256sum "$G/measurement.json" "$N/committed-measurement.json"
cat "$N/logs/validate-default.exit" "$N/logs/validate-strict.exit"
date -u +%FT%TZ > "$N/completed.txt"
