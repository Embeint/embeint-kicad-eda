# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic hypot inputs only, never expected results. Little-endian (x,y) pairs.

Fixed cases follow the branches of glibc 2.39 e_hypot.c (__hypot, hypot@@GLIBC_2.35):
the non-finite branch (infinities, quiet/signalling NaN payload order, both signs),
the ax > 0x1p+511 and ay < 0x1p-459 scaling branches, the ay <= ax * 0x1p-54
widely-varying shortcut in all three regions, the kernel's h <= 2 * ay split (ay/ax
near 1/sqrt(3)), every exponent including subnormals, overflow next to DBL_MAX and
subnormal results. The seeded modes use their own seed, so the accepted angle and pow
domains are unchanged. No host libm is used: the only irrational constants come from
decimal, and every other value is built from raw bits or exact binary64 arithmetic.
"""
import decimal
import hashlib
import struct

from domain import MASK, SIGN, bits

COUNT = 1_000_000
SEED = 0x487970744C69626D
MAX_FINITE = 0x7fefffffffffffff
INF = 0x7ff0000000000000
LARGE = 0x5fe0000000000000  # 0x1p+511
TINY = 0x2340000000000000  # 0x1p-459
QNANS = [0x7ff8000000000000, 0x7ff8000000000001, 0x7ff80000deadbeef, 0x7fffffffffffffff]
SNANS = [0x7ff0000000000001, 0x7ff4000000000abc, 0x7ff7ffffffffffff]

CONTEXT = decimal.Context(prec=60)
# float(1 / sqrt(3)) and float(1 / sqrt(2)), correctly rounded through decimal.
INV_SQRT3 = float(CONTEXT.divide(1, CONTEXT.sqrt(decimal.Decimal(3))))
INV_SQRT2 = float(CONTEXT.divide(1, CONTEXT.sqrt(decimal.Decimal(2))))


class SplitMix:
    def __init__(self): self.state = SEED

    def next(self):
        self.state = (self.state + 0x9e3779b97f4a7c15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xbf58476d1ce4e5b9) & MASK
        z = ((z ^ (z >> 27)) * 0x94d049bb133111eb) & MASK
        return z ^ (z >> 31)


def finite(b):
    return 0 <= b <= MAX_FINITE


def with_exponent(b, e):
    """Magnitude b's mantissa at biased exponent e (1..2046), or None."""
    if not 1 <= e <= 2046: return None
    return (b & 0x000fffffffffffff) | (e << 52)


def fixed():
    out = set()

    def pair(x, y, both=True):
        """Add (x, y) with all sign combinations, and (y, x) when both is set."""
        for sx in (0, SIGN):
            for sy in (0, SIGN):
                out.add(((x | sx) & MASK, (y | sy) & MASK))
                if both: out.add(((y | sy) & MASK, (x | sx) & MASK))

    def near(b, width):
        return [b + d for d in range(-width, width + 1) if finite(b + d)]

    # Non-finite branch and special magnitudes, full Cartesian product with signs.
    special = [0, 1, 2, 0x000fffffffffffff, 0x0010000000000000, bits(0.5), bits(1.0),
               bits(2.0), bits(3.0), LARGE, TINY, MAX_FINITE, INF] + QNANS + SNANS
    for x in special:
        for y in special: pair(x, y, both=False)

    # Every exponent (biased 0 is subnormal) against relative exponents around
    # the 0x1p-54 shortcut and the kernel, in both orders.
    xm = [0, 0x8000000000001, 0xfffffffffffff]
    ym = [0, 0x6a09e667f3bcd, 0xfffffffffffff]
    for e in range(0, 2047):
        for m in xm:
            x = (e << 52) | m
            if x == 0: continue
            for d in (0, 1, 2, 27, 52, 53, 54, 55, 56, 60):
                for n in ym:
                    ey = e - d
                    if ey < 0: continue
                    y = (ey << 52) | n
                    if y == 0: continue
                    out.add((x, y)); out.add((y | SIGN, x))

    # ax > LARGE_VAL threshold with the shortcut and kernel continuations.
    for ax in near(LARGE, 3) + near(MAX_FINITE - 3, 3) + [bits(2.0 ** 600), bits(2.0 ** 1000)]:
        e = (ax >> 52) & 0x7ff
        for ay in near(with_exponent(ax, e - 54) or 1, 3) + near(with_exponent(ax, e - 55) or 1, 2) \
                + near(with_exponent(ax, e - 53) or 1, 2) + [bits(1.0), 1, ax, ax - 1,
                                                             with_exponent(ax, e - 1) or 1]:
            pair(ax, ay)

    # ay < TINY_VAL threshold with ax >= ay / EPS and the scaled kernel.
    for ay in near(TINY, 3) + [1, 2, 3, 0x000fffffffffffff, 0x0010000000000000,
                               0x0010000000000001, bits(2.0 ** -1000)]:
        e = (ay >> 52) & 0x7ff
        cands = [ay, ay + 1, bits(1.0), bits(2.0 ** -405), bits(2.0 ** -404)]
        for k in (53, 54, 55):
            if e >= 1:
                t = with_exponent(ay, e + k)
                if t is not None: cands += near(t, 3)
            else:
                # Subnormal ay = m * 2^-1074; ay * 2^k is exact.
                t = exact_bits(ay, k - 1074)
                if t is not None: cands += near(t, 3)
        for ax in cands:
            if finite(ax): pair(ax, ay)

    # Shortcut ay <= ax * EPS in the common region for every exponent with room.
    for e in range(55, 2047):
        for m in (0, 1, 0x8000000000000, 0xfffffffffffff):
            ax = (e << 52) | m
            for ay in near((e - 54) << 52 | m, 2):
                out.add((ax, ay)); out.add((ay | SIGN, ax | SIGN))

    # Kernel split h <= 2 * ay: ay/ax near 1/sqrt(3) at many scales.
    for base in (1.0, 1.5, 1.9999999999999998, 1.2345678901234567):
        b = bits(base)
        ay0 = bits(base * INV_SQRT3)  # in [0.5, 2): its exponent is one below ax's
        for e in list(range(1, 2047, 7)) + [1023, 1534, 1535, 1536, 564, 565, 566, 100, 50, 2]:
            ax = with_exponent(b, e)
            ay = with_exponent(ay0, e - 1)
            if ax is None or ay is None: continue
            for d in range(-8, 9):
                if finite(ay + d): out.add((ax, ay + d)); out.add((ay + d, ax | SIGN))

    # Results near DBL_MAX: x = y = DBL_MAX / sqrt(2) and neighbours (overflow edge).
    v = bits(float.fromhex('0x1.fffffffffffffp+1023') * INV_SQRT2)
    for a in near(v, 16):
        for b in near(v, 4) + [bits(1.0), MAX_FINITE]:
            pair(a, b)

    # Exact Pythagorean triples at every scale, including subnormal operands.
    for a, b in [(3, 4), (5, 12), (8, 15), (20, 21), (7, 24), (119, 120), (696, 697)]:
        for k in range(-1074, 1024 - 10):
            # Built exactly: an integer times a power of two, never rounded.
            pa, pb = exact_bits(a, k), exact_bits(b, k)
            if pa is not None and pb is not None: pair(pa, pb)

    # Subnormal pairs: every small mantissa against a spread of subnormals.
    subs = list(range(1, 65)) + [0x000fffffffffffff - d for d in range(8)] + \
        [1 << s for s in range(52)]
    for a in subs:
        for b in (1, 2, 3, 7, 1 << 20, 1 << 51, 0x000fffffffffffff, 0x0010000000000000):
            pair(a, b)
    return sorted(out)


