#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline consistency check of candidate.toml, inputs.lock.json, the Dockerfile
and the probe tables. No network, no Docker. This proves the recipe is
internally consistent; it does not prove that an image builds or reproduces.
"""

import argparse
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import refenv  # noqa: E402

HEX64 = re.compile(r"^[0-9a-f]{64}$")
DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
REGISTRY_PREFIX = "registry.taileef599.ts.net/embeint-kicad/"


def table(text, name, suffix):
    match = re.search(name + r"[^{\[]*[{\[](.*?)[}\]];", text, re.S)
    if not match:
        raise ValueError("table not found: " + name)
    return [int(value, 16) for value in re.findall(r"0x([0-9a-f]+)" + suffix, match.group(1))]


def check(env_dir=refenv.ENV_DIR):
    failures = []

    def require(condition, message):
        if not condition:
            failures.append(message)

    candidate_path = env_dir / "candidate.toml"
    candidate = refenv.load_candidate(candidate_path)
    lock = refenv.load_lock(env_dir / "inputs.lock.json")
    dockerfile = (env_dir / "Dockerfile").read_text(encoding="utf-8")

    require(lock["schema"] == 1 and candidate["schema_version"] == 1, "unexpected schema")
    require(lock["status"] == "candidate-unqualified", "the lock must stay candidate-unqualified")
    require(lock["candidate_sha256"] == refenv.sha256_file(candidate_path),
            "candidate.toml changed since inputs.lock.json was generated; rerun tools/lock_inputs.py")
    require(candidate["image"]["reference"].startswith(REGISTRY_PREFIX), "image must use the internal registry")
    require(candidate["platform"] == "linux/amd64", "platform must be linux/amd64")

    base = candidate["base"]
    for key in ("index_digest", "manifest_digest"):
        require(DIGEST.match(base[key]) and lock["base_image"][key] == base[key], "base " + key + " differs")
    expected_from = f"FROM {base['repository']}:{base['tag']}@{base['manifest_digest']}"
    from_lines = [line for line in dockerfile.splitlines() if line.startswith("FROM ")]
    require(from_lines == [expected_from], f"Dockerfile must have exactly one {expected_from!r}")
    require(f'org.opencontainers.image.base.digest="{base["manifest_digest"]}"' in dockerfile,
            "Dockerfile base digest label differs")
    tunables = re.findall(r"^ENV GLIBC_TUNABLES=(\S+)$", dockerfile, re.M)
    require(tunables == [candidate["dispatch"]["glibc_tunables"]], "Dockerfile GLIBC_TUNABLES differs")

    snapshot = candidate["snapshot"]
    require(lock["snapshot"]["id"] == snapshot["id"] and lock["snapshot"]["url"] == snapshot["url"],
            "snapshot differs")
    require(sorted(lock["snapshot"]["suites"]) == sorted(snapshot["suites"]), "snapshot suites differ")
    for suite, record in lock["snapshot"]["suites"].items():
        require(HEX64.match(record["inrelease_sha256"]), "bad InRelease hash for " + suite)
        require(sorted(record["indices"]) == sorted(snapshot["components"]), "components differ for " + suite)

    final = {entry["name"]: entry for entry in lock["packages"]["final"]}
    require(len(final) == len(lock["packages"]["final"]), "duplicate final package")
    for entry in final.values():
        require(entry.get("source") and entry.get("source_version"), "missing source identity: " + entry["name"])
    merged = {entry["name"]: entry for entry in lock["packages"]["base"]}
    prefix = f"{snapshot['url']}/{snapshot['id']}/"
    for entry in lock["packages"]["install"]:
        require(HEX64.match(entry["sha256"]), "bad hash for " + entry["name"])
        require(entry["url"] == prefix + entry["filename"], "package URL outside the snapshot: " + entry["name"])
        require(entry["filename"].startswith("pool/") and ".." not in entry["filename"],
                "unexpected filename " + entry["filename"])
        require(entry["requested"] == (entry["name"] in candidate["packages"]["requested"]),
                "requested flag differs for " + entry["name"])
        merged[entry["name"]] = entry
    require({n: (e["version"], e["architecture"]) for n, e in merged.items()}
            == {n: (e["version"], e["architecture"]) for n, e in final.items()},
            "final package set is not base plus install")
    for name in candidate["packages"]["requested"]:
        require(name in final, "requested package missing from the final set: " + name)

    paths = [item["path"] for item in candidate["toolchain_file"]]
    require([item["path"] for item in lock["toolchain_files"]] == sorted(paths), "toolchain file list differs")
    for item in lock["toolchain_files"]:
        require(item["package"] in final and HEX64.match(item["sha256"]), "bad toolchain file " + item["path"])
    names = {name for values in lock["libm"]["multiarch_symbols"].values() for name in values}
    for name in ("__sin_sse2", "__cos_sse2", "__sincos_sse2"):
        require(name in names, "libm table lacks " + name)

    rust = candidate["rust"]
    require(lock["rust"]["version"] == rust["version"], "Rust version differs")
    require(lock["rust"]["signing_key_fingerprint"] == rust["signing_key_fingerprint"], "Rust key differs")
    require([(c["name"], c["target"]) for c in lock["rust"]["components"]]
            == [(c["name"], c["target"]) for c in rust["components"]], "Rust components differ")
    for component in lock["rust"]["components"]:
        require(component["url"].startswith("https://static.rust-lang.org/dist/") and
                HEX64.match(component["sha256"]), "bad Rust component " + component["name"])
        require(f"-{rust['version']}-" in component["url"], "Rust component URL version differs")

    common = (env_dir / "probe/probe_common.h").read_text(encoding="utf-8")
    rust_probe = (env_dir / "probe/rust_probe.rs").read_text(encoding="utf-8")
    for name in ("LIBM_INPUTS", "NAN_OPERANDS"):
        require(table(common, name, "ULL") == table(rust_probe, name, ""), name + " differs between probes")
    for name, value in (("SWEEP_SEED", "0x4b69436164526566"), ("TWO_PI", "6.283185307179586")):
        require(value in common and value in rust_probe, name + " differs between probes")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    failures = check()
    for failure in failures:
        print("FAIL: " + failure, file=sys.stderr)
    if failures:
        return 1
    print("Candidate recipe is internally consistent (not a build or qualification result)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
