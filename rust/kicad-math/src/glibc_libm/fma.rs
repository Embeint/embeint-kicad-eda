// SPDX-License-Identifier: GPL-3.0-or-later
// Compute x * y + z as ternary operation.
// Copyright (C) 2010-2024 Free Software Foundation, Inc.
// Derived from glibc 2.39 sysdeps/ieee754/dbl-64/s_fma.c (LGPL-2.1-or-later), as built for
// x86_64 `__fma_sse2` by sysdeps/x86_64/fpu/multiarch/s_fma.c; distributed under
// GPL-3.0-or-later as permitted by LGPL-2.1 section 3. See port/glibc-libm/README.md.

//! `fma@@GLIBC_2.2.5`, an IFUNC that selects `__fma_sse2` (libm+0x32810) when neither FMA nor
//! FMA4 is usable, as under the reference tunables. That body is the generic dbl-64 s_fma.c
//! (`USE_FMA_BUILTIN` 0, x86 `TININESS_AFTER_ROUNDING` 1): Dekker's product, Knuth's sum and a
//! final round-to-odd addition done in round-toward-zero with the inexact flag.
//!
//! Rust cannot change the rounding mode or read the flags, so the two operations glibc runs
//! in `FE_TOWARDZERO` ([`add_toward_zero`]) compute the same binary64 result and inexact flag
//! exactly from the round-to-nearest sum and its TwoSum error. Every other operation is the
//! same round-to-nearest IEEE operation as the C. errno and exception flags are not modelled.
//!
//! The special-operand returns (`( z + x ) + y`, `x * y`, `x * y + z`) are the only ones that
//! can produce a NaN. Their NaN follows SSE2 in the compiled operand order: the destination
//! operand's NaN, quieted, then the source's, else the negative indefinite NaN. The locked
//! body computes `x * y + z` as `mulsd %xmm1,%xmm0; addsd %xmm2,%xmm0` (x, then the product),
//! `x * y` as `mulsd %xmm1,%xmm0`, and `( z + x ) + y` as `( x + z ) + y`.

use super::{quiet, INVALID};

const BIAS: i32 = 1023;
const MANT_DIG: i32 = 53;
const EXPONENT_MASK: u64 = 0x7ff << 52;

fn exponent(x: f64) -> i32 {
    ((x.to_bits() >> 52) & 0x7ff) as i32
}

/// `u.ieee.exponent = e`: the 11-bit field, other bits unchanged.
fn with_exponent(x: f64, e: i32) -> f64 {
    let field = (e as u64 & 0x7ff) << 52;
    f64::from_bits((x.to_bits() & !EXPONENT_MASK) | field)
}

fn negative(x: f64) -> bool {
    x.to_bits() >> 63 != 0
}

/// `u.ieee.mantissa1 |= bit` (the low 32 bits of the mantissa).
fn or_low(x: f64, bit: bool) -> f64 {
    f64::from_bits(x.to_bits() | u64::from(bit))
}

fn scale(e: i32) -> f64 {
    // 2^e for the normal exponents used here (-108..=108).
    f64::from_bits(((BIAS + e) as u64) << 52)
}

/// One SSE2 `addsd`/`mulsd` whose destination operand is `first`.
fn sse2(first: f64, second: f64, result: f64) -> f64 {
    if first.is_nan() {
        quiet(first)
    } else if second.is_nan() {
        quiet(second)
    } else if result.is_nan() {
        INVALID
    } else {
        result
    }
}

fn add(first: f64, second: f64) -> f64 {
    sse2(first, second, first + second)
}

fn mul(first: f64, second: f64) -> f64 {
    sse2(first, second, first * second)
}

/// `a + b` for finite `a`, `b` in `FE_TOWARDZERO`, with whether it raised `FE_INEXACT`.
///
/// The round-to-nearest sum `s` is either the exact sum or one of its two neighbours. Knuth's
/// TwoSum gives the exact error `exact - s` when `s` is finite. A zero error is exact; otherwise
/// the toward-zero result is `s` when the error has the sign of `s` (`|s| < |exact|`) and the
/// next double toward zero otherwise. A nonzero exact sum of binary64 values is at least the
/// smallest subnormal, so `s` is then nonzero. An overflowing sum rounds toward zero to the
/// largest finite value of its sign.
fn add_toward_zero(a: f64, b: f64) -> (f64, bool) {
    let s = a + b;
    if s.is_infinite() {
        let sign = s.to_bits() & (1 << 63);
        return (f64::from_bits(sign | f64::MAX.to_bits()), true);
    }
    let bb = s - a;
    let error = (a - (s - bb)) + (b - bb);
    if error == 0.0 {
        (s, false)
    } else if (error < 0.0) == (s < 0.0) {
        (s, true)
    } else {
        // |s| > |exact|: one step toward zero, which decrements the magnitude bits.
        (f64::from_bits(s.to_bits() - 1), true)
    }
}

