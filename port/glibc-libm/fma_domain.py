# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic fma inputs only, never expected results. Little-endian (x, y, z) triples.

Fixed cases follow the branches of glibc 2.39 s_fma.c as built for x86_64 __fma_sse2: the
special-operand returns ((z + x) + y for an infinite or NaN z with finite x and y, x * y for a
zero z, x * y + z for an infinite/NaN operand or a zero factor) with every quiet/signalling NaN
order and sign, the certain-overflow return, the tiny-product return (and its z exponent split at
3), each exponent-adjustment branch and its boundaries (ue + ve against 3017, 3070, 968 and 1076,
ue/ve/we against 1994, we against 53, 218 and 0), exact zero results of both signs, cancellation
(z = -fl(x * y) and neighbours, which leaves only the round-to-odd error term), halfway and
subnormal results. The seeded modes use their own seed. Only exact binary64 arithmetic is used
(a single correctly rounded product for z = -fl(x * y)), never the host libm.
"""
import hashlib
import struct

from domain import MASK, SIGN, bits

COUNT = 1_000_000
SEED = 0x464D41676C696263
MAX_FINITE = 0x7fefffffffffffff
INF = 0x7ff0000000000000
QNANS = [0x7ff8000000000000, 0x7ff80000deadbeef, 0x7fffffffffffffff]
SNANS = [0x7ff0000000000001, 0x7ff4000000000abc]
MANTISSAS = [0, 1, 0x8000000000000, 0x6a09e667f3bcd, 0xfffffffffffff]


class SplitMix:
    def __init__(self): self.state = SEED

    def next(self):
        self.state = (self.state + 0x9e3779b97f4a7c15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xbf58476d1ce4e5b9) & MASK
        z = ((z ^ (z >> 27)) * 0x94d049bb133111eb) & MASK
        return z ^ (z >> 31)


def value(b):
    return struct.unpack('<d', struct.pack('<Q', b & MASK))[0]


def finite(b):
    return (b & ~SIGN & MASK) <= MAX_FINITE


def make(e, m, sign=0):
    """Bits with biased exponent e (0..2046) and mantissa m."""
    return (sign << 63) | (e << 52) | (m & 0x000fffffffffffff)


def neg_product(x, y):
    """-fl(x * y): one correctly rounded IEEE product (Python floats are binary64)."""
    p = value(x) * value(y)
    return bits(-p) if p == p else None


def fixed():
    out = set()

    def add(x, y, z):
        out.add((x & MASK, y & MASK, z & MASK))

    # Special operands: the full Cartesian product with both signs (NaN order and sign, zero
    # signs, infinities against zeros, finite x and y with an infinite or NaN z).
    special = [0, 1, 0x000fffffffffffff, 0x0010000000000000, bits(1.0), bits(1.5), bits(2.0),
               bits(2.0 ** -600), bits(2.0 ** 600), MAX_FINITE, INF] + QNANS + SNANS
    for x in special:
        for y in special:
            for z in special:
                for s in range(8):
                    add(x | (SIGN if s & 1 else 0), y | (SIGN if s & 2 else 0),
                        z | (SIGN if s & 4 else 0))

    # Exponent branches: ue + ve around 3017 (scale down), 3070 (certain overflow), 968 (tiny
    # product) and 1076 (scale up); ue, ve, we around 1994; we around 53, 218, 3 and 0.
    sums = sorted({s + d for s in (968, 1076, 1076 + 53, 3017, 3070) for d in (-2, -1, 0, 1, 2)}
                  | {0, 1, 500, 1023, 1500, 2046, 2500, 3500, 4092})
    zexp = [0, 1, 2, 3, 4, 52, 53, 54, 100, 217, 218, 219, 500, 1023, 1500, 1993, 1994, 1995, 2046]
    for total in sums:
        for ue in sorted({max(0, min(2046, total - ve)) for ve in (0, 1, 53, 1023, 1993, 1994, 2046)}):
            ve = total - ue
            if not 0 <= ve <= 2046:
                continue
            for mx in (MANTISSAS[0], MANTISSAS[3], MANTISSAS[4]):
                x = make(ue, mx)
                y = make(ve, MANTISSAS[(ue + mx) % 5])
                if x == 0 or y == 0:
                    continue
                for we in zexp:
                    for mz in (0, MANTISSAS[2], MANTISSAS[4]):
                        z = make(we, mz)
                        for s in range(4):
                            add(x | (SIGN if s & 1 else 0), y, z | (SIGN if s & 2 else 0))
                # Cancellation against the rounded product, and its neighbours.
                nz = neg_product(x, y)
                if nz is not None and finite(nz):
                    for d in (-2, -1, 0, 1, 2):
                        if finite(nz + d) and (nz + d) & ~SIGN & MASK:
                            add(x, y, nz + d)
                            add(x | SIGN, y | SIGN, nz + d)

    # Exact zero results: x * y == -z exactly, of both signs, and zero products with zero z.
    for a, b in [(3, 5), (7, 9), (1, 1), (12345, 6789)]:
        for k in (-1074, -600, -40, 0, 40, 400):
            for s in range(4):
                x = bits(float(a) * 2.0 ** (k // 2)) | (SIGN if s & 1 else 0)
                y = bits(float(b) * 2.0 ** (k - k // 2)) | (SIGN if s & 2 else 0)
                nz = neg_product(x, y)
                if nz is not None and finite(nz):
                    add(x, y, nz)

    # Halfway results: (1 + 2^-52 a)(1 + 2^-52 b) + z with a product error of exactly half an
    # ulp of the sum, at several scales, so round-to-odd decides the final rounding.
    for a in (1, 3, 0x80000, 0xfffffffffffff):
        for b in (1, 2, 0x40000, 0x8000000000000):
            for e in (1, 60, 1023, 1500, 1990):
                x = make(e, a)
                y = make(1023, b)
                for z in (make(e, 0), make(e - 1, 1), make(e + 1, 0x8000000000000), 0,
                          make(max(0, e - 53), 0), make(max(0, e - 54), 0)):
                    for s in range(2):
                        add(x, y, z | (SIGN if s else 0))

    # Subnormal and tiny results (the adjust < 0 path and its exponent 108/109 checks).
    for ex in (1, 2, 20, 100, 400, 485, 486, 487, 500):
        for ey in (1, 2, 20, 100, 400, 485, 486, 487, 500, 600):
            if ex + ey > 1076 + 53:
                continue
            for mx in MANTISSAS:
                x = make(ex, mx)
                y = make(ey, MANTISSAS[(mx + ey) % 5])
                if x == 0 or y == 0:
                    continue
                for z in (0, 1, 2, 3, 0x000fffffffffffff, 0x0010000000000000, make(1, 1),
                          make(54, 0), make(108, 0), make(109, 1), make(110, 0xfffffffffffff)):
                    add(x, y, z)
                    add(x, y | SIGN, z)
                    add(x, y, z | SIGN)
    return sorted(out)


def seeded():
    rng = SplitMix()
    for i in range(COUNT):
        r, s, t = rng.next(), rng.next(), rng.next()
        mode = i % 7
        if mode == 0:
            x, y, z = r, s, t  # full raw bits, including NaN payloads and infinities
        elif mode == 1:
            # Cancellation: z = -fl(x * y) plus a few ulps, moderate exponents.
            x = (r & 0x800fffffffffffff) | ((700 + r % 650) << 52)
            y = (s & 0x800fffffffffffff) | ((700 + s % 650) << 52)
            nz = neg_product(x, y)
            z = (nz + (t % 9) - 4) if nz is not None and finite(nz) and nz & ~SIGN & MASK > 8 else t
        elif mode == 2:
            # Near overflow: ue + ve around 3017..3075, z of any size.
            total = 3010 + r % 66
            ue = max(1, min(2046, total // 2 + (s % 200) - 100))
            ve = max(1, min(2046, total - ue))
            x = (r & 0x800fffffffffffff) | (ue << 52)
            y = (s & 0x800fffffffffffff) | (ve << 52)
            z = (t & 0x800fffffffffffff) | ((t >> 52) % 2047 << 52)
        elif mode == 3:
            # Tiny products and subnormal results: ue + ve around 900..1140, small z.
            total = 900 + r % 241
            ue = max(0, min(2046, total // 2 + (s % 300) - 150))
            ve = max(0, min(2046, total - ue))
            x = (r & 0x800fffffffffffff) | (ue << 52)
            y = (s & 0x800fffffffffffff) | (ve << 52)
            z = (t & 0x800fffffffffffff) | (((t >> 52) % 240) << 52)
        elif mode == 4:
            # Huge z (we >= 1994) with products of any size.
            x = (r & 0x800fffffffffffff) | ((r >> 52) % 2047 << 52)
            y = (s & 0x800fffffffffffff) | ((s >> 52) % 2047 << 52)
            z = (t & 0x800fffffffffffff) | ((1990 + t % 57) << 52)
        elif mode == 5:
            # One non-finite operand (random payload), the others random.
            special = (INF | (t & 0x000fffffffffffff if t & 2 else 0)) | ((t >> 63) << 63)
            x, y, z = r, s, t
            k = (t >> 2) % 3
            x, y, z = (special, y, z) if k == 0 else (x, special, z) if k == 1 else (x, y, special)
        else:
            # Moderate normal operands of similar magnitude.
            x = (r & 0x800fffffffffffff) | ((1000 + r % 48) << 52)
            y = (s & 0x800fffffffffffff) | ((1000 + s % 48) << 52)
            z = (t & 0x800fffffffffffff) | ((990 + t % 70) << 52)
        yield x & MASK, y & MASK, z & MASK


def triples():
    yield from fixed()
    yield from seeded()


def write(path):
    digest = hashlib.sha256()
    count = 0
    fixed_count = len(fixed())
    with open(path, 'wb') as out:
        for x, y, z in triples():
            b = struct.pack('<QQQ', x, y, z)
            out.write(b)
            digest.update(b)
            count += 1
    return dict(fixed_count=fixed_count, seeded_count=COUNT, count=count, sha256=digest.hexdigest())


if __name__ == '__main__':
    import json, sys
    print(json.dumps(write(sys.argv[1]), sort_keys=True))
