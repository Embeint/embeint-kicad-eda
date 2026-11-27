#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fresh locked glibc reference, no goldens. Native Linux x86-64 only."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import domain
import fma_domain
import hypot_domain
import pow_domain

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
spec=importlib.util.spec_from_file_location('trig_reference',HERE.parent/'glibc-trig/glibc_trig_reference.py')
trig=importlib.util.module_from_spec(spec)
spec.loader.exec_module(trig)
FUNCTIONS=['atan','atan2','tan','asin','acos']
# pow and hypot each have their own input domain and 24-byte (x, y, f) records; fma has a
# 24-byte (x, y, z) domain and 32-byte (x, y, z, f) records.
ALL_FUNCTIONS=FUNCTIONS+['pow','hypot','fma']
SEPARATE={'pow':'pow_domain','hypot':'hypot_domain','fma':'fma_domain'}
DOMAIN_WIDTH={'fma_domain':24}
COMPARISONS=['native-debug','native-release','wasip1-debug','wasip1-release','unknown-debug','unknown-release']

def load(): return json.loads((HERE/'qualification.json').read_text())
def check_domain(path,key='domain'):
    want=load()[key]
    trig.need(os.path.getsize(path)==want['count']*DOMAIN_WIDTH.get(key,16),'domain size')
    trig.need(trig.sha256_file(path)==want['sha256'],'domain hash')

def reference(out,mode=None):
    out=Path(out).resolve();out.mkdir(parents=True,exist_ok=True)
    prefix,key,width=(mode+'-',SEPARATE[mode],24) if mode else ('','domain',56)
    inputs_width=DOMAIN_WIDTH.get(key,16)
    if mode=='fma': width=32
    domain_path,records_path=out/(prefix+'domain.bin'),out/(prefix+'reference.bin')
    check_domain(domain_path,key)
    lock=trig.load_lock(HERE.parent/'glibc-trig/reference.lock.json')
    trig.bind_lock(lock,(out/'inputs.lock.json').read_bytes())
    trig.need(platform.system()=='Linux' and platform.machine()=='x86_64','requires Linux x86-64')
    with tempfile.TemporaryDirectory(prefix='angle-reference-') as work:
        work=Path(work);deb=work/'libc6.deb'
        url=trig.fetch_package(lock,deb)
        root=work/'root'
        subprocess.run(['dpkg-deb','-x',str(deb),str(root)],check=True)
        trig.verify_extracted(lock,root)
        libdir=(root/lock['files']['libm']['path']).parent.resolve()
        loader=root/lock['files']['loader']['path'];probe=work/'probe'
        subprocess.run(['gcc','-O0','-std=c11','-Wall','-Wextra','-Werror','-fno-fast-math',
                        '-ffp-contract=off','-o',str(probe),str(HERE/'probe.c'),'-Wl,--no-as-needed','-lm','-ldl'],check=True)
        env={'PATH':'/usr/bin:/bin','LC_ALL':'C','GLIBC_TUNABLES':lock['dispatch']['glibc_tunables']}
        count=load()[key]['count']
        run=subprocess.run([str(loader),'--inhibit-cache','--library-path',str(libdir),str(probe),
                            str(domain_path),str(records_path),str(libdir),str(count)]
                           +([mode] if mode else []),
                           env=env,capture_output=True,text=True)
        (out/(prefix+'probe.log')).write_text(run.stdout+run.stderr)
        trig.need(run.returncode==0,f'probe failed: {run.stderr}')
        trig.need(records_path.stat().st_size==count*width,'reference size')
        with domain_path.open('rb') as inputs,records_path.open('rb') as records:
            for i in range(count):
                trig.need(records.read(width)[:inputs_width]==inputs.read(inputs_width),
                          f'reference input {i}')
        identity={'package':lock['package'],'download_url':url,'files':lock['files'],
                  'tunables':env['GLIBC_TUNABLES'],'dispatch':load()['dispatch'],
                  'probe_source_sha256':trig.sha256_file(HERE/'probe.c'),
                  'probe_binary_sha256':trig.sha256_file(probe),
                  'gcc':trig.tool_version(['gcc','--version']),
                  'host':platform.platform(),'domain':load()[key],
                  'reference_sha256':trig.sha256_file(records_path)}
        (out/(prefix+'reference-identity.json')).write_text(json.dumps(identity,indent=2)+'\n')
        print(json.dumps({'records':count,'sha256':identity['reference_sha256']}))

def report(out):
    out=Path(out);result={};ok=True
    for name in COMPARISONS:
        path=out/(name+'.log');text=path.read_text() if path.exists() else ''
        results={}
        for fn in ALL_FUNCTIONS:
            count=load()[SEPARATE.get(fn,'domain')]['count']
            hits=re.findall(rf'glibc_libm {fn}: (\d+) records, (\d+) mismatches',text)
            passed=hits==[(str(count),'0')]
            results[fn]={'status':'PASS' if passed else 'FAIL / NOT RUN','reported':hits}
            ok &= passed
        result[name]=results
    obj={'result':'PASS' if ok else 'FAIL','comparisons':result,'domain':load()['domain'],
         'pow_domain':load()['pow_domain'],'hypot_domain':load()['hypot_domain'],
         'fma_domain':load()['fma_domain'],
         'sha256':{p.name:trig.sha256_file(p) for p in sorted(out.iterdir())
                   if p.is_file() and p.name!='report.json'}}
    (out/'report.json').write_text(json.dumps(obj,indent=2)+'\n')
    print(json.dumps({'result':obj['result'],'comparisons':result},indent=2))
    return ok

def write_domain(out,module,name,key):
    out=Path(out);out.mkdir(parents=True,exist_ok=True)
    actual=module.write(out/name)
    trig.need(actual==load()[key],'input generator drift')
    print(json.dumps(actual))

def main():
    p=argparse.ArgumentParser()
    p.add_argument('command',choices=['domain','reference','pow-domain','pow-reference',
                                      'hypot-domain','hypot-reference','fma-domain',
                                      'fma-reference','report','wasi-test'])
    p.add_argument('--out');a=p.parse_args()
    if a.command=='domain': write_domain(a.out,domain,'domain.bin','domain')
    elif a.command=='pow-domain': write_domain(a.out,pow_domain,'pow-domain.bin','pow_domain')
    elif a.command=='reference': reference(a.out)
    elif a.command=='hypot-domain': write_domain(a.out,hypot_domain,'hypot-domain.bin','hypot_domain')
    elif a.command=='pow-reference': reference(a.out,'pow')
    elif a.command=='hypot-reference': reference(a.out,'hypot')
    elif a.command=='fma-domain': write_domain(a.out,fma_domain,'fma-domain.bin','fma_domain')
    elif a.command=='fma-reference': reference(a.out,'fma')
    elif a.command=='report': return 0 if report(a.out) else 1
    else: print(trig.wasi_test(sys.stdin.read()))
    return 0
if __name__=='__main__':
    try: sys.exit(main())
    except trig.Refused as error:
        print(f'refused: {error}',file=sys.stderr);sys.exit(1)
