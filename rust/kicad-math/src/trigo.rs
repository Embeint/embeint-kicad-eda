// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/src/trigo.cpp.
// Copyright (C) 2014 Jean-Pierre Charras, jp.charras at wanadoo.fr
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! Trigonometric and geometric basic functions (libs/kimath/src/trigo.cpp) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.src.trigo.application.ui.p001` owns every definition of trigo.cpp:
//! [`is_point_on_segment`], [`segment_intersects_segment`], [`test_segment_hit`],
//! [`calc_arc_mid`], the four `RotatePoint` overloads ([`rotate_point_i`],
//! [`rotate_point_i_about`], [`rotate_point_d_about`], [`rotate_point_d`]), the three
//! `CalcArcCenter` overloads ([`calc_arc_center_angle`], [`calc_arc_center_d`],
//! [`calc_arc_center_i`]), [`is_straight_arc`] and the anonymous-namespace helpers
//! `degenerateArcCenter`, `collinearArcCenter`, `det2`, `snapArcCenter` and
//! `unclampedArcCenter` with their constants. `CircleCenterFrom3Points` is compiled out
//! (`USE_ALTERNATE_CENTER_ALGO` is not defined) and is not ported.
//!
//! Two bodies call inline wrappers of `trigo.h`, which belongs to
//! `libs.kimath.include.application.ui.p001` and is not ported: `CalcArcMid` calls
//! `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` and the chord/angle
//! `CalcArcCenter` calls `RotatePoint( VECTOR2D&, const EDA_ANGLE& )`. Those calls return
//! [`TrigoError::Unported`] naming that owner, through the [`HeaderCallees`] seam, and the
//! continuation after each of them is implemented (and exercised by in-crate tests through a
//! forwarding seam, which is not oracle evidence).
//!
//! Arithmetic follows the source expression order without contraction, on the accepted
//! [`EdaAngle`], [`Vector2`], [`Seg`], `KiROUND` ([`round_i32`]) and `KI_INT128` bodies.
//! `double` operations use the reference platform's NaN rules ([`fp`]). The explicit
//! `std::fma` calls (in `det2` and the three-point `CalcArcCenter`) are the correctly rounded
//! IEEE fused multiply-add ([`fma`]). `std::minmax( { a, b, c } )` is libstdc++'s
//! `__minmax_element` order ([`minmax3`]).
//!
//! Signed overflow and the negation or `std::abs` of the minimum are undefined in the C++ and
//! are refused with [`TrigoError::Undefined`] at the first such operation. A `KiROUND`
//! overflow keeps the math util unit's `kimathLogOverflow` deferral.
use crate::eda_angle::{self, EdaAngle, GeometryError, ANGLE_0, ANGLE_180, ANGLE_270};
use crate::eda_angle::{ANGLE_360, ANGLE_90};
use crate::fp;
use crate::seg::{Seg, SegError};
use crate::util::round_i32;
use crate::vector::{Vector2, Vector2D, Vector2I, Vector2L, VectorDomainError};
use crate::wide_int::{cross_wide, to_double, KiInt128, WideIntError};

/// This unit.
pub const UNIT: &str = "libs.kimath.src.trigo.application.ui.p001";
/// Owner of `trigo.h`, whose inline `RotatePoint` wrappers two bodies call.
pub const TRIGO_HEADER_UNIT: &str = "libs.kimath.include.application.ui.p001";
/// The `trigo.h` wrapper `CalcArcMid` calls.
pub const ROTATE_VECTOR2I_ABOUT: &str = "RotatePoint(VECTOR2I&, const VECTOR2I&, EDA_ANGLE)";
/// The `trigo.h` wrapper the chord/angle `CalcArcCenter` calls.
pub const ROTATE_VECTOR2D: &str = "RotatePoint(VECTOR2D&, EDA_ANGLE)";

/// Why a trigo operation produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrigoError {
    /// The pinned C++ evaluation is undefined for this input.
    Undefined,
    /// A dependency call that is not yet ported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

pub type TrigoResult<T> = Result<T, TrigoError>;

impl From<VectorDomainError> for TrigoError {
    fn from(error: VectorDomainError) -> Self {
        match error {
            VectorDomainError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

impl From<SegError> for TrigoError {
    fn from(error: SegError) -> Self {
        match error {
            SegError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            SegError::Undefined => Self::Undefined,
        }
    }
}

impl From<GeometryError> for TrigoError {
    fn from(error: GeometryError) -> Self {
        match error {
            GeometryError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

impl From<WideIntError> for TrigoError {
    fn from(_: WideIntError) -> Self {
        Self::Undefined
    }
}

fn checked<T>(value: Option<T>) -> TrigoResult<T> {
    value.ok_or(TrigoError::Undefined)
}

/// `double / double` with the reference NaN rules (the accepted EDA_ANGLE division helper).
fn div(a: f64, b: f64) -> f64 {
    eda_angle::div(a, b)
}

/// `std::fma( x, y, z )`: the single correctly rounded IEEE fused multiply-add. For finite
/// operands the value is unique and NaN cannot occur, so [`f64::mul_add`] is exact on every
/// target. With an infinite or NaN operand the non-NaN result is equally fixed by IEEE 754; a
/// NaN result is spelled out as glibc's special-operand path computes it (`x * y + z` in SSE2,
/// or `( z + x ) + y` for an infinite `z`, which is never NaN there): the first NaN operand in
/// `x, y, z` order quieted, otherwise x86's negative indefinite NaN. Wasm does not specify NaN
/// propagation, so the rule is explicit here.
fn fma(x: f64, y: f64, z: f64) -> f64 {
    let result = x.mul_add(y, z);
    if !result.is_nan() {
        return result;
    }
    const QUIET: u64 = 0x0008_0000_0000_0000;
    for operand in [x, y, z] {
        if operand.is_nan() {
            return f64::from_bits(operand.to_bits() | QUIET);
        }
    }
    f64::from_bits(0xfff8_0000_0000_0000)
}

/// `std::minmax( { a, b, c } )` (libstdc++ `std::__minmax_element` with `operator<`, GCC
/// 13.3 bits/stl_algo.h): the first pair is ordered, then the last element replaces the
/// minimum when strictly smaller, otherwise the maximum when not smaller than it. Ties keep
/// the first minimum and the last maximum. Only the values are observable. `!( c < max )` is
/// libstdc++'s own test and is kept verbatim.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn minmax3(a: f64, b: f64, c: f64) -> (f64, f64) {
    let (mut min, mut max) = if b < a { (b, a) } else { (a, b) };
    if c < min {
        min = c;
    } else if !(c < max) {
        max = c;
    }
    (min, max)
}

fn dvec(v: Vector2I) -> Vector2D {
    Vector2::new(f64::from(v.x), f64::from(v.y))
}

/// The `trigo.h` inline wrappers that trigo.cpp bodies call, as one seam so their
/// continuations can be exercised by in-crate tests. [`PinnedHeader`] is the only production
/// implementation.
pub(crate) trait HeaderCallees {
    /// `RotatePoint( VECTOR2I& aPoint, const VECTOR2I& aCentre, const EDA_ANGLE& aAngle )`.
    fn rotate_vector2i_about(
        &self,
        point: &mut Vector2I,
        centre: Vector2I,
        angle: EdaAngle,
    ) -> TrigoResult<()>;
    /// `RotatePoint( VECTOR2D& point, const EDA_ANGLE& aAngle )`.
    fn rotate_vector2d(&self, point: &mut Vector2D, angle: EdaAngle) -> TrigoResult<()>;
}

/// The pinned header callees: typed deferrals to the unported `trigo.h` owner.
pub(crate) struct PinnedHeader;

impl HeaderCallees for PinnedHeader {
    fn rotate_vector2i_about(
        &self,
        _point: &mut Vector2I,
        _centre: Vector2I,
        _angle: EdaAngle,
    ) -> TrigoResult<()> {
        // UNPORTED(libs.kimath.include.application.ui.p001): RotatePoint(VECTOR2I&, const VECTOR2I&, EDA_ANGLE)
        Err(TrigoError::Unported {
            upstream: ROTATE_VECTOR2I_ABOUT,
            unit: TRIGO_HEADER_UNIT,
        })
    }

    fn rotate_vector2d(&self, _point: &mut Vector2D, _angle: EdaAngle) -> TrigoResult<()> {
        // UNPORTED(libs.kimath.include.application.ui.p001): RotatePoint(VECTOR2D&, EDA_ANGLE)
        Err(TrigoError::Unported {
            upstream: ROTATE_VECTOR2D,
            unit: TRIGO_HEADER_UNIT,
        })
    }
}

/// `IsPointOnSegment( aSegStart, aSegEnd, aTestPoint )`: whether the test point lies on the
/// segment (collinear and between the ends).
pub fn is_point_on_segment(
    seg_start: Vector2I,
    seg_end: Vector2I,
    test_point: Vector2I,
) -> TrigoResult<bool> {
    // Both VECTOR2I differences are int subtractions.
    let vect_seg = seg_end.checked_sub(seg_start)?; // Vector from S1 to S2
    let vect_point = test_point.checked_sub(seg_start)?; // Vector from S1 to P

    let (sx, sy) = (i64::from(vect_seg.x), i64::from(vect_seg.y));
    let (px, py) = (i64::from(vect_point.x), i64::from(vect_point.y));

    // Use long long here to avoid overflow in calculations
    let cross = checked(checked(sx.checked_mul(py))?.checked_sub(checked(sy.checked_mul(px))?))?;
    if cross != 0 {
        return Ok(false); /* Cross product non-zero, vectors not parallel */
    }

    let dot = checked(checked(sx.checked_mul(px))?.checked_add(checked(sy.checked_mul(py))?))?;
    let norm = checked(checked(px.checked_mul(px))?.checked_add(checked(py.checked_mul(py))?))?;
    if dot < norm {
        return Ok(false); /* Point not on segment */
    }

    Ok(true)
}

/// `SegmentIntersectsSegment( a_p1_l1, a_p2_l1, a_p1_l2, a_p2_l2, aIntersectionPoint )`.
/// `intersection` is the optional output (`nullptr` by default): when given, it is set to the
/// lines' intersection before the bounds tests, whether or not the segments intersect.
pub fn segment_intersects_segment(
    p1_l1: Vector2I,
    p2_l1: Vector2I,
    p1_l2: Vector2I,
    p2_l2: Vector2I,
    intersection: Option<&mut Vector2I>,
) -> TrigoResult<bool> {
    // Differences of two ints always fit int64_t.
    let dx_a = i64::from(p2_l1.x) - i64::from(p1_l1.x);
    let dy_a = i64::from(p2_l1.y) - i64::from(p1_l1.y);
    let dx_b = i64::from(p2_l2.x) - i64::from(p1_l2.x);
    let dy_b = i64::from(p2_l2.y) - i64::from(p1_l2.y);
    let dx_ab = i64::from(p1_l2.x) - i64::from(p1_l1.x);
    let dy_ab = i64::from(p1_l2.y) - i64::from(p1_l1.y);

    let mut den =
        checked(checked(dy_a.checked_mul(dx_b))?.checked_sub(checked(dy_b.checked_mul(dx_a))?))?;

    // Check if lines are parallel.
    if den == 0 {
        return Ok(false);
    }

    let mut num_a =
        checked(checked(dy_ab.checked_mul(dx_b))?.checked_sub(checked(dy_b.checked_mul(dx_ab))?))?;
    let mut num_b =
        checked(checked(dy_ab.checked_mul(dx_a))?.checked_sub(checked(dy_a.checked_mul(dx_ab))?))?;

    // Only compute the intersection point if requested.
    if let Some(point) = intersection {
        *point = p1_l1;
        // dX_a * ( double )num_a / ( double )den: dX_a converts to double first.
        let ox = round_i32(div(fp::mul(dx_a as f64, num_a as f64), den as f64))?;
        point.x = checked(point.x.checked_add(ox))?;
        let oy = round_i32(div(fp::mul(dy_a as f64, num_b as f64), den as f64))?;
        point.y = checked(point.y.checked_add(oy))?;
    }

    if den < 0 {
        den = checked(den.checked_neg())?;
        num_a = checked(num_a.checked_neg())?;
        num_b = checked(num_b.checked_neg())?;
    }

    // Test sign( u_a ) and return false if negative.
    if num_a < 0 {
        return Ok(false);
    }

    // Test sign( u_b ) and return false if negative.
    if num_b < 0 {
        return Ok(false);
    }

    // Test to ensure (u_a <= 1).
    if num_a > den {
        return Ok(false);
    }

    // Test to ensure (u_b <= 1).
    if num_b > den {
        return Ok(false);
    }

    Ok(true)
}

/// `TestSegmentHit( aRefPoint, aStart, aEnd, aDist )`: whether the point is within `dist` of
/// the segment.
pub fn test_segment_hit(
    ref_point: Vector2I,
    start: Vector2I,
    end: Vector2I,
    dist: i32,
) -> TrigoResult<bool> {
    let mut xmin = start.x;
    let mut xmax = end.x;
    let mut ymin = start.y;
    let mut ymax = end.y;
    let delta = start.checked_sub(ref_point)?;

    if xmax < xmin {
        core::mem::swap(&mut xmax, &mut xmin);
    }

    if ymax < ymin {
        core::mem::swap(&mut ymax, &mut ymin);
    }

    // Check if we are outside of the bounding box. Each int difference is evaluated only when
    // the || reaches it.
    if checked(ymin.checked_sub(ref_point.y))? > dist
        || checked(ref_point.y.checked_sub(ymax))? > dist
    {
        return Ok(false);
    }

    if checked(xmin.checked_sub(ref_point.x))? > dist
        || checked(ref_point.x.checked_sub(xmax))? > dist
    {
        return Ok(false);
    }

    // Eliminate easy cases.
    if start.x == end.x && ref_point.y > ymin && ref_point.y < ymax {
        return Ok(checked(delta.x.checked_abs())? <= dist);
    }

    if start.y == end.y && ref_point.x > xmin && ref_point.x < xmax {
        return Ok(checked(delta.y.checked_abs())? <= dist);
    }

    let segment = Seg::new(start, end);
    // The operands of < are unsequenced in C++; aDist + 1 overflowing is undefined whichever
    // runs first, so it is checked before SquaredDistance (whose own refusals are the same
    // undefined operations, or a KiROUND overflow log that cannot precede a defined result).
    let threshold = Seg::square(checked(dist.checked_add(1))?);
    Ok(segment.squared_distance_point(ref_point)? < threshold)
}

/// `CalcArcMid( aStart, aEnd, aCenter, aMinArcAngle = true )`: the midpoint of the arc.
pub fn calc_arc_mid(
    start: Vector2I,
    end: Vector2I,
    center: Vector2I,
    min_arc_angle: bool,
) -> TrigoResult<Vector2I> {
    calc_arc_mid_with(&PinnedHeader, start, end, center, min_arc_angle)
}

pub(crate) fn calc_arc_mid_with(
    header: &impl HeaderCallees,
    start: Vector2I,
    end: Vector2I,
    center: Vector2I,
    min_arc_angle: bool,
) -> TrigoResult<Vector2I> {
    let start_vector = start.checked_sub(center)?;
    let end_vector = end.checked_sub(center)?;

    // EDA_ANGLE( const VECTOR2D& ) through the exact int -> double vector conversion.
    let start_angle = EdaAngle::from_vector(dvec(start_vector))?;
    let end_angle = EdaAngle::from_vector(dvec(end_vector))?;
    let mut difference = start_angle - end_angle;
    let mut mid_point_rot_angle = difference.normalize180() / 2.0;

    if !min_arc_angle {
        mid_point_rot_angle.add_assign(ANGLE_180);
    }

    let mut new_mid = start;
    header.rotate_vector2i_about(&mut new_mid, center, mid_point_rot_angle)?;

    Ok(new_mid)
}

/// `RotatePoint( int* pX, int* pY, const EDA_ANGLE& aAngle )`. On an error neither output is
/// written.
pub fn rotate_point_i(px: &mut i32, py: &mut i32, angle: EdaAngle) -> TrigoResult<()> {
    let mut angle = angle;

    angle.normalize();

    // Cheap and dirty optimizations for 0, 90, 180, and 270 degrees.
    let pt = if angle == ANGLE_0 {
        Vector2::new(*px, *py)
    } else if angle == ANGLE_90 {
        /* sin = 1, cos = 0 */
        Vector2::new(*py, checked(px.checked_neg())?)
    } else if angle == ANGLE_180 {
        /* sin = 0, cos = -1 */
        Vector2::new(checked(px.checked_neg())?, checked(py.checked_neg())?)
    } else if angle == ANGLE_270 {
        /* sin = -1, cos = 0 */
        Vector2::new(checked(py.checked_neg())?, *px)
    } else {
        let sinus = angle.sin();
        let cosinus = angle.cos();
        let (x, y) = (f64::from(*px), f64::from(*py));

        let ptx = round_i32(fp::add(fp::mul(y, sinus), fp::mul(x, cosinus)))?;
        let pty = round_i32(fp::sub(fp::mul(y, cosinus), fp::mul(x, sinus)))?;
        Vector2::new(ptx, pty)
    };

    *px = pt.x;
    *py = pt.y;
    Ok(())
}

/// `RotatePoint( int* pX, int* pY, int cx, int cy, const EDA_ANGLE& angle )`. On an error
/// neither output is written.
pub fn rotate_point_i_about(
    px: &mut i32,
    py: &mut i32,
    cx: i32,
    cy: i32,
    angle: EdaAngle,
) -> TrigoResult<()> {
    let mut ox = checked(px.checked_sub(cx))?;
    let mut oy = checked(py.checked_sub(cy))?;

    rotate_point_i(&mut ox, &mut oy, angle)?;

    let x = checked(ox.checked_add(cx))?;
    let y = checked(oy.checked_add(cy))?;
    *px = x;
    *py = y;
    Ok(())
}

/// `RotatePoint( double* pX, double* pY, double cx, double cy, const EDA_ANGLE& angle )`.
pub fn rotate_point_d_about(px: &mut f64, py: &mut f64, cx: f64, cy: f64, angle: EdaAngle) {
    let mut ox = fp::sub(*px, cx);
    let mut oy = fp::sub(*py, cy);

    rotate_point_d(&mut ox, &mut oy, angle);

    *px = fp::add(ox, cx);
    *py = fp::add(oy, cy);
}

/// `RotatePoint( double* pX, double* pY, const EDA_ANGLE& aAngle )`.
pub fn rotate_point_d(px: &mut f64, py: &mut f64, angle: EdaAngle) {
    let mut angle = angle;

    angle.normalize();

    // Cheap and dirty optimizations for 0, 90, 180, and 270 degrees. Unary minus only flips
    // the sign bit (NaNs included), as x86 xorpd and wasm f64.neg do.
    let pt = if angle == ANGLE_0 {
        Vector2::new(*px, *py)
    } else if angle == ANGLE_90 {
        /* sin = 1, cos = 0 */
        Vector2::new(*py, -*px)
    } else if angle == ANGLE_180 {
        /* sin = 0, cos = -1 */
        Vector2::new(-*px, -*py)
    } else if angle == ANGLE_270 {
        /* sin = -1, cos = 0 */
        Vector2::new(-*py, *px)
    } else {
        let sinus = angle.sin();
        let cosinus = angle.cos();

        Vector2::new(
            fp::add(fp::mul(*py, sinus), fp::mul(*px, cosinus)),
            fp::sub(fp::mul(*py, cosinus), fp::mul(*px, sinus)),
        )
    };

    *px = pt.x;
    *py = pt.y;
}

/// `CalcArcCenter( const VECTOR2D& aStart, const VECTOR2D& aEnd, const EDA_ANGLE& aAngle )`.
pub fn calc_arc_center_angle(
    start: Vector2D,
    end: Vector2D,
    angle: EdaAngle,
) -> TrigoResult<Vector2D> {
    calc_arc_center_angle_with(&PinnedHeader, start, end, angle)
}

pub(crate) fn calc_arc_center_angle_with(
    header: &impl HeaderCallees,
    a_start: Vector2D,
    a_end: Vector2D,
    a_angle: EdaAngle,
) -> TrigoResult<Vector2D> {
    let mut angle = a_angle;
    let mut start = a_start;
    let mut end = a_end;

    if angle < ANGLE_0 {
        core::mem::swap(&mut start, &mut end);
        angle = -angle;
    }

    if angle > ANGLE_180 {
        core::mem::swap(&mut start, &mut end);
        angle = ANGLE_360 - angle;
    }

    let chord = (start - end).euclidean_norm()?;
    let sin_half_angle = (angle / 2.0).sin();

    // A zero arc angle has no defined center, so fall back to the chord midpoint
    if sin_half_angle == 0.0 {
        return Ok((start + end).divide(2.0)?);
    }

    // The center sits (chord/2) * cot(angle/2) off the chord; sqrt(r^2 - chord^2/4) cancels near 180 degrees
    let mut d = div(
        fp::mul(div(chord, 2.0), (angle / 2.0).cos()),
        sin_half_angle,
    );

    if !d.is_finite() {
        d = 0.0;
    }

    let mut vec2 = (end - start).resize(d)?;
    let vc = (end - start).resize(div(chord, 2.0))?;

    header.rotate_vector2d(&mut vec2, -ANGLE_90)?;

    Ok(start + vc + vec2)
}

// No unique circumcircle exists if any two of the three points coincide
// Bbox below catches all three; pairwise checks below catch just one pair
const CLUSTER_EXTENT: f64 = 5.0;

// A pair separated by more than integer rounding is a real, if small, arc.  This is not the
// cluster extent above: three points inside a 5 IU box are all noise, but two points 4 IU
// apart with a distant third still span a healthy triangle
const COINCIDENT_RADIUS: f64 = 2.0;
const COINCIDENT_RADIUS_SQUARED: f64 = COINCIDENT_RADIUS * COINCIDENT_RADIUS;

// Radius agreement required of a snapped center, in IU
const SNAP_RADIUS_TOLERANCE: f64 = 1.0;

// Stand-in radius for collinear points, whose true center is at infinity
const COLLINEAR_RADIUS: f64 = 1e17;

// Largest magnitude at which every integer is still a double
const MAX_EXACT_INTEGER: f64 = 9007199254740992.0;

// Integer arc centers are clamped to this range: numeric_limits<int>::min() + 100.0 and
// max() - 100.0, both exact doubles.
const MIN_ARC_CENTER: f64 = i32::MIN as f64 + 100.0;
const MAX_ARC_CENTER: f64 = i32::MAX as f64 - 100.0;

/// `degenerateArcCenter`: `Some( center )` (the C++ `true` with `aCenter` set) when no
/// unique circumcircle exists.
fn degenerate_arc_center(
    start: Vector2D,
    mid: Vector2D,
    end: Vector2D,
) -> TrigoResult<Option<Vector2D>> {
    let (min_x, max_x) = minmax3(start.x, mid.x, end.x);
    let (min_y, max_y) = minmax3(start.y, mid.y, end.y);

    if fp::sub(max_x, min_x) < CLUSTER_EXTENT && fp::sub(max_y, min_y) < CLUSTER_EXTENT {
        return Ok(Some(Vector2::new(
            div(fp::add(fp::add(start.x, mid.x), end.x), 3.0),
            div(fp::add(fp::add(start.y, mid.y), end.y), 3.0),
        )));
    }

    let coincident = |a: Vector2D, b: Vector2D| -> TrigoResult<bool> {
        Ok((a - b).squared_euclidean_norm()? < COINCIDENT_RADIUS_SQUARED)
    };

    // Two distinct points fall back to the chord midpoint, same as the diameter-arc paths below
    if coincident(start, mid)? || coincident(mid, end)? {
        return Ok(Some(Vector2::new(
            div(fp::add(start.x, end.x), 2.0),
            div(fp::add(start.y, end.y), 2.0),
        )));
    }

    if coincident(start, end)? {
        return Ok(Some(Vector2::new(
            div(fp::add(start.x, mid.x), 2.0),
            div(fp::add(start.y, mid.y), 2.0),
        )));
    }

    Ok(None)
}

/// `collinearArcCenter`.
fn collinear_arc_center(start: Vector2D, end: Vector2D) -> TrigoResult<Vector2D> {
    let chord = end - start;
    let mid = Vector2::new(
        div(fp::add(start.x, end.x), 2.0),
        div(fp::add(start.y, end.y), 2.0),
    );

    Ok(mid + Vector2::new(chord.y, -chord.x).resize(COLLINEAR_RADIUS)?)
}

/// `det2`: Kahan's a*d - b*c, accurate to about one ulp despite the cancellation
fn det2(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let w = fp::mul(b, c);
    let e = fma(-b, c, w);
    let f = fma(a, d, -w);

    fp::add(f, e)
}

/// `snapArcCenter`: prefer a nice 100 nm or 10 nm center when it still lies on the circle to
/// within a nanometer.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn snap_arc_center(
    center: Vector2D,
    start: Vector2D,
    mid: Vector2D,
    end: Vector2D,
) -> TrigoResult<Vector2D> {
    // `!( x < k )` is kept verbatim: it is also true for a NaN.
    if !(center.x.abs() < MAX_EXACT_INTEGER) || !(center.y.abs() < MAX_EXACT_INTEGER) {
        return Ok(center);
    }

    let radii_agree = |candidate: Vector2D| -> TrigoResult<bool> {
        let rs = (candidate - start).euclidean_norm()?;
        let rm = (candidate - mid).euclidean_norm()?;
        let re = (candidate - end).euclidean_norm()?;
        let (min_r, max_r) = minmax3(rs, rm, re);

        Ok(fp::sub(max_r, min_r) <= SNAP_RADIUS_TOLERANCE)
    };

    for grid in [100.0, 10.0] {
        let candidate = Vector2::new(
            fp::mul(fp::add(div(center.x, grid), 0.5).floor(), grid),
            fp::mul(fp::add(div(center.y, grid), 0.5).floor(), grid),
        );

        if radii_agree(candidate)? {
            return Ok(candidate);
        }
    }

    Ok(center)
}

fn wide(v: i64) -> KiInt128 {
    KiInt128::from(v)
}

fn mul128(a: KiInt128, b: KiInt128) -> TrigoResult<KiInt128> {
    checked(a.checked_mul(b))
}

/// `KI_INT128( s ) * d + p * KI_INT128( q ) - r * KI_INT128( t )`, left to right.
fn numerator(
    s: i32,
    d: KiInt128,
    p: KiInt128,
    q: i64,
    r: KiInt128,
    t: i64,
) -> TrigoResult<KiInt128> {
    let sum = checked(mul128(KiInt128::from(s), d)?.checked_add(mul128(p, wide(q))?))?;
    checked(sum.checked_sub(mul128(r, wide(t))?))
}

/// `unclampedArcCenter`.
fn unclamped_arc_center(start: Vector2I, mid: Vector2I, end: Vector2I) -> TrigoResult<Vector2D> {
    let d_start = dvec(start);
    let d_mid = dvec(mid);
    let d_end = dvec(end);

    if let Some(d_center) = degenerate_arc_center(d_start, d_mid, d_end)? {
        return Ok(d_center);
    }

    // Deltas are exact in 64 bits and the numerators in 128, so only the final divisions round
    let b: Vector2L = Vector2::new(
        i64::from(mid.x) - i64::from(start.x),
        i64::from(mid.y) - i64::from(start.y),
    );
    let c: Vector2L = Vector2::new(
        i64::from(end.x) - i64::from(start.x),
        i64::from(end.y) - i64::from(start.y),
    );

    // Every KI_INT128 value below is at most about 2^99 in magnitude; checked anyway.
    let b2 = checked(mul128(wide(b.x), wide(b.x))?.checked_add(mul128(wide(b.y), wide(b.y))?))?;
    let c2 = checked(mul128(wide(c.x), wide(c.x))?.checked_add(mul128(wide(c.y), wide(c.y))?))?;
    let d = mul128(cross_wide(&b, &c)?, 2)?;

    if d == 0 {
        return collinear_arc_center(d_start, d_end);
    }

    // Fold start into the numerator so the sum is rounded once
    let dd = to_double(d);
    let nx = numerator(start.x, d, b2, c.y, c2, b.y)?;
    let ny = numerator(start.y, d, c2, b.x, b2, c.x)?;
    let cx = div(to_double(nx), dd);
    let cy = div(to_double(ny), dd);

    snap_arc_center(Vector2::new(cx, cy), d_start, d_mid, d_end)
}

/// `CalcArcCenter( const VECTOR2D& aStart, const VECTOR2D& aMid, const VECTOR2D& aEnd )`.
///
/// The C++ passes the coordinates to `std::minmax`, whose `operator<` must be a strict weak
/// ordering: a NaN coordinate violates that library precondition, so callers (the oracle
/// adapter included) do not pass one. This port evaluates such input by the libstdc++ order.
pub fn calc_arc_center_d(start: Vector2D, mid: Vector2D, end: Vector2D) -> TrigoResult<Vector2D> {
    if let Some(center) = degenerate_arc_center(start, mid, end)? {
        return Ok(center);
    }

    // Work relative to start so the result cannot depend on where the arc sits
    let bx = fp::sub(mid.x, start.x);
    let by = fp::sub(mid.y, start.y);
    let cx = fp::sub(end.x, start.x);
    let cy = fp::sub(end.y, start.y);

    let b2 = fma(bx, bx, fp::mul(by, by));
    let c2 = fma(cx, cx, fp::mul(cy, cy));
    let d = fp::mul(2.0, det2(bx, by, cx, cy));

    if d == 0.0 {
        return collinear_arc_center(start, end);
    }

    let ux = div(det2(b2, c2, by, cy), d);
    let uy = div(det2(c2, b2, cx, bx), d);

    snap_arc_center(
        Vector2::new(fp::add(start.x, ux), fp::add(start.y, uy)),
        start,
        mid,
        end,
    )
}

/// `std::clamp( v, lo, hi )` (libstdc++: `v < lo ? lo : hi < v ? hi : v`), spelled out so the
/// comparison order is the reference's.
#[allow(clippy::manual_clamp)]
fn clamp_center(v: f64) -> f64 {
    if v < MIN_ARC_CENTER {
        MIN_ARC_CENTER
    } else if MAX_ARC_CENTER < v {
        MAX_ARC_CENTER
    } else {
        v
    }
}

/// `CalcArcCenter( const VECTOR2I& aStart, const VECTOR2I& aMid, const VECTOR2I& aEnd )`.
pub fn calc_arc_center_i(start: Vector2I, mid: Vector2I, end: Vector2I) -> TrigoResult<Vector2I> {
    let d_center = unclamped_arc_center(start, mid, end)?;

    // The clamped values are inside the int range, so neither KiROUND logs.
    Ok(Vector2::new(
        round_i32(clamp_center(d_center.x))?,
        round_i32(clamp_center(d_center.y))?,
    ))
}

/// `IsStraightArc( aStart, aMid, aEnd )`.
pub fn is_straight_arc(start: Vector2I, mid: Vector2I, end: Vector2I) -> TrigoResult<bool> {
    if start == end {
        return Ok(false);
    }

    let l = |v: Vector2I| -> Vector2L { Vector2::new(i64::from(v.x), i64::from(v.y)) };
    // VECTOR2L differences of int values always fit int64_t.
    let mid_l = Vector2::new(l(mid).x - l(start).x, l(mid).y - l(start).y);
    let end_l = Vector2::new(l(end).x - l(start).x, l(end).y - l(start).y);

    if cross_wide(&mid_l, &end_l)? == 0 {
        return Ok(true);
    }

    let center = unclamped_arc_center(start, mid, end)?;

    // Near-collinear points put the center outside the clamp
    Ok(!(center.x >= MIN_ARC_CENTER
        && center.x <= MAX_ARC_CENTER
        && center.y >= MIN_ARC_CENTER
        && center.y <= MAX_ARC_CENTER))
}

#[cfg(test)]
#[path = "trigo_tests.rs"]
mod tests;
