#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Refuse missing/substituted IEC60664 reference compilation and absent archive links."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from audit import audit


class CalculatorReferenceGuards(unittest.TestCase):
    def fixture(self, root, *, reference=True, archive=True, substitute=False):
        source, build, overlay = (root / name for name in ("source", "build", "overlay"))
        for path in (source, build, overlay):
            path.mkdir()
        (build / "config.h").write_text("config", encoding="utf-8")
        relative = "pcb_calculator/calculator_panels/iec60664.cpp"
        database = []
        for target in ("core", "clipper2", "kimath", "kicad-oracle") + (("iec60664-reference",) if reference else ()):
            path = source / relative if target == "iec60664-reference" and not substitute else overlay / (target + ".cpp")
            database.append({"file": str(path), "arguments": [
                "g++", "-fno-fast-math", "-ffp-contract=off", "-c", str(path),
                "-o", f"CMakeFiles/{target}.dir/impl.cpp.o"]})
        (build / "compile_commands.json").write_text(json.dumps(database), encoding="utf-8")
        manifest = {"files": {relative: "test-hash"}, "overlay_revision": "test-revision",
                    "overlay_dirty_at_export": False}
        link = "g++ -fno-fast-math -ffp-contract=off -o kicad-oracle libkimath.a"
        if archive:
            link += " libiec60664-reference.a"

        def output(args, **_kwargs):
            return link if args[0] == "ninja" else "test-toolchain"

        return build, source, overlay, manifest, output

    def run_audit(self, root, **kwargs):
        build, source, overlay, manifest, output = self.fixture(root, **kwargs)
        with patch("audit.verify_source", return_value=(manifest, "manifest-test-hash")), \
                patch("audit.subprocess.check_output", side_effect=output):
            audit(build.resolve(), source.resolve(), overlay.resolve())
        return build

    def test_missing_calculator_compilation_is_refused_even_with_archive_link(self):
        with tempfile.TemporaryDirectory() as directory, \
                self.assertRaisesRegex(ValueError, "Unexpected reference target closure"):
            self.run_audit(Path(directory), reference=False)

    def test_shim_under_reference_target_name_is_refused(self):
        with tempfile.TemporaryDirectory() as directory, \
                self.assertRaisesRegex(ValueError, "must compile the actual pinned source"):
            self.run_audit(Path(directory), substitute=True)

    def test_missing_calculator_archive_link_is_refused_even_with_compilation(self):
        with tempfile.TemporaryDirectory() as directory, \
                self.assertRaisesRegex(ValueError, "does not link the actual IEC60664 archive"):
            self.run_audit(Path(directory), archive=False)

    def test_actual_reference_target_and_link_are_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            build = self.run_audit(Path(directory))
            record = json.loads((build / "oracle-provenance.json").read_text(encoding="utf-8"))
            self.assertEqual(len(record["compile_commands"]), 5)
            self.assertIn("libiec60664-reference.a", record["build_commands"][0])


if __name__ == "__main__":
    unittest.main()
