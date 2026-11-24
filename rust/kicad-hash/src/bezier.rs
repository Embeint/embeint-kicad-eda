// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kimath/include/bezier_curves.h`: the inline definitions this unit owns.
//!
//! - `BEZIER<T>( Start, C1, C2, End )` (`constexpr`, bezier_curves.h:95-99): [`Bezier::new`];
//! - `BEZIER<T>::PointAt( double )` (:110-120): [`Bezier::point_at`];
//! - `BEZIER<T>::Split( double, BEZIER&, BEZIER& )` (:129-140): [`Bezier::split`];
//! - `BEZIER<T>::SubCurve( double, double )` (:145-191): [`Bezier::sub_curve`];
//! - `BEZIER_POLY( const std::vector<VECTOR2D>& )` (:41-45): [`BezierPoly::from_vector2d`].
//!
//! KiCad instantiates `BEZIER` with `double` (`EDA_SHAPE`: `PointAt`, `SubCurve` and through
//! it `Split`) and `int` (`TransformEllipseToBeziers`, used by the Altium importer). Both are
//! ported generically through [`BezierCoordinate`], which supplies exactly the `VECTOR2`
//! operations each body calls, co-ported in `kicad_math::vector`:
//!
//! - `double * VECTOR2<T>` yields `VECTOR2<double>` for both component types;
//! - `VECTOR2<T> + VECTOR2<double>` yields `VECTOR2<double>`;
//! - `VECTOR2<T> - VECTOR2<T>` is an `int` subtraction for `T = int` (overflow refused);
//! - storing a `VECTOR2<double>` into a `VECTOR2<T>` is a copy for `double` and the clamping,
//!   truncating casting constructor for `int` (a NaN component refused).
//!
//! `BEZIER<double>` is evaluated with the operand order of the bound reference build, statement by
//! statement (private `bezier_f64_sites`): IEEE 754 leaves open which NaN an operation with two
//! NaN operands returns, and the compiled reference answers that per site. `BEZIER<int>` keeps the
//! generic bodies below: an `int` component converts to a non-NaN `double`, so every NaN of an
//! `int` evaluation comes from `aT` alone and is refused when stored, whatever the order.
//!
//! Every other `BEZIER_POLY` member (the `VECTOR2I` constructors, `GetPoly` and the private
//! helpers) and `TransformEllipseToBeziers` belong to
//! `libs.kimath.src.bezier_curves.application.ui.p001` and are not implemented here.

use kicad_math::{Vector2, Vector2D, VectorDomainError};

use crate::bezier_f64_sites::{self, PointAtSite};
use crate::sse2;

mod sealed {
    use super::{Bezier, PointAtSite, Vector2, VectorDomainError};

    /// The per-instantiation bodies behind the generic members (not nameable outside the crate).
    pub trait Sealed: Sized {
        /// `PointAt( aT )`, as compiled at `site`.
        fn point_at_at(
            curve: &Bezier<Self>,
            t: f64,
            site: PointAtSite,
        ) -> Result<Vector2<Self>, VectorDomainError>;
        /// `Split( aT, aLeft, aRight )`.
        fn split_at(
            curve: &Bezier<Self>,
            t: f64,
        ) -> Result<(Bezier<Self>, Bezier<Self>), VectorDomainError>;
    }

    impl Sealed for i32 {
        fn point_at_at(
            curve: &Bezier<i32>,
            t: f64,
            _site: PointAtSite,
        ) -> Result<Vector2<i32>, VectorDomainError> {
            super::generic_point_at(curve, t)
        }

        fn split_at(
            curve: &Bezier<i32>,
            t: f64,
        ) -> Result<(Bezier<i32>, Bezier<i32>), VectorDomainError> {
            super::generic_split(curve, t)
        }
    }

    impl Sealed for f64 {
        fn point_at_at(
            curve: &Bezier<f64>,
            t: f64,
            site: PointAtSite,
        ) -> Result<Vector2<f64>, VectorDomainError> {
            Ok(super::bezier_f64_sites::point_at(curve, t, site))
        }

        fn split_at(
            curve: &Bezier<f64>,
            t: f64,
        ) -> Result<(Bezier<f64>, Bezier<f64>), VectorDomainError> {
            Ok(super::bezier_f64_sites::split(curve, t))
        }
    }
}

