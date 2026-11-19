# SPDX-License-Identifier: GPL-3.0-or-later
"""Identity and lifetime guards for the disposable Linux acceptance runtime.

Native identity is the complete retained install manifest, not a few named
binaries: shared libraries such as libkicommon carry the editor-common API code
and embed their install prefix, so two installs of one build can differ there.
"""

from contextlib import contextmanager
from hashlib import file_digest, sha256
import json
import os
from pathlib import Path
import select
import subprocess
import time

from qualification import run

# Mappings with no stable file identity: shared memory, never a loaded module.
ANONYMOUS_SHARED = ("/memfd:", "/SYSV", "/dev/shm/")
DELETED = " (deleted)"


def file_hash(path):
    with path.open("rb") as stream:
        return file_digest(stream, "sha256").hexdigest()


def native_identity(install, provenance):
    """Verify every retained install file and the source archive the record names.

    ``provenance`` holds ``files_sha256`` (install-relative paths) together with
    the source revision, description, change blobs and archive, so binary bytes
    and source attribution come from one retained record and cannot be paired
    with a different one. The archive must sit beside the record.
    """
    source = json.loads(provenance.read_text())
    expected = source["files_sha256"]
    required = {"bin/kicad", "bin/kicad-cli", "bin/_eeschema.kiface", "bin/_pcbnew.kiface",
                "lib/libkicommon.so.10.99.0"}
    if not required <= expected.keys():
        raise ValueError("native provenance must cover core binaries and install libraries")
    install = install.resolve()
    actual = {}
    for name, digest in sorted(expected.items()):
        path = install / name
        if not path.resolve().is_relative_to(install):
            raise ValueError(f"native manifest path escapes install: {name}")
        actual[name] = file_hash(path)
        if actual[name] != digest:
            raise ValueError(f"retained native provenance does not match selected install: {name}")
    archive = source["source_archive"]
    archive_path = provenance.parent / Path(archive["filename"]).name
    if file_hash(archive_path) != archive["sha256"]:
        raise ValueError("retained native source archive does not match its provenance record")
    return {"install": str(install), "files_sha256": actual,
            "retained_provenance_exact_bytes_sha256": sha256(provenance.read_bytes()).hexdigest(),
            "source_revision": source["source_revision"],
            "source_description": source["source_description"],
            "source_archive": archive, "source_changes": source["source_changes"],
            "scope": "every retained install file and the source archive verified; "
                     "source contents asserted by the retained archive/record, not rebuilt"}


def dependency_identity(search_path):
    """Freeze private dependency bytes before launch; no claim about their source."""
    paths = {path.resolve() for directory in search_path.split(os.pathsep) if directory
             for path in Path(directory).rglob("*.so*") if path.is_file()}
    return {str(path): file_hash(path) for path in sorted(paths)}


def loaded_identity(pid, install, native, dependencies, maps=None):
    """Check every loaded install module and private dependency, and hash system files.

    Read /proc only for the manager PID returned by our own launch. Anonymous and
    shared-memory mappings have no file identity. A deleted mapped file, or any
    install file absent from the retained manifest, is a verification failure.
    """
    if maps is None:
        maps = Path(f"/proc/{pid}/maps").read_text()
    paths, shared = set(), set()
    for line in maps.splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) != 6 or not fields[5].startswith("/"):
            continue
        name = fields[5]
        if name.startswith(ANONYMOUS_SHARED):
            shared.add(name)
        elif name.endswith(DELETED):
            raise ValueError(f"owned manager maps a deleted file: {name}")
        else:
            paths.add(Path(name).resolve())
    install = install.resolve()
    loaded = {"install": {}, "private_dependencies": {}, "system": {}, "special": [],
              "anonymous_shared": sorted(shared)}
    for path in sorted(paths):
        if not path.is_file():
            # Device nodes and similar are not module identities; never read them.
            loaded["special"].append(str(path))
            continue
        digest = file_hash(path)
        if path.is_relative_to(install):
            name = path.relative_to(install).as_posix()
            if native["files_sha256"].get(name) != digest:
                raise ValueError(f"loaded native module absent from or differs from provenance: {path}")
            loaded["install"][name] = digest
        elif str(path) in dependencies:
            if dependencies[str(path)] != digest:
                raise ValueError(f"loaded private dependency changed after preflight: {path}")
            loaded["private_dependencies"][str(path)] = digest
        else:
            loaded["system"][str(path)] = digest
    required = {"bin/kicad", "bin/_eeschema.kiface", "bin/_pcbnew.kiface", "lib/libkicommon.so.10.99.0"}
    if not required <= loaded["install"].keys():
        raise ValueError("owned manager did not load both verified editor modules and libkicommon")
    return loaded


def display_number(read_fd, timeout):
    """Wait for one Xvfb display line with a deadline, including a partial line."""
    deadline = time.monotonic() + timeout
    data = b""
    while b"\n" not in data:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([read_fd], [], [], remaining)[0]:
            raise TimeoutError("owned Xvfb display allocation timed out")
        chunk = os.read(read_fd, 128)
        if not chunk:
            raise RuntimeError("owned Xvfb closed display pipe without a display")
        data += chunk
        if len(data) > 128:
            raise RuntimeError("invalid owned Xvfb display response")
    number = data.strip().decode("ascii")
    if not number.isdigit():
        raise RuntimeError("owned Xvfb did not allocate a display")
    return number


def cleanup_steps(report, steps):
    """Attempt each independent cleanup; preserve all failures in the receipt."""
    for name, action in steps:
        try:
            action()
        except BaseException as error:
            report.setdefault("cleanup_errors", []).append(f"{name}: {type(error).__name__}: {error}")
            report["result"] = "failed"


@contextmanager
def owned_display(output, report, timeout=20):
    """Start a private Xvfb inside the cleanup scope; stop it even if startup stalls."""
    proc = log = None
    read_fd, write_fd = os.pipe()
    try:
        log = (output / "xvfb.log").open("w")
        proc = subprocess.Popen(
            ["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1600x1200x24", "-nolisten", "tcp"],
            pass_fds=(write_fd,), stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        os.close(write_fd)
        write_fd = None
        yield display_number(read_fd, timeout)
    finally:
        steps = [("Xvfb", lambda: run.stop(proc, log))] if proc is not None else []
        if proc is None and log is not None:
            steps.append(("Xvfb log", log.close))
        steps.append(("display pipe", lambda: os.close(read_fd)))
        if write_fd is not None:
            steps.append(("display pipe writer", lambda: os.close(write_fd)))
        cleanup_steps(report, steps)
