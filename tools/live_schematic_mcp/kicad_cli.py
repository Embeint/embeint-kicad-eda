# SPDX-License-Identifier: GPL-3.0-or-later
"""Run kicad-cli with the environment its KiCad was launched with.

kicad-cli is a separate KiCad process.  To load the same shared libraries, settings (library tables,
path variables, ERC settings) and stock libraries as the editor, it needs the environment the
launcher gave KiCad: run_kicad_wsl.sh for the development build, /usr/bin/embeint-kicad (via
libexec/environment.sh) for the installed package.  The running KiCad reports that environment
(GetKiCadEnvironment); without it this module rebuilds it from the install layout.  No KiCad calls
here: client code supplies what KiCad reported.
"""

import asyncio
import locale
import os
from pathlib import Path
import re
import subprocess
import sys

# The read-only kicad-cli commands live_kicad_cli may run: checks and exports of saved files.  Anything
# else is refused before starting a process: jobset run (its "Execute Command" jobs run through a
# shell), upgrade/downgrade (rewrite files in place), import, diff, render and the rest.  The value is
# the allowed export kinds, or None for a command that takes no kind.
SCH_EXPORTS = ("bom", "dxf", "hpgl", "netlist", "pdf", "png", "ps", "python-bom", "svg")
PCB_EXPORTS = ("3dpdf", "bom", "brep", "drill", "dxf", "gencad", "gerbers", "glb", "hpgl", "idf", "ipc2581",
               "ipcd356", "odb", "pdf", "ply", "png", "pos", "ps", "specctra", "stackup", "stats", "step",
               "stl", "stpz", "svg", "u3d", "vrml", "xao")
COMMANDS = {("sch", "erc"): None, ("sch", "export"): SCH_EXPORTS,
            ("pcb", "drc"): None, ("pcb", "export"): PCB_EXPORTS,
            ("sym", "export"): ("svg",), ("fp", "export"): ("svg",),
            ("version",): None}
# Options that make an allowed command write into the project (pcb drc --save-board saves the board)
WRITE_OPTIONS = ("--save-board",)
# Options whose value is a file or folder kicad-cli writes
OUTPUT_OPTIONS = ("-o", "--output", "--models-dir", "--report-path")
# Outputs that would overwrite a design, project or library file
PROTECTED_SUFFIXES = (".kicad_sch", ".kicad_pcb", ".kicad_pro", ".kicad_prl", ".kicad_sym", ".kicad_mod",
                      ".kicad_dru")
