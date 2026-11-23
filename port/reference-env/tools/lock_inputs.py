#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Resolve candidate.toml into inputs.lock.json from primary official sources.

Network sources: the Docker Hub registry (base manifest, config and layer),
snapshot.ubuntu.com (InRelease, Packages.gz and selected .deb files) and
static.rust-lang.org (the Rust channel manifest). Every downloaded object is
checked against the digest that names it. OpenPGP signatures are verified only
when gpgv is available on this host; the image build always verifies InRelease
with the base image's own gpgv and archive keyring.

This tool records inputs. It does not build the image or qualify anything.
"""

import argparse
import gzip
import io
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))

import debian  # noqa: E402
import refenv  # noqa: E402

REGISTRY = "https://registry-1.docker.io/v2/"
INDEX_TYPES = ("application/vnd.oci.image.index.v1+json, "
               "application/vnd.docker.distribution.manifest.list.v2+json")
MANIFEST_TYPES = ("application/vnd.oci.image.manifest.v1+json, "
                  "application/vnd.docker.distribution.manifest.v2+json")
VARIANT = re.compile(r"^__.+_(sse2|sse41|sse4_1|avx|avx2|fma|fma4|avx512|evex)$")


def docker_token(repository):
    data = refenv.fetch("https://auth.docker.io/token?service=registry.docker.io&scope=repository:"
                        + repository + ":pull")
    return json.loads(data)["token"]


def resolve_base(base):
    repository = base["repository"].removeprefix("docker.io/")
    token = docker_token(repository)
    auth = {"Authorization": "Bearer " + token}
    index_bytes = refenv.fetch(REGISTRY + repository + "/manifests/" + base["index_digest"],
                               {**auth, "Accept": INDEX_TYPES}, base["index_digest"][7:])
    tag_bytes = refenv.fetch(REGISTRY + repository + "/manifests/" + base["tag"],
                             {**auth, "Accept": INDEX_TYPES})
    if refenv.sha256_bytes(tag_bytes) != base["index_digest"][7:]:
        # Tags move; the digest is the pin. Report it without changing the lock.
        print(f"note: tag {base['tag']} no longer resolves to the pinned index", file=sys.stderr)
    index = json.loads(index_bytes)
    amd64 = [m for m in index["manifests"]
             if m.get("platform", {}).get("architecture") == "amd64"
             and m.get("platform", {}).get("os") == "linux"]
    if len(amd64) != 1 or amd64[0]["digest"] != base["manifest_digest"]:
        raise ValueError("Pinned index does not name the pinned linux/amd64 manifest")
    manifest_bytes = refenv.fetch(REGISTRY + repository + "/manifests/" + base["manifest_digest"],
                                  {**auth, "Accept": MANIFEST_TYPES}, base["manifest_digest"][7:])
    manifest = json.loads(manifest_bytes)
    config = json.loads(refenv.fetch(REGISTRY + repository + "/blobs/" + manifest["config"]["digest"],
                                     auth, manifest["config"]["digest"][7:]))
    if config["architecture"] != "amd64" or config["os"] != "linux":
        raise ValueError("Base config is not linux/amd64")
    layers = []
    status = None
    os_release = None
    for layer in manifest["layers"]:
        blob = refenv.fetch(REGISTRY + repository + "/blobs/" + layer["digest"], auth, layer["digest"][7:])
        layers.append({"digest": layer["digest"], "size": layer["size"], "mediaType": layer["mediaType"]})
        with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as archive:
            for name in ("var/lib/dpkg/status", "usr/lib/os-release"):
                try:
                    content = archive.extractfile(name).read().decode()
                except KeyError:
                    continue
                if name.endswith("status"):
                    status = content
                else:
                    os_release = content
    if status is None or len(manifest["layers"]) != 1:
        raise ValueError("Expected one base layer with a dpkg status file")
    annotations = amd64[0].get("annotations", {})
    record = {
        "repository": base["repository"], "tag": base["tag"],
        "index_digest": base["index_digest"], "manifest_digest": base["manifest_digest"],
        "config_digest": manifest["config"]["digest"], "layers": layers,
        "diff_ids": config["rootfs"]["diff_ids"], "created": config["created"],
        "source": annotations.get("org.opencontainers.image.source"),
        "source_revision": annotations.get("org.opencontainers.image.revision"),
        "os_release": dict(line.split("=", 1) for line in os_release.splitlines() if "=" in line),
    }
    installed = {}
    for stanza in debian.parse_paragraphs(status):
        if stanza.get("Status", "").endswith(" installed"):
            installed[stanza["Package"]] = stanza
    return record, installed


def gpgv_verify(inrelease, keyring):
    gpgv = shutil.which("gpgv")
    if gpgv is None or keyring is None:
        return False
    with tempfile.TemporaryDirectory() as directory:
        signed = Path(directory) / "InRelease"
        signed.write_bytes(inrelease)
        subprocess.run([gpgv, "--keyring", str(keyring), str(signed)], check=True, capture_output=True)
    return True


def resolve_snapshot(snapshot, keyring_bytes):
    root = f"{snapshot['url']}/{snapshot['id']}"
    stanzas = []
    suites = {}
    verified = []
    with tempfile.TemporaryDirectory() as directory:
        keyring = None
        if keyring_bytes is not None:
            keyring = Path(directory) / "keyring.gpg"
            keyring.write_bytes(keyring_bytes)
        for suite in snapshot["suites"]:
            inrelease = refenv.fetch(f"{root}/dists/{suite}/InRelease")
            verified.append(gpgv_verify(inrelease, keyring))
            text = inrelease.decode()
            sums = refenv.inrelease_sha256(text)
            indices = {}
            for component in snapshot["components"]:
                path = f"{component}/binary-{snapshot['architecture']}/Packages.gz"
                digest, size = sums[path]
                data = refenv.fetch(f"{root}/dists/{suite}/{path}", expected_sha256=digest)
                indices[component] = {"path": path, "sha256": digest, "size": size}
                for stanza in debian.parse_paragraphs(gzip.decompress(data).decode()):
                    stanza["_suite"] = suite
                    stanzas.append(stanza)
            suites[suite] = {"inrelease_sha256": refenv.sha256_bytes(inrelease),
                             "date": refenv.inrelease_field(text, "Date"), "indices": indices}
    # Signature checking depends on this host having gpgv, so it is reported, not
    # locked. The image build verifies every InRelease with the base image's gpgv.
    print("InRelease signatures " + ("verified with gpgv" if all(verified) else
                                      "NOT verified here (no gpgv); the image build verifies them"),
          file=sys.stderr)
    record = {"url": snapshot["url"], "id": snapshot["id"], "architecture": snapshot["architecture"],
              "keyring": snapshot["keyring"], "suites": suites}
    return record, stanzas


def keyring_from_base(base_record):
    """The archive keyring inside the pinned base layer (bytes), for gpgv."""
    repository = base_record["repository"].removeprefix("docker.io/")
    token = docker_token(repository)
    layer = base_record["layers"][0]["digest"]
    blob = refenv.fetch(REGISTRY + repository + "/blobs/" + layer,
                        {"Authorization": "Bearer " + token}, layer[7:])
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as archive:
        return archive.extractfile("usr/share/keyrings/ubuntu-archive-keyring.gpg").read()


def source_identity(stanza):
    source = stanza.get("Source", stanza["Package"])
    return {"source": source.split(" ")[0],
            "source_version": source.split("(")[1].rstrip(")") if "(" in source else stanza["Version"]}


def package_entry(stanza, snapshot_url, base, requested):
    name = stanza["Package"]
    return {
        "name": name, "version": stanza["Version"], "architecture": stanza["Architecture"],
        **source_identity(stanza),
        "suite": stanza["_suite"], "filename": stanza["Filename"],
        "url": f"{snapshot_url}/{stanza['Filename']}", "sha256": stanza["SHA256"],
        "size": int(stanza["Size"]), "requested": name in requested,
        "replaces_base_version": base[name]["Version"] if name in base else None,
    }


def inspect_debs(candidate, final, stanzas, snapshot_url):
    """Hash identity files and name libm's multiarch implementations from the debs.

    A package kept from the base image is read from the snapshot's .deb of the
    exact base version; the image check then compares the installed bytes.
    """
    versions = {entry["name"]: entry["version"] for entry in final}
    available = {(stanza["Package"], stanza["Version"]): stanza for stanza in stanzas}
    cache = {}

    def deb(name):
        if name not in versions:
            raise ValueError("Toolchain file package is not in the final package set: " + name)
        if name not in cache:
            stanza = available.get((name, versions[name]))
            if stanza is None:
                raise ValueError(f"Snapshot has no .deb for {name} {versions[name]}")
            cache[name] = refenv.deb_data_tar(refenv.fetch(f"{snapshot_url}/{stanza['Filename']}",
                                                           expected_sha256=stanza["SHA256"]))
        return cache[name]

    files = []
    libm_bytes = None
    for item in candidate["toolchain_file"]:
        resolved, data = refenv.tar_file_bytes(deb(item["package"]), item["path"])
        files.append({"package": item["package"], "path": item["path"], "resolved_path": resolved,
                      "sha256": refenv.sha256_bytes(data), "size": len(data)})
        if item["path"].endswith("/libm.so.6"):
            libm_bytes = data
    if libm_bytes is None:
        raise ValueError("candidate.toml must list libm.so.6")
    build_id = refenv.Elf64(libm_bytes).build_id()
    debug_path = f"/usr/lib/debug/.build-id/{build_id[:2]}/{build_id[2:]}.debug"
    _resolved, debug_bytes = refenv.tar_file_bytes(deb("libc6-dbg"), debug_path)
    debug = refenv.Elf64(debug_bytes)
    if debug.build_id() != build_id:
        raise ValueError("libc6-dbg debug file does not match libm build ID")
    variants = {}
    for name, value, _binding in debug.function_symbols():
        if VARIANT.match(name):
            variants.setdefault(f"0x{value:x}", set()).add(name)
    libm = {"path": "/usr/lib/x86_64-linux-gnu/libm.so.6", "build_id": build_id,
            "debug_file": debug_path, "debug_file_sha256": refenv.sha256_bytes(debug_bytes),
            "multiarch_symbols": {offset: sorted(names) for offset, names in sorted(variants.items())}}
    return sorted(files, key=lambda f: f["path"]), libm


def resolve_rust(rust):
    url = f"https://static.rust-lang.org/dist/channel-rust-{rust['version']}.toml"
    manifest_bytes = refenv.fetch(url)
    published = refenv.fetch(url + ".sha256").decode().split()[0]
    if refenv.sha256_bytes(manifest_bytes) != published:
        raise ValueError("Rust channel manifest does not match its published SHA-256")
    import tomllib
    manifest = tomllib.loads(manifest_bytes.decode())
    components = []
    for component in rust["components"]:
        package = manifest["pkg"][component["name"]]
        target = package["target"][component["target"]]
        if not target.get("available"):
            raise ValueError(f"Rust component unavailable: {component}")
        components.append({"name": component["name"], "target": component["target"],
                           "version": package["version"], "url": target["xz_url"],
                           "sha256": target["xz_hash"]})
    if not manifest["pkg"]["rustc"]["version"].startswith(rust["version"] + " "):
        raise ValueError("Rust manifest version differs from the requested version")
    key_bytes = refenv.fetch(rust["signing_key_url"])
    fingerprint = refenv.openpgp_primary_fingerprint(refenv.dearmor(key_bytes.decode()))
    if fingerprint != rust["signing_key_fingerprint"]:
        raise ValueError("Rust signing key fingerprint differs: " + fingerprint)
    signature = refenv.fetch(url + ".asc")
    return {"version": rust["version"], "channel_manifest_url": url,
            "channel_manifest_sha256": published, "date": manifest["date"],
            "channel_signature_url": url + ".asc",
            "channel_signature_sha256": refenv.sha256_bytes(signature),
            "signing_key_url": rust["signing_key_url"],
            "signing_key_sha256": refenv.sha256_bytes(key_bytes),
            "signing_key_fingerprint": fingerprint,
            "signing_keyring_sha256": refenv.sha256_bytes(refenv.dearmor(key_bytes.decode())),
            "components": components}


def build_lock(candidate, candidate_bytes):
    base_record, base = resolve_base(candidate["base"])
    keyring = keyring_from_base(base_record)
    snapshot_record, stanzas = resolve_snapshot(candidate["snapshot"], keyring)
    snapshot_record["keyring_sha256"] = refenv.sha256_bytes(keyring)
    archive = debian.Archive(stanzas, candidate["snapshot"]["architecture"])
    requested = candidate["packages"]["requested"]
    selected, _upgraded = debian.resolve(archive, base, requested,
                                         candidate["packages"].get("providers", {}))
    install = sorted((package_entry(stanza, snapshot_record["url"] + "/" + snapshot_record["id"], base,
                                    requested) for stanza in selected.values()),
                     key=lambda entry: entry["name"])
    base_packages = [{"name": name, "version": stanza["Version"], "architecture": stanza["Architecture"],
                      **source_identity(stanza)} for name, stanza in base.items()]
    final = {entry["name"]: entry
             for entry in base_packages}
    for entry in install:
        final[entry["name"]] = {key: entry[key] for key in ("name", "version", "architecture",
                                                         "source", "source_version")}
    files, libm = inspect_debs(candidate, final.values(), stanzas,
                               snapshot_record["url"] + "/" + snapshot_record["id"])
    return {
        "schema": 1,
        "generated_by": "port/reference-env/tools/lock_inputs.py",
        "status": "candidate-unqualified",
        "candidate": candidate["candidate"],
        "candidate_sha256": refenv.sha256_bytes(candidate_bytes),
        "base_image": base_record,
        "snapshot": snapshot_record,
        "packages": {
            "base": sorted(base_packages, key=lambda e: e["name"]),
            "install": install,
            "final": sorted(final.values(), key=lambda e: e["name"]),
        },
        "rust": resolve_rust(candidate["rust"]),
        "toolchain_files": files,
        "libm": libm,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--candidate", type=Path, default=refenv.CANDIDATE)
    parser.add_argument("--output", type=Path, default=refenv.LOCK)
    parser.add_argument("--check", action="store_true",
                        help="re-resolve from the network and fail if the lock would change")
    args = parser.parse_args()
    candidate_bytes = args.candidate.read_bytes()
    lock = build_lock(refenv.load_candidate(args.candidate), candidate_bytes)
    text = refenv.dump_json(lock)
    if args.check:
        current = args.output.read_text(encoding="utf-8") if args.output.exists() else ""
        if current != text:
            print("inputs.lock.json differs from a fresh resolution", file=sys.stderr)
            return 1
        print("inputs.lock.json matches a fresh resolution")
        return 0
    args.output.write_text(text, encoding="utf-8")
    print(f"Wrote {args.output}: {len(lock['packages']['install'])} packages to install, "
          f"{len(lock['packages']['final'])} in the final set")
    return 0


if __name__ == "__main__":
    sys.exit(main())
