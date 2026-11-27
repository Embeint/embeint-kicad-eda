// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/mirror.h` (unit `include.core.application.pure.p001`): `FLIP_DIRECTION`,
//! `MIRRORVAL`, both `MIRROR` overloads and `MIRROR_AXIS`.
//!
//! `MIRRORVAL` evaluates `-( aPoint - aMirrorRef ) + aMirrorRef` in the instantiation's
//! own arithmetic: for `int` and `long long` every signed overflow of that expression is
//! undefined and refused with [`MirrorError::IntegerOverflow`]. For `double`, GCC compiles
//! the expression as `aMirrorRef - ( aPoint - aMirrorRef )`: adding a negation is exactly
//! subtracting for every non-NaN value (signed zeros included), and the compiled form keeps
//! the first NaN operand of each subtraction, quieted, without flipping its sign. The port
//! evaluates that form with the reference platform's NaN rules ([`crate::fp`]); see
//! `port/findings/core-app-include-mirrorval-compiled-subtraction.md`.

use crate::eda_angle::{EdaAngle, ANGLE_0, ANGLE_90};
use crate::fp;
use crate::vector::Vector2;

/// `FLIP_DIRECTION`, a scoped enumeration with the fixed underlying type `int`: every `int`
/// is a valid value. The two enumerators are `LEFT_RIGHT` (0) and `TOP_BOTTOM` (1); any
/// other value is [`FlipDirection::Other`] and takes the `TOP_BOTTOM` branches of
/// `MIRROR` and `MIRROR_AXIS`, which test only for `LEFT_RIGHT`.
///
/// Co-ported first for `ELLIPSE::Mirror` (unit
/// `libs.kimath.src.geometry.ellipse.application.ui.p001`), which uses it as
/// `kicad_math::ellipse::FlipDirection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlipDirection {
    /// Flip left to right (around the Y axis).
    LeftRight,
    /// Flip top to bottom (around the X axis).
    TopBottom,
    /// A value other than the two enumerators.
    Other(NonEnumerator),
}

/// A `FLIP_DIRECTION` value that is neither enumerator. Only [`FlipDirection::from_int`]
/// constructs one, so each value has exactly one representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NonEnumerator(i32);

impl NonEnumerator {
    /// The underlying `int`.
    pub const fn value(self) -> i32 {
        self.0
    }
}

impl FlipDirection {
    /// `static_cast<FLIP_DIRECTION>( aValue )`.
    pub const fn from_int(value: i32) -> Self {
        match value {
            0 => Self::LeftRight,
            1 => Self::TopBottom,
            _ => Self::Other(NonEnumerator(value)),
        }
    }

    /// The underlying `int`.
    pub const fn to_int(self) -> i32 {
        match self {
            Self::LeftRight => 0,
            Self::TopBottom => 1,
            Self::Other(value) => value.0,
        }
    }
}

/// An evaluation that is undefined in C++.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorError {
    /// A signed integer operation overflows.
    IntegerOverflow,
}

/// A type `MIRRORVAL<T>` is instantiated for: `int`, `long long` and `double`.
pub trait MirrorValue: Copy {
    /// `MIRRORVAL( aPoint, aMirrorRef )`: `-( aPoint - aMirrorRef ) + aMirrorRef`.
    fn mirror_val(point: Self, mirror_ref: Self) -> Result<Self, MirrorError>;
}

macro_rules! integer_mirror {
    ($($t:ty),*) => {$(
        impl MirrorValue for $t {
            fn mirror_val(point: Self, mirror_ref: Self) -> Result<Self, MirrorError> {
                point
                    .checked_sub(mirror_ref)
                    .and_then(<$t>::checked_neg)
                    .and_then(|negated| negated.checked_add(mirror_ref))
                    .ok_or(MirrorError::IntegerOverflow)
            }
        }
    )*};
}
integer_mirror!(i32, i64);

