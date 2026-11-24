#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public deterministic R-tree operation sequences; no expected results."""
import argparse
import json
import random
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.geometry.rtree.application.pure.p001'
CONFIGS = [('i32.d2.m4',2,4), ('i32.d2.m16',2,16), ('i32.d2.m28',2,28),
           ('i32.d3.m5',3,5), ('i32.d1.m8',1,8), ('i64.d2.m16',2,16), ('i64.d3.m8',3,8), ('i32.d3.m3',3,3)]

def entry(i, d, pattern, rng):
    if pattern == 'equal': lo=[0]*d; hi=[0]*d
    elif pattern == 'touch': lo=[(i%11)*2]*d; hi=[x+2 for x in lo]
    elif pattern == 'grid': lo=[((i//(11**axis))%11)*7-35 for axis in range(d)]; hi=[x+1+(i%4) for x in lo]
    else: lo=[rng.randint(-1000,1000) for _ in range(d)]; hi=[x+rng.randrange(0,100) for x in lo]
    return {'min':lo, 'max':hi, 'data':i%53}

def generate():
    rows=[]
    def put(case,config,steps): rows.append({'unit':UNIT,'case':case,'input':{'config':config,'steps':steps}})
    for config,d,f in CONFIGS:
        for pattern in ['grid','equal','touch','random']:
            for n in sorted(set([0,1,f,f+1,f*f+1,257])):
                rng=random.Random(0x51A8+n*41+d*7+f)
                entries=[entry(i,d,pattern,rng) for i in range(n)]
                steps=[{'op':'state'},{'op':'nearest','point':[0]*d,'k':3}]
                steps += [dict(op='insert',**e) for e in entries]
                def observe():
                    steps.append({'op':'state'})
                    for p in [0,1,-1,10000]:
                        steps.append({'op':'query','min':[p]*d,'max':[p+4]*d,'stop':p})
                    for k in [-1,0,1,7,n+3]: steps.append({'op':'nearest','point':[0]*d,'k':k})
                    for limit in [0,1,-1]: steps.append({'op':'iter','limit':limit})
                observe()
                steps += [{'op':'move','kind':kind} for kind in ['construct','assign','self']]
                # Identity duplicates, wrong boxes (fallback), absent removals and
                # interspersed queries observe swap-last and condensation order.
                for i,e in enumerate(entries):
                    bbox={'min':[20000]*d,'max':[20001]*d} if i%3==0 else {'min':e['min'],'max':e['max']}
                    steps.append(dict(op='remove',data=e['data'],**bbox))
                    if i%43==0:steps.append({'op':'state'})
                steps.append(dict(op='remove',min=[0]*d,max=[0]*d,data=65534))
                observe()
                steps += [{'op':'bulk','entries':entries},{'op':'state'}]
                # Nonempty packed roots, including a short last group, are also
                # mutated; these are not only static bulk-load queries.
                steps += [dict(op='insert',**entry(n+j,d,'random',rng)) for j in range(f+2)]
                observe()
                # Force leaf and internal orphan reinsertions from packed trees.
                for i,e in enumerate(entries):
                    steps.append(dict(op='remove',**e))
                    if i%61==0:steps.append({'op':'state'})
                observe()
                steps += [{'op':'bulk','entries':[]},{'op':'state'},{'op':'clear'},{'op':'clear'}]
                put(f'{config}.{pattern}.{n}',config,steps)
    # Domain extremes that remain defined: these must not be excluded by a broad
    # global coordinate/box bound. Inverted boxes are defined in many operations.
    for config,d,f in CONFIGS:
        wide=config.startswith('i64'); a=-(1<<63) if wide else -(1<<31);b=-a-1
        steps=[]
        for i,p in enumerate([a,b,0]): steps.append({'op':'insert','min':[p]*d,'max':[p]*d,'data':i})
        steps += [{'op':'query','min':[a]*d,'max':[b]*d,'stop':1},{'op':'iter','limit':-1},
                  {'op':'nearest','point':[b]*d,'k':0},{'op':'move','kind':'assign'},{'op':'clear'},
                  {'op':'insert','min':[1]*d,'max':[-1]*d,'data':0}, {'op':'query','min':[0]*d,'max':[0]*d,'stop':0},
                  {'op':'remove','min':[5]*d,'max':[4]*d,'data':0}, {'op':'state'}]
        put(config+'.extremes',config,steps)
    return rows

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    path=ROOT/'port/cases'/f'{UNIT}.jsonl'
    rows=generate();data=''.join(json.dumps(r,separators=(',',':'))+'\n' for r in rows)
    if args.check:
        if path.read_text()!=data:raise SystemExit('corpus differs')
    else:path.write_text(data)
    print(f'{len(rows)} cases, {sum(len(r["input"]["steps"]) for r in rows)} operations, {len(data)} bytes')
if __name__=='__main__':main()
