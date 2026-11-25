#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public synthetic corpus. No expected outputs and no reference lookups."""
import argparse,json,random
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
UNIT='libs.kimath.include.geometry.rtree.application.pure.p002'
CONFIGS=[('i32.d2.m4',2,4),('i32.d2.m8',2,8),('i32.d2.m16',2,16),('i32.d2.m28',2,28),('i32.d3.m5',3,5),('i32.d3.m16',3,16),('i32.d1.m8',1,8),('i32.d1.m3',1,3),('i64.d2.m16',2,16),('i64.d3.m8',3,8)]
rng=random.Random(0xC0A5A002)
cases=[]
def op(name,a=0,**kw):return dict(op=name,a=a,**kw)
def state(a=0):return op('state',a)
def iterate(a=0,skip=0,take=8192):return op('iterate',a,skip=skip,take=take)
def search(d,a=0,stop=8192,lo=-100000,hi=100000):return op('search',a,min=[lo]*d,max=[hi]*d,stop=stop)
def entry(i,d,mode='random'):
 if mode=='equal':lo=[0]*d;hi=[0]*d
 elif mode=='touch':lo=[(i%19)*2+j%2 for j in range(d)];hi=[x+2 for x in lo]
 elif mode=='inverted':lo=[rng.randrange(-100,100) for _ in range(d)];hi=[x+rng.randrange(-20,20) for x in lo]
 elif mode=='sorted':lo=[i*3+j for j in range(d)];hi=[x+1 for x in lo]
 else:lo=[rng.randrange(-900,900) for _ in range(d)];hi=[x+rng.randrange(0,40) for x in lo]
 return dict(min=lo,max=hi,item=None if i%23==0 else i%8192)
def addcase(tree,c,name,steps):cases.append(dict(unit=UNIT,case=f'{tree}.{c}.{name}',input=dict(tree=tree,config=c,steps=steps)))
for c,d,m in CONFIGS:
 for mode in ['random','equal','touch','inverted']:
  # Several full levels and a partial final node; m28 creates >800 items.
  n=m*m+7
  es=[entry(i,d,mode) for i in range(n)]
  steps=[state(),iterate(),search(d,stop=0),op('reserve',count=n+3)]
  steps += [op('add',**e) for e in es]+[op('build'),state(),iterate(),iterate(skip=1,take=5),iterate(skip=n,take=2)]
  steps += [search(d,stop=s) for s in [0,1,3,8192]]+[search(d,lo=0,hi=0),search(d,lo=100,hi=-100),search(d,lo=9999,hi=10000)]
  steps += [op('move_new',1,src=0),state(0),state(1),iterate(0),iterate(1),op('move_assign',1,src=1),state(1)]
  steps += [op('add',0,**entry(777,d)),op('build',0),op('move_assign',0,src=1),state(0),state(1),iterate(0),op('build',0),state(0)]
  addcase('packed',c,mode,steps)
 for n in [0,1,m,m+1]:
  es=[entry(i,d,'touch') for i in range(n)]
  steps=[op('reserve',count=0)]+[op('add',**e) for e in es]+[op('build'),state(),iterate(),search(d,stop=0),op('build'),state(),op('build'),state()]
  addcase('packed',c,f'boundary-{n}',steps)
 # Incremental COW insertion includes root and repeated nonroot splits.
 for mode in ['random','equal','touch','inverted']:
  n=3*m+9; es=[entry(i,d,mode) for i in range(n)]
  steps=[state(),iterate(),search(d),op('remove',**entry(99,d))]
  steps += [op('insert',**e) for e in es]+[state(),iterate()]
  steps += [op('clone',1,src=0),state(0),state(1),op('clone',2,src=1),state(0),state(1),state(2)]
  steps += [op('insert',1,**entry(999,d)),op('remove',2,**es[0]),op('remove',1,**entry(7777,d)),state(0),state(1),state(2)]
  steps += [search(d,a,stop=s) for a in range(3) for s in [0,3,8192]]
  steps += [op('destroy',0),state(1),state(2),op('clear',1),state(1),state(2),op('insert',1,**entry(555,d)),state(1)]
  steps += [op('clone',3,src=2),op('clear',2),state(3)]
  steps += [op('remove',3,**e) for e in reversed(es)]+[state(3),iterate(3),search(d,3)]
  steps += [op('move_new',1,src=3),state(1),state(3),op('clone',3,src=3),state(3),op('new',0),op('insert',0,**entry(666,d)),op('move_assign',0,src=1),state(0),state(1),op('move_assign',0,src=0),state(0)]
  addcase('cow',c,mode,steps)
 if d>=2:
  for mode in ['random','equal','touch']:
   es=[entry(i,d,mode) for i in range(m*m+7)]
   steps=[op('insert',**entry(8000,d)),op('bulk',entries=es),state(),iterate(),op('clone',1,src=0),op('clone',2,src=0)]
   steps += [op('remove',1,**e) for e in es[::3]]+[state(0),state(1),state(2)]
   steps += [op('insert',2,**entry(i+2000,d,mode)) for i in range(m+3)]+[state(2)]
   steps += [search(d,a,stop=s) for a in range(3) for s in [0,5,8192]]
   steps += [op('bulk',0,entries=[]),state(0),state(1),state(2),op('bulk',1,entries=list(reversed(es[:m+1]))),state(1),iterate(1)]
   steps += [op('destroy',2),op('clone',1,src=1),state(1),op('clear',1),state(1)]
   addcase('cow',c,f'bulk-{mode}',steps)
