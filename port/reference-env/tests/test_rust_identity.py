# SPDX-License-Identifier: GPL-3.0-or-later
"""Filesystem closure tests with version-command stubs, not a Rust build."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock

ENV = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENV / "image"))
import verify_image


class RustIdentityTest(unittest.TestCase):
    def fixture(self):
        prefix = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, prefix)
        lock = json.loads((ENV / "inputs.lock.json").read_text())
        components = [
            "cargo",
            "rustc",
            "rust-std-x86_64-unknown-linux-gnu",
            "rust-std-wasm32-unknown-unknown",
        ]
        files = {
            "bin/rustc": b"compiler",
            "bin/cargo": b"cargo",
            "lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld": b"linker",
            "lib/rustlib/x86_64-unknown-linux-gnu/lib/libstd-native.rlib": b"native stdlib",
            "lib/rustlib/wasm32-unknown-unknown/lib/libstd-wasm.rlib": b"wasm stdlib",
            "lib/rustlib/rust-installer-version": b"3\n",
            "lib/rustlib/components": ("\n".join(components) + "\n").encode(),
        }
        for name, data in files.items():
            p = prefix / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
        for name in components:
            (prefix / "lib/rustlib" / ("manifest-" + name)).write_text(
                "file:" + str(prefix / "bin/rustc") + "\n"
            )
        return prefix, lock

    def check(self, prefix, lock, expected=None):
        with mock.patch.object(
            verify_image,
            "run",
            side_effect=lambda *args: (
                "rustc 1.99.1\nrelease: 1.99.1\nhost: x86_64-unknown-linux-gnu\n"
                if args[0] == "rustc"
                else "cargo 1.99.1\n"
            ),
        ):
            failures = []
            record = verify_image.check_rust(lock, failures, prefix=prefix, expected=expected)
            return record, failures

    def test_complete_stable_closure(self):
        prefix, lock = self.fixture()
        record, failures = self.check(prefix, lock)
        self.assertEqual(failures, [])
        self.assertEqual(list(record["inputs"]), sorted(record["inputs"]))
        self.assertTrue(all(p.startswith("/usr/local/") for p in record["inputs"]))
        self.assertEqual(self.check(prefix, lock, record)[1], [])
        self.assertTrue(any("/wasm32-" in p for p in record["inputs"]))

    def test_native_wasm_linker_manifests_removals_additions_and_symlinks(self):
        paths = [
            "lib/rustlib/x86_64-unknown-linux-gnu/lib/libstd-native.rlib",
            "lib/rustlib/wasm32-unknown-unknown/lib/libstd-wasm.rlib",
            "lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld",
            "lib/rustlib/manifest-rustc",
        ]
        for name in paths:
            for operation in ("mutate", "remove", "add", "symlink"):
                with self.subTest(name=name, operation=operation):
                    prefix, lock = self.fixture()
                    expected, failures = self.check(prefix, lock)
                    self.assertEqual(failures, [])
                    path = prefix / name
                    if operation == "mutate":
                        path.write_bytes(b"mutated")
                    elif operation == "remove":
                        path.unlink()
                    elif operation == "add":
                        path.with_name("extra-input").write_bytes(b"extra")
                    else:
                        path.unlink()
                        path.symlink_to("/etc/passwd")
                    self.assertTrue(self.check(prefix, lock, expected)[1])


if __name__ == "__main__":
    unittest.main()