def exact_bits(n, k):
    """Bits of n * 2^k (n a positive integer below 2^53) if exact and finite, else None."""
    if n <= 0: return None
    v = n
    e = k
    while v % 2 == 0: v //= 2; e += 1
    width = v.bit_length()
    top = e + width - 1  # unbiased exponent of the leading bit
    if top > 1023: return None
    if top >= -1022:
        mant = (v << (53 - width)) & 0x000fffffffffffff
        return ((top + 1023) << 52) | mant
    shift = e + 1074  # subnormal: v * 2^e = (v << shift) * 2^-1074
    if shift < 0: return None
    return v << shift


def seeded():
    rng = SplitMix()
    for i in range(COUNT):
        r, s = rng.next(), rng.next()
        mode = i % 7
        if mode == 0:
            x, y = r, s  # full raw bits, including NaN payloads and infinities
        elif mode == 1:
            # Relative exponent within +-64 of a random finite x.
            e = 1 + r % 2046
            x = (r & 0x800fffffffffffff) | (e << 52)
            ey = max(0, min(2046, e + (s % 129) - 64))
            y = (s & 0x800fffffffffffff) | (ey << 52)
        elif mode == 2:
            # Close magnitudes in the common region.
            e = 564 + r % (1534 - 564)
            x = (r & 0x800fffffffffffff) | (e << 52)
            y = (s & 0x800fffffffffffff) | ((e - s % 4) << 52)
        elif mode == 3:
            # Tiny and subnormal operands (the scaled-up branch).
            x = (r & 0x800fffffffffffff) | ((r >> 52) % 600 << 52)
            y = (s & 0x800fffffffffffff) | ((s >> 52) % 600 << 52)
        elif mode == 4:
            # Huge operands (the scaled-down branch) down to the shortcut gap.
            e = 1500 + r % 547
            x = (r & 0x800fffffffffffff) | (e << 52)
            y = (s & 0x800fffffffffffff) | ((e - s % 70) << 52)
        elif mode == 5:
            # ay/ax near 1/sqrt(3), the kernel's h <= 2 * ay split, at any exponent.
            e = 2 + r % 2044
            x = (r & 0x000fffffffffffff) | (e << 52)
            ay = bits(struct.unpack('<d', struct.pack('<Q', x & ~SIGN & MASK))[0] * INV_SQRT3)
            y = (ay + (s % 65) - 32) | ((s >> 63) << 63)
            x |= (r >> 63) << 63
        else:
            # One non-finite operand with random payload, the other random.
            special = (INF | (s & 0x000fffffffffffff if s & 2 else 0)) | ((s >> 63) << 63)
            x, y = (special, r) if s & 1 else (r, special)
        yield x & MASK, y & MASK


def pairs():
    yield from fixed()
    yield from seeded()


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
