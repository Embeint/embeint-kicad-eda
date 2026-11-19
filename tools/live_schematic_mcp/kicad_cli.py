# SPDX-License-Identifier: GPL-3.0-or-later
"""Run kicad-cli with the environment its KiCad was launched with.

kicad-cli is a separate KiCad process.  To load the same shared libraries, settings (library tables,
path variables, ERC settings) and stock libraries as the editor, it needs the environment the
launcher gave KiCad: run_kicad_wsl.sh for the development build, /usr/bin/embeint-kicad (via
libexec/environment.sh) for the installed package.  The running KiCad reports that environment
(GetKiCadEnvironment); without it this module rebuilds it from the install layout.  No KiCad calls
here: client code supplies what KiCad reported.
"""

import os
from pathlib import Path
import re
import subprocess
import sys

# kicad-cli's top-level commands; anything else is refused before starting a process
COMMANDS = ("sch", "pcb", "sym", "fp", "jobset", "version")
OUTPUT_LIMIT = 20000


def child_environment(environ=None, frozen=None):
    """The MCP's environment for programs it starts.  A frozen (PyInstaller) MCP puts its private
    libraries on LD_LIBRARY_PATH; they must not reach KiCad, so restore the original value."""
    env = dict(os.environ if environ is None else environ)
    if frozen if frozen is not None else getattr(sys, "frozen", False):
        original = env.pop("LD_LIBRARY_PATH_ORIG", None)
        if original is None:
            env.pop("LD_LIBRARY_PATH", None)
        else:
            env["LD_LIBRARY_PATH"] = original
    return env


def layout_environment(cli_path, env):
    """Fill in what the launchers set up, from where kicad-cli is installed (when KiCad cannot say).

    Development build ($KICAD_LIVE_ROOT/install/bin/kicad-cli): run_kicad_wsl.sh's libraries,
    config/ and local stock library copies.  Installed package (/opt/embeint-kicad/bin/kicad-cli):
    libexec/environment.sh's library path and configuration home.  Variables already set win."""
    env = dict(env)
    install = Path(cli_path).resolve().parent.parent
    root = install.parent
    libs = [str(install / "lib")]
    if (root / "deps/usr/lib/x86_64-linux-gnu").is_dir():
        libs.insert(0, str(root / "deps/usr/lib/x86_64-linux-gnu"))
    if sys.platform != "win32":
        current = [p for p in env.get("LD_LIBRARY_PATH", "").split(":") if p and p not in libs]
        env["LD_LIBRARY_PATH"] = ":".join(libs + current)
    if (install / "libexec/environment.sh").is_file():
        config = env.get("XDG_CONFIG_HOME") or str(Path(env.get("HOME", str(Path.home()))) / ".config")
        env.setdefault("KICAD_CONFIG_HOME", str(Path(config) / "embeint-kicad"))
    elif (root / "config").is_dir():
        env.setdefault("KICAD_CONFIG_HOME", str(root / "config"))
    stock = Path(env.get("KICAD_LIBRARY_ROOT", "/mnt/c/Program Files/KiCad/10.0/share/kicad"))
    for kind, name in (("symbols", "SYMBOL"), ("footprints", "FOOTPRINT")):
        if (root / "share" / kind).is_dir():
            for version in (10, 11):
                env.setdefault(f"KICAD{version}_{name}_DIR", str(root / "share" / kind))
    if (root / "share/symbols").is_dir() and stock.is_dir():
        for kind, name in (("3dmodels", "3DMODEL"), ("template", "TEMPLATE")):
            for version in (10, 11):
                env.setdefault(f"KICAD{version}_{name}_DIR", str(stock / kind))
    return env


def cli_environment(cli_path, kicad_variables=None, environ=None, frozen=None):
    """(environment, description) for running kicad-cli.  kicad_variables, as reported by the running
    KiCad, take precedence; None means KiCad could not report them and the install layout is used."""
    env = child_environment(environ, frozen)
    if kicad_variables is not None:
        env.update(kicad_variables)
        return env, {"source": "kicad", "variables": sorted(kicad_variables)}
    before = dict(env)
    env = layout_environment(cli_path, env)
    return env, {"source": "install layout",
                 "variables": sorted(k for k in env if env[k] != before.get(k))}


def default_cli(environ=None, executable=None):
    """kicad-cli without asking KiCad: $KICAD_CLI, the installed package beside a frozen MCP, or the
    development build.  None if there is none."""
    environ = os.environ if environ is None else environ
    candidates = []
    if environ.get("KICAD_CLI"):
        candidates.append(Path(environ["KICAD_CLI"]))
    if executable or getattr(sys, "frozen", False):
        # /opt/embeint-kicad/libexec/mcp/embeint-kicad-mcp -> /opt/embeint-kicad/bin/kicad-cli
        candidates.append(Path(executable or sys.executable).resolve().parents[2] / "bin/kicad-cli")
    live_root = environ.get("KICAD_LIVE_ROOT") or str(Path(environ.get("HOME", str(Path.home()))) / "kicad-live")
    candidates.append(Path(live_root) / "install/bin/kicad-cli")
    for candidate in candidates:
        for path in (candidate, candidate.with_suffix(".exe")):
            if path.is_file():
                return str(path)
    return None


def cli_arguments(args):
    """Validate kicad-cli arguments.  On Linux/WSL, Windows paths (C:\\...) become /mnt/c/... ."""
    if not isinstance(args, (list, tuple)) or not args or not all(isinstance(a, str) for a in args):
        raise ValueError("args must be a non-empty list of strings, e.g. [\"sch\", \"erc\", \"design.kicad_sch\"]")
    if args[0] not in COMMANDS:
        raise ValueError(f"the first argument must be a kicad-cli command: {', '.join(COMMANDS)}")
    if any("\0" in a for a in args):
        raise ValueError("arguments must not contain NUL characters")
    if sys.platform == "win32":
        return list(args)
    from model_handoff import to_wsl
    return [to_wsl(a) if re.match(r"^[A-Za-z]:[\\/]", a) else a for a in args]


def tail(text):
    return (text[-OUTPUT_LIMIT:], True) if len(text) > OUTPUT_LIMIT else (text, False)


def run(cli_path, args, env, cwd=None, timeout_s=300):
    """Run kicad-cli once; returns its exit code and output (the last 20000 characters of each)."""
    if not 0 < timeout_s <= 3600:
        raise ValueError("timeout_s must be between 0 and 3600 seconds")
    command = [cli_path, *cli_arguments(args)]
    if cwd is not None and sys.platform != "win32":
        from model_handoff import to_wsl
        cwd = to_wsl(cwd)
    completed = subprocess.run(command, env=env, cwd=cwd, capture_output=True, text=True,
                               errors="replace", timeout=timeout_s)
    stdout, stdout_cut = tail(completed.stdout)
    stderr, stderr_cut = tail(completed.stderr)
    return {"command": command, "returncode": completed.returncode, "stdout": stdout, "stderr": stderr,
            **({"stdout_truncated": True} if stdout_cut else {}),
            **({"stderr_truncated": True} if stderr_cut else {})}
