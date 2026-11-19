# SPDX-License-Identifier: GPL-3.0-or-later
"""Discovery of KiCad's local IPC endpoints, without dialing them.

Endpoint names follow KICAD_API_SERVER::Start in common/api/api_server.cpp:
``ipc://<temp>/kicad/api.sock`` (or ``api-<pid>.sock``).  On Linux/WSL that is a Unix
socket file.  On Windows NNG serves the same URL as the local named pipe
``\\\\.\\pipe\\<temp>\\kicad\\api.sock``; the pipe exists only while a server holds it.
A candidate is never proof of a live server. No locks, socket files, pipes or
application data are modified here.
"""

import os
import errno
from pathlib import Path, PureWindowsPath
import re
import stat
import socket
import sys
import tempfile


ENDPOINT_KINDS = ("socket", "named_pipe")
PIPE_ROOT = "\\\\.\\pipe\\"
# CreateNamedPipe's limit for the whole \\.\pipe\name
PIPE_NAME_LIMIT = 256
ENDPOINT_NAME = re.compile(r"api(?:-[0-9]+)?\.sock", re.IGNORECASE)


NO_EDITOR_ACTION = ("No KiCad editor is listening. Start Embeint KiCad with the API server enabled "
                    "(Preferences > Plugins > Enable KiCad API) and open a project, as the same "
                    "user and with the same temporary directory as the MCP, or set KICAD_API_SOCKET.")


