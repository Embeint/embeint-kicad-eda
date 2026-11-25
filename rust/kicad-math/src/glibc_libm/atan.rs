// SPDX-License-Identifier: GPL-3.0-or-later
// IBM Accurate Mathematical Library, written by International Business Machines Corp.
// Copyright (C) 2001-2024 Free Software Foundation, Inc.
// Port of glibc 2.39 s_atan.c, e_atan2.c, atnat.h and atnat2.h.
// LGPL-2.1-or-later converted to GPL-3.0-or-later under LGPL-2.1 section 3.
use super::{atan_table::ATAN, eadd, emulv, esub, quiet};

const D3: f64 = f64::from_bits(0xbfd5555555555555);
const D5: f64 = f64::from_bits(0x3fc99999999997fd);
const D7: f64 = f64::from_bits(0xbfc24924923f7603);
const D9: f64 = f64::from_bits(0x3fbc71c6e5129a3b);
const D11: f64 = f64::from_bits(0xbfb7458022b13c25);
const D13: f64 = f64::from_bits(0x3fb375f08b31cbce);
const HPI: f64 = f64::from_bits(0x3ff921fb54442d18);
const HPI1: f64 = f64::from_bits(0x3c91a62633145c07);
const OPI: f64 = f64::from_bits(0x400921fb54442d18);
const OPI1: f64 = f64::from_bits(0x3ca1a62633145c07);
const TWO52: f64 = 4503599627370496.0;

// All callers establish 1/16 <= u <= 1. The rounded index is in 16..=256,
// hence the subtraction and every constant column access are bounded.
fn row(u: f64) -> &'static [f64; 7] {
    let i = ((TWO52 + 256.0 * u) - TWO52) as usize - 16;
    &ATAN[i]
}

pub(crate) fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return quiet(x);
    }
    let u = x.abs();
    if u < 1.0 {
        if u < 0.0625 {
            if u < f64::from_bits(0x3e4bb67a00000000) {
                return x;
            }
            let v = x * x;
            let mut yy = D11 + v * D13;
            yy = D9 + v * yy;
            yy = D7 + v * yy;
            yy = D5 + v * yy;
            yy = D3 + v * yy;
            yy *= x * v;
            x + yy
        } else {
            let c = row(u);
            let z = u - c[0];
            let mut yy = c[5] + z * c[6];
            yy = c[4] + z * yy;
            yy = c[3] + z * yy;
            yy = c[2] + z * yy;
            yy *= z;
            (c[1] + yy).copysign(x)
        }
    } else if u < 16.0 {
        let w = 1.0 / u;
        let (t1, t2) = emulv(w, u);
        let ww = w * ((1.0 - t1) - t2);
        let c = row(w);
        let z = (w - c[0]) + ww;
        let mut yy = c[5] + z * c[6];
        yy = c[4] + z * yy;
        yy = c[3] + z * yy;
        yy = c[2] + z * yy;
        yy = HPI1 - z * yy;
        let t1 = HPI - c[1];
        (t1 + yy).copysign(x)
    } else if u < f64::from_bits(0x43349ff200000000) {
        let w = 1.0 / u;
        let v = w * w;
        let (t1, t2) = emulv(w, u);
        let mut yy = D11 + v * D13;
        yy = D9 + v * yy;
        yy = D7 + v * yy;
        yy = D5 + v * yy;
        yy = D3 + v * yy;
        yy *= w * v;
        let ww = w * ((1.0 - t1) - t2);
        let (t3, cor) = esub(HPI, w);
        yy = ((HPI1 + cor) - ww) - yy;
        (t3 + yy).copysign(x)
    } else {
        HPI.copysign(x)
    }
}

fn polynomial(u: f64) -> f64 {
    let v = u * u;
    u * v * (D3 + v * (D5 + v * (D7 + v * (D9 + v * (D11 + v * D13)))))
}

