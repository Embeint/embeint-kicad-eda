# SPDX-License-Identifier: GPL-3.0-or-later
"""Static tests of the glibc_trig reference harness's fail-closed checks (no network, no
compiler, no probe run). python -I -B -m unittest discover -s port/glibc-trig"""

import copy
import hashlib
import io
import json
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import glibc_trig_reference as harness  # noqa: E402

LOCK = harness.load_lock(Path(__file__).resolve().parent / "reference.lock.json")


def synthetic_source(lock):
    """An input lock carrying exactly the entries reference.lock.json copies."""
    pkg = lock["package"]
    files = [{"package": "libc6", "path": "/" + spec["path"], "resolved_path": "/" + spec["path"],
              "sha256": spec["sha256"], "size": spec["size"]} for spec in lock["files"].values()]
    files.append({"package": "libgcc-s1", "path": "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
                  "resolved_path": "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1", "sha256": "0" * 64, "size": 1})
    return {
        "libm": {"build_id": lock["files"]["libm"]["build_id"], "path": "/" + lock["files"]["libm"]["path"]},
        "packages": {"install": [dict(pkg, filename="pool/x.deb", source="glibc")]},
        "toolchain_files": files,
    }


def bound(lock, source):
    data = json.dumps(source).encode()
    lock = copy.deepcopy(lock)
    lock["lock_binding"]["sha256"] = hashlib.sha256(data).hexdigest()
    return lock, data


class BindLock(unittest.TestCase):
    def test_accepts_exact_copy(self):
        lock, data = bound(LOCK, synthetic_source(LOCK))
        self.assertTrue(harness.bind_lock(lock, data))

    def test_rejects_other_lock_bytes(self):
        # Correct content, but not the reviewed lock's bytes.
        _, data = bound(LOCK, synthetic_source(LOCK))
        with self.assertRaises(harness.Refused):
            harness.bind_lock(LOCK, data)

    def test_rejects_each_changed_field(self):
        mutations = [
            lambda s: s["packages"]["install"][0].update(sha256="1" * 64),
            lambda s: s["packages"]["install"][0].update(size=1),
            lambda s: s["packages"]["install"][0].update(url="https://example.invalid/libc6.deb"),
            lambda s: s["packages"]["install"][0].update(version="2.39-0ubuntu8.11"),
            lambda s: s["packages"]["install"].append(dict(s["packages"]["install"][0])),
            lambda s: s["toolchain_files"][0].update(sha256="2" * 64),
            lambda s: s["toolchain_files"][1].update(size=7),
            lambda s: s["toolchain_files"][2].update(resolved_path="/usr/lib/other/libm.so.6"),
            lambda s: s["libm"].update(build_id="0" * 40),
        ]
        for mutate in mutations:
            source = synthetic_source(LOCK)
            mutate(source)
            lock, data = bound(LOCK, source)
            with self.assertRaises(harness.Refused):
                harness.bind_lock(lock, data)


class Domain(unittest.TestCase):
    def setUp(self):
        self.lock = copy.deepcopy(LOCK)
        self.lock["domain"].update(fixed_count=2, seeded_count=3)
        self.values = [1, 2, 3, 4, 5]
        self.data = b"".join(struct.pack("<Q", v) for v in self.values)
        self.lock["domain"]["sha256"] = hashlib.sha256(self.data).hexdigest()
        self.lock["domain"]["fixed_sha256"] = hashlib.sha256(self.data[:16]).hexdigest()
        self.dir = tempfile.TemporaryDirectory()
        self.path = Path(self.dir.name) / "domain.bin"
        self.path.write_bytes(self.data)

    def tearDown(self):
        self.dir.cleanup()

    def test_domain(self):
        self.assertTrue(harness.check_domain(self.lock, self.path))
        self.path.write_bytes(self.data[:-8])
        with self.assertRaises(harness.Refused):
            harness.check_domain(self.lock, self.path)
        self.path.write_bytes(self.data[:-1] + b"\x01")
        with self.assertRaises(harness.Refused):
            harness.check_domain(self.lock, self.path)

    def test_records_order_and_count(self):
        records = Path(self.dir.name) / "reference.bin"
        good = b"".join(struct.pack("<5Q", v, 0, 0, 0, 0) for v in self.values)
        records.write_bytes(good)
        self.assertEqual(harness.verify_records(self.lock, self.path, records), 5)
        swapped = good[40:80] + good[:40] + good[80:]
        for bad in (good[:-40], good + good[:40], swapped):
            records.write_bytes(bad)
            with self.assertRaises(harness.Refused):
                harness.verify_records(self.lock, self.path, records)

    def test_package_and_extracted_files(self):
        lock = copy.deepcopy(self.lock)
        deb = Path(self.dir.name) / "libc6.deb"
        deb.write_bytes(b"package")
        lock["package"].update(size=7, sha256=hashlib.sha256(b"package").hexdigest())
        harness.verify_package(lock, deb)
        deb.write_bytes(b"packagf")
        with self.assertRaises(harness.Refused):
            harness.verify_package(lock, deb)
        root = Path(self.dir.name) / "root"
        for role, spec in lock["files"].items():
            path = root / spec["path"]
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(role.encode())
            spec.update(size=len(role), sha256=hashlib.sha256(role.encode()).hexdigest())
        harness.verify_extracted(lock, root)
        (root / lock["files"]["libc"]["path"]).write_bytes(b"lib")
        with self.assertRaises(harness.Refused):
            harness.verify_extracted(lock, root)


