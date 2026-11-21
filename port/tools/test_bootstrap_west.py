# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression checks for preserving work and rejecting inconsistent references."""

import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import bootstrap_west as bootstrap


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.pins = bootstrap.load_pins(Path(__file__).resolve().parents[2])

    def test_git_settings_are_process_local_and_keep_existing_overrides(self):
        existing = {"GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "test.key", "GIT_CONFIG_VALUE_0": "value"}
        with patch.dict(os.environ, existing, clear=True):
            with patch.object(bootstrap.subprocess, "run") as run:
                bootstrap.run(["git", "status"], self.root)
            environment = run.call_args.kwargs["env"]
            self.assertEqual(environment["GIT_CONFIG_COUNT"], "3")
            self.assertEqual(environment["GIT_CONFIG_KEY_0"], "test.key")
            self.assertEqual(environment["GIT_CONFIG_KEY_1"], "core.autocrlf")
            self.assertEqual(environment["GIT_CONFIG_VALUE_1"], "input")
            self.assertEqual(environment["GIT_CONFIG_VALUE_2"], "false")
            self.assertEqual(environment["GIT_OPTIONAL_LOCKS"], "0")
            self.assertEqual(os.environ["GIT_CONFIG_COUNT"], "1")

    def test_configuration_is_local_and_idempotent(self):
        bootstrap.configure(self.root, False)
        path = self.root / ".west/config"
        original = path.read_bytes()
        bootstrap.configure(self.root, False)
        bootstrap.configure(self.root, True)
        self.assertEqual(path.read_bytes(), original)
        self.assertFalse((self.root.parent / ".west/config").exists())

    def test_check_does_not_create_configuration(self):
        with self.assertRaisesRegex(ValueError, "missing .west/config"):
            bootstrap.configure(self.root, True)
        self.assertFalse((self.root / ".west").exists())

    def test_existing_manifest_is_preserved(self):
        path = self.root / ".west/config"
        path.parent.mkdir()
        original = "[manifest]\npath = another-project\n"
        path.write_text(original)
        with self.assertRaisesRegex(ValueError, "refusing to overwrite"):
            bootstrap.configure(self.root, False)
        self.assertEqual(path.read_text(), original)

    def test_short_commit_is_rejected(self):
        source = Path(__file__).resolve().parents[1] / "pins.toml"
        (self.root / "port").mkdir()
        (self.root / "port/pins.toml").write_text(
            source.read_text().replace(self.pins["kicad"]["revision"], "4cf9397")
        )
        with self.assertRaisesRegex(ValueError, "full lowercase commit SHA"):
            bootstrap.load_pins(self.root)

    def test_manifest_drift_is_rejected_before_west(self):
        project = copy.deepcopy(self.pins["solidcore"])
        project.pop("solidcraft_revision")
        project["name"] = "solidcore"
        project["revision"] = "main"
        data = {"manifest": {"version": "1.2", "projects": [project], "self": {"path": "."}}}
        with patch.object(bootstrap, "run", return_value=json.dumps(data)) as run:
            with self.assertRaisesRegex(ValueError, "disagrees"):
                bootstrap.check_manifest(self.root, "python", self.pins)
        self.assertEqual(run.call_count, 1)

    def checkout(self):
        checkout = self.root / self.pins["solidcore"]["path"]
        (checkout / ".git").mkdir(parents=True)
        return checkout

    def test_dirty_checkout_blocks_update(self):
        self.checkout()
        with patch.object(bootstrap, "run", side_effect=[self.pins["solidcore"]["url"], " M src/lib.rs"]):
            with self.assertRaisesRegex(ValueError, "dirty"):
                bootstrap.check_checkout(self.root, self.pins, before_update=True)

    def test_wrong_origin_blocks_update(self):
        self.checkout()
        with patch.object(bootstrap, "run", return_value="https://example.invalid/other.git"):
            with self.assertRaisesRegex(ValueError, "unexpected solidcore origin"):
                bootstrap.check_checkout(self.root, self.pins, before_update=True)

    def test_head_and_trailer_must_both_match(self):
        self.checkout()
        prefix = [self.pins["solidcore"]["url"], ""]
        with patch.object(bootstrap, "run", side_effect=prefix + ["0" * 40]):
            with self.assertRaisesRegex(ValueError, "HEAD differs"):
                bootstrap.check_checkout(self.root, self.pins)
        with patch.object(bootstrap, "run", side_effect=prefix + [self.pins["solidcore"]["revision"], "wrong trailer"]):
            with self.assertRaisesRegex(ValueError, "solidcraft trailer"):
                bootstrap.check_checkout(self.root, self.pins)


if __name__ == "__main__":
    unittest.main()