/// e_atan2.c has four separate quadrant continuations. In particular, it does
/// not compute atan(y/x) and subtract from pi; the low parts and EADD/ESUB matter.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    // Packaged SSE2 x+y selects x's payload when both operands are NaNs.
    // The dedicated probe checks all ordered quiet/signalling payload pairs.
    if x.is_nan() {
        return quiet(x);
    }
    if y.is_nan() {
        return quiet(y);
    }
    if y == 0.0 {
        return if x.is_sign_negative() {
            OPI.copysign(y)
        } else {
            y
        };
    }
    if x == 0.0 {
        return HPI.copysign(y);
    }
    if x.is_infinite() {
        if y.is_infinite() {
            let a = if x > 0.0 {
                f64::from_bits(0x3fe921fb54442d18)
            } else {
                f64::from_bits(0x4002d97c7f3321d2)
            };
            return a.copysign(y);
        }
        return if x > 0.0 {
            0.0f64.copysign(y)
        } else {
            OPI.copysign(y)
        };
    }
    if y.is_infinite() {
        return HPI.copysign(y);
    }
    let de = ((y.to_bits() >> 32) & 0x7ff00000) as i32 - ((x.to_bits() >> 32) & 0x7ff00000) as i32;
    let mut ax = x.abs();
    let mut ay = y.abs();
    if de >= 59768832 {
        return HPI.copysign(y);
    }
    if de <= -59768832 {
        return if x > 0.0 {
            (ay / ax).copysign(y)
        } else {
            OPI.copysign(y)
        };
    }
    let two500 = f64::from_bits(0x5f30000000000000);
    let twom500 = f64::from_bits(0x20b0000000000000);
    if ax < twom500 || ay < twom500 {
        ax *= two500;
        ay *= two500;
    }
    if ax > two500 || ay > two500 {
        ax *= twom500;
        ay *= twom500;
    }
    let (u, du) = if ay < ax {
        let u = ay / ax;
        let (v, vv) = emulv(ax, u);
        (u, ((ay - v) - vv) / ax)
    } else {
        let u = ax / ay;
        let (v, vv) = emulv(ay, u);
        (u, ((ax - v) - vv) / ay)
    };
    let z = if x > 0.0 {
        if ay < ax {
            if u < 0.0625 {
                let zz = du + polynomial(u);
                u + zz
            } else {
                let c = row(u);
                let t3 = u - c[0];
                let (v, dv) = eadd(t3, du);
                let t1 = c[1];
                let t2 = c[2];
                let zz = v * t2 + (dv * t2 + v * v * (c[3] + v * (c[4] + v * (c[5] + v * c[6]))));
                t1 + zz
            }
        } else if u < 0.0625 {
            let zz = polynomial(u);
            let (t2, cor) = esub(HPI, u);
            let t3 = ((HPI1 + cor) - du) - zz;
            t2 + t3
        } else {
            let c = row(u);
            let v = (u - c[0]) + du;
            let zz = HPI1 - v * (c[2] + v * (c[3] + v * (c[4] + v * (c[5] + v * c[6]))));
            let t1 = HPI - c[1];
            t1 + zz
        }
    } else if ax < ay {
        if u < 0.0625 {
            let zz = polynomial(u);
            let (t2, cor) = eadd(HPI, u);
            let t3 = ((HPI1 + cor) + du) + zz;
            t2 + t3
        } else {
            let c = row(u);
            let v = (u - c[0]) + du;
            let zz = HPI1 + v * (c[2] + v * (c[3] + v * (c[4] + v * (c[5] + v * c[6]))));
            let t1 = HPI + c[1];
            t1 + zz
        }
    } else if u < 0.0625 {
        let zz = polynomial(u);
        let (t2, cor) = esub(OPI, u);
        let t3 = ((OPI1 + cor) - du) - zz;
        t2 + t3
    } else {
        let c = row(u);
        let v = (u - c[0]) + du;
        let zz = OPI1 - v * (c[2] + v * (c[3] + v * (c[4] + v * (c[5] + v * c[6]))));
        let t1 = OPI - c[1];
        t1 + zz
    };
    z.copysign(y)
}
