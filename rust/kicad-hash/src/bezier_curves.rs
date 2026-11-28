// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/src/bezier_curves.cpp, copyright The KiCad Developers.
// Portions based on draw2d, Copyright (c) 2010, Laurent Le Goff (BSD-2-Clause); on BezierKit,
// Copyright (c) 2017 Holmes Futrell (MIT); and on spline-research, Copyright 2018 Raph Levien
// (Apache-2.0 or MIT), as the pinned source records. All are GPL-3.0-compatible.
//! `libs/kimath/src/bezier_curves.cpp` at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7: the
//! out-of-line bodies owned by unit `libs.kimath.src.bezier_curves.application.ui.p001`.
//!
//! | C++ definition | Rust |
//! | --- | --- |
//! | `BEZIER_POLY( const VECTOR2I& ×4 )` | [`BezierPoly::from_points`] |
//! | `BEZIER_POLY( const std::vector<VECTOR2I>& )` | [`BezierPoly::from_vector2i`] |
//! | `GetPoly( std::vector<VECTOR2I>&, int = 10 )` | [`BezierPoly::get_poly_i`] |
//! | `GetPoly( std::vector<VECTOR2D>&, double = 10.0 )` | [`BezierPoly::get_poly_d`] |
//! | private `isNaN`, `isFlat`, `eval`, `subdivide` | [`BezierPoly::is_nan`], [`BezierPoly::is_flat`], [`BezierPoly::eval`], [`BezierPoly::subdivide`] |
//! | private `numberOfInflectionPoints`, `findInflectionPoints` | [`BezierPoly::number_of_inflection_points`], [`BezierPoly::find_inflection_points`] |
//! | private `thirdControlPointDeviation` | [`BezierPoly::third_control_point_deviation`] |
//! | private `getQuadPoly`, `getCubicPoly`, `cubicParabolicApprox`, `recursiveSegmentation` | private, reached through `get_poly_*` |
//! | file-static `approx_int`, `approx_inv_int` | private |
//! | `TransformEllipseToBeziers<double>`, `<int>` | [`transform_ellipse_to_beziers`] |
//!
//! The types are the accepted ones: [`BezierPoly`] and [`Bezier`] of the header unit
//! `libs.kimath.include.application.pure.p001`, `ELLIPSE<T>` ([`Ellipse`]), `VECTOR2`,
//! `EDA_ANGLE`, `KiROUND` and the trigo.h `RotatePoint( VECTOR2D&, const EDA_ANGLE& )` from
//! `kicad_math`. `std::pow`, `std::hypot`, `std::tan` and GCC's `sincos` are the glibc 2.39
//! bodies of `kicad_math::reference_libm`; `std::sqrt` (`sqrtsd`) and the inlined `std::ceil`
//! are exact.
//!
//! The private helpers are public here so the oracle can compare them; in C++ only `GetPoly`
//! reaches them, always with three or four control points (and only four in the cubic helpers),
//! so their other branches (`isFlat` with three points, the four-point `eval`, the
//! three-point `subdivide` and every `wxASSERT`) are reached only through these entries.
//!
//! # NaN results
//!
//! Every `double` operation goes through the SSE2 rules of [`crate::sse2`] (`op(dst, src)`):
//! for `-` and `/` the C++ left operand is `dst`; for `+` and `*` the operand GCC 13.3 placed in
//! the destination register decides which of two NaN operands survives. The out-of-line
//! `eval`, `subdivide` and `thirdControlPointDeviation` and `TransformEllipseToBeziers<double>`
//! follow the compiled order of the reference object (`port/findings/bezier-curves-port.md`
//! lists every site). The bodies `GetPoly` inlines need no such choice: there no two distinct
//! NaNs can meet in an observable value. With finite control points every NaN of the
//! three-point path is the default NaN `0xfff8_0000_0000_0000` (a non-finite control point makes
//! `n` undefined), and the cubic path only outputs copied control points or points that passed
//! `isFlat`, which a NaN never does. They use the out-of-line order too.
//!
//! # Refusals and bounds
//!
//! [`BezierError::Undefined`]: getQuadPoly's `int n = std::ceil( ... )` and
//! TransformEllipseToBeziers' `static_cast<int>( std::ceil( ... ) )` of a NaN or out-of-range
//! value ([conv.fpint]); for `ELLIPSE<int>` a NaN point component converted by `VECTOR2<int>`'s
//! casting constructor (checked, though unreachable: `RotatePoint` folds a NaN angle to zero and
//! every other value is finite); a private member whose subscripts leave its vectors; three-point
//! `isFlat`'s `std::max`/`std::min` of a NaN ([alg.min.max]).
//!
//! [`BezierError::LimitExceeded`]: the loops of `GetPoly` (getQuadPoly's `n` iterations and one
//! per iteration of recursiveSegmentation's and cubicParabolicApprox's loops) are counted
//! against a caller-supplied step limit, and `TransformEllipseToBeziers`' curve count against a
//! curve limit. A NaN control point can make recursiveSegmentation subdivide until allocation
//! fails, and a tiny error makes it run for an impractically long time; the C++ has no such
//! bound, so the limit stands for exhausting the host's resources. The counting is exact, so a
//! host that passes the oracle's limits gets its refusals.
//!
//! # Assertions
//!
//! `wxASSERT( false )` calls the wx assertion handler, a host effect. [`WxAssert`] names the
//! site; the production host has none and defers as `wxOnAssert` (this unit owns the macro's
//! expansion). A host that returns lets the body continue as C++ does.

