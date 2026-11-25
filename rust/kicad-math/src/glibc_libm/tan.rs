// SPDX-License-Identifier: GPL-3.0-or-later
// IBM Accurate Mathematical Library, written by International Business Machines Corp.
// Copyright (C) 2001-2024 Free Software Foundation, Inc.
// Port of glibc 2.39 s_tan.c and utan.h, LGPL-2.1-or-later converted to
// GPL-3.0-or-later under LGPL-2.1 section 3.
use super::{eadd, quiet, tan_table::TAN, INVALID};

const D3: f64 = f64::from_bits(0x3fd5555555555555);
const D5: f64 = f64::from_bits(0x3fc11111111107c6);
const D7: f64 = f64::from_bits(0x3faba1ba1cdb8745);
const D9: f64 = f64::from_bits(0x3f9664ed49cfc666);
const D11: f64 = f64::from_bits(0x3f82385a3cf2e4ea);
const E0: f64 = f64::from_bits(0x3fd5555555554dbd);
const E1: f64 = f64::from_bits(0x3fc11112e0a6b45f);
const G2: f64 = f64::from_bits(0x3faf212d00000000);

// dla.h MUL12 uses a different summation from EMULV in the non-FMA variant.
fn mul12(x: f64, y: f64) -> (f64, f64) {
    let p = 134217729.0 * x;
    let hx = (x - p) + p;
    let tx = x - hx;
    let p = 134217729.0 * y;
    let hy = (y - p) + p;
    let ty = y - hy;
    let p = hx * hy;
    let q = hx * ty + tx * hy;
    let z = p + q;
    (z, ((p - z) + q) + tx * ty)
}

// DIV2(1, 0, b, db), preserving both roundings before the final c+dc.
fn reciprocal(b: f64, db: f64) -> f64 {
    let c = 1.0 / b;
    let (u, uu) = mul12(c, b);
    let cc = ((((1.0 - u) - uu) + 0.0) - c * db) / b;
    let z = c + cc;
    let zz = (c - z) + cc;
    z + zz
}

fn polynomial(a: f64, da: f64) -> f64 {
    let a2 = a * a;
    let mut t2 = D9 + a2 * D11;
    t2 = D7 + a2 * t2;
    t2 = D5 + a2 * t2;
    t2 = D3 + a2 * t2;
    da + a * a2 * t2
}

// A range-reduced absolute argument is at most pi/4. The source's truncating
// index -15.5 + 256*ya is 0..=185 for every table path (ya > G2).
fn table(ya: f64, yya: f64, sy: f64, odd: bool) -> f64 {
    let i = (-15.5 + 256.0 * ya) as usize;
    let c = &TAN[i];
    let z = (ya - c[0]) + yya;
    let z2 = z * z;
    let pz = z + z * z2 * (E0 + z2 * E1);
    let fi = c[1];
    let gi = c[2];
    if odd {
        let t2 = pz * (fi + gi) / (fi + pz);
        let y = gi - t2;
        -sy * y
    } else {
        let t2 = pz * (gi + fi) / (gi - pz);
        let y = fi + t2;
        sy * y
    }
}

pub(crate) fn tan(x: f64) -> f64 {
    if x.is_nan() {
        return quiet(x);
    }
    if x.is_infinite() {
        return INVALID;
    }
    let w = x.abs();
    if w <= f64::from_bits(0x3e4b096c00000000) {
        return x;
    }
    if w <= G2 {
        let x2 = x * x;
        let mut t2 = D9 + x2 * D11;
        t2 = D7 + x2 * t2;
        t2 = D5 + x2 * t2;
        t2 = D3 + x2 * t2;
        t2 *= x * x2;
        return x + t2;
    }
    if w <= f64::from_bits(0x3fe92f1a00000000) {
        // Here the source has no +yya operation (including signed zero).
        let i = (-15.5 + 256.0 * w) as usize;
        let c = &TAN[i];
        let z = w - c[0];
        let z2 = z * z;
        let s = if x < 0.0 { -1.0 } else { 1.0 };
        let pz = z + z * z2 * (E0 + z2 * E1);
        let fi = c[1];
        let gi = c[2];
        let t2 = pz * (gi + fi) / (gi - pz);
        let y = fi + t2;
        return s * y;
    }
    let (odd, a, da) = if w <= 100000000.0 {
        let t = x * f64::from_bits(0x3fe45f306dc9c883) + 6755399441055744.0;
        let xn = t - 6755399441055744.0;
        let t1 =
            (x - xn * f64::from_bits(0x3ff921fb58000000)) - xn * f64::from_bits(0xbe4dde973c000000);
        let odd = t.to_bits() & 1 != 0;
        if w <= 25.0 {
            let da = xn * f64::from_bits(0xbc8cb3b399d747f2);
            let a = t1 - da;
            (odd, a, (t1 - a) - da)
        } else {
            let da = xn * f64::from_bits(0xbc8cb3b398000000);
            let t = t1 - da;
            let da = (t1 - t) - da;
            let t1 = xn * f64::from_bits(0xbacd747f23e32ed7);
            let a = t - t1;
            let da = ((t - a) - t1) + da;
            let (a, da) = eadd(a, da);
            (odd, a, da)
        }
    } else {
        let (n, a, da) = crate::glibc_trig::branred(x);
        let (a, da) = eadd(a, da);
        (n & 1 != 0, a, da)
    };
    let (ya, yya, sy) = if a < 0.0 {
        (-a, -da, -1.0)
    } else {
        (a, da, 1.0)
    };
    if ya <= G2 {
        let t2 = polynomial(a, da);
        if odd {
            let (b, db) = eadd(a, t2);
            -reciprocal(b, db)
        } else {
            a + t2
        }
    } else {
        table(ya, yya, sy, odd)
    }
}
