# SPDX-License-Identifier: GPL-3.0-or-later
#
#   scripts\run_tests.ps1          unit tests (no KiCad needed)
#   scripts\run_tests.ps1 -Live    also the end-to-end tests against the running native Windows KiCad
#                                  (start it with run_kicad_windows.ps1: the project manager with the
#                                  live-demo project, which e2e_pcb needs).  The SolidCraft handoff
#                                  test runs when SOLIDCRAFT_CLI points at solidcraft-cli.
#
# The Windows counterpart of run_tests.sh.  The live tests never save the open schematic; library
# tests use scratch libraries in <root>\live\libs and remove them.  They run library tests first on
# purpose: adding and removing global libraries must not break placing stock symbols afterwards.
# Images and logs go to KICAD_LIVE_TEST_OUTPUT (default <root>\live\test-output).  Exits 1 on failure.

[CmdletBinding()]
param(
    [switch] $Live
)

$ErrorActionPreference = 'Stop'
. ( Join-Path $PSScriptRoot 'common.ps1' )
Assert-Python
Update-Protos
Set-Location $Mcp

$units = Get-ChildItem $Mcp -Filter 'test_*.py' | ForEach-Object Name
& $Python -m pytest -q -p no:cacheprovider @units
if( $LASTEXITCODE -ne 0 ) { exit 1 }
if( -not $Live ) { exit 0 }

& ( Join-Path $Scripts 'wait_ready.ps1' ) | Out-Null
if( $LASTEXITCODE -ne 0 ) { exit 1 }

$out = if( $env:KICAD_LIVE_TEST_OUTPUT ) { $env:KICAD_LIVE_TEST_OUTPUT } else { Join-Path $LiveRoot 'test-output' }
$libs = Join-Path $LiveRoot 'libs'
New-Item -ItemType Directory -Force $out, $libs | Out-Null

# The installed 3D library (linked from upstream KiCad by build_windows.ps1) supplies the STEP model
$step = Join-Path $Install 'share\kicad\3dmodels\Package_SO.3dshapes\SOIC-8_3.9x4.9mm_P1.27mm.step'
$arguments = @{
    'e2e_symbols'       = @( $out, $libs )
    'e2e_footprints'    = @( $out, $libs ) + @( if( Test-Path $step ) { $step } )
    'e2e_model_handoff' = @( $out, $libs )
}

$tests = @( 'e2e_symbols', 'e2e_footprints' )
if( $env:SOLIDCRAFT_CLI ) { $tests += 'e2e_model_handoff' }
$tests += @( 'e2e_smoke', 'e2e_features', 'e2e_item_roundtrip', 'e2e_pcb', 'e2e_design', 'e2e_design_checks' )

$env:PYTHONUTF8 = '1'
$failed = 0
foreach( $test in $tests )
{
    Write-Host "--- $test"
    $testArgs = if( $arguments.ContainsKey( $test ) ) { $arguments[$test] } else { @( $out ) }
    $log = Join-Path $out "$test.log"
    # Byte-for-byte UTF-8 logs: no PowerShell re-encoding or error records around stderr
    # Trailing backslashes are doubled, or a directory such as C:\out\ would escape its closing quote
    $argLine = ( @( "$test.py" ) + $testArgs |
                 ForEach-Object { '"' + ( "$_" -replace '(\\+)$', '$1$1' ) + '"' } ) -join ' '
    $process = Start-Process -FilePath $Python -ArgumentList $argLine -NoNewWindow -Wait -PassThru `
                            -RedirectStandardOutput $log -RedirectStandardError "$log.stderr"
    Get-Content "$log.stderr" -Encoding UTF8 | Add-Content $log -Encoding UTF8
    Remove-Item "$log.stderr"
    if( $process.ExitCode -ne 0 ) { $failed = 1 }
    Get-Content $log -Tail 3 -Encoding UTF8
}
if( -not $env:SOLIDCRAFT_CLI ) { Write-Host '(skipped e2e_model_handoff: set SOLIDCRAFT_CLI)' }
Write-Host "images and logs: $out"
exit $failed