/// A `BEZIER<NumericType>` component type KiCad instantiates: `int` (`i32`) or `double` (`f64`).
pub trait BezierCoordinate: Copy + sealed::Sealed {
    /// `aLhs - aRhs` in `VECTOR2<T>` (the common type of `T` with itself).
    fn difference(
        lhs: Vector2<Self>,
        rhs: Vector2<Self>,
    ) -> Result<Vector2<Self>, VectorDomainError>;
    /// `aScalar * aVector`, a `VECTOR2<double>`.
    fn scaled(scalar: f64, vector: Vector2<Self>) -> Vector2D;
    /// `aLhs + aRhs` with a `VECTOR2<double>` right operand, a `VECTOR2<double>`.
    fn offset(lhs: Vector2<Self>, rhs: Vector2D) -> Vector2D;
    /// A `VECTOR2<double>` initialising (or returned as) a `VECTOR2<T>`.
    fn store(vector: Vector2D) -> Result<Vector2<Self>, VectorDomainError>;
}

impl BezierCoordinate for i32 {
    fn difference(lhs: Vector2<i32>, rhs: Vector2<i32>) -> Result<Vector2<i32>, VectorDomainError> {
        lhs.checked_sub(rhs)
    }

    fn scaled(scalar: f64, vector: Vector2<i32>) -> Vector2D {
        scalar * vector
    }

    fn offset(lhs: Vector2<i32>, rhs: Vector2D) -> Vector2D {
        lhs + rhs
    }

    fn store(vector: Vector2D) -> Result<Vector2<i32>, VectorDomainError> {
        Vector2::<i32>::try_from(vector)
    }
}

impl BezierCoordinate for f64 {
    fn difference(lhs: Vector2D, rhs: Vector2D) -> Result<Vector2D, VectorDomainError> {
        Ok(lhs - rhs)
    }

    fn scaled(scalar: f64, vector: Vector2D) -> Vector2D {
        scalar * vector
    }

    fn offset(lhs: Vector2D, rhs: Vector2D) -> Vector2D {
        lhs + rhs
    }

    fn store(vector: Vector2D) -> Result<Vector2D, VectorDomainError> {
        // VECTOR2<double> from VECTOR2<double> is the copy constructor.
        Ok(vector)
    }
}

/// `BEZIER<NumericType>`: a cubic Bezier curve with public control points.
///
/// `Default` is the defaulted `BEZIER()`, whose members are value-initialised `VECTOR2`s.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bezier<T> {
    pub start: Vector2<T>,
    pub c1: Vector2<T>,
    pub c2: Vector2<T>,
    pub end: Vector2<T>,
}

impl<T: BezierCoordinate> Bezier<T> {
    /// `BEZIER( aStart, aC1, aC2, aEnd )`: stores the four control points unchanged.
    pub const fn new(start: Vector2<T>, c1: Vector2<T>, c2: Vector2<T>, end: Vector2<T>) -> Self {
        Self { start, c1, c2, end }
    }

    /// `PointAt( double aT )`: the Bernstein form
    /// `( t_m1_3 * Start ) + ( 3.0 * aT * t_m1_2 * C1 ) + ( 3.0 * t2 * t_m1 * C2 ) + ( t3 * End )`,
    /// evaluated left to right in `double` and stored as a `VECTOR2<T>`. `aT` outside `[0, 1]`
    /// is evaluated as given. For `int`, a NaN component of the sum is refused. For `double`,
    /// NaN results follow the reference's inlined `point_at` site.
    pub fn point_at(&self, t: f64) -> Result<Vector2<T>, VectorDomainError> {
        T::point_at_at(self, t, PointAtSite::Inline)
    }

    /// `Split( double aT, BEZIER& aLeft, BEZIER& aRight )`: one de Casteljau step at `aT`,
    /// returning `( aLeft, aRight )`. Each intermediate point is `a + aT * ( b - a )`, stored as a
    /// `VECTOR2<T>` before it is used, in the C++ statement order. For `int`, an overflowing
    /// difference or a NaN intermediate is refused and neither output is produced.
    pub fn split(&self, t: f64) -> Result<(Self, Self), VectorDomainError> {
        T::split_at(self, t)
    }

