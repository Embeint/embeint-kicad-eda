#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write the public, deterministic matrix3x3/wide_int requests (LF UTF-8).

All data is generated here from fixed seeds and boundary tables; no design data is used.
Binary64 values are 16 lowercase hex digits of their IEEE-754 bits.

NaN selection policy (see README): copy-only operations carry arbitrary NaN payloads and
signalling NaNs. Arithmetic cases combine NaNs of a single bit pattern only: whenever an
infinity can generate a NaN, the only NaN used is the x86-64 default NaN fff8000000000000.
Which payload propagates when two *different* NaNs meet depends on compiler operand order,
not on the KiCad source, so such mixtures are not reference behaviour.
"""

import argparse
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

    names = [c["case"] for c in out]
    if len(names) != len(set(names)):
        raise ValueError("duplicate case id")
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
