#!/usr/bin/env bash
# Generator-dependency refresh for PR39: replay V5 read-only, recapture, apply post-parse.
# Resumable: each completed stage leaves stages/<name>.done. Holds the shared lock throughout.
set -euo pipefail
W="$HOME/kicad-port/generator-refresh-57e"
S=/mnt/c/Users/KarlVonRichter/.t3/phase0-gen-refresh
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-7461f604-ff4777aca1333cd1
V5="$HOME/kicad-port/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
export PYTHONDONTWRITEBYTECODE=1
mkdir -p "$W/stages" "$W/evidence" "$W/logs"
echo "$$" > "$W/run.pid"
reported=0
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do
  if test "$reported" -eq 0; then echo "$(date -u) waiting for shared build lock"; reported=1; fi
  sleep 10
done
trap 'rmdir "$HOME/kicad-port.build-lock"; date -u > "$W/run-exited.txt"' EXIT
echo "$(date -u) acquired shared build lock"

stage() { test -f "$W/stages/$1.done"; }
done_stage() { date -u > "$W/stages/$1.done"; echo "$(date -u) stage $1 done"; }

if ! stage code; then
  mkdir -p "$W/current/port" "$W/v5-runtime/port"
  rsync -a --delete --exclude generated --exclude __pycache__ "$ROOT/port/extractor/" "$W/current/port/extractor/"
  rsync -a --delete --exclude __pycache__ "$ROOT/port/tools/" "$W/current/port/tools/"
  rsync -a --delete --exclude __pycache__ "$V5/runner/port/" "$W/v5-runtime/port/"
  cp "$S/units.tar" "$W/units.tar"
  "$PY" "$S/identities.py" "$W"
  done_stage code
fi
if ! stage tests; then
  (cd "$W/current" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v) > "$W/logs/extractor-tests-wsl.log" 2>&1
  done_stage tests
fi
if ! stage replay; then
  rm -rf "$W/replay"
  "$PY" "$S/v5_replay.py" "$W" > "$W/logs/v5-replay.log" 2>&1
  done_stage replay
fi
if ! stage canonical; then
  "$PY" "$S/reproduce_canonical.py" "$W" > "$W/logs/reproduce-canonical.log" 2>&1
  done_stage canonical
fi
if ! stage capture; then
  rm -rf "$W/capture"
  "$PY" "$W/current/port/extractor/refresh_generator_dependencies.py" capture \
    --base-manifest "$V5/inputs/generated-sources.json" \
    --source-root "$HOME/kicad-port/extract-source" --output "$W/capture" > "$W/logs/capture.log" 2>&1
  done_stage capture
fi
if ! stage apply; then
  G="$W/canonical-old/port/extractor/generated"
  for out in refreshed refreshed-repeat; do
    rm -rf "$W/$out"
    "$PY" "$W/current/port/extractor/refresh_generator_dependencies.py" apply \
      --graph "$G/graph.json.gz" --inventory "$G/inventory.json" \
      --base-manifest "$V5/inputs/generated-sources.json" \
      --capture "$W/capture/generator-dependency-capture.json" \
      --recipes "$W/capture/generated-recipes.json" --output "$W/$out" > "$W/logs/apply-$out.log" 2>&1
  done
  for f in graph.json.gz inventory.json generated-sources.json refresh-report.json generator-dependency-capture.json; do
    cmp "$W/refreshed/$f" "$W/refreshed-repeat/$f"
  done
  done_stage apply
fi
if ! stage assemble; then
  "$PY" "$S/assemble.py" "$W" > "$W/logs/assemble.log" 2>&1
  done_stage assemble
fi
if ! stage evidence; then
  "$PY" "$S/evidence.py" "$W" > "$W/logs/evidence.log" 2>&1
  done_stage evidence
fi
if ! stage fixtures; then
  "$PY" "$S/fixtures.py" "$W" > "$W/logs/fixtures.log" 2>&1
  done_stage fixtures
fi
if ! stage candidates; then
  "$PY" "$S/candidates.py" "$W" > "$W/logs/candidates.log" 2>&1
  done_stage candidates
fi
date -u > "$W/completed.txt"
echo "$(date -u) generator refresh workflow completed"
