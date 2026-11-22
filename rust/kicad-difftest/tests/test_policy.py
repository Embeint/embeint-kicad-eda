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
RUNNER = ROOT / "rust/target/debug" / ("kicad-difftest.exe" if sys.platform == "win32" else "kicad-difftest")


class FakeRunner:
    """Stands in for cargo and kicad-difftest in ci(), writing the runner's report format.

    behaviour maps a unit to {"defer": {case: owner}, "returncode": n, "mutate": f}.
    A unit's deferrals are honoured only with --partial, as in the real runner, where
    a strict run with any deferral, or an all-deferred run, exits nonzero with status
    "deferred". An explicit returncode and mutate simulate a faulty or dishonest runner.
    """

    def __init__(self, root, behaviour=None):
        self.root, self.behaviour, self.calls = root, behaviour or {}, []

    def __call__(self, args, **kwargs):
        if args[0] != "runner":
            return subprocess.CompletedProcess(args, 0)
        action, partial = args[1], "--partial" in args
        uid = args[args.index("--unit") + 1]
        report_path = Path(args[args.index("--report") + 1])
        self.calls.append((uid, action, partial))
        behaviour = self.behaviour.get(uid, {})
        lines = (self.root / "port/cases" / f"{uid}.jsonl").read_text(encoding="utf-8").splitlines()
        cases = [json.loads(line)["case"] for line in lines]
        defer = behaviour.get("defer", {})
        compared = [case for case in cases if case not in defer]
        deferred = [{"case": case, "upstream": "Callee::Fn", "unit": defer[case]}
                    for case in cases if case in defer]
        status, returncode = "partial" if deferred else "green", 0
        if deferred and not partial or not compared:
            status, returncode = "deferred", 1
        returncode = behaviour.get("returncode", returncode)
        report = {"unit": uid, "status": status, "action": action,
                  "mode": "partial" if partial else "strict", "cases": len(cases),
                  "compared": len(compared), "deferred": len(deferred), "failures": 0,
                  "golden_checked": action == "check", "compared_cases": compared,
                  "deferred_cases": deferred}
        if "mutate" in behaviour:
            behaviour["mutate"](report)
        report_path.write_text(json.dumps(report), encoding="utf-8")
        return subprocess.CompletedProcess(args, returncode)


def write_unit(root, uid, status, layer=0, cases=("a",)):
    units, corpus = root / "port/units", root / "port/cases"
    units.mkdir(parents=True, exist_ok=True)
    corpus.mkdir(parents=True, exist_ok=True)
    (units / f"{uid}.toml").write_text(
        f'id = "{uid}"\nstatus = "{status}"\nlayer = {layer}\n'
        '[difftest]\nrust_package = "shim"\nrust_binary = "shim"\n', encoding="utf-8")
    (corpus / f"{uid}.jsonl").write_text("".join(
        json.dumps({"unit": uid, "case": case, "input": {}}) + "\n" for case in cases), encoding="utf-8")


