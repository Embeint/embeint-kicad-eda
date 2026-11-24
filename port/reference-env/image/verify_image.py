#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check that a running image is exactly the locked candidate environment.

Runs inside the image at build time (writing --record) and again before every
probe or oracle run (comparing with the record). Any difference is a failure;
nothing here accepts a near match.
"""

import argparse
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import stat
import tomllib

LIB_DIRS = (
    "/usr/lib/x86_64-linux-gnu",
    "/lib/x86_64-linux-gnu",
    "/usr/local/lib",
    "/usr/lib",
    "/lib",
)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(*command, env=None):
    return subprocess.run(command, check=True, capture_output=True, text=True, env=env).stdout


def dpkg_excludes():
    """path-exclude/path-include rules from the base image's dpkg configuration."""
    rules = []
    directory = Path("/etc/dpkg/dpkg.cfg.d")
    for path in sorted(directory.iterdir()) if directory.is_dir() else []:
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line.startswith(("path-exclude", "path-include")):
                kind, pattern = line.split("=", 1) if "=" in line else line.split(None, 1)
                rules.append((kind.strip(), pattern.strip()))
    return rules


def excluded(path, rules):
    state = False
    for kind, pattern in rules:
        if fnmatch.fnmatchcase(path, pattern):
            state = kind == "path-exclude"
    return state


def check_packages(lock, failures, *, verify_files=False):
    output = run("dpkg-query", "-W", "-f", "${Package}\t${Architecture}\t${Version}\t${Status}\n")
    installed = {}
    for line in output.splitlines():
        name, arch, version, status = line.split("\t")
        if status == "install ok installed":
            installed[name] = (arch, version)
        elif status not in ("deinstall ok config-files", "unknown ok not-installed"):
            failures.append(f"package {name} in state {status!r}")
    expected = {p["name"]: (p["architecture"], p["version"]) for p in lock["packages"]["final"]}
    for name in sorted(set(installed) | set(expected)):
        if installed.get(name) != expected.get(name):
            failures.append(
                f"package {name}: installed {installed.get(name)}, locked {expected.get(name)}"
            )
    if verify_files:
        verify_package_files(failures)
    return hashlib.sha256(output.encode()).hexdigest()


def verify_package_files(failures):
    # Build-time check as root: a host uid cannot read every package-owned file.
    # Runtime checks retain the exact package list and compiler/runtime hashes.
    # Paths dropped by the base's path-exclude rules were never installed.
    rules = dpkg_excludes()
    verify = subprocess.run(["dpkg", "--verify"], capture_output=True, text=True)
    problems = [
        line for line in verify.stdout.splitlines() if not excluded(line.split()[-1], rules)
    ]
    if verify.returncode not in (0, 1) or verify.stderr.strip():
        failures.append("dpkg --verify failed: " + verify.stderr.strip())
    failures.extend("dpkg --verify: " + line for line in problems)


def check_files(lock, failures):
    record = {}
    for item in lock["toolchain_files"]:
        path = Path(item["path"])
        resolved = path.resolve()
        if str(resolved) != item["resolved_path"]:
            failures.append(f"{path} resolves to {resolved}, locked {item['resolved_path']}")
            continue
        actual = sha256(resolved)
        record[item["path"]] = actual
        if actual != item["sha256"]:
            failures.append(f"{path}: sha256 {actual}, locked {item['sha256']}")
    libm = lock["libm"]
    debug = Path(libm["debug_file"])
    if not debug.is_file() or sha256(debug) != libm["debug_file_sha256"]:
        failures.append("libm detached debug file missing or different: " + str(debug))
    return record


def check_loader(candidate, failures):
    for directory in LIB_DIRS:
        hwcaps = Path(directory) / "glibc-hwcaps"
        if hwcaps.exists():
            failures.append(f"{hwcaps} exists: the loader could select ISA-specific libraries")
    if Path("/etc/ld.so.preload").exists():
        failures.append("/etc/ld.so.preload exists")
    for name in ("LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT", "LD_BIND_NOW", "LD_HWCAP_MASK"):
        if name in os.environ:
            failures.append(f"{name} is set")
    tunables = os.environ.get("GLIBC_TUNABLES")
    if tunables != candidate["dispatch"]["glibc_tunables"]:
        failures.append(
            f"GLIBC_TUNABLES is {tunables!r}, contract {candidate['dispatch']['glibc_tunables']!r}"
        )


