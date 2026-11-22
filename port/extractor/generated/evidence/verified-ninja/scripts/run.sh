#!/usr/bin/env bash
# Guarded generator-dependency recapture for PR39 at c1e8a467da: capture and apply with
# the source-identity API from the same original V5 base. Resumable stage markers; holds
# the shared lock throughout. The D2 root ~/kicad-port/generator-refresh-57e is read only.
set -euo pipefail
N="$HOME/kicad-port/generator-refresh-ninja-c1e8a467"
S=/mnt/c/Users/KarlVonRichter/.t3/phase0-verified-ninja
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-04614140-10140941291c27ae
V5="$HOME/kicad-port/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
export PYTHONDONTWRITEBYTECODE=1
mkdir -p "$N/stages" "$N/evidence" "$N/logs"
echo "$$" > "$N/run.pid"
mkdir "$HOME/kicad-port.build-lock" || exit 3
trap 'rmdir "$HOME/kicad-port.build-lock"; date -u > "$N/run-exited.txt"' EXIT
echo "$(date -u) acquired shared build lock"
stage() { test -f "$N/stages/$1.done"; }
done_stage() { date -u > "$N/stages/$1.done"; echo "$(date -u) stage $1 done"; }

if ! stage code; then
  mkdir -p "$N/current/port" "$N/v5-runtime/port"
  tar -xf "$S/code.tar" -C "$N/current"
  rsync -a --delete --exclude __pycache__ "$V5/runner/port/" "$N/v5-runtime/port/"
  cp "$S/units.tar" "$N/units.tar"
  "$PY" "$S/prepare.py" "$N" > "$N/logs/prepare.log" 2>&1
  done_stage code
fi
if ! stage tests; then
  (cd "$N/current" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v) > "$N/logs/extractor-tests-wsl.log" 2>&1
  done_stage tests
fi
if ! stage old-implementation-control; then
  cp -a "$N/current" "$N/negative-control"
  cp "$S/old-refresh.py" "$N/negative-control/port/extractor/refresh_generator_dependencies.py"
  if (cd "$N/negative-control" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -k test_post_parse_dependency_refresh_changes_only_generator_metadata -v) > "$N/logs/old-implementation-control.log" 2>&1; then
    echo "Old implementation unexpectedly passed PATH substitution regression" >&2
    exit 1
  fi
  grep -F 'Capture invoked an unrecorded PATH executable' "$N/logs/old-implementation-control.log"
  done_stage old-implementation-control
fi
if ! stage capture; then
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
