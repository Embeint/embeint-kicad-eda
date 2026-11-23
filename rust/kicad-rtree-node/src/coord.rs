// SPDX-License-Identifier: GPL-3.0-or-later
use core::fmt::Debug;

mod sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
}

/// A supported `ELEMTYPE`.
///
/// Only signed `int` and `int64_t` are ported.  For both, the reference's SSE2
/// `OverlapMask2D` path (signed 32-bit lane compares, taken when
/// `sizeof( ELEMTYPE ) == 4`) and its scalar path agree with plain signed
/// comparisons.  Other element types are deliberately not supported: `uint32_t`
/// and `float` would be compared as reinterpreted signed lanes on x86-64, and
/// narrower or floating types change the arithmetic.
pub trait Coord: sealed::Sealed + Copy + Ord + Debug + Default {
    /// `std::numeric_limits<ELEMTYPE>::max()`.
    const MAX: Self;
    /// `std::numeric_limits<ELEMTYPE>::lowest()`.
    const LOWEST: Self;
    /// `sizeof( ELEMTYPE )` on Linux x86-64.
    const REFERENCE_SIZE: u64;
    /// `alignof( ELEMTYPE )` on Linux x86-64.
    const REFERENCE_ALIGN: u64;

    /// `static_cast<int64_t>( value )`.
    fn widen(self) -> i64;
}

impl Coord for i32 {
    const MAX: Self = i32::MAX;
    const LOWEST: Self = i32::MIN;
    const REFERENCE_SIZE: u64 = 4;
    const REFERENCE_ALIGN: u64 = 4;

    fn widen(self) -> i64 {
        i64::from(self)
    }
}

impl Coord for i64 {
    const MAX: Self = i64::MAX;
    const LOWEST: Self = i64::MIN;
    const REFERENCE_SIZE: u64 = 8;
    const REFERENCE_ALIGN: u64 = 8;

    fn widen(self) -> i64 {
        self
    }
}
