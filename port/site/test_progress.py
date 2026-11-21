# SPDX-License-Identifier: GPL-3.0-or-later
"""Public, generated fixtures exercising the progress site's reporting contract."""

import importlib.util
from html.parser import HTMLParser
import json
from pathlib import Path
import sys
import tempfile
import unittest
from urllib.parse import urlsplit


sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("progress", Path(__file__).parents[1] / "tools/progress.py")
progress = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(progress)


class ProgressTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root, self.out = self.base / "repo", self.base / "site"
        self.root.mkdir()

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")

    def unit(self, uid, status="todo", **extras):
        fields = dict(id=uid, upstream=["libs/kimath/src/geometry/seg.cpp"],
                      rust_module="kicad_math::seg", depends_on=[], soft_depends_on=[],
                      est_lines=400, layer=0, tests=["qa/tests/libs/kimath/geometry/test_seg.cpp"],
                      status=status, claimed_by="", claimed_at="", notes="")
        fields.update(schema_version=1, wx_class="pure", source_category="application",
                      functions=["c:@S@SEG@F@Length#1"],
                      source_ranges=["libs/kimath/src/geometry/seg.cpp:1:400"], coverage_gaps=[])
        fields.update(extras)
        self.write(f"port/units/{uid}.toml", "\n".join(
            f"{key} = {json.dumps(value)}" for key, value in fields.items()) + "\n")

    def pin(self):
        self.write("port/pins.toml", '''schema_version = 1
[kicad]
url = "https://github.com/KiCad/kicad-source-mirror"
revision = "4cf939778df04d2b747bb2c4714fc5014a7565c7"
[solidcore]
url = "https://github.com/tangency-labs/solidcore"
revision = "7a588fa284765a83823293ae20b84da8356ab479"
solidcraft_revision = "93d5492ef295a1124664d36e967f58a81a76d7d2"
''')

    def records(self, uid, deferred=False, empty=False):
        request = {"unit": uid, "case": "degenerate-zero-length", "input": {"a": [0, 0], "b": [0, 0]}}
        reply = {"unit": uid, "case": request["case"], "result": {"length_nm": 0}}
        if deferred:
            reply.pop("result")
            reply["error"] = {"kind": "unported", "upstream": "SEG::Collide", "unit": "math.arc"}
        self.write(f"port/cases/{uid}.jsonl", "" if empty else json.dumps(request) + "\n")
        self.write(f"port/goldens/{uid}.jsonl", "" if empty else json.dumps(reply) + "\n")

    def extraction(self, **provenance_overrides):
        provenance = dict(inventory_complete=True, parse_complete=False,
                          compile_database_complete=True, all_configurations_complete=False,
                          gate_complete=False, compile_commands_count=2471, commands_attempted=2471,
                          unresolved_call_count=17, upstream_pin="4cf939778df04d2b747bb2c4714fc5014a7565c7",
                          configuration="Linux WSL compile database; active preprocessor branches only",
                          source_root="/home/private/worktree", compile_commands_path="/home/private/build/compile_commands.json",
                          limitations=["No Windows/macOS or inactive preprocessor branch extraction"],
                          parse_failures=[dict(file="common/platform.cpp", status="failed", diagnostics=["/home/private/build/error"])],
                          uncompiled_application_sources=["common/windows.cpp"])
        provenance.update(provenance_overrides)
        graph = dict(schema_version=1, provenance=provenance,
                     functions={"c:@S@SEG@F@Length#1": dict(name="SEG::Length", file="libs/kimath/src/geometry/seg.cpp",
                                                        start=1, end=400, lines=400, wx_class="pure")}, edges=[])
        inventory = dict(schema_version=1, provenance=provenance, files=[
            dict(path="libs/kimath/src/geometry/seg.cpp", category="application", coverage="parsed", lines=400),
            dict(path="common/platform.cpp", category="application", coverage="command-only", lines=200),
            dict(path="common/windows.cpp", category="application", coverage="uncompiled", lines=300),
            dict(path="eeschema/dialogs/dialog_base.cpp", category="generated", coverage="uncompiled", lines=100),
            dict(path="qa/tests/seg.cpp", category="qa", coverage="parsed", lines=50),
            dict(path="thirdparty/clipper2/clipper.cpp", category="thirdparty", coverage="parsed", lines=500)])
        report = dict(schema_version=1, function_units={"c:@S@SEG@F@Length#1": "math.seg"},
                      back_edges=[dict(caller="math.arc", callee="math.seg", weight=10,
                                       calls=[["c:@S@ARC@F@Length#1", "c:@S@SEG@F@Length#1", "static"]])])
        for name, data in (("graph", graph), ("inventory", inventory), ("units", report)):
            self.write(f"port/extractor/generated/{name}.json", json.dumps(data))

    def build(self):
        return progress.build(self.root, self.out)

    def html(self, path="index.html"):
        return (self.out / path).read_text(encoding="utf-8")

    def test_empty_is_not_a_vacuous_phase0_pass(self):
        data = self.build()
        self.assertEqual(data["summary"]["units"], 0)
        self.assertEqual(data["phase0_gate"], "not_verified")
        self.assertIn("No unit records available", self.html())
        self.assertIn("Generated inventory not available", self.html())
        self.assertIn("Upstream pin not available", self.html())
        self.assertIn("Zero records cannot satisfy it", self.html())
        self.assertNotIn("100%", self.html())

    def test_mixed_statuses_stored_counts_and_source_links(self):
        self.pin()
        for status in progress.STATUSES:
            self.unit("math." + status, status, notes="Claim #42 with [evidence](https://example.org/public)")
        self.records("math.partial", deferred=True)
        self.write("port/goldens/evidence/math.partial-baseline.json", '{"reason":"public generated case"}')
        data = self.build()
        self.assertEqual(data["summary"]["count"], dict.fromkeys(progress.STATUSES, 1))
        self.assertEqual(data["summary"]["green_lines"], 400)
        self.assertEqual(data["summary"]["partial_lines"], 400)
        unit = self.html("units/math.partial.html")
        self.assertIn("1 stored deferred replies", unit)
        self.assertIn('/blob/rust/port/cases/math.partial.jsonl', unit)
        self.assertIn('/blob/rust/port/goldens/evidence/math.partial-baseline.json', unit)
        self.assertIn('/pull/42', unit)
        self.assertIn('href="https://example.org/public"', unit)
        self.assertIn("4cf939778df04d2b747bb2c4714fc5014a7565c7", self.html())
        self.assertIn("93d5492ef295a1124664d36e967f58a81a76d7d2", self.html())

    def test_three_green_records_do_not_certify_live_oracle_gate(self):
        for uid in ("math.seg", "math.box", "math.vector"):
            self.unit(uid, "green")
            self.records(uid)
        data = self.build()
        self.assertEqual(data["summary"]["count"]["green"], 3)
        self.assertEqual(data["phase0_gate"], "not_verified")
        self.assertIn("3 / 3 units declared green", self.html())
        self.assertIn("Phase 0 incomplete: pending pilot verification", self.html())

    def test_green_with_deferred_evidence_is_flagged(self):
        self.unit("math.seg", "green")
        self.records("math.seg", deferred=True)
        data = self.build()
        self.assertIn("this evidence cannot support green", self.html("units/math.seg.html"))
        self.assertEqual(data["summary"]["count"]["green"], 0)
        self.assertEqual(data["summary"]["count"]["partial"], 1)
        self.assertEqual(data["units"][0]["declared_status"], "green")

    def test_green_with_coverage_gap_is_reported_partial(self):
        self.unit("math.seg", "green", coverage_gaps=["common/windows.cpp"])
        data = self.build()
        self.assertEqual(data["summary"]["count"]["green"], 0)
        self.assertEqual(data["units"][0]["status"], "partial")

    def test_empty_golden_and_missing_golden_are_distinct(self):
        self.unit("math.empty")
        self.unit("math.missing")
        self.records("math.empty", empty=True)
        data = self.build()
        by_id = {unit["id"]: unit for unit in data["units"]}
        self.assertEqual(by_id["math.empty"]["goldens"]["records"], 0)
        self.assertIsNone(by_id["math.missing"]["goldens"])

    def test_escape_toml_markdown_urls_and_findings(self):
        payload = '<script>alert("x")</script> [bad](javascript:alert(1)) [quote](https://example.org/"onload="x)'
        self.unit("math.seg", claimed_by='<img src=x onerror=alert(1)>', notes=payload,
                  wx_classification='<svg onload="x">')
        self.write("port/findings/math.seg--public.md", payload)
        self.write("port/upstream-issues/WR-1.md", '<iframe src="file:///private"></iframe>')
        self.build()
        unit = self.html("units/math.seg.html")
        for page in (unit, self.html("records.html")):
            self.assertNotIn("<script>", page)
            self.assertNotIn('<img src=x', page)
            self.assertNotIn('<iframe', page)
            self.assertNotIn('href="javascript:', page)
            self.assertNotIn('"onload="x', page)
            self.assertIn("&lt;script&gt;", page)
        self.assertIn("&lt;svg", unit)

    def test_deterministic_bytes_without_clock_or_network(self):
        self.unit("math.seg")
        self.records("math.seg")
        self.build()
        first = {p.relative_to(self.out).as_posix(): p.read_bytes() for p in self.out.rglob("*") if p.is_file()}
        self.build()
        second = {p.relative_to(self.out).as_posix(): p.read_bytes() for p in self.out.rglob("*") if p.is_file()}
        self.assertEqual(first, second)

    def test_rebuild_removes_obsolete_unit_pages(self):
        self.unit("math.seg")
        self.build()
        (self.root / "port/units/math.seg.toml").unlink()
        self.build()
        self.assertFalse((self.out / "units/math.seg.html").exists())

    def test_nonempty_unowned_output_is_not_modified(self):
        self.out.mkdir()
        unrelated = self.out / "index.html"
        unrelated.write_text("keep this", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "not owned"):
            self.build()
        self.assertEqual(unrelated.read_text(encoding="utf-8"), "keep this")

    def test_extractor_flags_coverage_limits_and_back_edges(self):
        self.unit("math.seg", "green")
        self.unit("math.arc", "blocked", wx_class="thin-replacement", soft_depends_on=["math.seg"])
        self.extraction()
        data = self.build()
        flags = data["extraction"]["completeness"]
        self.assertIs(flags["inventory_complete"], True)
        self.assertIs(flags["parse_complete"], False)
        self.assertIs(flags["compile_database_complete"], True)
        self.assertIs(flags["all_configurations_complete"], False)
        self.assertIs(flags["gate_complete"], False)
        inventory = self.html("inventory.html")
        for label in ("2471", "6 tracked C/C++ paths", "command-only", "uncompiled", "thirdparty", "thin-replacement",
                      "Unresolved calls", "common/platform.cpp", "common/windows.cpp", "Extractor completeness (not the plan phase 0 gate)"):
            self.assertIn(label, inventory)
        self.assertNotIn("/home/private", inventory)
        self.assertNotIn("/home/private", self.html())
        self.assertIn("Recorded cycle back-edges", self.html("layers.html"))
        self.assertIn("Weight", self.html("units/math.arc.html"))
        self.assertIn("#L1-L400", self.html("units/math.seg.html"))
        self.assertEqual(data["phase0_gate"], "not_verified")

    def test_partial_generated_artifacts_remain_visibly_incomplete(self):
        self.extraction()
        (self.root / "port/extractor/generated/units.json").unlink()
        self.build()
        self.assertIn("Generated artifacts not available: units", self.html())

    def test_generated_navigation_and_stylesheet_links_resolve(self):
        self.unit("math.seg", "partial")
        self.unit("math.arc", "blocked", depends_on=["math.seg"])
        self.extraction()
        self.build()

        class Links(HTMLParser):
            def __init__(self):
                super().__init__()
                self.hrefs = []

            def handle_starttag(self, tag, attrs):
                self.hrefs.extend(value for key, value in attrs if key == "href")

        for page in self.out.rglob("*.html"):
            parser = Links()
            parser.feed(page.read_text(encoding="utf-8"))
            for href in parser.hrefs:
                parsed = urlsplit(href)
                if not parsed.scheme and parsed.path:
                    self.assertTrue((page.parent / parsed.path).is_file(), f"{page.name}: {href}")

    def test_extractor_nonboolean_flag_is_rejected(self):
        self.extraction(inventory_complete="true")
        with self.assertRaisesRegex(ValueError, "must be a boolean"):
            self.build()

    def test_dependencies_and_layers_are_visible(self):
        self.unit("math.seg", "partial")
        self.unit("math.arc", "blocked", depends_on=["math.seg"], soft_depends_on=["math.seg"], layer=2)
        self.build()
        self.assertIn("math.seg", self.html("layers.html"))
        self.assertIn("partial", self.html("layers.html"))
        self.assertIn("Layer 2", self.html("units/math.arc.html"))

    def test_malformed_record_and_duplicate_keys_fail_build(self):
        self.unit("math.seg")
        self.write("port/goldens/math.seg.jsonl", '{"unit":"math.seg","case":"a","result":0,"result":1}\n')
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            self.build()

    def test_invalid_unit_and_unknown_dependency_fail_build(self):
        self.unit("math.seg", "complete")
        with self.assertRaisesRegex(ValueError, "unknown status"):
            self.build()
        self.unit("math.seg", depends_on=["missing"])
        with self.assertRaisesRegex(ValueError, "unknown dependency"):
            self.build()

    def test_source_paths_cannot_escape_repository(self):
        self.unit("math.seg", upstream=["../../private.kicad_pcb"])
        with self.assertRaisesRegex(ValueError, "repository-relative"):
            self.build()

    def test_findings_symlink_cannot_read_private_file(self):
        private = self.base / "private.md"
        private.write_text("private fixture", encoding="utf-8")
        findings = self.root / "port/findings"
        findings.mkdir(parents=True)
        try:
            (findings / "leak.md").symlink_to(private)
        except OSError:
            self.skipTest("host cannot create symlinks")
        with self.assertRaisesRegex(ValueError, "escapes the repository"):
            self.build()


if __name__ == "__main__":
    unittest.main()
