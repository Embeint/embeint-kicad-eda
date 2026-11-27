#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Public deterministic source-branch corpus, not expected answers. No private designs."""
import argparse
import json
import math
from pathlib import Path
import random
import struct

ROOT = Path(__file__).resolve().parents[2]
UNIT = 'libs.kimath.include.geometry.application.ui.p001'
SEED = 0xEDA4CF939

def bits(x):
    return struct.pack('>d', x).hex()

def generate(include_undefined=False):
    cases = []
    undefined = []
    def add(name, **v):
        cases.append({'unit': UNIT, 'case': name, 'input': v})
    add('defaults-and-constants', op='default')
    angles = [0., -0., 1e-300, -1e-300, math.ulp(0.), -math.ulp(0.),
              0.1, -0.1, 1., -1., 45., -45., 90., -90., 135., -135.,
              180., -180., 225., -225., 270., -270., 315., -315., 360., -360.,
              540., -540., 720., -720., 1080., -1080., 1e20, -1e20, 1e300, -1e300,
              float.fromhex('0x1.fffffffffffffp1023'), -float.fromhex('0x1.fffffffffffffp1023'),
              math.inf, -math.inf]
    for b in [-720., -540., -360., -180., -90., 0., 90., 180., 360., 540., 720.]:
        angles += [math.nextafter(b, -math.inf), math.nextafter(b, math.inf)]
    raw = [bits(x) for x in angles] + ['7ff8000000000042','fff8000000000042','7ff0000000000042','fff0000000000042']
    rng = random.Random(SEED)
    raw += [f'{rng.getrandbits(64):016x}' for _ in range(256)]
    for i, a in enumerate(raw):
        for unit in ['degrees', 'radians', 'tenths']:
            add(f'construct-{unit}-{i}', op='construct', a=a, unit=unit)
        for mode in ['positive','negative','90','180','720','copy']:
            add(f'normalize-{mode}-{i}', op='normalize', a=a, mode=mode)
        for kind in ['sin','cos','tan','asin','acos','atan','negate','invert','abs','radians','predicates','tenths']:
            add(f'unary-{kind}-{i}', op='unary', a=a, kind=kind)
        for digits in [-400,-4,0,3,400]:
            add(f'round-{digits}-{i}', op='round', a=a, digits=digits)
    pairs = [(x,y) for x in [0.,-0.,1.,-1.,2.,-2.,math.inf,-math.inf] for y in [0.,-0.,1.,-1.,2.,-2.,math.inf,-math.inf]]
    pairs += [(rng.uniform(-1e8,1e8), rng.uniform(-1e8,1e8)) for _ in range(384)]
    pair_bits = [(bits(x),bits(y)) for x,y in pairs]
    nan_bits = ['7ff8000000000042','fff8000000000042','7ff0000000000042','fff0000000000042']
    pair_bits += [(a,b) for a in nan_bits for b in [bits(0.),bits(-1.),bits(math.inf)] + nan_bits]
    pair_bits += [(b,a) for a in nan_bits for b in [bits(0.),bits(-1.),bits(math.inf)]]
    for i,(a,b) in enumerate(pair_bits):
        add(f'vector-{i}',op='vector',x=a,y=b)
        for kind in ['add','sub','mul','div','ratio','add_assign','sub_assign','reflect','atan2','compare','parallel','snapped']:
            add(f'binary-{kind}-{i}',op='binary',a=a,b=b,kind=kind)
        add(f'orientation-{i}',op='orientation',a=a,b=b,axis=raw[i % len(raw)])
    # Snapping non-positive and NaN steps, inclusive +/-90 parallel boundaries.
    for i,a in enumerate([-1080.,-270.,-180.,-90.,0.,90.,180.,270.,1080.]):
        for j,b in enumerate([-90.,90.,-0.,-1.,0.5,45.,math.inf,math.nan]):
            for kind in ['parallel','snapped']:
                add(f'special-{kind}-{i}-{j}',op='binary',a=bits(a),b=bits(b),kind=kind)
    for i,a in enumerate([0.,-0.,45.,-123.456789,1e-300,1e300,math.inf,-math.inf,math.nan]):
        for orientation in [False,True]:
            for fmt in ['general','fixed','scientific','hex']:
                for precision,width,fill,showpos,upper in [(6,0,' ',False,False),(0,20,'_',True,True),(-1,-1,'.',False,True),(17,28,'0',True,False)]:
                    add(f'stream-{i}-{orientation}-{fmt}-{precision}',op='stream',a=bits(a),orientation=orientation,format=fmt,precision=precision,width=width,fill=fill,showpos=showpos,uppercase=upper)
    for t in ['i32','f64']:
        for endpoint in ['default','start','end']:
            add(f'ellipse-{t}-{endpoint}',op='ellipse',type=t,endpoint=endpoint)
    for i,(pa,pb) in enumerate([(0,1),(1,0),(0,0),(1,1),(2**32-1,0),(0,2**32-1),(2**32-1,2**32-1)]):
        for ka in ['cartesian','polar']:
            for kb in ['cartesian','polar']:
                for x,y in [(10.,20.),(0.,0.),(-1.,2.),(math.inf,math.nan)]:
                    add(f'grid-{i}-{ka}-{kb}-{bits(x)}',op='grid',a=dict(priority=pa,kind=ka,x=bits(x),y=bits(y)),b=dict(priority=pb,kind=kb,x=bits(2.),y=bits(3.)))
    def poly(outline, holes=()): return dict(outline=outline, holes=list(holes))
    square = [[0,0],[100,0],[100,100],[0,100]]
    hole = [[25,25],[75,25],[75,75],[25,75]]
    polygons = [[], [poly([])], [poly([[0,0]])], [poly([[0,0],[1,1]])],
                [poly([[0,0],[50,0],[100,0]])], [poly(square)], [poly(square,[hole])],
                [poly(square[::-1],[hole[::-1]])], [poly([], [hole])],
                [poly(square, [[],[[1,1]],[[1,1],[2,2]]])],
                [poly([[0,0],[0,0],[100,0],[100,100],[0,100],[0,0]])],
                [poly([[0,0],[100,100],[0,100],[100,0]])],
                [poly([[0,0],[100,1],[0,2]])],
                [poly([[0,-2147483648],[1,-2147483647],[2,-2147483648]])],
                [poly([[0,2147483646],[1,2147483647],[2,2147483646]])],
                [poly([[0,0],[0,100],[100,100],[100,0]]),poly(hole)],
                [poly([[i*200,0],[i*200+100,0],[i*200+100,100],[i*200,100]]) for i in range(9)]]
    queries = [[-1,-1],[0,0],[1,1],[50,50],[25,25],[75,75],[0,50],[100,50],[-1,50],[101,50],[50,0],[50,100],[0,-1000],[0,1000],[2147483647,2147483647],[-2147483648,-2147483648]]
    for i,polys in enumerate(polygons):
        for j,pt in enumerate(queries):
            # Extreme-y thin contours use only non-straddling probes, avoiding
            # signed subtraction UB; separate controls exercise that refusal.
            if i in (13,14) and pt[1] not in (-2147483648,2147483647): continue
            if i not in (13,14) and pt[0] == -2147483648: continue
            for accuracy in [None,-1,0,1,2,10]:
                case=dict(unit=UNIT,case=f'poly-{i}-{j}-{accuracy}',input=dict(op='poly',builds=[dict(polygons=polys,point=pt,accuracy=accuracy)]))
                if accuracy in (2,10) and ((i==13 and j==14) or (i==14 and j==15)):
                    undefined.append(case)
                else:
                    cases.append(case)
    # Multiple builds on one index exercise clearing and early-return stale fields.
    for i,polys in enumerate(polygons[:13]):
        add(f'rebuild-{i}',op='poly',builds=[dict(polygons=[poly(square)],point=[50,50],accuracy=None),dict(polygons=polys,point=[50,50],accuracy=0),dict(polygons=[],point=[50,50],accuracy=2),dict(polygons=[poly(square,[hole])],point=[50,50],accuracy=0)])
    for i in range(400):
        x,y=rng.randrange(-100000,100000),rng.randrange(-100000,100000)
        w,h=rng.randrange(1,10000),rng.randrange(1,10000)
        points=[[x,y],[x+w,y],[x+w,y+h],[x,y+h]]
        pt=[rng.randrange(x-w,x+2*w),rng.randrange(y-h,y+2*h)]
        add(f'random-poly-{i}',op='poly',builds=[dict(polygons=[poly(points)],point=pt,accuracy=rng.choice([0,1,2,10]))])
    for i in range(512):
        # Ordered arbitrary contours intentionally include concavity and crossings;
        # this tests the index itself, independent of polygon validity repair.
        points=[[rng.randrange(-1000000,1000000),rng.randrange(-1000000,1000000)] for _ in range(rng.randrange(3,130))]
        holes=[[[rng.randrange(-10000,10000),rng.randrange(-10000,10000)] for _ in range(7)]] if i%3==0 else []
        pt=[rng.randrange(-1000000,1000000),rng.randrange(-1000000,1000000)]
        add(f'random-slanted-poly-{i}',op='poly',builds=[dict(polygons=[poly(points,holes)],point=pt,accuracy=i%4)])
    # Additive libm continuation inputs use their own seed so the merged corpus
    # above stays byte-for-byte unchanged. These are requests, never answers.
    libm_rng = random.Random(SEED ^ 0xA7A22)
    for i in range(512):
        x = libm_rng.uniform(-1e200, 1e200)
        y = libm_rng.uniform(-1e200, 1e200)
        a = libm_rng.uniform(-1., 1.)
        angle = libm_rng.uniform(-1e10, 1e10)
        add(f'libm-vector-{i}', op='vector', x=bits(x), y=bits(y))
        add(f'libm-atan2-{i}', op='binary', kind='atan2', a=bits(y), b=bits(x))
        for kind, value in [('atan', x), ('asin', a), ('acos', a), ('tan', angle)]:
            add(f'libm-{kind}-{i}', op='unary', kind=kind, a=bits(value))
        if i < 32:
            add(f'libm-round-{i}', op='round', a=bits(angle), digits=i-16)
    # Additive util.h continuations, with their own seed to preserve earlier inputs.
    util_rng=random.Random(0xB02EDA)
    for i in range(128):
        a=util_rng.uniform(-10000,10000)
        add(f'ut-wired-tenths-{i}',op='unary',a=bits(a),kind='tenths')
        add(f'ut-wired-snapped-{i}',op='binary',a=bits(a),b=bits(util_rng.choice([1.,5.,45.,90.])),kind='snapped')
        add(f'ut-wired-parallel-{i}',op='binary',a=bits(a),b=bits(a+util_rng.choice([0.,90.,180.,360.])),kind='parallel')
    # Additive EDA_ANGLE::Round continuations after the ported pow, own seed.
    # Digit counts cross pow's exact/inexact powers of ten, the subnormal,
    # underflow and overflow edges of 10^d; values cover KiROUND ties, the
    # signed zero and NaN results of 0/0, and the int range edges.
    for digits in [-1100,-1075,-400,-324,-323,-322,-308,-307,-23,-22,-17,-16,-9,-3,-2,-1,
                   0,1,2,3,9,15,16,22,23,308,309,400,1100]:
        for j, a in enumerate([0., -0., 0.5, -2.5, 1.25, -0.125, 123.456789, -359.999999,
                               2147483647.4, -2147483648.4, 1e-300, math.inf, math.nan]):
            add(f'pow-round-{digits}-{j}', op='round', a=bits(a), digits=digits)
    pow_rng=random.Random(0x506F77)
    for i in range(256):
        digits=pow_rng.randrange(-12,10)
        scale=10**abs(digits)
        a=pow_rng.uniform(-2e9,2e9)
        a=a/scale if digits>0 else a*scale
        add(f'pow-round-seeded-{i}', op='round', a=bits(a), digits=digits)
    # Contains' accuracy fallback after SEG::Square (seg.h, wired by the header unit
    # libs.kimath.include.geometry.application.ui.p003): hand-placed requests at the
    # squared-distance boundary, hole skipping, unindexed horizontal edges, clamped stripes
    # and the largest accuracy. Appended without randomness, so every case above is unchanged.
    def contains(name, polys, pt, accuracy):
        add(f'square-continuation-{name}', op='poly',
            builds=[dict(polygons=polys, point=pt, accuracy=accuracy)])
    small = [poly([[0,0],[10,0],[10,10],[0,10]])]
    for name, pt, accuracy in [('review', [11,5], 2), ('edge-exact', [12,5], 2),
                               ('edge-beyond', [13,5], 2), ('corner-within', [12,12], 3),
                               ('corner-beyond', [13,13], 3), ('corner-exact', [10,12], 2),
                               ('horizontal-unindexed', [5,11], 2),
                               ('horizontal-reached', [5,11], 6), ('left', [-2,7], 2),
                               ('below-clamped', [5,-1000], 999), ('below-exact', [5,-1000], 1000),
                               ('far-int-max', [2147483647,5], 2147483647),
                               ('inside', [5,5], 2), ('accuracy-one', [11,5], 1)]:
        contains(name, small, pt, accuracy)
    for name, pt, accuracy in [('hole-skipped', [50,50], 30), ('hole-outline', [50,50], 50),
                               ('hole-near-edge', [26,50], 10), ('hole-outside', [102,50], 2)]:
        contains(name, [poly(square,[hole])], pt, accuracy)
    nine = polygons[-1]
    for name, pt, accuracy in [('between-within', [150,50], 50), ('between-beyond', [150,50], 49),
                               ('past-last', [1702,50], 2), ('past-last-beyond', [1703,50], 2)]:
        contains(name, nine, pt, accuracy)
    for name, pt, accuracy in [('slanted', [101,1], 2), ('slanted-beyond', [104,1], 2)]:
        contains(name, [poly([[0,0],[100,1],[0,2]])], pt, accuracy)
    big = [poly([[2147483000,-2147483000],[2147483600,-2147483000],[2147483600,-2147482400],
                 [2147483000,-2147482400]])]
    for name, pt, accuracy in [('high-within', [2147483602,-2147482700], 2),
                               ('high-beyond', [2147483603,-2147482700], 2),
                               ('high-int-max', [2147483647,-2147482700], 47)]:
        contains(name, big, pt, accuracy)
    add('square-continuation-rebuild', op='poly',
        builds=[dict(polygons=small, point=[11,5], accuracy=2),
                dict(polygons=[poly(square,[hole])], point=[50,50], accuracy=30),
                dict(polygons=[], point=[11,5], accuracy=2),
                dict(polygons=small, point=[13,5], accuracy=3)])
    assert len({c['case'] for c in cases}) == len(cases)
    if include_undefined: return undefined
    return ''.join(json.dumps(c,separators=(',',':'),allow_nan=False)+'\n' for c in cases)