def check_compiler(candidate, failures):
    toolchain = candidate["toolchain"]
    record = {}
    for driver, real in (
        ("gcc", "x86_64-linux-gnu-gcc-13"),
        ("g++", "x86_64-linux-gnu-g++-13"),
        ("cc", "x86_64-linux-gnu-gcc-13"),
        ("c++", "x86_64-linux-gnu-g++-13"),
    ):
        path = Path(run("sh", "-c", f"command -v {driver}").strip()).resolve()
        if path.name != real:
            failures.append(f"{driver} resolves to {path}, expected {real}")
        version = run(driver, "-dumpfullversion").strip()
        if version != toolchain["gcc_version"]:
            failures.append(f"{driver} version {version}, contract {toolchain['gcc_version']}")
        record[driver] = {"path": str(path), "version": version}
    # Predefined macros as the oracle compiles: CMake Release flags plus the fixed FP flags.
    flags = ["-O3", "-DNDEBUG", "-std=gnu++20", *toolchain["oracle_flags"]]
    macros = {}
    for line in run(
        "sh", "-c", "g++ " + " ".join(flags) + " -dM -E -x c++ - < /dev/null"
    ).splitlines():
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[0] == "#define":
            macros[parts[1]] = parts[2] if len(parts) == 3 else ""
    for name in toolchain["required_macros"]:
        if name not in macros:
            failures.append("compiler does not define " + name)
    for name in toolchain["forbidden_macros"]:
        if name in macros:
            failures.append("compiler defines " + name)
    if macros.get("__FLT_EVAL_METHOD__") != toolchain["flt_eval_method"]:
        failures.append("__FLT_EVAL_METHOD__ is " + repr(macros.get("__FLT_EVAL_METHOD__")))
    record["predefined_macros_sha256"] = hashlib.sha256(
        "".join(f"{k}={v}\n" for k, v in sorted(macros.items())).encode()
    ).hexdigest()
    record["target_options"] = [
        line.strip()
        for line in run("gcc", "-Q", "--help=target").splitlines()
        if line.strip().startswith(("-march=", "-mtune=", "-mfpmath="))
    ]
    if not any(option.split() == ["-march=", "x86-64"] for option in record["target_options"]):
        failures.append("gcc default -march is not x86-64: " + repr(record["target_options"]))
    libc = run("getconf", "GNU_LIBC_VERSION").strip()
    if libc != "glibc " + toolchain["glibc_version"]:
        failures.append("glibc runtime is " + libc)
    record["glibc"] = libc
    runtime = Path("/usr/lib/x86_64-linux-gnu/libstdc++.so.6").resolve().name
    if runtime != toolchain["libstdcxx_runtime_soname"]:
        failures.append("libstdc++ runtime is " + runtime)
    record["libstdcxx_runtime"] = runtime
    return record


def rust_inventory(prefix, failures):
    """Hash compiler/linker libraries and installation manifests at stable paths.

    Documentation/completions are not compiler inputs. install.log is an append
    log, not an input, and carries build-specific installation paths. All other
    files under bin/lib/libexec/etc are covered, including all rustlib targets.
    Symlinks must name regular files inside this prefix; directory links and
    special files are rejected instead of hiding an unscanned subtree.
    """
    files = {}
    for top in ("bin", "lib", "libexec", "etc"):
        directory = prefix / top
        if directory.is_symlink():
            failures.append("Rust input root is a symlink: " + top)
            continue
        for path in sorted(directory.rglob("*")):
            relative = path.relative_to(prefix).as_posix()
            if relative == "lib/rustlib/install.log":
                if path.is_symlink() or not path.is_file():
                    failures.append("Rust install.log must be a regular file")
                continue
            logical = "/usr/local/" + relative
            if path.is_symlink():
                resolved = path.resolve()
                if not resolved.is_relative_to(prefix.resolve()) or not resolved.is_file():
                    failures.append(
                        "Rust symlink escapes input closure or is not a regular file: " + logical
                    )
                    continue
                files[logical] = {
                    "kind": "symlink",
                    "target": os.readlink(path),
                    "resolved": "/usr/local/" + resolved.relative_to(prefix.resolve()).as_posix(),
                    "sha256": sha256(resolved),
                }
            elif path.is_file():
                files[logical] = {
                    "kind": "file",
                    "sha256": sha256(path),
                    "mode": stat.S_IMODE(path.stat().st_mode),
                }
            elif not path.is_dir():
                failures.append("Rust special file: " + logical)
    return dict(sorted(files.items()))


