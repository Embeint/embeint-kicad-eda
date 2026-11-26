// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/src/geometry/ellipse.cpp, copyright The KiCad Developers.
//! Out-of-line `ELLIPSE<NumericType>` bodies at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.src.geometry.ellipse.application.ui.p001` owns the two constructors,
//! `Mirror`, `GetSubtendedAngle`, `GetPointAtAngle` and `GetAngleAtPoint`, explicitly
//! instantiated for `double` and `int` (`template class ELLIPSE<double>; ELLIPSE<int>;`). They
//! are methods of the header-owned [`Ellipse`] model; this module adds no second ellipse type.
//! [`EllipseCoord`] carries the only places where the two instantiations differ.
//!
//! Every `double` operation goes through the accepted reference-NaN helpers ([`crate::fp`],
//! [`crate::eda_angle::div`]) in source order without contraction; `std::cos`, `std::sin` and
//! `std::atan2` are the accepted glibc 2.39 backends, so native and wasm results agree.
//!
//! Operations the pinned C++ leaves undefined are refused rather than given a value:
//! signed `int` overflow in `Mirror` and in `GetPointAtAngle`'s final addition, a
//! `double` -> `int` conversion of a NaN or out-of-range value in the major-axis constructor
//! ([conv.fpint]), and `std::max` with a NaN radius in `GetAngleAtPoint` ([alg.min.max]
//! requires `Cpp17LessThanComparable` values, which NaN is not).
use crate::eda_angle::{div, AngleUnit, EdaAngle, GeometryError, GeometryResult};
use crate::eda_angle::{ANGLE_0, ANGLE_180, FULL_CIRCLE};
use crate::geometry_headers::Ellipse;
use crate::vector::{VectorComponent, VectorDomainError};
use crate::{fp, glibc_libm, glibc_trig, util, Vector2};

pub const UNIT: &str = "libs.kimath.src.geometry.ellipse.application.ui.p001";

/// Refusals and deferrals of the ellipse bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EllipseError {
    /// A callee owned by another unit is not ported yet.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
    /// Signed `int` arithmetic overflowed: undefined behaviour in C++.
    IntegerOverflow,
    /// `NumericType( double )` of a NaN or a value whose truncation is not representable.
    UndefinedConversion,
    /// `std::max` received a NaN operand.
    UndefinedComparison,
}
impl From<GeometryError> for EllipseError {
    fn from(e: GeometryError) -> Self {
        match e {
            GeometryError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            GeometryError::IntegerOverflow | GeometryError::ResourceLimit => Self::IntegerOverflow,
        }
    }
}
fn vector_error(e: VectorDomainError) -> GeometryError {
    match e {
        VectorDomainError::Unported { upstream, unit } => {
            GeometryError::Unported { upstream, unit }
        }
        _ => GeometryError::IntegerOverflow,
    }
}

/// `FLIP_DIRECTION` (`include/core/mirror.h`), co-ported with its two enumerators only; the
/// header belongs to unit `include.core.application.pure.p001`. Any value other than
/// `LEFT_RIGHT` takes `Mirror`'s `else` branches, exactly like `TopBottom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipDirection {
    /// Flip left to right (around the Y axis).
    LeftRight,
    /// Flip top to bottom (around the X axis).
    TopBottom,
}

