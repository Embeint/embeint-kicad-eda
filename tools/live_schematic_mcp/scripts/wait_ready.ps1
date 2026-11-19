# SPDX-License-Identifier: GPL-3.0-or-later
#
# Wait (up to -TimeoutSeconds, default 120) until the native Windows KiCad answers the API; prints
# its version, open schematic and endpoint (the exact KICAD_API_SOCKET value for that instance).
# Use after run_kicad_windows.ps1.  Exits 1 on timeout.

[CmdletBinding()]
param(
    [int] $TimeoutSeconds = 120
)

$ErrorActionPreference = 'Stop'
. ( Join-Path $PSScriptRoot 'common.ps1' )
Assert-Python
Update-Protos

# readiness.py prints the documents and, last, the endpoint: the exact URL to use for
# KICAD_API_SOCKET (pipe names are literal).  It exits 75 while KiCad does not answer yet.
$deadline = ( Get-Date ).AddSeconds( $TimeoutSeconds )
$out = ''
Push-Location $Mcp
try
{
    do
    {
        # Native stderr would become PowerShell errors under 'Stop'; collect it as text instead
        $ErrorActionPreference = 'Continue'
        $lines = @( & $Python 'readiness.py' 2>&1 | ForEach-Object { "$_" } )
        $code = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if( $code -eq 0 ) { $lines | ForEach-Object { Write-Output $_ }; exit 0 }
        $out = $lines | Select-Object -Last 1

        # Not started yet, still loading (AS_NOT_READY) or no endpoint: wait.  Anything else is real.
        if( $code -ne 75 ) { break }
        Start-Sleep -Seconds 2
    } while( ( Get-Date ) -lt $deadline )
}
finally
{
    Pop-Location
}

[Console]::Error.WriteLine( "KiCad did not answer: $out" )
exit 1
