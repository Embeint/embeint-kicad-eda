# SPDX-License-Identifier: GPL-3.0-or-later
"""Functions of scripts/build_windows.ps1, run by Windows PowerShell against temporary folders.

Each test loads only the named functions from the script (through PowerShell's parser), so nothing
is built or installed.
"""

from pathlib import Path
import shutil
import subprocess
import sys

import pytest

SCRIPT = Path(__file__).with_name("scripts") / "build_windows.ps1"
POWERSHELL = shutil.which("powershell.exe") if sys.platform == "win32" else None

pytestmark = pytest.mark.skipif(POWERSHELL is None, reason="needs Windows PowerShell")


def run_functions(names, body, folder):
    """Run body after defining the named functions of build_windows.ps1; returns the process.
    The script is written to folder (PowerShell reads multi-line blocks only from a file)."""
    loader = (
        "$ErrorActionPreference = 'Stop'\n"
        f"$ast = [System.Management.Automation.Language.Parser]::ParseFile('{SCRIPT}', [ref] $null, [ref] $null)\n"
        f"foreach( $name in @( {', '.join(repr(name) for name in names)} ) ) {{\n"
        "    $def = $ast.Find( { param( $a ) $a -is [System.Management.Automation.Language.FunctionDefinitionAst]"
        " -and $a.Name -eq $name }, $true )\n"
        "    if( -not $def ) { throw \"no function $name\" }\n"
        "    . ( [scriptblock]::Create( $def.Extent.Text ) )\n"
        "}\n"
    )
    script = folder / "run.ps1"
    script.write_text(loader + body, encoding="utf-8-sig")
    return subprocess.run([POWERSHELL, "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                           "-File", str(script)], capture_output=True, text=True, timeout=120)


def make_package(folder, marker):
    (folder / "licenses" / "third-party").mkdir(parents=True)
    (folder / "embeint-kicad-mcp.exe").write_text(marker)
    (folder / "build-info.json").write_text(marker)
    (folder / "licenses" / "third-party" / "nng-LICENSE.txt").write_text(marker)
    return sorted(str(path.relative_to(folder)) for path in folder.rglob("*"))


def test_replacing_a_folder_with_a_file_in_use_changes_nothing(tmp_path):
    old, new = tmp_path / "install" / "mcp", tmp_path / "package"
    old_files, new_files = make_package(old, "old"), make_package(new, "new")
    # A file in the middle of the folder is open, as a running server's DLL would be
    locked = old / "licenses" / "third-party" / "nng-LICENSE.txt"
    result = run_functions(["Update-Folder"], f"""
$handle = [IO.File]::Open('{locked}', 'Open', 'Read', 'Read')
try {{ Update-Folder '{new}' '{old}'; 'replaced' }}
catch {{ "refused: $_" }}
finally {{ $handle.Dispose() }}
""", tmp_path)
    assert result.returncode == 0, result.stderr
    assert "refused: Cannot replace" in result.stdout and "nothing was changed" in result.stdout
    assert sorted(str(path.relative_to(old)) for path in old.rglob("*")) == old_files
    assert (old / "build-info.json").read_text() == "old"
    assert sorted(str(path.relative_to(new)) for path in new.rglob("*")) == new_files
    assert [path.name for path in old.parent.iterdir()] == ["mcp"]


def test_replacing_a_running_program_changes_nothing(tmp_path):
    """Windows renames a folder whose program runs; the exclusive-open check must refuse anyway."""
    old, new = tmp_path / "install" / "mcp", tmp_path / "package"
    make_package(old, "old")
    make_package(new, "new")
    program = old / "embeint-kicad-mcp.exe"
    shutil.copyfile(shutil.which("ping.exe") or r"C:\Windows\System32\PING.EXE", program)
    running = subprocess.Popen([str(program), "-n", "30", "127.0.0.1"], stdout=subprocess.DEVNULL)
    try:
        result = run_functions(["Update-Folder"], f"""
try {{ Update-Folder '{new}' '{old}'; 'replaced' }} catch {{ "refused: $_" }}
""", tmp_path)
    finally:
        running.kill()
        running.wait()
    assert result.returncode == 0, result.stderr
    assert "refused: Cannot replace" in result.stdout, result.stdout
    assert (old / "build-info.json").read_text() == "old"
    assert (new / "build-info.json").read_text() == "new"


def test_replacing_a_free_folder_swaps_it_and_deletes_the_old_one(tmp_path):
    old, new = tmp_path / "install" / "mcp", tmp_path / "package"
    make_package(old, "old")
    new_files = make_package(new, "new")
    result = run_functions(["Update-Folder"], f"Update-Folder '{new}' '{old}'; 'replaced'", tmp_path)
    assert result.returncode == 0, result.stderr
    assert "replaced" in result.stdout
    assert sorted(str(path.relative_to(old)) for path in old.rglob("*")) == new_files
    assert (old / "build-info.json").read_text() == "new"
    assert not new.exists()
    assert [path.name for path in old.parent.iterdir()] == ["mcp"]


def test_first_install_moves_the_package_in(tmp_path):
    old, new = tmp_path / "install" / "mcp", tmp_path / "package"
    old.parent.mkdir()
    make_package(new, "new")
    result = run_functions(["Update-Folder"], f"Update-Folder '{new}' '{old}'; 'replaced'", tmp_path)
    assert result.returncode == 0, result.stderr
    assert (old / "build-info.json").read_text() == "new"


def test_mcp_python_must_be_64_bit_3_11_or_newer(tmp_path):
    result = run_functions(["Get-McpPythonProblem"], f"""
"current=[$( Get-McpPythonProblem @( '{sys.executable}' ) )]"
"missing=[$( Get-McpPythonProblem @( '{tmp_path / "absent" / "python.exe"}' ) )]"
""", tmp_path)
    assert result.returncode == 0, result.stderr
    lines = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
    supported = sys.version_info >= (3, 11) and sys.maxsize > 2 ** 32
    assert (lines["current"] == "[]") == supported, lines
    assert "does not run" in lines["missing"], lines