/// The default arguments of both constructors: `aStartAngle = ANGLE_0`,
/// `aEndAngle = FULL_CIRCLE`. C++ can omit the end angle alone or both, never only the start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArcAngles {
    /// Both defaulted.
    Defaults,
    /// `aStartAngle` given, `aEndAngle` defaulted.
    Start(EdaAngle),
    /// Both given.
    StartEnd(EdaAngle, EdaAngle),
}
impl ArcAngles {
    fn resolve(self) -> (EdaAngle, EdaAngle) {
        match self {
            Self::Defaults => (ANGLE_0, FULL_CIRCLE),
            Self::Start(start) => (start, FULL_CIRCLE),
            Self::StartEnd(start, end) => (start, end),
        }
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for f64 {}
}

/// The two explicit instantiations, `ELLIPSE<int>` and `ELLIPSE<double>`.
pub trait EllipseCoord: VectorComponent + sealed::Sealed {
    /// `static_cast<double>( v )` and the usual arithmetic conversion to `double`.
    fn to_f64(self) -> f64;
    /// `NumericType( MajorRadius * aRatio )`.
    fn scaled_radius(major: Self, ratio: f64) -> Result<Self, EllipseError>;
    /// `2 * aRef - aCenter` in the instantiation's arithmetic.
    fn mirrored(reference: Self, center: Self) -> Result<Self, EllipseError>;
    /// `Center + VECTOR2<NumericType>( ... )` from the rotated `double` offsets.
    fn offset(center: Vector2<Self>, rx: f64, ry: f64) -> GeometryResult<Vector2<Self>>;
}
impl EllipseCoord for i32 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn scaled_radius(major: Self, ratio: f64) -> Result<Self, EllipseError> {
        let product = fp::mul(f64::from(major), ratio);
        // [conv.fpint]: the truncated value must be representable, and NaN never is.
        if product.is_nan() {
            return Err(EllipseError::UndefinedConversion);
        }
        let truncated = product.trunc();
        if !(-2_147_483_648.0..2_147_483_648.0).contains(&truncated) {
            return Err(EllipseError::UndefinedConversion);
        }
        Ok(truncated as i32)
    }
    fn mirrored(reference: Self, center: Self) -> Result<Self, EllipseError> {
        2i32.checked_mul(reference)
            .and_then(|twice| twice.checked_sub(center))
            .ok_or(EllipseError::IntegerOverflow)
    }
    fn offset(center: Vector2<Self>, rx: f64, ry: f64) -> GeometryResult<Vector2<Self>> {
        // VECTOR2I( KiROUND( rx ), KiROUND( ry ) ); the argument order is unspecified, and either
        // overflow reaches the same unported kimathLogOverflow.
        let x = util::round_i32(rx).map_err(vector_error)?;
        let y = util::round_i32(ry).map_err(vector_error)?;
        center
            .try_add(Vector2::new(x, y))
            .map_err(|_| GeometryError::IntegerOverflow)
    }
}
impl EllipseCoord for f64 {
    fn to_f64(self) -> f64 {
        self
    }
    fn scaled_radius(major: Self, ratio: f64) -> Result<Self, EllipseError> {
        // Compiled as aRatio * MajorRadius: a NaN ratio wins over a NaN radius.
        Ok(fp::mul(ratio, major))
    }
    fn mirrored(reference: Self, center: Self) -> Result<Self, EllipseError> {
        // The int literal 2 converts to 2.0 before the multiplication.
        Ok(fp::sub(fp::mul(2.0, reference), center))
    }
    fn offset(center: Vector2<Self>, rx: f64, ry: f64) -> GeometryResult<Vector2<Self>> {
        // Compiled as ( rx, ry ) + Center: a NaN offset wins over a NaN centre.
        Ok(Vector2::new(fp::add(rx, center.x), fp::add(ry, center.y)))
    }
}

/// How the oracle applies `Rotation.Sin()` to an operand. `EDA_ANGLE::Sin` returns the
/// constants 1.0 and -1.0 for the normalized 90 and 270 degree branches, and GCC replaces the
/// multiplication by those constants with a copy and a sign flip (`xorpd`). Neither quiets a
/// signalling NaN and the flip negates a NaN, so they are kept distinct from a multiplication;
/// every other branch, including glibc results that happen to equal +/-1, multiplies.
#[derive(Clone, Copy)]
enum SinFactor {
    One,
    MinusOne,
    Value(f64),
}
impl SinFactor {
    fn of(rotation: EdaAngle) -> Self {
        let test = rotation.normalized().as_degrees();
        if test == 90.0 {
            Self::One
        } else if test == 270.0 {
            Self::MinusOne
        } else {
            Self::Value(rotation.sin())
        }
    }
    fn times(self, operand: f64) -> f64 {
        match self {
            Self::One => operand,
            Self::MinusOne => -operand,
            Self::Value(sin) => fp::mul(operand, sin),
        }
    }
}

/// `std::max( 1.0, v )`: libstdc++ returns `( 1.0 < v ) ? v : 1.0`, but a NaN `v` breaks the
/// function's ordering precondition.
fn at_least_one(v: f64) -> Result<f64, EllipseError> {
    if v.is_nan() {
        return Err(EllipseError::UndefinedComparison);
    }
    Ok(util::max(1.0, v))
}

