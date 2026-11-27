// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/trigo.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The inline bodies of `trigo.h` (libs/kimath/include/trigo.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.include.application.ui.p001` owns every inline definition of the header:
//! the four `RotatePoint` reference wrappers ([`rotate_point`], [`rotate_point_about`],
//! [`rotate_point_d`], [`rotate_point_d_about`]), the three `GetRotated` ([`get_rotated`],
//! [`get_rotated_about`], [`get_rotated_d`]), [`deg2rad`], [`rad2deg`], [`rad2decideg`], the
//! `NormalizeAnglePos`, `NORMALIZE_ANGLE_POS` and `NormalizeAngle180` templates
//! ([`normalize_angle_pos`], [`normalize_angle_pos_in_place`], [`normalize_angle_180`]) and
//! [`intercepts_positive_x`] / [`intercepts_negative_x`]. The out-of-line declarations belong to
//! trigo.cpp (`libs.kimath.src.trigo.application.ui.p001`, [`crate::trigo`]), which these
//! wrappers call unchanged.
//!
//! The wrappers only forward a vector's components to the trigo.cpp pointer overloads, so their
//! results, refusals ([`TrigoError::Undefined`]) and the `KiROUND` `kimathLogOverflow` deferral
//! are those of the callee. The `double` conversions are each one product and one quotient with a
//! constant, evaluated in source order with the reference NaN rules ([`fp`]); a constant is never
//! a NaN, so which operand GCC makes the destination does not change the result.
//!
//! The angle templates are ported for the integer types the pinned tree instantiates: `int`
//! (`NORMALIZE_ANGLE_POS`, eeschema's legacy library reader) and `long long`
//! (`NormalizeAngle180( UNIT_BINDER::GetValue() )`, pcbnew's zone properties panel). Their loops
//! cannot overflow (an addition runs only below `-1800`, a subtraction only above `1800`) and
//! always terminate; GCC 13.3 compiles them as loops, which this port evaluates in closed form
//! with the identical result. A floating-point `T` is not instantiated anywhere in the pinned
//! tree and is not ported.
use crate::eda_angle::{self, EdaAngle};
use crate::fp;
use crate::trigo::{self, TrigoResult};
use crate::vector::{Vector2D, Vector2I};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.application.ui.p001";

/// `M_PI` (glibc math.h `3.14159265358979323846`), which rounds to [`core::f64::consts::PI`].
const M_PI: f64 = core::f64::consts::PI;

/// `RotatePoint( VECTOR2I& point, const EDA_ANGLE& aAngle )`: rotate in place about the origin.
/// On an error the point is unchanged.
pub fn rotate_point(point: &mut Vector2I, angle: EdaAngle) -> TrigoResult<()> {
    trigo::rotate_point_i(&mut point.x, &mut point.y, angle)
}

/// `GetRotated( const VECTOR2I& aVector, const EDA_ANGLE& aAngle )`.
pub fn get_rotated(vector: Vector2I, angle: EdaAngle) -> TrigoResult<Vector2I> {
    let mut result = vector;
    trigo::rotate_point_i(&mut result.x, &mut result.y, angle)?;
    Ok(result)
}

/// `RotatePoint( VECTOR2I& aPoint, const VECTOR2I& aCentre, const EDA_ANGLE& aAngle )`: rotate
/// in place about `centre`. On an error the point is unchanged.
pub fn rotate_point_about(
    point: &mut Vector2I,
    centre: Vector2I,
    angle: EdaAngle,
) -> TrigoResult<()> {
    trigo::rotate_point_i_about(&mut point.x, &mut point.y, centre.x, centre.y, angle)
}

/// `GetRotated( const VECTOR2I& aVector, const VECTOR2I& aCentre, const EDA_ANGLE& aAngle )`.
pub fn get_rotated_about(
    vector: Vector2I,
    centre: Vector2I,
    angle: EdaAngle,
) -> TrigoResult<Vector2I> {
    let mut result = vector;
    trigo::rotate_point_i_about(&mut result.x, &mut result.y, centre.x, centre.y, angle)?;
    Ok(result)
}

/// `RotatePoint( VECTOR2D& point, const EDA_ANGLE& aAngle )`.
pub fn rotate_point_d(point: &mut Vector2D, angle: EdaAngle) {
    trigo::rotate_point_d(&mut point.x, &mut point.y, angle);
}

/// `RotatePoint( VECTOR2D& point, const VECTOR2D& aCenter, const EDA_ANGLE& aAngle )`.
pub fn rotate_point_d_about(point: &mut Vector2D, center: Vector2D, angle: EdaAngle) {
    trigo::rotate_point_d_about(&mut point.x, &mut point.y, center.x, center.y, angle);
}

