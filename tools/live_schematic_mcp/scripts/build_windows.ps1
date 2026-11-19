# SPDX-License-Identifier: GPL-3.0-or-later
#
# Build and install Embeint KiCad natively on Windows (x64, MSVC, vcpkg).
#
#   powershell -ExecutionPolicy Bypass -File tools\live_schematic_mcp\scripts\build_windows.ps1
#
# Everything lives under -Root (default C:\kb; keep it short, MSVC and vcpkg hit path limits):
#   vcpkg\    vcpkg tool checkout (cloned and bootstrapped on first run)
#   build\    CMake/Ninja build tree; vcpkg installs the manifest dependencies into build\vcpkg_installed
#   install\  the installed KiCad; run install\bin\kicad.exe
#
# The first run builds every dependency from source with vcpkg (several hours); later runs reuse
# vcpkg's binary cache and only rebuild what changed.  Requires Visual Studio with the "Desktop
# development with C++" workload (MSVC, Windows SDK, CMake and Ninja); tested with Visual Studio 2026.

[CmdletBinding()]
param(
    [string] $Root = 'C:\kb',
    [ValidateSet('Release', 'RelWithDebInfo', 'Debug')]
    [string] $Config = 'Release',
    [int] $Jobs = 0,
    # Configure only (also installs the vcpkg dependencies)
    [switch] $ConfigureOnly,
    # Skip the configure step when the build tree already exists
    [switch] $NoConfigure,
    # share\kicad directory of a KiCad install providing the stock libraries (default: newest
    # C:\Program Files\KiCad\<version>)
    [string] $StockLibraries
)

$ErrorActionPreference = 'Stop'
$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$Vcpkg = Join-Path $Root 'vcpkg'
$Build = Join-Path $Root 'build'
$Install = Join-Path $Root 'install'
$OverlayPorts = Join-Path $Repo 'tools\live_schematic_mcp\windows\vcpkg-overlay-ports'

