#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build only Ninja CUSTOM_COMMAND C/C++ inputs in the private analysis tree."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import write_json

SOURCE_SUFFIXES = {".h", ".hh", ".hpp", ".hxx", ".c", ".cc", ".cpp", ".cxx", ".inc"}


def generated_targets(listing):
    targets = []
    for line in listing.splitlines():
        target, separator, rule = line.rpartition(": ")
        if separator and rule == "CUSTOM_COMMAND" and Path(target).suffix in SOURCE_SUFFIXES:
            targets.append(target)
    return sorted(set(targets))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--build-root", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    build = a.build_root.resolve()
    raw_build = (Path.home() / "kicad-live/build").resolve()
    if build == raw_build or build.is_relative_to(raw_build):
        p.error("never generate in the read-only live build")
    if not (Path.home() / "kicad-port.build-lock").is_dir():
        p.error("acquire the shared build lock first")
    listing_command = ["ninja", "-C", str(build), "-t", "targets", "all"]
    listing = subprocess.check_output(listing_command, encoding="utf-8")
    targets = generated_targets(listing)
    command = ["ninja", "-C", str(build), "-j8", *targets]
    a.output.mkdir(parents=True, exist_ok=True)
    with (a.output / "generated-inputs.log").open("w", encoding="utf-8") as log:
        # No default/all target is used: only actual generator outputs from
        # Ninja's own graph, plus their generator executable prerequisites.
        code = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False).returncode if targets else 0
    result = {"schema_version": 1, "listing_command": listing_command,
              "generation_command": command, "targets": targets, "exit_code": code,
              "scope": "Ninja CUSTOM_COMMAND source/header outputs only; no KiCad library/application build or install"}
    write_json(a.output / "generated-inputs.json", result)
    print(json.dumps({"targets": len(targets), "exit_code": code}))
    return code


if __name__ == "__main__":
    sys.exit(main())
