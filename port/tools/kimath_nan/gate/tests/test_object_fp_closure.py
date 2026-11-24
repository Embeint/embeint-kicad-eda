# SPDX-License-Identifier: GPL-3.0-or-later
"""Counterexamples for the whole-executable FP scan and installed-closure check."""
import contextlib
import io
import subprocess
import unittest
from types import SimpleNamespace
from unittest import mock

import objgate
from x86len import decode


class ObjectFpScanTests(unittest.TestCase):
    def scan(self, code):
        # Deliberately outside .text: the separate objdump scan cannot cover this section.
        elf = SimpleNamespace(data=code, symbols=[], sections=[{
            'name': '.init', 'flags': 4, 'type': 1, 'addr': 4096,
            'size': len(code), 'offset': 0,
        }])
        return objgate.fp_sweep(elf)

    def test_fp_writes_with_prefixes_and_displacements(self):
        for encoding, instruction in (
            ('0fae10', 'ldmxcsr'),             # (%rax)
            ('670fae10', 'ldmxcsr'),           # (%eax), address-size override
            ('0fae5566', 'ldmxcsr'),           # 0x66(%rbp), not an operand prefix
            ('0fae95123456c0', 'ldmxcsr'),     # disp32 ends in a byte with high bits 11
            ('660fae10', 'ldmxcsr'),
            ('0fae08', 'fxrstor'),
            ('0fae28', 'xrstor'),
            ('d928', 'fldcw'),
        ):
            with self.subTest(encoding=encoding):
                code = bytes.fromhex(encoding)
                hits, errors, covered = self.scan(code)
                self.assertEqual(hits, [('0x1000', instruction)])
                self.assertEqual(errors, [])
                self.assertEqual(covered, len(code))

    def test_displacement_bytes_are_not_reported_as_prefixes(self):
        ins = decode(bytes.fromhex('0fae5566'), 0, 4096)
        self.assertEqual(ins['prefixes'], b'')
        self.assertEqual(ins['modrm_mod'], 1)

    def test_state_reads_and_ordinary_instructions_are_controls(self):
        for encoding in ('0fae18', '0fae00', '90', 'f30f1efa'):
            hits, errors, covered = self.scan(bytes.fromhex(encoding))
            self.assertEqual(hits, [])
            self.assertEqual(errors, [])
            self.assertEqual(covered, len(bytes.fromhex(encoding)))


class ObjectClosureTests(unittest.TestCase):
    def test_closure_command_cannot_be_supplied_by_the_caller(self):
        args = ['verify', '--oracle', 'oracle', '--object', 'matrix.o', '--providers', 'objects',
                '--locks', 'locks', '--out', 'report.json', '--verify-closure']
        self.assertIs(objgate.parser().parse_args(args).verify_closure, True)
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            objgate.parser().parse_args(args + ['true'])
        self.assertEqual(error.exception.code, 2)

    def test_fixed_verifier_failure_is_not_a_closure_pass(self):
        with mock.patch.object(objgate.subprocess, 'run', return_value=SimpleNamespace(
                returncode=1, stdout='', stderr='changed installed file')) as run:
            ok, detail = objgate.verify_installed_closure('expected-identity.json')
        self.assertFalse(ok)
        self.assertIn('changed installed file', detail['output'])
        command = run.call_args.args[0]
        self.assertTrue(command[1].endswith('/port/reference-env/image/verify_image.py'))
        self.assertEqual(command[2:], ['--lock', '/opt/refenv/inputs.lock.json', '--candidate',
                                     '/opt/refenv/candidate.toml', '--expect', 'expected-identity.json'])
        self.assertEqual(run.call_args.kwargs['timeout'], 180)

    def test_closure_timeout_is_a_failure(self):
        with mock.patch.object(objgate.subprocess, 'run', side_effect=subprocess.TimeoutExpired('verify', 180)):
            ok, detail = objgate.verify_installed_closure('expected-identity.json')
        self.assertFalse(ok)
        self.assertIn('timed out', detail['error'])
