#!/usr/bin/env bash
set -euo pipefail
N="$HOME/kicad-port/freeze-symlink-preflight"
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-04614140-10140941291c27ae
S=/mnt/c/Users/KarlVonRichter/.t3/phase0-freeze-symlink
PY="$HOME/kicad-port-extractor-venv/bin/python"
mkdir "$N"
mkdir -p "$N/current/port"
rsync -a --exclude generated --exclude __pycache__ "$ROOT/port/extractor/" "$N/current/port/extractor/"
rsync -a --exclude __pycache__ "$ROOT/port/tools/" "$N/current/port/tools/"
mkdir "$HOME/kicad-port.build-lock" || exit 3
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
export PYTHONDONTWRITEBYTECODE=1
(cd "$N/current" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v) > "$N/tests-wsl.log" 2>&1
cp -a "$N/current" "$N/old-control"
cp "$S/old-generated_sources.py" "$N/old-control/port/extractor/generated_sources.py"
if (cd "$N/old-control" && "$PY" -m unittest discover -s port/extractor -p 'test_*.py' -k test_freeze_cli_preserves_tool_symlink_invocation_names -v) > "$N/old-control.log" 2>&1; then
    echo 'Old CLI unexpectedly passed the symlink dispatch regression' >&2
    exit 1
fi
grep -F 'tool-manager: invoke through a tool symlink' "$N/old-control.log"
tail -n 5 "$N/tests-wsl.log"
