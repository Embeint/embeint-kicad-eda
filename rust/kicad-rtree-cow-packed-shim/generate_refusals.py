#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Controls are separate from successful-operation comparison counts."""
import argparse,json
from pathlib import Path
UNIT='libs.kimath.include.geometry.rtree.application.pure.p002'
rows=[];replies=[]
def case(name,steps,message,tree='cow',config='i32.d2.m16'):
 rows.append(dict(unit=UNIT,case=name,input=dict(tree=tree,config=config,steps=steps)))
 replies.append(dict(unit=UNIT,case=name,error=dict(kind='invalid_input',message=message)))
ins=dict(op='insert',a=0,min=[0,0],max=[1,1],item=1)
for field,value,message in [('min',[0],'invalid coordinates'),('max',[0,1.5],'invalid coordinates'),('min',[True,0],'invalid coordinates'),('max',[2**63,0],'invalid coordinates'),('a',4,'integer out of range: a'),('item',8192,'invalid item'),('item',-1,'invalid item'),('item',True,'invalid item')]:
 case('bad-'+field+'-'+str(value),[dict(ins,**{field:value})],message)
case('unknown-config',[ins],'unknown config',config='u32.d2.m16')
case('unknown-tree',[ins],'unknown tree',tree='other')
case('empty-steps',[],'steps outside 1..4096')
case('too-many-steps',[dict(op='state',a=0)]*4097,'steps outside 1..4096')
case('unknown-op',[dict(op='bad',a=0)],'unknown op')
case('missing-op',[dict(a=0)],'expected op string')
case('extra-field',[dict(ins,extra=1)],'unexpected or missing field')
case('missing-item',[{k:v for k,v in ins.items() if k!='item'}],'unexpected or missing field')
case('missing-a',[{k:v for k,v in ins.items() if k!='a'}],'integer out of range: a')
case('bad-stop',[dict(op='search',a=0,min=[0,0],max=[1,1],stop=-1)],'integer out of range: stop')
case('bad-batch',[dict(op='bulk',a=0,entries=False)],'invalid entries')
case('large-batch',[dict(op='bulk',a=0,entries=[dict(min=[0,0],max=[0,0],item=None)]*8193)],'invalid entries')
case('bad-src',[dict(op='clone',a=1,src=4)],'integer out of range: src')
case('destroyed-tree',[dict(op='destroy',a=0),dict(op='state',a=0)],'missing tree')
case('moved-insert',[dict(op='move_new',a=1,src=0),ins],'tree invariant violation')
case('one-dimensional-bulk',[dict(op='bulk',a=0,entries=[dict(min=[0],max=[1],item=None)])],'invalid template parameters',config='i32.d1.m8')
case('negative-reserve',[dict(op='reserve',a=0,count=-1)],'integer out of range: count',tree='packed')
case('large-reserve',[dict(op='reserve',a=0,count=8193)],'integer out of range: count',tree='packed')
case('iterator-outside-data',[dict(op='iterate',a=0,skip=1,take=0)],'iterator outside data',tree='packed')
p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
for name,data in [('requests',rows),('replies',replies)]:
 path=Path(__file__).resolve().parent/'fixtures'/f'refusals.{name}.jsonl'
 text=''.join(json.dumps(r,separators=(',',':'))+'\n' for r in data)
 if a.check:
  if path.read_text()!=text:raise SystemExit('refusal fixture differs')
 else:path.write_text(text)
print(f'{len(rows)} refusal controls')
