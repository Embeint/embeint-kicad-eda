# SPDX-License-Identifier: GPL-3.0-or-later
#
# Start the native Windows Embeint KiCad (build_windows.ps1) with the IPC API server enabled, for
# the live MCP.  Uses an isolated configuration directory, so it never touches the settings of an
# installed KiCad or of the same build started from the Start Menu.
#
#   run_kicad_windows.ps1 [-Project <file.kicad_pro | file.kicad_sch>] [-Fresh] [-Wait]
#       default: creates <root>\live\projects\live-demo and opens it in the project manager
#   run_kicad_windows.ps1 -Stop
#       stops the KiCad this script started (unsaved edits are lost); never touches another KiCad
#
# A project opens KiCad's project manager, which hosts the schematic AND PCB editors in one process
# on one API endpoint; the MCP opens the editors (live_schematic_open, live_pcb_open).  A .kicad_sch
# opens the schematic editor on its own: schematic, symbol and footprint tools work, PCB tools don't.
#
#   -Fresh  recreate the live-demo test project (discarding anything saved into it)
#   -Wait   wait until the API answers (scripts\wait_ready.ps1) before returning
#
# KiCad starts in the background; its process ID is written to <root>\live\kicad.pid.  The root is
# KICAD_WINDOWS_ROOT (default C:\kb), as for build_windows.ps1.

[CmdletBinding()]
param(
    [string] $Project,
    [switch] $Fresh,
    [switch] $Wait,
    [switch] $Stop
)

$ErrorActionPreference = 'Stop'
. ( Join-Path $PSScriptRoot 'scripts\common.ps1' )

$bin = Join-Path $Install 'bin'
$pidFile = Join-Path $LiveRoot 'kicad.pid'

# The KiCad this script started, if it is still running (never another KiCad the user opened)
function Get-LiveKiCad
{
    if( -not ( Test-Path $pidFile ) ) { return $null }
    $process = Get-Process -Id ( [int]( Get-Content $pidFile -Raw ) ) -ErrorAction SilentlyContinue
    if( $process -and $process.Path -and $process.Path.StartsWith( $bin, 'OrdinalIgnoreCase' ) )
    {
        return $process
    }
    return $null
}

if( $Stop )
{
    $process = Get-LiveKiCad
    if( $process )
    {
        # Unsaved edits in the test project are discarded; nothing is saved implicitly
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit( 30000 ) | Out-Null
        Write-Host "Stopped KiCad (PID $( $process.Id ))"
    }
    Remove-Item $pidFile -ErrorAction SilentlyContinue
    return
}

if( -not ( Test-Path ( Join-Path $bin 'kicad.exe' ) ) )
{
    throw "Native KiCad not installed at $Install (run scripts\build_windows.ps1)"
}
if( Get-LiveKiCad )
{
    throw "The live KiCad started by this script is still running (PID $( ( Get-LiveKiCad ).Id )); use -Stop first"
}

# On Windows the API endpoint is the named pipe \\.\pipe\<temp>\kicad\api.sock.  A second KiCad
# cannot take it and runs without an API, so the MCP would reach the first one instead: refuse.
# The pipe name is the literal temp path, which may be the short (8.3) or the long spelling depending
# on how a KiCad was started, so check every spelling of this user's temp directory.
$temps = @( $env:TEMP, $env:TMP, [IO.Path]::GetTempPath(),
            [Environment]::GetEnvironmentVariable( 'TEMP', 'User' ) ) |
    Where-Object { $_ } | ForEach-Object { Join-Path $_ 'kicad\api.sock' }
