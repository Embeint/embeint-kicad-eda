#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compare two probe output directories bit for bit (for example two hosts).

Reference observations (everything produced under the dispatch contract, the
exported sweep bits and the image identity) must be byte-identical. Any
difference is listed and the exit status is 1; there is no tolerance and no
field is dropped. Host-default dispatch runs and host descriptions are expected
to differ between CPUs; they are reported separately and do not affect the
result, because they are not reference observations.
"""

import argparse
import json
from pathlib import Path
import sys
import artifacts

REFERENCE_FILES = ("primitive-contract.json", "primitive-contract.sweep.bin", "rust-contract.json",
                   "rust-contract.sweep.bin", "matrix-contract.json")
HOST_FILES = ("primitive-host-default.json", "primitive-host-default.sweep.bin")


def json_differences(left, right, path="$"):
    if type(left) is not type(right):
        return [(path, left, right)]
    if isinstance(left, dict):
        found = []
        for key in sorted(set(left) | set(right)):
            if key not in left or key not in right:
                found.append((f"{path}.{key}", left.get(key), right.get(key)))
            else:
                found.extend(json_differences(left[key], right[key], f"{path}.{key}"))
        return found
    if isinstance(left, list):
        if len(left) != len(right):
            return [(path + ".length", len(left), len(right))]
        found = []
        for index, (a, b) in enumerate(zip(left, right)):
            found.extend(json_differences(a, b, f"{path}[{index}]"))
        return found
    return [] if left == right else [(path, left, right)]


def compare_file(left, right):
    if not left.exists() or not right.exists():
        return [("presence", left.exists(), right.exists())]
    if left.suffix == ".json":
        return json_differences(artifacts.load(left),
                                artifacts.load(right))
    a, b = left.read_bytes(), right.read_bytes()
    if not a or not b or len(a) % 24 or len(b) % 24:
        return [("invalid sweep length", len(a), len(b))]
    if a == b:
        return []
    if len(a) != len(b):
        return [("length", len(a), len(b))]
    return [(f"record {i // 24} word {(i % 24) // 8}", a[i:i + 8][::-1].hex(), b[i:i + 8][::-1].hex())
            for i in range(0, len(a), 8) if a[i:i + 8] != b[i:i + 8]]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("left", type=Path)
    parser.add_argument("right", type=Path)
    parser.add_argument("--show", type=int, default=20, help="differences printed per file")
    args = parser.parse_args()
    failed = False
    try:
        for directory in (args.left, args.right):
            artifacts.validate(directory)
    except (ValueError, KeyError, TypeError, OSError, StopIteration) as error:
        print("Invalid probe evidence: " + str(error), file=sys.stderr)
        return 1
    for group, names, fatal in (("reference", REFERENCE_FILES, True), ("host-dependent", HOST_FILES, False)):
        for name in names:
            left, right = args.left / name, args.right / name
            if name == "matrix-contract.json" and not left.exists() and not right.exists():
                print(f"{group} {name}: absent in both (no oracle build supplied)")
                continue
            differences = compare_file(left, right)
            status = "identical" if not differences else f"{len(differences)} differences"
            print(f"{group} {name}: {status}")
            for where, a, b in differences[:args.show]:
                print(f"  {where}: {a!r} != {b!r}")
            failed = failed or (fatal and bool(differences))
    identity = [artifacts.load(d / "environment.json")["image_identity"]
                for d in (args.left, args.right)]
    differences = json_differences(*identity)
    print("reference image identity: " + ("identical" if not differences else f"{len(differences)} differences"))
    failed = failed or bool(differences)
    hosts = [artifacts.load(d / "environment.json")["host"]["cpu"]
             for d in (args.left, args.right)]
    print(f"hosts: {hosts[0].get('model name')!r} / {hosts[1].get('model name')!r} (informational)")
    print("RESULT: " + ("reference observations differ" if failed else "reference observations identical"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
