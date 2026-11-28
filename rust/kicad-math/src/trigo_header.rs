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
//! ([`normalize_angle_pos`], [`normalize_angle_pos_in_place`], [`normalize_angle_180`], and for
//! `double` [`normalize_angle_pos_d`], [`normalize_angle_pos_in_place_d`],
//! [`normalize_angle_180_d`]) and
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
//! The angle templates are ported for `int` and `long long`, the types the pinned tree
//! instantiates (`NORMALIZE_ANGLE_POS` in eeschema's legacy library reader,
//! `NormalizeAngle180( UNIT_BINDER::GetValue() )` in pcbnew's zone properties panel), and for
//! `double`, which no pinned caller uses. The integer loops cannot overflow (an addition runs only
//! below `-1800`, a subtraction only above `1800`) and always terminate; GCC 13.3 compiles them
//! as loops, which this port evaluates in closed form with the identical result. The `double`
//! loops round at every pass; [`normalize_angle_pos_d`] and [`normalize_angle_180_d`] reproduce
//! each rounded pass exactly and refuse only the inputs on which a loop never terminates.
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

// ---------------------------------------------------------------------------------------------
// The `double` instantiations.
//
// GCC 13.3 (`-O3 -ffp-contract=off`, no fast math) compiles each loop literally: a `comisd` test
// and one `addsd` or `subsd` of the constant 3600.0 per pass, so every pass rounds to nearest,
// ties to even. A NaN fails every comparison and is returned unchanged (no arithmetic touches
// it, so a signalling NaN keeps its bits), as is `-0.0`.
//
// Rounding is symmetric under negation, so both directions are one sequence on the magnitude:
// `m -> fl( m - 3600 )` (an addition to a negative angle is this step on `-angle`). That step:
//
// * is exact while `m <= 2^57`: `m` is then a multiple of its ulp `u <= 16`, 3600 = 225 * 16 is
//   a multiple of `u`, and a smaller non-negative result is a multiple of its own (smaller) ulp.
//   Only the pass that changes the angle's sign can round (an addition from a tiny negative
//   angle to just below 3600, for example, gives 3600 and so one more pass of the second loop of
//   `NormalizeAnglePos`); the reduction below stops before such a pass and runs it literally.
// * above `2^57`, within the binade `[2^e, 2^(e+1))` of ulp `u`, rounds `m - 3600` to the grid
//   `u` whenever `m - 3600 >= 2^e`. The step is then the same for every member of the binade:
//   3600 rounded to a multiple of `u` is 3584 for `u = 64 .. 512` and 4096 for `u = 1024 .. 4096`;
//   for `u = 32` it is a tie, decided by the parity of `m / 32`, and every result of a step is an
//   even multiple of 32, from which the step is 3584 and the parity stays even.
// * makes no progress (`fl( m - 3600 ) == m`) for every `m > 2^65`: the ulp is at least 8192,
//   more than twice 3600, and at the bottom of a binade above `2^65` the lower binade's ulp is
//   at least 8192 too. At `m = 2^65` the lower ulp is 4096 and the step 4096. Below `2^65` every
//   step is at least 3584, so a sequence that progresses once never stops progressing.
//
// A loop that makes no progress never terminates: undefined behaviour in C++ (a side-effect-free
// loop must terminate, [intro.progress]), refused as `TrigoError::Undefined`. That is exactly the
// magnitude above `2^65` or infinite, tested by the first pass. Every other input terminates and
// is evaluated exactly, however many passes the loops need.

/// The literal constant of the template loops.
const TURN: f64 = 3600.0;

/// `2^57`: up to this magnitude every pass is exact.
const EXACT_STEPS: i128 = 1 << 57;

/// `2^65`: the largest magnitude whose sequence progresses.
const LAST_PROGRESS: f64 = 36_893_488_147_419_103_232.0;

/// `fl( m - 3600 )` for an integer magnitude whose binary64 value is `m` (an `i128` to binary64
/// conversion rounds to nearest, ties to even, as the `subsd` of the exact difference does).
fn step_large(m: i128) -> i128 {
    // Every member here is below 2^66, so the conversion back is exact.
    ((m - 3600) as f64) as i128
}

