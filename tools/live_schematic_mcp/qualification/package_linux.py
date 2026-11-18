# SPDX-License-Identifier: GPL-3.0-or-later
"""Make an Ubuntu development .deb from a staged native install and frozen MCP."""

import argparse
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("install", type=Path, help="native prefix built for /opt/embeint-kicad")
    parser.add_argument("mcp", type=Path, help="frozen MCP directory")
    parser.add_argument("output", type=Path, help="new .deb path")
    parser.add_argument("--version", required=True, help="Debian version, including source revision")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("output already exists")
    for required in (args.install / "bin/kicad", args.mcp / "embeint-kicad-mcp", args.mcp / "build-info.json"):
        if not required.is_file():
            parser.error(f"missing input: {required}")
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
        binaries = []
        for path in prefix.rglob("*"):
            if path.is_file() and not path.is_symlink():
                with path.open("rb") as stream:
                    if stream.read(4) == b"\x7fELF":
                        binaries.append(path)
        libraries = sorted({str(path.parent) for path in binaries})
        private = set()
        for path in binaries:
            dynamic = subprocess.check_output(["readelf", "-d", str(path)], text=True)
            soname = re.search(r"\(SONAME\).*\[(.+)\]", dynamic)
            versioned = re.fullmatch(r"(.+)\.so\.(.+)", soname[1]) if soname else None
            if versioned:
                private.add(f"{versioned[1]} {versioned[2]} embeint-kicad")
        (debian / "shlibs.local").write_text("\n".join(sorted(private)) + "\n")
        command = ["dpkg-shlibdeps", "-O", "-xembeint-kicad", *["-l" + folder for folder in libraries],
                   *["-e" + str(path) for path in binaries]]
        dependency_line = subprocess.check_output(command, cwd=stage, text=True).strip()
        dependencies = next(line.split("=", 1)[1] for line in dependency_line.splitlines()
                            if line.startswith("shlibs:Depends="))
        metadata = stage / "DEBIAN"
        metadata.mkdir()
        (metadata / "control").write_text(
            f"Package: embeint-kicad\nVersion: {args.version}\nArchitecture: {arch}\n"
            "Maintainer: Embeint Pty Ltd <karl@embeint.com>\nSection: electronics\nPriority: optional\n"
            f"Depends: {dependencies}, kicad-symbols, kicad-footprints\n"
            "Description: Embeint KiCad development distribution with bundled MCP\n"
            " Isolated configuration and a bundled stdio MCP runtime. Development build.\n")
        shutil.rmtree(debian)
        (prefix / "distribution.json").write_text(json.dumps({
            "package_version": args.version, "architecture": arch,
            "mcp": json.loads((args.mcp / "build-info.json").read_text()),
            "release_qualified": False,
        }, indent=2) + "\n")
        subprocess.run(["dpkg-deb", "--build", "--root-owner-group", str(stage), str(output)], check=True)
    print(output)


if __name__ == "__main__":
    main()
