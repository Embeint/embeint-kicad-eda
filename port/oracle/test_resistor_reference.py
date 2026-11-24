#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Host-independent negative controls for the resistor runtime preflight."""
import unittest
from unittest.mock import patch

import check_resistor_reference as reference


class ContractTest(unittest.TestCase):
    def test_environment_requires_the_exact_startup_contract(self):
        good = {"GLIBC_TUNABLES": reference.TUNABLES}
        reference.check_environment(good, "Linux", "x86_64")
        for env, system, machine in [
            ({}, "Linux", "x86_64"),
            ({"GLIBC_TUNABLES": ""}, "Linux", "x86_64"),
            (good, "Darwin", "arm64"),
            (good, "Linux", "aarch64"),
            *[({**good, name: "override"}, "Linux", "x86_64") for name in
              ("LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT", "LD_ASSUME_KERNEL")],
        ]:
            with self.subTest(env=env, system=system, machine=machine):
                with self.assertRaises(ValueError):
                    reference.check_environment(env, system, machine)

    def test_linkage_rejects_missing_or_different_runtime_objects(self):
        paths = list(reference.FILES)
        lines = [f"{path.rsplit('/', 1)[1]} => {path} (0x123)" for path in paths]
        # Preserve Linux path identities when the test host is another platform.
        with patch.object(reference.Path, "resolve", lambda self: self):
            reference.check_libraries("\n".join(lines), reference.FILES)
            for index in range(len(lines)):
                for changed in (lines[:index] + lines[index+1:],
                                lines[:index] + [lines[index].replace('/usr/lib/', '/tmp/')] + lines[index+1:]):
                    with self.assertRaises(ValueError):
                        reference.check_libraries("\n".join(changed), reference.FILES)


if __name__ == "__main__":
    unittest.main()
