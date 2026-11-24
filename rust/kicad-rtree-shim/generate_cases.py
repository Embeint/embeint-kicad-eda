#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public deterministic topology/lifecycle corpus; seed 20261124. No expected outputs."""
import argparse
import json
import random
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
UNIT='libs.kimath.include.geometry.rtree.application.pure.p001'
CONFIGS=[('i32.d2.m4',2,4),('i32.d2.m16',2,16),('i32.d2.m28',2,28),
         ('i32.d3.m5',3,5),('i32.d3.m16',3,16),('i32.d1.m8',1,8),
         ('i64.d2.m16',2,16),('i64.d3.m8',3,8),('i32.d1.m3',1,3),('i32.d2.m8',2,8)]
def op(name,a=0,**kw):return dict(op=name,a=a,**kw)
def entry(lo,hi,item):return dict(min=lo,max=hi,item=item)
def observe(d):
    return [op('layout'),op('state'),op('iterate',skip=0,take=8192),op('iterate',skip=3,take=5),
            op('overlap',min=[-50]*d,max=[50]*d,take=8192),
            op('overlap',min=[3000]*d,max=[3001]*d,take=0),
            op('search',min=[-1000]*d,max=[1000]*d,stop=8192,seed=[8191,None]),
            op('search',min=[0]*d,max=[0]*d,stop=0,seed=[]),
            op('search',min=[-1000]*d,max=[1000]*d,stop=7,seed=[8191]),
            *[op('nearest',point=[p]*d,k=k,seed=[8191,None]) for p,k in [(0,0),(0,-1),(0,1),(0,12),(17,8192)]],
            op('overlap',min=[20]*d,max=[-20]*d,take=8192)]
