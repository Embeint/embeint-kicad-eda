# SPDX-License-Identifier: GPL-3.0-or-later
"""The Rust binaries of the MCP package: build, bundle, notices and provenance.

package_mcp.py --rust builds two executables from rust/ in this checkout and puts them beside
the frozen Python server:

  embeint-mcp      the native MCP server (roadmap gate 4; rust/MCP.md).  It serves a subset of
                   the tools and is not the default: clients register it explicitly.
  embeint-layout   the pin layout runner.  The frozen server's layout_backend.py looks for it
                   next to its own executable when EMBEINT_LAYOUT_BACKEND is shadow or rust.

The crates are statically linked, so their licence files ship in licenses/rust/<crate>-<version>,
together with the Rust standard library's notices from the toolchain.  build-info.json gets a
"rust" section recording the binaries' hashes, the toolchain, Cargo.lock and the native server's
own capability manifest; check_package.py verifies all of it.
"""

from hashlib import sha256
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

RUST = Path(__file__).resolve().parents[3] / "rust"
# cargo package -> executable it builds
PACKAGES = {"embeint-mcp": "embeint-mcp", "embeint-layout-cli": "embeint-layout"}
LICENCE_FILE = re.compile(r"(?:^|[-_.])(licen[cs]e|copying|copyright|notice|unlicense)(?:$|[-_.])", re.IGNORECASE)
# Standard library notices in <sysroot>/share/doc/rust (installed with the rustc component)
STD_NOTICES = ("COPYRIGHT-library.html", "licenses")
# Upstream licence texts of crates whose published package ships none (notices/README.md)
RUST_NOTICES = Path(__file__).resolve().parent / "notices" / "rust"


def executable_name(name, windows):
    return name + (".exe" if windows else "")


def host_triple():
    text = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return re.search(r"^host: (\S+)$", text, re.MULTILINE).group(1)


def build_environment(revision, target):
    """Preserve caller flags, but link the MSVC runtime into shipped executables."""
    environment = dict(os.environ, EMBEINT_SOURCE_REVISION=revision or "")
    flags = (environment["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
             if "CARGO_ENCODED_RUSTFLAGS" in environment else environment.get("RUSTFLAGS", "").split())
    if target.endswith("-windows-msvc"):
        flags.append("-Ctarget-feature=+crt-static")
        environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    return environment


def build(rust=RUST, revision=None):
    """Build the executables with --locked and return {executable name: path} from cargo's own
    report of what it built, so a stale file in target/ can never be bundled.  revision (e.g.
    g0123456789) becomes part of embeint-mcp's version."""
    target = host_triple()
    environment = build_environment(revision, target)
    # An explicit target keeps crt-static off host proc macros and build scripts.
    command = ["cargo", "build", "--locked", "--release", "--target", target,
               "--message-format=json-render-diagnostics"]
    for package in PACKAGES:
        command += ["-p", package]
    output = subprocess.run(command, cwd=rust, stdout=subprocess.PIPE, text=True, check=True,
                            env=environment).stdout
    built = {}
    for line in output.splitlines():
        message = json.loads(line) if line.startswith("{") else {}
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            built[message["target"]["name"]] = Path(message["executable"])
    missing = set(PACKAGES.values()) - set(built)
    if missing:
        raise RuntimeError(f"cargo did not report building {sorted(missing)}")
    return {name: built[name] for name in PACKAGES.values()}


def parse_tree(text):
    """(name, version) of every crate in `cargo tree --prefix none --format {p}` output."""
    crates = set()
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1].startswith("v"):
            crates.add((parts[0], parts[1][1:]))
    return crates


def linked_crates(rust=RUST):
    """The third-party crates linked into the executables for this host: normal dependencies,
    proc macros and build-time tools excluded.  [{name, version, license, root}]"""
    command = ["cargo", "tree", "--locked", "--edges", "normal,no-proc-macro", "--target", host_triple(),
               "--prefix", "none", "--format", "{p}"]
    for package in PACKAGES:
        command += ["-p", package]
    names = parse_tree(subprocess.run(command, cwd=rust, capture_output=True, text=True, check=True).stdout)
    metadata = json.loads(subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=rust,
                                         capture_output=True, text=True, check=True).stdout)
    crates = []
    if not names:
        raise RuntimeError("cargo tree reported no crates; cannot establish licence coverage")
    known = {(package["name"], package["version"]) for package in metadata["packages"]}
    if names - known:
        raise RuntimeError(f"cargo tree crates missing from metadata: {sorted(names - known)}")
    for package in metadata["packages"]:
        if (package["name"], package["version"]) in names and package["source"] is not None:
            crates.append({"name": package["name"], "version": package["version"],
                           "license": package["license"], "root": Path(package["manifest_path"]).parent})
    if not crates:
        raise RuntimeError("cargo tree reported no third-party crates to license")
    return sorted(crates, key=lambda crate: (crate["name"], crate["version"]))


def licence_files(root):
    return sorted(path for path in root.iterdir() if path.is_file() and LICENCE_FILE.search(path.name))


