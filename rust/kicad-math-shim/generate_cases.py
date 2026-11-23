#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write the public, deterministic matrix3x3/wide_int requests (LF UTF-8).

All data is generated here from fixed seeds and boundary tables; no design data is used.
Binary64 values are 16 lowercase hex digits of their IEEE-754 bits.

The first 3,485 requests are the original corpus, qualified unchanged on Linux x86-64; their
arithmetic cases happen to combine NaNs of one bit pattern only. Inputs with two or more
different NaNs are valid and are covered by the retained mixed-NaN regressions, which
reproduce the qualified request set byte for byte (generator sha256 6b334d54..., requests
sha256 85fdf99b...). Later groups add the MATRIX3x3<int> instantiation, explicit stream state
and the glibc CPU-variant-sensitive rotation angles. Groups are appended so earlier requests
never change.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct

UNIT = "libs.kimath.include.math.application.pure.p001"
SEED = 0x4B694D617468  # "KiMath"
INT64_MIN, INT64_MAX = -(1 << 63), (1 << 63) - 1
UINT64_MAX = (1 << 64) - 1
INT128_MIN, INT128_MAX = -(1 << 127), (1 << 127) - 1


def bits(value):
    return struct.pack(">d", value).hex()


def raw(pattern):
    return f"{pattern:016x}"


ZERO, NEG_ZERO, ONE = bits(0.0), bits(-0.0), bits(1.0)
INF, NEG_INF = raw(0x7FF0000000000000), raw(0xFFF0000000000000)
DEFAULT_NAN = raw(0xFFF8000000000000)  # x86-64 default (generated) NaN
QUIET_NAN = raw(0x7FF8000000000000)
PAYLOAD_NAN = raw(0x7FF80000DEADBEEF)
NEG_PAYLOAD_NAN = raw(0xFFF4000000000001)  # signalling, negative
SIGNALLING_NAN = raw(0x7FF0000000000001)
MIN_SUBNORMAL, MAX_SUBNORMAL = raw(1), raw(0x000FFFFFFFFFFFFF)
MIN_NORMAL, MAX_FINITE = raw(0x0010000000000000), raw(0x7FEFFFFFFFFFFFFF)

FINITE_SPECIALS = [ZERO, NEG_ZERO, ONE, bits(-1.0), bits(0.5), bits(2.0), bits(math.pi),
                   bits(1 / 3), bits(0.1), bits(-1e-300), bits(1e300), MIN_SUBNORMAL,
                   MAX_SUBNORMAL, MIN_NORMAL, MAX_FINITE, raw(0xFFEFFFFFFFFFFFFF)]
COPY_SPECIALS = FINITE_SPECIALS + [INF, NEG_INF, DEFAULT_NAN, QUIET_NAN, PAYLOAD_NAN,
                                   NEG_PAYLOAD_NAN, SIGNALLING_NAN]
IDENTITY = [[ONE, ZERO, ZERO], [ZERO, ONE, ZERO], [ZERO, ZERO, ONE]]


def matrix(values):
    return [list(values[0:3]), list(values[3:6]), list(values[6:9])]


class Gen:
    """Fixed-seed value generator."""

    def __init__(self):
        self.rng = random.Random(SEED)

    def moderate(self):
        """A finite double of moderate magnitude (no overflow in matrix products)."""
        kind = self.rng.randrange(4)
        if kind == 0:
            return bits(float(self.rng.randint(-1000, 1000)))
        if kind == 1:
            return bits(self.rng.uniform(-1.0, 1.0))
        if kind == 2:
            return bits(self.rng.uniform(-1e6, 1e6))
        return bits(math.ldexp(self.rng.uniform(0.5, 1.0) * self.rng.choice([-1, 1]),
                               self.rng.randint(-60, 60)))

    def finite_bits(self):
        """Any finite double, from its bits (huge, tiny and subnormal included)."""
        while True:
            pattern = self.rng.getrandbits(64)
            if (pattern >> 52) & 0x7FF != 0x7FF:
                return raw(pattern)

    def matrix(self, value=None):
        value = value or self.moderate
        return matrix([value() for _ in range(9)])

    def vector(self, n, value=None):
        value = value or self.moderate
        return [value() for _ in range(n)]

    def int64(self):
        kind = self.rng.randrange(4)
        if kind == 0:
            return self.rng.randint(INT64_MIN, INT64_MAX)
        if kind == 1:  # 33-bit coordinate deltas, the documented KiCad use
            return self.rng.randint(-(1 << 32), 1 << 32)
        if kind == 2:
            return self.rng.choice([INT64_MIN, INT64_MIN + 1, -1, 0, 1, INT64_MAX - 1, INT64_MAX])
        return self.rng.randint(-1000, 1000)


