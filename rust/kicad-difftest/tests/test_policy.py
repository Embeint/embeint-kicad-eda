# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression checks for non-vacuous staging and golden review evidence."""
import importlib.util
import gzip
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("golden_policy", ROOT / "port/tools/check_goldens_policy.py")
POLICY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = POLICY
SPEC.loader.exec_module(POLICY)


class PolicyTests(unittest.TestCase):
    def test_jsonl_preserves_unicode_line_separators_inside_strings(self):
        for character in ("\u0085", "\u2028", "\u2029"):
            with self.subTest(character=repr(character)):
                record = {"unit": "u", "case": "a", "result": "before" + character + "after"}
                literal = json.dumps(record, ensure_ascii=False).encode("utf-8")
                escaped = json.dumps(record, ensure_ascii=True).encode("utf-8")
                expected = POLICY.replies(escaped + b"\n", "u")[0][2]
                for ending in (b"", b"\n", b"\r\n"):
                    self.assertEqual(POLICY.replies(literal + ending, "u")[0][2], expected)
        with self.assertRaises(ValueError):
            POLICY.replies(b'{"unit":"u","case":"a","result":0}\n\n', "u")

    def test_strict_gate_counts_only_live_green_layer_zero_units(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            validator = root / "port/tools/validate_units.py"
            validator.parent.mkdir(parents=True)
            validator.write_text("", encoding="utf-8")
            units, cases = root / "port/units", root / "port/cases"
            units.mkdir()
            cases.mkdir()
            for layer, expected in ((1, "INCOMPLETE"), (0, "COMPLETE")):
                for uid in ("a", "b", "c"):
                    (units / f"{uid}.toml").write_text(
                        f'id = "{uid}"\nstatus = "green"\nlayer = {layer}\n'
                        '[difftest]\nrust_package = "shim"\nrust_binary = "shim"\n', encoding="utf-8")
                    (cases / f"{uid}.jsonl").write_text(
                        json.dumps({"unit": uid, "case": "a", "input": {}}) + "\n", encoding="utf-8")
                with mock.patch.object(POLICY.subprocess, "run"):
                    if layer:
                        with self.assertRaisesRegex(ValueError, "layer-0"):
                            POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", True)
                    else:
                        POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", True)
                report = json.loads((root / "reports/phase0.json").read_text(encoding="utf-8"))
                self.assertEqual(report["phase0"], expected)
                self.assertEqual(report["total_live_green_units"], 3)
                self.assertEqual(report["live_green_units"], 0 if layer else 3)

    def test_evidence_checks_committed_head_and_ignores_dirty_worktree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root).decode().strip()

            def commit():
                if (root / "port").exists():
                    git("add", "--", "port/goldens/u.jsonl", "port/cases/u.jsonl",
                        "port/goldens/evidence/u-baseline.json")
                git("commit", "--quiet", "--allow-empty", "-m",
                    "test(chore): record the golden policy fixture\n\n"
                    "Provide committed bytes for the policy boundary regression.\n\n"
                    "Assisted-by: Codex\nSigned-off-by: Karl Von Richter <karl@embeint.com>")

            def write(name, data):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)

            def evidence(new, cases):
                return json.dumps({"format": 1, "unit": "u", "reason": "fixture oracle baseline",
                    "changes": POLICY.changes([], POLICY.replies(new, "u")),
                    "cases_sha256": POLICY.sha(cases), "old_sha256": POLICY.sha(b""),
                    "new_sha256": POLICY.sha(new), "oracle": {"provenance": {"pin": "fixture"},
                        "oracle_sha256": "a" * 64, "rust_sha256": "b" * 64}}).encode()

            git("init", "--quiet")
            git("config", "user.name", "Karl Von Richter")
            git("config", "user.email", "karl@embeint.com")
            git("config", "core.autocrlf", "false")
            commit()
            base = git("rev-parse", "HEAD")
            cases = b'{"unit":"u","case":"a","input":{}}\n'
            first = b'{"unit":"u","case":"a","result":1}\n'
            second = b'{"unit":"u","case":"a","result":2}\n'
            golden_path, case_path = "port/goldens/u.jsonl", "port/cases/u.jsonl"
            evidence_path = "port/goldens/evidence/u-baseline.json"
            write(golden_path, first)
            write(case_path, cases)
            write(evidence_path, evidence(first, cases))
            commit()
            for path in (golden_path, case_path, evidence_path):
                write(path, b"dirty bytes that must not be validated")
            POLICY.check_changes(root, base)
            write(golden_path, first)
            write(case_path, cases)
            write(evidence_path, evidence(second, cases))
            commit()
            write(golden_path, second)
            with self.assertRaisesRegex(ValueError, "stale evidence new_sha256"):
                POLICY.check_changes(root, base)

    def test_empty_infrastructure_is_incomplete_but_strict_gate_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = [sys.executable, str(ROOT / "port/tools/check_goldens_policy.py"),
                    "--root", directory, "--ci", "--runner", "unused", "--oracle", "unused",
                    "--reports", str(root / "reports")]
            result = subprocess.run(args, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((root / "reports/phase0.json").read_text())
            self.assertEqual(report["phase0"], "INCOMPLETE")
            self.assertEqual(report["live_green_units"], 0)
            generated = root / "port/extractor/generated"
            generated.mkdir(parents=True)
            with gzip.open(generated / "graph.json.gz", "wt", encoding="utf-8") as stream:
                json.dump({"provenance": {"all_configurations_complete": False,
                                         "gate_complete": False}}, stream)
            result = subprocess.run(args, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((root / "reports/phase0.json").read_text())
            self.assertIs(report["extraction"]["all_configurations_complete"], False)
            self.assertEqual(report["phase0"], "INCOMPLETE")
            result = subprocess.run(args + ["--require-phase0"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)

    def test_all_new_portable_crates_are_discovered_by_default(self):
        names = ["embeint-layout", "embeint-web", "kicad-new-pilot", "kicad-difftest", "native-shim"]
        packages = [{"id": name, "name": name, "metadata": {}} for name in names]
        packages[0]["metadata"] = None
        packages[-1]["metadata"] = {"kicad": {"portable": False}}
        metadata = {"workspace_members": names, "packages": packages}
        self.assertEqual(POLICY.portable_packages(metadata), names[:3])

    def test_policy_retains_numeric_tokens_and_rejects_duplicate_json(self):
        self.assertNotEqual(POLICY.exact("9007199254740993"), POLICY.exact("9007199254740992"))
        self.assertNotEqual(POLICY.exact("1"), POLICY.exact("1.0"))
        self.assertNotEqual(POLICY.exact("-0"), POLICY.exact("0"))
        self.assertNotEqual(POLICY.exact("1"), POLICY.exact('"1"'))
        for text in ('{"a":1,"a":2}', "NaN", "Infinity"):
            with self.assertRaises(ValueError):
                POLICY.exact(text)

    def test_baseline_evidence_cannot_omit_cases_values_or_oracle(self):
        new = b'{"unit":"u","case":"a","result":9007199254740993}\n'
        case = b'{"unit":"u","case":"a","input":{}}\n'
        evidence = {"format": 1, "unit": "u", "reason": "first pinned baseline",
                    "changes": [{"case": "a", "old_index": None, "new_index": 0,
                                 "old": None, "new": new.decode().strip()}],
                    "old_sha256": POLICY.sha(b""), "new_sha256": POLICY.sha(new),
                    "cases_sha256": POLICY.sha(case),
                    "oracle": {"provenance": {"kicad_pin": "pin", "compiler": "test"},
                               "oracle_sha256": "a" * 64, "rust_sha256": "b" * 64}}
        POLICY.verify_evidence(evidence, "u", b"", new, case)
        for key, bad in [("changes", []), ("reason", ""), ("oracle", {}), ("new_sha256", "0" * 64)]:
            broken = dict(evidence, **{key: bad})
            with self.assertRaises(ValueError):
                POLICY.verify_evidence(broken, "u", b"", new, case)
        with self.assertRaises(ValueError):
            POLICY.verify_evidence(evidence, "u", b"", new.replace(b"993", b"992"), case)


if __name__ == "__main__":
    unittest.main()