use crate::bezier::{Bezier, BezierPoly};
use crate::sse2::{add, div, mul, sub};
use kicad_math::eda_angle::{AngleUnit, EdaAngle, ANGLE_0, ANGLE_360};
use kicad_math::geometry_headers::Ellipse;
use kicad_math::{reference_libm, trigo_header, util};
use kicad_math::{Vector2, Vector2D, Vector2I, VectorDomainError};

/// Short name for the point constructor.
use kicad_math::Vector2 as V;

/// This unit.
pub const UNIT: &str = "libs.kimath.src.bezier_curves.application.ui.p001";

/// The upstream symbol of the host assertion effect of a failed `wxASSERT`.
pub const WX_ON_ASSERT: &str = "wxOnAssert";

/// `GetPoly( std::vector<VECTOR2I>&, int aMaxError = 10 )`'s default argument.
pub const DEFAULT_MAX_ERROR_I: i32 = 10;

/// `GetPoly( std::vector<VECTOR2D>&, double aMaxError = 10.0 )`'s default argument.
pub const DEFAULT_MAX_ERROR_D: f64 = 10.0;

/// Why a body produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BezierError {
    /// The pinned C++ evaluation is undefined for this input.
    Undefined,
    /// The evaluation needs more loop iterations or curves than the caller's limit.
    LimitExceeded,
    /// A callee owned by another unit, or the host assertion effect, is not ported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

/// The result of a body.
pub type BezierResult<T> = Result<T, BezierError>;

/// One `wxASSERT( false )` site: the `wxOnAssert( __FILE__, __LINE__, __WXFUNCTION__, "false",
/// NULL )` call it makes when `wxTheAssertHandler` is set (wx 3.2 `wxASSERT_MSG_AT`). The file is
/// the compiler's spelling of the bezier_curves.cpp path, left to the host; the message is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WxAssert {
    /// `__LINE__` in the pinned bezier_curves.cpp.
    pub line: u32,
    /// `__WXFUNCTION__` (`__FUNCTION__`).
    pub function: &'static str,
}

impl WxAssert {
    /// `#cond`.
    pub const CONDITION: &'static str = "false";
}

/// bezier_curves.cpp:154, `isFlat` with neither three nor four control points.
pub const WX_ASSERT_IS_FLAT: WxAssert = WxAssert {
    line: 154,
    function: "isFlat",
};
/// bezier_curves.cpp:205, `eval`.
pub const WX_ASSERT_EVAL: WxAssert = WxAssert {
    line: 205,
    function: "eval",
};
/// bezier_curves.cpp:321, `subdivide`.
pub const WX_ASSERT_SUBDIVIDE: WxAssert = WxAssert {
    line: 321,
    function: "subdivide",
};
/// bezier_curves.cpp:600, `GetPoly( std::vector<VECTOR2D>&, double )`.
pub const WX_ASSERT_GET_POLY: WxAssert = WxAssert {
    line: 600,
    function: "GetPoly",
};

/// The host effects the bodies call. [`Pinned`] is the only production implementation; tests
/// supply hosts whose assertion handler returns, to exercise the continuations.
pub(crate) trait Host {
    /// The host effect of a failed `wxASSERT`: `wxOnAssert( ... )`, then `wxTrap()` if the
    /// handler set `wxTrapInAssert`. `Ok` means that effect completed and the body continues.
    fn wx_assert(&self, site: &WxAssert) -> BezierResult<()>;
}

/// The production host: the wx assertion handler is not ported.
pub(crate) struct Pinned;

impl Host for Pinned {
    fn wx_assert(&self, _site: &WxAssert) -> BezierResult<()> {
        // The handler belongs to the wx host, which no unit ports; the macro's expansion is this
        // unit's own body, so the deferral names this unit.
        // UNPORTED(libs.kimath.src.bezier_curves.application.ui.p001): wxOnAssert
        Err(BezierError::Unported {
            upstream: WX_ON_ASSERT,
            unit: UNIT,
        })
    }
}

/// The loop iterations one `GetPoly` call may still run.
struct Steps {
    used: u64,
    limit: u64,
}

impl Steps {
    fn new(limit: u64) -> Self {
        Self { used: 0, limit }
    }

    fn add(&mut self, count: u64) -> BezierResult<()> {
        if count > self.limit - self.used {
            return Err(BezierError::LimitExceeded);
        }
        self.used += count;
        Ok(())
    }
}

/// `std::sqrt` as `sqrtsd`: exact; a NaN operand is returned quieted, a negative one gives the
/// default NaN.
fn sqrt(x: f64) -> f64 {
    if x.is_nan() {
        return f64::from_bits(x.to_bits() | 0x0008_0000_0000_0000);
    }
    let root = x.sqrt();
    if root.is_nan() {
        f64::from_bits(crate::sse2::DEFAULT_NAN_BITS)
    } else {
        root
    }
}

/// `VECTOR2<double>( a ) - b`, componentwise `subsd`/`subpd` (`a` the destination).
fn vsub(a: Vector2D, b: Vector2D) -> Vector2D {
    V::new(sub(a.x, b.x), sub(a.y, b.y))
}

/// `a + aT * ( b - a )` as the reference's out-of-line `subdivide` compiles it: the difference
/// is the destination of the product and the product the destination of the sum.
fn lerp(a: Vector2D, b: Vector2D, t: f64) -> Vector2D {
    let d = vsub(b, a);
    V::new(add(mul(d.x, t), a.x), add(mul(d.y, t), a.y))
}