def cases():
    gen = Gen()
    out = []

    def case(name, **input_fields):
        out.append({"unit": UNIT, "case": name, "input": input_fields})

    # Constructors.
    case("matrix-default", op="matrix_default")
    case("from-vectors-specials", op="matrix_from_vectors", a1=COPY_SPECIALS[0:3],
         a2=COPY_SPECIALS[16:19], a3=COPY_SPECIALS[20:23])
    for i in range(12):
        case(f"from-vectors-{i}", op="matrix_from_vectors", a1=gen.vector(3, gen.finite_bits),
             a2=gen.vector(3), a3=gen.vector(3))
    for i in range(0, len(COPY_SPECIALS), 9):
        chunk = (COPY_SPECIALS[i:i + 9] + COPY_SPECIALS)[:9]
        case(f"from-elements-specials-{i // 9}", op="matrix_from_elements", elements=matrix(chunk))
    for i in range(8):
        case(f"from-elements-{i}", op="matrix_from_elements", elements=gen.matrix(gen.finite_bits))

    # Setters/getters: every other member must be preserved bit for bit.
    special_matrix = matrix([PAYLOAD_NAN, NEG_ZERO, SIGNALLING_NAN, INF, DEFAULT_NAN, NEG_INF,
                             MIN_SUBNORMAL, MAX_FINITE, NEG_PAYLOAD_NAN])
    for i, m in enumerate([IDENTITY, special_matrix] + [gen.matrix(gen.finite_bits) for _ in range(4)]):
        case(f"set-identity-{i}", op="set_identity", matrix=m)
        case(f"get-translation-{i}", op="get_translation", matrix=m)
        case(f"get-scale-{i}", op="get_scale", matrix=m)
    translations = [[ZERO, NEG_ZERO], [INF, DEFAULT_NAN], [PAYLOAD_NAN, SIGNALLING_NAN],
                    [MAX_FINITE, MIN_SUBNORMAL]] + [gen.vector(2, gen.finite_bits) for _ in range(4)]
    for i, v in enumerate(translations):
        m = special_matrix if i % 2 else gen.matrix()
        case(f"set-translation-{i}", op="set_translation", matrix=m, translation=v)
        case(f"set-scale-{i}", op="set_scale", matrix=m, scale=v)

    # SetRotation: cos/sin of the angle, other members preserved.
    angles = [ZERO, NEG_ZERO, bits(math.pi / 2), bits(math.pi), bits(-math.pi), bits(2 * math.pi),
              bits(math.pi / 4), bits(-math.pi / 6), bits(1e-300), MIN_SUBNORMAL, bits(1e-8),
              bits(1.0), bits(-1.0), bits(1e22), bits(-1e22), bits(1e300), MAX_FINITE,
              raw(0xFFEFFFFFFFFFFFFF), bits(2.0 ** 1023), bits(3.0 * 2 ** 27), INF, NEG_INF,
              DEFAULT_NAN, QUIET_NAN, PAYLOAD_NAN]
    angles += [bits(math.radians(d)) for d in (30, 45, 60, 90, 120, 135, 180, 270, 360, -90)]
    angles += [bits(gen.rng.uniform(-10.0, 10.0)) for _ in range(160)]
    angles += [bits(gen.rng.uniform(-1e6, 1e6)) for _ in range(40)]
    angles += [gen.finite_bits() for _ in range(40)]
    for i, angle in enumerate(angles):
        m = special_matrix if i % 5 == 0 else (IDENTITY if i % 5 == 1 else gen.matrix())
        case(f"set-rotation-{i}", op="set_rotation", matrix=m, angle=angle)

    # Determinant and inverse.
    arithmetic = [
        ("identity", IDENTITY),
        ("zero", matrix([ZERO] * 9)),
        ("negative-zero", matrix([NEG_ZERO] * 9)),
        ("singular-rows", matrix([bits(v) for v in (1, 2, 1, 2, 4, 2, 1, 1, 1)])),
        ("singular-negative", matrix([bits(v) for v in (-1, 2, 1, 2, -4, -2, 1, 1, 1)])),
        ("rank-one", matrix([bits(v) for v in (1, 2, 3, 2, 4, 6, 3, 6, 9)])),
        ("integer-det-minus-3", matrix([bits(v) for v in (1, 2, 3, 4, 5, 6, 7, 8, 10)])),
        ("near-singular", matrix([bits(v) for v in (1, 2, 3, 4, 5, 6, 7, 8, 9 + 2 ** -40)])),
        ("affine", matrix([bits(v) for v in (0.5, -0.25, 100.0, 0.25, 0.5, -200.0, 0, 0, 1)])),
        ("huge-overflow", matrix([MAX_FINITE, ONE, ZERO, ONE, MAX_FINITE, ZERO, ZERO, ZERO, MAX_FINITE])),
        ("huge-cancel", matrix([bits(1e200), bits(1e200), ZERO, bits(1e200), bits(1e200), ZERO,
                                ZERO, ZERO, ONE])),
        ("tiny-underflow", matrix([bits(1e-200), ZERO, ZERO, ZERO, bits(1e-200), ZERO, ZERO, ZERO,
                                   bits(1e-200)])),
        ("subnormal-diagonal", matrix([MIN_SUBNORMAL, ZERO, ZERO, ZERO, ONE, ZERO, ZERO, ZERO, ONE])),
        ("infinity", matrix([INF, ZERO, ZERO, ZERO, ONE, ZERO, ZERO, ZERO, ONE])),
        ("infinities", matrix([INF, NEG_INF, ONE, ONE, INF, ZERO, NEG_INF, ONE, ONE])),
        ("default-nan-with-infinity", matrix([DEFAULT_NAN, INF, ONE, ONE, ONE, ZERO, ZERO, ZERO, ONE])),
        ("quiet-nan-only", matrix([ONE, ZERO, ZERO, ZERO, QUIET_NAN, ZERO, ZERO, ZERO, ONE])),
        ("payload-nan-only", matrix([bits(2.0), ONE, ZERO, ONE, bits(3.0), PAYLOAD_NAN, ZERO, ONE, ONE])),
    ]
    arithmetic += [(f"random-{i}", gen.matrix()) for i in range(40)]
    arithmetic += [(f"random-bits-{i}", gen.matrix(gen.finite_bits)) for i in range(20)]
    for name, m in arithmetic:
        case(f"determinant-{name}", op="determinant", matrix=m)
        case(f"inverse-{name}", op="inverse", matrix=m)
        case(f"transpose-{name}", op="transpose", matrix=m)
        case(f"stream-{name}", op="stream", matrix=m)
    case("transpose-specials", op="transpose", matrix=special_matrix)

    # Equality: equal, signed zero, NaN, and a difference in each position (each && / ||).
    base = gen.matrix()
    case("compare-identical", op="compare", matrix=base, other=base)
    case("compare-signed-zero", op="compare", matrix=matrix([ZERO] * 9), other=matrix([NEG_ZERO] * 9))
    case("compare-nan-self", op="compare", matrix=special_matrix, other=special_matrix)
    case("compare-infinity", op="compare", matrix=matrix([INF] * 9), other=matrix([INF] * 9))
    flat = [value for row in base for value in row]
    for position in range(9):
        other = list(flat)
        other[position] = bits(struct.unpack(">d", bytes.fromhex(other[position]))[0] + 1.0)
        case(f"compare-differs-{position}", op="compare", matrix=base, other=matrix(other))
        nan = list(flat)
        nan[position] = DEFAULT_NAN
        case(f"compare-nan-{position}", op="compare", matrix=matrix(nan), other=matrix(nan))

    # Products.
    products = [("identity", IDENTITY, IDENTITY), ("identity-left", IDENTITY, base),
                ("zero-infinity", matrix([ZERO] * 9), matrix([INF] * 9)),
                ("signed-zeros", matrix([NEG_ZERO] * 9), matrix([ZERO] * 9)),
                ("overflow", matrix([MAX_FINITE] * 9), matrix([bits(2.0)] * 9)),
                ("default-nan", matrix([DEFAULT_NAN] + [ONE] * 8), matrix([INF] * 9))]
    products += [(f"random-{i}", gen.matrix(), gen.matrix()) for i in range(40)]
    products += [(f"random-bits-{i}", gen.matrix(gen.finite_bits), gen.matrix(gen.finite_bits))
                 for i in range(10)]
    for name, a, b in products:
        case(f"mul-matrix-{name}", op="mul_matrix", a=a, b=b)
    vectors = [("identity", IDENTITY, [ONE, bits(2.0), bits(3.0)]),
               ("zero-infinity", matrix([ZERO] * 9), [INF, NEG_INF, INF]),
               ("signed-zeros", matrix([NEG_ZERO] * 9), [ZERO, NEG_ZERO, ZERO]),
               ("payload-nan-only", IDENTITY, [PAYLOAD_NAN, ONE, ONE])]
    vectors += [(f"random-{i}", gen.matrix(), gen.vector(3)) for i in range(30)]
    vectors += [(f"random-bits-{i}", gen.matrix(gen.finite_bits), gen.vector(3, gen.finite_bits))
                for i in range(10)]
    for name, m, v in vectors:
        case(f"mul-vector2-{name}", op="mul_vector2", matrix=m, vector=v[:2])
        case(f"mul-vector3-{name}", op="mul_vector3", matrix=m, vector=v)
    scalars = [ZERO, NEG_ZERO, ONE, bits(-0.5), bits(1 / 3), INF, NEG_INF, DEFAULT_NAN, MAX_FINITE,
               MIN_SUBNORMAL] + [gen.moderate() for _ in range(10)]
    for i, scalar in enumerate(scalars):
        m = gen.matrix() if i % 2 else matrix([ZERO, NEG_ZERO, ONE, bits(-1.0), MAX_FINITE,
                                               MIN_SUBNORMAL, bits(0.5), bits(3.0), bits(-7.0)])
        case(f"mul-scalar-{i}", op="mul_scalar", matrix=m, scalar=scalar)
        case(f"scalar-mul-{i}", op="scalar_mul", scalar=scalar, matrix=m)

    # Stream formatting: %.6g boundaries, exact ties, signs of zero and NaN.
    stream_values = [0.0, -0.0, 1.0, -1.0, 0.5, 0.125, 1 / 3, 2 / 3, 123456.0, 1234565.0,
                     1234575.0, 999999.4, 999999.5, 9.9999950, 99999.95, 0.0001, 0.00001,
                     0.000123456500000000, 1e-5, 9.99999e-5, 1e5, 1e6, 1e15, 1e16, 1e100,
                     -2.5e-300, 1e-310, 5e-324, 2.2250738585072014e-308, 1.7976931348623157e308]
    stream_bits = [bits(v) for v in stream_values] + [INF, NEG_INF, DEFAULT_NAN, QUIET_NAN,
                                                     PAYLOAD_NAN, NEG_PAYLOAD_NAN, SIGNALLING_NAN]
    stream_bits += [gen.finite_bits() for _ in range(27)]
    for i in range(0, len(stream_bits), 9):
        chunk = (stream_bits[i:i + 9] + stream_bits)[:9]
        case(f"stream-values-{i // 9}", op="stream", matrix=matrix(chunk))

    # WordsToDouble: every shift, exact ties, sticky bits, negation carries.
    words = [(0, 0), (0, 1), (0, UINT64_MAX), (1, 0), (-1, 0), (-1, UINT64_MAX), (-1, 1),
             (INT64_MIN, 0), (INT64_MIN, 1), (INT64_MAX, UINT64_MAX), (INT64_MIN + 1, 0),
             (0, 1 << 63), (0, (1 << 53) + 1), (1, 1 << 11), (1, (1 << 11) + 1), (1, 3 << 11)]
    for shift in range(64):
        hi = 1 << (63 - shift)
        if hi > INT64_MAX:
            hi = INT64_MIN  # top bit set: the negative branch with the minimum word
        for lo in (0, 1, 1 << 63, UINT64_MAX, (1 << 63) - 1):
            words.append((hi, lo))
            words.append((-hi if hi != INT64_MIN else INT64_MIN, lo))
    # Exact halfway cases for every positive leading-zero count of hi: the 128-bit value has
    # 128 - shift significant bits, so the rounding bit is 74 - shift. Ties to even round down
    # (even), ties with an odd last significand bit round up, and any lower bit (folded into the
    # sticky bit by the C++) rounds up. Negated values take the two's-complement branch.
    for shift in range(1, 64):
        leading, rounding = 1 << (127 - shift), 1 << (74 - shift)
        for value in (leading | rounding, leading | rounding | (rounding << 1),
                      leading | rounding | 1, leading | (rounding - 1)):
            for signed in (value, -value):
                words.append((signed >> 64, signed & UINT64_MAX))
    for _ in range(300):
        words.append((gen.int64(), gen.rng.getrandbits(64)))
    for i, (hi, lo) in enumerate(words):
        case(f"words-to-double-{i}", op="words_to_double", hi=hi, lo=lo)

    # ToDouble: boundaries, binary64 rounding ties at every exponent, fixed-seed values.
    values = [0, 1, -1, INT128_MAX, INT128_MIN, INT128_MIN + 1, 1 << 64, -(1 << 64), (1 << 64) - 1,
              INT64_MIN, INT64_MAX, UINT64_MAX, (1 << 53) + 1, -((1 << 53) + 1)]
    for exponent in range(53, 127, 3):
        unit = 1 << (exponent - 53)  # one ULP at 2^exponent
        tie = (1 << exponent) + unit // 2 if unit > 1 else None
        for value in ((1 << exponent) + unit, (1 << exponent) + 3 * unit // 2 if unit > 1 else None,
                      tie, tie + 1 if tie else None):
            if value is not None and value <= INT128_MAX:
                values += [value, -value]
    for _ in range(300):
        values.append(gen.rng.randint(INT128_MIN, INT128_MAX) >> gen.rng.randrange(128))
    for i, value in enumerate(values):
        case(f"to-double-{i}", op="to_double", value=str(value))

    # CrossWide / DotWide: extremes, 33-bit deltas, fixed-seed values.
    extremes = [INT64_MIN, INT64_MIN + 1, -(1 << 32), -1, 0, 1, (1 << 32), INT64_MAX]
    pairs = []
    for ax in extremes:
        for by in extremes:
            pairs.append(([ax, INT64_MIN], [INT64_MAX, by]))
            pairs.append(([ax, by], [by, ax]))
    pairs += [([gen.int64(), gen.int64()], [gen.int64(), gen.int64()]) for _ in range(200)]
    for i, (a, b) in enumerate(pairs):
        case(f"cross-wide-{i}", op="cross_wide", a=a, b=b)
        all_min = a == [INT64_MIN, INT64_MIN] and b == [INT64_MIN, INT64_MIN]
        if not all_min:  # the single undefined-behaviour input is a refusal, tested separately
            case(f"dot-wide-{i}", op="dot_wide", a=a, b=b)
    case("dot-wide-max-defined", op="dot_wide", a=[INT64_MIN, INT64_MIN], b=[INT64_MIN, INT64_MIN + 1])

    out += integer_cases(gen)
    out += stream_cases(gen)
    out += variant_rotation_cases()
    out += mixed_nan_cases()

    names = [c["case"] for c in out]
    if len(names) != len(set(names)):
        raise ValueError("duplicate case id")
    return out


