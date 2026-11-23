# SPDX-License-Identifier: GPL-3.0-or-later
"""Isolated Git/export/process fixtures. No Docker/oracle build is performed."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import venv

ENV = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENV / "tools"))
import source_binding as binding


class SourceBindingTest(unittest.TestCase):
    def fixture(self):
        root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, root)
        repo = root / "repo"
        repo.mkdir()

        def git(*args):
            return (
                subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.PIPE)
                .decode()
                .strip()
            )

        git("init")
        git("config", "user.name", "Fixture")
        git("config", "user.email", "fixture@example.invalid")
        (repo / "reference.cpp").write_text("public reference bytes\n")
        git("add", "reference.cpp")
        git("commit", "-m", "reference")
        pin = git("rev-parse", "HEAD")
        (repo / "port/oracle").mkdir(parents=True)
        (repo / "port/oracle/pin.py").write_text(
            f'KICAD_PIN = "{pin}"\nSOURCE_PATHS = ("reference.cpp",)\n'
        )
        (repo / "port/pins.toml").write_text(f'[kicad]\nrevision="{pin}"\n')
        (repo / "port/oracle/overlay.cpp").write_text("public overlay bytes\n")
        git("add", ".")
        git("commit", "-m", "overlay")
        head = git("rev-parse", "HEAD")
        return root, repo, head, pin

    def export(self, root, repo, head):
        bundle = root / "bundle"
        digest = binding.export(repo, head, bundle)
        return bundle, digest

    def test_valid_export_materialization_preserves_original_stage(self):
        root, repo, head, pin = self.fixture()
        bundle, digest = self.export(root, repo, head)
        original = (bundle / "stage/source-manifest.json").read_bytes()
        # Simulate Windows-produced delivery: verification/materialization must not call Git.
        with mock.patch.object(
            binding, "git", side_effect=AssertionError("Git must not run on consumer")
        ):
            binding.materialize(bundle, head, digest, root / "snapshot")
            binding.check_snapshot(root / "snapshot", head, digest)
        self.assertEqual((root / "snapshot/stage/source-manifest.json").read_bytes(), original)
        marker = root / "build-marker"
        command = [
            sys.executable,
            str(ENV / "tools/source_binding.py"),
            "check",
            "--bundle",
            str(root / "snapshot"),
            "--revision",
            head,
            "--source-sha256",
            digest,
        ]
        script = "import subprocess,sys,pathlib; subprocess.run(sys.argv[2:],check=True); pathlib.Path(sys.argv[1]).touch()"
        result = subprocess.run(
            [sys.executable, "-c", script, str(marker), *command], capture_output=True
        )
        self.assertEqual(result.returncode, 0)
        self.assertTrue(marker.exists())
        marker.unlink()
        (root / "snapshot/source/port/oracle/overlay.cpp").write_text("later drift")
        result = subprocess.run(
            [sys.executable, "-c", script, str(marker), *command], capture_output=True
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(marker.exists())

    def test_dirty_and_wrong_checkout_stop_before_export_or_build(self):
        for change in ("dirty", "untracked", "wrong-head"):
            with self.subTest(change=change):
                root, repo, head, pin = self.fixture()
                if change == "dirty":
                    (repo / "port/oracle/overlay.cpp").write_text("dirty")
                elif change == "untracked":
                    (repo / "untracked.cpp").write_text("unexpected")
                expected = pin if change == "wrong-head" else head
                with self.assertRaises(ValueError):
                    binding.export(repo, expected, root / "bundle")
                self.assertFalse((root / "bundle/source.json").exists())

    def test_stage_wrong_pin_stale_dirty_and_actual_drift_are_rejected(self):
        for change in ("stale", "dirty", "pin", "actual-source"):
            with self.subTest(change=change):
                root, repo, head, pin = self.fixture()
                bundle, digest = self.export(root, repo, head)
                stage = bundle / "stage"
                m = json.loads((stage / "source-manifest.json").read_text())
                if change == "stale":
                    m["overlay_revision"] = "0" * 40
                elif change == "dirty":
                    m["overlay_dirty_at_export"] = True
                elif change == "pin":
                    m["kicad_pin"] = "0" * 40
                else:
                    # Matching self-reported archive+manifest are insufficient: compare pinned Git bytes.
                    (repo / "reference.cpp").write_text("mutated source")
                    subprocess.run(
                        [
                            "tar",
                            "-cf",
                            str(stage / "reference.tar"),
                            "-C",
                            str(repo),
                            "reference.cpp",
                        ],
                        check=True,
                    )
                    m["archive_sha256"] = hashlib.sha256(
                        (stage / "reference.tar").read_bytes()
                    ).hexdigest()
                    m["files"]["reference.cpp"] = hashlib.sha256(
                        (repo / "reference.cpp").read_bytes()
                    ).hexdigest()
                    subprocess.run(["git", "-C", str(repo), "restore", "reference.cpp"], check=True)
                (stage / "source-manifest.json").write_text(json.dumps(m))
                with self.assertRaises(ValueError):
                    binding.export(repo, head, root / "retry", stage)

    def test_bundle_wrong_digest_revision_and_post_export_drift(self):
        for change in ("digest", "revision", "archive", "manifest", "extra-code"):
            root, repo, head, pin = self.fixture()
            bundle, digest = self.export(root, repo, head)
            if change == "digest":
                digest = "0" * 64
            elif change == "revision":
                head = pin
            elif change == "archive":
                (bundle / "source.tar.gz").write_bytes(b"mutated")
            elif change == "manifest":
                (bundle / "stage/source-manifest.json").write_text("{}")
            else:
                binding.materialize(bundle, head, digest, root / "snapshot")
                (root / "snapshot/source/extra.py").write_text('print("untracked")')
                with self.assertRaises(ValueError):
                    binding.check_snapshot(root / "snapshot", head, digest)
                continue
            with self.assertRaises(ValueError):
                binding.materialize(bundle, head, digest, root / "snapshot")

    def test_snapshot_directory_links_stop_the_real_check_entrypoint(self):
        for change in ("tracked-inside", "tracked-outside", "untracked", "source-root"):
            with self.subTest(change=change):
                root, repo, head, pin = self.fixture()
                bundle, digest = self.export(root, repo, head)
                snapshot = root / "snapshot"
                binding.materialize(bundle, head, digest, snapshot)
                source = snapshot / "source"
                if change == "source-root":
                    source.rename(snapshot / "real-source")
                    source.symlink_to("real-source", target_is_directory=True)
                elif change == "untracked":
                    target = snapshot / "external-oracle"
                    target.mkdir()
                    (target / "extra.cpp").write_text("unrecorded compiler input")
                    (source / "extra").symlink_to("../external-oracle", target_is_directory=True)
                else:
                    target = (snapshot if change == "tracked-inside" else root) / "external-oracle"
                    overlay = source / "port/oracle"
                    shutil.move(overlay, target)
                    (target / "extra.cpp").write_text("unrecorded compiler input")
                    overlay.symlink_to(os.path.relpath(target, overlay.parent), target_is_directory=True)
                result = subprocess.run(
                    [
                        sys.executable, str(ENV / "tools/source_binding.py"), "check",
                        "--bundle", str(snapshot), "--revision", head,
                        "--source-sha256", digest,
                    ],
                    capture_output=True, text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, "symlink|real directory")

    def test_recorded_relative_directory_link_is_preserved_without_traversal(self):
        root, repo, head, pin = self.fixture()
        (repo / "port/alias").symlink_to("oracle", target_is_directory=True)
        subprocess.run(["git", "-C", str(repo), "add", "port/alias"], check=True)
        subprocess.run(
            ["git", "-C", str(repo), "commit", "-m", "recorded directory alias"],
            check=True, stdout=subprocess.DEVNULL,
        )
        head = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"]).decode().strip()
        bundle, digest = self.export(root, repo, head)
        binding.materialize(bundle, head, digest, root / "snapshot")
        binding.check_snapshot(root / "snapshot", head, digest)
        # The target's actual directory is still enumerated and cannot hide additions.
        (root / "snapshot/source/port/oracle/extra.cpp").write_text("extra compiler input")
        with self.assertRaisesRegex(ValueError, "unexpected source snapshot file"):
            binding.check_snapshot(root / "snapshot", head, digest)

    def linux_venv_snapshot(self):
        root, repo, head, pin = self.fixture()
        bundle, digest = self.export(root, repo, head)
        snapshot = root / "snapshot"
        binding.materialize(bundle, head, digest, snapshot)
        command = [
            sys.executable, str(ENV / "tools/source_binding.py"), "check",
            "--bundle", str(snapshot), "--revision", head, "--source-sha256", digest,
        ]
        self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
        environment = snapshot / "source/.port-venv/linux"
        # Exercise CPython's actual Linux directory-creation branch on macOS
        # too. This does not claim native Linux/package/bootstrap execution.
        # Change only venv's branch selector, leaving sysconfig's native host
        # configuration intact (globally replacing sys.platform corrupts it).
        linux_sys = SimpleNamespace(**{**vars(venv.sys), "platform": "linux"})
        with mock.patch.object(venv, "sys", linux_sys):
            venv.EnvBuilder().ensure_directories(environment)
        self.assertEqual(os.readlink(environment / "lib64"), "lib")
        return root, snapshot, environment, command

    def test_linux_venv_directory_creation_keeps_source_guard_valid(self):
        root, snapshot, environment, command = self.linux_venv_snapshot()
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_linux_venv_alias_exception_cannot_hide_directory_links(self):
        for change in ("escape", "other-confined", "lib-target-escape", "missing-lib", "extra-link"):
            with self.subTest(change=change):
                root, snapshot, environment, command = self.linux_venv_snapshot()
                alias = environment / "lib64"
                if change == "lib-target-escape":
                    outside = root / "external-lib"
                    shutil.move(environment / "lib", outside)
                    (environment / "lib").symlink_to(outside, target_is_directory=True)
                elif change == "missing-lib":
                    shutil.rmtree(environment / "lib")
                elif change == "extra-link":
                    (environment / "extra").symlink_to("lib", target_is_directory=True)
                else:
                    alias.unlink()
                    if change == "escape":
                        outside = root / "external-lib"
                        outside.mkdir()
                        alias.symlink_to(outside, target_is_directory=True)
                    else:
                        alias.symlink_to("bin", target_is_directory=True)
                result = subprocess.run(command, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, "symlink|alias")

    def test_only_known_unavailable_qa_absolute_alias_is_preserved(self):
        root, repo, head, pin = self.fixture()
        (repo / "qa/tests").mkdir(parents=True)
        (repo / "qa/tests/resources").symlink_to("/work/qa/resources")
        subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(repo), "commit", "-m", "QA alias"],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        head = (
            subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"]).decode().strip()
        )
        bundle, digest = self.export(root, repo, head)
        binding.materialize(bundle, head, digest, root / "snapshot")
        binding.check_snapshot(root / "snapshot", head, digest)
        # Git for Windows' symlink text-stub representation is accepted only
        # in a clean checkout; Linux snapshots still require real link identity.
        subprocess.run(["git", "-C", str(repo), "config", "core.symlinks", "false"], check=True)
        (repo / "qa/tests/resources").unlink()
        (repo / "qa/tests/resources").write_text("/work/qa/resources")
        binding.export(repo, head, root / "windows-stub-bundle")
        (repo / "unsafe-link").symlink_to("/etc/passwd")
        subprocess.run(["git", "-C", str(repo), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(repo), "commit", "-m", "invalid alias"],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        head = (
            subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"]).decode().strip()
        )
        with self.assertRaisesRegex(ValueError, "absolute source link"):
            binding.export(repo, head, root / "invalid-bundle")

    def test_stage_export_does_not_rewrite_supplied_provenance(self):
        root, repo, head, pin = self.fixture()
        bundle, digest = self.export(root, repo, head)
        original = (bundle / "stage/source-manifest.json").read_bytes()
        binding.export(repo, head, root / "second", bundle / "stage")
        self.assertEqual((root / "second/stage/source-manifest.json").read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