PROTECTED_NAMES = ("sym-lib-table", "fp-lib-table", "design-block-lib-table")
MAX_TIMEOUT_S = 600
OUTPUT_LIMIT = 20000
WINDOWS_PATH = re.compile(r"^[A-Za-z]:[\\/]")


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
    KiCad, take precedence; None means KiCad could not report them and the install layout is used.  An
    empty report is not a usable environment either (KiCad always has a configuration home): it is
    rebuilt from the install layout and the description says KiCad reported nothing."""
    env = child_environment(environ, frozen)
    if kicad_variables:
        env.update(kicad_variables)
        return env, {"source": "kicad", "variables": sorted(kicad_variables)}
    before = dict(env)
    env = layout_environment(cli_path, env)
    description = {"source": "install layout", "variables": sorted(k for k in env if env[k] != before.get(k))}
    if kicad_variables is not None:
        description["note"] = "KiCad reported an empty launch environment"
    return env, description


def default_cli(environ=None, executable=None):
    """kicad-cli without asking KiCad: $KICAD_CLI, the installed package beside a frozen MCP, or the
    development build.  None if there is none."""
    environ = os.environ if environ is None else environ
    candidates = []
    if environ.get("KICAD_CLI"):
        candidates.append(Path(environ["KICAD_CLI"]))
    if executable or getattr(sys, "frozen", False):
        mcp = Path(executable or sys.executable).resolve()
        # Windows bundle: C:\kb\install\mcp\embeint-kicad-mcp.exe -> C:\kb\install\bin\kicad-cli.exe
        candidates.append(mcp.parents[1] / "bin/kicad-cli")
        # Linux package: /opt/embeint-kicad/libexec/mcp/embeint-kicad-mcp -> /opt/embeint-kicad/bin/kicad-cli
        candidates.append(mcp.parents[2] / "bin/kicad-cli")
    live_root = environ.get("KICAD_LIVE_ROOT") or str(Path(environ.get("HOME", str(Path.home()))) / "kicad-live")
    candidates.append(Path(live_root) / "install/bin/kicad-cli")
    for candidate in candidates:
        for path in (candidate, candidate.with_suffix(".exe")):
            if path.is_file():
                return str(path)
    return None


def allowed_commands():
    """The allowed commands, for messages: "sch erc, sch export <kind>, ..."."""
    return ", ".join(" ".join(verb) + (" <kind>" if kinds else "") for verb, kinds in COMMANDS.items())


def path_parts(value):
    """The components of a path as Windows opens them, lower-cased: a stream suffix (name:stream,
    name::$DATA) is cut and trailing dots and spaces are dropped, so design.kicad_sch. or
    design.kicad_sch::$DATA is seen as design.kicad_sch.  Applied on every platform: a WSL KiCad
    writes Windows folders through /mnt/<drive>."""
    parts = []
    for index, part in enumerate(p for p in re.split(r"[\\/]+", value) if p):
        if not (index == 0 and re.fullmatch(r"[A-Za-z]:", part)):
            part = part.split(":", 1)[0]
        part = part.rstrip(". ")
        if part:
            parts.append(part.lower())
    return parts


def resolved(value, cwd):
    """The path as the system will open it, against kicad-cli's working directory: existing short
    (8.3) names and links are resolved (DESIGN~1.KIC may be design.kicad_sch), the rest kept."""
    if sys.platform != "win32":
        # The WSL KiCad sees C:\... as /mnt/c/... (as cli_arguments and prepare translate them)
        from model_handoff import to_wsl
        value = to_wsl(value) if WINDOWS_PATH.match(value) else value
        cwd = to_wsl(cwd) if cwd is not None and WINDOWS_PATH.match(cwd) else cwd
    path = value if cwd is None or os.path.isabs(value) else os.path.join(cwd, value)
    try:
        return os.path.realpath(path)
    except (OSError, ValueError):
        return path


def path_forms(value, cwd):
    """The path as given and as resolved, each as Windows-normalised components."""
    return [path_parts(value), path_parts(resolved(value, cwd))]


def check_output(option, value, cwd=None):
    """Refuse an output that would overwrite a design, project or library file, or land in a .pretty
    footprint library (every file there is read as a footprint), as given or once resolved."""
    for parts in path_forms(value, cwd):
        name = parts[-1] if parts else ""
        if name.endswith(PROTECTED_SUFFIXES) or name in PROTECTED_NAMES:
            raise ValueError(f"{option} {value!r} would overwrite a design, project or library file; "
                             "write kicad-cli outputs elsewhere")
    for parts in path_forms(value, cwd):
        if any(part.endswith(".pretty") for part in parts):
            raise ValueError(f"{option} {value!r} is inside a footprint library (.pretty folder); "
                             "write kicad-cli outputs elsewhere")


def check_working_directory(cwd):
    """kicad-cli writes relative outputs, and outputs a command names itself, into its working
    directory: it must not be a footprint library, as given or once resolved."""
    if cwd is not None and any(part.endswith(".pretty") for parts in path_forms(cwd, None) for part in parts):
        raise ValueError(f"working_directory {cwd!r} is inside a footprint library (.pretty folder); "
                         "run kicad-cli elsewhere")


def check_options(options, cwd=None):
    """Refuse options that write into the project, and check every output target.  Options are given
    as kicad-cli parses them: "--name value" or "--name=value" for long options, "-o value" for short
    ones.  Combined short options ("-no out") are refused because they can hide an output."""
    for index, arg in enumerate(options):
        long_option = arg.startswith("--")
        name, assign, value = arg.partition("=") if long_option else (arg, "", "")
        if name in WRITE_OPTIONS:
            raise ValueError(f"{name} writes into the project; live_kicad_cli only runs read-only commands")
        if not long_option and len(arg) > 2 and arg[0] == "-" and not re.match(r"-[\d.]", arg):
            raise ValueError(f"combined short options are not accepted ({arg!r}); give each option separately")
        if name in OUTPUT_OPTIONS:
            if assign:
                check_output(name, value, cwd)
            elif index + 1 < len(options):
                check_output(name, options[index + 1], cwd)


def translate_path(arg):
    """On Linux/WSL, C:\\... (alone or as --option=C:\\...) becomes /mnt/c/... ."""
    from model_handoff import to_wsl
    if WINDOWS_PATH.match(arg):
        return to_wsl(arg)
    name, assign, value = arg.partition("=")
    if arg.startswith("--") and assign and WINDOWS_PATH.match(value):
        return f"{name}={to_wsl(value)}"
    return arg


def cli_arguments(args, cwd=None):
    """Validate kicad-cli arguments against the read-only allow-list (COMMANDS) and the output rules.
    On Linux/WSL, Windows paths (C:\\...) become /mnt/c/... ."""
    if not isinstance(args, (list, tuple)) or not args or not all(isinstance(a, str) for a in args):
        raise ValueError("args must be a non-empty list of strings, e.g. [\"sch\", \"erc\", \"design.kicad_sch\"]")
    if any("\0" in a for a in args):
        raise ValueError("arguments must not contain NUL characters")
    verb = tuple(args[:1]) if args[0] == "version" else tuple(args[:2])
    if verb not in COMMANDS:
        raise ValueError(f"{' '.join(verb)!r} is not allowed; live_kicad_cli runs only read-only kicad-cli "
                         f"commands: {allowed_commands()}")
    kinds = COMMANDS[verb]
    options = list(args[len(verb):])
    if kinds is not None:
        if not options or options[0] not in kinds:
            raise ValueError(f"{' '.join(verb)} needs one of these kinds next: {', '.join(kinds)}")
        options = options[1:]
    check_options(options, cwd)
    if sys.platform == "win32":
        return list(args)
    return [translate_path(a) for a in args]


def tail(text):
    return (text[-OUTPUT_LIMIT:], True) if len(text) > OUTPUT_LIMIT else (text, False)


def prepare(cli_path, args, cwd, timeout_s):
    """The command line and working directory for one run, after validating everything."""
    if not 0 < timeout_s <= MAX_TIMEOUT_S:
        raise ValueError(f"timeout_s must be between 0 and {MAX_TIMEOUT_S} seconds")
    command = [cli_path, *cli_arguments(args, cwd)]
    check_working_directory(cwd)
    if cwd is not None and sys.platform != "win32":
        from model_handoff import to_wsl
        cwd = to_wsl(cwd)
    return command, cwd


def result(command, returncode, stdout, stderr):
    stdout, stdout_cut = tail(stdout)
    stderr, stderr_cut = tail(stderr)
    return {"command": command, "returncode": returncode, "stdout": stdout, "stderr": stderr,
            **({"stdout_truncated": True} if stdout_cut else {}),
            **({"stderr_truncated": True} if stderr_cut else {})}


def run(cli_path, args, env, cwd=None, timeout_s=300):
    """Run kicad-cli once; returns its exit code and output (the last 20000 characters of each).
    Blocks the caller: the MCP tool uses run_async."""
    command, cwd = prepare(cli_path, args, cwd, timeout_s)
    completed = subprocess.run(command, env=env, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True,
                               text=True, errors="replace", timeout=timeout_s)
    return result(command, completed.returncode, completed.stdout, completed.stderr)


def decode(data):
    """Process output as text, like subprocess's text mode."""
    return data.decode(locale.getpreferredencoding(False), errors="replace").replace("\r\n", "\n")


async def run_async(cli_path, args, env, cwd=None, timeout_s=300):
    """run() without blocking the event loop.  kicad-cli is killed when it times out (TimeoutError) or
    when the caller is cancelled, so no process outlives the tool call."""
    command, cwd = prepare(cli_path, args, cwd, timeout_s)
    process = await asyncio.create_subprocess_exec(*command, env=env, cwd=cwd, stdin=asyncio.subprocess.DEVNULL,
                                                   stdout=asyncio.subprocess.PIPE,
                                                   stderr=asyncio.subprocess.PIPE)
    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout_s)
    except asyncio.TimeoutError:
        process.kill()
        await process.wait()
        raise TimeoutError(f"kicad-cli did not finish within {timeout_s} s and was stopped") from None
    except BaseException:
        # Cancelled: kill without awaiting, as a cancelled task may be cancelled again at any await
        if process.returncode is None:
            process.kill()
        raise
    return result(command, process.returncode, decode(stdout), decode(stderr))
