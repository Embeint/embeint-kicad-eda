#!/usr/bin/env bash
set -eu
W=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-cc7d0fad-0bc9e64b3c386645
RUN="$HOME/kicad-port/shape-collision"
mkdir -p "$RUN/runner/port/extractor" "$RUN/runner/port/tools"
rsync -a --delete --exclude generated --exclude __pycache__ "$W/port/extractor/" "$RUN/runner/port/extractor/"
rsync -a --delete --exclude __pycache__ "$W/port/tools/" "$RUN/runner/port/tools/"
cd "$RUN/runner"
"$HOME/kicad-port-extractor-venv/bin/python" -m unittest discover -s port/extractor -p 'test_*.py' ${1:+-k "$1"} > "$RUN/tests${1:+-$1}.log" 2>&1 && echo PASS || echo FAIL
tail -5 "$RUN/tests${1:+-$1}.log"
