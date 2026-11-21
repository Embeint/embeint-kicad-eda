#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reject altered archives and unexpected paths before extraction."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import sys
import tarfile

from pin import KICAD_PIN


def verify(stage):
    manifest = json.loads((stage / "source-manifest.json").read_text(encoding="utf-8"))
    archive = stage / "reference.tar"
    if manifest["kicad_pin"] != KICAD_PIN:
        raise ValueError("Incorrect KiCad pin")
    if hashlib.sha256(archive.read_bytes()).hexdigest() != manifest["archive_sha256"]:
        raise ValueError("Reference archive hash mismatch")
    files = {}
    with tarfile.open(archive) as source:
        for entry in source:
            path = PurePosixPath(entry.name)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("Unsafe reference path")
            if entry.isfile():
                with source.extractfile(entry) as stream:
                    files[entry.name] = hashlib.sha256(stream.read()).hexdigest()
            elif not entry.isdir():
                raise ValueError("Reference contains a non-file")
    if files != manifest["files"]:
        raise ValueError("Reference archive does not match source manifest")


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