# Nonempty 1D bulk is undefined, but its empty-input clearing branch is defined.
for c,d,m in CONFIGS:
 if d==1:
  addcase('cow',c,'empty-bulk',[op('insert',**entry(70,d)),op('clone',1,src=0),op('bulk',entries=[]),state(),state(1),op('bulk',1,entries=[]),state(1),iterate(),iterate(1)])
# Pathological COW depth is linear; exercise >64 levels without recursive JSON
# topology serialization obscuring that independent implementation boundary.
for c,d,m in [('i32.d1.m3',1,3),('i32.d2.m4',2,4)]:
 es=[entry(i,d,'sorted') for i in range(400)]
 steps=[op('insert',**e) for e in es]+[op('clone',1,src=0),iterate(),iterate(1),search(d,stop=0),search(d,1,stop=7),op('destroy',0),iterate(1)]
 steps += [op('remove',1,**e) for e in reversed(es)]+[state(1),iterate(1)]
 addcase('cow',c,'deep-paths',steps)
# Exact full-domain leaf operations require no area arithmetic.
for c,lo,hi in [('i32.d2.m16',-2**31,2**31-1),('i64.d2.m16',-2**63,2**63-1)]:
 e=dict(min=[lo,lo],max=[hi,hi],item=5)
 addcase('cow',c,'full-domain-leaf',[op('insert',**e),op('clone',1,src=0),state(),state(1),search(2,1,stop=0),op('remove',1,**e),state(),state(1)])
 addcase('packed',c,'full-domain-centers',[op('add',**e),op('add',min=[hi,hi],max=[hi,hi],item=9),op('add',min=[lo,lo],max=[lo,lo],item=None),op('build'),state(),iterate(),search(2,stop=1)])
# Empty clones keep their allocator ancestry alive; release long chains iteratively.
steps=[op('clone',0,src=0) for _ in range(2000)]+[state(),op('destroy',0)]
addcase('cow','i32.d2.m16','empty-chain',steps)
text=''.join(json.dumps(c,separators=(',',':'))+'\n' for c in cases)
p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
path=ROOT/'port/cases'/f'{UNIT}.jsonl'
if a.check:
 if path.read_text()!=text:raise SystemExit('corpus differs')
else:path.write_text(text)
print(f'{len(cases)} programs, {sum(len(c["input"]["steps"]) for c in cases)} steps')
