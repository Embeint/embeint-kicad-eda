# SPDX-License-Identifier: GPL-3.0-or-later
"""Fresh genuine reference for kicad_math::glibc_trig (CI harness; Linux x86-64 only).

Every check fails closed (exit status 1, message on stderr):

  bind-lock      reference.lock.json must copy the reviewed PR55 input lock exactly: the lock
                 bytes hash to lock_binding.sha256, and the libc6 package entry and the locked
                 loader/libc/libm file hashes and libm build ID agree with it.
  check-domain   the domain file written by the Rust test reference_write_domain has the
                 locked count, SHA-256 and fixed-part SHA-256.
  reference      downloads the one locked libc6 .deb, verifies size and SHA-256 BEFORE
                 extracting it with dpkg-deb -x (no maintainer scripts), verifies the extracted
                 loader, libc and libm, builds probe.c with the host gcc, runs it ONLY through
                 the extracted loader (--inhibit-cache --library-path, cleared environment,
                 exact GLIBC_TUNABLES), and verifies the records: count, input order and input
                 words equal to the domain.
  wasi-test      prints the single wasm32-wasip1 kicad_math library test executable from
                 `cargo test --no-run --message-format=json` on stdin.
  report         hashes every file in the output directory and summarises the comparison logs
                 into report.json; exits 1 unless every expected comparison reported 0
                 mismatches on the full record count.

This measures the hash-pinned package from the reviewed lock. It does not verify Ubuntu
signatures again and does not adopt the PR55 reference image.
"""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

LIB_RESOLVED = "/usr/lib/x86_64-linux-gnu/"
COMPARISONS = [
    "native-debug", "native-release", "wasip1-debug", "wasip1-release",
    "unknown-set-rotation-debug", "unknown-set-rotation-release",
]


class Refused(Exception):
    pass


def need(condition, message):
    if not condition:
        raise Refused(message)


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_lock(path):
    lock = json.loads(Path(path).read_text(encoding="utf-8"))
    need(lock.get("schema") == 1, "reference.lock.json schema")
    return lock


def domain_count(lock):
    return lock["domain"]["fixed_count"] + lock["domain"]["seeded_count"]


# ---- bind-lock

def bind_lock(lock, source_bytes):
    """Check reference.lock.json against the bytes of the reviewed input lock."""
    need(hashlib.sha256(source_bytes).hexdigest() == lock["lock_binding"]["sha256"],
         "input lock bytes do not match lock_binding.sha256")
    source = json.loads(source_bytes)
    pkg = lock["package"]
    installs = [p for p in source["packages"]["install"] if p["name"] == pkg["name"]]
    need(len(installs) == 1, "input lock must name exactly one libc6 install package")
    entry = installs[0]
    for key in ("version", "architecture", "url", "size", "sha256"):
        need(entry[key] == pkg[key], f"package {key} differs from the input lock")
    by_path = {f["resolved_path"]: f for f in source["toolchain_files"] if f["package"] == pkg["name"]}
    for role, spec in lock["files"].items():
        locked = by_path.get("/" + spec["path"])
        need(locked is not None, f"{role} not in the input lock")
        need(locked["path"] == locked["resolved_path"], f"{role} is not a regular file in the lock")
        need(locked["sha256"] == spec["sha256"] and locked["size"] == spec["size"],
             f"{role} hash or size differs from the input lock")
    need(source["libm"]["build_id"] == lock["files"]["libm"]["build_id"], "libm build ID differs")
    need(source["libm"]["path"] == "/" + lock["files"]["libm"]["path"], "libm path differs")
    return True


# ---- check-domain

def check_domain(lock, domain_path):
    size = os.path.getsize(domain_path)
    need(size == domain_count(lock) * 8, f"domain size {size}, expected {domain_count(lock) * 8}")
    need(sha256_file(domain_path) == lock["domain"]["sha256"], "domain SHA-256 differs")
    with open(domain_path, "rb") as handle:
        fixed = handle.read(lock["domain"]["fixed_count"] * 8)
    need(hashlib.sha256(fixed).hexdigest() == lock["domain"]["fixed_sha256"], "fixed inputs differ")
    return True