$taken = [IO.Directory]::GetFiles( '\\.\pipe\' ) |
    ForEach-Object { $_.Substring( '\\.\pipe\'.Length ) } |
    Where-Object { $temps -contains $_ } | Select-Object -First 1
if( $taken )
{
    throw ( "Another KiCad with the API enabled already serves $taken.  Close it (or disable its API " +
            "server) first: this KiCad could not serve the API and the MCP would reach the other one." )
}

$share = Join-Path $Install 'share\kicad'
$configHome = Join-Path $LiveRoot 'config'
$configDir = Join-Path $configHome '10.99'
New-Item -ItemType Directory -Force $configDir | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding $false

# Enable the API server and acknowledge the first-run privacy prompts; otherwise the modal setup
# wizard blocks startup and the API answers AS_NOT_READY.  Merged so other preferences are kept.
$commonPath = Join-Path $configDir 'kicad_common.json'
# Windows PowerShell 5.1 otherwise serialises some arrays as { value, Count } objects
Remove-TypeData System.Array -ErrorAction SilentlyContinue
$common = $null
if( Test-Path $commonPath )
{
    try { $common = Get-Content -Raw $commonPath | ConvertFrom-Json } catch { $common = $null }
}
if( -not $common ) { $common = New-Object PSObject }

function Set-JsonValue( $Object, [string] $Section, [string] $Name, $Value )
{
    if( -not $Object.PSObject.Properties[$Section] )
    {
        $Object | Add-Member -NotePropertyName $Section -NotePropertyValue ( New-Object PSObject )
    }
    $Object.$Section | Add-Member -NotePropertyName $Name -NotePropertyValue $Value -Force
}

Set-JsonValue $common 'api' 'enable_server' $true
Set-JsonValue $common 'do_not_show_again' 'data_collection_prompt' $true
Set-JsonValue $common 'do_not_show_again' 'update_check_prompt' $true

# The project manager offers, in a modal dialog, to (re)register the bundled Explorer 3D model
# preview handler whenever its version differs from the registered one, i.e. after most rebuilds.
# That dialog blocks the API (AS_BUSY), and this KiCad must not change the user's shell
# registration, so decline the offer for the installed handler.  KiCad keys the decline by
# windows-preview-handler-v1-<file version of the handler DLL>.
$previewer = Join-Path $bin 'kicad_3d_model_previewer.dll'
$requirement = 'windows-preview-handler-v1'
if( Test-Path $previewer )
{
    $version = ( Get-Item $previewer ).VersionInfo
    $requirement += "-$( $version.FileMajorPart ).$( $version.FileMinorPart ).$( $version.FileBuildPart )." +
                    "$( $version.FilePrivatePart )"
}
$declined = @( @( $common.system.declined_model_preview_requirements ) + $requirement |
               Where-Object { $_ } | Select-Object -Unique )
Set-JsonValue $common 'system' 'declined_model_preview_requirements' $declined
[IO.File]::WriteAllText( $commonPath, ( $common | ConvertTo-Json -Depth 32 ), $utf8 )

# Global library tables referencing the stock libraries (versioned variables).  All three must exist
# and parse, or the setup wizard runs.
foreach( $table in 'sym-lib-table', 'fp-lib-table' )
{
    $dest = Join-Path $configDir $table
    $source = Join-Path $share "template\$table"
    if( -not ( Test-Path $dest ) -and ( Test-Path $source ) )
    {
        Copy-Item $source $dest
        Set-ItemProperty $dest -Name IsReadOnly -Value $false
    }
}
$designBlocks = Join-Path $configDir 'design-block-lib-table'
if( -not ( Test-Path $designBlocks ) )
{
    [IO.File]::WriteAllText( $designBlocks, "(design_block_lib_table`n  (version 7)`n)`n", $utf8 )
}

# build_windows.ps1 links the stock libraries into the install as junctions.  Point KiCad at their
# targets: resolving the junction on every library access makes symbol searches about twice as slow.
function Resolve-LibraryDir( [string] $Path )
{
    $item = Get-Item $Path -ErrorAction SilentlyContinue
    if( $item -and $item.LinkType -eq 'Junction' -and $item.Target ) { return @( $item.Target )[0] }
    return $Path
}

# Process environment only: KiCad started here inherits it, nothing is stored for the user
$env:KICAD_CONFIG_HOME = $configHome
foreach( $ver in 10, 11 )
{
    Set-Item "env:KICAD$( $ver )_SYMBOL_DIR" ( Resolve-LibraryDir ( Join-Path $share 'symbols' ) )
    Set-Item "env:KICAD$( $ver )_FOOTPRINT_DIR" ( Resolve-LibraryDir ( Join-Path $share 'footprints' ) )
    Set-Item "env:KICAD$( $ver )_3DMODEL_DIR" ( Resolve-LibraryDir ( Join-Path $share '3dmodels' ) )
    Set-Item "env:KICAD$( $ver )_TEMPLATE_DIR" ( Join-Path $share 'template' )
}

if( -not $Project )
{
    $demo = Join-Path $LiveRoot 'projects\live-demo'
    $Project = Join-Path $demo 'live-demo.kicad_pro'

    if( $Fresh -and ( Test-Path $demo ) ) { Remove-Item -Recurse -Force $demo }

    if( -not ( Test-Path $Project ) )
    {
        New-Item -ItemType Directory -Force $demo | Out-Null
        [IO.File]::WriteAllText( $Project,
            "{`n  `"meta`": { `"filename`": `"live-demo.kicad_pro`", `"version`": 3 }`n}`n", $utf8 )
        $sch = "(kicad_sch (version 20260306) (generator `"eeschema`") (generator_version `"10.0`")`n" +
               "  (uuid $( [guid]::NewGuid() ))`n" +
               "  (paper `"A4`")`n" +
               "  (lib_symbols)`n" +
               "  (sheet_instances (path `"/`" (page `"1`")))`n" +
               ")`n"
        [IO.File]::WriteAllText( ( Join-Path $demo 'live-demo.kicad_sch' ), $sch, $utf8 )
        Write-Host "Created test project $Project"
    }
}

$Project = ( Resolve-Path $Project ).Path

# A schematic opens straight in the schematic editor (which hosts its own API server);
# a project opens the project manager (schematic and PCB editors, one API server).
$exe = if( $Project -like '*.kicad_sch' ) { 'eeschema.exe' } else { 'kicad.exe' }
$process = Start-Process -FilePath ( Join-Path $bin $exe ) -ArgumentList "`"$Project`"" `
                         -WorkingDirectory $bin -PassThru
[IO.File]::WriteAllText( $pidFile, "$( $process.Id )", $utf8 )
Write-Host "Started $exe (PID $( $process.Id )) with $Project"
Write-Host "Configuration: $configDir"

if( $Wait )
{
    & ( Join-Path $Scripts 'wait_ready.ps1' )
    if( $LASTEXITCODE -ne 0 ) { exit $LASTEXITCODE }
}
