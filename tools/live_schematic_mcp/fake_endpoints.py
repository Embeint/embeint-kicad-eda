# SPDX-License-Identifier: GPL-3.0-or-later
"""Endpoint URLs for offline tests: valid local IPC on this platform, never served by KiCad."""

from pathlib import PureWindowsPath
import sys
import tempfile


def unused_socket(name):
    """ipc:///name on Linux/WSL; a named-pipe path under the temp directory on Windows."""
    if sys.platform == "win32":
        return "ipc://" + str(PureWindowsPath(tempfile.gettempdir()) / "embeint-mcp-unused" / name)
    return "ipc:///" + name