def check_rust(lock, failures, *, prefix=Path("/usr/local"), expected=None):
    version = run("rustc", "-vV")
    fields = dict(line.split(": ", 1) for line in version.splitlines() if ": " in line)
    if (
        fields.get("release") != lock["rust"]["version"]
        or fields.get("host") != "x86_64-unknown-linux-gnu"
    ):
        failures.append("rustc is " + repr(fields))
    installed = (prefix / "lib/rustlib/components").read_text(encoding="utf-8").split()
    wanted = sorted(
        f"{c['name']}-{c['target']}" if c["name"] == "rust-std" else c["name"]
        for c in lock["rust"]["components"]
    )
    if sorted(installed) != wanted:
        failures.append("Rust component membership differs from the lock")
    for name in wanted:
        manifest = prefix / "lib/rustlib" / ("manifest-" + name)
        if not manifest.is_file() or manifest.is_symlink():
            failures.append("Rust installation manifest missing: " + name)
            continue
        seen = set()
        for line in manifest.read_text(encoding="utf-8").splitlines():
            kind, separator, target = line.partition(":")
            path = Path(target)
            if (
                not separator
                or kind not in ("file", "dir")
                or target in seen
                or ".." in path.parts
                or not path.is_relative_to(prefix)
                or not path.resolve().is_relative_to(prefix.resolve())
            ):
                failures.append("Invalid Rust installation manifest entry: " + line)
                continue
            seen.add(target)
            if not path.exists() or (kind == "file" and not path.is_file()):
                failures.append("Missing Rust manifest input: " + target)
    files = rust_inventory(prefix, failures)
    for target in ("x86_64-unknown-linux-gnu", "wasm32-unknown-unknown"):
        if not any((prefix / "lib/rustlib" / target / "lib").glob("libstd-*.rlib")):
            failures.append("Rust target stdlib missing: " + target)
    for name in (
        "bin/rustc",
        "bin/cargo",
        "lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld",
        "lib/rustlib/rust-installer-version",
    ):
        if "/usr/local/" + name not in files:
            failures.append("Rust required input missing: " + name)
    result = {
        "rustc": fields,
        "cargo": run("cargo", "-V").strip(),
        "components": sorted(installed),
        "inputs": files,
    }
    if expected is not None and result != expected:
        failures.append("Rust installed input closure differs from the image record")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--record", type=Path, help="write the identity record (image build)")
    parser.add_argument("--expect", type=Path, help="compare with the build-time identity record")
    args = parser.parse_args()
    lock = json.loads(args.lock.read_text(encoding="utf-8"))
    candidate = tomllib.loads(args.candidate.read_text(encoding="utf-8"))
    failures = []
    if lock["candidate_sha256"] != sha256(args.candidate):
        failures.append("candidate.toml differs from the lock's candidate_sha256")
    expected = (
        json.loads(args.expect.read_text(encoding="utf-8")) if args.expect is not None else None
    )
    identity = {
        "schema": 1,
        "status": lock["status"],
        "lock_sha256": sha256(args.lock),
        "candidate_sha256": sha256(args.candidate),
        "dpkg_query_sha256": check_packages(lock, failures, verify_files=args.record is not None),
        "toolchain_files_sha256": check_files(lock, failures),
        "compiler": check_compiler(candidate, failures),
        "rust": check_rust(lock, failures, expected=expected["rust"] if expected else None),
        "glibc_tunables": os.environ.get("GLIBC_TUNABLES"),
    }
    check_loader(candidate, failures)
    if args.expect is not None:
        if expected != identity:
            failures.append("identity differs from the image build record " + str(args.expect))
    if failures:
        for failure in failures:
            print("FAIL: " + failure, file=sys.stderr)
        return 1
    text = json.dumps(identity, indent=2, sort_keys=True) + "\n"
    if args.record is not None:
        args.record.write_text(text, encoding="utf-8")
    print(
        f"Image matches lock {identity['lock_sha256'][:16]}: "
        f"{len(lock['packages']['final'])} packages, {len(identity['toolchain_files_sha256'])} identity files"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
