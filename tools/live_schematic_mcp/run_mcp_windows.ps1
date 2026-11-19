# SPDX-License-Identifier: GPL-3.0-or-later
#
# stdio entry point for MCP clients on Windows, for the native Windows KiCad (run_kicad_windows.ps1).
# The server finds KiCad's local named pipe itself; set KICAD_API_SOCKET to pick one explicitly.
# Register it with an MCP client as:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File C:\<path-to-fork>\tools\live_schematic_mcp\run_mcp_windows.ps1
#
# stdout is the MCP channel: this script writes nothing else to it.  Set up the Python environment
# first with scripts\setup_windows.ps1.

$ErrorActionPreference = 'Stop'
. ( Join-Path $PSScriptRoot 'scripts\common.ps1' )

try
{
    Assert-Python
    # Regenerates the protocol code after a pull that changed any .proto
    Update-Protos
}
catch
{
    [Console]::Error.WriteLine( "run_mcp_windows.ps1: $_" )
    exit 1
}

& $Python ( Join-Path $Mcp 'server.py' )
exit $LASTEXITCODE
