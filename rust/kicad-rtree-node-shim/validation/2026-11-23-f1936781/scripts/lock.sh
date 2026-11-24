#!/usr/bin/env bash
# Run a command under the shared build lock; release only the lock acquired here.
set -uo pipefail
lock="$HOME/kicad-port.build-lock"
until mkdir "$lock" 2>/dev/null; do echo "waiting for $lock" >&2; sleep 10; done
trap 'rmdir "$lock"' EXIT
"$@"
