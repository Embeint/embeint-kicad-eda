#!/usr/bin/env bash
# Start the resumable continued integration proof detached, so it survives this session.
set -euo pipefail
rev=${1:?revision}
work="$HOME/kicad-port/phase0-final-cont-$rev"
mkdir -p "$work"
cp /mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont/run.sh "$work/run.sh"
nohup setsid bash "$work/run.sh" "$rev" >> "$work/run.log" 2>&1 < /dev/null &
echo $! > "$work/run.pid"
echo "started PID $(cat "$work/run.pid"), log $work/run.log"
