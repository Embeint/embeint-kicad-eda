#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Separate malformed/undefined conversion controls, followed by valid requests.
Known signed-overflow controls run only on Rust behaviour tests, never on C++.
"""
import argparse
import copy
import json
import subprocess
import sys
UNIT='libs.kimath.include.geometry.rtree.application.pure.p001'
valid={'unit':UNIT,'case':'valid','input':{'config':'i32.d2.m4','steps':[{'op':'insert','min':[0,0],'max':[1,1],'data':1},{'op':'state'}]}}

def controls():
    out=[]
    def add(fn):
        v=copy.deepcopy(valid);fn(v);out.append(json.dumps(v))
    add(lambda v:v['input'].update(extra=1))
    add(lambda v:v['input'].update(config='other'))
    add(lambda v:v['input'].update(config=1))
    add(lambda v:v['input'].update(steps=[]))
    add(lambda v:v['input'].update(steps={}))
    add(lambda v:v['input']['steps'][0].update(op='unknown'))
    add(lambda v:v['input']['steps'][0].update(op=1))
    add(lambda v:v['input']['steps'][0].update(extra=1))
    for field in ['min','max','data']:
        add(lambda v,f=field:v['input']['steps'][0].pop(f))
    for field in ['min','max']:
        for value in [[],[1],[1,2,3],[True,0],[1.5,0],['1',0],[2147483648,0],[-2147483649,0],None]:
            add(lambda v,f=field,x=value:v['input']['steps'][0].update({f:x}))
    for value in [-2,65536,True,1.5,'1',None]:
        add(lambda v,x=value:v['input']['steps'][0].update(data=x))
    for op,fields in [('iter',{'limit':2147483648}),('nearest',{'point':[0,0],'k':2147483648}),('query',{'min':[0,0],'max':[1,1],'stop':1.5}),('move',{'kind':'other'}),('move',{'kind':1}),('bulk',{'entries':None}),('bulk',{'entries':[{'min':[10,10],'max':[-10,-10],'data':1}]})]:
        add(lambda v,o=op,f=fields:v['input'].update(steps=[dict(op=o,**f)]))
    add(lambda v:v['input'].update(steps=[{'op':'clear'}]*4097))
    add(lambda v:v['input'].update(steps=[{'op':'bulk','entries':[{'min':[0,0],'max':[1,1],'data':0}]*65537}]))
    out += ['{"unit":"x","unit":"y","case":"c","input":{}}', '{', '[]',
            json.dumps(dict(valid,case='',extra=1)),json.dumps(dict(valid,unit='unknown'))]
    return out

def main():
    bad=controls();requests=[]
    for i,line in enumerate(bad):
        requests.append(line);v=copy.deepcopy(valid);v['case']=f'after-{i}';requests.append(json.dumps(v))
    parser=argparse.ArgumentParser();parser.add_argument('binary');parser.add_argument('--reference');args=parser.parse_args()
    p=subprocess.run([args.binary],input='\n'.join(requests)+'\n',text=True,capture_output=True,check=True)
    replies=[json.loads(line) for line in p.stdout.splitlines()]
    assert len(replies)==len(requests),(len(replies),len(requests),p.stderr)
    for i,r in enumerate(replies):
        if i%2==0:assert 'error' in r and r['error']['kind'] in ['invalid_request','invalid_input','unknown_unit'],(i,r)
        else:assert 'result' in r and r['case']==f'after-{i//2}',(i,r)
    if args.reference:
        ref=subprocess.run([args.reference],input='\n'.join(requests)+'\n',text=True,capture_output=True,check=True)
        expected=[json.loads(line) for line in ref.stdout.splitlines()]
        assert len(expected)==len(replies),(len(expected),len(replies))
        for i,(actual,wanted) in enumerate(zip(replies,expected)):
            assert actual==wanted,(i,actual,wanted)
    print(json.dumps({'probes':len(bad),'requests':len(replies),'failures':0,'exact_reference':bool(args.reference)}))
    if p.stderr:print(p.stderr,file=sys.stderr)
if __name__=='__main__':main()
