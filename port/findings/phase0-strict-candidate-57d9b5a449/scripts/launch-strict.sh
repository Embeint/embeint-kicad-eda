#!/usr/bin/env bash
# Start the prospective strict-gate run detached; report a busy lock instead of waiting.
set -euo pipefail
rev=${1:?revision}
work="$HOME/kicad-port/phase0-strict-candidate-$rev"
if [[ -d "$HOME/kicad-port.build-lock" ]]; then
    echo "shared lock busy; not starting"; ls -ld --time-style=full-iso "$HOME/kicad-port.build-lock"; exit 3
fi
mkdir -p "$work"
cp /mnt/c/Users/KarlVonRichter/.t3/phase0-final-cont/run-strict.sh "$work/run.sh"
nohup setsid bash "$work/run.sh" "$rev" >> "$work/run.log" 2>&1 < /dev/null &
echo $! > "$work/run.pid"
echo "started PID $(cat "$work/run.pid"), log $work/run.log"