def cases():
    rng=random.Random(20261124)
    for c,d,m in CONFIGS:
        for n in [0,1,m-1,m,m+1,m*m+1]:
            es=[entry([i%17]*d,[i%17+3]*d,i) for i in range(n)]
            yield f'{c}-bulk-{n}',c,[op('insert',**entry([-90]*d,[-80]*d,8191)),op('bulk',entries=es),*observe(d),op('bulk',entries=[]),op('state')]
        for family in ['equal','touch','grid','random']:
            es=[];steps=[]
            for i in range(max(180,m*m+17)):
                if family=='equal':lo=[0]*d;hi=[10]*d
                elif family=='touch':lo=[i%19]*d;hi=[i%19+1]*d
                elif family=='grid':lo=[(i*(97+axis*29))%997-500 for axis in range(d)];hi=[x+11 for x in lo]
                else:lo=[rng.randrange(-500,500) for _ in range(d)];hi=[x+rng.randrange(0,25) for x in lo]
                e=entry(lo,hi,None if i%31==0 else i);es.append(e);steps.append(op('insert',**e))
                if i in [m-1,m,m*m,m*m+1]:steps.append(op('state'))
            steps+=observe(d)
            order=list(range(len(es)));rng.shuffle(order)
            for j,i in enumerate(order):
                e=es[i]
                # Miss then stored-insertion fallback, null and duplicate item values.
                steps.append(op('remove',**(dict(e,min=[20000]*d,max=[20001]*d) if j%2==0 else e)))
                if j%37==0:steps+= [op('state'),op('nearest',point=[0]*d,k=9,seed=[8191])]
            steps+=observe(d)+[op('remove',**entry([0]*d,[0]*d,8191)),op('insert',**entry([1]*d,[2]*d,7)),op('clear'),op('state')]
            yield f'{c}-{family}-insert-remove',c,steps
        for kind in ['move_new','move_assign']:
            steps=[op('bulk',entries=[entry([i]*d,[i+1]*d,i) for i in range(m*m+2)]),op('insert',a=1,**entry([-1]*d,[0]*d,8191)),
                   op(kind,a=1,src=0),op('state'),op('state',a=1),op('insert',**entry([7]*d,[8]*d,None)),
                   op(kind,a=0,src=1),op('state'),op('state',a=1),op('move_assign',a=0,src=0),*observe(d)]
            yield f'{c}-{kind}',c,steps
        # Extreme coordinates valid without area evaluation; invalid query bounds
        # are accepted rather than normalized or broadly rejected.
        low=-(2**31) if c.startswith('i32') else -(2**63)
        high=2**31-1 if c.startswith('i32') else 2**63-1
        steps=[op('insert',**entry([low]*d,[high]*d,1)),op('state'),
               op('nearest',point=[0]*d,k=1,seed=[]),
               op('search',min=[0]*d,max=[0]*d,stop=8192,seed=[]),
               op('overlap',min=[high]*d,max=[low]*d,take=8192),op('clear'),op('state')]
        yield f'{c}-full-coordinate-domain',c,steps
        yield f'{c}-inverted-leaf',c,[op('insert',**entry([8]*d,[2]*d,3)),op('state'),op('search',min=[0]*d,max=[10]*d,stop=8192,seed=[]),op('nearest',point=[5]*d,k=1,seed=[]),op('clear')]
    for c,d,m in CONFIGS:
        low=-(2**31) if c.startswith('i32') else -(2**63)
        high=2**31-1 if c.startswith('i32') else 2**63-1
        es=[entry([low]*d,[high]*d,None if i%7==0 else i) for i in range(m*m+1)]
        yield f'{c}-bulk-full-domain',c,[op('bulk',entries=es),op('state'),op('iterate',skip=0,take=8192),
            op('search',min=[0]*d,max=[0]*d,stop=8192,seed=[]),op('nearest',point=[0]*d,k=8192,seed=[8191])]
        if c.startswith('i64'):
            for center in [2**53,-(2**53),high-16]:
                es=[entry([center+i]*d,[center+i+1]*d,i) for i in range(8)]
                yield f'{c}-bulk-rounded-center-{center}',c,[op('bulk',entries=es),op('state'),
                    op('iterate',skip=0,take=8192),op('nearest',point=[center]*d,k=8192,seed=[])]
            # This inverted entry normalizes to a negative fraction in (-1,0).
            # Truncation to uint32 is defined as zero, so rejecting scaled < 0
            # would incorrectly drop a valid reference input.
            es=[entry([0]*d,[-1]*d,1),entry([0]*d,[high]*d,2)]
            yield f'{c}-bulk-negative-fraction-truncates-zero',c,[op('bulk',entries=es),op('state'),op('iterate',skip=0,take=8192)]
    d=2
    es=[entry([(i*97)%997-500,(i*43)%787-400],[(i*97)%997-491,(i*43)%787-387],i) for i in range(800)]
    steps=[op('insert',**e) for e in es]+observe(d)
    for i in range(0,800,2):steps.append(op('remove',**dict(es[i],min=[10000]*d,max=[10001]*d)))
    steps+=observe(d)
    for i in range(1,800,2):steps.append(op('remove',**es[i]))
    steps+=observe(d)
    yield 'i32.d2.m8-observed-multilevel-condensation','i32.d2.m8',steps
    # Bulk equal-center pairs retain input order via (curve,index), not item order.
    for c,d,m in CONFIGS:
        for n in [17,33,101]:
            es=[entry([-(i%7)]*d,[i%7]*d,1000-i) for i in range(n)]
            yield f'{c}-bulk-equal-centers-{n}',c,[op('bulk',entries=es),*observe(d)]
    for c,d,m in CONFIGS:
        for seed in range(6):
            local=random.Random(2000+seed)
            live=[];steps=[]
            for j in range(500):
                if live and local.randrange(3)==0:
                    e=live.pop(local.randrange(len(live)));steps.append(op('remove',**e))
                else:
                    lo=[local.randrange(-30,31) for _ in range(d)];e=entry(lo,[x+local.randrange(5) for x in lo],j)
                    live.append(e);steps.append(op('insert',**e))
                if j%50==0:steps+=observe(d)
            steps+=observe(d)+[op('clear'),op('state')]
            yield f'{c}-mixed-seed-{seed}',c,steps

def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
    rows=[dict(unit=UNIT,case=name,input=dict(config=c,steps=s)) for name,c,s in cases()]
    path=ROOT/'port/cases'/f'{UNIT}.jsonl'
    text=''.join(json.dumps(r,separators=(',',':'))+'\n' for r in rows)
    if a.check:assert path.read_text()==text
    else:path.write_text(text)
    print(f'{len(rows)} cases, {sum(len(r["input"]["steps"]) for r in rows)} steps')
if __name__=='__main__':main()
