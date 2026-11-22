#!/usr/bin/env bash
set -euo pipefail
work="$HOME/kicad-port/md5-envelope-regressions"
mkdir -p "$work"
test ! -e "$work/job.log"
cp /mnt/c/Users/KarlVonRichter/.t3/md5-envelope-export/validate.sh "$work/validate.sh"
setsid nohup bash "$work/validate.sh" > "$work/job.log" 2>&1 < /dev/null &
echo $! > "$work/job.pid"
echo "started $(cat "$work/job.pid")"
