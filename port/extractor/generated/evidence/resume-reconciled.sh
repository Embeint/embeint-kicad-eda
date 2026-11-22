#!/usr/bin/env bash
set -u
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-230f667d-d7c5b42cde1e6a30
PORT_ROOT="$HOME/kicad-port"
if ! mkdir "$HOME/kicad-port.build-lock"; then exit 3; fi
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
printf '%s\n' "$$" > "$PORT_ROOT/finish.pid"
mkdir -p "$PORT_ROOT/reconciled-runner"
rsync -a --exclude=__pycache__ --exclude=/extractor/generated/ --exclude=/units/ "$ROOT/port/" "$PORT_ROOT/reconciled-runner/port/"
"$HOME/kicad-port-extractor-venv/bin/python" "$PORT_ROOT/reconciled-runner/port/tools/extract_graph.py" --compile-commands "$PORT_ROOT/extract-build/compile_commands.json" --source-root "$PORT_ROOT/extract-source" --original-source-root "$PORT_ROOT/extract-source" --manifest "$PORT_ROOT/source-manifest.json" --cache "$PORT_ROOT/reconciled-cache" --output "$PORT_ROOT/reconciled-output" --jobs 8 > "$PORT_ROOT/reconciled-extraction.log" 2>&1
code=$?
printf '%s\n' "$code" > "$PORT_ROOT/reconciled-exit-code.txt"
date -u > "$PORT_ROOT/finish-completed.txt"
exit "$code"