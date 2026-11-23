#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Record the environment a probe or oracle run executed in (JSON on stdout).

Host-specific facts (kernel, CPU model, microcode, CPUID-derived loader
features, container runtime hints) are recorded separately from the image's
locked identity. Environment variable values are recorded only for a fixed list
of relevant names; other variables are listed by name, so secrets are not copied.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

LOADER = "/lib64/ld-linux-x86-64.so.2"
RECORDED_VARIABLES = ("GLIBC_TUNABLES", "LANG", "LC_ALL", "TZ", "PATH", "HOME", "PYTHONDONTWRITEBYTECODE",
                      "SOURCE_DATE_EPOCH", "ORACLE_JOBS", "CC", "CXX", "CFLAGS", "CXXFLAGS", "LDFLAGS",
                      "RUSTFLAGS", "CARGO_BUILD_JOBS")


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def output(command, env=None):
    result = subprocess.run(command, capture_output=True, text=True, env=env)
    return {"command": command, "returncode": result.returncode, "stdout": result.stdout,
            "stderr": result.stderr}


def cpu():
    text = Path("/proc/cpuinfo").read_text(encoding="utf-8", errors="replace")
    first = text.split("\n\n", 1)[0]
    fields = {}
    for line in first.splitlines():
        if ":" in line:
            key, value = line.split(":", 1)
            fields[key.strip()] = value.strip()
    keep = ("vendor_id", "cpu family", "model", "model name", "stepping", "microcode", "flags", "bugs")
    return {"logical_cpus": text.count("processor\t:"), **{k: fields.get(k) for k in keep}}


def libm_symbols(debug_file):
    """Multiarch implementation symbols from the installed detached debug file (nm)."""
    result = output(["nm", "--defined-only", debug_file])
    table = {}
    for line in result["stdout"].splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[1] in ("t", "T", "i") and parts[2].startswith("__") and any(
                parts[2].endswith(suffix) for suffix in ("_sse2", "_sse41", "_sse4_1", "_avx", "_avx2",
                                                          "_fma", "_fma4", "_avx512", "_evex")):
            table.setdefault(f"0x{int(parts[0], 16):x}", []).append(parts[2])
    return {offset: sorted(names) for offset, names in sorted(table.items())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", type=Path, help="primitive probe output naming loaded objects")
    parser.add_argument("--lock", type=Path, default=Path("/opt/refenv/inputs.lock.json"))
    args = parser.parse_args()
    lock = json.loads(args.lock.read_text(encoding="utf-8"))
    contract_env = dict(os.environ)
    default_env = {k: v for k, v in contract_env.items() if k != "GLIBC_TUNABLES"}
    record = {
        "schema": 1,
        "host": {
            "uname": platform.uname()._asdict(),
            "cpu": cpu(),
            "container_hints": {
                "dockerenv": Path("/.dockerenv").exists(),
                "cgroup": Path("/proc/self/cgroup").read_text(encoding="utf-8", errors="replace"),
                "binfmt_misc": sorted(p.name for p in Path("/proc/sys/fs/binfmt_misc").glob("*"))
                if Path("/proc/sys/fs/binfmt_misc").is_dir() else None,
            },
        },
        "loader": {
            "contract_diagnostics": output([LOADER, "--list-diagnostics"], contract_env),
            "host_default_diagnostics": output([LOADER, "--list-diagnostics"], default_env),
            "contract_help": output([LOADER, "--help"], contract_env),
            "host_default_help": output([LOADER, "--help"], default_env),
        },
        "environment": {
            "recorded": {name: os.environ.get(name) for name in RECORDED_VARIABLES},
            "names": sorted(os.environ),
            "umask": f"{os.umask(0o022):03o}",
        },
        "tools": {name: output(command) for name, command in {
            "gcc": ["gcc", "-v"], "g++": ["g++", "--version"], "ld": ["ld", "--version"],
            "as": ["as", "--version"], "cmake": ["cmake", "--version"], "ninja": ["ninja", "--version"],
            "python3": ["python3", "--version"], "rustc": ["rustc", "-vV"], "cargo": ["cargo", "-V"],
            "git": ["git", "--version"], "wx-config": ["wx-config", "--version-full"],
            "dpkg-query": ["dpkg-query", "-W", "-f", "${Package}\t${Architecture}\t${Version}\n"],
        }.items()},
        "image_identity": json.loads(Path("/opt/refenv/image-identity.json").read_text(encoding="utf-8")),
        "os_release": Path("/usr/lib/os-release").read_text(encoding="utf-8"),
    }
    os.umask(int(record["environment"]["umask"], 8))
    debug = lock["libm"]["debug_file"]
    installed = libm_symbols(debug)
    record["libm"] = {"debug_file": debug, "multiarch_symbols_from_nm": installed,
                      "matches_lock": installed == lock["libm"]["multiarch_symbols"]}
    if args.probe is not None:
        probe = json.loads(args.probe.read_text(encoding="utf-8"))
        record["loaded_objects"] = {name: {"resolved": str(Path(name).resolve()),
                                           "sha256": sha256(Path(name).resolve())}
                                    for name in probe["loaded_objects"] if name.startswith("/")}
    print(json.dumps(record, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
