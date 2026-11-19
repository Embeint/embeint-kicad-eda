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

# The endpoint goes last: it is the exact URL to use for KICAD_API_SOCKET (pipe names are literal)
# (no double quotes: Windows PowerShell 5.1 drops them from native command arguments)
$probe = "from client import LiveSchematic; c = LiveSchematic(timeout_ms=5000); d = c.documents(); " +
         "print(d); print('endpoint:', c.socket_url)"
$deadline = ( Get-Date ).AddSeconds( $TimeoutSeconds )
$out = ''
Push-Location $Mcp
try
{
    do
    {
        # Native stderr would become PowerShell errors under 'Stop'; collect it as text instead
        $ErrorActionPreference = 'Continue'
        $lines = @( & $Python -c $probe 2>&1 | ForEach-Object { "$_" } )
        $code = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if( $code -eq 0 ) { $lines | ForEach-Object { Write-Output $_ }; exit 0 }
        $out = $lines | Select-Object -Last 1

        # Not started yet, still loading (AS_NOT_READY) or no endpoint: wait.  Anything else is real.
        if( $out -notmatch 'NOT_READY|Timed out|Timeout|Connection|refused|default_missing|unavailable|No such' )
        {
            break
        }
        Start-Sleep -Seconds 2
    } while( ( Get-Date ) -lt $deadline )
}
finally
{
    Pop-Location
}

[Console]::Error.WriteLine( "KiCad did not answer: $out" )
exit 1
