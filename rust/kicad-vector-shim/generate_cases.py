#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic public synthetic inputs; no expected answers or oracle output lookup."""
import json
from pathlib import Path
import random
import struct
ROOT=Path(__file__).resolve().parents[2]
UNIT='libs.kimath.include.math.application.ui.p002'
CASES=[]; REFUSALS=[]
def bits(x): return struct.pack('>d',x).hex()
def case(name,dim,typ,op,a,*,refuse=False,**kw):
    r={'unit':UNIT,'case':name,'input':dict(dim=dim,type=typ,op=op,a=a,**kw)}
    (REFUSALS if refuse else CASES).append(r)
def floats(a): return [bits(x) for x in a]
for dim,types in [(2,['i32','i64','f64']),(3,['i32','u32','f64'])]:
 for typ in types:
    prefix=f'{dim}-{typ}'
    a=[2,3,4][:dim]; b=[4,2,3][:dim]; scalar=2
    if typ=='f64': a=floats(a);b=floats(b);scalar=bits(2)
    for op in ['new','copy','assign','default','traits','norm','stream']+(['squared_norm','neg','perpendicular','format'] if dim==2 else ['normalize']):
        case(f'{prefix}-{op}',dim,typ,op,a)
    for op in ['compare','dot','cross']+(['add','sub','operator_dot','squared_distance','distance','add_assign','sub_assign','mul_assign'] if dim==2 else []):
        case(f'{prefix}-{op}',dim,typ,op,a,b=b)
    for op in (['mul_scalar','scalar_mul','add_scalar','sub_scalar','resize','add_assign_scalar','sub_assign_scalar','mul_assign_scalar'] if dim==2 else ['set_all','mul_assign_scalar','div_assign_scalar']):
        case(f'{prefix}-{op}',dim,typ,op,a,scalar=scalar)
    for target in (['i32','i64','f64'] if dim==2 else ['i32','u32','f64']):
     for op in (['cast','cast_call'] if dim==2 else ['cast']): case(f'{prefix}-{op}-{target}',dim,typ,op,a,target=target)
    for alias in ('xy' if dim==2 else 'xyz'):
     for op in (['add_assign_scalar','sub_assign_scalar','mul_assign_scalar'] if dim==2 else ['mul_assign_scalar','div_assign_scalar']):
        case(f'{prefix}-{op}-alias-{alias}',dim,typ,op,a,alias=alias)
    if dim==2:
     for op in ['divide','add_float','sub_float']:
      for s in [0.0,-0.0,0.5,-0.5,1.5,-1.5,float('inf'),-float('inf'),float('nan')]:
        case(f'{prefix}-{op}-{bits(s)}',dim,typ,op,a,scalar=bits(s))
# Boundaries have named cases rather than a global coordinate bound.
for typ,lo,hi in [('i32',-2**31,2**31-1),('i64',-2**63,2**63-1)]:
 for a in [[lo,0],[hi,hi],[lo,lo],[3_037_000_499,1] if typ=='i64' else [hi,0]]:
  for op in ['new','copy','assign','norm','squared_norm','compare']:
   case(f'2-{typ}-{op}-{a}',2,typ,op,a,**({'b':a} if op=='compare' else {}))
 case(f'2-{typ}-perpendicular-min-x',2,typ,'perpendicular',[lo,0])
 case(f'2-{typ}-perpendicular-min-y',2,typ,'perpendicular',[0,lo],refuse=True)
 case(f'2-{typ}-self-dot-overflow',2,typ,'dot',[lo,lo],b=[lo,lo],refuse=True)
 case(f'2-{typ}-self-cross-overflow',2,typ,'cross',[hi,hi],b=[hi,hi],refuse=True if typ=='i64' else False)
 case(f'2-{typ}-squared-distance-overflow',2,typ,'squared_distance',[lo,0],b=[hi,0],refuse=True)
 case(f'2-{typ}-sub-overflow',2,typ,'sub',[lo,0],b=[1,0],refuse=True)
 case(f'2-{typ}-compound-late-overflow',2,typ,'add_assign',[2,hi],b=[1,1],refuse=True)
 for a,length in [([0,0],lo),([3,4],1),([1,1],1),([1,0],lo)]:
  # int64 newLength^2 overflows for MIN; int32 widening makes it defined.
  case(f'2-{typ}-resize-{a}-{length}',2,typ,'resize',a,scalar=length,refuse=typ=='i64' and a!=[0,0] and length==lo)
 case(f'2-{typ}-resize-diagonal-min-length',2,typ,'resize',[1,1],scalar=lo,refuse=True)
 case(f'2-{typ}-resize-min-source',2,typ,'resize',[lo,0],scalar=1,refuse=True)
