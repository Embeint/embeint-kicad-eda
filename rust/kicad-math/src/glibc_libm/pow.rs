// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2018-2024 Free Software Foundation, Inc.
// Port of glibc 2.39 sysdeps/ieee754/dbl-64/e_pow.c (the x86_64 multiarch
// __ieee754_pow_sse2 build: no __FP_FAST_FMA, no TOINT_INTRINSICS,
// FLT_EVAL_METHOD 0), its math_err.c special-value helpers and the public
// math/w_pow_template.c wrapper. LGPL-2.1-or-later, distributed under
// GPL-3.0-or-later under LGPL-2.1 section 3. See port/glibc-libm/README.md.
use super::pow_table::{
    A, C, EXP_TAB, INV_LN2_N, LN2HI, LN2LO, NEG_LN2_HI_N, NEG_LN2_LO_N, POW_LOG, SHIFT,
};
use super::{quiet, INVALID};

const POW_LOG_TABLE_BITS: u32 = 7;
const EXP_TABLE_BITS: u32 = 7;
const OFF: u64 = 0x3fe6_9555_0000_0000;
const SIGN_BIAS: u32 = 0x800 << EXP_TABLE_BITS;
const ONE_BITS: u64 = 0x3ff0_0000_0000_0000;
const INF_BITS: u64 = 0x7ff0_0000_0000_0000;
// top12 of 0x1p-54, 512.0 and 1024.0.
const TOP_TINY: u32 = 0x3c9;
const TOP_512: u32 = 0x408;
const TOP_1024: u32 = 0x409;

fn top12(x: f64) -> u32 {
    (x.to_bits() >> 52) as u32
}

/// The SSE2 `addsd` the kernel uses for every `x + y` NaN return: the first
/// operand is x (disassembly `movapd %xmm0,%xmm2; addsd %xmm1,%xmm2`), so a NaN
/// x is returned quieted, otherwise a NaN y. WebAssembly does not specify NaN
/// propagation, so the selection is explicit.
fn add_nan(x: f64, y: f64) -> f64 {
    if x.is_nan() {
        quiet(x)
    } else if y.is_nan() {
        quiet(y)
    } else {
        x + y
    }
}

// math_err.c: xflow( sign, y ) = ( sign ? -y : y ) * y, without errno.
fn xflow(sign: u32, y: f64) -> f64 {
    (if sign != 0 { -y } else { y }) * y
}
fn math_uflow(sign: u32) -> f64 {
    xflow(sign, f64::from_bits(0x1000_0000_0000_0000)) // 0x1p-767
}
fn math_oflow(sign: u32) -> f64 {
    xflow(sign, f64::from_bits(0x7000_0000_0000_0000)) // 0x1p769
}
fn math_divzero(sign: u32) -> f64 {
    (if sign != 0 { -1.0 } else { 1.0 }) / 0.0
}

/// Returns `(y, tail)` with y + tail = log(x); `ix` is normalized as in the source.
fn log_inline(ix: u64) -> (f64, f64) {
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> (52 - POW_LOG_TABLE_BITS)) & ((1 << POW_LOG_TABLE_BITS) - 1)) as usize;
    let k = ((tmp as i64) >> 52) as i32;
    let iz = ix.wrapping_sub(tmp & (0xfff << 52));
    let z = f64::from_bits(iz);
    let kd = f64::from(k);

    let [invc, logc, logctail] = POW_LOG[i];

    // Non-FMA split: rhi, rlo and rhi*rhi are exact and |rlo| <= |r|.
    let zhi = f64::from_bits(iz.wrapping_add(1 << 31) & (u64::MAX << 32));
    let zlo = z - zhi;
    let rhi = zhi * invc - 1.0;
    let rlo = zlo * invc;
    let r = rhi + rlo;

    let t1 = kd * LN2HI + logc;
    let t2 = t1 + r;
    let lo1 = kd * LN2LO + logctail;
    let lo2 = t1 - t2 + r;

    let ar = A[0] * r;
    let ar2 = r * ar;
    let ar3 = r * ar2;
    let arhi = A[0] * rhi;
    let arhi2 = rhi * arhi;
    let hi = t2 + arhi2;
    let lo3 = rlo * (ar + arhi);
    let lo4 = t2 - hi + arhi2;
    let p = ar3 * (A[1] + r * A[2] + ar2 * (A[3] + r * A[4] + ar2 * (A[5] + r * A[6])));
    let lo = lo1 + lo2 + lo3 + lo4 + p;
    let y = hi + lo;
    (y, hi - y + lo)
}

