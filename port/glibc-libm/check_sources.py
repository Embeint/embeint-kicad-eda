#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check all imported coefficient bits against the hash-pinned original sources.

Does not rewrite Rust or expected results. --source accepts an extracted glibc-2.39
root; otherwise downloads the pinned orig tarball, verifies it before extraction,
and reads only the source files listed in source.json.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import tarfile
import tempfile
import urllib.request
import urllib.error
import time

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
RUST=ROOT/'rust/kicad-math/src/glibc_libm'

def need(ok,what):
    if not ok: raise ValueError(what)
def words(text): return [int(s,16) for s in re.findall(r'0[xX]([a-fA-F0-9]+)',text)]
def doubles(text): return [int(s,16) for s in re.findall(r'f64::from_bits\(0x([a-fA-F0-9_]+)\)',text)]
def bits(x): return struct.unpack('>Q',struct.pack('>d',float(x)))[0]
def check(get):
    manifest=json.loads((HERE/'source.json').read_text())
    src={}
    for path,digest in manifest['files'].items():
        b=get(path);need(hashlib.sha256(b).hexdigest()==digest,f'source hash {path}');src[Path(path).name]=b.decode()
    for name,out in [('uatan.tbl','atan_table.rs'),('utan.tbl','tan_table.rs'),('asincos.tbl','asin_table.rs')]:
        text=src[name].split('#else')[0]
        text=text[text.index('='):]
        v=words(text);want=[a<<32|b for a,b in zip(v[::2],v[1::2])]
        actual=doubles((RUST/out).read_text())
        need(actual[:len(want)]==want,f'coefficient bits {name}')
        if name=='asincos.tbl':
            v=[bits(x) for x in src['root.tbl'].split('= {')[1].split('}')[0].split(',') if x.strip()]
            need(actual[len(want):]==v,'root.tbl bits')
        else: need(len(actual)==len(want),f'table size {name}')
    for source,file,names in [('atnat.h','atan.rs',['d3','d5','d7','d9','d11','d13','hpi','hpi1']),
                              ('atnat2.h','atan.rs',['opi','opi1']),
                              ('utan.h','tan.rs',['d3','d5','d7','d9','d11','e0','e1','g2'])]:
        text=src[source].split('#else')[0];rust=(RUST/file).read_text()
        for name in names:
            m=re.search(r'\b'+name+r'\s*=\s*\{\{\s*(0[xX][a-fA-F0-9]+),\s*(0[xX][a-fA-F0-9]+)',text)
            need(m is not None,f'source constant {name}')
            want=int(m[1],16)<<32|int(m[2],16)
            actual=re.search(r'const '+name.upper()+r': f64 = f64::from_bits\(0x([a-fA-F0-9_]+)\)',rust)
            need(actual is not None and int(actual[1],16)==want,f'constant {name}')
    rust=(RUST/'asin.rs').read_text()
    for name,v in re.findall(r'\b(f[1-6]|rt[0-3])\s*=\s*([0-9.E+-]+)',src['uasncs.h']):
        actual=re.search(r'const '+name.upper()+r': f64 = f64::from_bits\(0x([a-fA-F0-9_]+)\)',rust)
        need(actual is not None and int(actual[1],16)==bits(v),f'constant {name}')
    check_pow(src['e_pow_log_data.c'],src['e_exp_data.c'],(RUST/'pow_table.rs').read_text())
    return manifest

def hexexpr(text,n=128):
    # Initializers are a hex float times exact small integers (N, -2, 4, -8);
    # binary64 folding of those products is exact.
    value=1.0
    for part in text.replace('N',str(n)).split('*'):
        part=part.strip()
        value*=float.fromhex(part) if 'x' in part else float(int(part))
    return value
def init_list(text,start):
    body=text.split(start,1)[1].split('}',1)[0]
    return [l.strip().rstrip(',') for l in body.splitlines()
            if l.strip() and not l.strip().startswith(('#','//'))]
