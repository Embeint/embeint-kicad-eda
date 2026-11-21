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

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("golden_policy", ROOT / "port/tools/check_goldens_policy.py")
POLICY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = POLICY
SPEC.loader.exec_module(POLICY)


class PolicyTests(unittest.TestCase):
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
