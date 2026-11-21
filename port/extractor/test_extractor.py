# SPDX-License-Identifier: GPL-3.0-or-later
"""Run with: python -m unittest discover -s port/extractor -p 'test_*.py'."""
import importlib.util
import gzip
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from extractor.model import PIN, classify, digest, expand_virtual, layers, weighted_dag, write_json, write_graph, write_cache, read_json, wx_class
from split_units import dump, split
from validate_units import validate
import drift_report
from extractor.reconcile_database import compare, cmake_options
from extractor.compact_cache import compact
from extraction_report import summarize, measure_units


def fixture(root):
    files, functions = [], {}
    for name, content in (("a.cpp", "int a() { return 1; }\n"),
                          ("b.cpp", "int b() { return 2; }\n")):
        (root / name).write_text(content, encoding="utf-8")
        files.append({"path": name, "lines": 1, "sha256": digest(content.encode()),
                      "category": "application", "coverage": "parsed"})
        functions[name] = {"file": name, "start": 1, "end": 1, "lines": 1,
                           "name": name, "wx": [], "wx_direct": "pure", "wx_class": "pure"}
    provenance = {"upstream_pin": PIN, "source_commit": "a" * 40,
                  "source_inventory_sha256": digest(json.dumps([{k: v for k, v in f.items()
                      if k != "coverage"} for f in files], sort_keys=True).encode()),
                  "inventory_complete": True, "parse_complete": True,
                  "compile_database_complete": True, "all_configurations_complete": False,
                  "gate_complete": True}
    graph = {"functions": functions, "edges": [["a.cpp", "b.cpp", "static"]], "provenance": provenance}
    inventory = {"files": files, "provenance": provenance}
    return graph, inventory


def save_fixture(root, graph, inventory, target=1):
    units, report = split(graph, inventory, target)
    directory = root / "port/extractor/generated"
    for name, value in (("graph", graph), ("inventory", inventory), ("units", report)):
        write_json(directory / (name + ".json"), value)
    (root / "port/units").mkdir(parents=True, exist_ok=True)
    for uid, unit in units.items():
        (root / "port/units" / (uid + ".toml")).write_text(dump(unit), encoding="utf-8")
    return units, report


