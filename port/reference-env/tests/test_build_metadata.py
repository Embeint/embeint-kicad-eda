# SPDX-License-Identifier: GPL-3.0-or-later
"""Filesystem cleanup and host-record tests; no Docker or package installation."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ENV = Path(__file__).resolve().parents[1]


class BuildMetadataTest(unittest.TestCase):
    def test_cleanup_keeps_apt_directories(self):
        recipe = (ENV / "Dockerfile").read_text(encoding="utf-8")
        match = re.search(r'(    rm -rf "\$rust".*?)\n\nCOPY', recipe, re.S)
        self.assertIsNotNone(match)
        # Exercise the actual shell cleanup against an isolated filesystem.
        # This proves its lifecycle behavior, not dpkg or native image identity.
        for partial_exists in (False, True):
            with self.subTest(partial_exists=partial_exists), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for name in ("var/lib/apt/lists", "var/cache/apt", "var/log/apt", "rust"):
                    (root / name).mkdir(parents=True)
                if partial_exists:
                    (root / "var/lib/apt/lists/partial").mkdir()
                for name in ("var/lib/apt/lists/index", "var/cache/apt/cache.bin",
                             "var/log/apt/history.log", "rust/installer"):
                    (root / name).write_text("disposable", encoding="utf-8")
                untouched = root / "var/lib/apt/extended_states"
                untouched.write_text("retained", encoding="utf-8")
                script = match.group(1).replace("/var/", str(root) + "/var/")
                result = subprocess.run(
                    ["sh", "-c", script], env={**os.environ, "rust": str(root / "rust")},
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue((root / "var/lib/apt/lists/partial").is_dir())
                self.assertTrue((root / "var/log/apt").is_dir())
                self.assertEqual(list((root / "var/log/apt").iterdir()), [])
                self.assertFalse((root / "var/lib/apt/lists/index").exists())
                self.assertFalse((root / "var/cache/apt/cache.bin").exists())
                self.assertFalse((root / "rust").exists())
                self.assertEqual(untouched.read_text(encoding="utf-8"), "retained")

    def provenance(self, root, proof):
        source = root / "source.json"
        source.write_text(json.dumps(proof), encoding="utf-8")
        info = root / "info.json"
        info.write_text('{"Architecture":"x86_64","IgnoredPrivateField":"omit"}', encoding="utf-8")
        version = root / "version.json"
        version.write_text("{}", encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(ENV / "tools/host_provenance.py"),
             "--run-id", "fixture", "--image", "sha256:fixture", "--tag", "candidate",
             "--lock-sha256", "0" * 64, "--repo", str(root / "not-a-git-checkout"),
             "--docker-version", str(version), "--docker-info", str(info),
             "--source-manifest", str(source)],
            capture_output=True, text=True,
        )

    def test_provenance_records_the_producer_platform(self):
        for producer in ("win32", "linux", "darwin"):
            with self.subTest(producer=producer), tempfile.TemporaryDirectory() as temporary:
                proof = {"revision": "1" * 40, "source_clean": True, "export_platform": producer}
                result = self.provenance(Path(temporary), proof)
                self.assertEqual(result.returncode, 0, result.stderr)
                record = json.loads(result.stdout)
                self.assertEqual(record["source_export_platform"], producer)
                self.assertEqual(record["source_export"], proof)
                self.assertEqual(record["checkout_status"], "clean at recorded source export")
                self.assertEqual(record["qualification"], "not asserted")
                self.assertNotIn("IgnoredPrivateField", record["docker_info"])

    def test_provenance_requires_recorded_clean_export(self):
        mutations = [
            {"source_clean": value, "export_platform": "win32"}
            for value in (False, None, 0, 1, "true")
        ] + [{"export_platform": "win32"}] + [
            {"source_clean": True, "export_platform": value} for value in (None, "", 1)
        ] + [{"source_clean": True}]
        for mutation in mutations:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                result = self.provenance(Path(temporary), {"revision": "1" * 40, **mutation})
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(result.stdout, "")