/// The first member of the sequence `m, fl( m - 3600 ), ...` that is at most `floor`, for an
/// integral magnitude `m <= 2^65` and `floor >= 7200`. Within a binade the constant step (above;
/// below `2^57` it is the exact 3600) lets the members be skipped in one subtraction.
fn skip_large(mut m: i128, floor: i128) -> TrigoResult<i128> {
    while m > floor {
        let next = step_large(m);

        if next == m {
            return Err(trigo::TrigoError::Undefined);
        }

        m = next;

        if m <= floor {
            break;
        }

        // The binade [lo, 2 lo) of this member: while a member minus 3600 stays at or above lo,
        // its step is the same as this member's.
        let lo = 1_i128 << (127 - m.leading_zeros());
        let delta = m - step_large(m);

        if delta > 0 && m - 3600 >= lo {
            // Passes from members m - s * delta, s = 0 .. passes - 1, each still at least
            // lo + 3600 and above the floor.
            let by_binade = (m - 3600 - lo) / delta + 1;
            let by_floor = (m - floor - 1) / delta;
            m -= by_binade.min(by_floor).max(0) * delta;
        }
    }

    Ok(m)
}

/// A member of the magnitude sequence `m, fl( m - 3600 ), ...` for a positive (or infinite) `m`
/// on which every one of the four loops still runs: `m` itself below 7200, otherwise the first
/// member below 7200 (which is at least 3600). Every member before it is a pass the loop takes.
fn skip_turns(m: f64) -> TrigoResult<f64> {
    if m < 2.0 * TURN {
        return Ok(m);
    }

    // The first pass decides termination (above).
    if m - TURN == m {
        return Err(trigo::TrigoError::Undefined);
    }

    let mut m = m;

    if m > EXACT_STEPS as f64 {
        // Integral and at most 2^65 here: the conversions are exact.
        if m > LAST_PROGRESS {
            return Err(trigo::TrigoError::Undefined);
        }
        m = skip_large(m as i128, EXACT_STEPS)? as f64;
    }

    // Exact passes from here: the member m - 3600 i in [3600, 7200), computed from the integer
    // part n (exact, m < 2^63) and the exact fraction f = m - n.
    let n = m as i64;
    let f = m - n as f64;
    let whole = (n - 3600).rem_euclid(3600) + 3600;

    // Both terms are exact and so is their sum, a member of the sequence.
    Ok(whole as f64 + f)
}

/// `NormalizeAnglePos<double>`: both loops, run literally after the reduction.
fn pos_loops(mut angle: f64) -> f64 {
    while angle < 0.0 {
        angle += TURN;
    }

    while angle >= TURN {
        angle -= TURN;
    }

    angle
}

/// `NormalizeAngle180<double>`: both loops, run literally after the reduction.
fn half_loops(mut angle: f64) -> f64 {
    while angle <= -1800.0 {
        angle += TURN;
    }

    while angle > 1800.0 {
        angle -= TURN;
    }

    angle
}

/// `NormalizeAnglePos<double>( double Angle )`. `Err(Undefined)` where a loop never terminates:
/// an infinite angle, or a magnitude above `2^65`, where `Angle + 3600` (or `- 3600`) rounds back
/// to `Angle`. Any other angle gives the exact result of the rounded passes; a NaN and `-0.0` are
/// returned unchanged.
pub fn normalize_angle_pos_d(angle: f64) -> TrigoResult<f64> {
    let mut angle = angle;

    // Skip the passes of whichever loop runs first to a member from which it still runs.
    if angle < 0.0 {
        angle = -skip_turns(-angle)?;
    } else if angle >= TURN {
        angle = skip_turns(angle)?;
    }

    Ok(pos_loops(angle))
}

/// `NORMALIZE_ANGLE_POS<double>( double& Angle )`: `Angle = NormalizeAnglePos( Angle )`. On an
/// error the angle is unchanged.
pub fn normalize_angle_pos_in_place_d(angle: &mut f64) -> TrigoResult<()> {
    *angle = normalize_angle_pos_d(*angle)?;
    Ok(())
}

/// `NormalizeAngle180<double>( double Angle )`, refusing as [`normalize_angle_pos_d`] does.
pub fn normalize_angle_180_d(angle: f64) -> TrigoResult<f64> {
    let mut angle = angle;

    if angle <= -1800.0 {
        angle = -skip_turns(-angle)?;
    } else if angle > 1800.0 {
        angle = skip_turns(angle)?;
    }

    Ok(half_loops(angle))
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
