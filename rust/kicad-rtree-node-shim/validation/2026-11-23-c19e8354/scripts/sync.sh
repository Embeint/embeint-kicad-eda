#!/usr/bin/env bash
# Snapshot the Windows worktree onto the Linux filesystem (never build on /mnt/c).
set -euo pipefail
W="$HOME/kicad-port/rtree-p003"
mkdir -p "$W/repo"
rsync -a --delete --exclude .git --exclude rust/target --exclude .port-deps \
    --exclude .port-venv --exclude .west /mnt/c/kb/worktrees/rtree-p003/ "$W/repo/"
if [ ! -d "$W/repo/.port-deps/solidcore" ]; then
    cp -a --reflink=auto "$HOME/kicad-port/md5-corrected-graph/repo/.port-deps" "$W/repo/"
fi
echo "synced to $W/repo"
