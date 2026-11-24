#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Refuse a live oracle whose compiled behaviour differs from what the port is bound to.

Two results of unit libs.kimath.include.application.pure.p001 are properties of the reference
build rather than of the KiCad source, and the port reproduces the measured build:

1. BEZIER<double> NaN payloads where two NaN operands meet. The port spells out the operand
   order of every arithmetic instruction of the owned sites (rust/kicad-hash/src/
   bezier_f64_sites.rs). This script extracts those instructions from the built adapter object
   (objdump, Intel syntax) and requires them to equal the recorded sequence exactly. Any other
   compiler, flag or adapter change fails here instead of being compared, and the order must be
   re-derived and reviewed, never silently re-recorded.
2. HASH_128::ToString grouping. The runtime's num_put<char> applies the caller's grouping up to
   its first NUL byte (COW-ABI twin shim; rust/kicad-hash/src/hash_128.rs). The linked
   libstdc++ must be one of the measured runtimes, and the adapter must not interpose its own
   __numpunct_cache<char> instantiation.

Every identity check fails closed: there is no option that accepts an unmeasured runtime or a
drifted site. --diagnose-runtime only adds a structural analysis of the libstdc++ grouping path
to the report (it is always printed for an unmeasured runtime); it never changes the exit status.

Usage:
  check_reference_bindings.py --object OBJ --oracle ORACLE [--libstdcxx SO] [--binding JSON]
      [--diagnose-runtime]
  check_reference_bindings.py --record --object OBJ [--binding JSON]   (prints the site
      signature of OBJ; for a reviewed re-derivation only, never to accept drift)

