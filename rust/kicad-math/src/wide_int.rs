// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! Port of `libs/kimath/include/math/wide_int.h` at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7: 128-bit integers for exact products of 64-bit
//! coordinate deltas.
//!
//! Coordinates span 2^32 units, so deltas need 33 bits and their cross and dot products need 66.
//! [`KiInt128`] holds those products exactly.

use crate::vector::Vector2L;

/// `KI_INT128`. The non-MSVC header aliases `__int128`; MSVC uses `std::_Signed128`. Both are
/// two's-complement 128-bit integers, which is exactly `i128`.
pub type KiInt128 = i128;

/// `KI_UINT128` (`unsigned __int128` / `std::_Unsigned128`).
pub type KiUint128 = u128;

/// A wide-integer result outside the C++ defined domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WideIntError {
    /// The exact result does not fit in `KI_INT128`; the C++ expression would be signed
    /// overflow (undefined behaviour). Only `DotWide` with all four components equal to
    /// `INT64_MIN` reaches this.
    Overflow {
        /// The C++ symbol whose arithmetic overflowed.
        upstream: &'static str,
    },
}

impl core::fmt::Display for WideIntError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overflow { upstream } => write!(formatter, "{upstream} overflows KI_INT128"),
        }
    }
}

/// Exact `2^aExponent` for `1 <= aExponent <= 64`, built from its IEEE-754 bit pattern.
///
/// `std::ldexp( x, e )` multiplies by `2^e` exactly unless the result overflows or underflows;
/// with `x < 2^64` and `e <= 64` neither can happen, so a binary64 multiply by the exact power
/// gives the same bits.
fn power_of_two(exponent: u32) -> f64 {
    f64::from_bits(u64::from(1023 + exponent) << 52)
}

/// `KIGEOM_WIDE::WordsToDouble`: nearest double to `hi * 2^64 + lo`, ties to even.
pub fn words_to_double(a_hi: i64, a_lo: u64) -> f64 {
    let negative = a_hi < 0;
    // static_cast<uint64_t>( aHi ): two's-complement reinterpretation.
    let mut hi = a_hi as u64;
    let mut lo = a_lo;

    if negative {
        lo = (!lo).wrapping_add(1);
        hi = (!hi).wrapping_add(if lo == 0 { 1 } else { 0 });
    }

    let magnitude = if hi == 0 {
        lo as f64
    } else {
        // Fold the discarded low bits into a sticky bit so the single uint64 to double rounding
        // matches rounding the full 128-bit value
        let shift = hi.leading_zeros(); // 0..=63 because hi != 0
        let mut top = if shift == 0 {
            hi
        } else {
            hi.wrapping_shl(shift) | lo.wrapping_shr(64 - shift)
        };

        if lo.wrapping_shl(shift) != 0 {
            top |= 1;
        }

        (top as f64) * power_of_two(64 - shift)
    };

    if negative {
        -magnitude
    } else {
        magnitude
    }
}

/// `ToDouble( KI_INT128 )`, non-MSVC branch: `static_cast<double>( __int128 )`, which rounds to
/// nearest, ties to even (libgcc `__floattidf`), as does Rust's `i128 as f64`.
pub fn to_double(a_value: KiInt128) -> f64 {
    a_value as f64
}

/// `ToDouble( const KI_INT128& )`, MSVC branch: `WordsToDouble( _Word[1], _Word[0] )`.
///
/// `std::_Signed128` stores the low word in `_Word[0]` and the high word in `_Word[1]`. The
/// result equals [`to_double`] for every value; both are kept because the header has both.
pub fn to_double_via_words(a_value: KiInt128) -> f64 {
    words_to_double((a_value >> 64) as i64, a_value as u64)
}

/// `CrossWide`: exact `aA.x * aB.y - aA.y * aB.x`. Never overflows for any int64 components;
/// the checked arithmetic cannot fail and exists only to keep the portable crate panic-free.
pub fn cross_wide(a_a: &Vector2L, a_b: &Vector2L) -> Result<KiInt128, WideIntError> {
    const OVERFLOW: WideIntError = WideIntError::Overflow {
        upstream: "CrossWide",
    };
    let left = KiInt128::from(a_a.x)
        .checked_mul(KiInt128::from(a_b.y))
        .ok_or(OVERFLOW)?;
    let right = KiInt128::from(a_a.y)
        .checked_mul(KiInt128::from(a_b.x))
        .ok_or(OVERFLOW)?;
    left.checked_sub(right).ok_or(OVERFLOW)
}

/// `DotWide`: exact `aA.x * aB.x + aA.y * aB.y`. Overflows only when all four components are
/// `INT64_MIN`; the C++ is then undefined, and this returns [`WideIntError::Overflow`].
pub fn dot_wide(a_a: &Vector2L, a_b: &Vector2L) -> Result<KiInt128, WideIntError> {
    const OVERFLOW: WideIntError = WideIntError::Overflow {
        upstream: "DotWide",
    };
    let left = KiInt128::from(a_a.x)
        .checked_mul(KiInt128::from(a_b.x))
        .ok_or(OVERFLOW)?;
    let right = KiInt128::from(a_a.y)
        .checked_mul(KiInt128::from(a_b.y))
        .ok_or(OVERFLOW)?;
    left.checked_add(right).ok_or(OVERFLOW)
}
