#!/usr/bin/env bash
set -u
HOME_PORT="$HOME/kicad-port"
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-230f667d-d7c5b42cde1e6a30
if ! mkdir "$HOME/kicad-port.build-lock"; then exit 3; fi
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
printf '%s\n' "$$" > "$HOME_PORT/finish.pid"
PY="$HOME/kicad-port-extractor-venv/bin/python"
date -u > "$HOME_PORT/finish-started.txt"
"$PY" "$HOME_PORT/resume-raw.py" --compile-commands "$HOME/kicad-live/build/compile_commands.json" --source-root "$HOME_PORT/extract-source" --original-source-root "$HOME/kicad-live/src" --manifest "$HOME_PORT/source-manifest.json" --cache "$HOME_PORT/extract-cache" --output "$HOME_PORT/raw-output" --jobs 8 >> "$HOME_PORT/extraction.log" 2>&1
raw_code=$?
printf '%s\n' "$raw_code" > "$HOME_PORT/raw-exit-code.txt"
# Keep raw diagnostics and graph intact even though deleted commands fail parsing.
mkdir -p "$HOME_PORT/reconciled-runner"
rsync -a --exclude=__pycache__ "$ROOT/port/" "$HOME_PORT/reconciled-runner/port/"
"$PY" "$HOME_PORT/reconciled-runner/port/tools/extract_graph.py" --compile-commands "$HOME_PORT/extract-build/compile_commands.json" --source-root "$HOME_PORT/extract-source" --original-source-root "$HOME_PORT/extract-source" --manifest "$HOME_PORT/source-manifest.json" --cache "$HOME_PORT/reconciled-cache" --output "$HOME_PORT/reconciled-output" --jobs 8 > "$HOME_PORT/reconciled-extraction.log" 2>&1
reconciled_code=$?
printf '%s\n' "$reconciled_code" > "$HOME_PORT/reconciled-exit-code.txt"
date -u > "$HOME_PORT/finish-completed.txt"
exit "$reconciled_code"