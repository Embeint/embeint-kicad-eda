#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Configure the agreed snapshot in a separate tree and account DB differences.

This invokes CMake configuration only. The shared build lock must already be held
by this worker. It never changes the raw source/build tree or invents commands.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extractor.model import digest, write_json


def cmake_options(cache):
    values = {}
    for line in cache.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith(("#", "//")) or ":" not in line or "=" not in line:
            continue
        left, value = line.split("=", 1)
        key, kind = left.split(":", 1)
        if ((key.startswith("KICAD_") and kind in ("BOOL", "STRING")) or key in
                ("CMAKE_BUILD_TYPE", "CMAKE_C_COMPILER", "CMAKE_CXX_COMPILER",
                 "Poppler_Glib_INCLUDE_DIR", "Poppler_Glib_LIBRARY")):
            values[key] = value
    return values


def compare(raw, current, old_root, source_root, raw_build=None, build_root=None):
    def names(rows, root, build):
        def name(row):
            path = row["file"]
            if path.startswith(root.rstrip("/") + "/"):
                return path.removeprefix(root.rstrip("/") + "/")
            if build and path.startswith(build.rstrip("/") + "/"):
                return "@build/" + path.removeprefix(build.rstrip("/") + "/")
            return path
        return Counter(name(r) for r in rows)
    a, b = names(raw, old_root, raw_build), names(current, source_root, build_root)
    return {"raw_command_count": len(raw), "reconciled_command_count": len(current),
            "raw_unique_source_count": len(a), "reconciled_unique_source_count": len(b),
            "raw_only_sources": sorted(a.keys() - b.keys()),
            "reconciled_only_sources": sorted(b.keys() - a.keys()),
            "shared_source_count": len(a.keys() & b.keys()),
            "command_multiplicity_changes": [{"file": path, "raw": a[path], "reconciled": b[path]}
                for path in sorted(a.keys() & b.keys()) if a[path] != b[path]]}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raw-build", type=Path, required=True)
    p.add_argument("--raw-source", type=Path, required=True)
    p.add_argument("--source-root", type=Path, required=True)
    p.add_argument("--build-root", type=Path, required=True)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    raw_build, raw_source, source, build = (x.resolve() for x in
        (a.raw_build, a.raw_source, a.source_root, a.build_root))
    if build == raw_build or build.is_relative_to(raw_build) or source == raw_source:
        p.error("configuration requires a separate source/build tree")
    lock = Path.home() / "kicad-port.build-lock"
    if not lock.is_dir():
        p.error("acquire ~/kicad-port.build-lock before configuration")
    options = cmake_options(raw_build / "CMakeCache.txt")
    for key in ("KICAD_DATA", "KICAD_DOCS", "KICAD_LIBRARY_DATA"):
        if key in options:
            options[key] = options[key].replace(str(raw_build.parent / "install"), str(build.parent / "extract-install"))
    options["CMAKE_INSTALL_PREFIX"] = str(build.parent / "extract-install")
    options["CMAKE_EXPORT_COMPILE_COMMANDS"] = "ON"
    command = ["cmake", "-S", str(source), "-B", str(build), "-G", "Ninja"]
    command += [f"-D{k}={v}" for k, v in sorted(options.items())]
    a.output.mkdir(parents=True, exist_ok=True)
    raw_path = raw_build / "compile_commands.json"
    raw_bytes = raw_path.read_bytes()
    manifest = json.loads(a.manifest.read_text(encoding="utf-8"))
    record = {"schema_version": 1, "source_commit": manifest["source_commit"],
              "upstream_pin": manifest["upstream_pin"], "raw_compile_commands_sha256": digest(raw_bytes),
              "raw_cmake_cache_sha256": digest((raw_build / "CMakeCache.txt").read_bytes()),
              "raw_compile_commands_path": str(raw_path), "configure_command": command,
              "configuration_options_origin": "existing raw CMake cache; install prefix relocated; compile command export enabled",
              "source_root": str(source), "build_root": str(build)}
    with (a.output / "configure.log").open("w", encoding="utf-8") as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
    record["configure_exit_code"] = result.returncode
    if result.returncode == 0:
        current_path = build / "compile_commands.json"
        current_bytes = current_path.read_bytes()
        raw, current = json.loads(raw_bytes), json.loads(current_bytes)
        record.update(compare(raw, current, str(raw_source), str(source), str(raw_build), str(build)))
        record["reconciled_compile_commands_sha256"] = digest(current_bytes)
        record["reconciled_compile_commands_path"] = str(current_path)
        tracked = set(manifest["files"])
        record["stale_deleted_sources"] = sorted(r["file"].removeprefix(str(raw_source) + "/") for r in raw
            if r["file"].startswith(str(raw_source) + "/")
            and r["file"].removeprefix(str(raw_source) + "/") not in tracked
            and not (source / r["file"].removeprefix(str(raw_source) + "/")).exists())
        record["reconciled_missing_sources"] = sorted(r["file"] for r in current if not Path(r["file"]).is_file())
    if raw_path.read_bytes() != raw_bytes:
        raise RuntimeError("read-only raw compile database changed during configuration")
    write_json(a.output / "reconciliation.json", record)
    print(json.dumps(record, indent=2))
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
