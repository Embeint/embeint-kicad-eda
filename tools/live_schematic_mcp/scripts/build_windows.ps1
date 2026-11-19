# SPDX-License-Identifier: GPL-3.0-or-later
#
# Build and install Embeint KiCad natively on Windows (x64, MSVC, vcpkg).
#
#   powershell -ExecutionPolicy Bypass -File tools\live_schematic_mcp\scripts\build_windows.ps1
#
# Everything lives under -Root (default C:\kb; keep it short, MSVC and vcpkg hit path limits):
#   vcpkg\    vcpkg tool checkout (cloned and bootstrapped on first run)
#   build\    CMake/Ninja build tree; vcpkg installs the manifest dependencies into build\vcpkg_installed;
#             build\mcp holds the Python environment that freezes the MCP server
#   install\  the installed KiCad; run install\bin\kicad.exe.  install\mcp\embeint-kicad-mcp.exe is the
#             bundled MCP server (stdio), with its own Python runtime and licences
#
# The first run builds every dependency from source with vcpkg (several hours); later runs reuse
# vcpkg's binary cache and only rebuild what changed.  Requires Visual Studio with the "Desktop
# development with C++" workload (MSVC, Windows SDK, CMake and Ninja); tested with Visual Studio 2026.
# Bundling the MCP also needs 64-bit Python 3.11+ (python.org or the py launcher) at build time only.

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
    [string] $StockLibraries,
    # Do not bundle the MCP server
    [switch] $NoMcp,
    # Only (re)bundle the MCP server into an existing install
    [switch] $McpOnly,
    # Python used to create the MCP build environment (default: py -3, then python on PATH)
    [string] $McpPython
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

# Copy files whose size or time differ.  Unchanged files are skipped, so a rerun works while a KiCad
# started from the install holds its DLLs open (Windows refuses to overwrite a loaded DLL).
function Copy-Changed( [string] $Pattern, [string] $Destination )
{
    foreach( $file in Get-ChildItem $Pattern -File )
    {
        $target = Join-Path $Destination $file.Name
        $existing = Get-Item $target -ErrorAction SilentlyContinue
        if( $existing -and $existing.Length -eq $file.Length -and
            $existing.LastWriteTimeUtc -eq $file.LastWriteTimeUtc )
        {
            continue
        }
        Copy-Item $file.FullName $target -Force
    }
}

# Why a Python command cannot freeze the MCP (it must be 64-bit CPython 3.11+), or '' if it can.
# $Command is the executable and its leading arguments, e.g. @( 'py', '-3' ).
function Get-McpPythonProblem( [string[]] $Command )
{
    $probe = "import struct, sys; print(sys.version_info[0], sys.version_info[1], struct.calcsize('P') * 8)"
    $ErrorActionPreference = 'Continue'
    try
    {
        $out = "$( & $Command[0] @( @( $Command | Select-Object -Skip 1 ) + @( '-c', $probe ) ) 2>$null )".Trim()
        $code = $LASTEXITCODE
    }
    catch
    {
        $out, $code = '', 1
    }
    $ErrorActionPreference = 'Stop'

    if( $code -ne 0 -or $out -notmatch '^(\d+) (\d+) (\d+)$' ) { return "$( $Command -join ' ' ) does not run" }

    $major, $minor, $bits = [int] $Matches[1], [int] $Matches[2], [int] $Matches[3]
    if( $major -ne 3 -or $minor -lt 11 -or $bits -ne 64 )
    {
        return "$( $Command -join ' ' ) is Python $major.$minor ($bits-bit); bundling the MCP needs 64-bit Python 3.11+"
    }

    return ''
}

# The Python that creates build\mcp\venv, or $null when the existing environment can be reused (it
# exists, -McpPython is not given and it passes Get-McpPythonProblem).  Throws when no suitable
# Python is found, so a full build stops before compiling rather than hours later.
function Resolve-McpBasePython
{
    $venvPython = Join-Path $Build 'mcp\venv\Scripts\python.exe'
    if( -not $McpPython -and ( Test-Path $venvPython ) )
    {
        $problem = Get-McpPythonProblem @( $venvPython )
        if( -not $problem ) { return $null }
        Write-Warning "Recreating the MCP build environment: $problem"
    }

    $base = @()
    if( $McpPython ) { $base = @( $McpPython ) }
    elseif( Get-Command py -ErrorAction SilentlyContinue ) { $base = @( 'py', '-3' ) }
    elseif( Get-Command python -ErrorAction SilentlyContinue ) { $base = @( 'python' ) }
    else { throw 'Bundling the MCP needs 64-bit Python 3.11+ (python.org or -McpPython); or pass -NoMcp' }

    $problem = Get-McpPythonProblem $base
    if( $problem ) { throw "$problem (choose one with -McpPython, or pass -NoMcp)" }

    return , $base
}