class GraphTests(unittest.TestCase):
    def test_absent_source_is_never_passed_to_clang(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            result = extract_translation_unit({"root": tmp, "old_root": "/old/source",
                "file": "/old/source/deleted.cpp"})
            self.assertEqual(result["status"], "missing-source")
            self.assertEqual(result["functions"], {})

    def test_reconciliation_uses_actual_commands_and_preserves_stale_accounting(self):
        raw = [{"file": "/old/src/a.cpp"}, {"file": "/old/src/deleted.cpp"}, {"file": "/old/build/api/p.cc"}]
        new = [{"file": "/new/src/a.cpp"}, {"file": "/new/src/new.cpp"}, {"file": "/new/build/api/p.cc"}]
        result = compare(raw, new, "/old/src", "/new/src", "/old/build", "/new/build")
        self.assertEqual(result["raw_only_sources"], ["deleted.cpp"])
        self.assertEqual(result["reconciled_only_sources"], ["new.cpp"])
        self.assertEqual(result["shared_source_count"], 2)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "CMakeCache.txt"
            path.write_text("KICAD_USE_PCH:BOOL=ON\nKICAD_NOT_OPTION:INTERNAL=omit\nPoppler_Glib_LIBRARY:FILEPATH=/real/lib.so\nCMAKE_BUILD_TYPE:STRING=Release\n")
            self.assertEqual(cmake_options(path), {"KICAD_USE_PCH": "ON", "Poppler_Glib_LIBRARY": "/real/lib.so", "CMAKE_BUILD_TYPE": "Release"})

    def test_portable_deterministic_graph_storage(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "graph.json"
            value = {"z": [3, 2, 1], "a": "graph"}
            write_graph(path, value)
            first = path.with_suffix(".json.gz").read_bytes()
            write_graph(path, dict(reversed(list(value.items()))))
            self.assertEqual(first, path.with_suffix(".json.gz").read_bytes())
            self.assertEqual(first[4:8], b"\0\0\0\0")
            self.assertEqual(first[9], 255)
            self.assertEqual(read_json(path), value)

    def test_compressed_cache_roundtrip_and_atomic_publication(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cache" / "tu.json"
            value = {"status": "ok", "functions": {"f": {"name": "function"}}}
            write_cache(path, value)
            self.assertEqual(read_json(path), value)
            self.assertFalse(path.exists())
            self.assertFalse(list(path.parent.glob("*.tmp")))
            self.assertEqual(path.with_suffix(".json.gz").read_bytes()[9], 255)
            write_cache(path, {"status": "parse-error"})
            self.assertEqual(read_json(path), {"status": "parse-error"})

    def test_cache_compaction_preserves_exact_bytes_and_ignores_incomplete_entries(self):
        with tempfile.TemporaryDirectory() as tmp:
            cache = Path(tmp)
            path = cache / "key.json"
            original = b'{ "status": "parse-error", "functions": {}, "diagnostics": ["failure"] }\n'
            path.write_bytes(original)
            (cache / "pending.json").write_text('{"status":')
            result = compact(cache, 0)
            self.assertEqual(len(result["entries"]), 1)
            self.assertFalse(path.exists())
            self.assertEqual(gzip.decompress(path.with_suffix(".json.gz").read_bytes()), original)
            self.assertEqual(result["entries"][0]["original_sha256"], digest(original))
            self.assertTrue((cache / "pending.json").exists())

    def test_reports_do_not_count_stale_exclusions_as_successful_parses(self):
        with tempfile.TemporaryDirectory() as tmp:
            graph, inventory = fixture(Path(tmp))
            graph["provenance"].update(compile_commands_sha256="a", source_manifest_sha256="b",
                extractor_sha256="c", commands_attempted=2, compile_commands_count=2,
                unresolved_call_count=0, uncompiled_application_sources=[])
            graph.update(overrides=[], translation_units=[
                {"file": "a.cpp", "status": "ok", "diagnostics": []},
                {"file": "deleted.cpp", "status": "stale-excluded", "diagnostics": ["not parsed"]}])
            result = summarize(graph, inventory, ["deleted.cpp"])
            self.assertEqual(result["parsed_ok"], 1)
            self.assertEqual(result["stale_deleted_excluded"][0]["recorded_status"], "stale-excluded")
            self.assertEqual(result["non_stale_failures"], [])
            graph["translation_units"][1]["status"] = "ok"
            with self.assertRaisesRegex(ValueError, "deleted source"):
                summarize(graph, inventory, ["deleted.cpp"])

    def test_all_virtual_overrides_transitive_and_uninstantiated(self):
        edges = expand_virtual([("caller", "Base::f", True), ("direct", "Base::f", False)],
                               [("Base::f", "Left::f"), ("Left::f", "Leaf::f"),
                                ("Base::f", "Unused::f")])
        self.assertEqual({t for c, t, _ in edges if c == "caller"},
                         {"Base::f", "Left::f", "Leaf::f", "Unused::f"})
        self.assertEqual([e for e in edges if e[0] == "direct"], [("direct", "Base::f", "static")])

    def test_weighted_cycle_break_and_layers(self):
        weights = {("a", "b"): 100, ("b", "c"): 20, ("c", "a"): 1, ("c", "c"): 3}
        adj, dropped = weighted_dag(["c", "b", "a"], weights)
        self.assertEqual(adj, {"a": {"b"}, "b": {"c"}, "c": set()})
        self.assertEqual({(e["caller"], e["callee"]) for e in dropped}, {("c", "a"), ("c", "c")})
        self.assertEqual(layers(adj), {"c": 0, "b": 1, "a": 2})
        self.assertEqual(weighted_dag(["a", "b", "c"], dict(reversed(list(weights.items())))), (adj, dropped))
        with self.assertRaisesRegex(ValueError, "cycle"):
            layers({"a": {"b"}, "b": {"a"}})

    def test_wx_propagates_through_cycles_and_overrides(self):
        functions = {"a": {"wx": []}, "b": {"wx": ["wxString"]}, "ui": {"wx": ["wxFrame"]}}
        classify(functions, [("a", "b", "static"), ("b", "a", "static"), ("a", "ui", "override")])
        self.assertEqual(functions["b"]["wx_direct"], "thin-replacement")
        self.assertEqual({f["wx_class"] for f in functions.values()}, {"UI"})
        self.assertEqual(wx_class([]), "pure")
        self.assertEqual(wx_class(["wxString", "wxPoint"]), "thin-replacement")

    def test_stable_splitting_preserves_function_boundaries_and_soft_calls(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            graph["edges"].append(["b.cpp", "a.cpp", "override"])
            first = split(graph, inventory, 1)
            graph["functions"] = dict(reversed(list(graph["functions"].items())))
            graph["edges"].reverse()
            inventory["files"].reverse()
            self.assertEqual(first, split(graph, inventory, 1))
            self.assertEqual(len(first[1]["back_edges"]), 1)
            self.assertEqual(first[1]["back_edges"][0]["weight"], 1)
            graph["functions"]["a.cpp"]["end"] = 6
            inventory["files"][1]["lines"] = 7
            units, _ = split(graph, inventory, 2)
            owner = next(u for u in units.values() if "a.cpp" in u["functions"])
            self.assertIn("a.cpp:1:6", owner["source_ranges"])

    def test_source_category_and_path_aliases_cannot_drop_units(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            inventory["files"][1]["category"] = "generated"
            units, report = split(graph, inventory)
            self.assertEqual(len(units), 2)
            self.assertEqual(report["accounted_lines"], 2)
            inventory["files"][0]["coverage"] = "uncompiled"
            units, _ = split(graph, inventory)
            unknown = next(u for u in units.values() if "a.cpp" in u["upstream"])
            self.assertEqual(unknown["status"], "blocked")
            self.assertEqual(unknown["wx_class"], "UI")


class ValidationTests(unittest.TestCase):
    def test_valid_incomplete_configuration_and_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            units, _ = save_fixture(root, graph, inventory)
            self.assertEqual(validate(root, True), [])
            graph["provenance"]["gate_complete"] = False
            save_fixture(root, graph, inventory)
            self.assertEqual(validate(root), [])
            self.assertTrue(any("incomplete" in e for e in validate(root, True)))
            unit = next(iter(units.values()))
            unit["depends_on"] = ["missing"]
            (root / "port/units" / (unit["id"] + ".toml")).write_text(dump(unit))
            self.assertTrue(any("unknown/self" in e for e in validate(root)))

    def test_empty_malformed_hash_and_green_coverage(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertIn("No unit", validate(root)[0])
            graph, inventory = fixture(root)
            units, _ = save_fixture(root, graph, inventory)
            (root / "a.cpp").write_text("changed\n")
            self.assertTrue(any("source changed" in e for e in validate(root)))
            uid, unit = next(iter(units.items()))
            unit.update(status="green", tests=[], coverage_gaps=["a.cpp"])
            (root / "port/units" / (uid + ".toml")).write_text(dump(unit))
            self.assertTrue(any("green requires" in e for e in validate(root)))
            (root / "port/units" / (uid + ".toml")).write_text("invalid = [")
            self.assertTrue(validate(root))

    def test_missing_calls_wx_and_parse_provenance_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            units, _ = save_fixture(root, graph, inventory)
            uid, unit = next((k, v) for k, v in units.items() if v["depends_on"])
            unit["depends_on"] = []
            (root / "port/units" / (uid + ".toml")).write_text(dump(unit))
            self.assertTrue(any("cross-unit call" in e for e in validate(root)))
            graph["functions"]["a.cpp"]["wx"] = ["wxFrame"]
            graph["provenance"]["parse_failures"] = [{"file": "a.cpp"}]
            save_fixture(root, graph, inventory)
            errors = validate(root)
            self.assertTrue(any("wx classification" in e for e in errors))
            self.assertTrue(any("parse failures hidden" in e for e in errors))


class DriftTests(unittest.TestCase):
    def test_insert_delete_mapping(self):
        self.assertEqual(drift_report.hunks("@@ -2,0 +3,5 @@\n@@ -9 +14 @@"), [(2, 0, 3, 5), (9, 1, 14, 1)])
        self.assertEqual(drift_report.map_range(13, 18, [(2, 0, 3, 5)]), (8, 13))
        self.assertTrue(drift_report.touched(5, 10, [(7, 0)]))
        self.assertTrue(drift_report.touched(5, 10, [(1, 5)]))
        self.assertFalse(drift_report.touched(5, 10, [(11, 2)]))

    def test_committed_body_deletion_header_and_todo_filter(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def git(*args):
                return subprocess.check_output(["git", "-C", str(root), *args], encoding="utf-8").strip()
            git("init", "--quiet")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.invalid")
            (root / "a.cpp").write_text("int a() {\nreturn 1;\n}\n")
            (root / "b.h").write_text("using X = int;\n")
            git("add", "a.cpp", "b.h")
            git("commit", "-qm", "test fixture base")
            base = git("rev-parse", "HEAD")
            graph = {"provenance": {"upstream_pin": base, "source_commit": base},
                     "functions": {"a": {"file": "a.cpp", "start": 1, "end": 3, "name": "a"}}}
            units = [{"id": "u", "status": "green", "functions": ["a"], "upstream": ["a.cpp", "b.h"]}]
            (root / "a.cpp").write_text("int a() {\nreturn 2;\n}\n")
            (root / "b.h").write_text("using X = long;\n")
            git("add", "a.cpp", "b.h")
            git("commit", "-qm", "test fixture changed")
            original = drift_report.PIN
            try:
                drift_report.PIN = base
                result = drift_report.report(root, graph, units)
                self.assertEqual(result["affected_units"], ["u"])
                self.assertEqual({a["file"] for a in result["affected"]}, {"a.cpp", "b.h"})
                units[0]["status"] = "todo"
                self.assertFalse(drift_report.report(root, graph, units)["affected"])
                units[0]["status"] = "partial"
                (root / "a.cpp").unlink()
                git("add", "a.cpp")
                git("commit", "-qm", "test fixture removed")
                self.assertTrue(any(a.get("function") == "a" for a in drift_report.report(root, graph, units)["affected"]))
            finally:
                drift_report.PIN = original


@unittest.skipUnless(importlib.util.find_spec("clang"), "matching libclang Python bindings unavailable")
class ClangIntegrationTests(unittest.TestCase):
    def test_template_instances_reach_primary_bodies_and_keep_explicit_specializations(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "template.cpp"
            source.write_text("""
template<class T> T twice(T x) { return x + x; }
template<> int twice<int>(int x) { return x * 3; }
template<class T> struct Box { T get() { return T(); } };
double call_generic() { Box<double> box; return twice(1.0) + box.get(); }
int call_specialized() { return twice(1); }
""", encoding="utf-8")
            result = extract_translation_unit({"root": str(root), "old_root": str(root), "file": str(source),
                "arguments": ["clang++", "-std=c++20", "-c", str(source)], "directory": str(root),
                "libclang": "/usr/lib/llvm-21/lib/libclang-21.so.1"})
            self.assertEqual(result["status"], "ok", result["diagnostics"])
            functions = result["functions"]
            generic = next(u for u, f in functions.items() if f["name"] == "call_generic()")
            specialized = next(u for u, f in functions.items() if f["name"] == "call_specialized()")
            generic_targets = {b for a, b, _ in result["calls"] if a == generic and b in functions}
            explicit_targets = {b for a, b, _ in result["calls"] if a == specialized and b in functions}
            self.assertTrue(any("twice" in functions[u]["name"] for u in generic_targets))
            self.assertTrue(any("get" in functions[u]["name"] for u in generic_targets))
            self.assertTrue(any("twice" in functions[u]["name"] for u in explicit_targets))
            self.assertFalse(generic_targets & explicit_targets)
            self.assertTrue(result["template_aliases"])

    def test_real_virtual_inheritance_wx_and_indirect_calls(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "fixture.cpp"
            source.write_text("""
struct wxString {};
struct wxFrame {};
struct Base { virtual int f() = 0; };
struct Left : Base { int f() override { return 1; } };
struct Leaf : Left { int f() override { return 2; } };
struct Unused : Base { int f() override { return 3; } };
struct Window : wxFrame { int empty() { return 0; } };
int dispatch(Base& b) { return b.f(); }
int thin(wxString s) { return 0; }
int transit(wxString s) { return thin(s); }
int indirect(int (*p)()) { return p(); }
""", encoding="utf-8")
            result = extract_translation_unit({"root": str(root), "old_root": str(root), "file": str(source),
                "arguments": ["clang++", "-std=c++20", "-c", str(source)], "directory": str(root),
                "libclang": "/usr/lib/llvm-21/lib/libclang-21.so.1"})
            self.assertEqual(result["status"], "ok", result["diagnostics"])
            edges = expand_virtual(result["calls"], result["overrides"])
            classify(result["functions"], edges)
            names = {k: v["name"] for k, v in result["functions"].items()}
            targets = {names.get(t, t) for c, t, kind in edges if names.get(c, "").startswith("dispatch(")}
            self.assertTrue(any("Unused::f" in n for n in targets))
            self.assertTrue(any("Leaf::f" in n for n in targets))
            self.assertEqual(next(v["wx_class"] for v in result["functions"].values() if "Window::empty" in v["name"]), "UI")
            self.assertEqual(next(v["wx_class"] for v in result["functions"].values() if v["name"].startswith("transit(")), "thin-replacement")
            self.assertTrue(any("indirect(" in names.get(c["caller"], "") for c in result["unresolved"]))


if __name__ == "__main__":
    unittest.main()