# ---- reference

def verify_package(lock, deb_path):
    need(os.path.getsize(deb_path) == lock["package"]["size"], "package size differs")
    need(sha256_file(deb_path) == lock["package"]["sha256"], "package SHA-256 differs")


def verify_extracted(lock, root):
    for role, spec in lock["files"].items():
        path = Path(root) / spec["path"]
        need(path.is_file() and not path.is_symlink(), f"{role} is not a regular file")
        need(path.stat().st_size == spec["size"], f"{role} size differs")
        need(sha256_file(path) == spec["sha256"], f"{role} SHA-256 differs")


def verify_records(lock, domain_path, records_path):
    count = domain_count(lock)
    need(os.path.getsize(records_path) == count * 40,
         f"records size {os.path.getsize(records_path)}, expected {count * 40}")
    with open(domain_path, "rb") as domain, open(records_path, "rb") as records:
        for index in range(count):
            record = records.read(40)
            need(record[:8] == domain.read(8), f"record {index} input differs from the domain")
    return count


def download(url, size, target):
    with urllib.request.urlopen(url, timeout=120) as response, open(target, "wb") as out:
        remaining = size + 1
        while remaining > 0:
            chunk = response.read(min(1 << 20, remaining))
            if not chunk:
                break
            out.write(chunk)
            remaining -= len(chunk)


def tool_version(args):
    try:
        return subprocess.run(args, capture_output=True, text=True, check=True).stdout.splitlines()[0]
    except (OSError, subprocess.CalledProcessError, IndexError):
        return "unavailable"


def reference(lock, domain_path, probe_source, out_dir):
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    check_domain(lock, domain_path)
    work = Path(tempfile.mkdtemp(prefix="glibc-trig-"))
    deb = work / "libc6.deb"
    download(lock["package"]["url"], lock["package"]["size"], deb)
    verify_package(lock, deb)
    root = work / "root"
    subprocess.run(["dpkg-deb", "-x", str(deb), str(root)], check=True)
    verify_extracted(lock, root)
    libdir = (root / lock["files"]["libm"]["path"]).parent.resolve()
    loader = root / lock["files"]["loader"]["path"]
    probe = work / "probe"
    subprocess.run(["gcc", "-O0", "-std=c11", "-Wall", "-Wextra", "-Werror", "-o", str(probe),
                    str(probe_source)], check=True)
    records = out_dir / "reference.bin"
    offsets = lock["dispatch"]["offsets"]
    env = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "GLIBC_TUNABLES": lock["dispatch"]["glibc_tunables"]}
    argv = [str(loader), "--inhibit-cache", "--library-path", str(libdir), str(probe),
            str(Path(domain_path).resolve()), str(records), str(libdir), str(domain_count(lock)),
            offsets["sin"], offsets["cos"], offsets["sincos"]]
    run = subprocess.run(argv, env=env, capture_output=True, text=True)
    (out_dir / "probe.log").write_text(run.stdout + run.stderr, encoding="utf-8")
    need(run.returncode == 0, f"probe exited {run.returncode}: {run.stderr.strip()}")
    for name, offset in offsets.items():
        need(f"{name} {libdir}/libm.so.6+{offset}" in run.stdout.splitlines(),
             f"probe did not confirm {name} at {offset}")
    count = verify_records(lock, domain_path, records)
    identity = {
        "lock_binding": lock["lock_binding"],
        "package": lock["package"],
        "files": {role: spec["sha256"] for role, spec in lock["files"].items()},
        "glibc_tunables": lock["dispatch"]["glibc_tunables"],
        "offsets": offsets,
        "loader_invocation": ["<loader>", "--inhibit-cache", "--library-path", "<libdir>", "probe", "..."],
        "environment": sorted(env),
        "probe_source_sha256": sha256_file(probe_source),
        "probe_binary_sha256": sha256_file(probe),
        "gcc": tool_version(["gcc", "--version"]),
        "dpkg_deb": tool_version(["dpkg-deb", "--version"]),
        "host": {"kernel": platform.release(), "machine": platform.machine(),
                 "cpu": next((l.split(":", 1)[1].strip() for l in Path("/proc/cpuinfo").read_text().splitlines()
                              if l.startswith("model name")), "unknown")},
        "domain_sha256": lock["domain"]["sha256"],
        "records": count,
        "reference_sha256": sha256_file(records),
    }
    (out_dir / "reference-identity.json").write_text(json.dumps(identity, indent=1) + "\n", encoding="utf-8")
    shutil.rmtree(work, ignore_errors=True)
    return identity


