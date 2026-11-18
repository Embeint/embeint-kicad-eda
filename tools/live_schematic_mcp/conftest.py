# SPDX-License-Identifier: GPL-3.0-or-later
"""Shared offline test fixtures."""

from pathlib import Path
import os
import tempfile

import pytest


@pytest.fixture
def socket_tmp_path():
    if os.name != "posix":
        pytest.skip("Local filesystem IPC discovery/probe peers require Unix; run inside WSL")
    # macOS's pytest paths under /private/var/folders can exceed sockaddr_un's
    # path limit before a socket filename is appended. Keep real IPC peers short.
    parent = "/tmp" if Path("/tmp").is_dir() else None
    with tempfile.TemporaryDirectory(prefix="emcp-", dir=parent) as directory:
        yield Path(directory)