def copy_notices(crates, destination, sysroot, notices=RUST_NOTICES):
    """licenses/rust: each crate's licence files and the standard library's notices.  A crate
    whose package ships no licence file takes the upstream texts kept for that exact version in
    notices/rust/<name>-<version>/; without them it stops the build: its notice cannot be
    shipped."""
    destination.mkdir(parents=True)
    listed = []
    for crate in crates:
        files = licence_files(crate["root"])
        if not files:
            kept = Path(notices) / f"{crate['name']}-{crate['version']}"
            files = licence_files(kept) if kept.is_dir() else []
        if not files:
            raise RuntimeError(f"{crate['name']} {crate['version']} has no licence file to ship "
                               f"({crate['license']}); add its upstream licence text to "
                               f"qualification/notices/rust/{crate['name']}-{crate['version']}/")
        folder = destination / f"{crate['name']}-{crate['version']}"
        folder.mkdir()
        for source in files:
            shutil.copyfile(source, folder / source.name)
        listed.append({"name": crate["name"], "version": crate["version"], "license": crate["license"],
                       "files": [source.name for source in files]})
    documents = Path(sysroot) / "share" / "doc" / "rust"
    standard = destination / "rust-standard-library"
    standard.mkdir()
    for name in STD_NOTICES:
        source = documents / name
        if source.is_dir():
            shutil.copytree(source, standard / name)
        elif source.is_file():
            shutil.copyfile(source, standard / name)
        else:
            raise RuntimeError(f"the Rust toolchain has no {source}; install the rustc component's "
                               "documentation (rustup component add rust-docs) or another toolchain")
    lines = ["Rust crates statically linked into embeint-mcp and embeint-layout, with the licence",
             "under which each is used (the project is GPL-3.0-or-later; see ../KiCad-GPL.txt).", ""]
    lines += [f"{c['name']} {c['version']}: {c['license']}" for c in listed]
    lines += ["", "rust-standard-library: the Rust standard library (MIT OR Apache-2.0), see",
              "rust-standard-library/COPYRIGHT-library.html."]
    (destination / "README.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return listed


def native_manifest(executable):
    """The native server's own capability manifest (embeint-mcp --parity, no KiCad contact)."""
    reply = subprocess.run([str(executable), "--parity"], input='{"op": "manifest"}\n', capture_output=True,
                           text=True, check=True, timeout=30).stdout
    return json.loads(reply)["ok"]


def check_windows_runtime(executable):
    """Inspect imports instead of relying on the build host's installed VC++ runtime.

    pefile is installed by PyInstaller on Windows in the package build environment.
    """
    import pefile
    with pefile.PE(str(executable)) as pe:
        imports = [entry.dll.decode("ascii").lower() for table in
                   ("DIRECTORY_ENTRY_IMPORT", "DIRECTORY_ENTRY_DELAY_IMPORT")
                   for entry in getattr(pe, table, [])]
    external = [name for name in imports if name.startswith(("vcruntime", "msvcp", "concrt"))]
    if external:
        raise RuntimeError(f"{executable.name} requires an unbundled VC++ runtime: {external}")
    return sorted(imports)


def bundle(package, windows, revision=None, rust=RUST):
    """Build, copy the executables into package and write their notices; build-info's "rust"."""
    built = build(rust, revision)
    binaries = {}
    for name, source in built.items():
        if windows:
            check_windows_runtime(source)
        target = package / executable_name(name, windows)
        shutil.copy2(source, target)
        binaries[target.name] = sha256(target.read_bytes()).hexdigest()
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True,
                             check=True).stdout.strip()
    crates = copy_notices(linked_crates(rust), package / "licenses" / "rust", sysroot)
    manifest = native_manifest(package / executable_name("embeint-mcp", windows))
    rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True, check=True).stdout.strip()
    target = host_triple()
    environment = build_environment(revision, target)
    sources = {path.relative_to(rust).as_posix(): sha256(path.read_bytes()).hexdigest()
               for folder in sorted(rust.glob("embeint-*"))
               for path in sorted(folder.rglob("*")) if path.is_file() and path.suffix in (".rs", ".toml", ".json")}
    for name in ("Cargo.toml", "Cargo.lock", ".cargo/config.toml"):
        if (rust / name).is_file():
            sources[name] = sha256((rust / name).read_bytes()).hexdigest()
    return {"binaries": binaries, "rustc": rustc, "target": target, "source_sha256": sources,
            "cargo": subprocess.run(["cargo", "--version"], capture_output=True, text=True, check=True).stdout.strip(),
            "rustflags": environment.get("RUSTFLAGS"),
            "encoded_rustflags": environment.get("CARGO_ENCODED_RUSTFLAGS"),
            "cargo_lock_sha256": sha256((rust / "Cargo.lock").read_bytes()).hexdigest(),
            "crates": crates,
            "mcp": {"server_version": manifest["server_version"], "schema_sha256": manifest["schema_sha256"],
                    "tool_count": manifest["tool_count"], "tools": [t["name"] for t in manifest["tools"]]}}