/// `VECTOR2<double>::Cross`: `x * b.y - y * b.x`.
fn cross(a: Vector2D, b: Vector2D) -> f64 {
    sub(mul(a.x, b.y), mul(a.y, b.x))
}

/// `VECTOR2<double>::Dot`: `x * b.x + y * b.y`.
fn dot(a: Vector2D, b: Vector2D) -> f64 {
    add(mul(a.x, b.x), mul(a.y, b.y))
}

/// `VECTOR2<double>::SquaredEuclideanNorm`: `x * x + y * y`.
fn squared_norm(a: Vector2D) -> f64 {
    add(mul(a.x, a.x), mul(a.y, a.y))
}

/// `VECTOR2<double>::EuclideanNorm` (vector2d.h:281-305), floating branch.
fn euclidean_norm(a: Vector2D) -> f64 {
    if a.x.abs() == a.y.abs() {
        return mul(a.x.abs(), core::f64::consts::SQRT_2);
    }
    if a.x == 0.0 {
        return a.y.abs();
    }
    if a.y == 0.0 {
        return a.x.abs();
    }
    reference_libm::hypot(a.x, a.y)
}

/// `VECTOR2<double>::operator==`: `( b.x == x ) && ( b.y == y )`.
fn equal(a: Vector2D, b: Vector2D) -> bool {
    b.x == a.x && b.y == a.y
}

/// `VECTOR2D( const VECTOR2I& )`, the casting constructor's floating branch: exact.
fn to_d(p: Vector2I) -> Vector2D {
    V::new(f64::from(p.x), f64::from(p.y))
}

/// The four zero points of `BEZIER_POLY( std::vector<VECTOR2D>( 4 ) )`.
fn zeros4() -> BezierPoly {
    BezierPoly::from_vector2d(&[V::new(0.0, 0.0); 4])
}

/// `approx_int` (file static).
fn approx_int(x: f64) -> f64 {
    const D: f64 = 0.6744897501960817;
    // `d * d * d * d`, folded by GCC with the same left-to-right rounding.
    let d4 = mul(mul(mul(D, D), D), D);
    div(
        x,
        add(
            sub(1.0, D),
            reference_libm::pow(add(d4, mul(mul(x, x), 0.25)), 0.25),
        ),
    )
}

/// `approx_inv_int` (file static).
fn approx_inv_int(x: f64) -> f64 {
    const P: f64 = 0.39538816;
    mul(
        x,
        add(sub(1.0, P), sqrt(add(mul(P, P), mul(mul(0.25, x), x)))),
    )
}

/// Whether an `int` can hold the integral `double` `std::ceil` returned ([conv.fpint]).
fn ceil_to_int(value: f64) -> BezierResult<i32> {
    // A NaN fails both comparisons.
    if value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) {
        Ok(value as i32)
    } else {
        Err(BezierError::Undefined)
    }
}

impl BezierPoly {
    /// `BEZIER_POLY( aStart, aCtrl1, aCtrl2, aEnd )`: the four points as `VECTOR2D`, and
    /// `m_minSegLen = 0.0`.
    pub fn from_points(start: Vector2I, ctrl1: Vector2I, ctrl2: Vector2I, end: Vector2I) -> Self {
        Self {
            min_seg_len: 0.0,
            ctrl_pts: vec![to_d(start), to_d(ctrl1), to_d(ctrl2), to_d(end)],
        }
    }

    /// `BEZIER_POLY( const std::vector<VECTOR2I>& )`: every point as `VECTOR2D`, and
    /// `m_minSegLen = 0.0`.
    pub fn from_vector2i(control_points: &[Vector2I]) -> Self {
        Self {
            min_seg_len: 0.0,
            ctrl_pts: control_points.iter().map(|p| to_d(*p)).collect(),
        }
    }

    /// The control point `index`, or [`BezierError::Undefined`] for a subscript `m_ctrlPts`
    /// does not hold.
    fn at(&self, index: usize) -> BezierResult<Vector2D> {
        self.ctrl_pts
            .get(index)
            .copied()
            .ok_or(BezierError::Undefined)
    }

    /// `isNaN()`: whether any control point component is a NaN.
    pub fn is_nan(&self) -> bool {
        self.ctrl_pts.iter().any(|p| p.x.is_nan() || p.y.is_nan())
    }

    /// `isFlat( aMaxError )`. A three-point curve compares the control point's distance from
    /// its projection on the chord (`std::max`/`std::min` of a NaN parameter is refused); a
    /// four-point curve compares both control points' squared distances, scaled by 3/4 or 4/9,
    /// with `aMaxError` squared. Any other count asserts and is not flat.
    pub fn is_flat(&self, max_error: f64) -> BezierResult<bool> {
        self.is_flat_with(&Pinned, max_error)
    }