case('2-i64-resize-default-int-round',2,'i64','resize',[1,0],scalar=3_000_000_000)
case('distance-subtract-before-double',2,'i64','distance',[2**53,0],b=[2**53+1,0])
case('distance-extended-argument',2,'i32','distance',[0,0],b=[2**32,0])
case('distance-overflow',2,'i64','distance',[-2**63,0],b=[2**63-1,0],refuse=True)
for op in ['cast','cast_call']:
 case(f'integral-clamp-i64-i32-{op}',2,'i64',op,[-2**63,2**63-1],target='i32')
 case(f'integral-widen-i32-i64-{op}',2,'i32',op,[-2**31,2**31-1],target='i64')
case('direct-i32-u32-modulo',3,'i32','cast',[-2**31,-1,2**31-1],target='u32')
case('direct-u32-i32-modulo',3,'u32','cast',[0,2**31,2**32-1],target='i32')
case('direct-u32-f64-high',3,'u32','cast',[2**32-1,0,2**31],target='f64')
case('direct-f64-i32-truncate-boundaries',3,'f64','cast',floats([-2**31-0.75,2**31-0.5,-0.0]),target='i32')
case('clamp-f64-i32-boundaries',2,'f64','cast',floats([-2**31-0.75,2**31-0.25]),target='i32')
patterns=['0000000000000000','8000000000000000','0000000000000001','8000000000000001','0010000000000000','7fefffffffffffff','ffefffffffffffff','7ff0000000000000','fff0000000000000','7ff8000000000123','fff8000000000456','7ff0000000000789','fff0000000000abc']
for n,x in enumerate(patterns):
 for m,y in enumerate(patterns):
  for dim in [2,3]:
   a=[x,y]+([bits(2)] if dim==3 else []);b=[y,x]+([bits(2)] if dim==3 else [])
   for op in ['new','copy','assign','compare']:
    case(f'bits-{dim}-{n}-{m}-{op}',dim,'f64',op,a,**({'b':b} if op=='compare' else {}))
  for op in ['mul_scalar','scalar_mul','add_scalar','sub_scalar','divide','add_assign_scalar','sub_assign_scalar','mul_assign_scalar']:
   for which,scalar in enumerate([x,y]):
    case(f'fp-scalar-{n}-{m}-{which}-{op}',2,'f64',op,[x,y],scalar=scalar)
  for op in ['add_assign_scalar','sub_assign_scalar','mul_assign_scalar']:
   for alias in 'xy':
    case(f'fp-alias-{n}-{m}-{alias}-{op}',2,'f64',op,[x,y],alias=alias)
  for op in ['neg','perpendicular']:
   case(f'fp-unary-{n}-{m}-{op}',2,'f64',op,[x,y])
  for op in ['add','sub','dot','cross','mul_assign','norm','squared_norm']:
   case(f'fp-sites-{n}-{m}-{op}',2,'f64',op,[x,y],**({'b':[y,x]} if op in ['add','sub','dot','cross','mul_assign'] else {}))
  for op in ['cross','dot']:
   case(f'3-fp-{n}-{m}-{op}',3,'f64',op,[x,y,bits(2)],b=[y,x,bits(3)])
  for op in ['mul_assign_scalar','div_assign_scalar']:
   for which,scalar in enumerate([x,y]):
    case(f'3-fp-scalar-{n}-{m}-{which}-{op}',3,'f64',op,[x,y,bits(2)],scalar=scalar)
  for op in ['normalize','norm']:
   case(f'3-fp-{n}-{m}-{op}',3,'f64',op,[x,y,bits(0)])
for n,x in enumerate(patterns):
 case(f'3-set-all-bits-{n}',3,'f64','set_all',floats([1,2,3]),scalar=x)
for op in ['equals','ki_round','ki_round_xy']:
 for n,x in enumerate(patterns+[bits(0.5),bits(-0.5),bits(2147483647.9)]):
  case(f'{op}-{n}',2,'f64',op,[x,bits(-0.5)],**({'b':[bits(1),bits(-0.5)]} if op=='equals' else {}))
for epsilon in [0.0,-1.0,2.220446049250313e-16,float('inf'),float('nan')]:
 case(f'equals-epsilon-{bits(epsilon)}',2,'f64','equals',floats([float('inf'),0]),b=floats([-float('inf'),0]),epsilon=bits(epsilon))
for x in [2.0**63, float.fromhex('0x1.fffffffffffffp62'),-2.0**63,float('inf'),-float('inf'),float('nan')]:
 case(f'cast-i64-{bits(x)}',2,'f64','cast',floats([x,0]),target='i64',refuse=x!=x or x>=2**63)