class DiscoveryError(RuntimeError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def windows():
    return sys.platform == "win32"


def is_endpoint(entry):
    return entry["kind"] in ENDPOINT_KINDS


def windows_temp_dir(environ=None):
    """The temporary directory KiCad uses on Windows, spelt as KiCad spells it.

    KiCad takes wxStandardPaths::GetTempDir(), i.e. wxFileName::GetTempDir(): the first of
    TMPDIR, TMP and TEMP that is set, else GetTempPath() (TMP, TEMP, USERPROFILE, then the
    Windows directory).  Python's tempfile prefers TEMP to TMP and checks writability, so it
    can name another directory.  The value is used as given (a short 8.3 spelling stays
    short), with forward slashes and the trailing separator normalised away as wxFileName does.
    """
    environ = os.environ if environ is None else environ
    for name in ("TMPDIR", "TMP", "TEMP", "USERPROFILE", "SystemRoot"):
        if environ.get(name):
            return str(PureWindowsPath(environ[name]))
    return str(PureWindowsPath(tempfile.gettempdir()))


def default_socket_url():
    if windows():
        return "ipc://" + str(PureWindowsPath(windows_temp_dir()) / "kicad" / "api.sock")
    return "ipc://" + str(Path(tempfile.gettempdir()) / "kicad" / "api.sock")


def _invalid_url():
    if windows():
        return DiscoveryError("invalid_socket", "Use a local ipc://C:\\...\\api.sock named-pipe URL in "
                              "KICAD_API_SOCKET; network transports are not supported.")
    return DiscoveryError("invalid_socket", "Use an absolute local ipc:///path socket URL in "
                          "KICAD_API_SOCKET; network transports are not supported.")


def _windows_pipe_path(name):
    """The pipe NNG serves for ipc://name is \\\\.\\pipe\\name, which Windows normalises.

    Only an absolute drive path made of plain components is accepted, so no "." or
    ".." component, forward slash or device prefix can redirect the pipe name to a
    UNC (network) pipe or another device.
    """
    match = re.fullmatch(r"([A-Za-z]):\\(.+)", name)
    if not match or len(PIPE_ROOT) + len(name) > PIPE_NAME_LIMIT:
        raise _invalid_url()
    for part in match.group(2).split("\\"):
        if (not part or part in (".", "..") or part[-1] in ". "
                or any(c in '<>:"/|?*' for c in part)):
            raise _invalid_url()
    return PureWindowsPath(name)


def local_socket_path(url):
    """Accept local IPC only, without ever echoing an invalid URL.

    Linux/WSL: an absolute ipc:///path Unix socket.  Windows: ipc://X:\\path, which NNG
    serves as a named pipe on this machine.  TCP, WebSocket, relative, query-bearing and
    (on Windows) UNC or dot-segment URLs are rejected.
    """
    if (not isinstance(url, str) or not url.startswith("ipc://")
            or any(ord(c) < 32 or c in "?#" for c in url)):
        raise _invalid_url()
    if windows():
        return _windows_pipe_path(url[len("ipc://"):])
    if not url.startswith("ipc:///"):
        raise _invalid_url()
    path = Path(url[len("ipc://"):])
    if url == "ipc:///":
        raise DiscoveryError("invalid_socket", "The IPC URL must name a socket file.")
    return path


def list_pipes():
    """Names of this machine's named pipes.  Enumerating them never connects to one."""
    return os.listdir(PIPE_ROOT)


def directory_key(path):
    """Compare directories as Windows does: case-insensitively and with long names.

    KiCad and Python can see the same temporary directory as C:\\Users\\LONGNA~1\\... and
    C:\\Users\\LongName\\...; resolving both makes them match.
    """
    try:
        resolved = os.path.realpath(str(path))
    except (OSError, ValueError):
        resolved = str(path)
    return os.path.normcase(resolved) if windows() else resolved


def _pipe_entry(url, pipes):
    path = local_socket_path(url)
    present = {name.lower() for name in pipes}
    return {"socket_url": url, "kind": "named_pipe" if str(path).lower() in present else "missing"}


def socket_entry(url):
    if windows():
        try:
            pipes = list_pipes()
        except OSError:
            local_socket_path(url)
            return {"socket_url": url, "kind": "unavailable"}
        return _pipe_entry(url, pipes)
    path = local_socket_path(url)
    try:
        mode = path.lstat().st_mode
        kind = ("socket" if stat.S_ISSOCK(mode) else
                "symlink" if stat.S_ISLNK(mode) else "not_socket")
    except FileNotFoundError:
        kind = "missing"
    except OSError:
        kind = "unavailable"
    return {"socket_url": url, "kind": kind}


def _windows_candidates(root):
    """KiCad pipes named under this user's temp\\kicad directory.

    Named pipes are machine-wide, so endpoints under other directories are visible too;
    they are never candidates.  Only the name is checked, not the pipe's owner: another
    local account could create a pipe with this name (see INSTANCE_DISCOVERY.md).
    """
    try:
        pipes = list_pipes()
    except OSError:
        raise DiscoveryError("discovery_unavailable", "Cannot list this machine's named pipes; "
                             "set KICAD_API_SOCKET to the intended instance.") from None
    wanted, keys, names = directory_key(root), {}, []
    for name in pipes:
        try:
            path = _windows_pipe_path(name)
        except DiscoveryError:
            continue
        if not ENDPOINT_NAME.fullmatch(path.name):
            continue
        parent = str(path.parent)
        if parent not in keys:
            keys[parent] = directory_key(parent)
        if keys[parent] == wanted:
            names.append(name)
    return [{"socket_url": "ipc://" + name, "kind": "named_pipe"}
            for name in sorted(names, key=lambda n: PureWindowsPath(n).name.lower())]


def discover_instances(socket_url=None, *, directory=None):
    """Return a stable inventory without dialing. Explicit selection never scans.

    directory is an internal test seam; normal discovery uses the same temp-dir
    convention as the existing adapter. Custom server paths require an explicit
    URL. Stale candidates deliberately still count towards ambiguity.
    """
    explicit = socket_url or os.environ.get("KICAD_API_SOCKET")
    if explicit:
        return {"selection": "explicit", "selected_socket": explicit,
                "candidates": [socket_entry(explicit)]}
    if windows():
        root = (PureWindowsPath(directory) if directory is not None
                else local_socket_path(default_socket_url()).parent)
        default = "ipc://" + str(root / "api.sock")
        candidates = _windows_candidates(root)
    else:
        root = Path(directory) if directory is not None else local_socket_path(default_socket_url()).parent
        default = "ipc://" + str(root / "api.sock")
        try:
            if root.is_symlink():
                raise DiscoveryError("discovery_unavailable", "The KiCad socket directory is a symlink; "
                                     "select a trusted local socket with KICAD_API_SOCKET.")
            names = sorted(p.name for p in root.iterdir()
                           if re.fullmatch(r"api(?:-[0-9]+)?\.sock", p.name))
        except FileNotFoundError:
            names = []
        except OSError:
            raise DiscoveryError("discovery_unavailable", "Cannot inspect the KiCad socket directory; "
                                 "check permissions or set KICAD_API_SOCKET.") from None
        candidates = [socket_entry("ipc://" + str(root / name)) for name in names]
    sockets = [entry["socket_url"] for entry in candidates if is_endpoint(entry)]
    if any(entry["kind"] == "unavailable" for entry in candidates):
        selection, selected = "unavailable", None
    elif len(sockets) > 1:
        selection, selected = "ambiguous", None
    elif sockets:
        selection, selected = "single_candidate", sockets[0]
    elif candidates:
        selection, selected = "unavailable", None
    else:
        selection, selected = "default_missing", default
    return {"selection": selection, "selected_socket": selected, "candidates": candidates}


def socket_reachability(url):
    """Check a local listener without sending NNG or application request bytes.

    Unix: only refusal or a vanished path proves the candidate cannot receive a request.
    Permission failures, timeouts and other errors remain ambiguous.  Windows: a named
    pipe disappears when its server closes, so absence proves there is no listener and
    presence is checked by enumeration, without connecting.
    """
    path = local_socket_path(url)
    if windows():
        try:
            return "connectable" if _pipe_entry(url, list_pipes())["kind"] == "named_pipe" else "not_listening"
        except OSError:
            return "unknown"
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
            peer.settimeout(0.1)
            peer.connect(str(path))
        return "connectable"
    except OSError as error:
        if error.errno in (errno.ECONNREFUSED, errno.ENOENT):
            return "not_listening"
        return "unknown"


def select_instance(socket_url=None, *, directory=None, check_liveness=False, require_listener=False):
    """Return (url, selection) for the instance to use, or raise DiscoveryError.

    selection is "explicit", "single_candidate" or "default_missing"; the last means no
    instance was found and url is only the default endpoint a KiCad would take, so a
    caller should look again later instead of keeping it.  With require_listener, a
    missing default raises DiscoveryError("no_editor") instead: no editor runs.
    """
    inventory = discover_instances(socket_url, directory=directory)
    if require_listener and inventory["selection"] == "default_missing":
        # No endpoint exists at all, so nothing can answer; fail now instead of timing out
        diagnose = ("embeint-kicad-mcp --diagnose" if getattr(sys, "frozen", False)
                    else "diagnose.py")
        raise DiscoveryError("no_editor", NO_EDITOR_ACTION + f" Nothing was sent. Run {diagnose} "
                             "for details.")
    # Online selection happens only immediately before an editor request. Offline
    # inventory and explicit selection never connect or clean up socket files.
    if check_liveness and inventory["selection"] not in {"explicit", "unavailable"}:
        sockets = [entry["socket_url"] for entry in inventory["candidates"] if is_endpoint(entry)]
        usable = [url for url in sockets if socket_reachability(url) != "not_listening"]
        if sockets:
            inventory["selected_socket"] = usable[0] if len(usable) == 1 else None
            inventory["selection"] = ("single_candidate" if len(usable) == 1 else
                                      "ambiguous" if usable else "unavailable")
    if inventory["selected_socket"] is None:
        raise DiscoveryError(inventory["selection"],
                             "KiCad socket selection is " + inventory["selection"] + ". "
                             "Run diagnose.py (optionally --probe) and set KICAD_API_SOCKET "
                             "to the intended instance. Stale sockets are never removed by MCP.")
    return inventory["selected_socket"], inventory["selection"]


def select_socket(socket_url=None, *, directory=None, check_liveness=False, require_listener=False):
    return select_instance(socket_url, directory=directory, check_liveness=check_liveness,
                           require_listener=require_listener)[0]


def main():
    """Print the offline inventory of the default endpoint as JSON; nothing is dialed.

    For run_kicad_windows.ps1, which refuses to start a KiCad unless the selection is
    "default_missing" (no other KiCad holds the endpoint the new one would take).
    KICAD_API_SOCKET is ignored: the inventory is always of the default location.
    Exits 2 if the endpoints cannot be listed.
    """
    import json
    os.environ.pop("KICAD_API_SOCKET", None)
    try:
        inventory = discover_instances()
    except DiscoveryError as error:
        print(json.dumps({"selection": error.code, "selected_socket": None, "candidates": [],
                          "error": str(error)}))
        return 2
    print(json.dumps(inventory))
    return 0


if __name__ == "__main__":
    sys.exit(main())