    pub(crate) fn is_flat_with(&self, host: &impl Host, max_error: f64) -> BezierResult<bool> {
        let p = &self.ctrl_pts;
        if p.len() == 3 {
            let d21 = vsub(p[1], p[0]);
            let d31 = vsub(p[2], p[0]);

            let t = div(dot(d21, d31), squared_norm(d31));
            if t.is_nan() {
                // [alg.min.max]: a NaN is not Cpp17LessThanComparable.
                return Err(BezierError::Undefined);
            }
            // std::max( t, 0.0 ) is `t < 0.0 ? 0.0 : t`; std::min( m, 1.0 ) is
            // `1.0 < m ? 1.0 : m`.
            let m = if t < 0.0 { 0.0 } else { t };
            let u = if 1.0 < m { 1.0 } else { m };
            let q = V::new(add(p[0].x, mul(u, d31.x)), add(p[0].y, mul(u, d31.y)));
            let delta = vsub(p[1], q);

            return Ok(mul(0.5, euclidean_norm(delta)) <= max_error);
        } else if p.len() == 4 {
            let delta = vsub(p[3], p[0]);

            let d21 = vsub(p[1], p[0]);
            let d31 = vsub(p[2], p[0]);

            let cross1 = cross(delta, d21);
            let cross2 = cross(delta, d31);

            let inv_delta_sq = div(1.0, squared_norm(delta));
            let d1 = mul(mul(cross1, cross1), inv_delta_sq);
            let d2 = mul(mul(cross2, cross2), inv_delta_sq);

            let factor = if mul(cross1, cross2) > 0.0 {
                3.0 / 4.0
            } else {
                4.0 / 9.0
            };
            let f2 = mul(factor, factor);
            let tol = mul(max_error, max_error);

            return Ok(mul(d1, f2) <= tol && mul(d2, f2) <= tol);
        }

        host.wx_assert(&WX_ASSERT_IS_FLAT)?;
        Ok(false)
    }

    /// `eval( t )`: the Bernstein form of a three- or four-point curve, in the reference's
    /// compiled order (`omt2 * P0` with `P0` the destination, `2.0 * omt` as `omt + omt`; for
    /// four points the `P1` term is the destination of the first sum). Any other count asserts
    /// and returns `( 0, 0 )`.
    pub fn eval(&self, t: f64) -> BezierResult<Vector2D> {
        self.eval_with(&Pinned, t)
    }

    pub(crate) fn eval_with(&self, host: &impl Host, t: f64) -> BezierResult<Vector2D> {
        let p = &self.ctrl_pts;
        // BEZIER_POLY::eval +0x4..+0x26: subsd t, 1.0; mulsd omt, omt.
        let omt = sub(1.0, t);
        let omt2 = mul(omt, omt);

        if p.len() == 3 {
            // +0x118..+0x15a: addsd omt, omt; mulsd t, t; mulsd t, 2omt; mulpd omt2, P0;
            // mulpd P1, c1; addpd; mulpd t2, P2; addpd.
            let c1 = mul(add(omt, omt), t);
            let t2 = mul(t, t);
            let term = |a: f64, b: f64, c: f64| add(add(mul(a, omt2), mul(c1, b)), mul(c, t2));
            return Ok(V::new(
                term(p[0].x, p[1].x, p[2].x),
                term(p[0].y, p[1].y, p[2].y),
            ));
        } else if p.len() == 4 {
            // +0x98..+0x10f: mulsd t, t (t2); mulsd 3.0, t; mulsd omt, omt2 (omt3); mulsd t2, t
            // (t3); mulsd 3.0, t2; mulsd omt2, 3t; mulsd omt, 3t2; mulpd P0, omt3; mulpd P1, a;
            // addpd term0, term1; mulpd b, P2; addpd; mulpd t3, P3; addpd.
            let t2 = mul(t, t);
            let a = mul(mul(t, 3.0), omt2);
            let omt3 = mul(omt2, omt);
            let t3 = mul(t, t2);
            let b = mul(mul(t2, 3.0), omt);
            let term = |p0: f64, p1: f64, p2: f64, p3: f64| {
                let sum = add(mul(a, p1), mul(omt3, p0));
                let sum = add(sum, mul(p2, b));
                add(sum, mul(p3, t3))
            };
            return Ok(V::new(
                term(p[0].x, p[1].x, p[2].x, p[3].x),
                term(p[0].y, p[1].y, p[2].y, p[3].y),
            ));
        }

        host.wx_assert(&WX_ASSERT_EVAL)?;
        Ok(V::new(0.0, 0.0))
    }

    /// `subdivide( aT, aLeft, aRight )`: one de Casteljau step at `aT` into `aLeft` and
    /// `aRight`, in the reference's compiled order (each `a + aT * ( b - a )` with the difference
    /// the product's destination, except `shared`, where `aT` is). A three-point curve's middle
    /// point is `eval( aT )`. A curve of three or four points needs that many points in both
    /// outputs (the C++ subscripts are unchecked: fewer is refused as undefined, before any
    /// write). Any other count asserts and writes nothing.
    pub fn subdivide(
        &self,
        t: f64,
        left: &mut BezierPoly,
        right: &mut BezierPoly,
    ) -> BezierResult<()> {
        self.subdivide_with(&Pinned, t, left, right)
    }

