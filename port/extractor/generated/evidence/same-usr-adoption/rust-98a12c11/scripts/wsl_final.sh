#!/usr/bin/env bash
# Final WSL checks on the merged adoption tree: libclang extractor suite from a raw-byte
# export of the final tools, and the accepted-versus-adopted graph comparison.
# Own staging only; the shared lock is held while running.
set -euo pipefail
K="$HOME/kicad-port"
N="$K/canonical-adopt-merge-98a12c11"
S=/mnt/c/Users/KarlVonRichter/.t3/canon-adopt-merge
W=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-290c6870-81dcfa65b1526978
PY="$HOME/kicad-port-extractor-venv/bin/python"
export PYTHONDONTWRITEBYTECODE=1
until mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do
  echo "$(date -u +%FT%TZ) waiting for shared build lock"; sleep 30
done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
echo "$(date -u +%FT%TZ) acquired shared build lock"
rm -rf "$N/final-runner" "$N/old-units" "$N/new-units"
mkdir -p "$N/final-runner" "$N/old-units" "$N/new-units" "$N/logs"
cp -r "$S/runner/port" "$N/final-runner/port"
set +e
(cd "$N/final-runner" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v) > "$N/logs/extractor-tests-wsl-final.log" 2>&1
echo "extractor suite exit $?"
set -e
tail -3 "$N/logs/extractor-tests-wsl-final.log"
cp "$S/old-units/"*.toml "$N/old-units/"
for f in "$W"/port/units/*.toml; do sed 's/\r$//' "$f" > "$N/new-units/$(basename "$f")"; done
/usr/bin/time -v "$PY" "$S/scripts/graphcmp.py" "$S/rust-graph.json.gz" "$W/port/extractor/generated/graph.json.gz" \
  "$N/old-units" "$N/new-units" "$N/graph-comparison.json" > "$N/logs/graphcmp.log" 2> "$N/logs/graphcmp.err"
cat "$N/logs/graphcmp.log"
grep -E 'Maximum resident|Elapsed' "$N/logs/graphcmp.err"
