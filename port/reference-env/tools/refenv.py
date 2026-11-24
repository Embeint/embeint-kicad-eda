#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Shared helpers for the reference environment tools (standard library only)."""

import hashlib
import base64
import io
import http.client
import json
import lzma
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tomllib
import time
import urllib.error
import urllib.request

HERE = Path(__file__).resolve().parent
ENV_DIR = HERE.parent
CANDIDATE = ENV_DIR / "candidate.toml"
LOCK = ENV_DIR / "inputs.lock.json"
USER_AGENT = "embeint-kicad-reference-env/1"

# The ELF reader is shared with the in-image probe (libm_elf.py is copied there).
sys.path.insert(0, str(ENV_DIR / "probe"))
from libm_elf import Elf64  # noqa: E402,F401


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_candidate(path=CANDIDATE):
    with open(path, "rb") as stream:
        return tomllib.load(stream)


def load_lock(path=LOCK):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def dump_json(value):
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def fetch(url, headers=None, expected_sha256=None):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, **(headers or {})})
    with urllib.request.urlopen(request, timeout=300) as response:
        data = response.read()
    if expected_sha256 is not None and sha256_bytes(data) != expected_sha256:
        raise ValueError(f"SHA-256 mismatch for {url}")
    return data


def download(url, destination, expected_sha256, alternate_urls=()):
    """Fetch locked bytes, retrying transient transport failures without relaxing the hash."""
    destination = Path(destination)
    if destination.is_file() and sha256_file(destination) == expected_sha256:
        return False
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".partial")
    urls = (url, *alternate_urls)
    for url_index, location in enumerate(urls):
        for attempt in range(3):
            request = urllib.request.Request(location, headers={"User-Agent": USER_AGENT})
            digest = hashlib.sha256()
            try:
                with urllib.request.urlopen(request, timeout=90) as response, open(partial, "wb") as stream:
                    for chunk in iter(lambda: response.read(1 << 20), b""):
                        digest.update(chunk)
                        stream.write(chunk)
                if digest.hexdigest() != expected_sha256:
                    raise ValueError(f"SHA-256 mismatch for {location}")
                partial.replace(destination)
                return True
            except urllib.error.HTTPError as error:
                if error.code not in (429, 500, 502, 503, 504):
                    raise
                if attempt == 2 and url_index == len(urls) - 1:
                    raise
            except (urllib.error.URLError, TimeoutError, ConnectionError, http.client.IncompleteRead):
                if attempt == 2 and url_index == len(urls) - 1:
                    raise
            finally:
                partial.unlink(missing_ok=True)
            print(f"Transient download failure: {location} (attempt {attempt + 1}/3)", file=sys.stderr)
            if attempt < 2:
                time.sleep(2 ** (attempt + 1))


def inrelease_sha256(text):
    """Map index path -> (sha256, size) from the SHA256 section of an InRelease file."""
    entries = {}
    active = False
    for line in text.splitlines():
        if line.startswith("SHA256:"):
            active = True
            continue
        if active:
            if not line.startswith(" "):
                active = False
                continue
            digest, size, path = line.split()
            entries[path] = (digest, int(size))
    if not entries:
        raise ValueError("InRelease has no SHA256 section")
    return entries


def inrelease_field(text, field):
    for line in text.splitlines():
        if line.startswith(field + ":"):
            return line.split(":", 1)[1].strip()
    return None


def ar_members(data):
    if not data.startswith(b"!<arch>\n"):
        raise ValueError("Not an ar archive")
    offset = 8
    while offset < len(data):
        header = data[offset:offset + 60]
        name = header[:16].decode().strip().rstrip("/")
        size = int(header[48:58].decode().strip())
        start = offset + 60
        yield name, data[start:start + size]
        offset = start + size + (size & 1)


def deb_data_tar(deb_bytes):
    """Return a tarfile for a .deb data member (zstd needs the zstd program)."""
    for name, payload in ar_members(deb_bytes):
        if not name.startswith("data.tar"):
            continue
        if name == "data.tar.zst":
            if shutil.which("zstd") is None:
                raise RuntimeError("zstd is required to inspect Ubuntu .deb files")
            payload = subprocess.run(["zstd", "-dc"], input=payload, capture_output=True,
                                     check=True).stdout
        elif name == "data.tar.xz":
            payload = lzma.decompress(payload)
        elif name != "data.tar":
            raise ValueError("Unsupported deb data member " + name)
        return tarfile.open(fileobj=io.BytesIO(payload))
    raise ValueError("No data member in deb")


def tar_file_bytes(archive, path):
    """Read a file from a deb data tar, following symbolic links within it."""
    wanted = "." + path if path.startswith("/") else path
    for _ in range(8):
        member = archive.getmember(wanted)
        if member.issym():
            target = member.linkname
            if not target.startswith("/"):
                target = str(Path(wanted).parent / target)
                parts = []
                for part in target.split("/"):
                    if part == "..":
                        parts.pop()
                    elif part not in ("", "."):
                        parts.append(part)
                target = "/" + "/".join(parts)
            wanted = "." + target
            continue
        if not member.isfile():
            raise ValueError("Not a regular file in deb: " + path)
        return wanted[1:], archive.extractfile(member).read()
    raise ValueError("Too many symbolic links: " + path)


def dearmor(text):
    """Decode an ASCII-armoured OpenPGP block (the CRC line is not needed by gpgv)."""
    body = []
    inside = False
    headers = True
    for line in text.splitlines():
        if line.startswith("-----BEGIN PGP"):
            inside = True
            continue
        if line.startswith("-----END PGP"):
            break
        if not inside:
            continue
        if headers:
            if not line.strip():
                headers = False
            elif ":" not in line:
                headers = False
                body.append(line.strip())
            continue
        if line.startswith("="):
            continue
        body.append(line.strip())
    if not body:
        raise ValueError("No armoured OpenPGP data")
    return base64.b64decode("".join(body))


def openpgp_primary_fingerprint(data):
    """Version 4 fingerprint of the first public-key packet in binary OpenPGP data."""
    offset = 0
    while offset < len(data):
        header = data[offset]
        offset += 1
        if header & 0x40:
            tag = header & 0x3F
            length = data[offset]
            offset += 1
            if 192 <= length < 224:
                length = ((length - 192) << 8) + data[offset] + 192
                offset += 1
            elif length == 255:
                length = int.from_bytes(data[offset:offset + 4], "big")
                offset += 4
            elif length >= 224:
                raise ValueError("Partial OpenPGP packet lengths are not expected in a key")
        else:
            tag = (header >> 2) & 0xF
            size = (1, 2, 4)[header & 3]
            length = int.from_bytes(data[offset:offset + size], "big")
            offset += size
        body = data[offset:offset + length]
        offset += length
        if tag == 6:
            if body[0] != 4:
                raise ValueError("Only version 4 OpenPGP keys are supported")
            return hashlib.sha1(b"\x99" + len(body).to_bytes(2, "big") + body).hexdigest().upper()
    raise ValueError("No public-key packet")
