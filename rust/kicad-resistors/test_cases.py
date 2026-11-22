# SPDX-License-Identifier: GPL-3.0-or-later
"""Generator contract, independent of libm pow and without writing the corpus."""
import hashlib
import unittest

import cases


class Regeneration(unittest.TestCase):
    def test_original_request_bytes_and_raw_tokens_are_retained(self):
        data=cases.render()
        original=b"".join(data.splitlines(keepends=True)[:199])
        self.assertEqual(hashlib.sha256(original).hexdigest(),cases.ORIGINAL_SHA256)
        self.assertIn(b'"values_bits":[-0]',data)
        self.assertIn(b'"series":-0',data)
        self.assertEqual(len(data.splitlines()),1050)

    def test_historical_power_bits_are_explicit(self):
        self.assertEqual(cases.bits(cases.decimal_power(23)),0x44B52D02C7E14AF7)
        self.assertEqual(cases.bits(cases.decimal_power(126)),0x5A17A2ECC414A040)
        self.assertEqual(cases.bits(cases.decimal_power(210)),0x6B88557F31326BBB)


if __name__=="__main__":
    unittest.main()