impl<T: EllipseCoord> Ellipse<T> {
    /// `ELLIPSE( aCenter, aMajorRadius, aMinorRadius, aRotation, aStartAngle, aEndAngle )`.
    pub fn new(
        center: Vector2<T>,
        major_radius: T,
        minor_radius: T,
        rotation: EdaAngle,
        angles: ArcAngles,
    ) -> Self {
        let (start_angle, end_angle) = angles.resolve();
        Self {
            center,
            major_radius,
            minor_radius,
            rotation,
            start_angle,
            end_angle,
        }
    }

    /// `ELLIPSE( aCenter, aMajor, aRatio, aStartAngle, aEndAngle )`, the DXF form whose major
    /// axis end point also defines the rotation.
    pub fn from_major_axis(
        center: Vector2<T>,
        major: Vector2<T>,
        ratio: f64,
        angles: ArcAngles,
    ) -> Result<Self, EllipseError> {
        let (start_angle, end_angle) = angles.resolve();
        // Body order: MajorRadius, MinorRadius, then Rotation.
        let major_radius = major
            .euclidean_norm()
            .map_err(|e| EllipseError::from(vector_error(e)))?;
        let minor_radius = T::scaled_radius(major_radius, ratio)?;
        let rotation = EdaAngle::new(
            glibc_libm::atan2(major.y.to_f64(), major.x.to_f64()),
            AngleUnit::Radians,
        );
        Ok(Self {
            center,
            major_radius,
            minor_radius,
            rotation,
            start_angle,
            end_angle,
        })
    }

    /// `Mirror( aRef, aFlipDirection )`. Nothing is changed when the result is refused.
    pub fn mirror(
        &mut self,
        reference: Vector2<T>,
        direction: FlipDirection,
    ) -> Result<(), EllipseError> {
        let mut center = self.center;
        if direction == FlipDirection::LeftRight {
            center.x = T::mirrored(reference.x, center.x)?;
        } else {
            center.y = T::mirrored(reference.y, center.y)?;
        }
        self.center = center;

        self.rotation = -self.rotation;

        let old_start = self.start_angle;
        let old_end = self.end_angle;

        if direction == FlipDirection::LeftRight {
            // theta' = pi - theta  ->  [s, e]  ->  [pi - e, pi - s]
            self.start_angle = ANGLE_180 - old_end;
            self.end_angle = ANGLE_180 - old_start;
        } else {
            // theta' = -theta  ->  [s, e]  ->  [-e, -s]
            self.start_angle = -old_end;
            self.end_angle = -old_start;
        }
        Ok(())
    }

    /// `GetSubtendedAngle()`: `EndAngle - StartAngle`, normalized to [0, 360).
    pub fn subtended_angle(&self) -> EdaAngle {
        let mut subtended = self.end_angle - self.start_angle;
        subtended.normalize()
    }

    /// `GetPointAtAngle( angle )` in the ellipse's local (major, minor) frame.
    pub fn point_at_angle(&self, angle: EdaAngle) -> GeometryResult<Vector2<T>> {
        let t = angle.as_radians();
        // The oracle evaluates std::cos( t ) and std::sin( t ) with one glibc sincos call.
        let (sin_t, cos_t) = glibc_trig::sincos(t);
        let ex = fp::mul(self.major_radius.to_f64(), cos_t);
        let ey = fp::mul(self.minor_radius.to_f64(), sin_t);

        let cos_r = self.rotation.cos();
        let sin_r = SinFactor::of(self.rotation);
        // Compiled operand order (oracle disassembly): rx = ex * cosR - ey * sinR, but
        // ry = ey * cosR + ex * sinR. The order decides only which of two NaNs survives.
        let rx = fp::sub(fp::mul(cos_r, ex), sin_r.times(ey));
        let ry = fp::add(fp::mul(cos_r, ey), sin_r.times(ex));

        T::offset(self.center, rx, ry)
    }

