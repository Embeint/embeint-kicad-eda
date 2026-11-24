# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic pre-installation inventory, bound by the hashed lock.

The bare base has no Python. Its shell verifier checks this inventory's digest
from the already hashed lock, then checks ALL files before reading any controls.
"""
import hashlib
from pathlib import PurePosixPath
import re


def digest(data):
    return hashlib.sha256(data).hexdigest()


def basename(url):
    name = PurePosixPath(url).name
    if not re.fullmatch(r"[A-Za-z0-9_.+%~:-]+", name):
        raise ValueError("Unsafe input basename")
    return name


def plan(lock):
    files = {}
    controls = {}

    def add(path, sha):
        if path in files or not re.fullmatch(r"[0-9a-f]{64}", sha):
            raise ValueError("Duplicate input or bad digest: " + path)
        files[path] = sha

    snapshot = lock["snapshot"]
    suites = sorted(snapshot["suites"])
    components = sorted(next(iter(snapshot["suites"].values()))["indices"])
    for suite in suites:
        if not re.fullmatch(r"[a-z0-9-]+", suite):
            raise ValueError("Unsafe suite")
        record = snapshot["suites"][suite]
        add("InRelease-" + suite, record["inrelease_sha256"])
        if sorted(record["indices"]) != components:
            raise ValueError("Inconsistent snapshot components")
        for component, index in sorted(record["indices"].items()):
            if not re.fullmatch(r"[a-z0-9-]+", component):
                raise ValueError("Unsafe component")
            add(f"Packages-{suite}-{component}.gz", index["sha256"])
    if snapshot["architecture"] != "amd64" or not re.fullmatch(
        r"/[A-Za-z0-9/._-]+", snapshot["keyring"]
    ):
        raise ValueError("Unexpected snapshot architecture/keyring")
    controls["snapshot.env"] = (
        f"SUITES='{' '.join(suites)}'\nCOMPONENTS='{' '.join(components)}'\nARCH='amd64'\n"
        f"KEYRING='{snapshot['keyring']}'\nKEYRING_SHA256='{snapshot['keyring_sha256']}'\n"
    ).encode()
    controls["inrelease.sha256"] = "".join(
        f"{snapshot['suites'][s]['inrelease_sha256']}  InRelease-{s}\n" for s in suites
    ).encode()
    debs = []
    for entry in lock["packages"]["install"]:
        add("debs/" + basename(entry["filename"]), entry["sha256"])
        debs.append(
            " ".join(
                str(entry[k]) for k in ("filename", "sha256", "name", "version", "architecture")
            )
        )
    controls["debs.list"] = ("\n".join(debs) + "\n").encode()
    rust = lock["rust"]
    for name, key in (
        ("channel.toml", "channel_manifest_sha256"),
        ("channel.toml.asc", "channel_signature_sha256"),
        ("rust-key.gpg", "signing_keyring_sha256"),
    ):
        add("rust/" + name, rust[key])
    installers = []
    for component in rust["components"]:
        name = basename(component["url"])
        if not name.endswith(".tar.xz"):
            raise ValueError("Unexpected Rust component archive")
        add("rust/" + name, component["sha256"])
        installers.append(name + " " + name.removesuffix(".tar.xz"))
    controls["rust-components.list"] = ("\n".join(installers) + "\n").encode()
    controls["rust.sha256"] = "".join(
        f"{sha}  {path[5:]}\n" for path, sha in files.items() if path.startswith("rust/")
    ).encode()
    for name, data in controls.items():
        add(name, digest(data))
    inventory = "".join(f"{sha}  {path}\n" for path, sha in sorted(files.items())).encode()
    return controls, inventory