INT_MIN, INT_MAX = -(1 << 31), (1 << 31) - 1


def int_matrix(values):
    return [list(values[0:3]), list(values[3:6]), list(values[6:9])]


def integer_cases(gen):
    """MATRIX3x3<int>, as instantiated by the pinned qa/tests/libs/kimath/math/test_matrix3x3.cpp.

    Only inputs whose C++ result is defined are cases; signed overflow and non-representable
    double-to-int conversions are refusals, checked by test_refusals.py.
    """
    rng = gen.rng
    out = []

    def case(name, **fields):
        out.append({"unit": UNIT, "case": f"int-{name}", "input": {"type": "int", **fields}})

    small = lambda: rng.randint(-1000, 1000)
    qa = int_matrix([1, 1, 1, 2, 2, 2, 3, 3, 3])
    # The three QA checks: CTAD from VECTOR3I rows, m1 * VECTOR3I( 5, 5, 5 ), m1 * 5, and ==.
    case("qa-from-vectors", op="matrix_from_vectors", a1=[1, 1, 1], a2=[2, 2, 2], a3=[3, 3, 3])
    case("qa-multiply-vector", op="mul_vector3", matrix=qa, vector=[5, 5, 5])
    case("qa-multiply-scalar", op="mul_scalar", scalar_type="int", matrix=qa, scalar=5)
    case("qa-expected-equal", op="compare", matrix=int_matrix([5, 5, 5, 10, 10, 10, 15, 15, 15]),
         other=int_matrix([5, 5, 5, 10, 10, 10, 15, 15, 15]))

    extremes = [INT_MIN, INT_MIN + 1, -1, 0, 1, INT_MAX - 1, INT_MAX, 46340, -46341]
    case("matrix-default", op="matrix_default")
    case("from-elements-extremes", op="matrix_from_elements", elements=int_matrix(extremes))
    case("from-vectors-extremes", op="matrix_from_vectors", a1=extremes[0:3], a2=extremes[3:6],
         a3=extremes[6:9])
    base = int_matrix(extremes)
    for i, m in enumerate([qa, base, int_matrix([small() for _ in range(9)])]):
        case(f"set-identity-{i}", op="set_identity", matrix=m)
        case(f"get-translation-{i}", op="get_translation", matrix=m)
        case(f"get-scale-{i}", op="get_scale", matrix=m)
        case(f"transpose-{i}", op="transpose", matrix=m)
        case(f"set-translation-{i}", op="set_translation", matrix=m, translation=[INT_MIN, INT_MAX])
        case(f"set-scale-{i}", op="set_scale", matrix=m, scale=[-7, INT_MAX])
    flat = [small() for _ in range(9)]
    case("compare-identical", op="compare", matrix=int_matrix(flat), other=int_matrix(flat))
    for position in range(9):
        other = list(flat)
        other[position] += 1
        case(f"compare-differs-{position}", op="compare", matrix=int_matrix(flat),
             other=int_matrix(other))

    # SetRotation<int>: cos/sin of the integer angle truncated toward zero. These integers are
    # close enough to multiples of pi that the double cosine is exactly +-1 (or, for 80143857,
    # one ulp below 1 and truncated to 0).
    angles = [0, 1, -1, 2, 3, 4, 7, 100, -100, INT_MAX, INT_MIN, 80143857, 165707065, 245850922,
              411557987, 823115974, 1068966896, 1234673961, 2137933792, -2137933792]
    angles += [rng.randint(INT_MIN, INT_MAX) for _ in range(20)]
    for i, angle in enumerate(angles):
        case(f"set-rotation-{i}", op="set_rotation", matrix=qa if i % 2 else base, angle=angle)

    # Arithmetic with defined results: small values, products at the int boundary, unimodular
    # and non-unimodular inverses (truncation toward zero of negative quotients).
    defined = [qa, int_matrix([0] * 9), int_matrix([46340, 0, 0, 0, 46340, 0, 0, 0, 1]),
               int_matrix([INT_MAX, 0, 0, 0, 1, 0, 0, 0, 1]),
               int_matrix([INT_MIN, 0, 0, 0, 1, 0, 0, 0, 1])]
    defined += [int_matrix([small() for _ in range(9)]) for _ in range(20)]
    for i, m in enumerate(defined):
        case(f"determinant-{i}", op="determinant", matrix=m)
    inverses = [int_matrix([1, 0, 0, 0, 1, 0, 0, 0, 1]), int_matrix([2, 1, 0, 1, 1, 0, 0, 0, 1]),
                int_matrix([2, 0, 0, 0, 2, 0, 0, 0, 1]), int_matrix([-3, 1, 0, 1, 1, 0, 0, 0, -1]),
                int_matrix([1, 2, 3, 0, 1, 4, 5, 6, 0]), int_matrix([4, 7, 2, 3, 6, 1, 2, 5, 3]),
                int_matrix([INT_MAX, 0, 0, 0, 1, 0, 0, 0, 1])]
    for i, m in enumerate(inverses):
        case(f"inverse-{i}", op="inverse", matrix=m)
    for i in range(12):
        a = int_matrix([small() for _ in range(9)])
        b = int_matrix([small() for _ in range(9)])
        case(f"mul-matrix-{i}", op="mul_matrix", a=a, b=b)
        case(f"mul-vector2-{i}", op="mul_vector2", matrix=a, vector=[small(), small()])
        case(f"mul-vector3-{i}", op="mul_vector3", matrix=a, vector=[small(), small(), small()])
    case("mul-matrix-boundary", op="mul_matrix", a=int_matrix([46340, 0, 0, 0, 1, 0, 0, 0, 1]),
         b=int_matrix([46340, 0, 0, 0, INT_MAX, 0, 0, 0, INT_MIN]))
    case("mul-vector2-boundary", op="mul_vector2",
         matrix=int_matrix([1, 1, INT_MIN + 2, 0, 1, INT_MAX - 3, 0, 0, 1]), vector=[1, 1])
    for i, scalar in enumerate([0, 1, -1, 5, 46340, -46341]):
        m = int_matrix([46340, -1, 0, 1, 2, 3, 0, 0, 1]) if abs(scalar) > 5 else qa
        case(f"mul-scalar-int-{i}", op="mul_scalar", scalar_type="int", matrix=m, scalar=scalar)
        case(f"scalar-mul-int-{i}", op="scalar_mul", scalar_type="int", scalar=scalar, matrix=m)
    doubles = [bits(0.5), bits(-0.5), bits(0.25), bits(-1.0), bits(1e-300), NEG_ZERO,
               bits(2147483647.0 / 2147483647.0), bits(1.9999999999999998), bits(-2.5)]
    for i, scalar in enumerate(doubles):
        m = int_matrix([1, -1, 3, -3, 7, -7, INT_MAX, INT_MIN + 1, 0])
        if scalar in (bits(1.9999999999999998), bits(-2.5)):
            m = qa
        case(f"mul-scalar-double-{i}", op="mul_scalar", scalar_type="double", matrix=m,
             scalar=scalar)
        case(f"scalar-mul-double-{i}", op="scalar_mul", scalar_type="double", scalar=scalar,
             matrix=m)

    # MATRIX3x3<double> with S = int, also instantiable.
    for i, scalar in enumerate([0, -1, 5, INT_MIN, INT_MAX]):
        m = gen.matrix(gen.finite_bits) if i % 2 else gen.matrix()
        out.append({"unit": UNIT, "case": f"double-mul-scalar-int-{i}",
                    "input": {"op": "mul_scalar", "scalar_type": "int", "matrix": m, "scalar": scalar}})
        out.append({"unit": UNIT, "case": f"double-scalar-mul-int-{i}",
                    "input": {"op": "scalar_mul", "scalar_type": "int", "scalar": scalar, "matrix": m}})
    return out


