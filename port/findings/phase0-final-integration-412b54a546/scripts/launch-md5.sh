#!/usr/bin/env bash
# Start the resumable MD5 follow-up proof detached; report a busy lock instead of waiting.
set -euo pipefail
rev=${1:?revision}
work="$HOME/kicad-port/phase0-final-cont-$rev"
if [[ -d "$HOME/kicad-port.build-lock" ]]; then
    echo "shared lock busy; not starting"; ls -ld --time-style=full-iso "$HOME/kicad-port.build-lock"; exit 3
fi
free -g | head -2
mkdir -p "$work"
cp /mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont/run-md5.sh "$work/run.sh"
nohup setsid bash "$work/run.sh" "$rev" >> "$work/run.log" 2>&1 < /dev/null &
echo $! > "$work/run.pid"
echo "started PID $(cat "$work/run.pid"), log $work/run.log"