/// scale*(1+tmp) where the exponent of scale may have left the normal range.
fn specialcase(tmp: f64, sbits: u64, ki: u64) -> f64 {
    if ki & 0x8000_0000 == 0 {
        // k > 0: the exponent of scale might have overflowed by <= 460.
        let scale = f64::from_bits(sbits.wrapping_sub(1009 << 52));
        let y = f64::from_bits(0x7f00_0000_0000_0000) * (scale + scale * tmp); // 0x1p1009
        return y;
    }
    // k < 0: sbits is the signed scale, rounded once before the subnormal scaling.
    let sbits = sbits.wrapping_add(1022 << 52);
    let scale = f64::from_bits(sbits);
    let mut y = scale + scale * tmp;
    if y.abs() < 1.0 {
        let one = if y < 0.0 { -1.0 } else { 1.0 };
        let mut lo = scale - y + scale * tmp;
        let hi = one + y;
        // Keep the source's left-to-right sum rather than clippy's `lo += ...`.
        #[allow(clippy::assign_op_pattern)]
        {
            lo = one - hi + y + lo;
        }
        y = (hi + lo) - one;
        // Fix the sign of 0.
        if y == 0.0 {
            y = f64::from_bits(sbits & 0x8000_0000_0000_0000);
        }
        // The source's forced 0x1p-1022 * 0x1p-1022 only raises underflow.
    }
    f64::from_bits(0x0010_0000_0000_0000) * y // 0x1p-1022
}

/// sign * exp(x + xtail), sign from `sign_bias`.
fn exp_inline(x: f64, xtail: f64, sign_bias: u32) -> f64 {
    let mut abstop = top12(x) & 0x7ff;
    if abstop.wrapping_sub(TOP_TINY) >= TOP_512 - TOP_TINY {
        if abstop.wrapping_sub(TOP_TINY) >= 0x8000_0000 {
            // Tiny x (WANT_ROUNDING): 1 + x, avoiding spurious underflow.
            let one = 1.0 + x;
            return if sign_bias != 0 { -one } else { one };
        }
        if abstop >= TOP_1024 {
            // inf and nan are already handled by the caller.
            return if x.to_bits() >> 63 != 0 {
                math_uflow(sign_bias)
            } else {
                math_oflow(sign_bias)
            };
        }
        // Large x is special cased below.
        abstop = 0;
    }

    let z = INV_LN2_N * x;
    let kd = z + SHIFT;
    let ki = kd.to_bits();
    let kd = kd - SHIFT;
    let mut r = x + kd * NEG_LN2_HI_N + kd * NEG_LN2_LO_N;
    r += xtail;
    let idx = (2 * (ki & ((1 << EXP_TABLE_BITS) - 1))) as usize;
    let top = ki.wrapping_add(u64::from(sign_bias)) << (52 - EXP_TABLE_BITS);
    let tail = f64::from_bits(EXP_TAB[idx]);
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    let tmp = tail + r + r2 * (C[0] + r * C[1]) + r2 * r2 * (C[2] + r * C[3]);
    if abstop == 0 {
        return specialcase(tmp, sbits, ki);
    }
    let scale = f64::from_bits(sbits);
    scale + scale * tmp
}

/// 0 if not an integer, 1 if odd, 2 if even, for a non-zero finite value.
fn checkint(iy: u64) -> i32 {
    let e = (iy >> 52 & 0x7ff) as i32;
    if e < 0x3ff {
        return 0;
    }
    if e > 0x3ff + 52 {
        return 2;
    }
    let shift = (0x3ff + 52 - e) as u32;
    if iy & ((1 << shift) - 1) != 0 {
        return 0;
    }
    if iy & (1 << shift) != 0 {
        return 1;
    }
    2
}

/// True for the bit pattern of 0, infinity or NaN.
fn zeroinfnan(i: u64) -> bool {
    (i << 1).wrapping_sub(1) >= 2 * INF_BITS - 1
}

