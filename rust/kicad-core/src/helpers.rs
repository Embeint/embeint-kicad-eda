// SPDX-License-Identifier: GPL-3.0-or-later
//! Ports of the small `libs/core/include/core` helpers: `arraydim.h` (`arrayDim`),
//! `ignore.h` (`ignore_unused`) and `type_helpers.h` (`to_underlying`; `always_false`
//! is a compile-time trait with no runtime body and needs no counterpart, since a Rust
//! `compile_error!` or an unsatisfied bound already rejects a branch).

/// `arrayDim( T const (&)[N] )`: the element count of an array, fixed at compile time.
pub const fn array_dim<T, const N: usize>(_array: &[T; N]) -> usize {
    N
}

/// `ignore_unused( const T& )`: accept a value and do nothing.
pub fn ignore_unused<T: ?Sized>(_value: &T) {}

/// An enumeration with a fixed underlying integer type (`std::underlying_type_t<E>`).
/// Ported enumerations implement it with their `#[repr]` type.
pub trait Underlying: Copy {
    /// The underlying integer type.
    type Repr: Copy;

    /// `static_cast<std::underlying_type_t<E>>( e )`.
    fn into_underlying(self) -> Self::Repr;
}

/// `to_underlying( E e )`: the enumerator's value in its underlying type.
pub fn to_underlying<E: Underlying>(e: E) -> E::Repr {
    e.into_underlying()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    #[repr(i8)]
    enum Small {
        Low = -128,
        High = 127,
    }

    impl Underlying for Small {
        type Repr = i8;
        fn into_underlying(self) -> i8 {
            self as i8
        }
    }

    #[test]
    fn helpers() {
        assert_eq!(array_dim(&[0u8; 7]), 7);
        assert_eq!(array_dim::<u64, 0>(&[]), 0);
        ignore_unused("anything");
        assert_eq!(to_underlying(Small::Low), -128);
        assert_eq!(to_underlying(Small::High), 127);
    }
}
