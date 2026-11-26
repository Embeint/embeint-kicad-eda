# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic pow inputs only, never expected results. Little-endian (x,y) pairs.

Fixed cases follow the branches of glibc 2.39 e_pow.c: special operands, the
top12 range checks on x and y, checkint's exponent edges, every log_inline table
bin, the exp_inline tiny/large/overflow thresholds, the specialcase subnormal
rounding and the integer powers of ten used by EDA_ANGLE::Round. The seeded
modes are separate from domain.py's seed, so the accepted domain is unchanged.
"""
import decimal
import hashlib
import math
import struct

from domain import MASK, SIGN, bits, value

COUNT = 1_000_000
SEED = 0x506F774C69626D32
OFF = 0x3fe6955500000000
MAX_FINITE = 0x7fefffffffffffff


# Inputs never use the host libm (its log differs between platforms, which
# would change the pinned hash): decimal's ln is correctly rounded, and the
# seeded modes use an exact-arithmetic logarithm estimate from the raw bits.
CONTEXT = decimal.Context(prec=50)


def exact_ratio(target, base):
    """float(target / ln(base)), from decimal arithmetic only."""
    ln = CONTEXT.ln(decimal.Decimal(base))
    return float(CONTEXT.divide(decimal.Decimal(repr(target)), ln))


def rough_ln(b):
    """ln of a positive finite bit pattern to a few digits: (e + m) * ln2."""
    e = (b >> 52) & 0x7ff
    m = (b & 0x000fffffffffffff) / 2 ** 52
    if e == 0: return -730.0  # subnormal: a representative magnitude suffices
    return ((e - 1023) + m) * 0.6931471805599453


class SplitMix:
    def __init__(self): self.state = SEED

    def next(self):
        self.state = (self.state + 0x9e3779b97f4a7c15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xbf58476d1ce4e5b9) & MASK
        z = ((z ^ (z >> 27)) * 0x94d049bb133111eb) & MASK
        return z ^ (z >> 31)


def near(b, width=2):
    """Raw neighbours of a finite magnitude, both signs."""
    out = []
    for delta in range(-width, width + 1):
        a = b + delta
        if 0 <= a <= MAX_FINITE:
            out += [a, a | SIGN]
    return out


def fixed():
    out = set()

    def pair(x, y): out.add((x & MASK, y & MASK))

    specials = [0, 1, 2, 0x000fffffffffffff, 0x0010000000000000, 0x0010000000000001,
                bits(math.ldexp(1.0, -126)), bits(0.5), bits(1.0) - 1, bits(1.0), bits(1.0) + 1,
                bits(1.5), bits(2.0), bits(3.0), bits(10.0), bits(0.1), MAX_FINITE,
                0x7ff0000000000000, 0x7ff0000000000001, 0x7ff4000000000abc,
                0x7ff7ffffffffffff, 0x7ff8000000000000, 0x7ff80000deadbeef,
                0x7fffffffffffffff]
    specials += [x | SIGN for x in specials]
    for x in specials:
        for y in specials: pair(x, y)

    # y range checks: |y| < 2^-65 (top 0x3be), |y| >= 2^63 (top 0x43e) and
    # 1075*ln2*2^53, with x on both sides of 1 and negative x.
    ys = []
    for e in range(-1074, -60):
        ys += near(bits(math.ldexp(1.0, e)), 1)
    for e in range(60, 1024):
        ys += near(bits(math.ldexp(1.0, e)), 1)
    ys += near(bits(float.fromhex('0x1.749p62'))) + near(bits(float.fromhex('0x1.e7b6p-65')))
    xs = [bits(v) for v in [0.5, 1.0, 2.0, 10.0, 1e-300, 1e300, -2.0, -0.5, -1.0, -10.0]]
    xs += [bits(1.0) + 1, bits(1.0) - 1, SIGN | (bits(1.0) + 1), 1, SIGN | 1]
    for x in xs:
        for y in ys: pair(x, y)

    # checkint: every exponent from 0x3fe to 0x435, odd/even/fractional
    # mantissas, with negative x so the sign and invalid branches matter.
    for e in range(0x3fe, 0x436):
        for m in [0, 1, 2, 3, (1 << 52) - 1, (1 << 52) - 2, 1 << 51, 3 << 50]:
            y = (e << 52) | (m & ((1 << 52) - 1))
            for x in [bits(-2.0), bits(-0.5), bits(-1.0) | 1, SIGN | 1, bits(-0.0),
                      bits(-math.inf), SIGN | 0x7ff80000deadbeef, SIGN | 0x7ff0000000000001]:
                pair(x, y); pair(x, y | SIGN)
            pair(bits(-1.0 - 2 ** -52), y)

    # Every log_inline table bin edge, for exponents spanning normal and
    # subnormal x, with y chosen to keep the result finite where possible.
    for k in list(range(-60, 61)) + [-1074, -1060, -1030, -1022, -1000, -500, 500, 1000, 1023]:
        for j in range(128):
            iz = OFF + (j << 45)
            base = iz + ((k + 0) << 52)
            for b in near(base & MASK, 1):
                if b >> 63 or not 0 < b <= MAX_FINITE: continue
                for y in [bits(1.0), bits(0.5), bits(2.0), bits(-1.0), bits(3.0), bits(1 / 3)]:
                    pair(b, y)
    # Subnormal x at every exponent and its neighbours.
    for e in range(-1074, -1021):
        for b in near(bits(math.ldexp(1.0, e)), 2) + near(bits(math.ldexp(1.5, e)) if e > -1074 else 1, 1):
            for y in [bits(1.0), bits(0.5), bits(-1.0), bits(-0.5), bits(2.0), bits(0.001), bits(-0.001)]:
                pair(b, y)

    # x near 1: log is tiny, including exp_inline's tiny-input path.
    for d in list(range(1, 65)) + [1 << s for s in range(6, 52)]:
        for x in [bits(1.0) + d, bits(1.0) - d]:
            for y in [1.0, -1.0, 0.125, 1e-5, 1e5, 2.0 ** 40, -(2.0 ** 40), 2.0 ** 52, 3.0, 1e300]:
                pair(x, bits(y))

    # Result thresholds: overflow at 2^1024, normal/subnormal boundary 2^-1022,
    # smallest subnormal 2^-1074 and 2^-1075 rounding, and exp_inline's
    # abstop boundaries |ehi| = 512 and 1024, positive and negative bases.
    for base in [2.0, 10.0, math.e, 0.5, 0.1, 3.0, 1.5, 7.0]:
        for target in [512.0, 709.78, 709.7827128933840, 710.0, 744.44, 745.1332191019411,
                       745.2, 1024.0, -512.0, -708.3964185322641, -709.08956571282,
                       -744.44007192138, -745.1332191019411, -745.2, -1024.0]:
            y = exact_ratio(target, base)
            for b in near(bits(y), 3):
                pair(bits(base), b); pair(bits(-base), b)
        for e in range(-1080, 1030):
            if base == 2.0:
                for y in near(bits(float(e)), 1) + near(bits(e + 0.5), 1):
                    pair(bits(2.0), y); pair(bits(-2.0), y); pair(bits(0.5), y)

    # Integer powers of ten (EDA_ANGLE::Round, digits) and neighbouring bases.
    for d in range(-1100, 1101):
        for b in [bits(10.0), bits(10.0) - 1, bits(10.0) + 1, bits(-10.0)]:
            pair(b, bits(float(d)))
    # Odd/even integer y with negative x across all result magnitudes.
    for n in list(range(-40, 41)) + [101, -101, 255, 1023, -1023, 1075, -1075]:
        for x in [-1.5, -3.0, -0.75, -1e10, -1e-10, -(2.0 ** -1070)]:
            pair(bits(x), bits(float(n)))
    return sorted(out)


def pairs():
    yield from fixed()
    rng = SplitMix()
    for i in range(COUNT):
        r, s = rng.next(), rng.next()
        mode = i % 8
        if mode == 0:
            x, y = r, s
        elif mode == 1:
            # Positive normal or subnormal x, y such that y*log(x) spans the
            # whole finite result range and its overflow/underflow edges.
            x = r & 0x7fffffffffffffff
            if x > MAX_FINITE or x == 0: x = bits(2.0)
            t = ((s >> 11) / 2 ** 53) * 1500.0 - 760.0
            lx = rough_ln(x)
            y = bits(t / lx) if lx != 0 else s
        elif mode == 2:
            # Negative x with integer y, odd and even.
            x = (r & 0x7fffffffffffffff) | SIGN
            if (x & 0x7fffffffffffffff) > MAX_FINITE: x = bits(-3.0)
            lx = rough_ln(x & 0x7fffffffffffffff) or 1.0
            n = round((((s >> 11) / 2 ** 53) * 1500.0 - 760.0) / lx)
            y = bits(float(n))
        elif mode == 3:
            # x within 2^24 ulps of 1, large |y|.
            x = (bits(1.0) + (r % (1 << 25)) - (1 << 24)) & MASK
            y = (s & 0x800fffffffffffff) | ((0x3ff + (s >> 52) % 64) << 52)
        elif mode == 4:
            # Powers of ten, the EDA_ANGLE::Round callee, with raw y near integers.
            x = (bits(10.0) + (r % 5) - 2) | ((r >> 8 & 1) << 63)
            y = bits(float((s % 2201) - 1100)) + ((s >> 32) % 5) - 2
        elif mode == 5:
            # Subnormal x with moderate y.
            x = (r & 0x000fffffffffffff) | ((r >> 63) << 63)
            y = bits((((s >> 11) / 2 ** 53) * 4.0 - 2.0))
        elif mode == 6:
            # Moderate exponents on both operands (no special-range branch).
            x = (r & 0x800fffffffffffff) | ((0x300 + (r >> 52) % 0x200) << 52)
            y = (s & 0x800fffffffffffff) | ((0x3be + (s >> 52) % 0x80) << 52)
        else:
            # y at the |y| range-check exponents with random x.
            x = r
            y = (s & 0x800fffffffffffff) | ((0x3b0 + (s >> 52) % 0x1c) << 52 if s & 1
                                            else (0x430 + (s >> 52) % 0x14) << 52)
        yield x & MASK, y & MASK


def write(path):
    digest = hashlib.sha256(); count = 0
    fixed_count = len(fixed())
    with open(path, 'wb') as out:
        for x, y in pairs():
            b = struct.pack('<QQ', x, y); out.write(b); digest.update(b); count += 1
    return dict(fixed_count=fixed_count, seeded_count=COUNT, count=count, sha256=digest.hexdigest())


if __name__ == '__main__':
    import json, sys
    print(json.dumps(write(sys.argv[1]), sort_keys=True))
