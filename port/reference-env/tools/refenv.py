#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Shared helpers for the reference environment tools (standard library only)."""

import hashlib
import base64
import io
import json
import lzma
from pathlib import Path
import shutil
import struct
import subprocess
import tarfile
import tomllib
import urllib.request

HERE = Path(__file__).resolve().parent
ENV_DIR = HERE.parent
CANDIDATE = ENV_DIR / "candidate.toml"
LOCK = ENV_DIR / "inputs.lock.json"
USER_AGENT = "embeint-kicad-reference-env/1"


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


def download(url, destination, expected_sha256):
    """Download to destination unless an identical file is already present."""
    destination = Path(destination)
    if destination.is_file() and sha256_file(destination) == expected_sha256:
        return False
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".partial")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    digest = hashlib.sha256()
    with urllib.request.urlopen(request, timeout=300) as response, open(partial, "wb") as stream:
        for chunk in iter(lambda: response.read(1 << 20), b""):
            digest.update(chunk)
            stream.write(chunk)
    if digest.hexdigest() != expected_sha256:
        partial.unlink()
        raise ValueError(f"SHA-256 mismatch for {url}")
    partial.replace(destination)
    return True


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


class Elf64:
    """Minimal little-endian ELF64 reader: sections, symbols and the build ID."""

    def __init__(self, data):
        if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
            raise ValueError("Not a little-endian ELF64 file")
        self.data = data
        (self.shoff,) = struct.unpack_from("<Q", data, 0x28)
        self.shentsize, self.shnum, self.shstrndx = struct.unpack_from("<HHH", data, 0x3A)
        self.sections = []
        for index in range(self.shnum):
            fields = struct.unpack_from("<IIQQQQIIQQ", data, self.shoff + index * self.shentsize)
            self.sections.append(fields)
        names = self.sections[self.shstrndx]
        self.names = [self._string(names[4], s[0]) for s in self.sections]

    def _string(self, offset, index):
        end = self.data.index(b"\0", offset + index)
        return self.data[offset + index:end].decode()

    def section(self, name):
        for section_name, fields in zip(self.names, self.sections):
            if section_name == name:
                return fields
        return None

    def build_id(self):
        note = self.section(".note.gnu.build-id")
        if note is None:
            return None
        offset = note[4]
        namesz, descsz, kind = struct.unpack_from("<III", self.data, offset)
        if kind != 3:
            raise ValueError("Unexpected build-id note type")
        start = offset + 12 + ((namesz + 3) & ~3)
        return self.data[start:start + descsz].hex()

    def function_symbols(self):
        """Yield (name, value, binding) for FUNC/IFUNC symbols in .symtab."""
        symtab = self.section(".symtab")
        if symtab is None:
            return
        strtab = self.sections[symtab[6]]
        entsize = symtab[9] or 24
        for offset in range(symtab[4], symtab[4] + symtab[5], entsize):
            name_index, info, _other, _shndx, value, _size = struct.unpack_from(
                "<IBBHQQ", self.data, offset)
            kind = info & 0xF
            if kind not in (2, 10) or not name_index:
                continue
            yield self._string(strtab[4], name_index), value, info >> 4


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
