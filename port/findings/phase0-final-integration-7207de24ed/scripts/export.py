#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Export one committed integration revision as raw LF Git bytes for the WSL proof.

Writes repo.tar (whole HEAD, core.autocrlf=false), the pinned oracle stage via the
unchanged port/oracle/prepare.py, and identity reports comparing every exported
byte with the Git blob it came from.
"""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import threading

repo = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2]).resolve()
# text=auto attributes follow core.eol, which is CRLF natively on Windows; a Linux
# checkout (CI) writes LF. Both settings are needed for raw committed bytes.
env = dict(os.environ, GIT_CONFIG_COUNT="2", GIT_CONFIG_KEY_0="core.autocrlf",
           GIT_CONFIG_VALUE_0="false", GIT_CONFIG_KEY_1="core.eol", GIT_CONFIG_VALUE_1="lf")
sha = lambda b: hashlib.sha256(b).hexdigest()


def git(*args, binary=False):
    return subprocess.run(["git", *args], cwd=repo, env=env, check=True,
                          capture_output=True, text=not binary).stdout


def blob_hashes(revision, paths=()):
    """sha256 of every raw blob in revision (optionally limited to paths)."""
    listing = git("ls-tree", "-r", "-z", "--full-tree", revision, "--", *paths, binary=True)
    entries = []
    for row in listing.split(b"\0"):
        if not row:
            continue
        meta, path = row.split(b"\t", 1)
        mode, kind, oid = meta.split()
        if kind == b"blob":
            entries.append((path.decode(), oid.decode(), mode.decode()))
    batch = subprocess.Popen(["git", "cat-file", "--batch"], cwd=repo, env=env,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    # Feed object IDs from a thread: writing them all first deadlocks on full pipes.
    def feed():
        batch.stdin.write("".join(oid + "\n" for _, oid, _ in entries).encode())
        batch.stdin.close()
    writer = threading.Thread(target=feed)
    writer.start()
    result = {}
    for path, oid, mode in entries:
        header = batch.stdout.readline().split()
        size = int(header[2])
        data = batch.stdout.read(size)
        batch.stdout.read(1)
        result[path] = (sha(data), mode)
    writer.join()
    if batch.wait() != 0:
        raise SystemExit("cat-file failed")
    return result


if git("status", "--porcelain").strip():
    raise SystemExit("worktree is not clean; export only committed bytes")
head = git("rev-parse", "HEAD").strip()
out.mkdir(parents=True, exist_ok=False)
archive = out / "repo.tar"
git("archive", "--format=tar", "-o", str(archive), "HEAD")

# Whole-tree identity: tar member bytes against raw blobs.
blobs = blob_hashes("HEAD")
exported, differs, links = {}, [], []
with tarfile.open(archive) as tar:
    for member in tar:
        if member.isfile():
            data = tar.extractfile(member).read()
            exported[member.name] = sha(data)
        elif member.issym():
            links.append(member.name)
for path, (digest, mode) in blobs.items():
    if mode == "120000":
        continue
    if exported.get(path) != digest:
        differs.append(path)
missing = sorted(set(p for p, (_, m) in blobs.items() if m != "120000") - set(exported))
head_report = {"head": head, "archive_sha256": sha(archive.read_bytes()),
               "blob_files": len(blobs), "exported_files": len(exported),
               "symlinks": len(links), "missing": missing,
               "bytes_differ_from_blob": sorted(differs)}
(out / "head-identity.json").write_text(json.dumps(head_report, indent=2) + "\n")

# Pinned oracle stage through the unchanged prepare.py, with the same raw config.
subprocess.run([sys.executable, str(repo / "port/oracle/prepare.py"), "--output-dir",
                str(out / "stage")], cwd=repo, env=env, check=True)
manifest = json.loads((out / "stage/source-manifest.json").read_text(encoding="utf-8"))
sys.path.insert(0, str(repo / "port/oracle"))
from pin import KICAD_PIN, SOURCE_PATHS  # noqa: E402
pin_blobs = blob_hashes(KICAD_PIN, SOURCE_PATHS)
pin_files = {p: d for p, (d, m) in pin_blobs.items() if m != "120000"}
pin_report = {"kicad_pin": KICAD_PIN, "overlay_revision": manifest["overlay_revision"],
              "overlay_dirty_at_export": manifest["overlay_dirty_at_export"],
              "manifest_files": len(manifest["files"]), "pin_blob_files": len(pin_files),
              "pin_symlinks": len(pin_blobs) - len(pin_files),
              "archive_sha256": manifest["archive_sha256"],
              "manifest_sha256": sha((out / "stage/source-manifest.json").read_bytes()),
              "differ_from_blob": sorted(p for p in pin_files if manifest["files"].get(p) != pin_files[p]),
              "not_exported": sorted(set(pin_files) - set(manifest["files"])),
              "extra_exported": sorted(set(manifest["files"]) - set(pin_files)),
              "source_paths": list(SOURCE_PATHS)}
(out / "pin-identity.json").write_text(json.dumps(pin_report, indent=2) + "\n")
assert manifest["overlay_revision"] == head and manifest["overlay_dirty_at_export"] is False
print(json.dumps({k: (v if not isinstance(v, list) or len(v) < 8 else len(v))
                  for k, v in {**head_report, **pin_report}.items()}, indent=2))
