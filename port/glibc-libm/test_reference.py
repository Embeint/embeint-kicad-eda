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
import hypot_domain
import pow_domain
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
            angle=''.join(f'glibc_libm {fn}: {count} records, 0 mismatches\n' for fn in ref.FUNCTIONS)
            for name in ref.COMPARISONS:(Path(d)/(name+'.log')).write_text(angle)
            self.assertFalse(ref.report(d),'pow is required on every target')
            pow_line=f"glibc_libm pow: {ref.load()['pow_domain']['count']} records, 0 mismatches\n"
            hypot_line=f"glibc_libm hypot: {ref.load()['hypot_domain']['count']} records, 0 mismatches\n"
            for name in ref.COMPARISONS:(Path(d)/(name+'.log')).write_text(angle+pow_line)
            self.assertFalse(ref.report(d),'hypot is required on every target')
            lines=angle+pow_line+hypot_line
            (Path(d)/'native-debug.log').write_text(angle+f'glibc_libm pow: {count} records, 0 mismatches\n'+hypot_line)
            for name in ref.COMPARISONS[1:]:(Path(d)/(name+'.log')).write_text(lines)
            self.assertFalse(ref.report(d),'pow needs its own record count')
            (Path(d)/'native-debug.log').write_text(angle+pow_line+f'glibc_libm hypot: {count} records, 0 mismatches\n')
            self.assertFalse(ref.report(d),'hypot needs its own record count')
            for name in ref.COMPARISONS:(Path(d)/(name+'.log')).write_text(lines)
            self.assertTrue(ref.report(d))
            path=Path(d)/'unknown-release.log';path.write_text(lines+lines)
            self.assertFalse(ref.report(d))
            path.write_text(lines.replace('0 mismatches','1 mismatches',1))
            self.assertFalse(ref.report(d))

class PowDomainTests(unittest.TestCase):
    def test_pow_generator_hash_and_order(self):
        fixed=pow_domain.fixed()
        self.assertEqual(fixed,sorted(set(fixed)))
        self.assertEqual(len(fixed),ref.load()['pow_domain']['fixed_count'])
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(pow_domain.write(Path(d)/'pow-domain.bin'),ref.load()['pow_domain'])
    def test_pow_source_branches_are_present(self):
        fixed=set(pow_domain.fixed())
        b=domain.bits
        snan,qnan=0x7ff4000000000abc,0x7ff80000deadbeef
        # y == 0, x == 1, NaN order, signed zero/infinity, invalid and
        # exponent-range branches of e_pow.c.
        for pair in [(snan,0),(qnan|domain.SIGN,0),(b(1.0),snan),(qnan,snan),(snan,qnan),
                     (b(-0.0),b(-3.0)),(b(-2.0),b(0.5)),(b(2.0),b(2.0**63)),
                     (b(2.0),b(2.0**-66)),(b(0.5),b(2.0**-66)),(b(-2.0),b(-1075.0)),
                     (b(2.0),b(-1075.0)),(b(2.0),b(1024.0)),(1,b(0.5))]:
            self.assertIn(pair,fixed)
        # Every digit count EDA_ANGLE::Round may pass reaches the full range.
        for digits in range(-1100,1101): self.assertIn((b(10.0),b(float(digits))),fixed)
        # Every log_inline table bin for x near 1.
        for j in range(128):
            self.assertIn((pow_domain.OFF+(j<<45),b(1.0)),fixed)
    def test_pow_domain_rejected_under_angle_key(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'pow-domain.bin';pow_domain.write(p)
            with self.assertRaises(ref.trig.Refused):ref.check_domain(p)
            ref.check_domain(p,'pow_domain')

class HypotDomainTests(unittest.TestCase):
    def test_hypot_generator_hash_and_order(self):
        fixed=hypot_domain.fixed()
        self.assertEqual(fixed,sorted(set(fixed)))
        self.assertEqual(len(fixed),ref.load()['hypot_domain']['fixed_count'])
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(hypot_domain.write(Path(d)/'hypot-domain.bin'),ref.load()['hypot_domain'])
    def test_hypot_source_branches_are_present(self):
        fixed=set(hypot_domain.fixed())
        b,S=domain.bits,domain.SIGN
        inf,large,tiny=hypot_domain.INF,hypot_domain.LARGE,hypot_domain.TINY
        # Non-finite branch: infinity against every NaN kind, both orders and signs,
        # and every quiet/signalling NaN order (x + y operand selection).
        for n in hypot_domain.QNANS+hypot_domain.SNANS:
            for pair in [(inf,n),(n,inf|S),(n|S,b(1.0)),(b(1.0),n)]: self.assertIn(pair,fixed)
        for q in hypot_domain.QNANS:
            for s in hypot_domain.SNANS: self.assertIn((q,s),fixed); self.assertIn((s|S,q),fixed)
        # Scaling thresholds and neighbours, the 0x1p-54 shortcut, overflow, zeros.
        for pair in [(large,b(1.0)),(large+1,b(1.0)),(large-1,b(1.0)),(b(1.0),tiny),(b(1.0),tiny-1),
                     (tiny,tiny),(b(1.0),b(2.0**-54)),(b(1.0),b(2.0**-54)+1),
                     (hypot_domain.MAX_FINITE,hypot_domain.MAX_FINITE),(1,1),(0,S)]:
            self.assertIn(pair,fixed)
        # Exact triples from the smallest subnormal to huge scales.
        self.assertIn((3,4),fixed)
        self.assertIn((hypot_domain.exact_bits(3,1000),hypot_domain.exact_bits(4,1000)),fixed)
    def test_hypot_domain_rejected_under_other_keys(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'hypot-domain.bin';hypot_domain.write(p)
            with self.assertRaises(ref.trig.Refused):ref.check_domain(p,'pow_domain')
            ref.check_domain(p,'hypot_domain')

class SourceTransportTests(unittest.TestCase):
    def test_mirror_fallback_keeps_hash_mandatory(self):
        import check_sources as source
        from unittest.mock import patch
        import hashlib,io,urllib.error
        data=b'pinned source'
        spec={'mirrors':['https://mirror.invalid/source'],'url':'https://archive.invalid/source',
              'size':len(data),'sha256':hashlib.sha256(data).hexdigest()}
        denied=urllib.error.HTTPError(spec['mirrors'][0],403,'Forbidden',None,None)
        with tempfile.TemporaryDirectory() as d:
            archive=Path(d)/'source'
            with patch.object(source.urllib.request,'urlopen',side_effect=[denied,io.BytesIO(data)]):
                self.assertEqual(source.fetch_archive(spec,archive),spec['url'])
            with patch.object(source.urllib.request,'urlopen',return_value=io.BytesIO(b'wrong bytes')):
                with self.assertRaisesRegex(ValueError,'archive hash'):source.fetch_archive(spec,archive)

if __name__=='__main__':unittest.main()