# Replace the folder $Destination with $New as a whole, or change nothing.  Remove-Item would delete
# what it can and then fail on a file in use (an MCP client's running embeint-kicad-mcp.exe keeps
# its executable and DLLs mapped), leaving a broken install.  A rename alone does not detect that
# either: Windows renames a folder whose programs are running.  So refuse while any file in it is
# open (opening it exclusively fails then, also for a running program's images), rename it aside,
# move the new folder in (renaming the old one back if that fails), and only then delete the old one.
function Update-Folder( [string] $New, [string] $Destination )
{
    $aside = $null
    if( Test-Path -LiteralPath $Destination )
    {
        $busy = @( foreach( $file in Get-ChildItem -LiteralPath $Destination -Recurse -File -Force )
        {
            $access = if( $file.IsReadOnly ) { 'Read' } else { 'ReadWrite' }
            try { [IO.File]::Open( $file.FullName, 'Open', $access, 'None' ).Dispose() }
            catch { $file.FullName }
        } )

        if( $busy.Count -gt 0 )
        {
            throw "Cannot replace $( $Destination ): $( $busy.Count ) file(s) are in use, e.g. $( $busy[0] ). " +
                  'Close the MCP client running embeint-kicad-mcp.exe from it and run again; nothing was changed.'
        }

        $aside = "$Destination.old-" + ( Get-Date -Format 'yyyyMMddHHmmss' )
        try { Rename-Item -LiteralPath $Destination ( Split-Path -Leaf $aside ) }
        catch { throw "Cannot replace $( $Destination ) (a file in it is in use); nothing was changed: $_" }
    }

    try { Move-Item -LiteralPath $New $Destination }
    catch
    {
        if( $aside ) { Rename-Item -LiteralPath $aside ( Split-Path -Leaf $Destination ) }
        throw
    }

    if( $aside )
    {
        try { Remove-Item -LiteralPath $aside -Recurse -Force }
        catch { Write-Warning "Installed the new $Destination, but could not delete the previous one ($aside): $_" }
    }
}

# Freeze the MCP server with its own Python runtime (qualification\package_mcp.py, PyInstaller) and
# install it to install\mcp, so MCP clients need neither Python nor a development checkout.  The
# package is built and checked offline in build\mcp, then replaces install\mcp.  $BasePython comes
# from Resolve-McpBasePython ($null: reuse build\mcp\venv).
function Install-Mcp( [string[]] $BasePython )
{
    $mcp = Join-Path $Repo 'tools\live_schematic_mcp'
    $work = Join-Path $Build 'mcp'
    $venv = Join-Path $work 'venv'
    $python = Join-Path $venv 'Scripts\python.exe'
    New-Item -ItemType Directory -Force $work | Out-Null

    if( $BasePython )
    {
        if( Test-Path $venv ) { Remove-Item -LiteralPath $venv -Recurse -Force }
        Invoke-Checked $BasePython[0] ( @( $BasePython | Select-Object -Skip 1 ) + @( '-m', 'venv', $venv ) )
        $problem = Get-McpPythonProblem @( $python )
        if( $problem ) { throw $problem }
    }

    Invoke-Checked $python @( '-m', 'pip', 'install', '-q', '--disable-pip-version-check',
                              '-r', ( Join-Path $mcp 'requirements.txt' ), 'pyinstaller>=6.16,<7' )

    $kicadCli = Join-Path $Install 'bin\kicad-cli.exe'
    if( -not ( Test-Path $kicadCli ) ) { throw "KiCad is not installed in $Install; build it first" }
    $lines = @( & $kicadCli version )
    $code = $LASTEXITCODE
    $version = "$( $lines | Select-Object -First 1 )".Trim()
    if( $code -ne 0 -or -not $version ) { throw "$kicadCli version failed with exit code $code" }

    Get-ChildItem $work -Directory -Filter 'package-*' | Remove-Item -Recurse -Force
    $package = Join-Path $work ( 'package-' + ( Get-Date -Format 'yyyyMMddHHmmss' ) )
    Invoke-Checked $python @( '-I', ( Join-Path $mcp 'qualification\package_mcp.py' ), $package,
                              '--application-version', $version )
    Invoke-Checked $python @( '-I', ( Join-Path $mcp 'qualification\check_package.py' ),
                              ( Join-Path $package 'embeint-kicad-mcp.exe' ) )

    $dest = Join-Path $Install 'mcp'
    Update-Folder $package $dest
    Write-Host "MCP server: $dest\embeint-kicad-mcp.exe (KiCad $version)"
}

# Check the MCP build Python before anything slow
$McpBasePython = $null
if( -not $NoMcp ) { $McpBasePython = Resolve-McpBasePython }

if( $McpOnly )
{
    if( $NoMcp ) { throw '-McpOnly and -NoMcp cannot be combined' }
    Install-Mcp $McpBasePython
    return
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
Copy-Changed ( Join-Path $triplet 'bin\*.dll' ) $bin
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
    Copy-Changed ( Join-Path $codeModels '*.cm' ) $dest
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

if( -not $NoMcp ) { Install-Mcp $McpBasePython }

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
if( -not $NoMcp ) { Write-Host "MCP server (register it with your MCP client): $Install\mcp\embeint-kicad-mcp.exe" }
