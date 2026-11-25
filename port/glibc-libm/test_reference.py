# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import domain
import reference as ref

class ReferenceTests(unittest.TestCase):
    def test_special_cartesian_products(self):
        fixed=set(domain.fixed())
        specials=[0,1,0x7ff0000000000000,0x7ff0000000000001,0x7ff80000deadbeef]
        for x in specials:
            for y in specials:
                for sx in [0,domain.SIGN]:
                    for sy in [0,domain.SIGN]: self.assertIn((x|sx,y|sy),fixed)
    def test_fixed_count_and_order(self):
        fixed=domain.fixed()
        self.assertEqual(len(fixed),ref.load()['domain']['fixed_count'])
        self.assertEqual(fixed,sorted(set(fixed)))
    def test_generator_hash(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(domain.write(Path(d)/'domain.bin'),ref.load()['domain'])
    def test_domain_rejects_wrong_size_and_hash(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'bad';p.write_bytes(b'bad')
            with self.assertRaises(ref.trig.Refused):ref.check_domain(p)
            with p.open('wb') as f: f.truncate(ref.load()['domain']['count']*16)
            with self.assertRaises(ref.trig.Refused):ref.check_domain(p)
    def test_report_requires_every_function_and_target(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertFalse(ref.report(d))
            count=ref.load()['domain']['count']
            lines=''.join(f'glibc_libm {fn}: {count} records, 0 mismatches\n' for fn in ref.FUNCTIONS)
            for name in ref.COMPARISONS:(Path(d)/(name+'.log')).write_text(lines)
            self.assertTrue(ref.report(d))
            path=Path(d)/'unknown-release.log';path.write_text(lines+lines)
            self.assertFalse(ref.report(d))
            path.write_text(lines.replace('0 mismatches','1 mismatches',1))
            self.assertFalse(ref.report(d))

if __name__=='__main__':unittest.main()
