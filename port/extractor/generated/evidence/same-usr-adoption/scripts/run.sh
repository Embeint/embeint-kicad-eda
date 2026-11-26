#!/usr/bin/env bash
# Canonical adoption of the same-USR endpoint aggregation at accepted rust head
# 4738a9c23f. Read-only V5 cache replay with the final aggregation code, guarded
# generator-dependency capture/apply, canonical split, validation, measurement and
# comparisons. Own staging only; shared V5, shape-collision and refresh roots are
# read, never written. Resumable stage markers; the shared lock is held throughout.
set -euo pipefail
K="$HOME/kicad-port"
N="$K/canonical-adopt-4738a9c2"
S=/mnt/c/Users/KarlVonRichter/.t3/canon-adopt
V5="$K/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
export PYTHONDONTWRITEBYTECODE=1
mkdir -p "$N/stages" "$N/evidence" "$N/logs"
until mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do
  echo "$(date -u +%FT%TZ) waiting for shared build lock"; sleep 30
done
trap 'rmdir "$HOME/kicad-port.build-lock"; date -u > "$N/run-exited.txt"' EXIT
echo "$$" > "$N/run.pid"
echo "$(date -u +%FT%TZ) acquired shared build lock"
stage() { test -f "$N/stages/$1.done"; }
done_stage() { date -u +%FT%TZ > "$N/stages/$1.done"; echo "$(date -u +%FT%TZ) stage $1 done"; }
cache_state() {
  (cd "$V5/cache" && find . -type f -printf '%P %s %T@\n' | sort) > "$N/evidence/cache-$1-listing.txt"
  (cd "$V5/cache" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$N/evidence/cache-$1-content.txt"
  sha256sum "$N/evidence/cache-$1-listing.txt" "$N/evidence/cache-$1-content.txt" | sed "s#$N/##" > "$N/evidence/cache-$1.sha256"
}

if ! stage code; then
  rm -rf "$N/head" "$N/runner"
  mkdir -p "$N/head" "$N/runner/port"
  tar -xf "$S/head.tar" -C "$N/head"
  rsync -a --exclude generated --exclude __pycache__ "$N/head/port/extractor/" "$N/runner/port/extractor/"
  rsync -a --exclude __pycache__ "$N/head/port/tools/" "$N/runner/port/tools/"
  "$PY" "$S/scripts/identities.py" "$N" > "$N/logs/identities.log" 2>&1
  done_stage code
fi
if ! stage tests; then
  set +e
  (cd "$N/runner" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v) > "$N/logs/extractor-tests-wsl.log" 2>&1
  code=$?
  set -e
  tail -4 "$N/logs/extractor-tests-wsl.log"
  test "$code" -eq 0
  done_stage tests
fi
if ! stage replay; then
  cache_state before
  rm -rf "$N/replay"
  set +e
  (cd "$N/runner" && /usr/bin/time -v "$PY" port/tools/extract_graph.py \
    --compile-commands "$K/reconciled-output/compile_commands.json" \
    --source-root "$K/extract-source" --original-source-root "$K/extract-source" \
    --manifest "$K/source-manifest.json" \
    --generated-manifest "$V5/inputs/generated-sources.json" \
    --cache "$V5/cache" --output "$N/replay" --jobs 8 \
    --replay-parse-runtime "$V5/runner") > "$N/logs/replay.log" 2> "$N/logs/replay.err"
  code=$?
  set -e
  echo "$code" > "$N/evidence/replay-exit.txt"
  cache_state after
  cmp "$N/evidence/cache-before-listing.txt" "$N/evidence/cache-after-listing.txt"
  cmp "$N/evidence/cache-before-content.txt" "$N/evidence/cache-after-content.txt"
  echo "replay exit $code; cache unchanged"
  test "$code" -eq 1   # the four recorded fmt GCC-module parse failures
  done_stage replay
fi
if ! stage compare-replay; then
  "$PY" "$S/scripts/compare_replay.py" "$N" > "$N/logs/compare-replay.log" 2>&1
  done_stage compare-replay
fi
if ! stage pre; then
  rm -rf "$N/pre"
  G="$N/pre/port/extractor/generated"
  mkdir -p "$G"
  rsync -a "$N/head/port/units/" "$N/pre/port/units/"
  cp "$N/replay/graph.json.gz" "$N/replay/inventory.json" "$G/"
  cp "$K/source-manifest.json" "$G/source-manifest.json"
  cp "$V5/inputs/generated-sources.json" "$V5/inputs/generated-recipes.json" "$G/"
  rsync -a "$V5/inputs/sources/" "$G/sources/"
  (cd "$N/runner" && "$PY" port/tools/split_units.py --graph "$G/graph.json" --inventory "$G/inventory.json" \
    --units-dir "$N/pre/port/units" --report "$G/units.json" --source-root "$K/extract-source") > "$N/logs/split-pre.log" 2>&1
  tail -1 "$N/logs/split-pre.log"
  done_stage pre
fi
if ! stage retained-controls; then
  "$PY" "$S/scripts/retained_controls.py" "$N" > "$N/logs/retained-controls.log" 2>&1
  done_stage retained-controls
fi
if ! stage capture; then
  rm -rf "$N/capture"
  "$PY" "$N/runner/port/extractor/refresh_generator_dependencies.py" capture \
    --base-manifest "$V5/inputs/generated-sources.json" \
    --inventory "$N/replay/inventory.json" \
    --generation-inputs "$N/head/port/extractor/generated/evidence/verified-ninja/base-generation-input-audit.json" \
    --source-root "$K/extract-source" --output "$N/capture" > "$N/logs/capture.log" 2>&1
  done_stage capture
fi
if ! stage capture-compare; then
  "$PY" "$S/scripts/capture_compare.py" "$N" > "$N/logs/capture-compare.log" 2>&1
  done_stage capture-compare
fi
if ! stage apply; then
  G="$N/pre/port/extractor/generated"
  for out in refreshed refreshed-repeat; do
    rm -rf "$N/$out"
    "$PY" "$N/runner/port/extractor/refresh_generator_dependencies.py" apply \
      --graph "$G/graph.json.gz" --inventory "$G/inventory.json" \
      --base-manifest "$V5/inputs/generated-sources.json" \
      --capture "$N/capture/generator-dependency-capture.json" \
      --recipes "$N/capture/generated-recipes.json" \
      --generation-inputs "$N/head/port/extractor/generated/evidence/verified-ninja/base-generation-input-audit.json" \
      --output "$N/$out" > "$N/logs/apply-$out.log" 2>&1
  done
  for f in graph.json.gz inventory.json generated-sources.json refresh-report.json generator-dependency-capture.json; do
    cmp "$N/refreshed/$f" "$N/refreshed-repeat/$f"
  done
  done_stage apply
fi
if ! stage assemble; then
  "$PY" "$S/scripts/assemble.py" "$N" > "$N/logs/assemble.log" 2>&1
  done_stage assemble
fi
if ! stage diffs; then
  "$PY" "$S/scripts/diffs.py" "$N" > "$N/logs/diffs.log" 2>&1
  done_stage diffs
fi
date -u +%FT%TZ > "$N/completed.txt"
echo "$(date -u +%FT%TZ) canonical adoption workflow completed"