Exit status 0: bound; 1: drift or unknown runtime; 2: usage or tool error.
"""

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
DEFAULT_BINDING = HERE / "reference-bindings.json"

# Functions whose arithmetic holds the owned BEZIER<double> sites in the bound build: the two
# out-of-line members and the adapter dispatcher that inlines point_at and SubCurve.
SITE_FUNCTIONS = (
    "BEZIER<double>::PointAt(double) const",
    "BEZIER<double>::Split(double, BEZIER<double>&, BEZIER<double>&) const",
    "(anonymous namespace)::KimathHeaderHash(",
)
ARITHMETIC = re.compile(r"^(add|sub|mul|div)(sd|pd)$")
HEADER = re.compile(r"^[0-9a-f]+ <(.*)>:$")
INSTRUCTION = re.compile(r"^\s*[0-9a-f]+:\s+(\S+)\s*(.*)$")


def run(command):
    try:
        return subprocess.run(command, check=True, capture_output=True, text=True).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"error: {' '.join(command)}: {error}", file=sys.stderr)
        sys.exit(2)


def sha256(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def site_signature(obj):
    """{function: [normalised arithmetic instruction, ...]} for the site functions of OBJ."""
    listing = run(["objdump", "-d", "-C", "-w", "-M", "intel", "--no-show-raw-insn", obj])
    signature = {}
    current = None
    for line in listing.splitlines():
        header = HEADER.match(line)
        if header:
            name = header.group(1)
            current = None
            if "[clone" not in name:
                for wanted in SITE_FUNCTIONS:
                    if name.startswith(wanted) or (wanted.endswith("(") and wanted in name):
                        current = wanted
                        signature.setdefault(current, [])
            continue
        if current is None:
            continue
        instruction = INSTRUCTION.match(line)
        if instruction and ARITHMETIC.match(instruction.group(1)):
            operands = instruction.group(2).split("#")[0].strip()
            signature[current].append(f"{instruction.group(1)} {operands}")
    return signature


def digest(signature):
    text = json.dumps(signature, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(text.encode()).hexdigest()


def linked_libstdcxx(oracle):
    for line in run(["ldd", oracle]).splitlines():
        match = re.match(r"\s*libstdc\+\+\.so\.6 => (\S+)", line)
        if match:
            return str(pathlib.Path(match.group(1)).resolve())
    print("error: the oracle does not link libstdc++.so.6", file=sys.stderr)
    sys.exit(2)


def dynamic_function(so, symbol):
    """Disassembly of one exported function of SO (located by its dynamic symbol)."""
    for line in run(["nm", "-D", "-S", "--defined-only", so]).splitlines():
        fields = line.split()
        if len(fields) >= 4 and fields[3].split("@")[0] == symbol:
            start = int(fields[0], 16)
            stop = start + int(fields[1], 16)
            return run(["objdump", "-d", "-M", "intel", "--no-show-raw-insn", "-C",
                        f"--start-address={start}", f"--stop-address={stop}", so])
    return ""


def structural_runtime_problems(so):
    """The COW-ABI grouping path, checked in the runtime's own code."""
    problems = []
    grouping = dynamic_function(so, "_ZNKSt8numpunctIcE11do_groupingEv")
    if "<strlen@plt>" not in grouping:
        problems.append("COW numpunct<char>::do_grouping() does not rebuild from a C string")
    cache = dynamic_function(so, "_ZNSt16__numpunct_cacheIcE8_M_cacheERKSt6locale")
    relocations = run(["objdump", "-R", so])
    old_id_slots = {int(line.split()[0], 16) for line in relocations.splitlines()
                    if "GLOB_DAT" in line and "_ZNSt8numpunctIcE2idE" in line}
    referenced = {int(m.group(1), 16) for m in re.finditer(r"# ([0-9a-f]+) <", cache)}
    if not old_id_slots & referenced:
        problems.append("__numpunct_cache<char>::_M_cache does not use the COW numpunct<char>::id")
    if "<strlen@plt>" not in cache:
        problems.append("__numpunct_cache<char>::_M_cache does not size the grouping as a C string")
    pairs = [line.split() for line in relocations.splitlines() if "R_X86_64_64" in line]
    twinned = any(
        "_ZNSt8numpunctIcE2idE" in a[2] and "_ZNSt7__cxx118numpunctIcE2idE" in b[2]
        and int(b[0], 16) - int(a[0], 16) == 8
        for a, b in zip(pairs, pairs[1:]) if len(a) >= 3 and len(b) >= 3)
    if not twinned:
        problems.append("no twinned-facet entry pairing the COW and C++11 numpunct<char>::id")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--object", required=True)
    parser.add_argument("--oracle")
    parser.add_argument("--libstdcxx")
    parser.add_argument("--binding", default=str(DEFAULT_BINDING))
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--diagnose-runtime", action="store_true",
                        help="also report the structural analysis of the libstdc++ grouping "
                             "path (diagnostic only; never changes the exit status)")
    args = parser.parse_args()

    signature = site_signature(args.object)
    if args.record:
        print(json.dumps({"object_sha256": sha256(args.object), "sites": signature,
                          "sites_sha256": digest(signature)}, indent=1))
        return 0
    if not args.oracle:
        parser.error("--oracle is required unless --record")

    binding = json.loads(pathlib.Path(args.binding).read_text())
    failures = []
    notes = []

    expected = binding["bezier"]["sites"]
    for function in SITE_FUNCTIONS:
        got = signature.get(function)
        want = expected.get(function)
        if got != want:
            failures.append(f"BEZIER<double> sites drifted in {function!r}: "
                            f"{len(got or [])} arithmetic instructions vs {len(want or [])} bound")
    if digest(signature) != binding["bezier"]["sites_sha256"]:
        failures.append("BEZIER<double> site signature sha256 differs from the bound build")
    object_sha = sha256(args.object)
    notes.append(f"adapter object sha256 {object_sha}"
                 + (" (the measured object)" if object_sha == binding["bezier"]["object_sha256"]
                    else " (not the measured object; only the site comparison binds it)"))

    defined = run(["nm", "--defined-only", args.object]) + run(
        ["nm", "-D", "--defined-only", args.oracle])
    if "_ZNSt16__numpunct_cacheIcE8_M_cacheERKSt6locale" in defined:
        failures.append("the adapter or oracle defines __numpunct_cache<char>::_M_cache and "
                        "can interpose the runtime's grouping cache")
    undefined = run(["nm", "--undefined-only", args.object])
    if "_ZTVNSt7__cxx118numpunctIcEE" not in undefined:
        failures.append("the adapter's grouping facet is not a C++11-ABI numpunct<char>")

    so = args.libstdcxx or linked_libstdcxx(args.oracle)
    so_sha = sha256(so)
    measured = {entry["sha256"]: entry for entry in binding["locale"]["runtimes"]}
    if so_sha in measured:
        notes.append(f"libstdc++ {so} sha256 {so_sha}: measured runtime "
                     f"{measured[so_sha]['package']}")
    else:
        failures.append(f"libstdc++ {so} sha256 {so_sha} is not a measured runtime; measure and "
                        "review it before it can be bound")
    if args.diagnose_runtime or so_sha not in measured:
        problems = structural_runtime_problems(so)
        notes.append("diagnostic only, not a binding: the libstdc++ grouping path "
                     + (f"has structural problems: {problems}" if problems
                        else "passes the structural checks"))

    for note in notes:
        print(f"note: {note}")
    for failure in failures:
        print(f"FAIL: {failure}")
    print("reference bindings: " + ("drift detected" if failures else "bound"))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
