# SPDX-License-Identifier: GPL-3.0-or-later
"""Make an Ubuntu development .deb from a staged native install and frozen MCP."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ENVIRONMENT = r'''#!/bin/sh
# Shared desktop/MCP configuration and discovery namespace for this distribution.
export KICAD_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}/embeint-kicad"
runtime="${XDG_RUNTIME_DIR:-/tmp}/embeint-kicad-$(id -u)"
if ! mkdir -m 700 -- "$runtime" 2>/dev/null; then
    if [ -L "$runtime" ] || [ ! -d "$runtime" ] ||
       [ "$(stat -c '%u:%a' -- "$runtime")" != "$(id -u):700" ]; then
        echo 'The Embeint KiCad runtime directory is not owned by this user with mode 700.' >&2
        exit 1
    fi
fi
export TMPDIR="$runtime"
export LD_LIBRARY_PATH="/opt/embeint-kicad/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
'''


def executable(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)
    path.chmod(0o755)


def native_provenance(install, manifest):
    """Bind native source/build identity to the actual staged payload."""
    info = json.loads(manifest.read_text())
    if not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", info.get("source_revision", "")):
        raise ValueError("native provenance requires a full source_revision")
    for key in ("application_version", "build", "platform", "source_archive"):
        if not info.get(key):
            raise ValueError(f"native provenance requires {key}")
    archive = info["source_archive"]
    if not archive.get("filename") or not re.fullmatch(r"[0-9a-f]{64}", archive.get("sha256", "")):
        raise ValueError("native source_archive requires filename and SHA-256")
    hashes = info.get("files_sha256", {})
    required = {"bin/kicad", "bin/kicad-cli", "bin/_eeschema.kiface", "bin/_pcbnew.kiface"}
    if not required <= hashes.keys():
        raise ValueError("native provenance omits core executable hashes")
    install = install.resolve()
    for name, digest in hashes.items():
        path = install / name
        if Path(name).is_absolute() or not path.resolve().is_relative_to(install):
            raise ValueError(f"native provenance path escapes install: {name}")
        if not path.is_file() or sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f"native payload does not match provenance: {name}")
    staged = set()
    aliases = {}
    for path in install.rglob("*"):
        name = path.relative_to(install).as_posix()
        if path.is_symlink():
            target = path.resolve()
            if (not target.is_relative_to(install) or not target.is_file()
                    or target.relative_to(install).as_posix() not in hashes):
                raise ValueError(f"native payload symlink requires a recorded file target: {name}")
            aliases[name] = str(path.readlink())
        elif path.is_file():
            staged.add(name)
    recorded = set(hashes) - set(aliases)
    if staged != recorded:
        raise ValueError(f"native provenance file inventory differs from staged payload: "
                         f"unrecorded={sorted(staged - recorded)}, "
                         f"unstaged={sorted(recorded - staged)}")
    return {**info, "symlinks": aliases} if aliases else info


def private_shlibs_entry(soname):
    # dpkg accepts stable libname.so.VERSION and unstable libname-VERSION.so
    # names. NumPy's bundled OpenBLAS uses the latter with a content hash.
    versioned = re.fullmatch(r"(.+)\.so\.(.+)", soname) or re.fullmatch(r"(.+)-(\d.*)\.so", soname)
    return f"{versioned[1]} {versioned[2]} embeint-kicad" if versioned else None


def shared_library_dependencies(binaries, debian):
    libraries = sorted({str(path.parent) for path in binaries})
    private = set()
    for path in binaries:
        dynamic = subprocess.check_output(["readelf", "-d", str(path)], text=True)
        soname = re.search(r"\(SONAME\).*\[(.+)\]", dynamic)
        entry = private_shlibs_entry(soname[1]) if soname else None
        if entry:
            private.add(entry)
    (debian / "shlibs.local").write_text("\n".join(sorted(private)) + "\n")
    command = ["dpkg-shlibdeps", "-O", "-xembeint-kicad", *["-l" + folder for folder in libraries],
               *["-e" + str(path) for path in binaries]]
    output = subprocess.check_output(command, cwd=debian.parent, text=True)
    dependencies = next(line.split("=", 1)[1] for line in output.splitlines()
                        if line.startswith("shlibs:Depends="))
    return set(dependencies.split(", ")) if dependencies else set()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("install", type=Path, help="native prefix built for /opt/embeint-kicad")
    parser.add_argument("mcp", type=Path, help="frozen MCP directory")
    parser.add_argument("output", type=Path, help="new .deb path")
    parser.add_argument("--version", required=True, help="Debian version, including source revision")
    parser.add_argument("--native-provenance", type=Path, required=True,
                        help="native source, patch, build/platform and staged-file hash manifest")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("output already exists")
    for required in (args.install / "bin/kicad", args.mcp / "embeint-kicad-mcp", args.mcp / "build-info.json"):
        if not required.is_file():
            parser.error(f"missing input: {required}")
    try:
        native = native_provenance(args.install, args.native_provenance)
    except (OSError, ValueError, TypeError) as error:
        parser.error(str(error))
    arch = subprocess.check_output(["dpkg", "--print-architecture"], text=True).strip()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="embeint-deb-", dir=output.parent) as temp:
        stage = Path(temp)
        prefix = stage / "opt/embeint-kicad"
        shutil.copytree(args.install, prefix, symlinks=True)
        shutil.copytree(args.mcp, prefix / "libexec/mcp", symlinks=True)
        executable(prefix / "libexec/environment.sh", ENVIRONMENT)
        executable(stage / "usr/bin/embeint-kicad", '''#!/bin/sh
set -eu
. /opt/embeint-kicad/libexec/environment.sh
/opt/embeint-kicad/libexec/mcp/embeint-kicad-mcp --configure-desktop
exec /opt/embeint-kicad/bin/kicad "$@"
''')
        executable(stage / "usr/bin/embeint-kicad-mcp", '''#!/bin/sh
set -eu
. /opt/embeint-kicad/libexec/environment.sh
exec /opt/embeint-kicad/libexec/mcp/embeint-kicad-mcp "$@"
''')
        desktop = stage / "usr/share/applications/embeint-kicad.desktop"
        desktop.parent.mkdir(parents=True)
        desktop.write_text('''[Desktop Entry]
Type=Application
Name=Embeint KiCad (development)
Comment=Embeint live electronic design development build
Exec=embeint-kicad %f
Terminal=false
Categories=Development;Electronics;
''')
        # Compute native runtime dependencies from the actual ELF payload. Private
        # KiCad and frozen-Python libraries travel in the same package.
        debian = stage / "debian"
        debian.mkdir()
        control = """Source: embeint-kicad
