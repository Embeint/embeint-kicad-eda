# SPDX-License-Identifier: GPL-3.0-or-later
import copy
import json
import pathlib
import unittest

import provider_layout


class ProviderLayoutTests(unittest.TestCase):
    def setUp(self):
        gate = pathlib.Path(provider_layout.__file__).parent
        refs = json.loads((gate / 'reference/provider-layout-preimages.json').read_bytes())
        self.name, self.old = next(iter(refs.items()))
        self.lock = json.loads((gate / 'locks/object.lock.json').read_bytes())['link']['provider_units'][self.name]
        self.new = dict(self.old, range=[2192, 2875])

    def test_historical_preimage_is_exactly_the_original_lock(self):
        self.assertEqual(provider_layout.digest(self.old), self.lock)
        self.assertTrue(provider_layout.bind(self.new, self.old, self.lock))
        self.assertTrue(provider_layout.bind(self.old, self.old, self.lock))

    def test_changed_code_target_or_relocation_is_rejected(self):
        for field in ('bytes_sha256', 'relocations', 'section'):
            value = copy.deepcopy(self.new)
            if field == 'bytes_sha256':
                value[field] = '0' * 64
            elif field == 'section':
                value[field] = '.other'
            else:
                value[field][0][2] += 1
            self.assertFalse(provider_layout.bind(value, self.old, self.lock), field)

    def test_range_must_retain_size_and_integer_type(self):
        for span in ([2192, 2874], [2192.0, 2875], [True, 684], [-1, 682], None):
            self.assertFalse(provider_layout.bind(dict(self.new, range=span), self.old, self.lock), span)

    def test_equal_valued_relocation_float_and_bad_historical_record_reject(self):
        value = copy.deepcopy(self.new)
        value['relocations'][0][0] = float(value['relocations'][0][0])
        self.assertFalse(provider_layout.bind(value, self.old, self.lock))
        self.assertFalse(provider_layout.bind(self.new, dict(self.old, range=[0, 683]), self.lock))
        self.assertFalse(provider_layout.bind(dict(self.new, extra=True), self.old, self.lock))


if __name__ == '__main__':
    unittest.main()
