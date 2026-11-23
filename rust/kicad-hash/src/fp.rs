// SPDX-License-Identifier: GPL-3.0-or-later
//! The oracle platform's NaN results for the scalar `double` expressions of the owned
//! `BEZIER<T>` bodies (`t2 = aT * aT`, `1.0 - aT`, `t0 / t1` and so on).
//!
//! These are the same rules as the private `fp` module of `kicad-math`, which the co-ported
//! `VECTOR2` operations use: one NaN operand is returned quieted; with two, the left operand
//! wins (a fixed choice; the compiled reference is not consistent there, see
//! `port/findings/kimath-header-hash-bezier-two-nan-payloads.md`); an invalid operation without
//! a NaN operand gives x86's negative indefinite NaN. Every non-NaN result is the plain IEEE
//! result.

const EXPONENT: u64 = 0x7ff0_0000_0000_0000;
const FRACTION: u64 = 0x000f_ffff_ffff_ffff;
const QUIET: u64 = 0x0008_0000_0000_0000;
const INDEFINITE: u64 = 0xfff8_0000_0000_0000;

fn is_nan(value: f64) -> bool {
    let bits = value.to_bits();
    bits & EXPONENT == EXPONENT && bits & FRACTION != 0
}

fn binary(first: f64, second: f64, result: f64) -> f64 {
    if is_nan(first) {
        f64::from_bits(first.to_bits() | QUIET)
    } else if is_nan(second) {
        f64::from_bits(second.to_bits() | QUIET)
    } else if is_nan(result) {
        f64::from_bits(INDEFINITE)
    } else {
        result
    }
}

pub(crate) fn sub(first: f64, second: f64) -> f64 {
    binary(first, second, first - second)
}

pub(crate) fn mul(first: f64, second: f64) -> f64 {
    binary(first, second, first * second)
}

pub(crate) fn div(first: f64, second: f64) -> f64 {
    binary(first, second, first / second)
}
