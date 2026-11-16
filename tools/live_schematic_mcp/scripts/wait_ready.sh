#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Wait (up to 2 minutes) until the development KiCad answers the API; prints its version and
# open schematic.  Use after run_kicad_wsl.sh.

set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
ensure_protos
cd "$MCP"
for _ in $(seq 60); do
    out=$("$PYTHON" -c 'from client import LiveSchematic; print(LiveSchematic().documents())' 2>&1 | tail -1)
    case "$out" in
        *NOT_READY*|*Connection*|*Timed*|*refused*|*No\ such*) sleep 2 ;;
        *) echo "$out"; exit 0 ;;
    esac
done
echo "KiCad did not answer: $out" >&2
exit 1
