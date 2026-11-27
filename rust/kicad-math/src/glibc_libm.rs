// SPDX-License-Identifier: GPL-3.0-or-later
// IBM Accurate Mathematical Library, written by International Business Machines Corp.
// Copyright (C) 2001-2024 Free Software Foundation, Inc.
// Derived from glibc 2.39 (LGPL-2.1-or-later); distributed under GPL-3.0-or-later
// as permitted by LGPL-2.1 section 3. See port/glibc-libm/README.md.

//! The pinned glibc 2.39 baseline SSE2 angle backends, `pow` and `fma` (`__fma_sse2`). IEEE
//! binary64 operations stay in source order without contraction. No host libm, hardware FMA,
//! errno or floating exception flags. Requires round-to-nearest and gradual underflow, like
//! glibc_trig; `fma`'s internal round-toward-zero step is computed exactly (see its module).

#[path = "glibc_libm/asin.rs"]
mod asin;
#[path = "glibc_libm/asin_table.rs"]
mod asin_table;
#[path = "glibc_libm/atan.rs"]
mod atan;
#[path = "glibc_libm/atan_table.rs"]
mod atan_table;
#[path = "glibc_libm/fma.rs"]
mod fma;
#[path = "glibc_libm/pow.rs"]
mod pow;
#[path = "glibc_libm/pow_table.rs"]
mod pow_table;
#[path = "glibc_libm/tan.rs"]
mod tan;
#[path = "glibc_libm/tan_table.rs"]
mod tan_table;

pub(crate) use asin::{acos, asin};
pub(crate) use atan::{atan, atan2};
pub(crate) use fma::fma;
pub(crate) use pow::pow;
pub(crate) use tan::tan;

const QUIET: u64 = 0x0008_0000_0000_0000;
const INVALID: f64 = f64::from_bits(0xfff8_0000_0000_0000);

fn quiet(x: f64) -> f64 {
    f64::from_bits(x.to_bits() | QUIET)
}

// dla.h: EADD, ESUB, EMULV, non-FMA branch. Inputs are finite and bounded by
// the callers' range reductions, so the splitting products cannot overflow.
fn eadd(x: f64, y: f64) -> (f64, f64) {
    let z = x + y;
    let zz = if x.abs() > y.abs() {
        (x - z) + y
    } else {
        (y - z) + x
    };
    (z, zz)
}
fn esub(x: f64, y: f64) -> (f64, f64) {
    let z = x - y;
    let zz = if x.abs() > y.abs() {
        (x - z) - y
    } else {
        x - (y + z)
    };
    (z, zz)
}
fn emulv(x: f64, y: f64) -> (f64, f64) {
    let p = 134217729.0 * x;
    let hx = (x - p) + p;
    let tx = x - hx;
    let p = 134217729.0 * y;
    let hy = (y - p) + p;
    let ty = y - hy;
    let z = x * y;
    let zz = (((hx * hy - z) + hx * ty) + tx * hy) + tx * ty;
    (z, zz)
}

#[cfg(test)]
#[path = "glibc_libm/tests.rs"]
mod tests;
