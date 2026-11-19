#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Wait (up to 2 minutes) until the development KiCad answers the API; prints its version, open
# schematic and endpoint.  Use after run_kicad_wsl.sh.  Exits 1 on timeout or on an error that
# waiting cannot fix (readiness.py decides from error codes which failures mean "not yet").

set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
ensure_protos
cd "$MCP"
out=""
deadline=$((SECONDS + 120))
while (( SECONDS < deadline )); do
    out=$("$PYTHON" readiness.py 2>&1)
    case $? in
        0) echo "$out"; exit 0 ;;
        75) sleep 2 ;;
        *) break ;;
    esac
done
echo "KiCad did not answer: $(tail -1 <<<"$out")" >&2
exit 1
