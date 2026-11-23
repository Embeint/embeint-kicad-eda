# SPDX-License-Identifier: GPL-3.0-or-later
"""Signature-STUBBED coverage tests, NOT cryptographic verification evidence.

Real Ubuntu/Rust signature verification is a separate native arm64 check.
"""
import copy
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ENV = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENV / "tools"))
import input_plan


class InstallInputsTest(unittest.TestCase):
    def fixture(self):
        root = Path(tempfile.mkdtemp(prefix="refenv-coverage-"))
        self.addCleanup(shutil.rmtree, root)
        inputs = root / "inputs"
        (inputs / "rust").mkdir(parents=True)
        (inputs / "debs").mkdir()
        key = root / "archive-key"
        key.write_bytes(b"fixture-key")
        payloads = {
            "rust/channel.toml": b'xz_hash = "' + b"0" * 64 + b'"\n',
            "rust/channel.toml.asc": b"fixture-signature",
            "rust/rust-key.gpg": b"fixture-key",
            "rust/rustc-1.99.1-target.tar.xz": b"fixture-archive",
            "debs/a.deb": b"fixture-deb",
        }
        sha = lambda data: hashlib.sha256(data).hexdigest()
        archive_sha = sha(payloads["rust/rustc-1.99.1-target.tar.xz"])
        payloads["rust/channel.toml"] = f'xz_hash = "{archive_sha}"\n'.encode()
        index = (
            f"Package: a\nVersion: 1\nArchitecture: amd64\nFilename: pool/a.deb\n"
            f'SHA256: {sha(payloads["debs/a.deb"])}\n\n'
        ).encode()
        payloads["Packages-noble-main.gz"] = gzip.compress(index)
        payloads["InRelease-noble"] = (
            f'SHA256:\n {sha(payloads["Packages-noble-main.gz"])} 1 '
            "main/binary-amd64/Packages.gz\n"
        ).encode()
        lock = {
            "snapshot": {
                "architecture": "amd64",
                "keyring": str(key),
                "keyring_sha256": sha(key.read_bytes()),
                "suites": {
                    "noble": {
                        "inrelease_sha256": sha(payloads["InRelease-noble"]),
                        "indices": {"main": {"sha256": sha(payloads["Packages-noble-main.gz"])}},
                    }
                },
            },
            "packages": {
                "install": [
                    {
                        "filename": "pool/a.deb",
                        "sha256": sha(payloads["debs/a.deb"]),
                        "name": "a",
                        "version": "1",
                        "architecture": "amd64",
                    }
                ]
            },
            "rust": {
                "channel_manifest_sha256": sha(payloads["rust/channel.toml"]),
                "channel_signature_sha256": sha(payloads["rust/channel.toml.asc"]),
                "signing_keyring_sha256": sha(payloads["rust/rust-key.gpg"]),
                "components": [
                    {"url": "https://fixture/rustc-1.99.1-target.tar.xz", "sha256": archive_sha}
                ],
            },
        }
        controls, inventory = input_plan.plan(lock)
        for name, data in {**payloads, **controls, "inputs.sha256": inventory}.items():
            (inputs / name).write_bytes(data)
        lock["verification_plan_sha256"] = sha(inventory)
        lock_path = root / "lock.json"
        lock_path.write_text(json.dumps(lock, indent=2, sort_keys=True) + "\n")
        bin_dir = root / "bin"
        bin_dir.mkdir()
        (bin_dir / "gpgv").write_text(
            '#!/bin/sh\nwhile test "$#" -gt 0; do\n'
            'if test "$1" = --output; then shift; out=$1; fi\nlast=$1; shift; done\n'
            'if test -n "${out:-}"; then cp "$last" "$out"; fi\n'
        )
        (bin_dir / "gpgv").chmod(0o755)
        return (
            inputs,
            lock_path,
            sha(lock_path.read_bytes()),
            {**os.environ, "PATH": str(bin_dir) + ":" + os.environ["PATH"]},
            lock,
        )

    def run_chain(self, fixture):
        inputs, lock, sha, env, _ = fixture
        return subprocess.run(
            ["sh", str(ENV / "image/verify_chain.sh"), str(inputs), str(lock), sha],
            env=env,
            capture_output=True,
            text=True,
        )

    def test_exact_inputs_coverage(self):
        result = self.run_chain(self.fixture())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_adversarial_inputs_fail_before_installation(self):
        for change in (
            "extra",
            "missing",
            "modified",
            "manifest",
            "signature",
            "key",
            "checksums",
            "controls",
            "plan",
            "duplicate-plan",
            "wrong-lock",
            "symlink",
            "archive-and-manifest",
        ):
            with self.subTest(change=change):
                fixture = self.fixture()
                inputs, lock, _sha, _env, _data = fixture
                archive = inputs / "rust/rustc-1.99.1-target.tar.xz"
                if change == "extra":
                    (inputs / "rust/unlisted.tar.xz").write_bytes(b"unverified installer")
                elif change == "missing":
                    archive.unlink()
                elif change == "modified":
                    archive.write_bytes(b"changed")
                elif change == "manifest":
                    (inputs / "rust/channel.toml").write_bytes(b"changed")
                elif change == "signature":
                    (inputs / "rust/channel.toml.asc").write_bytes(b"changed")
                elif change == "key":
                    (inputs / "rust/rust-key.gpg").write_bytes(b"changed")
                elif change == "checksums":
                    (inputs / "rust.sha256").write_text("")
                elif change == "controls":
                    (inputs / "snapshot.env").write_text("exit 0\n")
                elif change == "plan":
                    (inputs / "inputs.sha256").write_text("")
                elif change == "duplicate-plan":
                    p = inputs / "inputs.sha256"
                    p.write_bytes(p.read_bytes() * 2)
                elif change == "wrong-lock":
                    lock.write_text("{}")
                elif change == "symlink":
                    archive.unlink()
                    archive.symlink_to(inputs / "rust/channel.toml")
                else:
                    archive.write_bytes(b"changed")
                    (inputs / "rust/channel.toml").write_bytes(b"changed too")
                self.assertNotEqual(self.run_chain(fixture).returncode, 0)

    def test_duplicate_component_cannot_generate_trusted_plan(self):
        lock = self.fixture()[-1]
        lock["rust"]["components"].append(copy.deepcopy(lock["rust"]["components"][0]))
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            input_plan.plan(lock)


if __name__ == "__main__":
    unittest.main()
