// SPDX-License-Identifier: GPL-3.0-or-later
// IBM Accurate Mathematical Library, written by International Business Machines Corp.
// Copyright (C) 2001-2024 Free Software Foundation, Inc.
// Port of glibc 2.39 e_asin.c, uasncs.h and powtwo.tbl. LGPL-2.1-or-later
// converted to GPL-3.0-or-later under LGPL-2.1 section 3.
use super::{
    asin_table::{ASNCS, INROOT},
    quiet, INVALID,
};
const F1: f64 = f64::from_bits(0x3fc55555555554f9);
const F2: f64 = f64::from_bits(0x3fb333333336127d);
const F3: f64 = f64::from_bits(0x3fa6db6dae42c0e4);
const F4: f64 = f64::from_bits(0x3f9f1c7e04f4ad99);
const F5: f64 = f64::from_bits(0x3f96e442c822d419);
const F6: f64 = f64::from_bits(0x3f9292d80f453c72);
const RT0: f64 = f64::from_bits(0x3fefffffffecc1dd);
const RT1: f64 = f64::from_bits(0x3fdfffffff757304);
const RT2: f64 = f64::from_bits(0x3fd800496769c91a);
const RT3: f64 = f64::from_bits(0x3fd4006318d1dab9);
const HP0: f64 = f64::from_bits(0x3ff921fb54442d18);
const HP1: f64 = f64::from_bits(0x3c91a62633145c07);

fn series(z: f64) -> f64 {
    ((((F6 * z + F5) * z + F4) * z + F3) * z + F2) * z + F1
}

// e_asin.c's five table regions; n..n+14 stays within the original 2568
// coefficients. This is only called for 0.125 <= |x| < 0.96875. The nested
// polynomial is evaluated from its highest term, exactly as the C expression.
fn table(x: f64, k: u32) -> (f64, f64) {
    let (n, last) = if k < 0x3fe00000 {
        (
            if k < 0x3fd00000 {
                11 * ((k & 0xfffff) >> 15)
            } else {
                11 * ((k & 0xfffff) >> 14) + 352
            },
            6,
        )
    } else if k < 0x3fe80000 {
        (1056 + ((k & 0xfe000) >> 11) * 3, 7)
    } else if k < 0x3fed8000 {
        (992 + ((k & 0xfe000) >> 13) * 13, 8)
    } else if k < 0x3fee8000 {
        (884 + ((k & 0xfe000) >> 13) * 14, 9)
    } else {
        (768 + ((k & 0xfe000) >> 13) * 15, 10)
    };
    let n = n as usize;
    let xx = x.abs() - ASNCS[n];
    let mut t = ASNCS[n + 1] * xx;
    let mut p = ASNCS[n + last];
    for j in (2..last).rev() {
        p = ASNCS[n + j] + xx * p;
    }
    p = xx * xx * p + ASNCS[n + last + 1];
    t += p;
    (ASNCS[n + last + 2], t)
}

// The near-one branch uses the source's table-seeded inverse square root,
// not the host sqrt. For 0.96875 <= |x| < 1, z is in [2^-54, 2^-6].
// INROOT index <=127; powtwo's index 511-(k>>21) is in 3..=27. Constructing
// the exact power-of-two bits replaces that table without changing a rounding.
fn near_one(x: f64) -> (f64, f64, f64) {
    let z = 0.5 * if x > 0.0 { 1.0 - x } else { 1.0 + x };
    let k = (z.to_bits() >> 32) as u32;
    let pow = 511 - (k >> 21);
    let mut t =
        INROOT[((k & 0x1fffff) >> 14) as usize] * f64::from_bits(u64::from(1023 + pow) << 52);
    let r = 1.0 - t * t * z;
    t *= RT0 + r * (RT1 + r * (RT2 + r * RT3));
    let c = t * z;
    t = c * (1.5 - 0.5 * t * c);
    (z, c, t)
}

pub(crate) fn asin(x: f64) -> f64 {
    let k = ((x.to_bits() >> 32) as u32) & 0x7fffffff;
    if k < 0x3e500000 {
        return x;
    }
    if k < 0x3fc00000 {
        let x2 = x * x;
        let t = series(x2) * (x2 * x);
        return x + t;
    }
    if k < 0x3fef0000 {
        let (base, t) = table(x, k);
        let res = base + t;
        return if x > 0.0 { res } else { -res };
    }
    if k < 0x3ff00000 {
        let (z, c, t) = near_one(x);
        let y = (c + 16777216.0) - 16777216.0;
        let cc = (z - y * y) / (t + y);
        let p = series(z) * z;
        let cor = (HP1 - 2.0 * cc) - 2.0 * (y + cc) * p;
        let res1 = HP0 - 2.0 * y;
        let res = res1 + cor;
        return if x > 0.0 { res } else { -res };
    }
    if x.abs() == 1.0 {
        HP0.copysign(x)
    } else if x.is_nan() {
        quiet(x)
    } else {
        INVALID
    }
}

pub(crate) fn acos(x: f64) -> f64 {
    let k = ((x.to_bits() >> 32) as u32) & 0x7fffffff;
    if k < 0x3c880000 {
        return HP0;
    }
    if k < 0x3fc00000 {
        let x2 = x * x;
        let t = series(x2) * (x2 * x);
        let r = HP0 - x;
        let cor = (((HP0 - r) - x) + HP1) - t;
        return r + cor;
    }
    if k < 0x3fef0000 {
        let (base, t) = table(x, k);
        let y = if x > 0.0 { HP0 - base } else { HP0 + base };
        let t = if x > 0.0 { HP1 - t } else { HP1 + t };
        return y + t;
    }
    if k < 0x3ff00000 {
        let (z, c, t) = near_one(x);
        let y = (134217728.0 * c + c) - 134217728.0 * c;
        let cc = (z - y * y) / (t + y);
        let p = series(z) * z;
        if x < 0.0 {
            let cor = (HP1 - cc) - (y + cc) * p;
            let res1 = HP0 - y;
            let res = res1 + cor;
            res + res
        } else {
            let cor = cc + p * (y + cc);
            let res = y + cor;
            res + res
        }
    } else if x.abs() == 1.0 {
        if x > 0.0 {
            0.0
        } else {
            2.0 * HP0
        }
    } else if x.is_nan() {
        quiet(x)
    } else {
        INVALID
    }
}
