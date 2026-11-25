#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
UNIT='libs.kimath.include.geometry.rtree.application.pure.p001'
def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
    rows=[];expected=[]
    def case(name,steps,message,config='i32.d2.m16'):
        rows.append(dict(unit=UNIT,case=name,input=dict(config=config,steps=steps)))
        expected.append(dict(unit=UNIT,case=name,error=dict(kind='invalid_input',message=message)))
    ins=dict(op='insert',a=0,min=[0,0],max=[1,1],item=1)
    for field,value,message in [('min',[0],'invalid coordinates'),('min',[0,1.5],'invalid coordinates'),
            ('min',[0,2**63],'invalid coordinates'),('min',[True,0],'invalid coordinates'),
            ('a',2,'integer out of range: a'),('item',8192,'invalid item'),('item',-1,'invalid item'),
            ('item',True,'invalid item')]:
        case('bad-'+field+'-'+str(value),[dict(ins,**{field:value})],message)
    case('unknown-config',[ins],'unknown config',config='u32.d2.m16')
    case('empty-steps',[],'steps outside 1..4096')
    case('too-many-steps',[dict(op='state',a=0)]*4097,'steps outside 1..4096')
    case('unknown-op',[dict(op='bad',a=0)],'unknown op')
    case('extra-field',[dict(ins,extra=0)],'unexpected or missing field')
    case('missing-item',[{k:v for k,v in ins.items() if k!='item'}],'unexpected or missing field')
    case('missing-a',[{k:v for k,v in ins.items() if k!='a'}],'integer out of range: a')
    case('bad-seed',[dict(op='search',a=0,min=[0,0],max=[1,1],stop=0,seed=True)],'invalid seed')
    case('bad-stop',[dict(op='search',a=0,min=[0,0],max=[1,1],stop=-1,seed=[])],'integer out of range: stop')
    case('bad-nearest-k',[dict(op='nearest',a=0,point=[0,0],k=2**31,seed=[])],'integer out of range: k')
    for config,d,low,high in [('i32.d2.m16',2,-2**31,2**31-1),('i64.d2.m16',2,-2**63,2**63-1),('i64.d3.m8',3,-2**63,2**63-1)]:
        case('distance-overflow-'+config,[dict(op='insert',a=0,min=[low]*d,max=[low]*d,item=None),
            dict(op='nearest',a=0,point=[high]*d,k=1,seed=[])],'signed arithmetic overflow',config)
        case('bulk-conversion-'+config,[dict(op='insert',a=0,min=[0]*d,max=[1]*d,item=None),
            dict(op='bulk',a=0,entries=[dict(min=[high]*d,max=[low]*d,item=None)])],
            'floating conversion outside reference domain',config)
    case('tree-resource-limit',[dict(op='bulk',a=0,entries=[dict(min=[0,0],max=[0,0],item=None)]*8192),ins],'resource limit')
    case('bulk-entry-limit',[dict(op='bulk',a=0,entries=[dict(min=[0,0],max=[0,0],item=None)]*8193)],'invalid entries')
    for name,data in [('refusals.requests.jsonl',rows),('refusals.replies.jsonl',expected)]:
        path=ROOT/'rust/kicad-rtree-shim/fixtures'/name
        text=''.join(json.dumps(r,separators=(',',':'))+'\n' for r in data)
        if a.check:assert path.read_text()==text
        else:path.write_text(text)
    print(f'{len(rows)} refusal controls')
if __name__=='__main__':main()