def policy_root(directory):
    root = Path(directory)
    validator = root / "port/tools/validate_units.py"
    validator.parent.mkdir(parents=True)
    validator.write_text("", encoding="utf-8")
    return root


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
                runner = FakeRunner(root)
                with mock.patch.object(POLICY.subprocess, "run", side_effect=runner):
                    if layer:
                        with self.assertRaisesRegex(ValueError, "layer-0"):
                            POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", True)
                    else:
                        POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", True)
                report = json.loads((root / "reports/phase0.json").read_text(encoding="utf-8"))
                self.assertEqual(report["phase0"], expected)
                self.assertEqual(report["total_live_green_units"], 3)
                self.assertEqual(report["live_green_units"], 0 if layer else 3)
                self.assertEqual(runner.calls, [(uid, "check", False) for uid in ("a", "b", "c")])

    def run_ci(self, root, behaviour=None, required=False):
        runner = FakeRunner(root, behaviour)
        with mock.patch.object(POLICY.subprocess, "run", side_effect=runner):
            POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", required)
        return runner, json.loads((root / "reports/phase0.json").read_text(encoding="utf-8"))

    def test_partial_mode_follows_only_the_declared_partial_record(self):
        with tempfile.TemporaryDirectory() as directory:
            root = policy_root(directory)
            for uid in ("a", "b", "c"):
                write_unit(root, uid, "green")
            write_unit(root, "p", "partial", layer=1, cases=("x", "y", "z"))
            write_unit(root, "q", "claimed", cases=("x",))
            runner, summary = self.run_ci(root, {"p": {"defer": {"y": "a"}}}, required=True)
            # Green and other statuses run strict check; only p runs --partial,
            # live-only because goldens follow a green difftest.
            self.assertEqual(sorted(runner.calls), [("a", "check", False), ("b", "check", False),
                                                    ("c", "check", False), ("p", "compare", True),
                                                    ("q", "check", False)])
            self.assertEqual(summary["phase0"], "COMPLETE")
            self.assertEqual(summary["live_green_units"], 3)
            self.assertEqual(summary["live_partial_units"], 1)
            self.assertEqual(summary["partial_deferred_cases"], 1)
            self.assertEqual(summary["partial_units_without_deferrals"], [])
            self.assertEqual(summary["units"]["p"], {
                "declared_status": "partial", "mode": "partial", "action": "compare",
                "result": "partial", "cases": 3, "compared": 2, "deferred": 1})
            self.assertEqual(summary["units"]["a"]["result"], "green")
            report = json.loads((root / "reports/p.json").read_text(encoding="utf-8"))
            self.assertEqual(report["compared_cases"], ["x", "z"])
            self.assertEqual(report["deferred_cases"],
                             [{"case": "y", "upstream": "Callee::Fn", "unit": "a"}])
            # A partial unit with a golden is checked against all its reference replies.
            golden = root / "port/goldens/p.jsonl"
            golden.parent.mkdir(parents=True, exist_ok=True)
            golden.write_text("", encoding="utf-8")
            runner, _ = self.run_ci(root, {"p": {"defer": {"y": "a"}}})
            self.assertIn(("p", "check", True), runner.calls)

    def test_partial_units_never_count_towards_the_strict_phase0_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = policy_root(directory)
            for uid in ("a", "b"):
                write_unit(root, uid, "green")
            write_unit(root, "p", "partial", cases=("x", "y"))
            runner = FakeRunner(root, {"p": {"defer": {"y": "a"}}})
            with mock.patch.object(POLICY.subprocess, "run", side_effect=runner):
                with self.assertRaisesRegex(ValueError, "three live green layer-0"):
                    POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", True)
            summary = json.loads((root / "reports/phase0.json").read_text(encoding="utf-8"))
            self.assertEqual(summary["phase0"], "INCOMPLETE")
            self.assertEqual(summary["live_green_units"], 2)
            self.assertEqual(summary["live_partial_units"], 1)
            # A partial unit that defers nothing is still partial and still not counted.
            _, summary = self.run_ci(root)
            self.assertEqual(summary["partial_units_without_deferrals"], ["p"])
            self.assertEqual(summary["live_green_units"], 2)
            self.assertEqual(summary["units"]["p"]["result"], "green")
            self.assertEqual(summary["units"]["p"]["declared_status"], "partial")

    def test_green_and_undeclared_units_stay_zero_deferral(self):
        with tempfile.TemporaryDirectory() as directory:
            root = policy_root(directory)
            write_unit(root, "g", "green", cases=("x", "y"))
            write_unit(root, "o", "partial", cases=("x",))
            for status in ("green", "claimed", "todo", "blocked"):
                write_unit(root, "g", status, cases=("x", "y"))
                with self.subTest(status=status):
                    with self.assertRaisesRegex(ValueError, "strict live difftest check failed"):
                        self.run_ci(root, {"g": {"defer": {"y": "o"}}})
                    # A runner report claiming partial or deferrals for a strict run is refused.
                    for mutate in (lambda r: r.update(status="partial"),
                                   lambda r: r.update(mode="partial"),
                                   lambda r: r.update(deferred_cases=[{"case": "y", "upstream": "f",
                                                                       "unit": "o"}],
                                                      compared_cases=["x"], compared=1, deferred=1)):
                        with self.assertRaises(ValueError):
                            self.run_ci(root, {"g": {"mutate": mutate}})

    def test_partial_runs_still_reject_failures_and_dishonest_reports(self):
        bad = {
            "exit": {"defer": {"y": "a"}, "returncode": 1},
            "all deferred": {"defer": {"x": "a", "y": "a"}},
            "all deferred despite exit zero": {"defer": {"x": "a", "y": "a"}, "returncode": 0,
                                               "mutate": lambda r: r.update(status="partial")},
            "unknown owner": {"defer": {"y": "missing.unit"}},
            "failures": {"defer": {"y": "a"}, "mutate": lambda r: r.update(failures=1)},
            "failed status": {"defer": {"y": "a"}, "mutate": lambda r: r.update(status="failed")},
            "strict mode": {"defer": {"y": "a"}, "mutate": lambda r: r.update(mode="strict")},
            "wrong unit": {"mutate": lambda r: r.update(unit="a")},
            "wrong action": {"mutate": lambda r: r.update(action="check")},
            "dropped case": {"defer": {"y": "a"}, "mutate": lambda r: r.update(
                deferred_cases=[], deferred=0, status="green")},
            "duplicate case": {"mutate": lambda r: r.update(compared_cases=["x", "x", "y"], compared=3)},
            "case in both": {"defer": {"y": "a"}, "mutate": lambda r: r.update(
                compared_cases=["x", "y"], compared=2)},
            "reordered": {"mutate": lambda r: r.update(compared_cases=["y", "x"])},
            "unknown case": {"mutate": lambda r: r.update(compared_cases=["x", "w"])},
            "count": {"defer": {"y": "a"}, "mutate": lambda r: r.update(deferred=0)},
            "green with deferrals": {"defer": {"y": "a"}, "mutate": lambda r: r.update(status="green")},
            "partial without deferrals": {"mutate": lambda r: r.update(status="partial")},
            "empty upstream": {"defer": {"y": "a"}, "mutate": lambda r: r["deferred_cases"][0].update(
                upstream="")},
            "extra deferral key": {"defer": {"y": "a"}, "mutate": lambda r: r["deferred_cases"][0].update(
                note="x")},
        }
        with tempfile.TemporaryDirectory() as directory:
            root = policy_root(directory)
            write_unit(root, "a", "green")
            write_unit(root, "p", "partial", cases=("x", "y"))
            for name, behaviour in bad.items():
                with self.subTest(name=name), self.assertRaises(ValueError):
                    self.run_ci(root, {"p": behaviour})
            # A stale report from an earlier run is never reused: the first unit's
            # report is removed before its rerun, which here writes nothing.
            report = root / "reports/a.json"
            _, summary = self.run_ci(root, {"p": {"defer": {"y": "a"}}})
            self.assertEqual(summary["units"]["p"]["result"], "partial")
            self.assertTrue(report.exists())
            def no_report(args, **kwargs):
                return subprocess.CompletedProcess(args, 0)
            with mock.patch.object(POLICY.subprocess, "run", side_effect=no_report):
                with self.assertRaises(OSError):
                    POLICY.ci(root, Path("runner"), Path("oracle"), root / "reports", False)
            self.assertFalse(report.exists())

    def test_report_counts_and_golden_marker_are_exact(self):
        units = {"u": {"status": "partial"}, "owner": {"status": "todo"}}

        def base(action, mode="strict"):
            deferred = ([{"case": "b", "upstream": "F::g", "unit": "owner"}] if mode == "partial" else [])
            return {"unit": "u", "status": "partial" if deferred else "green", "action": action,
                    "mode": mode, "cases": 1 + len(deferred), "compared": 1, "deferred": len(deferred),
                    "failures": 0, "golden_checked": action == "check", "compared_cases": ["a"],
                    "deferred_cases": deferred}

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"

            def verify(text, action, mode):
                path.write_text(text, encoding="utf-8")
                cases = ["a", "b"] if mode == "partial" else ["a"]
                return POLICY.verify_report(path, "u", mode, action, cases, units)

            for action in ("check", "compare"):
                for mode in ("strict", "partial"):
                    text = json.dumps(base(action, mode))
                    with self.subTest(action=action, mode=mode, probe="control"):
                        verify(text, action, mode)
                    cases = base(action, mode)["cases"]
                    # Ordinary JSON decoding would accept every one of these as 0 or 1.
                    probes = {
                        "failures underflow": text.replace('"failures": 0', '"failures": 1e-400'),
                        "integral float count": text.replace('"compared": 1', '"compared": 1.0'),
                        "exponent count": text.replace('"compared": 1', '"compared": 1e0'),
                        "rounded case count": text.replace(
                            f'"cases": {cases}', f'"cases": {cases}.00000000000000000000001'),
                        "negative zero failures": text.replace('"failures": 0', '"failures": -0'),
                        "negative float zero": text.replace('"failures": 0', '"failures": -0.0'),
                        "negative count": text.replace('"failures": 0', '"failures": -1'),
                        "NaN extra field": text[:-1] + ', "oracle_sha256": NaN}',
                        "Infinity extra field": text[:-1] + ', "x": Infinity}',
                        "-Infinity count": text.replace('"failures": 0', '"failures": -Infinity'),
                        "duplicate key": text[:-1] + ', "failures": 0}',
                    }
                    for name, changes in {
                        "boolean counts": dict(cases=True, compared=True, deferred=False, failures=False),
                        "boolean failures": dict(failures=False),
                        "string count": dict(compared="1"),
                        "null count": dict(failures=None),
                        "missing failures": dict(failures="<drop>"),
                        "missing golden_checked": dict(golden_checked="<drop>"),
                        "golden_checked string": dict(golden_checked=str(action == "check").lower()),
                        "golden_checked integer": dict(golden_checked=int(action == "check")),
                        "golden_checked null": dict(golden_checked=None),
                        "golden_checked contradicts action": dict(golden_checked=action != "check"),
                    }.items():
                        value = base(action, mode)
                        for key, new in changes.items():
                            if new == "<drop>":
                                value.pop(key)
                            else:
                                value[key] = new
                        probes[name] = json.dumps(value)
                    for name, probe in probes.items():
                        with self.subTest(action=action, mode=mode, probe=name):
                            self.assertNotEqual(probe, text, name)
                            with self.assertRaises(ValueError):
                                verify(probe, action, mode)

    @unittest.skipUnless(RUNNER.exists(), "build kicad-difftest first (CI builds it before this suite)")
    def test_real_runner_reports_satisfy_the_policy_contract(self):
        peer = ROOT / "rust/kicad-difftest/tests/peer.py"
        units = {"p": {"status": "partial"}, "dependency": {"status": "todo"}}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_unit(root, "p", "partial", cases=("a", "b", "c"))
            report = root / "report.json"

            def run(oracle, rust, *extra):
                report.unlink(missing_ok=True)
                return subprocess.run(
                    [str(RUNNER), "compare", "--root", str(root), "--unit", "p",
                     "--oracle", sys.executable, "--oracle-arg", str(peer), "--oracle-arg", oracle,
                     "--rust", sys.executable, "--rust-arg", str(peer), "--rust-arg", rust,
                     "--report", str(report), *extra], capture_output=True, text=True)

            cases = ["a", "b", "c"]
            result = run("ok", "defer-b", "--partial")
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = POLICY.verify_report(report, "p", "partial", "compare", cases, units)
            self.assertEqual((summary["result"], summary["compared"], summary["deferred"]),
                             ("partial", 2, 1))
            result = run("ok", "ok")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(POLICY.verify_report(report, "p", "strict", "compare", cases, units)["result"],
                             "green")
            with self.assertRaises(ValueError):
                POLICY.verify_report(report, "p", "partial", "compare", cases, units)
            for oracle, rust, extra in (("ok", "defer-b", ()), ("ok", "deferred", ("--partial",)),
                                        ("ok", "changed-a,defer-b", ("--partial",))):
                result = run(oracle, rust, *extra)
                self.assertNotEqual(result.returncode, 0, rust)
                mode = "partial" if extra else "strict"
                with self.assertRaises(ValueError):
                    POLICY.verify_report(report, "p", mode, "compare", cases, units)
            # The owner must be a unit record.
            run("ok", "defer-b", "--partial")
            with self.assertRaisesRegex(ValueError, "names no unit record"):
                POLICY.verify_report(report, "p", "partial", "compare", cases, {"p": units["p"]})

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
