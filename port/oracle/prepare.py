#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Export the exact reference from Git; works in Windows worktrees and Linux CI."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

from pin import KICAD_PIN, SOURCE_PATHS


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    args.output_dir.mkdir(parents=True, exist_ok=True)
    archive = args.output_dir.resolve() / "reference.tar"
    subprocess.run(["git", "cat-file", "-e", KICAD_PIN + "^{commit}"], cwd=root, check=True)
    subprocess.run(["git", "archive", "--format=tar", "--output=" + str(archive),
                    KICAD_PIN, *SOURCE_PATHS], cwd=root, check=True)
    files = {}
    with tarfile.open(archive) as source:
        for entry in source:
            if entry.isfile():
                with source.extractfile(entry) as stream:
                    files[entry.name] = hashlib.sha256(stream.read()).hexdigest()
            elif not entry.isdir():
                raise ValueError("Unexpected non-file in reference: " + entry.name)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    status = subprocess.check_output(["git", "status", "--porcelain", "--", "port/oracle"],
                                     cwd=root, text=True)
    manifest = {"kicad_pin": KICAD_PIN, "files": files, "overlay_revision": revision,
                "overlay_dirty_at_export": bool(status),
                "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}
    (args.output_dir / "source-manifest.json").write_text(
        json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(f"Exported {len(files)} pinned files to {archive}")


if __name__ == "__main__":
    main()
