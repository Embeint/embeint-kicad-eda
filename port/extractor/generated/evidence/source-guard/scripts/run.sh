#!/usr/bin/env bash
# Guarded generator-dependency recapture for PR39 at ce9681f06f: capture and apply with
# the source-identity API from the same original V5 base. Resumable stage markers; holds
# the shared lock throughout. The D2 root ~/kicad-port/generator-refresh-57e is read only.
set -euo pipefail
N="$HOME/kicad-port/generator-refresh-ce9681f"
S=/mnt/c/Users/KarlVonRichter/.t3/phase0-source-guard
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-7461f604-ff4777aca1333cd1
V5="$HOME/kicad-port/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
export PYTHONDONTWRITEBYTECODE=1
mkdir -p "$N/stages" "$N/evidence" "$N/logs"
echo "$$" > "$N/run.pid"
reported=0
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do
  if test "$reported" -eq 0; then echo "$(date -u) waiting for shared build lock"; reported=1; fi
  sleep 10
done
trap 'rmdir "$HOME/kicad-port.build-lock"; date -u > "$N/run-exited.txt"' EXIT
echo "$(date -u) acquired shared build lock"
stage() { test -f "$N/stages/$1.done"; }
done_stage() { date -u > "$N/stages/$1.done"; echo "$(date -u) stage $1 done"; }

if ! stage code; then
  mkdir -p "$N/current/port" "$N/v5-runtime/port"
  rsync -a --delete --exclude generated --exclude __pycache__ "$ROOT/port/extractor/" "$N/current/port/extractor/"
  rsync -a --delete --exclude __pycache__ "$ROOT/port/tools/" "$N/current/port/tools/"
  rsync -a --delete --exclude __pycache__ "$V5/runner/port/" "$N/v5-runtime/port/"
  cp "$S/units.tar" "$N/units.tar"
  "$PY" "$S/prepare.py" "$N" > "$N/logs/prepare.log" 2>&1
  done_stage code
fi
if ! stage tests; then
  (cd "$N/current" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v) > "$N/logs/extractor-tests-wsl.log" 2>&1
  done_stage tests
fi
if ! stage capture; then
  rm -rf "$N/capture"
  "$PY" "$N/current/port/extractor/refresh_generator_dependencies.py" capture \
    --base-manifest "$V5/inputs/generated-sources.json" \
    --inventory "$V5/output/inventory.json" \
    --generation-inputs "$N/base-generation-input-audit.json" \
    --source-root "$HOME/kicad-port/extract-source" --output "$N/capture" > "$N/logs/capture.log" 2>&1
  done_stage capture
fi
if ! stage apply; then
  G="$N/canonical-old"
  for out in refreshed refreshed-repeat; do
    rm -rf "$N/$out"
    "$PY" "$N/current/port/extractor/refresh_generator_dependencies.py" apply \
      --graph "$G/graph.json.gz" --inventory "$G/inventory.json" \
      --base-manifest "$V5/inputs/generated-sources.json" \
      --capture "$N/capture/generator-dependency-capture.json" \
      --recipes "$N/capture/generated-recipes.json" \
      --generation-inputs "$N/base-generation-input-audit.json" --output "$N/$out" > "$N/logs/apply-$out.log" 2>&1
  done
  for f in graph.json.gz inventory.json generated-sources.json refresh-report.json generator-dependency-capture.json; do
    cmp "$N/refreshed/$f" "$N/refreshed-repeat/$f"
  done
  done_stage apply
fi
if ! stage compare; then
  "$PY" "$S/compare.py" "$N" > "$N/logs/compare.log" 2>&1
  done_stage compare
fi
if ! stage assemble; then
  "$PY" "$S/assemble.py" "$N" > "$N/logs/assemble.log" 2>&1
  done_stage assemble
fi
if ! stage actual-controls; then
  "$PY" "$S/actual_controls.py" "$N" > "$N/logs/actual-controls.log" 2>&1
  done_stage actual-controls
fi
if ! stage repro-controls; then
  "$PY" "$S/repro_controls.py" "$N" > "$N/logs/repro-controls.log" 2>&1
  done_stage repro-controls
fi
date -u > "$N/completed.txt"
echo "$(date -u) guarded generator refresh workflow completed"
