# SPDX-License-Identifier: GPL-3.0-or-later
"""Synthetic evidence-schema fixtures, not executed mathematical/reference proof."""
import copy
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest

ENV = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENV / 'probe'))
import artifacts
import run_probe


def write(path, value):
    path.write_text(json.dumps(value, sort_keys=True) + '\n')


def fixture(directory):
    directory.mkdir()
    (directory / 'bin').mkdir()
    for name in ('inputs.lock.json', 'candidate.toml'): shutil.copy(ENV / name, directory / name)
    lock = artifacts.load(directory / 'inputs.lock.json')
    import tomllib
    candidate = tomllib.loads((directory / 'candidate.toml').read_text())
    inputs, operands = artifacts.input_tables()
    bit = '3ff0000000000000'
    rust = {'schema': 1, 'probe': 'rust', 'libm_values': [dict(x=x, sin=bit, cos=bit,
            sin_cos_sin=bit, sin_cos_cos=bit) for x in inputs],
            'nan_arithmetic': [dict(a=a,b=b,add=bit,sub=bit,mul=bit,div=bit) for a in operands for b in operands],
            'sweep': {'seed': '4b69436164526566', 'count': 2, 'record': 'x,sin,cos as little-endian u64'}}
    primitive = copy.deepcopy(rust); primitive['probe'] = 'primitive'
    primitive['libm_values'] = [dict(x=x, sin=bit, cos=bit, sincos_sin=bit, sincos_cos=bit,
                                   compiled_sin=bit, compiled_cos=bit) for x in inputs]
    primitive['floating_point_state'] = dict(mxcsr='00001f80', x87_control='037f', round_to_nearest=True, global_locale='C')
    macros = ['__SSE2__', '__AVX__', '__FMA__', '__FAST_MATH__', '__FP_FAST_FMA', '__NO_MATH_ERRNO__']
    primitive['build'] = {k:'fixture' for k in ('compiler', 'glibcxx_release', 'glibcxx_header_date',
                            'glibc_headers', 'glibc_runtime', 'flt_eval_method', 'gcc_iec_559')}
    primitive['build']['glibc_runtime'] = candidate['toolchain']['glibc_version']
    primitive['build']['macros'] = [dict(name=n,defined=n=='__SSE2__',value='1') for n in macros]
    loaded = {p['resolved_path']: {'resolved': p['resolved_path'], 'sha256':p['sha256']}
              for p in lock['toolchain_files'] if p['resolved_path'].endswith(('.so.6', '.so.6.0.33', '.so.1'))}
    primitive['loaded_objects'] = list(loaded)
    primitive['libm_dispatch'] = []
    for name in candidate['dispatch']['checked_functions']:
        offset = next(offset for offset,names in lock['libm']['multiarch_symbols'].items()
                      if {f'__{name}_sse2', f'__ieee754_{name}_sse2'}.intersection(names))
        primitive['libm_dispatch'].append(dict(function=name,object=lock['libm']['path'],offset=offset))
    stream_inputs = ['7ff0000000000000','fff0000000000000','7ff8000000000000','fff8000000000000',
        '7ff8000000000123','0000000000000000','8000000000000000','3fb999999999999a',
        '7e37e43c8800759c','0000000000000001','40fe240c9fbe76c9','3fd5555555555555']
    primitive['stream_format'] = [dict(x=x, ostream={k:'inf' for k in ('default fixed scientific hexfloat '
        'uppercase_default uppercase_fixed uppercase_scientific uppercase_hexfloat showpos_fixed '
        'precision17_default precision0_fixed').split()}, printf={k:'inf' for k in '%f %F %e %E %g %G %a %A %.17g'.split()}) for x in stream_inputs]
    for name,data in [('rust-contract',rust),('primitive-contract',primitive),('primitive-host-default',primitive)]:
        write(directory / (name+'.json'), data)
        (directory / (name+'.sweep.bin')).write_bytes(struct.pack('<6Q',1,2,3,4,5,6))
    image = dict(schema=1,status='candidate-unqualified',lock_sha256=artifacts.sha256(directory/'inputs.lock.json'),
        candidate_sha256=lock['candidate_sha256'],toolchain_files_sha256={p['path']:p['sha256'] for p in lock['toolchain_files']},
        rust={'rustc': {'release':lock['rust']['version'],'host':'x86_64-unknown-linux-gnu'}})
    output = dict(command=['fixture'],returncode=0,stdout='fixture',stderr='')
    env = dict(schema=1,host=dict(cpu={'model name':'synthetic'},uname={}),
        loader={k:output for k in 'contract_diagnostics host_default_diagnostics contract_help host_default_help'.split()},
        environment=dict(recorded={},names=[],umask='022'), tools={k:output for k in
            'gcc g++ ld as cmake ninja python3 rustc cargo git wx-config dpkg-query'.split()},
        image_identity=image,os_release='fixture',libm=dict(multiarch_symbols_from_nm=lock['libm']['multiarch_symbols'],
            matches_lock=True),loaded_objects=loaded)
    write(directory/'environment.json', env)
    built = {}
    for name in ('primitive','rust'):
        binary = directory/'bin'/ (name+'_probe'); binary.write_bytes(b'synthetic binary')
        built[name]=dict(sha256=artifacts.sha256(binary),command=['fixture'])
    comparisons = run_probe.comparison_report(directory, {'contract':primitive,'host-default':primitive,'rust':rust}, lock,candidate)
    image.update(dpkg_query_sha256='0'*64,glibc_tunables=candidate['dispatch']['glibc_tunables'],
        compiler={**{n:dict(path='/usr/bin/gcc',version=candidate['toolchain']['gcc_version']) for n in ('gcc','g++','cc','c++')},
            'predefined_macros_sha256':'0'*64,'target_options':[], 'glibc':'glibc '+candidate['toolchain']['glibc_version'],
            'libstdcxx_runtime':candidate['toolchain']['libstdcxx_runtime_soname']})
    wanted = sorted(f"{c['name']}-{c['target']}" if c['name']=='rust-std' else c['name'] for c in lock['rust']['components'])
    closure = ['bin/rustc','bin/cargo','lib/rustlib/components','lib/rustlib/rust-installer-version',
               'lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld',
               'lib/rustlib/x86_64-unknown-linux-gnu/lib/libstd-native.rlib',
               'lib/rustlib/wasm32-unknown-unknown/lib/libstd-wasm.rlib'] + ['lib/rustlib/manifest-'+n for n in wanted]
    image['rust'].update(cargo='cargo 1.99.1',components=wanted,
        inputs={'/usr/local/'+p:dict(kind='file',sha256='0'*64,mode=420) for p in closure})
    write(directory/'environment.json',env)
    summary = dict(schema=2,status='candidate-unqualified',qualification='not asserted',sweep_count=2,
        lock_sha256=image['lock_sha256'],contract_passed=True,cpp_rust_identical=True,differences_accepted=False,
        contract_checks=[dict(check='synthetic',passed=True,detail={}) for _ in range(12)],
        comparisons=comparisons,built=built,
        runtime_object_sha256={v['resolved']:v['sha256'] for v in loaded.values()},output_sha256=artifacts.inventory(directory))
    write(directory/'summary.json',summary)