class Parsing(unittest.TestCase):
    def test_wasi_test_executable(self):
        lib = {"reason": "compiler-artifact", "executable": "/t/kicad_math-1.wasm",
               "target": {"name": "kicad_math", "kind": ["lib"]}}
        other = {"reason": "compiler-artifact", "executable": "/t/math-2.wasm",
                 "target": {"name": "math", "kind": ["test"]}}
        text = "\n".join(json.dumps(m) for m in (other, lib, {"reason": "build-finished"}))
        self.assertEqual(harness.wasi_test(text), "/t/kicad_math-1.wasm")
        with self.assertRaises(harness.Refused):
            harness.wasi_test(json.dumps(other))

    def test_summary_requires_every_full_comparison(self):
        count = harness.domain_count(LOCK)
        good = {name: (f"wasm32-unknown-unknown set_rotation: {count} records, 0 mismatches"
                       if name.startswith("unknown") else f"glibc_trig reference: {count} records, 0 mismatches")
                for name in harness.COMPARISONS}
        ok, summary = harness.summarise(LOCK, good)
        self.assertTrue(ok)
        self.assertTrue(all(s["status"] == "PASS" for s in summary.values()))
        for broken in (
            {**good, "wasip1-release": None},
            {**good, "native-debug": good["native-debug"].replace(f"{count} records", "5 records")},
            {**good, "unknown-set-rotation-debug": good["unknown-set-rotation-debug"].replace(" 0 mismatches", " 2 mismatches")},
            {**good, "native-release": good["native-release"] + "\n" + good["native-release"]},
        ):
            logs = {k: v for k, v in broken.items() if v is not None}
            self.assertFalse(harness.summarise(LOCK, logs)[0])

    def test_lock_internal_consistency(self):
        self.assertEqual(harness.domain_count(LOCK), 1_001_602)
        self.assertEqual(LOCK["dispatch"]["glibc_tunables"], "glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F")
        self.assertEqual(set(LOCK["files"]), {"loader", "libc", "libm"})
        self.assertTrue(LOCK["package"]["url"].startswith("https://snapshot.ubuntu.com/"))


class Download(unittest.TestCase):
    def test_retries_transient_service_error_then_verifies_exact_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "package.deb"
            target.write_bytes(b"a stale longer download")
            with patch.object(harness.urllib.request, "urlopen", side_effect=[
                HTTPError("url", 503, "unavailable", {}, None), io.BytesIO(b"package")
            ]) as request, patch.object(harness.time, "sleep") as sleep:
                harness.download("url", 7, target)
            self.assertEqual(request.call_count, 2)
            sleep.assert_called_once_with(2)
            self.assertEqual(target.read_bytes(), b"package")
            lock = copy.deepcopy(LOCK)
            lock["package"].update(size=7, sha256=hashlib.sha256(b"package").hexdigest())
            harness.verify_package(lock, target)

    def test_retry_limit_and_permanent_error(self):
        for code, calls in [(503, 3), (404, 1)]:
            with self.subTest(code=code), tempfile.TemporaryDirectory() as directory:
                with patch.object(harness.urllib.request, "urlopen", side_effect=
                                  HTTPError("url", code, "failed", {}, None)) as request, \
                        patch.object(harness.time, "sleep"):
                    with self.assertRaises(HTTPError):
                        harness.download("url", 7, Path(directory) / "package.deb")
                self.assertEqual(request.call_count, calls)


if __name__ == "__main__":
    unittest.main()
