# SPDX-License-Identifier: GPL-3.0-or-later
#
# Shared paths for the native Windows live MCP scripts.  Dot-source it; do not run it.
#
#   KICAD_WINDOWS_ROOT  root of build_windows.ps1 (default C:\kb):
#                       install\  the native KiCad
#                       venv\     the MCP's Python environment (setup_windows.ps1)
#                       live\     isolated KiCad configuration, test project, scratch libraries, test output
#   KICAD_LIVE_PYTHON   another Python interpreter for the MCP and its tests

$WinRoot = if( $env:KICAD_WINDOWS_ROOT ) { $env:KICAD_WINDOWS_ROOT } else { 'C:\kb' }
$Scripts = $PSScriptRoot
$Mcp = ( Resolve-Path ( Join-Path $Scripts '..' ) ).Path
$Repo = ( Resolve-Path ( Join-Path $Mcp '..\..' ) ).Path
$Install = Join-Path $WinRoot 'install'
$LiveRoot = Join-Path $WinRoot 'live'
$Python = if( $env:KICAD_LIVE_PYTHON ) { $env:KICAD_LIVE_PYTHON } else { Join-Path $WinRoot 'venv\Scripts\python.exe' }

# Regenerate the Python protocol code whenever a .proto is newer than the last generation
function Update-Protos
{
    $stamp = Join-Path $Mcp '_generated\.stamp'
    $newest = Get-ChildItem ( Join-Path $Repo 'api\proto' ) -Recurse -Filter *.proto |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1

    if( ( Test-Path $stamp ) -and ( Get-Item $stamp ).LastWriteTimeUtc -ge $newest.LastWriteTimeUtc ) { return }

    # Report on stderr only: stdout may be an MCP stdio channel (run_mcp_windows.ps1)
    $ErrorActionPreference = 'Continue'
    $out = & $Python ( Join-Path $Mcp 'generate_proto.py' ) 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if( $out ) { [Console]::Error.WriteLine( ( $out -join "`n" ) ) }
    if( $code -ne 0 ) { throw "generate_proto.py failed with exit code $code" }
    New-Item -ItemType File -Force $stamp | Out-Null
}

function Assert-Python
{
    if( -not ( Test-Path $Python ) )
    {
        throw "No MCP Python at $( $Python ); run tools\live_schematic_mcp\scripts\setup_windows.ps1 first"
    }
}