    pub(crate) fn subdivide_with(
        &self,
        host: &impl Host,
        t: f64,
        left: &mut BezierPoly,
        right: &mut BezierPoly,
    ) -> BezierResult<()> {
        let p = &self.ctrl_pts;
        let size = p.len();
        if (size == 3 || size == 4) && (left.ctrl_pts.len() < size || right.ctrl_pts.len() < size) {
            return Err(BezierError::Undefined);
        }

        if size == 3 {
            // +0x140..+0x1d8: aLeft[0] = P0; aLeft[1] = ( P1 - P0 ) * t + P0; aLeft[2] = the
            // out-of-line eval; aRight[2] = P2; aRight[1] = ( P2 - P1 ) * t + P1;
            // aRight[0] = aLeft[2].
            let l = &mut left.ctrl_pts;
            l[0] = p[0];
            l[1] = lerp(p[0], p[1], t);
            l[2] = self.eval_with(host, t)?;
            let r = &mut right.ctrl_pts;
            r[2] = p[2];
            r[1] = lerp(p[1], p[2], t);
            r[0] = l[2];
        } else if size == 4 {
            // +0xa0..+0x13b.
            let left_ctrl1 = lerp(p[0], p[1], t);
            let tmp = lerp(p[1], p[2], t);
            let left_ctrl2 = lerp(left_ctrl1, tmp, t);
            let right_ctrl2 = lerp(p[2], p[3], t);
            let right_ctrl1 = lerp(tmp, right_ctrl2, t);
            // mulpd diff, t (t the destination); addpd lc2, prod.
            let d = vsub(right_ctrl1, left_ctrl2);
            let shared = V::new(
                add(mul(t, d.x), left_ctrl2.x),
                add(mul(t, d.y), left_ctrl2.y),
            );

            let l = &mut left.ctrl_pts;
            l[0] = p[0];
            l[1] = left_ctrl1;
            l[2] = left_ctrl2;
            l[3] = shared;

            let r = &mut right.ctrl_pts;
            r[3] = p[3];
            r[2] = right_ctrl2;
            r[1] = right_ctrl1;
            r[0] = shared;
        } else {
            host.wx_assert(&WX_ASSERT_SUBDIVIDE)?;
        }

        Ok(())
    }

    /// `numberOfInflectionPoints()`: 1, 0 or -1 (the rare case of two or none). Needs four
    /// control points.
    pub fn number_of_inflection_points(&self) -> BezierResult<i32> {
        let (p0, p1, p2, p3) = (self.at(0)?, self.at(1)?, self.at(2)?, self.at(3)?);
        let d21 = vsub(p1, p0);
        let d32 = vsub(p2, p1);
        let d43 = vsub(p3, p2);

        let cross1 = mul(cross(d21, d32), cross(d32, d43));
        let cross2 = mul(cross(d21, d32), cross(d21, d43));

        if cross1 < 0.0 {
            return Ok(1);
        } else if cross2 > 0.0 {
            return Ok(0);
        }

        let b1 = dot(d21, d32) > 0.0;
        let b2 = dot(d32, d43) > 0.0;

        if b1 ^ b2 {
            return Ok(0);
        }

        // wxLogTrace( BEZIER_DBG, ... ): no trace mask is enabled, so no effect.
        Ok(-1)
    }

    /// `findInflectionPoints( aT1, aT2 )`: the roots in `(0, 1)` of the curvature numerator
    /// `a t^2 + b t + c`, in ascending order; 2, 1 or 0. Both outputs are set to `0.0` first.
    /// Needs four control points.
    pub fn find_inflection_points(&self, t1_out: &mut f64, t2_out: &mut f64) -> BezierResult<i32> {
        let (p0, p1, p2, p3) = (self.at(0)?, self.at(1)?, self.at(2)?, self.at(3)?);
        let coefficient = |p0: f64, p1: f64, p2: f64, p3: f64| {
            (
                add(sub(add(-p0, mul(3.0, p1)), mul(3.0, p2)), p3),
                add(sub(mul(3.0, p0), mul(6.0, p1)), mul(3.0, p2)),
                add(mul(-3.0, p0), mul(3.0, p1)),
            )
        };
        let (ax, bx, cx) = coefficient(p0.x, p1.x, p2.x, p3.x);
        let (ay, by, cy) = coefficient(p0.y, p1.y, p2.y, p3.y);
        let big_a = V::new(ax, ay);
        let big_b = V::new(bx, by);
        let big_c = V::new(cx, cy);

        let a = mul(3.0, cross(big_a, big_b));
        let b = mul(3.0, cross(big_a, big_c));
        let c = cross(big_b, big_c);

        // Solve the quadratic equation a*t^2 + b*t + c = 0
        let r2 = sub(mul(b, b), mul(mul(4.0, a), c));

        *t1_out = 0.0;
        *t2_out = 0.0;

        if r2 >= 0.0 && a != 0.0 {
            let r = sqrt(r2);

            let mut t1 = div(add(-b, r), mul(2.0, a));
            let mut t2 = div(sub(-b, r), mul(2.0, a));

            if (t1 > 0.0 && t1 < 1.0) && (t2 > 0.0 && t2 < 1.0) {
                if t1 > t2 {
                    core::mem::swap(&mut t1, &mut t2);
                }

                *t1_out = t1;
                *t2_out = t2;

                if sub(t2, t1) > 0.00001 {
                    return Ok(2);
                } else {
                    return Ok(1);
                }
            } else if t1 > 0.0 && t1 < 1.0 {
                *t1_out = t1;
                return Ok(1);
            } else if t2 > 0.0 && t2 < 1.0 {
                *t1_out = t2;
                return Ok(1);
            }

            return Ok(0);
        }

        Ok(0)
    }

    /// `thirdControlPointDeviation()`: the distance of the third control point from the line
    /// through the first two, or `0.0` when those are closer than `1e-3`. Needs three control
    /// points. In the reference's compiled order (`y * y + x * x` for the squared length,
    /// `s * p2.y + r * p2.x` for the first sum).
    pub fn third_control_point_deviation(&self) -> BezierResult<f64> {
        let (p0, p1, p2) = (self.at(0)?, self.at(1)?, self.at(2)?);
        // +0x23..+0x3f: subsd p0.y, p1.y; subsd p0.x, p1.x; mulsd dx, dx; mulsd dy, dy;
        // addsd dx2, dy2.
        let dy = sub(p1.y, p0.y);
        let dx = sub(p1.x, p0.x);
        let len_sq = add(mul(dy, dy), mul(dx, dx));

        if len_sq < 1e-6 {
            return Ok(0.0);
        }

        // +0x50..+0x86.
        let len = sqrt(len_sq);
        let r = div(dy, len);
        let s = div(sub(p0.x, p1.x), len);
        let u = div(sub(mul(p1.x, p0.y), mul(p0.x, p1.y)), len);

        Ok(add(add(mul(s, p2.y), mul(r, p2.x)), u).abs())
    }

