#!/usr/bin/env bash
# Read-only re-aggregation of the V5 parse cache with the location-aware fix.
set -eu
K="$HOME/kicad-port"
RUN="$K/shape-collision"
V5="$K/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
echo "lock acquired $(date -u +%FT%TZ)"
find "$V5/cache" -type f -printf '%P %s %T@\n' | sort | sha256sum > "$RUN/cache-before.txt"
cd "$RUN/runner"
set +e
/usr/bin/time -v "$PY" port/tools/extract_graph.py \
  --compile-commands "$K/reconciled-output/compile_commands.json" \
  --source-root "$K/extract-source" --original-source-root "$K/extract-source" \
  --manifest "$K/source-manifest.json" \
  --generated-manifest "$V5/inputs/generated-sources.json" \
  --cache "$V5/cache" --output "$RUN/replay" --jobs 8 \
  --replay-parse-runtime "$V5/runner" > "$RUN/replay.log" 2> "$RUN/replay.err"
code=$?
set -e
echo "$code" > "$RUN/replay-exit.txt"
find "$V5/cache" -type f -printf '%P %s %T@\n' | sort | sha256sum > "$RUN/cache-after.txt"
cmp "$RUN/cache-before.txt" "$RUN/cache-after.txt" && echo "cache unchanged"
echo "exit $code"; tail -1 "$RUN/replay.log"; grep -E "Maximum resident|Elapsed" "$RUN/replay.err"