def state(flags=(), precision=6, width=0, fill=" "):
    return {"flags": list(flags), "precision": precision, "width": width, "fill": fill}


def stream_cases(gen):
    """operator<< on a caller stream with explicit flags, precision, width and fill.

    The classic locale is the only supported locale (the oracle never changes it).
    """
    out = []

    def case(name, matrix, st, element="double"):
        fields = {"op": "stream", "matrix": matrix, "state": st}
        if element == "int":
            fields["type"] = "int"
        out.append({"unit": UNIT, "case": f"stream-state-{name}", "input": fields})

    eighths = matrix([bits(0.125)] + [ZERO] * 3 + [bits(0.125)] + [ZERO] * 3 + [bits(0.125)])
    values = matrix([bits(0.125), NEG_ZERO, INF, DEFAULT_NAN, MIN_SUBNORMAL, bits(100.0),
                     bits(1.5), bits(-3.14), MAX_FINITE])
    tiny = matrix([bits(1e-5), bits(123456.5), bits(0.5), bits(9.5), bits(2.5e-310), QUIET_NAN,
                   NEG_INF, bits(1e21), bits(-0.000123456789)])
    # The reviewer's example: fixed, precision 2 prints 0.125 as 0.12 (ties to even).
    case("fixed-2-eighths", eighths, state(["fixed"], 2))
    case("explicit-default", values, state(["skipws", "dec"], 6))
    fields = {"default": [], "fixed": ["fixed"], "scientific": ["scientific"],
              "hexfloat": ["fixed", "scientific"]}
    modifiers = {"plain": [], "showpos": ["showpos"], "showpoint": ["showpoint"],
                 "uppercase": ["uppercase"], "all": ["showpos", "showpoint", "uppercase"]}
    for field, field_flags in fields.items():
        for modifier, modifier_flags in modifiers.items():
            for precision in (-1, 0, 1, 2, 6, 17, 40):
                if field == "hexfloat" and precision not in (-1, 6):
                    continue  # hexfloat ignores precision; two probes suffice
                for name, m in (("values", values), ("tiny", tiny)):
                    case(f"{field}-{modifier}-p{precision}-{name}", m,
                         state(["dec"] + field_flags + modifier_flags, precision))
    case("fixed-p512-max", matrix([MAX_FINITE] * 9), state(["fixed"], 512))
    for width in (0, 1, 2, 3, 10):
        for adjust in ("none", "left", "right", "internal"):
            flags = ["dec"] + ([] if adjust == "none" else [adjust])
            case(f"width-{width}-{adjust}", values, state(flags, 6, width, "*"))
    case("width-negative", values, state(["dec"], 6, -5, "#"))
    case("left-and-right", values, state(["left", "right"], 6, 9, "."))
    ints = int_matrix([-1, 255, 0, 2, INT_MIN, INT_MAX, 3, -46341, 8])
    int_states = {
        "default": state(["skipws", "dec"]),
        "showpos": state(["dec", "showpos"]),
        "hex": state(["hex"]),
        "hex-showbase": state(["hex", "showbase"]),
        "hex-showbase-upper": state(["hex", "showbase", "uppercase"]),
        "oct": state(["oct"]),
        "oct-showbase": state(["oct", "showbase", "showpos"]),
        "dec-and-hex": state(["dec", "hex"]),
        "none": state([]),
        "width-left": state(["hex", "left"], 6, 12, "_"),
        "width-internal": state(["dec", "internal", "showpos"], 6, 12, "0"),
        "fixed-ignored": state(["dec", "fixed", "showpoint"], 2),
    }
    for name, st in int_states.items():
        case(f"int-{name}", ints, st, element="int")
    for i in range(6):
        st = state(gen.rng.sample(FLAG_NAMES, gen.rng.randint(0, 6)), gen.rng.randint(-2, 20),
                   gen.rng.randint(0, 8), gen.rng.choice("*_ 0."))
        case(f"random-{i}", gen.matrix(gen.finite_bits), st)
        case(f"random-int-{i}", int_matrix([gen.rng.randint(INT_MIN, INT_MAX) for _ in range(9)]),
             st, element="int")
    return out


