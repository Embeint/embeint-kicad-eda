#!/usr/bin/env bash
# Detach a script under the own root: launch.sh LOGNAME SCRIPT ARGS...
set -euo pipefail
root="$HOME/kicad-port/pilot-resistors-private-keys"
log=${1:?log name}
shift
cd "$root"
nohup setsid bash "$@" > "$root/$log.log" 2>&1 < /dev/null &
echo $! > "$root/$log.pid"
echo "$log pid $(cat "$root/$log.pid")"
