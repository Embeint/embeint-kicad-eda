#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The traced libm dispatch chain proof (dispatch_proof.py).

The synthetic tests run anywhere. RuntimeDispatchTest runs only inside the
pinned candidate image (it needs the locked libm and debug file, g++ and
ptrace); elsewhere it is reported as skipped, not passed.
"""

import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ENV = Path(__file__).resolve().parents[1]
for directory in ("tools", "probe"):
    sys.path.insert(0, str(ENV / directory))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import libm_fixtures as fx  # noqa: E402
import dispatch_proof  # noqa: E402
import libm_elf  # noqa: E402
import refenv  # noqa: E402

LOCK = refenv.load_lock()
CANDIDATE = refenv.load_candidate()
CHECKED = CANDIDATE["dispatch"]["checked_functions"]


class ProofTest(unittest.TestCase):
    def setUp(self):
        self.facts, records = fx.facts_and_records(LOCK, CHECKED)
        self.observation = {"libm_dispatch": records}

    def record(self, function):
        return next(r for r in self.observation["libm_dispatch"] if r["function"] == function)

    def reasons(self, function=None):
        _report, violations = dispatch_proof.dispatch_report(self.observation, self.facts, LOCK, CANDIDATE)
        if function is None:
            return violations
        return next((v["reasons"] for v in violations if v.get("function") == function), [])

    def assertFails(self, function, fragment):
        reasons = self.reasons(function)
        self.assertTrue(any(fragment in r for r in reasons), (fragment, reasons))

    def test_valid_direct_and_wrapped_paths(self):
        report, violations = dispatch_proof.dispatch_report(self.observation, self.facts, LOCK, CANDIDATE)
        self.assertEqual(violations, [])
        rows = {row["function"]: row for row in report}
        self.assertEqual(rows["sin"]["path"], "direct")
        self.assertEqual(rows["exp"]["path"], "wrapped")
        hop = rows["exp"]["hops"][0]
        self.assertEqual(hop["target_names"], ["__ieee754_exp_sse2"])
        self.assertEqual(hop["resolver_names"], ["__ieee754_exp"])
        self.assertEqual(rows["exp"]["entry_definition"]["version"], "GLIBC_2.29")
        self.assertEqual(rows["exp"]["executed_symbols"], ["__exp", "exp", "__ieee754_exp_sse2"])

    def test_wrong_target_through_slot(self):
        record = self.record("exp")
        record["trace"][4:6] = ["0x79b60", "0x79b64"]  # __ieee754_exp_fma
        self.facts["functions"].append(["0x79b60", 0x10, ["__ieee754_exp_fma"]])
        self.assertFails("exp", "did not lead to the locked SSE2 implementation")
        self.assertFails("exp", "forbidden variant executed: __ieee754_exp_fma")

    def test_wrong_direct_variant(self):
        record = self.record("sin")
        record["offset"] = record["linked_offset"] = "0x7b2d0"
        record["trace"] = ["0x7b2d0", "0x7b2d4"]
        self.assertFails("sin", "IFUNC resolved to an address other than the locked SSE2 implementation")
        self.assertFails("sin", "forbidden variant executed: __sin_fma")

    def test_other_sse2_variant_executed(self):
        record = self.record("sin")
        record["trace"].insert(2, "0x301f0")  # __cos_sse2 inside sin's trace
        self.assertFails("sin", "executed multiarch implementations differ")

    def test_wrong_object_and_binding(self):
        self.record("exp")["object"] = "/other/libm.so.6"
        self.assertFails("exp", "not in the locked libm")
        self.record("cos")["linked_offset"] = "0x10"
        self.assertFails("cos", "compiled reference and dlsym() resolve to different addresses")

    def test_compat_version_entry(self):
        record = self.record("pow")
        compat = self.facts["definitions"]["pow"][0]["value"]
        record["offset"] = record["linked_offset"] = record["trace"][0] = compat
        self.assertFails("pow", "entry is not the public wrapper definition")

    def test_no_default_definition(self):
        for row in self.facts["definitions"]["log"]:
            row["default"] = False
        self.assertFails("log", "no unique default-version public definition")
        self.facts["definitions"]["atan"][0]["type"] = "OBJECT"
        self.assertFails("atan", "neither FUNC nor IFUNC")

    def test_chain_defects(self):
        stub = next(s for s in self.facts["plt_stubs"] if s["stub"] == self.record("exp")["trace"][2])
        stub["template"] = False
        self.assertFails("exp", "not the exact .plt.sec form")
        stub["template"] = True
        stub["relocations"][0]["type"] = "R_X86_64_JUMP_SLOT"
        self.assertFails("exp", "not a single IRELATIVE relocation")
        stub["relocations"] = [dict(stub["relocations"][0], type="R_X86_64_IRELATIVE")] * 2
        self.assertFails("exp", "not a single IRELATIVE relocation")
        stub["relocations"] = stub["relocations"][:1]
        resolver = stub["relocations"][0]["addend"]
        self.facts["ifuncs"][resolver] = ["__ieee754_log"]
        self.assertFails("exp", "not the SSE2 variant of the slot's IFUNC resolver")

    def test_stub_entered_outside_wrapper_or_midway(self):
        record = self.record("atan2")
        record["trace"][1] = "0x2d840"  # caller inside __atan_sse2, not the wrapper
        self.assertFails("atan2", "not entered from the public wrapper")
        record = self.record("pow")
        del record["trace"][2]  # jump executed without the stub's start
        self.assertFails("pow", "not executed from its start through its jump")

    def test_wrapper_without_dispatch(self):
        record = self.record("log")
        record["trace"] = record["trace"][:2]
        self.assertFails("log", "returned without reaching an implementation")

    def test_trace_defects(self):
        self.record("tan")["trace"].append("!/usr/lib/x86_64-linux-gnu/libc.so.6+0x10")
        self.assertFails("tan", "execution left libm")
        self.record("atan")["trace"].append("0xdead0")
        self.assertFails("atan", "not inside a known libm function")
        self.record("sinf")["trace_status"] = "step bound exceeded"
        self.assertFails("sinf", "trace incomplete")
        self.record("cosf")["trace"] = self.record("cosf")["trace"][1:]
        self.assertFails("cosf", "does not start at the entry")
        self.record("expf")["trace"].insert(1, fx.hx(fx.PLT_START))
        self.assertFails("expf", "direct IFUNC target made a further PLT dispatch")

    def test_identity_and_record_set(self):
        self.facts["libm_sha256"] = "0" * 64
        self.facts["multiarch_symbols"] = {**LOCK["libm"]["multiarch_symbols"], "0x33320": ["__fma"]}
        problems = self.reasons()[0]["problems"]
        self.assertIn("dispatch facts were not read from the locked libm bytes", problems)
        self.assertIn("debug file multiarch implementations differ from the lock", problems)
        self.facts, records = fx.facts_and_records(LOCK, CHECKED)
        self.observation = {"libm_dispatch": records[1:] + records[:1]}
        self.assertEqual(self.reasons()[0]["reason"], "missing, extra, duplicate or reordered dispatch records")

    def test_resolve_objects_marks_unrecorded(self):
        loaded = {LOCK["libm"]["path"]: {"resolved": LOCK["libm"]["path"]}}
        observation = copy.deepcopy(self.observation)
        observation["libm_dispatch"][0]["linked_object"] = "/elsewhere"
        rows = dispatch_proof.resolve_objects(observation, loaded)["libm_dispatch"]
        self.assertEqual(rows[0]["linked_object"], "unrecorded:/elsewhere")
        self.assertEqual(rows[1]["object"], LOCK["libm"]["path"])


def in_pinned_image():
    try:
        libm = Path(LOCK["libm"]["path"])
        return (Path("/opt/refenv/inputs.lock.json").is_file() and libm.is_file()
                and hashlib.sha256(libm.read_bytes()).hexdigest() == fx.libm_sha256(LOCK))
    except OSError:
        return False


@unittest.skipUnless(in_pinned_image(), "needs the pinned candidate image (locked libm, g++, ptrace)")
class RuntimeDispatchTest(unittest.TestCase):
    """Build the primitive probe, trace the real libm and prove the chain."""

    @classmethod
    def setUpClass(cls):
        cls.directory = Path(tempfile.mkdtemp())
        binary = cls.directory / "primitive_probe"
        subprocess.run(["g++", "-O3", "-DNDEBUG", "-std=gnu++20", "-fno-fast-math", "-ffp-contract=off",
                        "-o", str(binary), str(ENV / "probe/primitive_probe.cpp")], check=True)
        cls.results = {}
        for label in ("contract", "host-default"):
            env = dict(os.environ)
            if label == "host-default":
                env.pop("GLIBC_TUNABLES", None)
            else:
                env["GLIBC_TUNABLES"] = CANDIDATE["dispatch"]["glibc_tunables"]
            out = subprocess.run([str(binary), "--functions", ",".join(CHECKED), "--sweep-count", "1",
                                  "--sweep-out", str(cls.directory / f"{label}.bin")], env=env, check=True,
                                 capture_output=True, text=True).stdout
            cls.results[label] = json.loads(out)
        cls.facts = libm_elf.dispatch_facts(Path(LOCK["libm"]["path"]).read_bytes(),
                                            Path(LOCK["libm"]["debug_file"]).read_bytes(), CHECKED)

    def report(self, label, facts=None):
        observation = self.results[label]
        loaded = {name: {"resolved": str(Path(name).resolve())} for name in observation["loaded_objects"]
                  if name.startswith("/")}
        return dispatch_proof.dispatch_report(dispatch_proof.resolve_objects(observation, loaded),
                                              facts or self.facts, LOCK, CANDIDATE)

    def test_contract_executes_locked_sse2(self):
        report, violations = self.report("contract")
        self.assertEqual(violations, [])
        rows = {row["function"]: row for row in report}
        for function in fx.WRAPPED:
            self.assertEqual(rows[function]["path"], "wrapped", function)
            self.assertTrue(rows[function]["hops"], function)
        self.assertEqual(rows["sin"]["path"], "direct")
        self.assertEqual(dispatch_proof.identity_violations(self.facts, LOCK), [])

    def test_host_default_is_detected_when_it_differs(self):
        flags = next((line for line in Path("/proc/cpuinfo").read_text().splitlines()
                      if line.startswith("flags")), "").split()
        _report, violations = self.report("host-default")
        if "fma" in flags:
            failed = {v["function"] for v in violations if "function" in v}
            self.assertTrue({"sin", "exp", "pow"}.issubset(failed), violations)
            self.assertTrue(any("forbidden variant executed" in r for v in violations
                                for r in v.get("reasons", [])))
        else:
            self.assertEqual(violations, [])

    def test_mutated_facts_fail_on_real_traces(self):
        facts = copy.deepcopy(self.facts)
        for stub in facts["plt_stubs"]:
            for relocation in stub["relocations"]:
                relocation["type"] = "R_X86_64_JUMP_SLOT"
        _report, violations = self.report("contract", facts)
        self.assertEqual({v["function"] for v in violations if "function" in v}, set(fx.WRAPPED))


if __name__ == "__main__":
    unittest.main()
