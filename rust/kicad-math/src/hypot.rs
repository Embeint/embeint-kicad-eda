// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2021-2024 Free Software Foundation, Inc.
// Derived from glibc 2.39 sysdeps/ieee754/dbl-64/e_hypot.c (non-FMA backend).
// The original is LGPL-2.1-or-later. As section 3 of that licence permits, this
// derived file is distributed under GPL-3.0-or-later, like the rest of this crate.
//! Shared reference hypot. This preserves the operation order of the glibc 2.39 scalar
//! backend. errno/FP exception flags are outside the portable API.
//!
//! `hypot` is the complete public `hypot@@GLIBC_2.35` (`__hypot`; x86_64 has no multiarch
//! variant, so there is no IFUNC). `finite` is its body after the non-finite branch, shared
//! with the callers that only pass finite operands. See port/glibc-libm/README.md.

/// `__hypot (x, y)` for any operands. Infinite and NaN operands take e_hypot.c's first
/// branch; every finite pair continues into `finite` unchanged.
pub(crate) fn hypot(x: f64, y: f64) -> f64 {
    if !x.is_finite() || !y.is_finite() {
        if (x.is_infinite() || y.is_infinite()) && !is_signaling(x) && !is_signaling(y) {
            return f64::INFINITY;
        }
        return nan_add(x, y);
    }
    finite(x, y)
}

/// math_config.h `issignaling_inline` (HIGH_ORDER_BIT_IS_SET_FOR_SNAN is 0 on x86_64).
/// The C `2 * ...` wraps in uint64_t; the shift discards the sign bit the same way.
fn is_signaling(x: f64) -> bool {
    ((x.to_bits() ^ QUIET) << 1) > (0x7ff8_0000_0000_0000_u64 << 1)
}

/// The `return x + y` of the non-finite branch, reached only with a NaN operand. The locked
/// libm computes it as `addsd %xmm1,%xmm0` (x is the first operand): a NaN x is returned
/// quieted, otherwise the quieted NaN y. Written out so that wasm, whose NaN propagation is
/// not specified, returns the same bits.
fn nan_add(x: f64, y: f64) -> f64 {
    if x.is_nan() {
        f64::from_bits(x.to_bits() | QUIET)
    } else if y.is_nan() {
        f64::from_bits(y.to_bits() | QUIET)
    } else {
        x + y
    }
}

const QUIET: u64 = 0x0008_0000_0000_0000;

pub(crate) fn finite(x: f64, y: f64) -> f64 {
    let x = x.abs();
    let y = y.abs();
    let (ax, ay) = if x < y { (y, x) } else { (x, y) };
    let scale = f64::from_bits((1023 - 600) << 52);
    let large = f64::from_bits((1023 + 511) << 52);
    let tiny = f64::from_bits((1023 - 459) << 52);
    let eps = f64::from_bits((1023 - 54) << 52);
    if ax > large {
        if ay <= ax * eps {
            return ax + ay;
        }
        return kernel(ax * scale, ay * scale) / scale;
    }
    if ay < tiny {
        if ax >= ay / eps {
            return ax + ay;
        }
        return kernel(ax / scale, ay / scale) * scale;
    }
    if ay <= ax * eps {
        return ax + ay;
    }
    kernel(ax, ay)
}
fn kernel(ax: f64, ay: f64) -> f64 {
    let h = (ax * ax + ay * ay).sqrt();
    let (t1, t2) = if h <= 2.0 * ay {
        let delta = h - ay;
        (ax * (2.0 * delta - ax), (delta - 2.0 * (ax - ay)) * delta)
    } else {
        let delta = h - ax;
        (
            2.0 * delta * (ax - 2.0 * ay),
            (4.0 * delta - ay) * ay + delta * delta,
        )
    };
    h - (t1 + t2) / (2.0 * h)
}
