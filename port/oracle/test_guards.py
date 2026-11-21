#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression checks for unsafe flags and source corruption, with no compiler needed."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from audit import audit, check_flags, verify_source, check_resistor_reference
from pin import KICAD_PIN, SOURCE_PATHS


class Guards(unittest.TestCase):
    def test_resistor_requires_both_reference_tus_and_archive_link(self):
        with tempfile.TemporaryDirectory() as directory:
            overlay = Path(directory)
            source = overlay / "reference-source"
            wrapper = overlay / "reference/resistors_access.cpp"
            wrapper.parent.mkdir()
            wrapper.write_text("#include <resistor_substitution_utils.cpp>\n", encoding="utf-8")
            commands = [{"file": str(path), "arguments": [
                "g++", "-o", "CMakeFiles/resistor-reference.dir/object.o"]}
                for path in (source / "pcb_calculator/eseries.cpp", wrapper)]
            link = ["g++ -o kicad-oracle libresistor-reference.a libfmt.a libkimath.a"]
            check_resistor_reference(commands, link, source, overlay)
            for missing in range(2):
                with self.subTest(missing=missing), self.assertRaisesRegex(ValueError, "compilation"):
                    check_resistor_reference(commands[:missing] + commands[missing + 1:], link, source, overlay)
            for archive in ("libresistor-reference.a", "libfmt.a"):
                with self.subTest(archive=archive), self.assertRaisesRegex(ValueError, "archives"):
                    check_resistor_reference(commands, [link[0].replace(archive, "")], source, overlay)
            wrapper.write_text("// no reference implementation\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "actual pinned"):
                check_resistor_reference(commands, link, source, overlay)

    @unittest.skipUnless(sys.platform == "linux", "WSL/Linux shell guards")
    def test_build_lock_and_job_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            home = root / "home"
            stage = root / "stage"
            overlay = root / "overlay"
            for path in (home, stage, overlay):
                path.mkdir()
            script = Path(__file__).resolve().with_name("build.sh")
            env = dict(os.environ, HOME=str(home), ORACLE_JOBS="9")
            command = ["bash", str(script), str(stage), str(overlay)]
            result = subprocess.run(command, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            env["ORACLE_JOBS"] = "1"
            lock = home / "kicad-port.build-lock"
            lock.mkdir()
            result = subprocess.run(command, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 3, result.stderr)
            self.assertTrue(lock.is_dir())
            result = subprocess.run(command + [str(home / "kicad-live/child")],
                                    env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertTrue(lock.is_dir())

    def test_conflicting_flags_even_when_overridden(self):
        for flag in ("-Ofast", "-ffast-math", "-ffp-contract=fast", "-ffp-contract=on",
                     "-ffinite-math-only", "-funsafe-math-optimizations", "-flto=thin",
                     "-fassociative-math", "-fno-signed-zeros", "-freciprocal-math",
                     "@hidden.rsp", "-Xclang", "-mllvm", "-ffp-model=fast"):
            with self.subTest(flag=flag), self.assertRaises(ValueError):
                check_flags([flag, "-fno-fast-math", "-ffp-contract=off"])
        check_flags(["-O3", "-DNDEBUG", "-fno-fast-math", "-ffp-contract=off"])

    def test_full_audit_rejects_conflicting_compile_command_before_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            build = root / "build"
            overlay = root / "overlay"
            for path in (source, build, overlay):
                path.mkdir()
            for relative in SOURCE_PATHS:
                path = source / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                if not path.suffix:
                    path.mkdir()
            (source / "source-manifest.json").write_text(json.dumps(
                {"kicad_pin": KICAD_PIN, "files": {}}), encoding="utf-8")
            (build / "compile_commands.json").write_text(json.dumps([{
                "file": str(source / "libs/kimath/bad.cpp"),
                "arguments": ["g++", "-Ofast", "-fno-fast-math", "-ffp-contract=off"],
            }]), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Conflicting.*-Ofast"):
                audit(build, source, overlay)

    def test_source_mismatch_and_extra_files(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            files = {}
            for relative in SOURCE_PATHS:
                path = source / relative
                if path.suffix:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"reference")
                    files[relative] = hashlib.sha256(b"reference").hexdigest()
                else:
                    path.mkdir(parents=True)
            (source / "source-manifest.json").write_text(json.dumps(
                {"kicad_pin": KICAD_PIN, "files": files}), encoding="utf-8")
            verify_source(source)
            changed = source / "cmake/config.h.cmake"
            changed.write_bytes(b"modified")
            with self.assertRaises(ValueError):
                verify_source(source)
            changed.write_bytes(b"reference")
            (source / "libs/kimath/stale.h").write_bytes(b"stale")
            with self.assertRaises(ValueError):
                verify_source(source)


if __name__ == "__main__":
    unittest.main()
