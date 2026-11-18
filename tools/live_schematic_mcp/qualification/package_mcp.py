# SPDX-License-Identifier: GPL-3.0-or-later
"""Freeze the current MCP adapter and interpreter into a relocatable directory.

Run with requirements.txt and PyInstaller installed in a dedicated build environment.
The result needs no development checkout, generated files, or user-installed Python.
"""

import argparse
import asyncio
from hashlib import sha256
import importlib.metadata
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import sysconfig
import tempfile

MCP = Path(__file__).resolve().parents[1]


def source_identity(archive_revision):
    try:
        root = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=MCP,
                              text=True, capture_output=True)
        # An archive may be unpacked inside an unrelated checkout. Its enclosing
        # repository cannot identify the source used to build this adapter.
        result = (subprocess.run(["git", "rev-parse", "HEAD"], cwd=MCP,
                                 text=True, capture_output=True)
                  if root.returncode == 0 and Path(root.stdout.strip()).resolve() == MCP.parents[1]
                  else None)
    except FileNotFoundError:
        result = None
    if result is not None and result.returncode == 0:
        revision = result.stdout.strip()
        if archive_revision and archive_revision != revision:
            raise ValueError("the supplied source revision differs from Git HEAD")
        dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=MCP))
        return revision, dirty, "git"
    if not archive_revision or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", archive_revision):
        raise ValueError("a source archive requires --source-revision with its full commit hash")
    return archive_revision, None, "caller-supplied archive revision; inspect source checksums"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new output directory (must not already exist)")
    parser.add_argument("--source-revision", help="source archive's full commit hash when Git metadata is absent")
    args = parser.parse_args()
    if not sys.flags.isolated or sys.prefix == sys.base_prefix:
        parser.error("use a dedicated virtual environment and invoke python -I")
    try:
        revision, dirty, revision_source = source_identity(args.source_revision)
    except ValueError as error:
        parser.error(str(error))
    output = args.output.resolve()
    if output.exists():
        parser.error("output already exists; choose a new directory")
    output.parent.mkdir(parents=True, exist_ok=True)
    env = {key: value for key, value in os.environ.items() if key not in {"PYTHONPATH", "PYTHONHOME"}}
    env["PYTHONNOUSERSITE"] = "1"
    subprocess.run([sys.executable, "-I", str(MCP / "generate_proto.py")], env=env, check=True)
    with tempfile.TemporaryDirectory(prefix="embeint-mcp-build-", dir=output.parent) as temp:
        build = Path(temp)
        subprocess.run([
            sys.executable, "-I", "-m", "PyInstaller", "--onedir", "--name", "embeint-kicad-mcp",
            "--distpath", str(build / "dist"), "--workpath", str(build / "work"),
            "--specpath", str(build), "--paths", str(MCP), "--paths", str(MCP / "_generated"),
            "--collect-all", "pynng", "--collect-all", "shapely",
            "--recursive-copy-metadata", "mcp",
            str(Path(__file__).with_name("packaged_main.py")),
        ], env=env, check=True)
        package = build / "dist/embeint-kicad-mcp"
        notices = package / "licenses"
        notices.mkdir()
        shutil.copyfile(MCP.parents[1] / "LICENSE", notices / "KiCad-GPL.txt")
        python_license = next((path for path in (
            Path(sysconfig.get_path("stdlib")) / "LICENSE.txt",
            Path(sys.base_prefix) / "LICENSE.txt",
        ) if path.is_file()), None)
        if python_license is None:
            raise RuntimeError("the build interpreter's LICENSE.txt is required for distribution")
        shutil.copyfile(python_license, notices / "Python-LICENSE.txt")
        distributions = {}
        for dist in importlib.metadata.distributions():
            name = dist.metadata["Name"]
            distributions[name] = dist.version
            for item in dist.files or []:
                if ".dist-info" not in item.parts[0]:
                    continue
                if "license" not in str(item).lower() and "copying" not in str(item).lower():
                    continue
                source = Path(dist.locate_file(item))
                if source.is_file():
                    destination = notices / name / Path(*item.parts[1:])
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, destination)
        sys.path.insert(0, str(MCP))
        import server
        manifest = asyncio.run(server.live_mcp_capabilities())
        sources = {str(path.relative_to(MCP)): sha256(path.read_bytes()).hexdigest()
                   for path in sorted(MCP.glob("*.py")) if not path.name.startswith(("test_", "e2e_"))}
        entrypoint = Path(__file__).with_name("packaged_main.py")
        sources[str(entrypoint.relative_to(MCP))] = sha256(entrypoint.read_bytes()).hexdigest()
        protocol_sources = {str(path.relative_to(MCP.parents[1])): sha256(path.read_bytes()).hexdigest()
                            for path in sorted((MCP.parents[1] / "api/proto").rglob("*.proto"))}
        info = {"source_revision": revision, "source_dirty": dirty, "source_sha256": sources,
                "revision_source": revision_source, "protocol_source_sha256": protocol_sources,
                "schema_sha256": manifest["schema_sha256"], "tool_count": manifest["tool_count"],
                "command_count": manifest["command_count"], "python": sys.version,
                "dependencies": dict(sorted(distributions.items()))}
        (package / "build-info.json").write_text(json.dumps(info, indent=2, sort_keys=True) + "\n")
        shutil.move(str(package), output)
    print(output)


if __name__ == "__main__":
    main()
