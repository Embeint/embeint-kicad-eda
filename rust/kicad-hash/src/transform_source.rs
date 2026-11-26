// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/src/transform.cpp.
// Copyright (C) 2010 Wayne Stambaugh <stambaughw@gmail.com>
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `libs/kimath/src/transform.cpp` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7:
//! the out-of-line `TRANSFORM` bodies owned by unit
//! `libs.kimath.src.transform.application.ui.p001`.
//!
//! | C++ definition | Rust |
//! | --- | --- |
//! | `TRANSFORM::operator==` | accepted co-port `PartialEq::eq` in [`crate::transform`] (not duplicated here) |
//! | `TRANSFORM::TransformCoordinate( const VECTOR2I& )` | [`Transform::transform_coordinate`] |
//! | `TRANSFORM::TransformCoordinate( const BOX2I& )` | [`Transform::transform_box`] |
//! | `TRANSFORM::InverseTransform()` | [`Transform::inverse_transform`] |
//! | `TRANSFORM::MapAngles( EDA_ANGLE*, EDA_ANGLE* )` | [`Transform::map_angles`] |
//! | `std::hash<TRANSFORM>::operator()` | [`StdHash`] for [`Transform`] |
//!
//! The types are the canonical accepted ones: [`Transform`] from the header unit
//! `libs.kimath.include.application.pure.p001`, `VECTOR2I`/`BOX2I` and `EDA_ANGLE` from
//! `kicad_math`, and `hash_combine` from [`crate::hash`]. Nothing here is unported.
//!
//! `int` arithmetic whose C++ evaluation overflows, and integer division by zero, are
//! undefined behaviour in the reference; the bodies return [`TransformError`] for exactly
//! those inputs instead of inventing a value. Every other input gives the reference result.
//!
//! The source also defines the mutable global `TRANSFORM DefaultTransform`, which
//! `eeschema/lib_symbol.cpp` reassigns at run time. Portable code keeps no global state, so it
//! is not modelled as a global: [`DEFAULT_TRANSFORM_INITIAL`] is only its initial value, and a
//! host that needs the current default owns and passes that state explicitly.

use crate::hash::{hash_combine, StdHash};
use crate::transform::Transform;
use kicad_math::box2::Box2I;
use kicad_math::eda_angle::{AngleUnit, EdaAngle, GeometryError, ANGLE_0, ANGLE_360};
use kicad_math::{Vector2D, Vector2I, VectorDomainError};

/// Owning unit of the bodies in this module.
pub const UNIT: &str = "libs.kimath.src.transform.application.ui.p001";

/// Initial value of the C++ global `DefaultTransform = TRANSFORM( 1, 0, 0, 1 )`. The C++
/// object is mutable and later reassigned by the schematic editor; this constant is not that
/// object (see the module documentation).
pub const DEFAULT_TRANSFORM_INITIAL: Transform = Transform {
    x1: 1,
    y1: 0,
    x2: 0,
    y2: 1,
};

/// Why a body refused an input whose reference evaluation is undefined, or why a canonical
/// callee failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformError {
    /// A signed `int` (or the box's `int64_t` end) expression overflowed: undefined behaviour.
    IntegerOverflow,
    /// `int` division by a zero determinant (singular matrix): undefined behaviour.
    DivisionByZero,
    /// A canonical callee is not available. No body here currently produces this.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
    /// Any other canonical vector/box domain refusal, passed through unchanged.
    Vector(VectorDomainError),
    /// Any other canonical angle refusal, passed through unchanged.
    Geometry(GeometryError),
}

impl From<VectorDomainError> for TransformError {
    fn from(error: VectorDomainError) -> Self {
        match error {
            VectorDomainError::IntegerOverflow => Self::IntegerOverflow,
            VectorDomainError::DivisionByZero => Self::DivisionByZero,
            VectorDomainError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            other => Self::Vector(other),
        }
    }
}

impl From<GeometryError> for TransformError {
    fn from(error: GeometryError) -> Self {
        match error {
            GeometryError::IntegerOverflow => Self::IntegerOverflow,
            GeometryError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            other => Self::Geometry(other),
        }
    }
}

pub type TransformResult<T> = Result<T, TransformError>;

fn mul(a: i32, b: i32) -> TransformResult<i32> {
    a.checked_mul(b).ok_or(TransformError::IntegerOverflow)
}

fn add(a: i32, b: i32) -> TransformResult<i32> {
    a.checked_add(b).ok_or(TransformError::IntegerOverflow)
}

