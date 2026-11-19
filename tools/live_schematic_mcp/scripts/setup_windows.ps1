# SPDX-License-Identifier: GPL-3.0-or-later
#
# Set up the live MCP for the native Windows KiCad (build_windows.ps1): a Python virtual environment
# with requirements.txt in <root>\venv, and the protocol code generated from this checkout.
# Idempotent: rerun after a pull that changes requirements.txt or a .proto.
#
#   powershell -ExecutionPolicy Bypass -File tools\live_schematic_mcp\scripts\setup_windows.ps1 [-BasePython <python.exe>]
#
# Needs a 64-bit CPython 3.11 or newer (python.org or the py launcher).  The root is
# KICAD_WINDOWS_ROOT (default C:\kb); KICAD_LIVE_PYTHON selects another existing environment instead.

[CmdletBinding()]
param(
    # Interpreter used to create the virtual environment (default: py -3, then python on PATH)
    [string] $BasePython
)

$ErrorActionPreference = 'Stop'
. ( Join-Path $PSScriptRoot 'common.ps1' )

if( -not ( Test-Path $Python ) )
{
    if( $env:KICAD_LIVE_PYTHON ) { throw "KICAD_LIVE_PYTHON does not exist: $env:KICAD_LIVE_PYTHON" }

    $base = @()
    if( $BasePython ) { $base = @( $BasePython ) }
    elseif( Get-Command py -ErrorAction SilentlyContinue ) { $base = @( 'py', '-3' ) }
    elseif( Get-Command python -ErrorAction SilentlyContinue ) { $base = @( 'python' ) }
    else { throw 'No Python found: install 64-bit Python 3.11+ from python.org or pass -BasePython' }

    $venv = Split-Path ( Split-Path $Python )
    Write-Host "Creating $venv"
    $exe = $base[0]
    $baseArgs = @( $base | Select-Object -Skip 1 )
    & $exe @baseArgs -m venv $venv
    if( $LASTEXITCODE -ne 0 ) { throw "Could not create the virtual environment $venv" }
}

& $Python -m pip install -q --disable-pip-version-check -r ( Join-Path $Mcp 'requirements.txt' )
if( $LASTEXITCODE -ne 0 ) { throw 'pip install -r requirements.txt failed' }

# Always regenerate here, so a stale stamp can never hide missing modules
Remove-Item ( Join-Path $Mcp '_generated\.stamp' ) -ErrorAction SilentlyContinue
Update-Protos

Write-Host ''
Write-Host "Ready.  Start KiCad:  $Mcp\run_kicad_windows.ps1"
Write-Host "Register the MCP:     powershell -NoProfile -ExecutionPolicy Bypass -File $Mcp\run_mcp_windows.ps1"
