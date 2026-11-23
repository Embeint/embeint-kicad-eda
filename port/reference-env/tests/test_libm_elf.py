#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""The shared libm multiarch classifier (synthetic ELF files)."""

import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest

ENV = Path(__file__).resolve().parents[1]
for directory in ("tools", "probe"):
    sys.path.insert(0, str(ENV / directory))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import collect_environment  # noqa: E402
import libm_fixtures as fx  # noqa: E402
import libm_elf  # noqa: E402
import lock_inputs  # noqa: E402
import refenv  # noqa: E402

FUNC, IFUNC, OBJECT = 2, 10, 1
LOCAL, GLOBAL, WEAK = 0, 1, 2

# Debug symbols: genuine variants, aliases, duplicates and names that only look like variants.
DEBUG_SYMBOLS = [
    ("__sin_sse2", 0x2F8E0, 0x100, FUNC, LOCAL, 18),
    ("__sin_sse2", 0x2F8E0, 0x100, FUNC, LOCAL, 18),        # duplicate entry, same name and offset
    ("__sin_fma", 0x7B2D0, 0x100, FUNC, LOCAL, 18),
    ("__ieee754_exp_sse2", 0x27EA0, 0x228, FUNC, LOCAL, 18),
    ("__exp1_sse2_alias_sse2", 0x27EA0, 0x228, FUNC, WEAK, 18),  # alias at the same offset
    ("__ieee754_exp", 0x280D0, 0x4F, IFUNC, LOCAL, 18),
    ("__ieee754_exp_ifunc", 0x280D0, 0x4F, FUNC, LOCAL, 18),
    ("__fma", 0x33320, 0x35, IFUNC, LOCAL, 18),             # public fma's IFUNC dispatcher
    ("__fma_sse2", 0x32810, 0x10, FUNC, LOCAL, 18),
    ("__avx2", 0x40000, 0x10, FUNC, GLOBAL, 18),            # bare name: no stem
    ("__cos_fma", 0x41000, 0x10, IFUNC, LOCAL, 18),         # variant-looking dispatcher
    ("__tan_sse2", 0, 0, FUNC, GLOBAL, 0),                  # undefined
    ("__exp_data_sse2", 0x50000, 0x80, OBJECT, LOCAL, 20),  # data, not code
    ("sin", 0x30AE0, 0x4F, IFUNC, WEAK, 18),
    ("exp", 0x3A700, 0x6F, FUNC, GLOBAL, 18),
]
EXPECTED_TABLE = {
    "0x27ea0": ["__exp1_sse2_alias_sse2", "__ieee754_exp_sse2"],
    "0x2f8e0": ["__sin_sse2"],
    "0x32810": ["__fma_sse2"],
    "0x7b2d0": ["__sin_fma"],
}


class ClassifierTest(unittest.TestCase):
    def test_bare_and_dispatcher_names_are_not_variants(self):
        self.assertFalse(libm_elf.classify("__fma", IFUNC)[0])
        self.assertIn("bare public name", libm_elf.classify("__fma", IFUNC)[1])
        self.assertIn("IFUNC dispatcher", libm_elf.classify("__fma", IFUNC)[1])
        self.assertIn("bare public name", libm_elf.classify("__fma", FUNC)[1])
        self.assertIn("IFUNC dispatcher", libm_elf.classify("__cos_fma", IFUNC)[1])
        self.assertIn("undefined", libm_elf.classify("__tan_sse2", FUNC, 0)[1])
        self.assertIn("not a function", libm_elf.classify("__exp_data_sse2", OBJECT)[1])
        self.assertEqual(libm_elf.classify("sin", IFUNC), (False, None))
        self.assertEqual(libm_elf.classify("__ieee754_exp_ifunc", FUNC), (False, None))
        for name in ("__sin_sse2", "__ieee754_pow_fma4", "__floor_sse41", "__ceil_sse4_1", "__x_avx512",
                     "__y_evex", "__z_avx2", "__w_avx"):
            self.assertEqual(libm_elf.classify(name, FUNC), (True, None), name)

    def test_table_and_exclusions(self):
        table, excluded = libm_elf.multiarch_symbols(libm_elf.Elf64(fx.debug_elf("f772fc0c", DEBUG_SYMBOLS)))
        self.assertEqual(table, EXPECTED_TABLE)
        self.assertEqual([(e["name"], e["offset"], e["type"]) for e in excluded], [
            ("__avx2", "0x40000", "FUNC"), ("__cos_fma", "0x41000", "IFUNC"),
            ("__exp_data_sse2", "0x50000", "OBJECT"), ("__fma", "0x33320", "IFUNC"),
            ("__tan_sse2", "0x0", "FUNC")])

    def test_lock_generator_and_collector_agree(self):
        """Both producers read the same bytes through the same classifier."""
        debug = fx.debug_elf("f772fc0c", DEBUG_SYMBOLS)
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory) / "libm.debug").write_bytes(debug)
            lock = {"libm": {"path": str(Path(directory) / "libm.so.6"),
                             "debug_file": str(Path(directory) / "libm.debug"),
                             "multiarch_symbols": EXPECTED_TABLE}}
            record = collect_environment.libm_record(lock)
        notes = io.StringIO()
        with contextlib.redirect_stderr(notes):
            generated = lock_inputs.libm_variants(refenv.Elf64(debug))
        self.assertEqual(generated, EXPECTED_TABLE)
        self.assertIn("note: libm symbol __fma at 0x33320 (IFUNC) is not locked", notes.getvalue())
        self.assertEqual(record["multiarch_symbols_from_debug_file"], generated)
        self.assertTrue(record["matches_lock"])
        self.assertIn("__fma", [e["name"] for e in record["excluded_suffix_matches"]])
        lock["libm"]["multiarch_symbols"] = {**EXPECTED_TABLE, "0x33320": ["__fma"]}
        self.assertNotEqual(record["multiarch_symbols_from_debug_file"], lock["libm"]["multiarch_symbols"])


if __name__ == "__main__":
    unittest.main()
