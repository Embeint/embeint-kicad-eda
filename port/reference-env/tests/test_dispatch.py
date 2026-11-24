#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The traced libm dispatch chain proof (dispatch_proof.py), its decoder and tracer.

ProofTest and DecoderTest run anywhere. LifecycleTest needs Linux, g++ and
ptrace of a child. RuntimeDispatchTest runs only inside the pinned candidate
image (locked libm and debug file, objdump, g++, rustc). Skipped tests are
reported as skipped, not passed.
"""

import copy
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time
import unittest

ENV = Path(__file__).resolve().parents[1]
for directory in ("tools", "probe"):
    sys.path.insert(0, str(ENV / directory))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import dispatch_proof  # noqa: E402
import libm_elf  # noqa: E402
import libm_fixtures as fx  # noqa: E402
import refenv  # noqa: E402
import x86_decode  # noqa: E402

LOCK = refenv.load_lock()
CANDIDATE = refenv.load_candidate()
CHECKED = CANDIDATE["dispatch"]["checked_functions"]


class ProofTest(unittest.TestCase):
    """Synthetic libm with real code: valid paths and every class of contradictory trace."""

    def setUp(self):
        self.syn = fx.synthetic_libm(LOCK, CHECKED)
        self.facts = self.syn.facts
        self.observation = {"libm_dispatch": self.syn.records}

    def record(self, function):
        return next(r for r in self.observation["libm_dispatch"] if r["function"] == function)

    def at(self, function, key):
        return self.syn.layout[function][key]

    def report(self):
        caller = dispatch_proof.Caller("/out/bin/primitive_probe", fx.probe_elf())
        return dispatch_proof.dispatch_report(self.observation, self.facts, self.syn.lock, CANDIDATE,
                                              self.syn.elf, caller=caller)

    def reasons(self, function):
        return next((v["reasons"] for v in self.report()[1] if v.get("function") == function), [])

    def assertFails(self, function, fragment):
        reasons = self.reasons(function)
        self.assertTrue(any(fragment in r for r in reasons), (fragment, reasons))

    def test_valid_direct_and_wrapped_paths(self):
        report, violations = self.report()
        self.assertEqual(violations, [])
        rows = {row["function"]: row for row in report}
        self.assertEqual(rows["sin"]["path"], "direct")
        self.assertEqual(rows["exp"]["path"], "wrapped")
        hop = rows["exp"]["hops"][0]
        self.assertEqual(hop["target_names"], ["__ieee754_exp_sse2"])
        self.assertEqual(hop["resolver_names"], ["__ieee754_exp"])
        self.assertEqual(rows["exp"]["entry_definition"]["version"], "GLIBC_2.29")
        self.assertEqual(rows["exp"]["returned_at"], fx.hx(self.at("exp", "wrapper") + 9))
        self.assertEqual(rows["log"]["returned_at"], fx.hx(self.at("log", "sse2") + 7))  # tail call

    def test_single_address_and_truncated_traces(self):
        self.record("sin")["trace"] = self.record("sin")["trace"][:1]
        self.assertFails("sin", "trace ended before the entry's return")
        exp = self.record("exp")
        exp["trace"] = exp["trace"][:5]  # stops on arrival at the SSE2 target
        self.assertFails("exp", "trace ended before the entry's return")
        pow_ = self.record("pow")
        pow_["trace"] = pow_["trace"][:-1]  # implementation returned, wrapper's return missing
        self.assertFails("pow", "trace ended before the entry's return")

    def test_mid_instruction_and_impossible_edges(self):
        sin = self.record("sin")
        sin["trace"].insert(1, fx.hx(self.at("sin", "sse2") + 1))  # inside endbr64
        self.assertFails("sin", "impossible successor")
        cos = self.record("cos")
        cos["trace"][1] = fx.hx(self.at("cos", "sse2") + 7)  # skips an instruction
        self.assertFails("cos", "impossible successor")
        atan2 = self.record("atan2")
        atan2["trace"][2:3] = []  # call lands on the stub's jump, not its start
        self.assertFails("atan2", "impossible successor")
        tan = self.record("tan")
        tan["trace"] = [tan["trace"][0], tan["trace"][2]]
        self.assertFails("tan", "impossible successor")

    def test_forged_completion(self):
        self.record("atan")["return_to"] = "!/out/bin/primitive_probe+0x9999"
        self.assertFails("atan", "does not reach the recorded return address")
        self.record("sinf")["return_address"] = self.record("sinf")["return_to"] = "0x20000"
        self.assertFails("sinf", "does not reach the recorded return address")
        self.record("cosf")["trace_status"] = "timeout during the traced call"
        self.assertFails("cosf", "trace incomplete")
        exp = self.record("exp")
        exp["trace"].append(exp["trace"][-1])  # a step after the entry's return
        self.assertFails("exp", "leaves the call before the end of the trace")

    def test_completion_requires_the_probe_executable_mapping(self):
        for marker in ("!" + self.syn.lock["libm"]["path"] + "+0x20000", "!+0x0",
                       "!/out/bin/primitive_probe+0x0", "!/unknown+0x1234", "!unmapped+0x1234"):
            with self.subTest(marker=marker):
                self.record("sin")["return_address"] = self.record("sin")["return_to"] = marker
                self.assertFails("sin", "in executable probe code")

    def test_caller_requires_supported_executable_load_segments(self):
        original = fx.probe_elf()
        phoff = struct.unpack_from("<Q", original, 0x20)[0]
        changes = (
            (7, "<B", 9),       # unsupported OS ABI
            (8, "<B", 1),       # unsupported ABI version
            (0x12, "<H", 183),  # AArch64
            (0x10, "<H", 1),    # relocatable, not executable/PIE
            (0x38, "<H", 0),    # no program headers
            (phoff + 56 + 4, "<I", 4),  # executable section, non-executable PT_LOAD
            (phoff + 56 + 32, "<Q", len(original) + 1),  # segment exceeds the file
            (0x18, "<Q", 0),    # entry is not in executable code
        )
        for offset, fmt, value in changes:
            with self.subTest(offset=offset, value=value):
                data = bytearray(original)
                struct.pack_into(fmt, data, offset, value)
                with self.assertRaises(ValueError):
                    dispatch_proof.Caller("/out/bin/primitive_probe", bytes(data))

    def test_caller_load_bias_and_section_flags(self):
        data = bytearray(fx.probe_elf())
        elf = libm_elf.Elf64(data)
        text_index = elf.names.index(".text")
        struct.pack_into("<Q", data, elf.shoff + text_index * elf.shentsize + 8, 0)
        caller = dispatch_proof.Caller("/out/bin/primitive_probe", bytes(data))
        self.assertTrue(caller.contains(fx.RETURN))  # PT_LOAD still grants execution.
        self.assertFalse(caller.contains("!/out/bin/primitive_probe+0x0"))
        phoff = struct.unpack_from("<Q", data, 0x20)[0]
        struct.pack_into("<H", data, 0x10, 2)  # ET_EXEC with a fixed, nonzero mapping base.
        struct.pack_into("<Q", data, 0x18, 0x401040)
        for i in range(2):
            at = phoff + 56 * i
            address = struct.unpack_from("<Q", data, at + 16)[0] + 0x400000
            struct.pack_into("<QQ", data, at + 16, address, address)
        caller = dispatch_proof.Caller("/out/bin/primitive_probe", bytes(data))
        self.assertTrue(caller.contains(fx.RETURN))
        self.assertFalse(caller.contains("!/out/bin/primitive_probe+0x401234"))

    def test_wrong_target_through_slot(self):
        record = self.record("exp")
        fma = self.at("exp", "fma")
        record["trace"][4:7] = [fx.hx(fma), fx.hx(fma + 4), fx.hx(fma + 7)]
        record["slot_reads"][0]["value"] = fx.hx(fma)
        self.assertFails("exp", "did not lead to the locked SSE2 implementation")
        self.assertFails("exp", "forbidden variant executed: __ieee754_exp_fma")

    def test_slot_read_must_match_jump(self):
        self.record("pow")["slot_reads"][0]["value"] = fx.hx(self.at("pow", "fma"))
        self.assertFails("pow", "recorded GOT slot read does not match")
        self.record("log")["slot_reads"] = []
        self.assertFails("log", "recorded GOT slot read does not match")
        self.record("sin")["slot_reads"] = [dict(self.record("exp")["slot_reads"][0])]
        self.assertFails("sin", "GOT slot reads without a matching stub jump")

    def test_wrong_direct_variant(self):
        record = self.record("sin")
        fma = self.at("sin", "fma")
        record["offset"] = record["linked_offset"] = fx.hx(fma)
        record["trace"] = [fx.hx(fma), fx.hx(fma + 4), fx.hx(fma + 7)]
        self.assertFails("sin", "IFUNC resolved to an address other than the locked SSE2 implementation")
        self.assertFails("sin", "forbidden variant executed: __sin_fma")

    def test_wrong_object_binding_and_version(self):
        self.record("exp")["object"] = "/other/libm.so.6"
        self.assertFails("exp", "not in the locked libm")
        self.record("cos")["linked_offset"] = "0x10"
        self.assertFails("cos", "compiled reference and dlsym() resolve to different addresses")
        pow_ = self.record("pow")
        compat = fx.hx(self.at("pow", "compat"))
        pow_["offset"] = pow_["linked_offset"] = pow_["trace"][0] = compat
        self.assertFails("pow", "entry is not the public wrapper definition")

    def test_unknown_paths_fail_closed(self):
        for row in self.facts["definitions"]["log"]:
            row["default"] = False
        self.assertFails("log", "no unique default-version public definition")
        self.facts["definitions"]["atan"][0]["type"] = "OBJECT"
        self.assertFails("atan", "neither FUNC nor IFUNC")
        exp = self.record("exp")
        exp["trace"] = exp["trace"][:2] + [exp["trace"][-1]]
        self.assertFails("exp", "impossible successor")
        self.assertFails("exp", "wrapper returned without reaching an implementation")

    def test_stub_and_relocation_defects(self):
        stub = next(s for s in self.facts["plt_stubs"] if s["stub"] == fx.hx(self.at("exp", "stub")))
        stub["template"] = False
        self.assertFails("exp", "indirect jump outside an exact .plt.sec stub")
        stub["template"] = True
        stub["relocations"][0]["symbol"] = 17
        self.assertFails("exp", "not a single symbol-less IRELATIVE relocation")
        stub["relocations"][0].update(symbol=0, type="R_X86_64_JUMP_SLOT")
        self.assertFails("exp", "not a single symbol-less IRELATIVE relocation")
        stub["relocations"] = [dict(stub["relocations"][0], type="R_X86_64_IRELATIVE")] * 2
        self.assertFails("exp", "not a single symbol-less IRELATIVE relocation")
        stub["relocations"] = stub["relocations"][:1]
        self.facts["ifuncs"][stub["relocations"][0]["addend"]] = ["__ieee754_log"]
        self.assertFails("exp", "not the SSE2 variant of the slot's IFUNC resolver")

    def test_trace_leaving_libm_and_unknown_code(self):
        self.record("tan")["trace"].insert(1, "!/usr/lib/x86_64-linux-gnu/libc.so.6+0x10")
        self.assertFails("tan", "execution left libm")
        self.record("atan")["trace"].insert(1, "0xdead0")
        self.assertFails("atan", "not inside a known libm function")

    def test_identity_and_record_set(self):
        facts = copy.deepcopy(self.facts)
        facts["libm_sha256"] = "0" * 64
        _report, violations = dispatch_proof.dispatch_report(self.observation, facts, self.syn.lock, CANDIDATE,
                                                             self.syn.elf)
        self.assertIn("dispatch facts were not read from the locked libm bytes", violations[0]["problems"])
        rotated = {"libm_dispatch": self.syn.records[1:] + self.syn.records[:1]}
        _report, violations = dispatch_proof.dispatch_report(rotated, self.facts, self.syn.lock, CANDIDATE,
                                                             self.syn.elf)
        self.assertEqual(violations[0]["reason"], "missing, extra, duplicate or reordered dispatch records")

    def test_recorded_facts_must_equal_locked_files(self):
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        (directory / "libm").mkdir()
        (directory / dispatch_proof.LIBM_COPY).write_bytes(self.syn.libm)
        (directory / dispatch_proof.DEBUG_COPY).write_bytes(self.syn.debug)
        table, excluded = libm_elf.multiarch_symbols(libm_elf.Elf64(self.syn.debug))
        environment = {"libm": {"dispatch_facts": copy.deepcopy(self.facts),
                                "multiarch_symbols_from_debug_file": table, "excluded_suffix_matches": excluded}}

        def verify():
            return dispatch_proof.verified_facts(directory, environment, self.syn.lock, CHECKED)[2]

        self.assertEqual(verify(), [])
        mutations = [
            lambda f: [s.update(bytes="cc" * 16, slot="0xdeadbeef") for s in f["plt_stubs"]],
            lambda f: [r.update(symbol=17) for s in f["plt_stubs"] for r in s["relocations"]],
            lambda f: [row.update(version=None, default=1) for rows in f["definitions"].values() for row in rows],
            lambda f: f.update(schema=True),
            lambda f: f["functions"].append(["0x99999", 4, ["__extra"]]),
        ]
        for mutate in mutations:
            environment["libm"]["dispatch_facts"] = copy.deepcopy(self.facts)
            mutate(environment["libm"]["dispatch_facts"])
            self.assertIn("recorded dispatch facts differ from the locked ELF files", verify())
        environment["libm"]["dispatch_facts"] = copy.deepcopy(self.facts)
        environment["libm"]["excluded_suffix_matches"] = [{"name": "__x"}]
        self.assertIn("recorded multiarch table differs from the locked debug file", verify())
        changed = bytearray(self.syn.libm)
        changed[-1] ^= 1
        (directory / dispatch_proof.LIBM_COPY).write_bytes(bytes(changed))
        self.assertIn(f"{dispatch_proof.LIBM_COPY} is not the locked file", verify())
        (directory / dispatch_proof.LIBM_COPY).unlink()
        self.assertIn(f"{dispatch_proof.LIBM_COPY} is missing or not a regular file", verify())
        if os.name != "nt":
            (directory / dispatch_proof.LIBM_COPY).symlink_to(directory / dispatch_proof.DEBUG_COPY)
            self.assertIn(f"{dispatch_proof.LIBM_COPY} is missing or not a regular file", verify())


class DecoderTest(unittest.TestCase):
    """Lengths and kinds of real encodings from the locked libm; everything else is refused."""

    CASES = [
        ("f30f1efa", 4, "seq"), ("55", 1, "seq"), ("4889e5", 3, "seq"), ("66480f7ec3", 5, "seq"),
        ("4883ec08", 4, "seq"), ("f30f7e0d61530500", 8, "seq"), ("f20f1015f9e60500", 8, "seq"),
        ("660f54d9", 4, "seq"), ("660f2ed3", 4, "seq"), ("488b5df8", 4, "seq"), ("c9", 1, "seq"),
        ("660f1f840000000000", 9, "seq"), ("488b0552d80a00", 7, "seq"), ("64c70022000000", 7, "seq"),
        ("48b80000000000000000", 10, "seq"), ("0f45c2", 3, "seq"), ("0f9bc0", 3, "seq"),
        ("0fae5c24fc", 5, "seq"), ("f6c510", 3, "seq"), ("f682b800000020", 7, "seq"),
        ("81e100000010", 6, "seq"), ("480f44c2", 4, "seq"), ("0fc6c000", 4, "seq"), ("f20f2ac0", 4, "seq"),
        ("660f3a0bc104", 6, "seq"), ("660f3800c1", 5, "seq"), ("660f1f440000", 6, "seq"),
        ("0f50c0", 3, "seq"), ("660f50c0", 4, "seq"),
        ("c3", 1, "ret"), ("f3c3", 2, "ret"), ("e8fbffffff", 5, "call"), ("e9fbffffff", 5, "jmp"),
        ("eb02", 2, "jmp"), ("721b", 2, "jcc"), ("0f8510000000", 6, "jcc"), ("ff2566000000", 6, "jmp_rip_memory"),
    ]
    REFUSED = ["c5f957c0", "c4e2f1a9c2", "62f1fd4858c0", "0f0b", "0f05", "cc", "ffd0", "ffe0", "ff10", "d9e8",
               "f3a4", "66e90000", "f4", "0fa2", "0f77", "8f00", "f0f0f0f0f0c3", "",
               "0f38ffc0", "0f3affc000", "660f38ffc0", "660f3affc000", "c7f801000000", "c6f801",
               "0f3800c1", "f30f3800c1", "f2660f3a0bc104", "f0c3", "660f5000", "f30f50c0"]

    def test_lengths_and_kinds(self):
        for code, length, kind in self.CASES:
            instruction = x86_decode.decode(bytes.fromhex(code) + b"\x90" * 8, 0x1000)
            self.assertEqual((instruction.length, instruction.kind), (length, kind), code)
        self.assertEqual(x86_decode.decode(bytes.fromhex("e8fbffffff"), 0x1000).target, 0x1000)
        self.assertEqual(x86_decode.decode(bytes.fromhex("721b"), 0x1000).target, 0x101D)
        self.assertEqual(x86_decode.decode(bytes.fromhex("ff2566000000"), 0x10264).memory, 0x102D0)

    def test_refused(self):
        for code in self.REFUSED:
            with self.assertRaises(x86_decode.Unsupported, msg=code):
                x86_decode.decode(bytes.fromhex(code), 0x1000)
        with self.assertRaises(x86_decode.Unsupported):
            x86_decode.decode(bytes.fromhex("e8fbff"), 0x1000)  # truncated


def can_trace():
    return sys.platform.startswith("linux") and shutil.which("g++") is not None


@unittest.skipUnless(can_trace(), "needs Linux, g++ and ptrace of a child")
class LifecycleTest(unittest.TestCase):
    """Real processes: the tracer's deadlines, error paths and child cleanup."""

    @classmethod
    def setUpClass(cls):
        cls.directory = Path(tempfile.mkdtemp())
        cls.harness = cls.directory / "trace_lifecycle_harness"
        subprocess.run(["g++", "-O2", "-std=gnu++20", "-Wall", "-Wextra", "-I", str(ENV / "probe"), "-o",
                        str(cls.harness), str(ENV / "tests/trace_lifecycle_harness.cpp"),
                        "-Wl,--wrap=waitpid", "-Wl,--wrap=kill"], check=True, timeout=300)
        # Orphaned tracees reparent to this process, so it can prove they were killed.
        libc = ctypes.CDLL(None, use_errno=True)
        cls.subreaper = libc.prctl(36, 1, 0, 0, 0) == 0  # PR_SET_CHILD_SUBREAPER

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.directory)

    def run_harness(self, scenario, total_ms=1500):
        start = time.monotonic()
        result = subprocess.run([str(self.harness), scenario, str(total_ms)], capture_output=True, text=True,
                                timeout=30)
        elapsed = time.monotonic() - start
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertTrue(report["reaped"], report)
        self.assertFalse(report["child_exists"], report)
        self.assertEqual(report["children_left"], [], report)
        return report, elapsed

    def test_success_and_repeated_calls(self):
        report, _elapsed = self.run_harness("success")
        self.assertEqual(report["status"], "returned")
        self.assertGreater(report["steps"], 2)
        self.assertEqual(report["return_to"], report["return_address"])
        report, _elapsed = self.run_harness("repeat17")
        self.assertEqual(report["statuses"], ["returned"] * 17)

    def test_auto_reaping_dispositions_are_rejected_without_forking(self):
        for scenario in ("auto-ignore", "auto-nocldwait"):
            with self.subTest(scenario=scenario):
                result = subprocess.run([str(self.harness), scenario, "100"], capture_output=True,
                                        text=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                report = json.loads(result.stdout)
                self.assertEqual(report["status"], "unsupported SIGCHLD auto-reaping disposition")
                self.assertEqual(report["child"], -1)
                self.assertEqual(report["children_left"], [])
                self.assertEqual(report["kills_after_lost"], 0)

    def test_echild_never_signals_the_released_pid(self):
        result = subprocess.run([str(self.harness), "lost-child", "500"], capture_output=True,
                                text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertIn("child no longer owned", report["status"])
        self.assertEqual(report["kills_after_lost"], 0)
        self.assertEqual(report["children_left"], [])
        self.assertFalse(report["child_exists"])

    def test_full_buffered_output_does_not_block_before_fork(self):
        result = subprocess.run([str(self.harness), "full-output", "1500"], capture_output=True,
                                text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("returned=1 reaped=1 children=0", result.stderr)

    def test_blocking_before_entry_is_bounded(self):
        report, elapsed = self.run_harness("block-before-entry", 800)
        self.assertEqual(report["status"], "timeout before the traced call started")
        self.assertLess(elapsed, 10)

    def test_blocking_in_callee_is_bounded(self):
        report, elapsed = self.run_harness("block-in-callee", 800)
        self.assertEqual(report["status"], "timeout during the traced call")
        self.assertLess(elapsed, 10)

    def test_abnormal_exit_and_signal(self):
        report, _elapsed = self.run_harness("exit-in-callee")
        self.assertEqual(report["status"], "child exited with status 7 during the traced call")
        report, _elapsed = self.run_harness("abort-in-callee")
        self.assertEqual(report["status"], "child stopped by signal 6 during the traced call")
        report, _elapsed = self.run_harness("exit-before-entry")
        self.assertEqual(report["status"], "child exited with status 9 before the traced call started")

    def test_step_bound(self):
        report, _elapsed = self.run_harness("step-bound")
        self.assertEqual(report["status"], "step bound exceeded")

    def test_tracee_dies_with_tracer(self):
        if not self.subreaper:
            self.skipTest("PR_SET_CHILD_SUBREAPER unavailable")
        process = subprocess.Popen([str(self.harness), "block-in-callee", "60000", "--announce"],
                                   stdout=subprocess.PIPE, text=True)
        try:
            child = json.loads(process.stdout.readline())["child"]
            time.sleep(0.2)
            process.send_signal(signal.SIGKILL)
            process.wait(timeout=10)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=10)
            process.stdout.close()
        deadline = time.monotonic() + 5
        reaped = False
        while time.monotonic() < deadline:
            try:
                pid, status = os.waitpid(child, os.WNOHANG)
            except ChildProcessError:
                reaped = not Path(f"/proc/{child}").exists()
                break
            if pid == child:
                reaped = os.WIFSIGNALED(status) and os.WTERMSIG(status) == signal.SIGKILL
                break
            time.sleep(0.05)
        self.assertTrue(reaped, "tracee survived its tracer")


def in_pinned_image():
    try:
        libm = Path(LOCK["libm"]["path"])
        return (Path("/opt/refenv/image-identity.json").is_file() and libm.is_file()
                and hashlib.sha256(libm.read_bytes()).hexdigest() == fx.libm_sha256(LOCK))
    except OSError:
        return False


@unittest.skipUnless(in_pinned_image(), "needs the pinned candidate image (locked libm, objdump, g++, rustc)")
class RuntimeDispatchTest(unittest.TestCase):
    """A real run_probe.py run in the image, its proof, and the full comparator on mutated copies."""

    @classmethod
    def setUpClass(cls):
        cls.root = Path(tempfile.mkdtemp())
        refenv_copy = cls.root / "refenv"
        shutil.copytree(ENV, refenv_copy, ignore=shutil.ignore_patterns("__pycache__"))
        shutil.copyfile("/opt/refenv/image-identity.json", refenv_copy / "image-identity.json")
        cls.out = cls.root / "out"
        env = dict(os.environ, GLIBC_TUNABLES=CANDIDATE["dispatch"]["glibc_tunables"], PYTHONDONTWRITEBYTECODE="1")
        cls.probe_run = subprocess.run([sys.executable, str(refenv_copy / "probe/run_probe.py"), "--out", str(cls.out),
                                  "--sweep-count", "1"], env=env, capture_output=True, text=True, timeout=1800)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.root)

    def compare(self, directory):
        return subprocess.run([sys.executable, "-B", str(ENV / "probe/compare_observations.py"), str(directory),
                               str(directory)], capture_output=True, text=True, timeout=600)

    def test_probe_passes_and_validates(self):
        self.assertEqual(self.probe_run.returncode, 0, self.probe_run.stderr[-4000:])
        summary = json.loads((self.out / "summary.json").read_text())
        dispatch = summary["contract_checks"][0]
        self.assertTrue(dispatch["passed"], dispatch["detail"]["violations"])
        rows = {row["function"]: row for row in dispatch["detail"]["dispatch"]}
        for function in fx.WRAPPED:
            self.assertEqual(rows[function]["path"], "wrapped", function)
            self.assertEqual(len(rows[function]["hops"]), 1, function)
        self.assertTrue(all(row["returned_at"] for row in rows.values()))
        host = summary["comparisons"]["contract_vs_host_default_dispatch"]["dispatch"]
        flags = next((line for line in Path("/proc/cpuinfo").read_text().splitlines()
                      if line.startswith("flags")), "").split()
        if "fma" in flags:
            self.assertTrue(all(row["violations"] for row in host))
        result = self.compare(self.out)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def mutated(self, name, change):
        """A copy of the real run with one change, comparisons and inventory recomputed."""
        import artifacts
        import run_probe

        directory = self.root / name
        shutil.copytree(self.out, directory)
        lock = artifacts.load(directory / "inputs.lock.json")
        env = artifacts.load(directory / "environment.json")
        results = {"contract": artifacts.load(directory / "primitive-contract.json"),
                   "host-default": artifacts.load(directory / "primitive-host-default.json"),
                   "rust": artifacts.load(directory / "rust-contract.json")}
        change(directory, env, results)
        for file, data in (("environment.json", env), ("primitive-contract.json", results["contract"])):
            (directory / file).write_text(json.dumps(data, sort_keys=True) + "\n")
        summary = artifacts.load(directory / "summary.json")
        summary["comparisons"] = run_probe.comparison_report(directory, results, lock, CANDIDATE, env)
        summary["output_sha256"] = artifacts.inventory(directory)
        (directory / "summary.json").write_text(json.dumps(summary, sort_keys=True) + "\n")
        return directory

    def assertRejected(self, name, change, fragment):
        result = self.compare(self.mutated(name, change))
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(fragment, result.stderr)

    @staticmethod
    def rows(results):
        return results["contract"]["libm_dispatch"]

    def test_comparator_rejects_contradictory_facts(self):
        def stubs(_directory, env, _results):
            for stub in env["libm"]["dispatch_facts"]["plt_stubs"]:
                stub.update(bytes="cc" * 16, slot="0xdeadbeef")
                for relocation in stub["relocations"]:
                    relocation["symbol"] = 17

        def versions(_directory, env, _results):
            facts = env["libm"]["dispatch_facts"]
            facts["schema"] = True
            for rows in facts["definitions"].values():
                for row in rows:
                    row["version"] = None
                    row["default"] = 1 if row["default"] else 0

        def library(directory, _env, _results):
            path = directory / dispatch_proof.LIBM_COPY
            data = bytearray(path.read_bytes())
            data[0x10230] = 0xCC
            path.write_bytes(bytes(data))

        self.assertRejected("stubs", stubs, "recorded dispatch facts differ from the locked ELF files")
        self.assertRejected("versions", versions, "recorded dispatch facts differ from the locked ELF files")
        self.assertRejected("library", library, "libm/libm.so.6 is not the locked file")

    def test_comparator_rejects_incomplete_or_impossible_traces(self):
        def entry_only(_directory, _env, results):
            for row in self.rows(results):
                if row["function"] not in fx.WRAPPED:
                    row["trace"] = row["trace"][:1]

        def truncated(_directory, _env, results):
            for row in self.rows(results):
                if row["function"] in fx.WRAPPED:
                    position = row["trace"].index(row["slot_reads"][0]["at"])
                    row["trace"] = row["trace"][:position + 2]

        def mid_instruction(_directory, _env, results):
            row = self.rows(results)[0]
            row["trace"].insert(1, f"0x{int(row['trace'][0], 16) + 1:x}")

        def forged_return(_directory, _env, results):
            self.rows(results)[0]["return_to"] = "!/elsewhere+0x10"

        def forged_slot(_directory, _env, results):
            row = next(r for r in self.rows(results) if r["function"] == "exp")
            row["slot_reads"][0]["value"] = "0x79b60"

        self.assertRejected("entry-only", entry_only, "trace ended before the entry's return")
        self.assertRejected("truncated", truncated, "trace ended before the entry's return")
        self.assertRejected("mid-instruction", mid_instruction, "impossible successor")
        self.assertRejected("forged-return", forged_return, "does not reach the recorded return address")
        self.assertRejected("forged-slot", forged_slot, "recorded GOT slot read does not match")

    def test_decoder_agrees_with_objdump(self):
        """Every instruction the decoder accepts in the locked libm has objdump's length and kind."""
        dump = subprocess.run(["objdump", "-d", "-w", "--no-show-raw-insn", "-j", ".text", "-j", ".plt.sec",
                               LOCK["libm"]["path"]], capture_output=True, text=True, check=True, timeout=600).stdout
        elf = libm_elf.Elf64(Path(LOCK["libm"]["path"]).read_bytes())
        rows = [(int(m.group(1), 16), m.group(2).strip()) for m in re.finditer(r"^\s+([0-9a-f]+):\t(.*)$", dump, re.M)]
        accepted = refused = 0
        for (address, text), (following, _next) in zip(rows, rows[1:] + [(None, "")]):
            try:
                instruction = x86_decode.decode(elf.code(address), address)
            except x86_decode.Unsupported:
                refused += 1
                continue
            words = text.split()
            prefixes = ("bnd", "notrack", "rep", "repz", "repnz", "data16", "cs", "ds", "fs", "gs")
            position = 1 if words and words[0] in prefixes and len(words) > 1 else 0
            mnemonic = words[position] if words else ""
            operand = words[position + 1] if len(words) > position + 1 else ""
            if following is not None and 0 < following - address <= 15:
                self.assertEqual(instruction.length, following - address, (hex(address), text))
            if mnemonic == "ret":
                control = "ret"
            elif mnemonic == "call":
                control = "call"
            elif mnemonic == "jmp":
                control = "jmp_rip_memory" if operand.startswith("*") else "jmp"
            elif mnemonic.startswith("j") or mnemonic.startswith("loop"):
                control = "jcc"
            else:
                control = "seq"
            self.assertEqual(instruction.kind, control, (hex(address), text))
            if instruction.target is not None:
                self.assertIn(f"{instruction.target:x}", text, (hex(address), text))
            accepted += 1
        self.assertGreater(accepted, 10000)
        self.assertGreater(refused, 0)  # FMA/AVX code is VEX encoded and refused


if __name__ == "__main__":
    unittest.main()