// HIGH_ORDER_BIT_IS_SET_FOR_SNAN is 0 on x86.
fn issignaling(x: f64) -> bool {
    (x.to_bits() ^ 0x0008_0000_0000_0000) << 1 > 2 * 0x7ff8_0000_0000_0000
}

/// `__ieee754_pow_sse2`.
fn ieee754_pow(x: f64, y: f64) -> f64 {
    let mut sign_bias = 0;
    let mut ix = x.to_bits();
    let iy = y.to_bits();
    let mut topx = top12(x);
    let topy = top12(y);
    if topx.wrapping_sub(0x001) >= 0x7ff - 0x001
        || (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be
    {
        // Special cases: (x < 0x1p-126 or inf or nan) or
        // (|y| < 0x1p-65 or |y| >= 0x1p63 or nan).
        if zeroinfnan(iy) {
            if (iy << 1) == 0 {
                return if issignaling(x) { add_nan(x, y) } else { 1.0 };
            }
            if ix == ONE_BITS {
                return if issignaling(y) { add_nan(x, y) } else { 1.0 };
            }
            if (ix << 1) > 2 * INF_BITS || (iy << 1) > 2 * INF_BITS {
                return add_nan(x, y);
            }
            if (ix << 1) == 2 * ONE_BITS {
                return 1.0;
            }
            if ((ix << 1) < 2 * ONE_BITS) == (iy >> 63 == 0) {
                return 0.0; // |x|<1 && y==inf or |x|>1 && y==-inf.
            }
            return y * y;
        }
        if zeroinfnan(ix) {
            // x is 0, inf or NaN. x * x of a NaN is the quieted operand.
            let mut x2 = if x.is_nan() { quiet(x) } else { x * x };
            if ix >> 63 != 0 && checkint(iy) == 1 {
                x2 = -x2;
                sign_bias = 1;
            }
            if (ix << 1) == 0 && iy >> 63 != 0 {
                return math_divzero(sign_bias);
            }
            return if iy >> 63 != 0 {
                if x2.is_nan() {
                    x2 // 1 / NaN returns the (already quiet) divisor.
                } else {
                    1.0 / x2
                }
            } else {
                x2
            };
        }
        // Here x and y are non-zero finite.
        if ix >> 63 != 0 {
            // Finite x < 0.
            let yint = checkint(iy);
            if yint == 0 {
                // __math_invalid: (x - x) / (x - x) of a finite x is SSE2's
                // default negative quiet NaN.
                return INVALID;
            }
            if yint == 1 {
                sign_bias = SIGN_BIAS;
            }
            ix &= 0x7fff_ffff_ffff_ffff;
            topx &= 0x7ff;
        }
        if (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be {
            // sign_bias == 0 here because y is not odd.
            if ix == ONE_BITS {
                return 1.0;
            }
            if (topy & 0x7ff) < 0x3be {
                // |y| < 2^-65, x^y ~= 1 + y*log(x) (WANT_ROUNDING).
                return if ix > ONE_BITS { 1.0 + y } else { 1.0 - y };
            }
            return if (ix > ONE_BITS) == (topy < 0x800) {
                math_oflow(0)
            } else {
                math_uflow(0)
            };
        }
        if topx == 0 {
            // Normalize subnormal x so the exponent becomes negative.
            ix = (x * f64::from_bits(0x4330_0000_0000_0000)).to_bits(); // 0x1p52
            ix &= 0x7fff_ffff_ffff_ffff;
            ix = ix.wrapping_sub(52 << 52);
        }
    }

    let (hi, lo) = log_inline(ix);
    // Non-FMA product y * (hi + lo).
    let yhi = f64::from_bits(iy & (u64::MAX << 27));
    let ylo = y - yhi;
    let lhi = f64::from_bits(hi.to_bits() & (u64::MAX << 27));
    let llo = hi - lhi + lo;
    let ehi = yhi * lhi;
    let elo = ylo * lhi + y * llo; // |elo| < |ehi| * 2^-25.
    exp_inline(ehi, elo, sign_bias)
}

/// The public `pow@@GLIBC_2.29` (w_pow_template.c). The wrapper only sets errno
/// and returns the kernel value unchanged; errno is outside this value contract.
pub(crate) fn pow(x: f64, y: f64) -> f64 {
    ieee754_pow(x, y)
}
