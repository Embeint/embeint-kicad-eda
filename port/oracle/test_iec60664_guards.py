#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Refuse missing/substituted IEC60664 reference compilation and absent archive links."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from audit import audit, FONT_REFERENCE_SOURCES


# The integrated oracle also compiles the resistor, reader, kiplatform Unix, sexpr and font
# reference closures. Every fixture supplies them completely, so each refusal below is
# caused only by the IEC60664 compilation or archive under test.
OTHER_TARGETS = ("core", "clipper2", "kimath", "fmt", "kicad-oracle")
RESISTOR_SOURCES = ("pcb_calculator/eseries.cpp",)
READER_SOURCES = ("pcb_calculator/datafile_read_write.cpp", "common/dsnlexer.cpp",
                  "common/richio.cpp", "common/exceptions.cpp")
READER_GENERATED = "reader-generated/pcb_calculator_datafile_keywords.cpp"
KIPLATFORM_SOURCES = ("libs/kiplatform/os/unix/drivers.cpp", "libs/kiplatform/os/unix/sysinfo.cpp")
SEXPR_SOURCES = ("libs/sexpr/sexpr.cpp",)
OTHER_ARCHIVES = ("libresistor-reference.a", "libfmt.a", "libcalculator-reader-reference.a",
                  "libkiplatform-unix-reference.a", "libsexpr-reference.a", "libfont-reference.a")


class CalculatorReferenceGuards(unittest.TestCase):
    def fixture(self, root, *, reference=True, archive=True, substitute=False, omit=()):
        root = root.resolve()
        source, build, overlay = (root / name for name in ("source", "build", "overlay"))
        for path in (source, build, overlay):
            path.mkdir()
        (build / "config.h").write_text("config", encoding="utf-8")
        wrapper = overlay / "reference/resistors_access.cpp"
        wrapper.parent.mkdir()
        wrapper.write_text("#include <resistor_substitution_utils.cpp>\n", encoding="utf-8")
        relative = "pcb_calculator/calculator_panels/iec60664.cpp"
        compiled = [(target, overlay / (target + ".cpp")) for target in OTHER_TARGETS]
        compiled += [("resistor-reference", source / path) for path in RESISTOR_SOURCES]
        compiled.append(("resistor-reference", wrapper))
        compiled += [("calculator-reader-reference", source / path) for path in READER_SOURCES]
        compiled.append(("calculator-reader-reference", build / READER_GENERATED))
        compiled += [("kiplatform-unix-reference", source / path) for path in KIPLATFORM_SOURCES]
        compiled += [("sexpr-reference", source / path) for path in SEXPR_SOURCES]
        compiled += [("font-reference", source / path) for path in FONT_REFERENCE_SOURCES]
        if reference:
            compiled.append(("iec60664-reference",
                             overlay / "iec60664.cpp" if substitute else source / relative))
        database = [{"file": str(path), "arguments": [
            "g++", "-fno-fast-math", "-ffp-contract=off", "-c", str(path),
            "-o", f"CMakeFiles/{target}.dir/impl.cpp.o"]}
            for target, path in compiled if target not in omit]
        (build / "compile_commands.json").write_text(json.dumps(database), encoding="utf-8")
        pinned = ((relative,) + RESISTOR_SOURCES + READER_SOURCES + KIPLATFORM_SOURCES + SEXPR_SOURCES
                  + FONT_REFERENCE_SOURCES)
        manifest = {"files": {path: "test-hash" for path in pinned},
                    "overlay_revision": "test-revision", "overlay_dirty_at_export": False}
        link = "g++ -fno-fast-math -ffp-contract=off -o kicad-oracle libkimath.a"
        if archive:
            link += " libiec60664-reference.a"
        link += " " + " ".join(OTHER_ARCHIVES)

        def output(args, **_kwargs):
            return link if args[0] == "ninja" else "test-toolchain"

        return build, source, overlay, manifest, output

    def run_audit(self, root, **kwargs):
        build, source, overlay, manifest, output = self.fixture(root, **kwargs)
        with patch("audit.verify_source", return_value=(manifest, "manifest-test-hash")), \
                patch("audit.verify_reader_generated", return_value={"test": "generated"}), \
                patch("audit.subprocess.check_output", side_effect=output):
            audit(build, source, overlay)
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
            self.assertEqual(len(record["compile_commands"]), 16 + len(FONT_REFERENCE_SOURCES))
            self.assertIn("libiec60664-reference.a", record["build_commands"][0])

    def test_other_reference_closures_remain_required_with_calculator_present(self):
        for target in ("fmt", "resistor-reference", "calculator-reader-reference",
                       "kiplatform-unix-reference", "sexpr-reference", "font-reference"):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory, \
                    self.assertRaisesRegex(ValueError, "Unexpected reference target closure"):
                self.run_audit(Path(directory), omit=(target,))


if __name__ == "__main__":
    unittest.main()
