#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# stdio entry point for MCP clients. KiCad's IPC socket is a Unix socket inside WSL, so the
# server must run in the same distro as the development KiCad. From Windows, register:
#
#   wsl.exe -d Ubuntu -- bash /mnt/c/<path-to-fork>/tools/live_schematic_mcp/run_mcp_wsl.sh

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON="${KICAD_LIVE_PYTHON:-${KICAD_LIVE_ROOT:-$HOME/kicad-live}/venv/bin/python}"

if [[ ! -d "$HERE/_generated/schematic" ]]; then
    "$PYTHON" "$HERE/generate_proto.py" >&2
fi

exec "$PYTHON" "$HERE/server.py"