    /// `SubCurve( double aT0, double aT1 )`: both parameters are clamped to `[0, 1]` (a NaN
    /// passes the clamp unchanged); an empty or reversed range gives the degenerate curve at
    /// `PointAt( t0 )`; the full range returns the curve itself; otherwise one or two `Split`
    /// calls extract the range, the second at `t0 / t1`.
    pub fn sub_curve(&self, t0: f64, t1: f64) -> Result<Self, VectorDomainError> {
        let t0 = clamp_unit(t0);
        let t1 = clamp_unit(t1);

        if t1 <= t0 {
            // The out-of-line PointAt; t0 is clamped and therefore never NaN here.
            let point = T::point_at_at(self, t0, PointAtSite::OutOfLine)?;
            return Ok(Self::new(point, point, point, point));
        }

        if t0 == 0.0 && t1 == 1.0 {
            return Ok(*self);
        }

        if t0 == 0.0 {
            let (left, _) = self.split(t1)?;
            return Ok(left);
        }

        if t1 == 1.0 {
            let (_, right) = self.split(t0)?;
            return Ok(right);
        }

        let (left, _) = self.split(t1)?;
        // KimathHeaderHash +0x25077: divsd t0, t1 (the left operand is always dst).
        let (_, sub_curve) = left.split(sse2::div(t0, t1))?;

        Ok(sub_curve)
    }
}

/// The generic `PointAt` body (used for `int`): scalars in C++ operand order, then the co-ported
/// `VECTOR2` operators. Every NaN here is a quieted `aT` or the default NaN, so the order of two
/// NaN operands cannot change a result, and the `int` store refuses any NaN anyway.
fn generic_point_at<T: BezierCoordinate>(
    curve: &Bezier<T>,
    t: f64,
) -> Result<Vector2<T>, VectorDomainError> {
    let t2 = sse2::mul(t, t);
    let t3 = sse2::mul(t2, t);
    let t_m1 = sse2::sub(1.0, t);
    let t_m1_2 = sse2::mul(t_m1, t_m1);
    let t_m1_3 = sse2::mul(t_m1_2, t_m1);

    let start = T::scaled(t_m1_3, curve.start);
    let c1 = T::scaled(sse2::mul(sse2::mul(3.0, t), t_m1_2), curve.c1);
    let c2 = T::scaled(sse2::mul(sse2::mul(3.0, t2), t_m1), curve.c2);
    let end = T::scaled(t3, curve.end);

    T::store(start + c1 + c2 + end)
}

/// The generic `Split` body (used for `int`).
fn generic_split<T: BezierCoordinate>(
    curve: &Bezier<T>,
    t: f64,
) -> Result<(Bezier<T>, Bezier<T>), VectorDomainError> {
    let start_c1 = lerp(curve.start, curve.c1, t)?;
    let c1_c2 = lerp(curve.c1, curve.c2, t)?;
    let c2_end = lerp(curve.c2, curve.end, t)?;
    let left_c2 = lerp(start_c1, c1_c2, t)?;
    let right_c1 = lerp(c1_c2, c2_end, t)?;
    let shared = lerp(left_c2, right_c1, t)?;

    Ok((
        Bezier::new(curve.start, start_c1, left_c2, shared),
        Bezier::new(shared, right_c1, c2_end, curve.end),
    ))
}

/// `if( t < 0.0 ) t = 0.0; else if( t > 1.0 ) t = 1.0;` (a NaN and -0.0 are kept). Spelled
/// out rather than `f64::clamp` so the comparison order is visibly the C++ one.
#[allow(clippy::manual_clamp)]
fn clamp_unit(t: f64) -> f64 {
    if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    }
}

/// `a + aT * ( b - a )` stored as a `VECTOR2<T>`.
fn lerp<T: BezierCoordinate>(
    a: Vector2<T>,
    b: Vector2<T>,
    t: f64,
) -> Result<Vector2<T>, VectorDomainError> {
    let difference = T::difference(b, a)?;
    T::store(T::offset(a, T::scaled(t, difference)))
}

/// `BEZIER_POLY`, as far as this unit owns it: the `std::vector<VECTOR2D>` constructor.
///
/// The private members are exposed read-only so the oracle can compare the constructed state.
#[derive(Clone, Debug)]
pub struct BezierPoly {
    min_seg_len: f64,
    ctrl_pts: Vec<Vector2D>,
}

impl BezierPoly {
    /// `BEZIER_POLY( const std::vector<VECTOR2D>& aControlPoints )`: copies the control points
    /// (any number, bits unchanged) into `m_ctrlPts` and sets `m_minSegLen = 0.0`.
    pub fn from_vector2d(control_points: &[Vector2D]) -> Self {
        Self {
            min_seg_len: 0.0,
            ctrl_pts: control_points.to_vec(),
        }
    }

    /// `m_minSegLen`.
    pub fn min_seg_len(&self) -> f64 {
        self.min_seg_len
    }

    /// `m_ctrlPts`.
    pub fn ctrl_pts(&self) -> &[Vector2D] {
        &self.ctrl_pts
    }
}
