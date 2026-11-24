#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Download every locked input into an image build context, verifying digests.

The image build itself then runs with no network. Downloads are cached in
--cache; the context receives exactly the locked files and nothing else.
"""

import argparse
import os
from pathlib import Path
import shutil
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import refenv  # noqa: E402
import input_plan  # noqa: E402


def stage(cache, destination, name, url, sha256, alternate_urls=()):
    cached = cache / sha256[:2] / sha256 / name
    refenv.download(url, cached, sha256, alternate_urls=alternate_urls)
    target = destination / name
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        target.unlink()
    try:
        os.link(cached, target)
    except OSError:
        shutil.copyfile(cached, target)


def fetch_inputs(lock, cache, destination):
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    snapshot = lock["snapshot"]
    root = f"{snapshot['url']}/{snapshot['id']}"
    for suite, record in sorted(snapshot["suites"].items()):
        release_url = f"{root}/dists/{suite}/InRelease"
        # A deterministic query gives a separate transport cache key for cached 5xx
        # responses. The response must still be the original signed, hash-locked file.
        release_alternate = f"{release_url}?sha256={record['inrelease_sha256']}"
        stage(cache, destination, f"InRelease-{suite}", release_url,
              record["inrelease_sha256"], (release_alternate,))
        for component, index in sorted(record["indices"].items()):
            # Same signed index bytes, addressed by their locked digest when the named
            # snapshot endpoint is unavailable. Hash/signature verification is unchanged.
            by_hash = (f"{root}/dists/{suite}/{Path(index['path']).parent}/by-hash/SHA256/"
                       f"{index['sha256']}")
            stage(cache, destination, f"Packages-{suite}-{component}.gz",
                  f"{root}/dists/{suite}/{index['path']}", index["sha256"], (by_hash,))
    lines = []
    for entry in lock["packages"]["install"]:
        name = Path(entry["filename"]).name
        mirror = "https://archive.ubuntu.com/ubuntu/" + entry["filename"]
        stage(cache, destination / "debs", name, entry["url"], entry["sha256"], (mirror,))
        lines.append(" ".join((entry["filename"], entry["sha256"], entry["name"], entry["version"],
                               entry["architecture"])))
    (destination / "debs.list").write_text("\n".join(lines) + "\n", encoding="utf-8")
    rust = lock["rust"]
    stage(cache, destination / "rust", "channel.toml", rust["channel_manifest_url"],
          rust["channel_manifest_sha256"])
    stage(cache, destination / "rust", "channel.toml.asc", rust["channel_signature_url"],
          rust["channel_signature_sha256"])
    key_cache = cache / "rust-key.gpg.ascii"
    refenv.download(rust["signing_key_url"], key_cache, rust["signing_key_sha256"])
    key = refenv.dearmor(key_cache.read_text(encoding="ascii"))
    if refenv.openpgp_primary_fingerprint(key) != rust["signing_key_fingerprint"]:
        raise ValueError("Rust signing key fingerprint differs from the lock")
    # gpgv reads binary keyrings. The image checks these bytes against the lock.
    if refenv.sha256_bytes(key) != rust["signing_keyring_sha256"]:
        raise ValueError("Dearmoured Rust keyring differs from the lock")
    (destination / "rust" / "rust-key.gpg").write_bytes(key)
    rust_lines = [f"{rust['channel_manifest_sha256']}  channel.toml",
                  f"{rust['channel_signature_sha256']}  channel.toml.asc",
                  f"{rust['signing_keyring_sha256']}  rust-key.gpg"]
    for component in lock["rust"]["components"]:
        name = Path(component["url"]).name
        stage(cache, destination / "rust", name, component["url"], component["sha256"])
        rust_lines.append(f"{component['sha256']}  {name}")
    (destination / "rust.sha256").write_text("\n".join(rust_lines) + "\n", encoding="utf-8")
    controls, inventory = input_plan.plan(lock)
    if input_plan.digest(inventory) != lock["verification_plan_sha256"]:
        raise ValueError("Verification inventory differs from the hashed lock")
    for name, data in controls.items():
        (destination / name).write_bytes(data)
    (destination / "inputs.sha256").write_bytes(inventory)
    return len(lines), len(rust["components"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, default=refenv.LOCK)
    parser.add_argument("--cache", type=Path, required=True, help="download cache directory")
    parser.add_argument("--dest", type=Path, required=True, help="context inputs directory (replaced)")
    args = parser.parse_args()
    debs, rust = fetch_inputs(refenv.load_lock(args.lock), args.cache.resolve(), args.dest.resolve())
    print(f"Staged {debs} packages and {rust} Rust components in {args.dest}")


if __name__ == "__main__":
    main()