FLAG_NAMES = ["boolalpha", "dec", "fixed", "hex", "internal", "left", "oct", "right",
              "scientific", "showbase", "showpoint", "showpos", "skipws", "unitbuf", "uppercase"]

# Angles where the qualified WSL Linux x86-64 probe (glibc 2.43, Ryzen 9 5950X) found glibc's
# default (FMA) and GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX2,-FMA,-FMA4,-AVX variants of sin or
# cos one ulp apart. Within one environment C++ and Rust agree; the golden depends on which
# variant produced it.
VARIANT_SIN_ANGLES = ["bfc9720f0476d1e0", "bfcf21071b2cc010", "bfeed2645782e8ec",
                      "bfea5996ecc71804", "3fd06645ac943414"]
VARIANT_COS_ANGLES = ["3fea18772c9fb53e", "3febe296e65b7d7a", "bfeea6d3571c8b74",
                      "bfe91c367a4e70fa", "bfedd9d1d8dfa03e"]


def variant_rotation_cases():
    out = []
    for kind, angles in (("sin", VARIANT_SIN_ANGLES), ("cos", VARIANT_COS_ANGLES)):
        for i, angle in enumerate(angles):
            out.append({"unit": UNIT, "case": f"set-rotation-variant-{kind}-{i}",
                        "input": {"op": "set_rotation", "matrix": IDENTITY, "angle": angle}})
    return out