for x in [-0.75,-1.0,2.0**32-0.5,2.0**32,float('inf'),float('nan')]:
 case(f'cast-u32-{bits(x)}',3,'f64','cast',floats([x,0,1]),target='u32',refuse=x!=x or x<=-1 or x>=2**32)
for a in [[65536,0,0],[65536,2,0],[2**32-1]*3,[0,0,0]]:
 for op in ['norm','normalize','dot','cross']:
  case(f'3-u32-{a}-{op}',3,'u32',op,a,**({'b':a} if op in ['dot','cross'] else {}))
case('3-i32-cross-products-overflow-cancel',3,'i32','cross',[0,50000,50000],b=[0,50000,50000],refuse=True)
case('3-i32-dot-sum-overflow',3,'i32','dot',[-2**31]*3,b=[-2**31]*3,refuse=True)
case('3-i32-norm-cast-overflow',3,'i32','norm',[2**31-1,2**31-1,0],refuse=True)
case('3-i32-div-zero',3,'i32','div_assign_scalar',[1,2,3],scalar=0,refuse=True)
case('3-i32-div-min-negative-one',3,'i32','div_assign_scalar',[-2**31,2,3],scalar=-1,refuse=True)
for a in [[1e300,2e300],[1e-300,2e-300],[0.0,-0.0],[1.0,1.0],[-1.0,1.0]]:
 case(f'floating-resize-{a}',2,'f64','resize',floats(a),scalar=bits(1.0))
rng=random.Random(0x564543544f52)
for n in range(64):
 a=[rng.randrange(-1000,1001) for _ in range(2)];b=[rng.randrange(-1000,1001) for _ in range(2)]
 for typ in ['i32','i64','f64']:
  aa=floats(a) if typ=='f64' else a;bb=floats(b) if typ=='f64' else b
  for op in ['add','sub','dot','cross','compare','squared_distance']:
   case(f'random-{n}-{typ}-{op}',2,typ,op,aa,b=bb)
# Resize's general branch now runs the accepted rescale<int64_t> before the KiROUND call:
# signs, half-away rounding, an __int128-only product and int64 square overflow (refused).
for typ in ['i32','i64']:
 for a,length in [([-3,4],-10),([3,-4],10),([-1,2],3),([2,-1],-1),([1,2],1),([5,0],-7),([0,-6],2)]:
  case(f'2-{typ}-resize-general-{a}-{length}',2,typ,'resize',a,scalar=length)
 for n in range(16):
  a=[rng.randrange(-100000,100001) for _ in range(2)];length=rng.randrange(-100000,100001)
  case(f'random-{n}-{typ}-resize-general',2,typ,'resize',a,scalar=length)
case('2-i64-resize-general-int128-product',2,'i64','resize',[3_037_000_499,1],scalar=3_037_000_499)
case('2-i64-resize-general-int128-negative',2,'i64','resize',[-3_037_000_499,-2],scalar=-3_037_000_499)
case('2-i32-resize-general-max-length',2,'i32','resize',[2**31-1,1],scalar=2**31-1)
case('2-i64-resize-square-overflow',2,'i64','resize',[3_037_000_500,1],scalar=1,refuse=True)
# Additive util.h wiring continuations; seed is isolated from the accepted corpus.
ut_rng=random.Random(0xB02EDA)
for n in range(64):
 a=[ut_rng.randrange(-10000,10001),ut_rng.randrange(-10000,10001)]
 for typ in ['i32','i64','f64']:
  aa=floats(a) if typ=='f64' else a
  length=ut_rng.randrange(-10000,10001)
  case(f'ut-wired-{n}-{typ}-resize',2,typ,'resize',aa,scalar=bits(length) if typ=='f64' else length)
  case(f'ut-wired-{n}-{typ}-norm',2,typ,'norm',aa)
  case(f'ut-wired-{n}-{typ}-divide',2,typ,'divide',aa,scalar=bits(3.0))
 case(f'ut-wired-{n}-equals-default',2,'f64','equals',floats(a),b=floats(a))
 case(f'ut-wired-{n}-round',2,'f64','ki_round',floats([a[0]+0.5,a[1]-0.5]))
