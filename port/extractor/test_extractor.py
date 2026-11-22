# SPDX-License-Identifier: GPL-3.0-or-later
"""Run with: python -m unittest discover -s port/extractor -p 'test_*.py'."""
import importlib.util
import gzip
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from extractor.model import PIN, classify, digest, expand_virtual, materialize_definitions, layers, weighted_dag, write_json, write_graph, write_cache, read_json, wx_class
from split_units import dump, normalize_eof_ranges, split
from validate_units import validate
import drift_report
from extractor.reconcile_database import compare, cmake_options
from extractor.compact_cache import compact
from extraction_report import summarize, measure_units
from extractor.generated_sources import capture_recipes, freeze, verify, source_path


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
    def test_cli_regeneration_preserves_port_wiring_and_refreshes_accounting(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            units, _ = save_fixture(root, graph, inventory)
            uid = sorted(units)[0]
            previous = dict(units[uid])
            previous.update(rust_module="kicad_md5::md5_hash", status="claimed",
                            claimed_by="worker", claimed_at="2026-11-22T00:00:00Z",
                            tests=["difftest::md5"], notes="Preserve portable wiring: \u03bc")
            previous["difftest"] = {"rust_package": "kicad-md5-shim",
                                    "rust_binary": "kicad-md5-shim", "rust_args": []}
            previous.update(est_lines=999, depends_on=["stale-dependency"],
                            source_ranges=["stale.cpp:1:999"])
            path = root / "port/units" / (uid + ".toml")
            path.write_text(dump(previous), encoding="utf-8", newline="\n")
            generated = root / "port/extractor/generated"
            command = [sys.executable, str(Path(__file__).resolve().parents[1] / "tools/split_units.py"),
                       "--graph", str(generated / "graph.json"),
                       "--inventory", str(generated / "inventory.json"),
                       "--units-dir", str(path.parent), "--report", str(generated / "units.json"),
                       "--source-root", str(root), "--target-lines", "1"]
            subprocess.run(command, check=True, capture_output=True, text=True)
            restored = tomllib.loads(path.read_text(encoding="utf-8"))
            for key in ("rust_module", "status", "claimed_by", "claimed_at", "tests", "notes", "difftest"):
                self.assertEqual(restored[key], previous[key])
            for key in ("est_lines", "depends_on", "source_ranges", "functions", "layer"):
                self.assertEqual(restored[key], units[uid][key])
            self.assertEqual(validate(root), [])
            first = path.read_bytes()
            subprocess.run(command, check=True, capture_output=True, text=True)
            self.assertEqual(path.read_bytes(), first)

    def test_exclusive_eof_recovery_counts_only_verified_physical_lines(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            fn = graph["functions"]["a.cpp"]
            data = (root / "a.cpp").read_bytes()
            fn.update(end=2, start_offset=0, end_offset=len(data), range_kind="enclosing-declaration",
                      sha256=digest(data.replace(b"\r\n", b"\n")))
            corrections = normalize_eof_ranges(graph, inventory, root)
            self.assertEqual(fn["end"], 1)
            self.assertEqual(fn["clang_end_line"], 2)
            self.assertEqual(corrections[0]["source_bytes_sha256"], digest(data))
            units, report = split(graph, inventory)
            self.assertEqual(report["accounted_lines"], 2)
            self.assertEqual(normalize_eof_ranges(graph, inventory, root), [])
            fn.update(end=2, end_offset=len(data) - 1)
            with self.assertRaisesRegex(ValueError, "Unproven"):
                normalize_eof_ranges(graph, inventory, root)

    def test_same_usr_in_separate_executables_keeps_each_body_owned(self):
        first = {"file": "one.cpp", "start": 1, "end": 2, "wx": []}
        second = {"file": "two.cpp", "start": 3, "end": 4, "wx": ["wxString"]}
        functions = {"main": first, "callee": {"wx": []}}
        calls, overrides, copies = materialize_definitions(functions,
            [("main", "callee", False), ("entry", "main", False)], [],
            {"main": {("one.cpp", 1, 2): first, ("two.cpp", 3, 4): second}})
        self.assertEqual(len(copies), 1)
        duplicate = next(iter(copies))
        self.assertEqual(functions[duplicate]["file"], "two.cpp")
        self.assertEqual(functions[duplicate]["clang_usr"], "main")
        self.assertIn((duplicate, "callee", False), calls)
        self.assertIn(("entry", duplicate, False), calls)
        self.assertEqual(overrides, [])

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
            os.utime(path, (1, 1))
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
    def test_source_manifest_does_not_reinventory_frozen_artifacts_or_port_tooling(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            for path in ['application.cpp','port/extractor/generated/sources/lexer.cpp','port/oracle/shim.cpp','rust/host/shim.cpp']:
                target=root/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_text('int value;\n')
            def git(*args):
                subprocess.run(['git','-C',str(root),*args],check=True,capture_output=True)
            git('init','--quiet');git('add','.')
            git('-c','user.name=Test','-c','user.email=test@example.invalid','commit','-qm','test source inventory fixture')
            manifest=root/'manifest.json'
            subprocess.run([sys.executable,str(Path(__file__).with_name('source_manifest.py')),
                '--root',str(root),'--output',str(manifest)],check=True,capture_output=True)
            data=read_json(manifest)
            self.assertEqual(data['files'],['application.cpp'])
            self.assertEqual(len(data['excluded_port_tooling']),3)

    def test_active_generated_tu_cannot_be_silently_uninventoried(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);graph,inventory=fixture(root)
            graph['provenance'].update(commands_attempted=1,compile_commands_count=1)
            graph['translation_units']=[{'file':'/private/build/lexer.cpp','status':'ok','category':'generated-build'}]
            save_fixture(root,graph,inventory)
            self.assertTrue(any('lacks frozen source mapping' in e for e in validate(root)))
            graph['translation_units']=[{'file':'a.cpp','status':'ok','category':'tracked-source','includes':['@build/late.h']}]
            save_fixture(root,graph,inventory)
            self.assertTrue(any('generated include lacks' in e for e in validate(root)))

    def test_generated_source_storage_accounting_and_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            build = root / 'private-build'
            build.mkdir()
            (build / 'lexer.cpp').write_bytes(b'int generated() { return 1; }\n')
            graph, inventory = fixture(root)
            generated = root / 'port/extractor/generated'
            manifest = freeze(build, generated, {'source_commit': 'a' * 40})
            recipe_bytes = b'{"@build/lexer.cpp":{"query":"public generator input"}}\n'
            (generated/'generated-recipes.json').write_bytes(recipe_bytes)
            manifest['recipes_sha256'] = digest(recipe_bytes)
            write_json(generated/'generated-sources.json', manifest)
            entries = verify(manifest, build, generated)
            inventory['files'].extend(dict(e, coverage='parsed') for e in entries)
            provenance = graph['provenance']
            provenance['tracked_source_inventory_sha256'] = digest(json.dumps([{k:v for k,v in f.items() if k!='coverage'}
                for f in inventory['files'] if f.get('origin') != 'build-generated'], sort_keys=True).encode())
            provenance['generated_sources_manifest_sha256'] = digest((generated / 'generated-sources.json').read_bytes())
            provenance['source_inventory_sha256'] = digest(json.dumps([{k:v for k,v in f.items() if k!='coverage'}
                for f in inventory['files']], sort_keys=True).encode())
            graph['functions']['generated'] = dict(graph['functions']['a.cpp'], file='@build/lexer.cpp', name='generated')
            graph['edges'].append(['a.cpp', 'generated', 'static'])
            units, report = save_fixture(root, graph, inventory)
            self.assertEqual(report['accounted_lines'], 3)
            self.assertTrue(any('@build/lexer.cpp' in u['upstream'] for u in units.values()))
            self.assertEqual(validate(root), [])
            compressed_recipe = generated/'generated-recipes.json.gz'
            compressed_recipe.write_bytes(gzip.compress(recipe_bytes, mtime=0))
            (generated/'generated-recipes.json').unlink()
            self.assertEqual(validate(root), [])
            compressed_recipe.write_bytes(gzip.compress(recipe_bytes+b' ', mtime=0))
            self.assertTrue(any('recipes fingerprint mismatch' in e for e in validate(root)))
            compressed_recipe.write_bytes(gzip.compress(recipe_bytes, mtime=0))
            manifest['provenance']['reconciliation.json'] = {'record': {'source_commit': 'b' * 40, 'upstream_pin': PIN}}
            write_json(generated/'generated-sources.json', manifest)
            provenance['generated_sources_manifest_sha256'] = digest((generated/'generated-sources.json').read_bytes())
            save_fixture(root, graph, inventory)
            self.assertTrue(any('configure source pin' in e for e in validate(root)))
            del manifest['provenance']['reconciliation.json']
            write_json(generated/'generated-sources.json', manifest)
            provenance['generated_sources_manifest_sha256'] = digest((generated/'generated-sources.json').read_bytes())
            save_fixture(root, graph, inventory)
            with self.assertRaises(ValueError):
                source_path(dict(entries[0], storage_path='../lexer.cpp'), root, generated)
            source_path(entries[0], root, generated).write_bytes(b'int fake() { return 2; }\n')
            self.assertTrue(any('source changed' in e for e in validate(root)))
            with self.assertRaisesRegex(ValueError, 'changed'):
                verify(manifest, build, generated)

    def test_inventory_coverage_and_categories_cannot_be_hidden_by_unit_edits(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            inventory["files"][0]["coverage"] = "uncompiled"
            units, _ = save_fixture(root, graph, inventory)
            unit = next(u for u in units.values() if "a.cpp" in u["upstream"])
            unit.update(coverage_gaps=[], status="green", tests=["case"], wx_class="pure")
            path = root / "port/units" / (unit["id"] + ".toml")
            path.write_text(dump(unit))
            errors = validate(root)
            self.assertTrue(any("coverage gaps differ" in e for e in errors))
            self.assertTrue(any("conservative UI" in e for e in errors))
            unit.update(coverage_gaps=["a.cpp"], status="blocked", wx_class="UI", source_category="generated")
            path.write_text(dump(unit))
            self.assertTrue(any("source category differs" in e for e in validate(root)))

    def test_manifest_fingerprint_survives_checkout_line_endings(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graph, inventory = fixture(root)
            manifest = {"source_commit": "a" * 40, "upstream_pin": PIN, "files": ["a.cpp", "b.cpp"]}
            data = (json.dumps(manifest, indent=2) + "\n").encode()
            graph["provenance"].update(source_manifest_sha256=digest(data),
                source_manifest_hash_policy="SHA-256 after CRLF-to-LF normalization")
            save_fixture(root, graph, inventory)
            (root / "port/extractor/generated/source-manifest.json").write_bytes(data.replace(b"\n", b"\r\n"))
            self.assertEqual(validate(root), [])

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
    @unittest.skipUnless(shutil.which('ninja'), 'Ninja is needed for real recipe capture')
    def test_captured_multistage_generator_inputs_reach_tracked_consumers(self):
        with tempfile.TemporaryDirectory() as tmp:
            parent = Path(tmp)
            root, build = parent / 'source', parent / 'build'
            (root / 'api').mkdir(parents=True)
            (build / 'api').mkdir(parents=True)
            (root / 'api/schema.proto').write_text('first\n')
            for name in ('direct', 'relative', 'absolute'):
                (root / (name + '.cpp')).write_text('int value() { return 1; }\n')
            for name in ('api/direct.pb.h', 'api/relative.pb.h', 'api/absolute.pb.h',
                         'stage.h', 'stage.tokens', 'consumer.o'):
                (build / name).write_text('// generated fixture\n')

            def ninja_path(path):
                return str(path.as_posix()).replace('$', '$$').replace(' ', '$ ').replace(':', '$:')

            proto = ninja_path(root / 'api/schema.proto')
            tokens = ninja_path(build / 'stage.tokens')
            (build / 'build.ninja').write_text(
                'rule gen\n  command = true\n'
                f'build api/direct.pb.h: gen {proto}\n'
                f'build stage.h: gen {proto}\n'
                f'build stage.tokens: gen {proto}\n'
                'build api/relative.pb.h: gen stage.h\n'
                f'build api/absolute.pb.h: gen {tokens}\n'
                'build consumer.o: gen api/relative.pb.h\n')
            manifest = freeze(build, parent / 'frozen', {})
            recipes = capture_recipes(build, root, manifest['files'])
            dependencies = {entry['path']: entry['generator_inputs'] for entry in manifest['files']}
            self.assertEqual(dependencies['@build/api/direct.pb.h'], ['api/schema.proto'])
            self.assertEqual(dependencies['@build/api/relative.pb.h'],
                             ['@build/stage.h', 'api/schema.proto'])
            self.assertEqual(dependencies['@build/api/absolute.pb.h'],
                             ['@build/stage.tokens', 'api/schema.proto'])
            self.assertNotIn('@build/stage.tokens', dependencies)
            self.assertEqual(recipes['@build/stage.tokens']['query_exit'], 0)
            self.assertNotIn('@build/consumer.o', dependencies['@build/api/relative.pb.h'])

            def git(*args):
                return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()

            git('init', '--quiet')
            git('add', '.')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                'commit', '-qm', 'test generator fixture')
            base = git('rev-parse', 'HEAD')
            graph = {'provenance': {'upstream_pin': base, 'source_commit': base,
                                   'generated_input_dependencies': dependencies},
                     'functions': {}, 'translation_units': []}
            units = []
            for name in ('direct', 'relative', 'absolute'):
                path = name + '.cpp'
                graph['functions'][name] = {'name': 'value', 'file': path, 'start': 1, 'end': 1}
                graph['translation_units'].append({'file': path,
                    'includes': ['@build/api/' + name + '.pb.h']})
                units.append({'id': name, 'status': 'partial', 'functions': [name], 'upstream': [path]})
            (root / 'api/schema.proto').write_text('second\n')
            git('add', '.')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                'commit', '-qm', 'test changed generator input')
            original = drift_report.PIN
            try:
                drift_report.PIN = base
                result = drift_report.report(root, graph, units)
                self.assertEqual(result['affected'], [])
                self.assertEqual(result['affected_units'], ['absolute', 'direct', 'relative'])
                self.assertEqual([review['changed_inputs'] for review in result['dependency_reviews']],
                                 [['api/schema.proto']] * 3)
            finally:
                drift_report.PIN = original

    def test_generated_header_recipe_changes_reach_tracked_owners_transitively(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'api').mkdir()
            (root/'a.cpp').write_text('int value() { return 1; }\n')
            (root/'api/schema.proto').write_text('first\n')
            def git(*args):
                return subprocess.check_output(['git','-C',str(root),*args],text=True).strip()
            git('init','--quiet');git('add','.')
            git('-c','user.name=Test','-c','user.email=test@example.invalid','commit','-qm','test generator fixture')
            base=git('rev-parse','HEAD')
            graph={'provenance':{'upstream_pin':base,'source_commit':base,'generated_input_dependencies':{
                '@build/api/schema.pb.h':['api/schema.proto','@build/cycle.h'],
                '@build/cycle.h':['@build/api/schema.pb.h']}},
                'functions':{'f':{'name':'value','file':'a.cpp','start':1,'end':1}},
                'translation_units':[{'file':'a.cpp','includes':['@build/api/schema.pb.h']}]}
            units=[{'id':'tracked','status':'partial','functions':['f'],'upstream':['a.cpp']}]
            (root/'api/schema.proto').write_text('second\n');git('add','.')
            git('-c','user.name=Test','-c','user.email=test@example.invalid','commit','-qm','test changed generator input')
            original=drift_report.PIN
            try:
                drift_report.PIN=base;result=drift_report.report(root,graph,units)
                self.assertEqual(result['affected'],[])
                self.assertEqual(result['affected_units'],['tracked'])
                self.assertEqual(result['dependency_reviews'][0]['changed_inputs'],['api/schema.proto'])
            finally:drift_report.PIN=original

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
                graph['functions']['generated'] = {'file': '@build/lexer.cpp', 'start': 1, 'end': 1, 'name': 'generated'}
                graph['provenance']['generated_input_dependencies'] = {'@build/lexer.cpp': ['sub/../b.h']}
                generated_unit = {'id': 'generated', 'status': 'partial', 'functions': ['generated'], 'upstream': ['@build/lexer.cpp']}
                generated_report = drift_report.report(root, graph, units + [generated_unit])
                self.assertTrue(any(r['unit'] == 'generated' and r['changed_inputs'] == ['b.h']
                                    for r in generated_report['dependency_reviews']))
                units[0]["status"] = "todo"
                self.assertFalse(drift_report.report(root, graph, units)["affected"])
                units[0]["status"] = "partial"
                (root / "a.cpp").unlink()
                git("add", "a.cpp")
                git("commit", "-qm", "test fixture removed")
                self.assertTrue(any(a.get("function") == "a" for a in drift_report.report(root, graph, units)["affected"]))
                units.append({"id": "declarations", "status": "partial", "functions": [], "upstream": ["a.cpp"]})
                self.assertIn("declarations", drift_report.report(root, graph, units)["affected_units"])
            finally:
                drift_report.PIN = original


@unittest.skipUnless(importlib.util.find_spec("clang"), "matching libclang Python bindings unavailable")
class ClangIntegrationTests(unittest.TestCase):
    def test_virtual_conversion_operators_include_unused_overrides(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);source=root/'conversion.cpp'
            source.write_text('''
struct Base { virtual operator int() { return 0; } };
struct Derived : Base { operator int() override { return 1; } };
struct Unused : Derived { operator int() override { return 2; } };
int dispatch(Base& value) { return value; }
''',encoding='utf-8')
            result=extract_translation_unit(dict(root=tmp,old_root=tmp,file=str(source),directory=tmp,
                arguments=['clang++','-std=c++20','-c',str(source)],libclang='/usr/lib/llvm-21/lib/libclang-21.so.1'))
            self.assertEqual(result['status'],'ok',result['diagnostics'])
            names={u:f['name'] for u,f in result['functions'].items()}
            targets={names.get(b,b) for a,b,_ in expand_virtual(result['calls'],result['overrides'])
                     if names.get(a,'').startswith('dispatch(')}
            self.assertEqual(targets,{'Base::operator int()','Derived::operator int()','Unused::operator int()'})
            self.assertTrue(all(f['virtual'] for f in result['functions'].values() if 'operator int' in f['name']))

    def test_destructor_override_delete_dispatch_and_lifetime_limits(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'lifetime.cpp'
            source.write_text('''
void cleanup() {}
struct Base { virtual ~Base() {} };
struct Left : Base { ~Left() override { cleanup(); } };
struct Leaf : Left { ~Leaf() override {} };
struct Unused : Base { ~Unused() override {} };
struct Implicit : Base {};
void destroy(Base* b) { delete b; }
void explicit_destroy(Base* b) { b->~Base(); }
using Ptr=Base*;
using NestedPtr=Ptr;
void alias_destroy(Ptr b) { delete b; }
void chained_destroy(NestedPtr b) { delete b; }
void array_destroy(Left* b) { delete[] b; }
void local(Left value) { Leaf leaf; Implicit implicit; }
Leaf make();
void temporary() { make(); }
''', encoding='utf-8')
            result = extract_translation_unit(dict(root=tmp, old_root=tmp, file=str(source), directory=tmp,
                arguments=['clang++', '-std=c++20', '-c', str(source)],
                libclang='/usr/lib/llvm-21/lib/libclang-21.so.1'))
            self.assertEqual(result['status'], 'ok', result['diagnostics'])
            names = {u:f['name'] for u,f in result['functions'].items()}
            edges = [(names.get(a,a), names.get(b,b), k) for a,b,k in expand_virtual(result['calls'],result['overrides'])]
            for caller in ['destroy(Base *)', 'explicit_destroy(Base *)','alias_destroy(Ptr)','chained_destroy(NestedPtr)']:
                self.assertEqual({b for a,b,k in edges if a==caller},
                    {'Base::~Base()', 'Left::~Left()', 'Leaf::~Leaf()', 'Unused::~Unused()'})
            self.assertIn(('Left::~Left()', 'Base::~Base()', 'static'), edges)
            self.assertIn(('Leaf::~Leaf()', 'Left::~Left()', 'static'), edges)
            self.assertIn(('local(Left)', 'Left::~Left()', 'static'), edges)
            self.assertIn(('local(Left)', 'Leaf::~Leaf()', 'static'), edges)
            self.assertTrue(all(f['virtual'] for f in result['functions'].values() if '::~' in f['name']))
            self.assertTrue(any('implicit destructor' in c['expression'] for c in result['unresolved']))
            self.assertTrue(any('temporary lifetime' in c['expression'] for c in result['unresolved']))
            self.assertTrue(any('deallocation function selection' in c['expression'] for c in result['unresolved']))

    def test_canonical_alias_nested_template_and_recursive_record_wx(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'aliases.cpp'
            source.write_text('''
struct wxFrame {};
struct wxString {};
using Widget = wxFrame;
typedef Widget Window;
template<class K, class V> struct Map { K key; V value; };
using NETNAMES_MAP = Map<wxString, int>;
using Widgets = Map<int, Map<int, Window*>>;
struct Node { Node* next; Widget* widget; int get() { return 1; } };
struct Owner { Widget* widget; int get() { return 2; } };
union UnionOwner { Widget* widget; int number; int get() { return 2; } };
template<class T> struct Partial;
template<class T> struct Partial<T*> { Widget* widget; int get() { return 2; } };
struct Hidden { Widget* widget; };
template<class T> struct BaseOnly : Hidden {};
BaseOnly<double> instantiated;
int base_complete(BaseOnly<double>* value) { return 0; }
template<class T> struct Fixed { Widget* widget; };
int primary_pointer(Fixed<int>* value) { return 0; }
int partial_pointer(Partial<int*>* value) { return 0; }
template<class T> struct Generic { T value; };
int plain_storage(Generic<int>* value) { return 0; }
int direct(Window* value) { return 0; }
int nested(Widgets const& value) { return 0; }
NETNAMES_MAP const& names(NETNAMES_MAP const& value) { return value; }
int recursive(Node const* value) { return 0; }
namespace Plain { struct Value {}; using Alias = Value; int get(Alias*) { return 1; } }
namespace Gui { using Alias = wxFrame; int get(Alias*) { return 1; } }
''', encoding='utf-8')
            result = extract_translation_unit(dict(root=tmp, old_root=tmp, file=str(source), directory=tmp,
                arguments=['clang++', '-std=c++20', '-c', str(source)],
                libclang='/usr/lib/llvm-21/lib/libclang-21.so.1'))
            self.assertEqual(result['status'], 'ok', result['diagnostics'])
            for f in result['functions'].values():
                if f['name'].startswith(('Plain::','plain_storage(')):
                    self.assertEqual(f['wx'], [])
                    continue
                self.assertIn('wxString' if f['name'].startswith('names(') else 'wxFrame', f['wx'], f)
            classify(result['functions'], expand_virtual(result['calls'], result['overrides']))
            self.assertEqual(next(f['wx_class'] for f in result['functions'].values()
                                  if f['name'].startswith('names(')), 'thin-replacement')

    def test_generated_lexer_definition_ownership_outside_source_root(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root, build = base/'src', base/'build'
            root.mkdir(); build.mkdir()
            (root/'caller.h').write_text('inline int caller() { return 3; }\n')
            (build/'lexer.h').write_text('struct Lexer { Lexer(); const char* TokenName(); };\n')
            source = build/'lexer.cpp'
            source.write_text('#include "lexer.h"\n#include "caller.h"\nLexer::Lexer() { caller(); }\nconst char* Lexer::TokenName() { return "token"; }\n')
            result = extract_translation_unit(dict(root=str(root), old_root=str(root), build_root=str(build),
                file=str(source), directory=str(build), arguments=['clang++','-I'+str(root),'-c',str(source)],
                libclang='/usr/lib/llvm-21/lib/libclang-21.so.1'))
            self.assertEqual(result['status'], 'ok', result['diagnostics'])
            self.assertEqual(result['file'], '@build/lexer.cpp')
            self.assertEqual({f['file'] for f in result['functions'].values()}, {'caller.h','@build/lexer.cpp'})
            self.assertIn('@build/lexer.h', result['includes'])
            names = {u:f['name'] for u,f in result['functions'].items()}
            self.assertIn(('Lexer::Lexer()', 'caller()'), [(names[a],names[b]) for a,b,_ in result['calls']])
            output = root/'port/extractor/generated'
            freeze(build, output, {'source_commit': 'a' * 40})
            manifest = output/'source-manifest.json'
            write_json(manifest, {'source_commit': 'a' * 40, 'upstream_pin': PIN, 'files': ['caller.h']})
            database = base/'compile_commands.json'
            write_json(database, [dict(file=str(source), directory=str(build),
                arguments=['clang++','-I'+str(root),'-c',str(source)])])
            tools = Path(__file__).resolve().parents[1]/'tools'
            command = [sys.executable, str(tools/'extract_graph.py'), '--compile-commands', str(database),
                '--source-root', str(root), '--original-source-root', str(root), '--manifest', str(manifest),
                '--generated-manifest', str(output/'generated-sources.json'), '--cache', str(base/'cache'),
                '--output', str(output), '--jobs', '1']
            subprocess.run(command, check=True, capture_output=True)
            full_graph = read_json(output/'graph.json')
            self.assertEqual({f['file'] for f in full_graph['functions'].values()}, {'caller.h','@build/lexer.cpp'})
            subprocess.run([sys.executable, str(tools/'split_units.py'), '--graph', str(output/'graph.json'),
                '--inventory', str(output/'inventory.json'), '--units-dir', str(root/'port/units'),
                '--report', str(output/'units.json'), '--source-root', str(root)], check=True, capture_output=True)
            self.assertEqual(validate(root), [])

    def test_macro_only_generated_includes_are_frozen_and_invalidate_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            base=Path(tmp);root=base/'repo';root.mkdir();build=base/'build';build.mkdir()
            out=root/'port/extractor/generated';freeze(build,out,{'source_commit':'a'*40})
            source=root/'entry.cpp'
            source.write_text('#include "late.h"\nint first(){return 1;}\nint second(){return 2;}\nint entry(){return TARGET();}\n')
            (build/'late.h').write_text('#define TARGET first\n')
            write_json(out/'source-manifest.json',{'source_commit':'a'*40,'upstream_pin':PIN,'files':['entry.cpp']})
            db=base/'compile_commands.json';write_json(db,[dict(file=str(source),directory=str(root),
                arguments=['clang++','-I'+str(build),'-c',str(source)])])
            tools=Path(__file__).resolve().parents[1]/'tools'
            def extract(cache,success=True):
                command=[sys.executable,str(tools/'extract_graph.py'),'--compile-commands',str(db),'--source-root',str(root),
                    '--original-source-root',str(root),'--manifest',str(out/'source-manifest.json'),
                    '--generated-manifest',str(out/'generated-sources.json'),'--cache',str(cache),'--output',str(out),'--jobs','1']
                result=subprocess.run(command,capture_output=True,text=True,cwd=root)
                self.assertEqual(result.returncode==0,success,result.stdout+result.stderr)
                if not success:return result
                graph=read_json(out/'graph.json');names={u:f['name'] for u,f in graph['functions'].items()}
                targets={names.get(b,b) for a,b,k in graph['edges'] if names.get(a)=='entry()'}
                self.assertIn('@build/late.h',graph['translation_units'][0]['includes'])
                return graph,targets
            failed=extract(base/'cache',False)
            self.assertIn('Frozen build file inventory',failed.stderr)
            freeze(build,out,{'source_commit':'a'*40})
            first,targets=extract(base/'cache');self.assertEqual(targets,{'first()'})
            (build/'late.h').write_text('#define TARGET second\n')
            failed=extract(base/'cache',False);self.assertIn('Frozen build file inventory',failed.stderr)
            freeze(build,out,{'source_commit':'a'*40})
            after,targets=extract(base/'cache');self.assertEqual(targets,{'second()'})
            fresh,fresh_targets=extract(base/'fresh-cache');self.assertEqual(targets,fresh_targets)
            hit,hit_targets=extract(base/'cache');self.assertEqual(targets,hit_targets)
            self.assertEqual(hit['provenance']['cache_reused_commands'],1)
            subprocess.run([sys.executable,str(tools/'split_units.py'),'--graph',str(out/'graph.json'),
                '--inventory',str(out/'inventory.json'),'--units-dir',str(root/'port/units'),'--report',str(out/'units.json'),
                '--source-root',str(root)],check=True,capture_output=True,cwd=root)
            self.assertEqual(validate(root),[])
            # Unusual include suffixes must also get a stored source mapping.
            (build/'extra.data').write_text('#define EXTRA 1\n')
            source.write_text('#include "late.h"\n#include "extra.data"\nint first(){return 1;}\nint second(){return 2;}\nint entry(){return TARGET()+EXTRA;}\n')
            freeze(build,out,{'source_commit':'a'*40})
            failed=extract(base/'cache',False);self.assertIn('Generated includes lack frozen source mappings',failed.stderr)
            freeze(build,out,{'source_commit':'a'*40},['extra.data'])
            graph,_=extract(base/'cache')
            self.assertIn('@build/extra.data',graph['translation_units'][0]['includes'])

    def test_include_aliases_and_constrained_auto_extents_have_real_owned_ranges(self):
        from extractor.clang_graph import extract_translation_unit
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "sub").mkdir()
            (root / "value.h").write_text('inline int value() { return 3; }\n')
            source = root / "ranges.cpp"
            source.write_text("""
#include "sub/../value.h"
template<class T> concept Number = requires(T n) { n + n; };
struct C {
    static constexpr auto twice(Number auto x) { return x + x; }
};
int call() { return value() + C::twice(3); }
""", encoding="utf-8")
            result = extract_translation_unit({"root": str(root), "old_root": str(root), "file": str(source),
                "arguments": ["clang++", "-std=c++20", "-c", str(source)], "directory": str(root),
                "libclang": "/usr/lib/llvm-21/lib/libclang-21.so.1"})
            self.assertEqual(result["status"], "ok", result["diagnostics"])
            self.assertIn("value.h", result["includes"])
            self.assertTrue(any(f["file"] == "value.h" for f in result["functions"].values()))
            for fn in result["functions"].values():
                self.assertNotIn("..", fn["file"])
                self.assertGreater(fn["start"], 0)
                self.assertGreater(fn["end_offset"], fn["start_offset"])
                self.assertIn(fn["range_kind"], ("function", "enclosing-declaration"))

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
