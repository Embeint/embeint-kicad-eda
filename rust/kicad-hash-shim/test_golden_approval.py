# SPDX-License-Identifier: GPL-3.0-or-later
import copy
import contextlib
import io
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import approve_golden as guard  # noqa: E402
import check_reference_bindings as bindings  # noqa: E402


class ApprovalTest(unittest.TestCase):
    def setUp(self):
        line = lambda c, v: json.dumps({"case": c, "unit": guard.UNIT, "result": v})
        self.a, self.b, self.c = line("a", 1), line("b", 2), line("c", 3)
        self.new_a, self.d = line("a", 4), line("d", 5)
        self.old = "\n".join([self.a, self.b, self.c]) + "\n"
        self.cases = "\n".join(json.dumps({"case": c}) for c in ["b", "a", "d"]) + "\n"
        self.new = "\n".join([self.b, self.new_a, self.d]) + "\n"
        self.patches = mock.patch.multiple(guard, OLD=guard.digest(self.old),
                                          CASES=guard.digest(self.cases), NEW=guard.digest(self.new))
        self.patches.start()
        self.addCleanup(self.patches.stop)
        self.proposal = {"unit": guard.UNIT, "old_sha256": guard.OLD,
                         "cases_sha256": guard.CASES, "new_sha256": guard.NEW,
                         "changes": [
                             {"case": "b", "old_index": 1, "new_index": 0, "old": self.b, "new": self.b},
                             {"case": "a", "old_index": 0, "new_index": 1, "old": self.a, "new": self.new_a},
                             {"case": "d", "old_index": None, "new_index": 2, "old": None, "new": self.d},
                             {"case": "c", "old_index": 2, "new_index": None, "old": self.c, "new": None},
                         ]}

    def check(self, proposal):
        return guard.replacement(proposal, self.old, self.cases)

    def test_exact_add_remove_move_and_changed_value(self):
        self.assertEqual(self.check(self.proposal), self.new)

    def test_consolidated_baseline_must_really_be_empty(self):
        proposal = copy.deepcopy(self.proposal)
        proposal["old_sha256"] = guard.EMPTY
        proposal["changes"] = [
            {"case": c, "old_index": None, "new_index": i, "old": None, "new": line}
            for i, (c, line) in enumerate(zip(["b", "a", "d"], self.new.splitlines()))
        ]
        self.assertEqual(guard.replacement(proposal, "", self.cases, base_empty=True), self.new)
        with self.assertRaises(ValueError):
            guard.replacement(proposal, self.old, self.cases, base_empty=True)

    def test_metadata_hashes_do_not_authorize_changed_fields_or_case_ids(self):
        for key, value in [("result", 999), ("case", "unapproved")]:
            proposal = copy.deepcopy(self.proposal)
            line = json.loads(proposal["changes"][1]["new"])
            line[key] = value
            proposal["changes"][1]["new"] = json.dumps(line)
            with self.assertRaises(ValueError):
                self.check(proposal)

    def test_original_bytes_and_typed_positions_are_bound(self):
        for key, value in [("old", self.b), ("old_index", False), ("new_index", 1.0),
                           ("new", self.new_a + "\n")]:
            proposal = copy.deepcopy(self.proposal)
            proposal["changes"][1][key] = value
            with self.assertRaises(ValueError):
                self.check(proposal)

    def test_missing_or_duplicate_movements_and_removals_fail(self):
        for index in [0, 3]:
            proposal = copy.deepcopy(self.proposal)
            proposal["changes"].pop(index)
            with self.assertRaises(ValueError):
                self.check(proposal)
        proposal = copy.deepcopy(self.proposal)
        proposal["changes"].append(proposal["changes"][0])
        with self.assertRaises(ValueError):
            self.check(proposal)


class BindingIdentityTest(unittest.TestCase):
    def test_unchanged_arithmetic_cannot_qualify_changed_object_bytes(self):
        binding = json.loads(bindings.DEFAULT_BINDING.read_bytes())
        for actual, expected_status in [(binding["bezier"]["object_sha256"], 0), ("0" * 64, 1)]:
            def hashes(path):
                return actual if path == "adapter.o" else binding["locale"]["runtimes"][0]["sha256"]

            def symbols(command):
                return "U _ZTVNSt7__cxx118numpunctIcEE" if "--undefined-only" in command else ""

            with mock.patch.object(sys, "argv", ["binding-check", "--object", "adapter.o",
                                                 "--oracle", "oracle", "--libstdcxx", "runtime"]), \
                    mock.patch.object(bindings, "site_signature", return_value=binding["bezier"]["sites"]), \
                    mock.patch.object(bindings, "sha256", side_effect=hashes), \
                    mock.patch.object(bindings, "run", side_effect=symbols), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(bindings.main(), expected_status)


if __name__ == "__main__":
    unittest.main()