def controls():
    result=[]
    def add(name,outline,point):
        result.append({'request':{'unit':UNIT,'case':name,'input':dict(op='poly',builds=[dict(polygons=[dict(outline=outline,holes=[])],point=point,accuracy=0)])},'error':{'kind':'invalid_input','message':'intermediate overflow'}})
    add('diff-x-overflow',[[-2147483648,0],[2147483647,100],[0,0]],[0,50])
    add('diff-y-overflow',[[0,-2147483648],[100,2147483647],[200,-2147483648]],[0,0])
    add('point-x-overflow',[[-10,0],[-10,100],[-20,0]],[2147483647,50])
    # The Rust port refuses these at the same SquaredDistance overflow now that SEG::Square
    # (libs.kimath.include.geometry.application.ui.p003) no longer defers before it.
    for request in generate(include_undefined=True):
        result.append(dict(request=request,error=dict(kind='invalid_input',message='intermediate overflow')))
    return ''.join(json.dumps(v,separators=(',',':'))+'\n' for v in result)

def priority_controls():
    result=[]
    for name,a,b,expected in [
        ('a-negative-zero','-0','1',False),
        ('b-negative-zero','1','-0',True),
        ('a-negative-zero-uint-max','-0','4294967295',False),
        ('b-negative-zero-uint-max','4294967295','-0',True),
        ('a-float-negative-zero','-0.0','1',None),
        ('b-float-zero','1','0.0',None),
    ]:
        def grid(priority): return dict(priority=priority,kind='cartesian',x=bits(0.),y=bits(0.))
        request=dict(unit=UNIT,case='priority-'+name,input=dict(op='grid',a=grid('__A__'),b=grid('__B__')))
        line=json.dumps(request,separators=(',',':')).replace('"__A__"',a).replace('"__B__"',b)
        control=dict(request_text=line)
        if expected is None: control['error']=dict(kind='invalid_input',message='priority out of range')
        else: control['result']=expected
        result.append(control)
    return ''.join(json.dumps(c,separators=(',',':'))+'\n' for c in result)

def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
    for path,data in [(ROOT/'port/cases'/f'{UNIT}.jsonl',generate()),(ROOT/'port/oracle/fixtures/geometry-controls.jsonl',controls()),(ROOT/'port/oracle/fixtures/geometry-priority-controls.jsonl',priority_controls())]:
        if a.check:
            if path.read_text()!=data: raise SystemExit(f'stale corpus: {path}')
        else: path.write_text(data)
        print(f'{path}: {len(data.splitlines())} records')
if __name__=='__main__': main()
