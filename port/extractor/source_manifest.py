#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Create the tracked source manifest with the host's Git (Windows for worktrees)."""
import argparse
import json
from pathlib import Path
import subprocess

SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx", ".m", ".mm"}
PIN = "4cf939778df04d2b747bb2c4714fc5014a7565c7"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--output", type=Path, required=True)
    a = parser.parse_args()
    def git(*args):
        return subprocess.check_output(["git", "-C", str(a.root), *args], encoding="utf-8")
    files = sorted(p for p in git("ls-files", "-z").split("\0") if Path(p).suffix.lower() in SUFFIXES)
    modified = set(git("diff", "HEAD", "--name-only", "-z").split("\0")) & set(files)
    if modified:
        raise ValueError("Commit tracked source edits before snapshotting: " + ", ".join(sorted(modified)))
    value = {"source_commit": git("rev-parse", "HEAD").strip(), "upstream_pin": PIN, "files": files}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(files)} tracked sources at {value['source_commit']}")


if __name__ == "__main__":
    main()