/// `GetRotated( const VECTOR2D& aVector, const EDA_ANGLE& aAngle )`.
pub fn get_rotated_d(vector: Vector2D, angle: EdaAngle) -> Vector2D {
    let mut result = vector;
    trigo::rotate_point_d(&mut result.x, &mut result.y, angle);
    result
}

/// `DEG2RAD( deg )`: `deg * M_PI / 180.0`, left to right.
pub fn deg2rad(deg: f64) -> f64 {
    eda_angle::div(fp::mul(deg, M_PI), 180.0)
}

/// `RAD2DEG( rad )`: `rad * 180.0 / M_PI`, left to right.
pub fn rad2deg(rad: f64) -> f64 {
    eda_angle::div(fp::mul(rad, 180.0), M_PI)
}

/// `RAD2DECIDEG( rad )`: `rad * 1800.0 / M_PI`, left to right.
pub fn rad2decideg(rad: f64) -> f64 {
    eda_angle::div(fp::mul(rad, 1800.0), M_PI)
}

/// The integer `T` of the angle templates: `int` and `long long`. Every value converts to
/// `i128` exactly, and every template result lies in `[-1799, 3599]`.
pub trait AngleInt: Copy + private::Sealed {
    #[doc(hidden)]
    fn wide(self) -> i128;
    #[doc(hidden)]
    fn from_result(value: i16) -> Self;
}

mod private {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
}

impl AngleInt for i32 {
    fn wide(self) -> i128 {
        i128::from(self)
    }
    fn from_result(value: i16) -> Self {
        i32::from(value)
    }
}

impl AngleInt for i64 {
    fn wide(self) -> i128 {
        i128::from(self)
    }
    fn from_result(value: i16) -> Self {
        i64::from(value)
    }
}

/// A template result, which the closed forms below keep within `[-1799, 3599]`.
fn narrow<T: AngleInt>(value: i128) -> T {
    // Each caller's value is a Euclidean remainder by 3600 shifted by at most 1799.
    T::from_result(value as i16)
}

/// `NormalizeAnglePos<T>( T Angle )`: the angle in 0 .. 3600 (tenths of a degree).
///
/// `while( Angle < 0 ) Angle += 3600;` stops at the first non-negative value, which is the
/// Euclidean remainder; afterwards `Angle < 3600`, so the second loop does not run. Otherwise
/// `while( Angle >= 3600 ) Angle -= 3600;` stops at the first value below 3600, again the
/// remainder. Neither loop can overflow `T`.
pub fn normalize_angle_pos<T: AngleInt>(angle: T) -> T {
    let mut angle = angle.wide();

    if angle < 0 {
        angle = angle.rem_euclid(3600);
    }

    if angle >= 3600 {
        angle = angle.rem_euclid(3600);
    }

    narrow(angle)
}

/// `NORMALIZE_ANGLE_POS<T>( T& Angle )`: `Angle = NormalizeAnglePos( Angle )`.
pub fn normalize_angle_pos_in_place<T: AngleInt>(angle: &mut T) {
    *angle = normalize_angle_pos(*angle);
}

/// `NormalizeAngle180<T>( T Angle )`: the angle in (-1800, 1800] (tenths of a degree).
///
/// `while( Angle <= -1800 ) Angle += 3600;` stops at the first value above -1800, the member of
/// the residue class in -1799 ..= 1800; the second loop then does not run. Otherwise
/// `while( Angle > 1800 ) Angle -= 3600;` stops at the first value not above 1800, the same
/// member. That member is `( Angle + 1799 ) mod 3600 - 1799` (Euclidean), computed in `i128`.
pub fn normalize_angle_180<T: AngleInt>(angle: T) -> T {
    let mut angle = angle.wide();

    if angle <= -1800 {
        angle = (angle + 1799).rem_euclid(3600) - 1799;
    }

    if angle > 1800 {
        angle = (angle + 1799).rem_euclid(3600) - 1799;
    }

    narrow(angle)
}

/// `InterceptsPositiveX( aStartAngle, aEndAngle )`: whether the counter-clockwise arc from
/// `start_angle` to `end_angle` (degrees) crosses the positive X axis.
pub fn intercepts_positive_x(start_angle: f64, end_angle: f64) -> bool {
    let mut end = end_angle;

    if start_angle > end_angle {
        end += 360.0;
    }

    start_angle < 360.0 && end > 360.0
}

/// `InterceptsNegativeX( aStartAngle, aEndAngle )`: whether the counter-clockwise arc from
/// `start_angle` to `end_angle` (degrees) crosses the negative X axis.
pub fn intercepts_negative_x(start_angle: f64, end_angle: f64) -> bool {
    let mut end = end_angle;

    if start_angle > end_angle {
        end += 360.0;
    }

    start_angle < 180.0 && end > 180.0
}

#[cfg(test)]
#[path = "trigo_header_tests.rs"]
mod tests;