impl MirrorValue for f64 {
    fn mirror_val(point: Self, mirror_ref: Self) -> Result<Self, MirrorError> {
        // Compiled as aMirrorRef - ( aPoint - aMirrorRef ).
        Ok(fp::sub(mirror_ref, fp::sub(point, mirror_ref)))
    }
}

/// `MIRRORVAL( aPoint, aMirrorRef )`.
pub fn mirror_val<T: MirrorValue>(point: T, mirror_ref: T) -> Result<T, MirrorError> {
    T::mirror_val(point, mirror_ref)
}

/// `MIRROR( T& aPoint, const T& aMirrorRef )`: `aPoint = MIRRORVAL( aPoint, aMirrorRef )`.
/// `point` is unchanged when the evaluation is refused.
pub fn mirror<T: MirrorValue>(point: &mut T, mirror_ref: T) -> Result<(), MirrorError> {
    *point = mirror_val(*point, mirror_ref)?;
    Ok(())
}

/// `MIRROR( VT& aPoint, const VT& aMirrorRef, FLIP_DIRECTION aFlipDirection )`: mirror the
/// x coordinate for `LEFT_RIGHT`, otherwise the y coordinate.
pub fn mirror_in<T: MirrorValue>(
    point: &mut Vector2<T>,
    mirror_ref: Vector2<T>,
    flip_direction: FlipDirection,
) -> Result<(), MirrorError> {
    if flip_direction == FlipDirection::LeftRight {
        mirror(&mut point.x, mirror_ref.x)
    } else {
        mirror(&mut point.y, mirror_ref.y)
    }
}

/// `MIRROR_AXIS( aFlipDirection )`: the angle of the axis of reflection.
pub const fn mirror_axis(flip_direction: FlipDirection) -> EdaAngle {
    if matches!(flip_direction, FlipDirection::LeftRight) {
        ANGLE_90
    } else {
        ANGLE_0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_direction_keeps_every_int() {
        for v in [i32::MIN, -1, 0, 1, 2, 42, i32::MAX] {
            assert_eq!(FlipDirection::from_int(v).to_int(), v);
        }
        assert_eq!(FlipDirection::from_int(0), FlipDirection::LeftRight);
        assert_eq!(FlipDirection::from_int(1), FlipDirection::TopBottom);
        // A non-enumerator takes the TOP_BOTTOM branches.
        let other = FlipDirection::from_int(7);
        assert_eq!(mirror_axis(other).as_degrees().to_bits(), 0);
        let mut p = Vector2::new(1, 2);
        assert_eq!(mirror_in(&mut p, Vector2::new(10, 10), other), Ok(()));
        assert_eq!((p.x, p.y), (1, 18));
    }

    #[test]
    fn integer_overflow_is_refused_without_change() {
        let mut p = i32::MIN;
        assert_eq!(mirror(&mut p, 0), Err(MirrorError::IntegerOverflow));
        assert_eq!(p, i32::MIN);
        assert_eq!(mirror_val(i64::MAX, -1), Err(MirrorError::IntegerOverflow));
        assert_eq!(mirror_val(-5i64, 3), Ok(11));
    }

    #[test]
    fn double_keeps_the_compiled_nan_operand() {
        // 0 - ( sNaN - 0 ): the quieted NaN keeps its sign; a NaN reference wins.
        let nan = f64::from_bits(0x7ff4_0000_0000_0abc);
        assert_eq!(
            mirror_val(nan, 0.0).map(f64::to_bits),
            Ok(0x7ffc_0000_0000_0abc)
        );
        let reference = f64::from_bits(0xfff8_0000_0000_0123);
        assert_eq!(
            mirror_val(nan, reference).map(f64::to_bits),
            Ok(0xfff8_0000_0000_0123)
        );
        assert_eq!(mirror_val(0.0, 0.0).map(f64::to_bits), Ok(0));
        assert_eq!(mirror_val(-0.0, 0.0).map(f64::to_bits), Ok(0));
    }
}
