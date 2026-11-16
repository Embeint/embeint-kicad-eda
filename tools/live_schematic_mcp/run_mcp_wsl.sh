#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# stdio entry point for MCP clients. KiCad's IPC socket is a Unix socket inside WSL, so the
# server must run in the same distro as the development KiCad. From Windows, register:
#
#   wsl.exe -d Ubuntu -- bash /mnt/c/<path-to-fork>/tools/live_schematic_mcp/run_mcp_wsl.sh

set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/scripts/common.sh"

# Regenerates the protocol code after a pull that changed any .proto
ensure_protos

exec "$PYTHON" "$MCP/server.py"