    /// `recursiveSegmentation( aOutput, aThreshhold )`: depth-first halving until each piece is
    /// flat (its end point is appended) or of zero length (dropped).
    fn recursive_segmentation(
        &self,
        host: &impl Host,
        output: &mut Vec<Vector2D>,
        threshold: f64,
        steps: &mut Steps,
    ) -> BezierResult<()> {
        let mut left = zeros4();
        let mut right = zeros4();
        let mut stack: Vec<BezierPoly> = vec![self.clone()];

        while let Some(bezier) = stack.last_mut() {
            steps.add(1)?;

            if equal(bezier.at(3)?, bezier.at(0)?) {
                // wxLogTrace: dropping zero length segment.
                stack.pop();
            } else if bezier.is_flat_with(host, threshold)? {
                output.push(bezier.at(3)?);
                stack.pop();
            } else {
                bezier.subdivide_with(host, 0.5, &mut left, &mut right)?;
                *bezier = right.clone();
                stack.push(left.clone());
            }
        }

        Ok(())
    }

    /// `cubicParabolicApprox( aOutput, aMaxError )` (Hain et al.): repeatedly split off the
    /// first parabola-like piece at `t = 2 sqrt( aMaxError / ( 3 d ) )`.
    fn cubic_parabolic_approx(
        &self,
        host: &impl Host,
        output: &mut Vec<Vector2D>,
        max_error: f64,
        steps: &mut Steps,
    ) -> BezierResult<()> {
        // `stack` holds four curves; `c` starts at `this`, then alternates between stack[1] and
        // stack[3]; b1/b2 are stack[0]/[1] or stack[2]/[3].
        let mut stack = [zeros4(), zeros4(), zeros4(), zeros4()];
        let mut c: Option<usize> = None;
        let (mut b1, mut b2) = (0usize, 1usize);

        loop {
            steps.add(1)?;
            let curve = match c {
                None => self.clone(),
                Some(index) => stack[index].clone(),
            };

            if curve.is_nan() {
                break;
            }

            if curve.is_flat_with(host, max_error)? {
                output.push(curve.at(3)?);
                break;
            }

            let d = curve.third_control_point_deviation()?;

            if d == 0.0 {
                curve.recursive_segmentation(host, output, max_error, steps)?;
                break;
            }

            // Forumla 2 in Hain et al.
            let t = mul(2.0, sqrt(div(max_error, mul(3.0, d))));

            if t > 1.0 {
                curve.recursive_segmentation(host, output, max_error, steps)?;
                break;
            }

            let (mut first, mut second) = (stack[b1].clone(), stack[b2].clone());
            curve.subdivide_with(host, t, &mut first, &mut second)?;
            stack[b1] = first;
            stack[b2] = second;

            if stack[b1].is_flat_with(host, max_error)? {
                output.push(stack[b1].at(3)?);
            } else {
                stack[b1].recursive_segmentation(host, output, max_error, steps)?;
            }

            c = Some(b2);

            if b1 == 0 {
                (b1, b2) = (2, 3);
            } else {
                (b1, b2) = (0, 1);
            }
        }

        Ok(())
    }

    /// `getCubicPoly( aOutput, aMaxError )`.
    fn get_cubic_poly(
        &self,
        host: &impl Host,
        output: &mut Vec<Vector2D>,
        max_error: f64,
        steps: &mut Steps,
    ) -> BezierResult<()> {
        output.push(self.at(0)?);

        if self.number_of_inflection_points()? == 0 {
            // If no inflection points then apply PA on the full Bezier segment.
            return self.cubic_parabolic_approx(host, output, max_error, steps);
        }

        // If one or more inflection points then we will have to subdivide the curve
        let (mut t1, mut t2) = (0.0, 0.0);
        let mut num_of_ifp = self.find_inflection_points(&mut t1, &mut t2)?;

        if num_of_ifp == 2 {
            let mut sub1 = zeros4();
            let mut tmp1 = zeros4();
            let mut sub2 = zeros4();
            let mut sub3 = zeros4();

            self.subdivide_with(host, t1, &mut sub1, &mut tmp1)?;

            // Use PA for first subsegment
            sub1.cubic_parabolic_approx(host, output, max_error, steps)?;

            // Now find the second inflection point in the second curve and subdivide
            num_of_ifp = tmp1.find_inflection_points(&mut t1, &mut t2)?;

            if num_of_ifp == 2 || num_of_ifp == 1 {
                tmp1.subdivide_with(host, t1, &mut sub2, &mut sub3)?;
            } else {
                output.push(tmp1.at(3)?);
                return Ok(());
            }

            // Use Segment for the second (middle) subsegment
            sub2.recursive_segmentation(host, output, max_error, steps)?;

            // Use PA for the third curve
            sub3.cubic_parabolic_approx(host, output, max_error, steps)?;
        } else if num_of_ifp == 1 {
            let mut sub1 = zeros4();
            let mut sub2 = zeros4();
            self.subdivide_with(host, t1, &mut sub1, &mut sub2)?;
            sub1.cubic_parabolic_approx(host, output, max_error, steps)?;
            sub2.cubic_parabolic_approx(host, output, max_error, steps)?;
        } else {
            // Case where there is no inflection, use PA directly
            self.cubic_parabolic_approx(host, output, max_error, steps)?;
        }

        Ok(())
    }

