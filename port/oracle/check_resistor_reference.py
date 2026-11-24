#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the selected resistor libm contract before live comparisons/blessing.

This checks runtime identity, loader configuration and a sensitive runtime call.
It is not a new instruction-trace proof or whole-image qualification.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import subprocess

TUNABLES = "glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F"
FILES = {
    "/usr/lib/x86_64-linux-gnu/libm.so.6": "5dcc4d25466e29ca390ed6fabb371c69b65dff6fa4e91c34561057a020d6994a",
    "/usr/lib/x86_64-linux-gnu/libc.so.6": "755a371e86cc9f564ab54f66140747700fb0a103f698f659f2060f707897e09a",
    "/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2": "5fc0940888e9d2e1a00e66d6b1a559c36364ee867683652e1074fa576952cc3f",
}


def check_environment(env, system, machine):
    if (system, machine) != ("Linux", "x86_64"):
        raise ValueError("the resistor reference requires native Linux x86_64")
    if env.get("GLIBC_TUNABLES") != TUNABLES:
        raise ValueError("set the recorded GLIBC_TUNABLES before starting the oracle")
    for name in ("LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT", "LD_ASSUME_KERNEL"):
        if env.get(name):
            raise ValueError(f"unsupported loader override: {name}")


def check_libraries(output, expected):
    loaded = {}
    for line in output.splitlines():
        fields = line.split()
        if len(fields) >= 3 and fields[1] == "=>" and fields[2].startswith("/"):
            loaded[fields[0]] = str(Path(fields[2]).resolve())
        elif fields and fields[0].startswith("/"):
            loaded[Path(fields[0]).name] = str(Path(fields[0]).resolve())
    for path in expected:
        name = Path(path).name
        if loaded.get(name) != str(Path(path).resolve()):
            raise ValueError(f"oracle does not load the selected {name}")
    return loaded


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    check_environment(os.environ, platform.system(), platform.machine())
    hashes = {}
    for name, expected in FILES.items():
        actual = hashlib.sha256(Path(name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"resistor runtime identity changed: {name}")
        hashes[name] = actual
    oracle = args.oracle.resolve(strict=True)
    linkage = subprocess.run(["ldd", str(oracle)], check=True, capture_output=True,
                             text=True, timeout=30)
    loaded = check_libraries(linkage.stdout, FILES)
    libm = ctypes.CDLL("/usr/lib/x86_64-linux-gnu/libm.so.6")
    libm.pow.argtypes = (ctypes.c_double, ctypes.c_double)
    libm.pow.restype = ctypes.c_double
    # A real dynamic call, never the compiler's folded decimal literal.
    bits = struct.unpack("<Q", struct.pack("<d", libm.pow(10.0, 126.0)))[0]
    if bits != 0x5A17A2ECC414A040:
        raise ValueError(f"resistor runtime pow(10,126) is {bits:016x}")
    report = {
        "contract": "resistor-glibc239-sse2-v1", "glibc_tunables": TUNABLES,
        "runtime_sha256": hashes, "oracle_loaded_objects": loaded,
        "oracle_sha256": hashlib.sha256(oracle.read_bytes()).hexdigest(),
        "pow_10_126_bits": f"{bits:016x}",
        "scope": "runtime preflight; live corpus comparison still required",
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    with args.report.open("x", encoding="utf-8") as out:
        json.dump(report, out, indent=2)
        out.write("\n")
    print("resistor runtime contract: matched")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"resistor reference: {error}") from error
