# SPDX-License-Identifier: GPL-3.0-or-later
"""Refuse release artifacts that do not identify the clean commit and version being tagged."""

import argparse
import json
from pathlib import Path
import re
import subprocess


PLATFORMS = ("windows-x86_64", "linux-x86_64", "linux-arm64")


def validate(tag, revision, packages):
    if not re.fullmatch(r"mcp-v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", tag):
        raise ValueError("release tag must be mcp-v followed by a version")
    if set(packages) != set(PLATFORMS):
        raise ValueError("release requires all three platform packages")
    version = tag.removeprefix("mcp-v")
    for platform, info in packages.items():
        if info.get("revision_source") != "git" or info.get("source_dirty") is not False:
            raise ValueError(f"{platform}: release requires a clean Git build")
        if info.get("source_revision") != revision:
            raise ValueError(f"{platform}: package revision differs from release tag")
        if info.get("mcp_version") != f"{version}+g{revision[:10]}":
            raise ValueError(f"{platform}: package version differs from release tag")
        native = info.get("rust", {}).get("mcp", {}).get("server_version")
        if native != f"{version}+rust.g{revision[:10]}":
            raise ValueError(f"{platform}: native server version differs from release tag")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("artifacts", type=Path)
    args = parser.parse_args()
    # Validate the tag before using it as a Git revision or a gh argument.
    if not re.fullmatch(r"mcp-v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", args.tag):
        parser.error("release tag must be mcp-v followed by a version")
    revision = subprocess.check_output(
        ["git", "rev-parse", "--verify", f"refs/tags/{args.tag}^{{commit}}"], text=True).strip()
    packages = {platform: json.loads((args.artifacts / f"embeint-kicad-mcp-{platform}" /
                                     f"build-info-{platform}.json").read_text(encoding="utf-8"))
                for platform in PLATFORMS}
    validate(args.tag, revision, packages)
    print(f"All release packages match {args.tag} ({revision})")


if __name__ == "__main__":
    main()