def rust_array(text,name):
    body=text.split('const '+name+':',1)[1].split('= [',1)[1].split('];',1)[0]
    return [int(v.replace('_',''),16) for v in re.findall(r'0x([a-fA-F0-9_]+)',body)]
def rust_const(text,name):
    m=re.search(r'const '+name+r': f64 = f64::from_bits\(0x([a-fA-F0-9_]+)\)',text)
    need(m is not None,f'pow constant {name}');return int(m[1].replace('_',''),16)
def check_pow(log_data,exp_data,rust):
    """e_pow_log_data.c and e_exp_data.c fields read by e_pow.c (N=128)."""
    for name,field,text in [('LN2HI','ln2hi',log_data),('LN2LO','ln2lo',log_data),
                            ('INV_LN2_N','invln2N',exp_data),('SHIFT','shift',exp_data),
                            ('NEG_LN2_HI_N','negln2hiN',exp_data),('NEG_LN2_LO_N','negln2loN',exp_data)]:
        m=re.search(r'\.'+field+r'\s*=\s*([^,\n]+),',text);need(m is not None,f'pow field {field}')
        need(rust_const(rust,name)==bits(hexexpr(m[1])),f'pow constant {name}')
    need(rust_array(rust,'A')==[bits(hexexpr(v)) for v in init_list(log_data,'.poly = {')],'pow_log_data poly')
    rows=re.findall(r'^A\(([^,]+), ([^,]+), ([^)]+)\)',log_data,re.M);need(len(rows)==128,'pow_log_data rows')
    need(rust_array(rust,'POW_LOG')==[bits(float.fromhex(v)) for r in rows for v in r],'pow_log_data tab')
    need(rust_array(rust,'C')==[bits(hexexpr(v)) for v in init_list(exp_data,'.poly = {')],'exp_data poly')
    tab=[int(v,16) for v in re.findall(r'0x[0-9a-fA-F]+',exp_data.split('.tab = {',1)[1].split('#endif',1)[0])]
    need(len(tab)==256 and rust_array(rust,'EXP_TAB')==tab,'exp_data tab')

def fetch_archive(spec, archive):
    # Transport mirrors may fail (archive.ubuntu.com refused hosted CI with
    # HTTP 403). Every successful transport must still return the pinned bytes.
    errors=[]
    for url in spec.get('mirrors',[])+[spec['url']]:
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url,timeout=60) as response, archive.open('wb') as out:
                    remaining=spec['size']+1
                    while remaining:
                        b=response.read(min(1<<20,remaining))
                        if not b: break
                        out.write(b);remaining-=len(b)
            except (urllib.error.URLError,TimeoutError,ConnectionError) as error:
                errors.append(f'{url}: {error}')
                if isinstance(error,urllib.error.HTTPError) and error.code in (403,404): break
                if attempt<2: time.sleep(2**attempt)
            else:
                need(archive.stat().st_size==spec['size'] and hashlib.sha256(archive.read_bytes()).hexdigest()==spec['sha256'],'archive hash')
                return url
    raise ValueError('all pinned source transports failed: '+'; '.join(errors))

def main():
    p=argparse.ArgumentParser();p.add_argument('--source');p.add_argument('--out',required=True);a=p.parse_args()
    if a.source: result=check(lambda path:(Path(a.source)/path).read_bytes())
    else:
        m=json.loads((HERE/'source.json').read_text())['archive']
        with tempfile.TemporaryDirectory(prefix='glibc-angle-source-') as d:
            archive=Path(d)/'glibc.tar.xz'
            fetch_archive(m,archive)
            with tarfile.open(archive) as t:
                def get(path):
                    f=t.extractfile('glibc-2.39/'+path);need(f is not None,'source member');return f.read()
                result=check(get)
    out=Path(a.out);out.mkdir(parents=True,exist_ok=True)
    (out/'source-verified.json').write_text(json.dumps(result,indent=2)+'\n')
    print('all source hashes and imported table/constant bits match')
if __name__=='__main__': main()
