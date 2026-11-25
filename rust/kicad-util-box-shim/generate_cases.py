#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fixed-seed public inputs only. Undefined controls remain outside the live corpus."""
import argparse,json,random,struct
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
UNIT='libs.kimath.include.math.application.ui.p001'
def bits(v,t='f64'):return struct.pack('>d' if t=='f64' else '>f',v).hex()
def generate():
    rng=random.Random(0x4cf939778)
    cases=[]
    def add(name,**v):cases.append(dict(unit=UNIT,case=name,input=v))
    ranges={'i8':(-128,127),'u8':(0,255),'i16':(-32768,32767),'u16':(0,65535),'i32':(-2**31,2**31-1),'u32':(0,2**32-1),'i64':(-2**63,2**63-1),'u64':(0,2**64-1)}
    for t,(lo,hi) in ranges.items():
        values=sorted(set([lo,lo+1,0,1,hi-1,hi]+[rng.randrange(lo,hi+1) for _ in range(20)]))
        for i,a in enumerate(values):
            for target in list(ranges)[:-1]:
                for quiet in [False,True]:add(f'round-{t}-{i}-{target}-{quiet}',op='round',type=t,a=a,target=target,quiet=quiet)
            add(f'sign-{t}-{i}',op='sign',type=t,a=a)
            add(f'isqrt-{t}-{i}',op='isqrt',type=t,a=a)
            if a>=0:add(f'ct_sqrt-{t}-{i}',op='ct_sqrt',type=t,a=a)
            add(f'clamp-{t}-{i}',op='clamp',type=t,a=a,lo=0,hi=hi//2)
        for i in range(20):
            a=rng.randrange(0,hi+1);add(f'ct-helper-{t}-{i}',op='ct_sqrt_helper',type=t,a=a,lo=0,hi=a//2+1)
    for t in ['f32','f64']:
        vals=[-0.,0.,-1.,1.,-.5,.5,-1.5,1.5,float('inf'),-float('inf'),float('nan'),2**-126 if t=='f32' else 2**-1022]
        for target,(lo,hi) in list(ranges.items())[:-1]:
            for x in [float(lo)-.5,float(lo),float(hi),float(hi)+.5]:vals.extend([x])
        vals += [rng.uniform(-1e6,1e6) for _ in range(50)]
        vals=[bits(x,t) for x in vals]
        vals += ['7f800123','ffc00123'] if t=='f32' else ['7ff0000000000123','fff8000000000123']
        for i,a in enumerate(vals):
            for target in list(ranges)[:-1]:
                for quiet in [False,True]:add(f'round-{t}-{i}-{target}-{quiet}',op='round',type=t,a=a,target=target,quiet=quiet)
            add(f'sign-{t}-{i}',op='sign',type=t,a=a)
            add(f'clamp-{t}-{i}',op='clamp',type=t,a=a,lo=bits(-1.,t),hi=bits(1.,t))
            for j,b in enumerate([bits(0.,t),a,bits(1.,t),bits(float('inf'),t)]):
                for e in [None,bits(0.,t),bits(-1.,t),bits(2**(-23 if t=='f32' else -52),t)]:add(f'equals-{t}-{i}-{j}-{e}',op='equals',type=t,a=a,b=b,epsilon=e)
        # Float rescale includes zero denominator, invalid, under/overflow and one payload NaN.
        for i in range(120):
            aa=rng.choice(vals);bb=bits(rng.uniform(-100,100),t);dd=bits(rng.choice([0.,1.,-1.,1e-20]),t)
            add(f'rescale-{t}-{i}',op='rescale',type=t,a=aa,b=bb,d=dd)
    for i,a in enumerate([-2**63,-2**31-1,-2**31,-1,0,2**31-1,2**31,2**63-1]):
        add(f'cast-{i}',op='checked_cast',type='i64',a=a);add(f'abs-{i}',op='abs',type='i64',a=a)
    h=[-2**63,-3037000500,-3037000499,-1,0,1,2,3,4,3037000499,3037000500,2**63-1]
    for i,a in enumerate(h):
        for j,b in enumerate(h):add(f'hypot-{i}-{j}',op='hypot',type='i64',a=a,b=b)
    for t in ['i32','i64']:
        lo,hi=ranges[t]
        for i in range(120):add(f'rescale-{t}-{i}',op='rescale',type=t,a=rng.randrange(lo,hi+1),b=rng.randrange(lo,hi+1),d=rng.choice([-1,1,2,-2,hi,lo]))
    for t in ['i32','i64','f64']:
        cv=lambda n:bits(float(n)) if t=='f64' else n
        pt=lambda p:list(map(cv,p))
        box=lambda mode='new',p=(10,20),s=(30,40):dict(mode=mode,pos=pt(p),size=pt(s))
        constructors=[dict(mode='default'),dict(mode='maximum'),dict(mode='at',pos=pt([1,-1])),box(),box(p=(10,20),s=(-30,-40)),box('raw',s=(-30,-40)),box('raw',s=(0,0)),box('center'),dict(mode='corners',pos=pt([30,40]),end=pt([10,20])),dict(mode='compute',points=[]),dict(mode='compute',points=list(map(pt,[[30,40],[-1,5],[30,6]])))]
        for i,a in enumerate(constructors):
            # Maximum state is tested separately: float size_max has an undefined cast.
            kinds=['state','normalize','safe'] if a['mode']=='maximum' else ['state','getters','normalize','area','diagonal','squared_diagonal','safe','format']
            for k in kinds:add(f'box-{t}-ctor-{i}-{k}',op='box',type=t,kind=k,a=a)
            for k in ['merge','contains_box','intersects','intersect','distance_box','squared_distance_box','compare']:
                if a['mode']=='maximum':continue
                for j,b in enumerate([dict(mode='default'),box(p=(-90,-80)),box('raw',p=(30,20),s=(-2,-3)),box(p=(40,60),s=(0,0))]):add(f'box-{t}-{i}-{k}-{j}',op='box',type=t,kind=k,a=a,b=b)
            if a['mode']=='maximum':continue
            for k in ['contains','contains_xy','merge_point','nearest','farthest','distance','squared_distance','offset','offset_xy','move','with_offset','set_end','set_end_xy','set_origin','set_origin_xy']:
                for j,p in enumerate([[10,20],[40,60],[-1,-1],[0,0],[100,100]]):add(f'box-{t}-{i}-{k}-{j}',op='box',type=t,kind=k,a=a,p=pt(p))
            for k in ['inflate','inflated','set_size']:
                for j,s in enumerate([[0,0],[-3,5],[-100,-100],[100,100]]):add(f'box-{t}-{i}-{k}-{j}',op='box',type=t,kind=k,a=a,s=pt(s))
            for k in ['inflate_uniform','inflated_uniform','set_x','set_y','set_width','set_height']:add(f'box-{t}-{i}-{k}',op='box',type=t,kind=k,a=a,n=cv(-100))
            for j,angle in enumerate([0,90,180,270,360,45,89.9999999999,90.000000002]):add(f'box-{t}-{i}-rotated-{j}',op='box',type=t,kind='rotated',a=a,b=box(),angle=bits(float(angle)))
            add(f'box-{t}-{i}-rotated-bbox',op='box',type=t,kind='rotated_bbox',a=a,p=[1,2],angle=bits(0.))
            for j,(p,q) in enumerate([([10,20],[0,0]),([-100,-100],[100,100]),([100,100],[100,100])]):add(f'box-{t}-{i}-segment-{j}',op='box',type=t,kind='segment',a=a,p=pt(p),q=pt(q))
            for k in ['circle','circle_edge']:
                for j,(p,r,w) in enumerate([([0,0],0,0),([10,20],1,2),([0,0],100,2),([100,100],-10,3),([25,40],20,-2)]):
                    v=dict(op='box',type=t,kind=k,a=a,p=pt(p),radius=r)
                    if k=='circle_edge':v['width']=w
                    add(f'box-{t}-{i}-{k}-{j}',**v)
            for j,points in enumerate([[],[[1,2]],[[1,2],[-5,20],[100,50]]]):add(f'box-{t}-{i}-compute-{j}',op='box',type=t,kind='compute',a=a,points=list(map(pt,points)))
        for i in range(80):
            a=box('raw',[rng.randrange(-100000,100000) for _ in range(2)],[rng.randrange(-1000,1000) for _ in range(2)])
            for k in ['inflate','inflated']:add(f'box-random-{t}-{i}-{k}',op='box',type=t,kind=k,a=a,s=pt([rng.randrange(-2000,2000) for _ in range(2)]))
        lo,hi=(-2**31,2**31-1) if t=='i32' else (-2**63,2**63-1)
        if t!='f64':
            # Defined long->int modulo cast and checked-int64 arithmetic, not UB.
            for i,(p,s) in enumerate([([lo,lo],[0,1]),([hi-1,hi-1],[1,1]),([0,0],[hi,hi]),([hi,hi],[-1,-1])]):
                a=box('raw',p,s)
                for k in ['state','getters','normalize','squared_diagonal','diagonal','safe']:add(f'box-extreme-{t}-{i}-{k}',op='box',type=t,kind=k,a=a)
        else:
            for i,n in enumerate([-0.,float('inf'),-float('inf'),float('nan'),1e300,1e-300]):
                a=box('raw',[n,n],[0,0])
                for k in ['state','normalize','contains','area','safe']:
                    v=dict(op='box',type=t,kind=k,a=a)
                    if k=='contains':v['p']=pt([0,0])
                    add(f'box-fp-{i}-{k}',**v)
    base=dict(mode='raw',pos=[1,2],size=[3,4])
    for i,edges in enumerate([[0,0,0,0],[-5,-6,7,8],[7,8,-5,-6],[-2**31,-2**31,2**31-1,2**31-1]]):
        for k in ['default','center','diameter','box','merge','point','xy','corners','segment','from_box','intersects','arc']:
            b=[3,4] if k in ['corners','segment','arc'] else base
            add(f'minmax-{i}-{k}',op='minmax',kind=k,a=edges,b=b,p=[1,2])
    for i in range(300):
        edges=[rng.randrange(-2**31,2**31) for _ in range(4)]
        add(f'minmax-hypot-{i}',op='minmax',kind='diameter',a=edges,b=base,p=[0,0])
    for t in ['i32','i64','f64']:
        cv=lambda x:bits(float(x)) if t=='f64' else x
        for i,(p,s) in enumerate([([1,2],[-3,-4]),([-2**31,-2**31],[2**32-1,2**32-1]),([2**31-1,2**31-1],[100,100])]):
            if t=='i32' and max(s)>2**31-1:continue
            add(f'safe-parts-{t}-{i}',op='safe_parts',type=t,pos=[cv(x) for x in p],size=[cv(x) for x in s])
    for op in ['safe_box','round_box']:
        for i in range(20):
            a=dict(mode='raw',pos=[bits(rng.uniform(-1e10,1e10)) for _ in range(2)],size=[bits(rng.uniform(-1e10,1e10)) for _ in range(2)])
            add(f'{op}-{i}',op=op,a=a)
    assert len({c['case'] for c in cases})==len(cases)
    return ''.join(json.dumps(c,separators=(',',':'),allow_nan=False)+'\n' for c in cases)
def controls():
    cs=[]
    def add(name,**v):cs.append(dict(request=dict(unit=UNIT,case=name,input=v),error=dict(kind='invalid_input',message='undefined box/util operation')))
    add('ub-offset',op='box',type='i32',kind='offset',a=dict(mode='raw',pos=[2**31-1,0],size=[0,0]),p=[1,0])
    add('ub-long-add',op='box',type='i64',kind='state',a=dict(mode='new',pos=[2**63-1,0],size=[1,0]))
    add('ub-corners',op='box',type='i32',kind='state',a=dict(mode='corners',pos=[-2**31,0],end=[2**31-1,0]))
    add('ub-negate',op='box',type='i64',kind='normalize',a=dict(mode='raw',pos=[0,0],size=[-2**63,0]))
    add('ub-area',op='box',type='i64',kind='area',a=dict(mode='raw',pos=[0,0],size=[2**63-1,2]))
    add('ub-distance',op='box',type='i64',kind='squared_distance',a=dict(mode='raw',pos=[0,0],size=[0,0]),p=[3037000500,0])
    add('ub-radius',op='box',type='i32',kind='circle_edge',a=dict(mode='new',pos=[0,0],size=[1,1]),p=[0,0],radius=2**31-1,width=2)
    add('ub-nan-cast',op='safe_box',a=dict(mode='raw',pos=[bits(float('nan')),bits(0.)],size=[bits(0.),bits(0.)]))
    add('ub-rescale-zero',op='rescale',type='i32',a=1,b=1,d=0)
    add('ub-inflate-delta',op='box',type='i64',kind='inflate_uniform',a=dict(mode='raw',pos=[0,0],size=[0,0]),n=2**63-1)
    return ''.join(json.dumps(c,separators=(',',':'))+'\n' for c in cs)
def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
    for path,data in [(ROOT/'port/cases'/f'{UNIT}.jsonl',generate()),(ROOT/'port/oracle/fixtures/util-box-controls.jsonl',controls())]:
        if a.check:
            if path.read_text()!=data:raise SystemExit(f'stale corpus: {path}')
        else:path.write_text(data)
        print(f'{path}: {len(data.splitlines())} records')
if __name__=='__main__':main()