function Invoke-Checked( [string] $Exe, [string[]] $Arguments )
{
    Write-Host "> $Exe $($Arguments -join ' ')"
    & $Exe @Arguments
    if( $LASTEXITCODE -ne 0 ) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

# Import the x64 MSVC developer environment into this process
function Import-VsEnvironment
{
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if( -not ( Test-Path $vswhere ) ) { throw 'Visual Studio is not installed (vswhere.exe not found)' }

    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
                     -property installationPath
    if( -not $vs ) { throw 'No Visual Studio installation with the MSVC x64 tools was found' }

    # vcvars64.bat looks for vswhere on PATH
    $env:PATH = "$( Split-Path $vswhere );$env:PATH"
    $vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
    foreach( $line in ( cmd.exe /d /c "`"$vcvars`" >nul && set" ) )
    {
        if( $line -match '^([^=]+)=(.*)$' ) { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
    }

    # VS ships CMake and Ninja; vcvars puts them on PATH
    foreach( $tool in 'cl', 'cmake', 'ninja' )
    {
        if( -not ( Get-Command $tool -ErrorAction SilentlyContinue ) ) { throw "$tool not found after vcvars64" }
    }

    Write-Host "Visual Studio: $vs"
}

# MSBuild finds the Universal CRT headers through the 64-bit registry's KitsRoot10.  Some other
# kits (e.g. the App Certification Kit) set that to C:\Program Files\Windows Kits\10, which holds
# no headers, while the SDK lives under Program Files (x86); MSBuild-built vcpkg ports then fail
# on stdio.h.  Point UCRTContentRoot at a kits root that has them, and let vcpkg pass it through.
function Set-UcrtContentRoot
{
    $roots = foreach( $key in 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots',
                              'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots' )
    {
        ( Get-ItemProperty $key -ErrorAction SilentlyContinue ).KitsRoot10
    }

    $root = $roots | Where-Object { $_ -and ( Test-Path ( Join-Path $_ 'Include\*\ucrt\stdio.h' ) ) } |
        Select-Object -First 1
    if( -not $root ) { throw 'No Windows 10/11 SDK with Universal CRT headers was found' }

    $env:UCRTContentRoot = $root
    $env:VCPKG_KEEP_ENV_VARS = ( @( $env:VCPKG_KEEP_ENV_VARS, 'UCRTContentRoot' ) |
        Where-Object { $_ } ) -join ';'
    Write-Host "Universal CRT: $root"
}

New-Item -ItemType Directory -Force $Root | Out-Null

if( -not ( Test-Path ( Join-Path $Vcpkg 'vcpkg.exe' ) ) )
{
    if( -not ( Test-Path $Vcpkg ) )
    {
        Invoke-Checked 'git' @( 'clone', '--depth', '1', 'https://github.com/microsoft/vcpkg.git', $Vcpkg )
    }

    Invoke-Checked 'cmd.exe' @( '/d', '/c', ( Join-Path $Vcpkg 'bootstrap-vcpkg.bat' ), '-disableMetrics' )
}

Import-VsEnvironment
Set-UcrtContentRoot
$env:VCPKG_ROOT = $Vcpkg
$env:VCPKG_DISABLE_METRICS = '1'

if( -not $NoConfigure -or -not ( Test-Path ( Join-Path $Build 'build.ninja' ) ) )
{
    Invoke-Checked 'cmake' @(
        '-G', 'Ninja',
        '-S', $Repo,
        '-B', $Build,
        "-DCMAKE_TOOLCHAIN_FILE=$Vcpkg\scripts\buildsystems\vcpkg.cmake",
        '-DVCPKG_TARGET_TRIPLET=x64-windows',
        '-DVCPKG_INSTALL_OPTIONS=--clean-after-build',
        "-DVCPKG_OVERLAY_PORTS=$OverlayPorts",
        "-DCMAKE_BUILD_TYPE=$Config",
        "-DCMAKE_INSTALL_PREFIX=$Install",
        '-DKICAD_BUILD_QA_TESTS=OFF',
        '-DKICAD_UPDATE_CHECK=OFF'
    )
}

if( $ConfigureOnly ) { return }

$buildArgs = @( '--build', $Build )
if( $Jobs -gt 0 ) { $buildArgs += @( '--parallel', "$Jobs" ) }
Invoke-Checked 'cmake' $buildArgs
Invoke-Checked 'cmake' @( '--install', $Build )

# Runtime dependencies: KiCad's install rules do not deploy vcpkg's DLLs on Windows (upstream does
# that in its separate packaging scripts), so copy them next to kicad.exe.
$triplet = Join-Path $Build 'vcpkg_installed\x64-windows'
$bin = Join-Path $Install 'bin'
Copy-Item ( Join-Path $triplet 'bin\*.dll' ) $bin -Force
Write-Host "Copied vcpkg runtime DLLs to $bin"

# Fontconfig looks for its configuration in ..\etc\fonts relative to its DLL
$fonts = Join-Path $triplet 'etc\fonts'
if( Test-Path $fonts )
{
    $dest = Join-Path $Install 'etc'
    New-Item -ItemType Directory -Force $dest | Out-Null
    Copy-Item $fonts $dest -Recurse -Force
}

$codeModels = Join-Path $triplet 'lib\ngspice'
if( Test-Path $codeModels )
{
    $dest = Join-Path $Install 'lib\ngspice'
    New-Item -ItemType Directory -Force $dest | Out-Null
    Copy-Item ( Join-Path $codeModels '*.cm' ) $dest -Force
}

# Stock symbol, footprint and 3D libraries are separate KiCad repositories and not part of this
# build.  Link them from the newest installed upstream KiCad, unless -StockLibraries names a
# share\kicad directory or the install already has them.
if( -not $StockLibraries )
{
    $StockLibraries = Get-ChildItem ( Join-Path $env:ProgramFiles 'KiCad' ) -Directory -ErrorAction SilentlyContinue |
        Sort-Object { [version]( $_.Name -replace '[^0-9.]', '' ) } -Descending |
        ForEach-Object { Join-Path $_.FullName 'share\kicad' } |
        Where-Object { Test-Path ( Join-Path $_ 'symbols' ) } |
        Select-Object -First 1
}

$share = Join-Path $Install 'share\kicad'
if( $StockLibraries )
{
    foreach( $dir in 'symbols', 'footprints', '3dmodels' )
    {
        $link = Join-Path $share $dir
        $target = Join-Path $StockLibraries $dir
        if( ( Test-Path $target ) -and -not ( Test-Path $link ) )
        {
            New-Item -ItemType Junction -Path $link -Target $target | Out-Null
            Write-Host "Linked $link -> $target"
        }
    }

    # Default global library tables offered by the first-run wizard
    foreach( $table in 'sym-lib-table', 'fp-lib-table' )
    {
        $dest = Join-Path $share "template\$table"
        $source = Join-Path $StockLibraries "template\$table"
        if( ( Test-Path $source ) -and -not ( Test-Path $dest ) ) { Copy-Item $source $dest }
    }
}
else
{
    Write-Warning 'No installed upstream KiCad found: stock symbol/footprint libraries are not linked'
}

# Start Menu shortcut for this user
$shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Embeint KiCad.lnk'
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut( $shortcut )
$lnk.TargetPath = Join-Path $bin 'kicad.exe'
$lnk.WorkingDirectory = $bin
$lnk.Description = 'Embeint KiCad (development build)'
$lnk.Save()
Write-Host "Start Menu shortcut: $shortcut"

Write-Host ''
Write-Host "Installed to $Install"
Write-Host "Run: $Install\bin\kicad.exe"
