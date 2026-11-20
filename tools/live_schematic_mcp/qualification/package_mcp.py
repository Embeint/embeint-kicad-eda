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
NOTICES = Path(__file__).resolve().with_name("notices")


def notice_manifest():
    """notices/manifest.json: the notices for native code that distribution metadata lacks, and
    the versions they were written for."""
    return json.loads((NOTICES / "manifest.json").read_text(encoding="utf-8"))


def notice_mismatches(manifest, versions, openssl_version_info):
    """Why notices/ does not describe this build environment ([] when it does).  versions maps
    installed distribution names to versions; openssl_version_info is ssl.OPENSSL_VERSION_INFO."""
    problems = []
    for name, entry in manifest["distributions"].items():
        if versions.get(name) != entry["version"]:
            problems.append(f"{name} {versions.get(name, '(not installed)')} is installed, but notices/ "
                            f"covers {name} {entry['version']} ({entry['components']})")
    openssl = manifest["runtime"]["openssl"]
    if openssl_version_info is None or openssl_version_info[0] != openssl["major_version"]:
        problems.append(f"the build interpreter's OpenSSL {openssl_version_info} is not covered by "
                        f"notices/ (OpenSSL {openssl['major_version']}.x)")
    return problems


def shipped_distributions(package):
    """Installed distributions with modules in the frozen package: pure Python modules in the
    executable's archives, extension modules and packages in _internal."""
    from PyInstaller.archive.readers import CArchiveReader
    executable = package / ("embeint-kicad-mcp.exe" if os.name == "nt" else "embeint-kicad-mcp")
    archive = CArchiveReader(str(executable))
    modules = set(archive.toc)
    for name, entry in archive.toc.items():
        if entry[-1] == "z":  # the PYZ archive of pure Python modules
            modules.update(archive.open_embedded_archive(name).toc)
    for entry in (package / "_internal").iterdir():
        # Package folders; not metadata (name.dist-info) or vendored libraries (name.libs)
        if (entry.is_dir() and "." not in entry.name) or entry.suffix in (".pyd", ".so"):
            modules.add(entry.name)
    owners = importlib.metadata.packages_distributions()
    return {dist for module in modules for dist in owners.get(module.split(".")[0], ())}


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
    parser.add_argument("--application-version",
                        help="version of the KiCad build this package ships with (e.g. kicad-cli version)")
    parser.add_argument("--rust", action="store_true",
                        help="also build and bundle the native Rust MCP server and layout runner "
                             "(needs cargo; see rust_bundle.py)")
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
    if args.rust and shutil.which("cargo") is None:
        parser.error("--rust needs cargo (a Rust toolchain) on PATH")
    import ssl
    installed = {dist.metadata["Name"]: dist.version for dist in importlib.metadata.distributions()}
    problems = notice_mismatches(notice_manifest(), installed, ssl.OPENSSL_VERSION_INFO)
    if problems:
        parser.error("; ".join(problems) + ". Update notices/ (see notices/README.md) or install "
                     "the covered versions.")
    output.parent.mkdir(parents=True, exist_ok=True)
    env = {key: value for key, value in os.environ.items() if key not in {"PYTHONPATH", "PYTHONHOME"}}
    env["PYTHONNOUSERSITE"] = "1"
    subprocess.run([sys.executable, "-I", str(MCP / "generate_proto.py")], env=env, check=True)
    sys.path.insert(0, str(MCP))
    import skills
    # Validate before freezing and include only the files advertised by discovery.
    # Do not sweep local configuration or arbitrary files into the distribution.
    skill_content = skills.skill_manifest()
    skill_data = []
    for relative in skill_content["skill_file_sha256"]:
        source = skills.SKILLS_DIR / relative
        destination = "skills/" + str(Path(relative).parent).replace("\\", "/")
        skill_data.extend(["--add-data", f"{source}{os.pathsep}{destination}"])
    with tempfile.TemporaryDirectory(prefix="embeint-mcp-build-", dir=output.parent) as temp:
        build = Path(temp)
        subprocess.run([
            sys.executable, "-I", "-m", "PyInstaller", "--onedir", "--name", "embeint-kicad-mcp",
            "--distpath", str(build / "dist"), "--workpath", str(build / "work"),
            "--specpath", str(build), "--paths", str(MCP), "--paths", str(MCP / "_generated"),
            "--collect-all", "pynng", "--collect-all", "shapely",
            "--hidden-import", "block_helpers",
            "--recursive-copy-metadata", "mcp",
            *skill_data,
            str(Path(__file__).with_name("packaged_main.py")),
        ], env=env, check=True)
        package = build / "dist/embeint-kicad-mcp"
        delivered = {relative: sha256((package / "_internal/skills" / relative).read_bytes()).hexdigest()
                     for relative in skill_content["skill_file_sha256"]}
        if delivered != skill_content["skill_file_sha256"] or skills.skill_manifest() != skill_content:
            raise RuntimeError("skill data changed during packaging or was not delivered exactly")
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
        # Upstream notices for native code linked into extension modules without its own
        # licence file in the distribution metadata (see notices/README.md)
        third_party = notices / "third-party"
        third_party.mkdir()
        for source in sorted(NOTICES.iterdir()):
            if source.is_file():
                shutil.copyfile(source, third_party / source.name)
        # Only what ships: the build environment also holds PyInstaller, pip, pytest's runner
        # dependencies and protocol generators that the frozen server does not include
        shipped = shipped_distributions(package)
        if not {"mcp", "pynng"} <= shipped:
            raise RuntimeError("could not identify the distributions in the frozen package")
        distributions = {}
        for dist in importlib.metadata.distributions():
            name = dist.metadata["Name"]
            if name not in shipped:
                continue
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
        import server
        from support import ADAPTER_VERSION
        manifest = asyncio.run(server.live_mcp_capabilities())
        sources = {str(path.relative_to(MCP)): sha256(path.read_bytes()).hexdigest()
                   for path in sorted(MCP.glob("*.py")) if not path.name.startswith(("test_", "e2e_"))}
        entrypoint = Path(__file__).with_name("packaged_main.py")
        sources[str(entrypoint.relative_to(MCP))] = sha256(entrypoint.read_bytes()).hexdigest()
        protocol_sources = {str(path.relative_to(MCP.parents[1])): sha256(path.read_bytes()).hexdigest()
                            for path in sorted((MCP.parents[1] / "api/proto").rglob("*.proto"))}
        mcp_version = f"{ADAPTER_VERSION}+g{revision[:10]}" + (".dirty" if dirty else "")
        rust = None
        if args.rust:
            sys.path.insert(0, str(Path(__file__).resolve().parent))  # -I leaves the script's folder out
            import rust_bundle
            rust = rust_bundle.bundle(package, os.name == "nt",
                                      f"g{revision[:10]}" + (".dirty" if dirty else ""))
        info = {"mcp_version": mcp_version, "application_version": args.application_version,
                "source_revision": revision, "source_dirty": dirty, "source_sha256": sources,
                "revision_source": revision_source, "protocol_source_sha256": protocol_sources,
                "schema_sha256": manifest["schema_sha256"], "tool_count": manifest["tool_count"],
                "command_count": manifest["command_count"], "python": sys.version,
                "skills": skill_content,
                "dependencies": dict(sorted(distributions.items())),
                "openssl_version": ssl.OPENSSL_VERSION, "third_party_notices": notice_manifest()}
        if rust is not None:
            info["rust"] = rust
        if source_identity(args.source_revision) != (revision, dirty, revision_source):
            raise RuntimeError("source revision or dirty status changed during packaging; rebuild from stable sources")
        (package / "build-info.json").write_text(json.dumps(info, indent=2, sort_keys=True) + "\n")
        shutil.move(str(package), output)
    print(output)


if __name__ == "__main__":
    main()