/// `fma( x, y, z )`: glibc 2.39 `__fma_sse2`.
#[allow(clippy::float_cmp)]
pub(crate) fn fma(x: f64, y: f64, z: f64) -> f64 {
    let (mut x, mut y, mut z) = (x, y, z);
    let mut adjust = 0;
    let (ue, ve, we) = (exponent(x), exponent(y), exponent(z));

    if ue + ve >= 0x7ff + BIAS - MANT_DIG
        || ue >= 0x7ff - MANT_DIG
        || ve >= 0x7ff - MANT_DIG
        || we >= 0x7ff - MANT_DIG
        || ue + ve <= BIAS + MANT_DIG
    {
        // If z is Inf, but x and y are finite, the result should be z rather than NaN.
        if we == 0x7ff && ue != 0x7ff && ve != 0x7ff {
            return add(add(x, z), y);
        }
        // If z is zero and x are y are nonzero, compute the result as x * y to avoid the wrong
        // sign of a zero result if x * y underflows to 0.
        if z == 0.0 && x != 0.0 && y != 0.0 {
            return mul(x, y);
        }
        // If x or y or z is Inf/NaN, or if x * y is zero, compute as x * y + z.
        if ue == 0x7ff || ve == 0x7ff || we == 0x7ff || x == 0.0 || y == 0.0 {
            return add(mul(x, y), z);
        }
        // If fma will certainly overflow, compute as x * y.
        if ue + ve > 0x7ff + BIAS {
            return x * y;
        }
        // If x * y is less than 1/4 of DBL_TRUE_MIN, neither the result nor whether there is
        // underflow depends on its exact value, only on its sign.
        if ue + ve < BIAS - MANT_DIG - 2 {
            let neg = negative(x) != negative(y);
            let tiny = if neg {
                -f64::from_bits(1)
            } else {
                f64::from_bits(1)
            };
            if we >= 3 {
                return tiny + z;
            }
            // Scaling up, adding TINY and scaling down produces the correct result, because in
            // round-to-nearest mode adding TINY has no effect. (The forced x * y only raises
            // the underflow flag.)
            let v = z * scale(54) + tiny;
            return v * scale(-54);
        }
        if ue + ve >= 0x7ff + BIAS - MANT_DIG {
            // Compute 1p-53 times smaller result and multiply at the end.
            if ue > ve {
                x = with_exponent(x, ue - MANT_DIG);
            } else {
                y = with_exponent(y, ve - MANT_DIG);
            }
            // If x + y exponent is very large and z exponent is very small, it doesn't matter
            // if we don't adjust it.
            if we > MANT_DIG {
                z = with_exponent(z, we - MANT_DIG);
            }
            adjust = 1;
        } else if we >= 0x7ff - MANT_DIG {
            // Similarly. If z exponent is very large and x and y exponents are very small,
            // adjust them up to avoid spurious underflows, rather than down.
            if ue + ve <= BIAS + 2 * MANT_DIG {
                if ue > ve {
                    x = with_exponent(x, ue + 2 * MANT_DIG + 2);
                } else {
                    y = with_exponent(y, ve + 2 * MANT_DIG + 2);
                }
            } else if ue > ve {
                if ue > MANT_DIG {
                    x = with_exponent(x, ue - MANT_DIG);
                }
            } else if ve > MANT_DIG {
                y = with_exponent(y, ve - MANT_DIG);
            }
            z = with_exponent(z, we - MANT_DIG);
            adjust = 1;
        } else if ue >= 0x7ff - MANT_DIG {
            x = with_exponent(x, ue - MANT_DIG);
            if ve != 0 {
                y = with_exponent(y, ve + MANT_DIG);
            } else {
                y *= scale(53);
            }
        } else if ve >= 0x7ff - MANT_DIG {
            y = with_exponent(y, ve - MANT_DIG);
            if ue != 0 {
                x = with_exponent(x, ue + MANT_DIG);
            } else {
                x *= scale(53);
            }
        } else {
            // u.ieee.exponent + v.ieee.exponent <= IEEE754_DOUBLE_BIAS + DBL_MANT_DIG
            if ue > ve {
                x = with_exponent(x, ue + 2 * MANT_DIG + 2);
            } else {
                y = with_exponent(y, ve + 2 * MANT_DIG + 2);
            }
            if we <= 4 * MANT_DIG + 6 {
                if we != 0 {
                    z = with_exponent(z, we + 2 * MANT_DIG + 2);
                } else {
                    z *= scale(108);
                }
                adjust = -1;
            }
            // Otherwise x * y should just affect inexact and nothing else.
        }
    }

    // Ensure correct sign of exact 0 + 0.
    if (x == 0.0 || y == 0.0) && z == 0.0 {
        return x * y + z;
    }

    // Multiplication m1 + m2 = x * y using Dekker's algorithm.
    const C: f64 = ((1 << ((MANT_DIG + 1) / 2)) + 1) as f64;
    let x1 = x * C;
    let y1 = y * C;
    let m1 = x * y;
    let x1 = (x - x1) + x1;
    let y1 = (y - y1) + y1;
    let x2 = x - x1;
    let y2 = y - y1;
    let m2 = (((x1 * y1 - m1) + x1 * y2) + x2 * y1) + x2 * y2;

    // Addition a1 + a2 = z + m1 using Knuth's algorithm.
    let a1 = z + m1;
    let t1 = a1 - z;
    let t2 = a1 - t1;
    let t1 = m1 - t1;
    let t2 = z - t2;
    let a2 = t1 + t2;

    // If the result is an exact zero, ensure it has the correct sign.
    if a1 == 0.0 && m2 == 0.0 {
        return z + m1;
    }

    // FE_TOWARDZERO from here until the environment is restored: perform m2 + a2 addition
    // with round to odd. FE_INEXACT was cleared just before.
    let (mut u, mut inexact) = add_toward_zero(a2, m2);
    let mut v = 0.0;

    if adjust < 0 {
        if u.to_bits() & 1 == 0 {
            u = or_low(u, inexact);
        }
        let (sum, sum_inexact) = add_toward_zero(a1, u);
        v = sum;
        inexact |= sum_inexact;
    }

    // Reset rounding mode and test for inexact simultaneously.
    let j = inexact;

    if adjust == 0 {
        if u.to_bits() & 1 == 0 && exponent(u) != 0x7ff {
            u = or_low(u, j);
        }
        // Result is a1 + u.d.
        a1 + u
    } else if adjust > 0 {
        if u.to_bits() & 1 == 0 && exponent(u) != 0x7ff {
            u = or_low(u, j);
        }
        // Result is a1 + u.d, scaled up.
        (a1 + u) * scale(53)
    } else {
        // If a1 + u.d is exact, the only rounding happens during scaling down.
        if !j {
            return v * scale(-108);
        }
        // If result rounded to zero is not subnormal, no double rounding will occur.
        if exponent(v) > 108 {
            return (a1 + u) * scale(-108);
        }
        // If v.d * 0x1p-108 with round to zero is a subnormal above or equal to DBL_MIN / 2,
        // then v.d * 0x1p-108 shifts mantissa down just by 1 bit, which means
        // v.ieee.mantissa1 |= j would change the round bit, not sticky or guard bit.
        // v.d * 0x1p-108 never normalizes by shifting up, so round bit plus sticky bit should be
        // already enough for proper rounding.
        if exponent(v) == 108 {
            // If the exponent would be in the normal range when rounding to normal precision
            // with unbounded exponent range, the exact result is known and spurious underflows
            // must be avoided on systems detecting tininess after rounding.
            let w = a1 + u;
            if exponent(w) == 109 {
                return w * scale(-108);
            }
            // v.ieee.mantissa1 & 2 is LSB bit of the result before rounding, v.ieee.mantissa1 & 1
            // is the round bit and j is our sticky bit.
            let vbits = v.to_bits();
            let low = ((vbits & 3) << 1) | u64::from(j);
            let w = f64::from_bits((vbits & (1 << 63)) | low);
            let v = f64::from_bits(vbits & !3) * scale(-108);
            let w = w * scale(-2);
            return v + w;
        }
        or_low(v, j) * scale(-108)
    }
}
