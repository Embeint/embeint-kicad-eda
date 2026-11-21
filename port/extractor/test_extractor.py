# SPDX-License-Identifier: GPL-3.0-or-later
"""Run with: python -m unittest discover -s port/extractor -p 'test_*.py'."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from extractor.model import PIN, classify, digest, expand_virtual, layers, weighted_dag, write_json, wx_class
from split_units import dump, split
from validate_units import validate
import drift_report


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