class ArtifactTest(unittest.TestCase):
    def pair(self):
        root=Path(tempfile.mkdtemp()); self.addCleanup(shutil.rmtree,root)
        for side in ('left','right'): fixture(root/side)
        return root

    def compare(self,root):
        return subprocess.run([sys.executable,str(ENV/'probe/compare_observations.py'),str(root/'left'),str(root/'right')],
                              capture_output=True,text=True)

    def rehash(self,directory):
        summary=artifacts.load(directory/'summary.json'); summary['output_sha256']=artifacts.inventory(directory)
        write(directory/'summary.json',summary)

    def test_valid_run(self):
        result=self.compare(self.pair()); self.assertEqual(result.returncode,0,result.stderr)

    def test_each_side_and_both_are_independently_validated(self):
        for sides in (('left',),('right',),('left','right')):
            for damage in ('empty','truncated','missing','extra','hash','duplicate','nonfinite','float-count',
                           'bool-count','wrong-count','wrong-schema','wrong-identity','missing-field','reordered',
                           'bits-number','stale-summary','binary','duplicate-operand'):
                with self.subTest(sides=sides,damage=damage):
                    root=self.pair()
                    for side in sides:
                        d=root/side; p=d/'primitive-contract.json'; value=artifacts.load(p)
                        if damage=='empty': (d/'primitive-contract.sweep.bin').write_bytes(b'')
                        elif damage=='truncated': (d/'primitive-contract.sweep.bin').write_bytes(bytes(23))
                        elif damage=='missing': p.unlink()
                        elif damage=='extra': (d/'extra.json').write_text('{}')
                        elif damage=='hash': p.write_text(p.read_text()+' ')
                        elif damage=='duplicate': p.write_text(p.read_text().replace('"sin":', '"sin":"0000000000000000","sin":',1))
                        elif damage=='nonfinite': p.write_text(p.read_text().replace('"count": 2','"count": NaN'))
                        elif damage=='float-count': value['sweep']['count']=2.0; write(p,value)
                        elif damage=='bool-count': value['sweep']['count']=True; write(p,value)
                        elif damage=='wrong-count': value['sweep']['count']=3; write(p,value)
                        elif damage=='wrong-schema': value['schema']=True; write(p,value)
                        elif damage=='wrong-identity': value['libm_values'][0]['x']='1111111111111111'; write(p,value)
                        elif damage=='missing-field': del value['libm_values'][0]['sin']; write(p,value)
                        elif damage=='reordered': value['libm_values'].reverse(); write(p,value)
                        elif damage=='bits-number': value['libm_values'][0]['sin']=1; write(p,value)
                        elif damage=='duplicate-operand': value['nan_arithmetic'][1]=value['nan_arithmetic'][0]; write(p,value)
                        elif damage=='binary': (d/'bin/rust_probe').write_bytes(b'mutated binary')
                        else:
                            value['libm_values'][0]['sin']='0000000000000000'; write(p,value)
                        # Schema tests also get fresh hashes: integrity alone must not certify malformed data.
                        if damage not in ('hash','empty','truncated','missing','extra'): self.rehash(d)
                    self.assertNotEqual(self.compare(root).returncode,0)

    def test_original_adversarial_minimal_fixture_is_rejected(self):
        root=self.pair()
        for side in ('left','right'):
            (root/side/'summary.json').write_text('{"contract_passed":true,"cpp_rust_identical":true}')
        self.assertNotEqual(self.compare(root).returncode,0)


if __name__=='__main__': unittest.main()