    /// `GetAngleAtPoint( aPt )`: the parametric angle of `point`.
    pub fn angle_at_point(&self, point: Vector2<T>) -> Result<EdaAngle, EllipseError> {
        let a = at_least_one(self.major_radius.to_f64())?;
        let b = at_least_one(self.minor_radius.to_f64())?;

        let dx = fp::sub(point.x.to_f64(), self.center.x.to_f64());
        let dy = fp::sub(point.y.to_f64(), self.center.y.to_f64());

        let cos_rot = self.rotation.cos();
        let sin_factor = SinFactor::of(self.rotation);
        let sin_rot = self.rotation.sin();
        let lx = fp::add(fp::mul(dx, cos_rot), sin_factor.times(dy));
        // -dx * sinRot + dy * cosRot is compiled as dy * cosRot - sinRot * dx: equal for every
        // value except the sign of a NaN dx, which the oracle keeps.
        let ly = fp::sub(fp::mul(dy, cos_rot), fp::mul(sin_rot, dx));

        Ok(EdaAngle::new(
            glibc_libm::atan2(div(ly, b), div(lx, a)),
            AngleUnit::Radians,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eda_angle::ANGLE_90;

    #[test]
    fn defaults_and_axis_aligned_points_are_exact() -> Result<(), EllipseError> {
        let e = Ellipse::<i32>::new(Vector2::new(10, -20), 100, 50, ANGLE_0, ArcAngles::Defaults);
        assert_eq!((e.start_angle, e.end_angle), (ANGLE_0, FULL_CIRCLE));
        assert_eq!(e.point_at_angle(ANGLE_0)?, Vector2::new(110, -20));
        assert_eq!(e.arc_start_point()?, Vector2::new(110, -20));
        assert_eq!(e.subtended_angle(), ANGLE_0);
        let e = Ellipse::<i32>::new(
            Vector2::new(0, 0),
            100,
            50,
            ANGLE_90,
            ArcAngles::Start(ANGLE_90),
        );
        assert_eq!(e.subtended_angle(), EdaAngle::degrees(270.0));
        Ok(())
    }

    #[test]
    fn major_axis_form_rounds_norm_and_refuses_undefined_minor() -> Result<(), EllipseError> {
        let e = Ellipse::<i32>::from_major_axis(
            Vector2::new(0, 0),
            Vector2::new(3, 4),
            0.5,
            ArcAngles::Defaults,
        )?;
        assert_eq!((e.major_radius, e.minor_radius), (5, 2));
        assert_eq!(
            Ellipse::<i32>::from_major_axis(
                Vector2::new(0, 0),
                Vector2::new(3, 4),
                f64::NAN,
                ArcAngles::Defaults
            )
            .map(|_| ()),
            Err(EllipseError::UndefinedConversion)
        );
        assert!(matches!(
            Ellipse::<f64>::from_major_axis(
                Vector2::new(0.0, 0.0),
                Vector2::new(3.0, 4.0),
                0.5,
                ArcAngles::Defaults
            ),
            Err(EllipseError::Unported {
                unit: "libs.kimath.include.math.application.ui.p002",
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn mirror_swaps_and_negates_angles_and_refuses_overflow() -> Result<(), EllipseError> {
        let mut e = Ellipse::<i32>::new(
            Vector2::new(3, 4),
            10,
            5,
            ANGLE_90,
            ArcAngles::StartEnd(EdaAngle::degrees(10.0), EdaAngle::degrees(30.0)),
        );
        e.mirror(Vector2::new(1, 1), FlipDirection::LeftRight)?;
        assert_eq!(e.center, Vector2::new(-1, 4));
        assert_eq!(e.rotation, EdaAngle::degrees(-90.0));
        assert_eq!(
            (e.start_angle, e.end_angle),
            (EdaAngle::degrees(150.0), EdaAngle::degrees(170.0))
        );
        let before = e;
        assert_eq!(
            e.mirror(Vector2::new(0, i32::MAX), FlipDirection::TopBottom),
            Err(EllipseError::IntegerOverflow)
        );
        assert_eq!(e.center, before.center);
        assert_eq!(e.rotation, before.rotation);
        Ok(())
    }

    #[test]
    fn angle_at_point_refuses_nan_radius_only() {
        let mut e = Ellipse::<f64>::new(
            Vector2::new(0.0, 0.0),
            f64::NAN,
            1.0,
            ANGLE_0,
            ArcAngles::Defaults,
        );
        assert_eq!(
            e.angle_at_point(Vector2::new(1.0, 1.0)),
            Err(EllipseError::UndefinedComparison)
        );
        e.major_radius = 2.0;
        assert_eq!(
            e.angle_at_point(Vector2::new(0.0, 1.0)),
            Ok(EdaAngle::new(
                glibc_libm::atan2(1.0, 0.0),
                AngleUnit::Radians
            ))
        );
    }
}