# Additive std::hypot continuations of EuclideanNorm (and Distance, which calls it), past the
# 45-degree and axis shortcuts: the non-finite branch's infinity and NaN operand order, the
# 0x1p+511 / 0x1p-459 scaling thresholds, the 0x1p-54 shortcut, the kernel's h <= 2 * ay
# split, overflow, subnormals and exact triples. Inputs only; seed isolated from the corpus.
def hx(b): return f'{b:016x}'
def f(b): return struct.unpack('>d',bytes.fromhex(hx(b)))[0]
def raw(x): return struct.unpack('>Q',struct.pack('>d',x))[0]
hy_special=[0x7ff0000000000000,0xfff0000000000000,0x7ff8000000000000,0xfff80000deadbeef,
            0x7ff0000000000001,0xfff4000000000abc,0x7ff7ffffffffffff,0x3ff0000000000000,
            0xbff8000000000000,0x0000000000000001,0x7fefffffffffffff]
for n,x in enumerate(hy_special):
 for m,y in enumerate(hy_special):
  case(f'hypot-special-{n}-{m}-norm',2,'f64','norm',[hx(x),hx(y)])
  case(f'hypot-special-{n}-{m}-distance',2,'f64','distance',[hx(x),hx(y)],b=[bits(0.5),bits(-2.0)])
hy_edges=[]
for ax in [0x5fe0000000000000+d for d in range(-2,3)]+[0x7fefffffffffffff,0x7fe0000000000000]:
 for ay in [ax-(54<<52),ax-(54<<52)+1,ax-(54<<52)-1,ax-(53<<52),0x3ff0000000000000,ax-(1<<52)]:
  hy_edges.append((ax,ay))
for ay in [0x2340000000000000+d for d in range(-2,3)]+[1,2,0x000fffffffffffff,0x0010000000000000]:
 # ay * 2^54 and 2^53 are exact (ax >= ay / EPS), plus a neighbour and the scaled kernel.
 for ax in [raw(f(ay)*2.0**54),raw(f(ay)*2.0**54)+1,raw(f(ay)*2.0**54)-1,raw(f(ay)*2.0**53),
            ay+1,0x3ff0000000000000,0x1ab0000000000000]:
  hy_edges.append((ax,ay))
for e in [1,100,563,564,565,1023,1533,1534,1535,2000]:
 for d in range(-3,4):
  ax=(e<<52)|0x8000000000000
  # ay/ax near float(1/sqrt(3)): the kernel's h <= 2 * ay split.
  ay=raw(f(ax)*float.fromhex('0x1.279a74590331cp-1'))+d
  hy_edges.append((ax,ay))
# Results next to DBL_MAX: both operands near DBL_MAX * float(1/sqrt(2)).
v=raw(float.fromhex('0x1.fffffffffffffp+1023')*float.fromhex('0x1.6a09e667f3bcdp-1'))
for d in range(-3,4): hy_edges.append((v+d,v-1)); hy_edges.append((v+d,v+1))
# Exact triples; integer times a power of two is exact down to the smallest subnormal.
for k in [-1074,-1060,-600,-460,-459,0,511,512,1000]:
 for a,b in [(3,4),(5,12),(20,21)]:
  hy_edges.append((raw(a*2.0**k),raw(b*2.0**k)))
for n,(x,y) in enumerate(hy_edges):
 for sx,sy in [(0,0),(1,1),(1,0)]:
  case(f'hypot-edge-{n}-{sx}{sy}-norm',2,'f64','norm',[hx(x|sx<<63),hx(y|sy<<63)])
  case(f'hypot-edge-{n}-{sx}{sy}-swap-norm',2,'f64','norm',[hx(y|sy<<63),hx(x|sx<<63)])
hy_rng=random.Random(0x48797074)
for n in range(256):
 r=hy_rng.getrandbits(64);s=hy_rng.getrandbits(64)
 if n%4==1: s=(s&0x800fffffffffffff)|(max(0,min(2046,((r>>52)&0x7ff)+hy_rng.randrange(-60,61)))<<52)
 if n%4==2: r&=0x800fffffffffffff;s&=0x800fffffffffffff
 case(f'hypot-random-{n}-norm',2,'f64','norm',[hx(r),hx(s)])
for n in range(64):
 for typ,lo,hi in [('i32',-2**31,2**31-1),('i64',-2**62,2**62)]:
  a=[hy_rng.randrange(lo,hi+1),hy_rng.randrange(lo,hi+1)]
  b=[hy_rng.randrange(lo,hi+1),hy_rng.randrange(lo,hi+1)]
  case(f'hypot-random-{n}-{typ}-distance',2,typ,'distance',a,b=b)
assert len({c['case'] for c in CASES+REFUSALS})==len(CASES)+len(REFUSALS)
for p,rs in [(ROOT/f'port/cases/{UNIT}.jsonl',CASES),(ROOT/'port/oracle/vector-refusals.jsonl',REFUSALS)]:
 p.write_text(''.join(json.dumps(r,separators=(',',':'))+'\n' for r in rs))
print(len(CASES),'cases;',len(REFUSALS),'refusals')