fn sub(a: i32, b: i32) -> TransformResult<i32> {
    a.checked_sub(b).ok_or(TransformError::IntegerOverflow)
}

fn neg(a: i32) -> TransformResult<i32> {
    a.checked_neg().ok_or(TransformError::IntegerOverflow)
}

/// C++ `int` division truncates toward zero, as Rust's does. A zero divisor and
/// `INT_MIN / -1` are undefined in C++ and refused.
fn div(a: i32, b: i32) -> TransformResult<i32> {
    if b == 0 {
        return Err(TransformError::DivisionByZero);
    }
    a.checked_div(b).ok_or(TransformError::IntegerOverflow)
}

/// `std::hash<TRANSFORM>`: `seed = std::hash<int>{}( x1 )`, then
/// `hash_combine( seed, y1, x2, y2 )` (64-bit libstdc++: the `int` sign-extends to `size_t`).
impl StdHash for Transform {
    fn std_hash(&self) -> u64 {
        let mut seed = self.x1.std_hash();
        hash_combine(&mut seed, &[&self.y1, &self.x2, &self.y2]);
        seed
    }
}

impl Transform {
    /// `TransformCoordinate( const VECTOR2I& aPoint )`:
    /// `( x1 * p.x + y1 * p.y, x2 * p.x + y2 * p.y )` in `int` arithmetic.
    ///
    /// Any overflowing product or sum is undefined in C++ and returns
    /// [`TransformError::IntegerOverflow`]; the C++ evaluation order of the four products is
    /// unspecified, which cannot change a defined result.
    pub fn transform_coordinate(&self, point: Vector2I) -> TransformResult<Vector2I> {
        let x = add(mul(self.x1, point.x)?, mul(self.y1, point.y)?)?;
        let y = add(mul(self.x2, point.x)?, mul(self.y2, point.y)?)?;
        Ok(Vector2I::new(x, y))
    }

    /// `TransformCoordinate( const BOX2I& aRect )`: a default `BOX2I`, then
    /// `SetOrigin( TransformCoordinate( aRect.GetOrigin() ) )` and
    /// `SetEnd( TransformCoordinate( aRect.GetEnd() ) )`, in that order.
    ///
    /// `GetEnd` adds the `int` origin and `int64_t` size and narrows modulo 2^32 (Linux's
    /// generic `KiCheckedCast<long, int>`), through the canonical [`Box2I::end`]; only an
    /// overflowing `int64_t` sum is refused there. The result is not normalized: a mirror or
    /// rotation yields a negative size exactly as in C++, and the result is always initialized.
    pub fn transform_box(&self, rect: &Box2I) -> TransformResult<Box2I> {
        let mut result = Box2I::default();
        result.set_origin(self.transform_coordinate(rect.origin())?);
        let end = self.transform_coordinate(rect.end()?)?;
        result.set_end(end)?;
        Ok(result)
    }

    /// `InverseTransform()`: `det = x1 * y2 - x2 * y1`, then the `int` quotients
    /// `y2 / det`, `-x2 / det`, `-y1 / det` and `x1 / det`, giving
    /// `TRANSFORM( invx1, invy1, invx2, invy2 )`.
    ///
    /// The source comment asserts the determinant is never null, but nothing enforces it: a
    /// singular matrix divides by zero, which is undefined, and returns
    /// [`TransformError::DivisionByZero`]. Overflow of the determinant, of a negation
    /// (`INT_MIN`) or of `INT_MIN / -1` returns [`TransformError::IntegerOverflow`]. Integer
    /// truncation of the quotients is kept: the result is only a true inverse for the
    /// unimodular orientation matrices KiCad uses.
    pub fn inverse_transform(&self) -> TransformResult<Transform> {
        let det = sub(mul(self.x1, self.y2)?, mul(self.x2, self.y1)?)?;
        let invx1 = div(self.y2, det)?;
        let invx2 = div(neg(self.x2)?, det)?;
        let invy1 = div(neg(self.y1)?, det)?;
        let invy2 = div(self.x1, det)?;
        Ok(Transform::new(invx1, invy1, invx2, invy2))
    }

