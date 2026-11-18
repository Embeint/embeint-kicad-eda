# SPDX-License-Identifier: GPL-3.0-or-later
"""Filesystem-only discovery of KiCad's Linux/WSL local IPC endpoints.

Socket names follow KICAD_API_SERVER::Start in common/api/api_server.cpp.
A socket file is a candidate, never proof of a live server. No locks, socket
contents or application data are modified here.
"""

import os
import errno
from pathlib import Path
import re
import stat
import socket
import tempfile


class DiscoveryError(RuntimeError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def default_socket_url():
    return "ipc://" + str(Path(tempfile.gettempdir()) / "kicad" / "api.sock")


def local_socket_path(url):
    """Accept filesystem IPC only, without ever echoing an invalid URL."""
    if (not isinstance(url, str) or not url.startswith("ipc:///")
            or any(ord(c) < 32 or c in "?#" for c in url)):
        raise DiscoveryError("invalid_socket", "Use an absolute local ipc:///path socket URL in "
                             "KICAD_API_SOCKET; network transports are not supported.")
    path = Path(url[len("ipc://"):])
    if url == "ipc:///":
        raise DiscoveryError("invalid_socket", "The IPC URL must name a socket file.")
    return path


def socket_entry(url):
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
    sockets = [entry["socket_url"] for entry in candidates if entry["kind"] == "socket"]
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

    Only refusal or a vanished path proves the candidate cannot receive a request.
    Permission failures, timeouts and other errors remain ambiguous.
    """
    path = local_socket_path(url)
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as peer:
            peer.settimeout(0.1)
            peer.connect(str(path))
        return "connectable"
    except OSError as error:
        if error.errno in (errno.ECONNREFUSED, errno.ENOENT):
            return "not_listening"
        return "unknown"


def select_socket(socket_url=None, *, directory=None, check_liveness=False):
    inventory = discover_instances(socket_url, directory=directory)
    # Online selection happens only immediately before an editor request. Offline
    # inventory and explicit selection never connect or clean up socket files.
    if check_liveness and inventory["selection"] not in {"explicit", "unavailable"}:
        sockets = [entry["socket_url"] for entry in inventory["candidates"]
                   if entry["kind"] == "socket"]
        usable = [url for url in sockets if socket_reachability(url) != "not_listening"]
        if sockets:
            inventory["selected_socket"] = usable[0] if len(usable) == 1 else None
            inventory["selection"] = "ambiguous" if len(usable) > 1 else "unavailable"
    if inventory["selected_socket"] is None:
        raise DiscoveryError(inventory["selection"],
                             "KiCad socket selection is " + inventory["selection"] + ". "
                             "Run diagnose.py (optionally --probe) and set KICAD_API_SOCKET "
                             "to the intended instance. Stale sockets are never removed by MCP.")
    return inventory["selected_socket"]