Section: electronics
Priority: optional
Maintainer: Embeint Pty Ltd <karl@embeint.com>

Package: embeint-kicad
Architecture: any
Description: Embeint KiCad development distribution with bundled MCP
"""
        (debian / "control").write_text(control)
        metadata = stage / "DEBIAN"
        metadata.mkdir()
        # dpkg-shlibdeps needs this marker to identify $ORIGIN paths in the
        # package build tree. The final dependency control is written below.
        (metadata / "control").write_text(f"Package: embeint-kicad\nVersion: {args.version}\nArchitecture: {arch}\n")
        binaries = []
        for path in prefix.rglob("*"):
            if path.is_file() and not path.is_symlink():
                with path.open("rb") as stream:
                    if stream.read(4) == b"\x7fELF":
                        binaries.append(path)
        # The frozen interpreter has its own library search namespace. Its
        # bundled libstdc++/libgcc cannot satisfy the desktop's dependencies.
        frozen = prefix / "libexec/mcp"
        dependencies = set()
        for bundled in (False, True):
            group = [path for path in binaries if path.is_relative_to(frozen) == bundled]
            dependencies.update(shared_library_dependencies(group, debian))
        (metadata / "control").write_text(
            f"Package: embeint-kicad\nVersion: {args.version}\nArchitecture: {arch}\n"
            "Maintainer: Embeint Pty Ltd <karl@embeint.com>\nSection: electronics\nPriority: optional\n"
            f"Depends: {', '.join(sorted(dependencies))}, kicad-symbols, kicad-footprints\n"
            "Description: Embeint KiCad development distribution with bundled MCP\n"
            " Isolated configuration and a bundled stdio MCP runtime. Development build.\n")
        shutil.rmtree(debian)
        (prefix / "distribution.json").write_text(json.dumps({
            "package_version": args.version, "architecture": arch,
            "mcp": json.loads((args.mcp / "build-info.json").read_text()),
            "native": native,
            "packaging": {"script_sha256": sha256(Path(__file__).read_bytes()).hexdigest()},
            "release_qualified": False,
        }, indent=2) + "\n")
        subprocess.run(["dpkg-deb", "--build", "--root-owner-group", str(stage), str(output)], check=True)
    print(output)


if __name__ == "__main__":
    main()
