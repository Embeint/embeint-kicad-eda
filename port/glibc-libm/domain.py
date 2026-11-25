# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic inputs only, never expected results. Little-endian (x,y) pairs."""
import hashlib
import math
import struct

COUNT = 1_000_000
SEED = 0x4174616E324C6962
MASK = (1 << 64) - 1
SIGN = 1 << 63

def bits(x):
    return struct.unpack('<Q', struct.pack('<d', x))[0]

def value(x):
    return struct.unpack('<d', struct.pack('<Q', x))[0]

class SplitMix:
    def __init__(self): self.state = SEED
    def next(self):
        self.state = (self.state + 0x9e3779b97f4a7c15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xbf58476d1ce4e5b9) & MASK
        z = ((z ^ (z >> 27)) * 0x94d049bb133111eb) & MASK
        return z ^ (z >> 31)

def fixed():
    out = set()
    def pair(x,y): out.add((x,y))
    def around(x):
        b=bits(x)
        for delta in (-2,-1,0,1,2):
            a=b+delta
            if 0 <= a <= 0x7fefffffffffffff:
                for sign in (0,SIGN):
                    pair(a|sign,bits(1.0)); pair(bits(1.0),a|sign)
                    pair(a|sign,bits(-1.0)); pair(bits(-1.0),a|sign)
    special=[0,1,2,0x000fffffffffffff,0x0010000000000000,
             bits(1.0),bits(0.5),bits(2.0),0x7fefffffffffffff,0x7ff0000000000000,
             0x7ff0000000000001,0x7ff4000000000abc,0x7ff7ffffffffffff,
             0x7ff8000000000000,0x7ff80000deadbeef,0x7fffffffffffffff]
    special += [x|SIGN for x in special]
    for x in special:
        for y in special: pair(x,y)
    for b in [0x3e4bb67a00000000,0x43349ff200000000,0x3e4b096c00000000,
              0x3faf212d00000000,0x3fe92f1a00000000,0x3c88000000000000,
              0x3e50000000000000,bits(25.0),bits(1e8),bits(0.0625),bits(16.0)]:
        around(value(b))
    # Every rounded atan row boundary, both sides of reciprocal reduction.
    for j in range(16,257):
        for u in [j/256, (j+0.5)/256]:
            around(u); around(1.0/u)
    # Every truncating tan table boundary and centres.
    for j in range(186):
        around((j+15.5)/256); around((j+16)/256)
    # Every asin/acos table-bin boundary, each polynomial region.
    for start,end,step in [(0x3fc00000,0x3fd00000,0x8000),
                           (0x3fd00000,0x3fe00000,0x4000),
                           (0x3fe00000,0x3fef0000,0x2000)]:
        for k in range(start,end+1,step): around(value(k<<32))
    around(1.0)
    # Near-one root-table bin edges in z=(1-|x|)/2. Rounding the back
    # transformation is intentional; neighbours cover the attainable x values.
    for e in range(-54,-5):
        for j in range(129): around(1.0-2.0*math.ldexp(1+j/128,e))
    # All binary exponents and subnormal edges, plus the atan2 exponent-gap
    # branches at +/-57. Independent signs, and ratios spanning every quadrant.
    for e in range(-1074,1024):
        a=math.ldexp(1.0,e)
        around(a)
        for gap in (-58,-57,-56,0,56,57,58):
            if -1074 <= e+gap <=1023:
                b=math.ldexp(1.0,e+gap)
                for delta in (-1,0,1):
                    ab=bits(a)+delta
                    if ab < 0: continue
                    for sx in (0,SIGN):
                        for sy in (0,SIGN): pair(ab|sx,bits(b)|sy)
    # Range-reduction edges near multiples of pi/2 at small and huge scale.
    for e in [0,1,4,16,24,26,32,40,48,52,100,500,900,1021]:
        for j in range(1,33):
            if e + math.frexp(j*math.pi/2)[1] - 1 <= 1023:
                around(math.ldexp(j*math.pi/2,e))
    return sorted(out)

def pairs():
    yield from fixed()
    rng=SplitMix()
    for i in range(COUNT):
        r,s=rng.next(),rng.next()
        mode=i%8
        if mode==0: x,y=r,s
        elif mode==1:
            x=bits((r>>11)/2**52-1); y=bits((s>>11)/2**52-1)
        elif mode==2:
            x=(bits(1.0)+(r%2049)-1024)|((r&1)<<63); y=s
        elif mode==3:
            x=((r&((1<<52)-1))|((r&1)<<63)); y=s
        elif mode==4:
            u=(16+(r%241)+0.5)/256
            x=bits(u)+(s%33)-16; y=bits(1.0)
            if r&1: x=bits(1.0/value(x))
            x |= ((r>>1)&1)<<63
        elif mode==5:
            # tan range reduction near an enormous multiple of pi/2.
            a=math.ldexp((r>>11)/2**53+1, int(s%1023))
            x=bits(a)+(s%17)-8; y=r
        elif mode==6:
            x=(0x3fc0000000000000+(r%(0x3ff0000000000000-0x3fc0000000000000)))|((r&1)<<63)
            y=s
        else:
            x=r
            # Deliberately correlated magnitudes exercise atan2's table paths.
            y=(r & 0x7ff0000000000000)|(s & 0x800fffffffffffff)
        yield x,y

def write(path):
    digest=hashlib.sha256(); count=0
    with open(path,'wb') as out:
        for x,y in pairs():
            b=struct.pack('<QQ',x,y);out.write(b);digest.update(b);count+=1
    return dict(fixed_count=len(fixed()),seeded_count=COUNT,count=count,sha256=digest.hexdigest())

if __name__=='__main__':
    import json,sys
    print(json.dumps(write(sys.argv[1]),sort_keys=True))