    /// `getQuadPoly( aOutput, aMaxError )` (Levien's parabola flattening).
    fn get_quad_poly(
        &self,
        host: &impl Host,
        output: &mut Vec<Vector2D>,
        max_error: f64,
        steps: &mut Steps,
    ) -> BezierResult<()> {
        let (p0, p1, p2) = (self.at(0)?, self.at(1)?, self.at(2)?);
        let ddx = sub(sub(mul(2.0, p1.x), p0.x), p2.x);
        let ddy = sub(sub(mul(2.0, p1.y), p0.y), p2.y);
        let u0 = add(mul(sub(p1.x, p0.x), ddx), mul(sub(p1.y, p0.y), ddy));
        let u2 = add(mul(sub(p2.x, p1.x), ddx), mul(sub(p2.y, p1.y), ddy));
        let cross = sub(mul(sub(p2.x, p0.x), ddy), mul(sub(p2.y, p0.y), ddx));
        let x0 = div(u0, cross);
        let x2 = div(u2, cross);
        let scale = div(
            cross.abs(),
            mul(reference_libm::hypot(ddx, ddy), sub(x2, x0).abs()),
        );

        let a0 = approx_int(x0);
        let a2 = approx_int(x2);

        let n = ceil_to_int(mul(mul(0.5, sub(a2, a0).abs()), sqrt(div(scale, max_error))).ceil())?;

        let v0 = approx_inv_int(a0);
        let v2 = approx_inv_int(a2);

        output.push(p0);

        if n > 0 {
            steps.add(n as u64)?;
        }

        for ii in 0..n {
            let u = approx_inv_int(add(a0, div(mul(sub(a2, a0), f64::from(ii)), f64::from(n))));
            let t = div(sub(u, v0), sub(v2, v0));
            output.push(self.eval_with(host, t)?);
        }

        output.push(p2);
        Ok(())
    }

    /// `GetPoly( std::vector<VECTOR2D>& aOutput, double aMaxError )`: appends the polyline of a
    /// three- or four-point curve to `output` (an error `<= 0` becomes 10). Any other count
    /// asserts and appends nothing. `step_limit` bounds the loop iterations
    /// ([`BezierError::LimitExceeded`]); the C++ default argument is
    /// [`DEFAULT_MAX_ERROR_D`].
    pub fn get_poly_d(
        &self,
        output: &mut Vec<Vector2D>,
        max_error: f64,
        step_limit: u64,
    ) -> BezierResult<()> {
        self.get_poly_d_with(&Pinned, output, max_error, step_limit)
    }

    pub(crate) fn get_poly_d_with(
        &self,
        host: &impl Host,
        output: &mut Vec<Vector2D>,
        max_error: f64,
        step_limit: u64,
    ) -> BezierResult<()> {
        let max_error = if max_error <= 0.0 { 10.0 } else { max_error };
        let mut steps = Steps::new(step_limit);

        if self.ctrl_pts.len() == 3 {
            self.get_quad_poly(host, output, max_error, &mut steps)?;
        } else if self.ctrl_pts.len() == 4 {
            self.get_cubic_poly(host, output, max_error, &mut steps)?;
        } else {
            host.wx_assert(&WX_ASSERT_GET_POLY)?;
        }

        // wxLogTrace( BEZIER_DBG, "GetPoly generated %zu points", ... ): no effect.
        Ok(())
    }

    /// `GetPoly( std::vector<VECTOR2I>& aOutput, int aMaxError )`: clears `output`, flattens
    /// into a `VECTOR2D` buffer with the `double` overload and appends each point through
    /// `KiROUND` (whose overflow log is the math util unit's deferred `kimathLogOverflow`). The
    /// C++ default argument is [`DEFAULT_MAX_ERROR_I`].
    pub fn get_poly_i(
        &self,
        output: &mut Vec<Vector2I>,
        max_error: i32,
        step_limit: u64,
    ) -> BezierResult<()> {
        self.get_poly_i_with(&Pinned, output, max_error, step_limit)
    }

    pub(crate) fn get_poly_i_with(
        &self,
        host: &impl Host,
        output: &mut Vec<Vector2I>,
        max_error: i32,
        step_limit: u64,
    ) -> BezierResult<()> {
        output.clear();
        let mut buffer = Vec::new();
        self.get_poly_d_with(host, &mut buffer, f64::from(max_error), step_limit)?;

        for pt in buffer {
            output.push(V::new(round(pt.x)?, round(pt.y)?));
        }

        Ok(())
    }
}

/// `KiROUND( v )` to `int`.
fn round(value: f64) -> BezierResult<i32> {
    util::round_i32(value).map_err(vector_error)
}

