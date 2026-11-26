// SPDX-License-Identifier: GPL-3.0-or-later

//! Public facade over the private `glibc_trig` and `glibc_libm` backends, for ports in other
//! crates whose pinned C++ calls `std::sin`, `std::cos`, `std::acos` or `std::atan2` on a
//! `double` (first user: the Clipper2 `ClipperOffset` port in `kicad-clipper2`).
//!
//! Value contract: each function returns exactly the bits of the pinned reference C library,
//! glibc 2.39's baseline SSE2 variant (`__sin_sse2`, `__cos_sse2`, `__ieee754_acos`,
//! `__ieee754_atan2`), natively and in WebAssembly, without the host libm. It assumes the
//! default round-to-nearest mode with gradual underflow, like the backends. errno, floating
//! point exception flags and other rounding modes are not modelled.
//!
//! Each function only forwards to the accepted private body: no degree conversion, argument
//! reordering or special-casing is added here, so signed zeros, infinities and NaN payloads
//! behave as the backends define them. `atan2` keeps the C argument order `(y, x)`.

/// `std::sin(double)`: glibc 2.39 `__sin_sse2`.
pub fn sin(x: f64) -> f64 {
    crate::glibc_trig::sin(x)
}

/// `std::cos(double)`: glibc 2.39 `__cos_sse2`.
pub fn cos(x: f64) -> f64 {
    crate::glibc_trig::cos(x)
}

/// `std::acos(double)`: glibc 2.39 `__ieee754_acos`.
pub fn acos(x: f64) -> f64 {
    crate::glibc_libm::acos(x)
}

/// `std::atan2(double y, double x)`: glibc 2.39 `__ieee754_atan2`, in the C argument order.
pub fn atan2(y: f64, x: f64) -> f64 {
    crate::glibc_libm::atan2(y, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The facade returns the private backends' bits unchanged, signed zeros and NaN
    /// payloads included.
    #[test]
    fn facade_forwards_exact_bits() {
        let samples = [
            0.0,
            -0.0,
            1e-300,
            0.5,
            -0.75,
            1.0,
            core::f64::consts::PI,
            -2.5e7,
            f64::INFINITY,
            f64::from_bits(0x7ff8_0000_0000_1234),
        ];
        for &x in &samples {
            assert_eq!(sin(x).to_bits(), crate::glibc_trig::sin(x).to_bits());
            assert_eq!(cos(x).to_bits(), crate::glibc_trig::cos(x).to_bits());
            assert_eq!(acos(x).to_bits(), crate::glibc_libm::acos(x).to_bits());
            for &y in &samples {
                assert_eq!(
                    atan2(y, x).to_bits(),
                    crate::glibc_libm::atan2(y, x).to_bits()
                );
            }
        }
    }
}