    /// `MapAngles( EDA_ANGLE* aAngle1, EDA_ANGLE* aAngle2 )`: maps both angles through the
    /// matrix, swaps them when the swept angle changed by more than 0.1 degree, orders them,
    /// and returns whether they were swapped.
    ///
    /// The C++ `wxCHECK_MSG` on null pointers cannot be reached: Rust references are never
    /// null. Both angles are updated in place in source order; on a refused callee (none is
    /// currently possible) the angles already assigned keep their new values, as in C++.
    ///
    /// The cosine and sine of any angle are finite (non-finite angles normalize to 0), so
    /// `x * x1 + y * y1` never produces a NaN and plain IEEE arithmetic gives the reference
    /// bits (no contraction: the reference builds with `-ffp-contract=off`).
    ///
    /// The reference compares against `epsilon.Invert().Normalize()`, which is 359.9 degrees,
    /// not -0.1; with a normalized residual in [0, 360) the swap condition therefore always
    /// holds. That is ported as written (see
    /// `port/upstream-issues/transform-mapangles-always-swaps.md`).
    pub fn map_angles(
        &self,
        angle1: &mut EdaAngle,
        angle2: &mut EdaAngle,
    ) -> TransformResult<bool> {
        let epsilon = EdaAngle::new(0.1, AngleUnit::Degrees);
        let mut swap = false;

        let delta = *angle2 - *angle1;

        let x = angle1.cos();
        let y = angle1.sin();
        let v = Vector2D::new(
            x * f64::from(self.x1) + y * f64::from(self.y1),
            x * f64::from(self.x2) + y * f64::from(self.y2),
        );
        *angle1 = EdaAngle::from_vector(v)?;

        let x = angle2.cos();
        let y = angle2.sin();
        let v = Vector2D::new(
            x * f64::from(self.x1) + y * f64::from(self.y1),
            x * f64::from(self.x2) + y * f64::from(self.y2),
        );
        *angle2 = EdaAngle::from_vector(v)?;

        let delta_transformed = *angle2 - *angle1;
        let mut residual_error = delta_transformed - delta;
        residual_error.normalize();

        // `epsilon.Invert().Normalize()` normalizes a temporary copy.
        if residual_error > epsilon || residual_error < epsilon.invert().normalized() {
            core::mem::swap(angle1, angle2);
            swap = true;
        }

        if *angle2 < *angle1 {
            if *angle2 < ANGLE_0 {
                angle2.normalize();
            } else {
                // `aAngle1->Normalize()` mutates *aAngle1 before the subtraction.
                *angle1 = angle1.normalize() - ANGLE_360;
            }
        }

        Ok(swap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity_and_maps_points_unchanged() {
        let t = DEFAULT_TRANSFORM_INITIAL;
        assert_eq!(t, Transform::default());
        assert_eq!(
            t.transform_coordinate(Vector2I::new(i32::MIN, i32::MAX)),
            Ok(Vector2I::new(i32::MIN, i32::MAX))
        );
    }

    #[test]
    fn overflow_and_singular_inputs_are_refused() {
        let t = Transform::new(-1, 0, 0, 1);
        assert_eq!(
            t.transform_coordinate(Vector2I::new(i32::MIN, 0)),
            Err(TransformError::IntegerOverflow)
        );
        assert_eq!(
            Transform::new(1, 2, 2, 4).inverse_transform(),
            Err(TransformError::DivisionByZero)
        );
        assert_eq!(
            Transform::new(1, 0, i32::MIN, 1).inverse_transform(),
            Err(TransformError::IntegerOverflow)
        );
        assert_eq!(
            Transform::new(0, 1, 1, 0).inverse_transform(),
            Ok(Transform::new(0, 1, 1, 0))
        );
    }

    #[test]
    fn mirrored_box_keeps_negative_size() {
        let mut rect = Box2I::default();
        rect.set_origin(Vector2I::new(10, 20));
        rect.set_size(kicad_math::Vector2L::new(5, 7));
        let out = Transform::new(-1, 0, 0, 1).transform_box(&rect);
        let out = out.map(|b| (b.origin(), b.size(), b.is_valid()));
        assert_eq!(
            out,
            Ok((
                Vector2I::new(-10, 20),
                kicad_math::Vector2L::new(-5, 7),
                true
            ))
        );
    }

    #[test]
    fn hash_follows_hash_combine() {
        let t = Transform::new(1, -1, 0, 2);
        let mut seed = 1_u64;
        hash_combine(&mut seed, &[&-1_i32, &0_i32, &2_i32]);
        assert_eq!(t.std_hash(), seed);
    }

    #[test]
    fn map_angles_always_swaps() {
        let mut a1 = EdaAngle::degrees(10.0);
        let mut a2 = EdaAngle::degrees(80.0);
        assert_eq!(
            DEFAULT_TRANSFORM_INITIAL.map_angles(&mut a1, &mut a2),
            Ok(true)
        );
    }
}