# The retained mixed-NaN regressions: the exact request set of the WSL qualification of
# 8dd2dc0b (scripts/gen_mixed_nan.py, sha256 6b334d54...), regenerated here with the same
# seed, pool and draw order. Valid inputs; the qualified oracle and Rust disagreed on 1,824.
MIXED_NAN_SHA256 = "85fdf99b914de6ce87c5288da470b4bd9b64e57cf9575eb3ff445ed032db6a2b"
MIXED_NANS = ["7ff8000000000001", "7ff8000000000002", "fff8000000000003", "7ff0000000000001",
              "fff0000000000002"]
MIXED_POOL = MIXED_NANS + ["0000000000000000", "3ff0000000000000", "4000000000000000"]
MIXED_LAYOUT = {"determinant": [("matrix", 9)], "inverse": [("matrix", 9)],
                "mul_matrix": [("a", 9), ("b", 9)],
                "mul_vector2": [("matrix", 9), ("vector", 2)],
                "mul_vector3": [("matrix", 9), ("vector", 3)],
                "mul_scalar": [("matrix", 9), ("scalar", 1)],
                "scalar_mul": [("scalar", 1), ("matrix", 9)]}


def mixed_nan_cases():
    rng = random.Random(50)
    out = [{"unit": UNIT, "case": "minimal-qnan-determinant",
            "input": {"op": "determinant", "matrix": [
                ["7ff8000000000001", "0000000000000000", "3ff0000000000000"],
                ["fff8000000000002", "0000000000000000", "0000000000000000"],
                ["0000000000000000", "0000000000000000", "0000000000000000"]]}}]
    for op, layout in MIXED_LAYOUT.items():
        for index in range(500):
            total = sum(n for _, n in layout)
            flat = [rng.choice(MIXED_POOL) for _ in range(total)]
            # Two distinct NaN patterns at two distinct positions.
            a, b = rng.sample(MIXED_NANS, 2)
            i, j = rng.sample(range(total), 2)
            flat[i], flat[j] = a, b
            fields, k = {"op": op}, 0
            for name, n in layout:
                part = flat[k:k + n]
                k += n
                fields[name] = [part[0:3], part[3:6], part[6:9]] if n == 9 else (
                    part[0] if n == 1 else part)
            out.append({"unit": UNIT, "case": f"mixed-nan-{op}-{index}", "input": fields})
    text = "".join(json.dumps(c, separators=(",", ":")) + "\n" for c in out)
    if hashlib.sha256(text.encode()).hexdigest() != MIXED_NAN_SHA256:
        raise ValueError("mixed-NaN regressions differ from the qualified request set")
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default = Path(__file__).resolve().parents[2] / "port" / "cases" / f"{UNIT}.jsonl"
    parser.add_argument("--output", type=Path, default=default)
    args = parser.parse_args()
    text = "".join(json.dumps(c, separators=(",", ":")) + "\n" for c in cases())
    args.output.write_bytes(text.encode("utf-8"))
    print(f"{args.output}: {text.count(chr(10))} cases")


if __name__ == "__main__":
    main()