fn vector_error(error: VectorDomainError) -> BezierError {
    match error {
        VectorDomainError::Unported { upstream, unit } => BezierError::Unported { upstream, unit },
        _ => BezierError::Undefined,
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for f64 {}
}

/// The two instantiations of `TransformEllipseToBeziers`: what differs between `ELLIPSE<int>`
/// and `ELLIPSE<double>`.
pub trait EllipseBezierCoordinate: sealed::Sealed + Copy {
    /// `VECTOR2D( aEllipse.MajorRadius, aEllipse.MinorRadius )`'s component.
    fn radius(self) -> f64;
    /// `aPoint += aEllipse.Center`, as compiled (`Center` the destination).
    fn offset(point: Vector2D, center: Vector2<Self>) -> Vector2D;
    /// The `VECTOR2<T>` the `BEZIER<T>` constructor stores.
    fn store(point: Vector2D) -> BezierResult<Vector2<Self>>;
}

impl EllipseBezierCoordinate for f64 {
    fn radius(self) -> f64 {
        self
    }

    fn offset(point: Vector2D, center: Vector2D) -> Vector2D {
        // TransformEllipseToBeziers<double>: addpd point, center.
        V::new(add(center.x, point.x), add(center.y, point.y))
    }

    fn store(point: Vector2D) -> BezierResult<Vector2D> {
        Ok(point)
    }
}

impl EllipseBezierCoordinate for i32 {
    fn radius(self) -> f64 {
        f64::from(self)
    }

    fn offset(point: Vector2D, center: Vector2I) -> Vector2D {
        // VECTOR2D( Center ) is exact and never a NaN, so the order cannot show.
        let center = to_d(center);
        V::new(add(center.x, point.x), add(center.y, point.y))
    }

    fn store(point: Vector2D) -> BezierResult<Vector2I> {
        // VECTOR2<int>'s casting constructor: std::clamp to the int range, then truncation; a
        // NaN component is undefined.
        Vector2I::try_from(point).map_err(vector_error)
    }
}

/// `TransformEllipseToBeziers( aEllipse, aBeziers )`: appends `ceil( |arc| / 90 degrees )`
/// cubic curves approximating the (clockwise) elliptical arc, each the unit-circle segment
/// `{ 1, 0 }, { 1, k }, ..., { cos θ, sin θ }` (`k = 4/3 tan( θ / 4 )`) rotated to its start,
/// scaled by the radii, rotated by the ellipse and offset to its centre. An arc count that is
/// NaN or beyond `int` is undefined; more than `bezier_limit` curves is
/// [`BezierError::LimitExceeded`] (checked before any curve is added).
pub fn transform_ellipse_to_beziers<T: EllipseBezierCoordinate>(
    ellipse: &Ellipse<T>,
    beziers: &mut Vec<Bezier<T>>,
    bezier_limit: u64,
) -> BezierResult<()> {
    // TransformEllipseToBeziers<double> +0x1f..+0x5a: subsd S, E; xorpd; subsd 360.
    let mut arc_angle = -(ellipse.end_angle - ellipse.start_angle);

    if arc_angle >= ANGLE_0 {
        arc_angle -= ANGLE_360;
    }

    // Minimum number of Beziers to use for a full circle to keep error manageable.
    const MIN_BEZIERS_PER_CIRCLE: i32 = 4;

    // `2 * M_PI / minBeziersPerCircle`, folded by GCC (an exact scaling of M_PI).
    let quarter = div(
        mul(2.0, core::f64::consts::PI),
        f64::from(MIN_BEZIERS_PER_CIRCLE),
    );
    let num_beziers = ceil_to_int(div(arc_angle.as_radians(), quarter).abs().ceil())?;

    if u64::try_from(num_beziers).is_ok_and(|n| n > bezier_limit) {
        return Err(BezierError::LimitExceeded);
    }

    // Angle occupied by each Bezier
    let angle_increment = div(arc_angle.as_radians(), f64::from(num_beziers));

    // k = 4/3 * tan(θ/4); `theta / 4` compiled as the exact `theta * 0.25`.
    let theta = angle_increment;
    let k = mul(4.0 / 3.0, reference_libm::tan(mul(theta, 0.25)));

    // One sincos( theta ) call serves every std::sin( theta ) and std::cos( theta ).
    let (sin_theta, cos_theta) = reference_libm::sincos(theta);

    // +0x13c..+0x148: mulsd k, sin; addsd cos, ksin; mulsd k, cos; subsd kcos, sin.
    let first = [
        V::new(1.0, 0.0),
        V::new(1.0, k),
        V::new(
            add(mul(sin_theta, k), cos_theta),
            sub(sin_theta, mul(cos_theta, k)),
        ),
        V::new(cos_theta, sin_theta),
    ];

    let transform_point = |point: Vector2D, angle: f64| -> Vector2D {
        let mut point = point;

        // Bring to the actual starting angle
        trigo_header::rotate_point_d(
            &mut point,
            -EdaAngle::new(
                sub(angle, ellipse.start_angle.as_radians()),
                AngleUnit::Radians,
            ),
        );

        // Then scale to the major and minor radiuses of the ellipse (mulpd point, radii)
        point = V::new(
            mul(ellipse.major_radius.radius(), point.x),
            mul(ellipse.minor_radius.radius(), point.y),
        );

        // Now rotate to the ellipse coordinate system
        trigo_header::rotate_point_d(&mut point, -ellipse.rotation);

        // And finally offset to the center location of the ellipse
        T::offset(point, ellipse.center)
    };

    for i in 0..num_beziers {
        let angle = mul(f64::from(i), angle_increment);
        // The braced list's elements are evaluated and converted in order.
        let start = T::store(transform_point(first[0], angle))?;
        let c1 = T::store(transform_point(first[1], angle))?;
        let c2 = T::store(transform_point(first[2], angle))?;
        let end = T::store(transform_point(first[3], angle))?;
        beziers.push(Bezier { start, c1, c2, end });
    }

    Ok(())
}

#[cfg(test)]
#[path = "bezier_curves_tests.rs"]
mod tests;
