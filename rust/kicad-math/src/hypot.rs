// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2021-2024 Free Software Foundation, Inc.
// Derived from glibc 2.39 sysdeps/ieee754/dbl-64/e_hypot.c (non-FMA backend).
//! Shared finite-input reference hypot. This preserves the operation order of the
//! glibc 2.39 scalar backend. errno/FP exception flags are outside the portable API.
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