# ---- wasi-test

def wasi_test(messages):
    found = set()
    for line in messages.splitlines():
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact" or not message.get("executable"):
            continue
        target = message.get("target", {})
        if target.get("name") == "kicad_math" and "lib" in target.get("kind", []):
            found.add(message["executable"])
    need(len(found) == 1, f"expected one kicad_math library test executable, found {sorted(found)}")
    return found.pop()


# ---- report

REFERENCE_LINE = re.compile(r"glibc_trig reference: (\d+) records, (\d+) mismatches")
SET_ROTATION_LINE = re.compile(r"wasm32-unknown-unknown set_rotation: (\d+) records, (\d+) mismatches")


def summarise(lock, logs):
    """logs: {comparison: text}. Returns (ok, summary)."""
    count = domain_count(lock)
    summary = {}
    ok = True
    for name in COMPARISONS:
        text = logs.get(name)
        pattern = SET_ROTATION_LINE if name.startswith("unknown") else REFERENCE_LINE
        matches = pattern.findall(text or "")
        if len(matches) != 1:
            summary[name] = {"status": "NOT RUN" if text is None else "NO RESULT"}
            ok = False
            continue
        records, mismatches = int(matches[0][0]), int(matches[0][1])
        passed = records == count and mismatches == 0
        summary[name] = {"status": "PASS" if passed else "FAIL", "records": records, "mismatches": mismatches}
        ok = ok and passed
    return ok, summary


def report(lock, out_dir):
    out_dir = Path(out_dir)
    logs = {}
    for name in COMPARISONS:
        path = out_dir / f"{name}.log"
        if path.exists():
            logs[name] = path.read_text(encoding="utf-8", errors="replace")
    ok, summary = summarise(lock, logs)
    hashes = {p.name: sha256_file(p) for p in sorted(out_dir.iterdir())
              if p.is_file() and p.name != "report.json"}
    result = {"result": "PASS" if ok else "FAIL", "expected_records": domain_count(lock),
              "comparisons": summary, "sha256": hashes,
              "note": "Targeted glibc_trig reference comparison only; the math unit's full corpus gate is separate."}
    (out_dir / "report.json").write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({"result": result["result"], "comparisons": summary}, indent=1))
    return ok


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["bind-lock", "check-domain", "reference", "wasi-test", "report"])
    parser.add_argument("--lock", required=True)
    parser.add_argument("--source-lock")
    parser.add_argument("--domain")
    parser.add_argument("--probe-source")
    parser.add_argument("--out")
    args = parser.parse_args(argv)
    lock = load_lock(args.lock)
    try:
        if args.command == "bind-lock":
            bind_lock(lock, Path(args.source_lock).read_bytes())
            print("input lock binding verified")
        elif args.command == "check-domain":
            check_domain(lock, args.domain)
            print(f"domain verified: {domain_count(lock)} inputs")
        elif args.command == "reference":
            identity = reference(lock, args.domain, args.probe_source, args.out)
            print(json.dumps({"records": identity["records"], "reference_sha256": identity["reference_sha256"]}))
        elif args.command == "wasi-test":
            print(wasi_test(sys.stdin.read()))
        else:
            return 0 if report(lock, args.out) else 1
    except Refused as error:
        print(f"refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
