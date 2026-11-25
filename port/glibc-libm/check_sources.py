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
    return manifest

def main():
    p=argparse.ArgumentParser();p.add_argument('--source');p.add_argument('--out',required=True);a=p.parse_args()
    if a.source: result=check(lambda path:(Path(a.source)/path).read_bytes())
    else:
        m=json.loads((HERE/'source.json').read_text())['archive']
        with tempfile.TemporaryDirectory(prefix='glibc-angle-source-') as d:
            archive=Path(d)/'glibc.tar.xz'
            urllib.request.urlretrieve(m['url'],archive)
            need(archive.stat().st_size==m['size'] and hashlib.sha256(archive.read_bytes()).hexdigest()==m['sha256'],'archive hash')
            with tarfile.open(archive) as t:
                def get(path):
                    f=t.extractfile('glibc-2.39/'+path);need(f is not None,'source member');return f.read()
                result=check(get)
    out=Path(a.out);out.mkdir(parents=True,exist_ok=True)
    (out/'source-verified.json').write_text(json.dumps(result,indent=2)+'\n')
    print('all source hashes and imported table/constant bits match')
if __name__=='__main__': main()
