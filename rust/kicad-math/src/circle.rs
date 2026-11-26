// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/src/geometry/circle.cpp.
// Copyright (C) 2021 Roberto Fernandez Bautista <roberto.fer.bau@gmail.com>
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `CIRCLE` (libs/kimath/src/geometry/circle.cpp) at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.src.geometry.circle.application.ui.p001` owns all 13 out-of-line bodies:
//! the three constructors, `ConstructFromTanTanPt`, both `Contains` overloads, `NearestPoint`
//! and `FurthestPoint` for `VECTOR2I` and `VECTOR2D`, both `Intersect` overloads and
//! `IntersectLine`. They are ported in source order onto the accepted [`Seg`], [`Vector2`],
//! [`EdaAngle`] and rounding backends; no second geometry model is introduced.
//!
//! Minimum co-ports, ownership unchanged:
//! - [`Circle`]'s two data members and the defaulted `operator==` come from `geometry/circle.h`,
//!   owned by `libs.kimath.include.geometry.application.pure.p001` (todo).
//! - [`MIN_PRECISION_IU`] is the constant `SHAPE::MIN_PRECISION_IU = 4` of `geometry/shape.h`,
//!   owned by `libs.kimath.include.geometry.application.ui.p003` (todo).
//!
//! Calls into unported owners return [`CircleError::Unported`] and the continuation after each
//! of them is implemented: `SEG::Square` (reached through `SEG::ApproxParallel`), `SEG::Center`
//! and `SEG::IntersectLines` (seg.h, header unit p003), `CalcArcMid` (trigo.cpp) and
//! `RotatePoint( VECTOR2I&, const EDA_ANGLE& )` (trigo.h). A `KiROUND` overflow keeps the math
//! util unit's `kimathLogOverflow` deferral.
//!
//! Signed overflow, `std::abs` of the minimum and out-of-range floating-to-integer conversions
//! are undefined in the C++ and are refused with [`CircleError::Undefined`] at the first such
//! operation. A failed `wxCHECK_MSG` returns `*this` as it is at that point, as the accepted
//! `VECTOR3::Normalize` port does; the host assertion handler is not modelled.
use crate::eda_angle::{EdaAngle, GeometryError};
use crate::fp;
use crate::seg::{OptVector2I, Seg, SegError, HEADER_UNIT as SEG_HEADER_UNIT};
use crate::util::round_i32;
use crate::vector::{Vector2, Vector2D, Vector2I, Vector2L, VectorDomainError};

/// This unit.
pub const UNIT: &str = "libs.kimath.src.geometry.circle.application.ui.p001";
/// Owner of `geometry/circle.h` (the [`Circle`] type co-port).
pub const CIRCLE_HEADER_UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";
/// Owner of `trigo.h`, whose inline `RotatePoint( VECTOR2I&, const EDA_ANGLE& )` is called.
pub const TRIGO_HEADER_UNIT: &str = "libs.kimath.include.application.ui.p001";
/// Owner of `trigo.cpp` (`CalcArcMid`).
pub const TRIGO_UNIT: &str = "libs.kimath.src.trigo.application.ui.p001";

/// `SHAPE::MIN_PRECISION_IU` (geometry/shape.h:129), the rounding margin of the
/// on-circumference and tangent tests. Minimum constant co-port from
/// `libs.kimath.include.geometry.application.ui.p003`, which keeps ownership.
pub const MIN_PRECISION_IU: i32 = 4;

/// Why a circle operation produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircleError {
    /// The pinned C++ evaluation is undefined for this input.
    Undefined,
    /// A dependency call that is not yet ported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

pub type CircleResult<T> = Result<T, CircleError>;

impl From<VectorDomainError> for CircleError {
    fn from(error: VectorDomainError) -> Self {
        match error {
            VectorDomainError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

impl From<SegError> for CircleError {
    fn from(error: SegError) -> Self {
        match error {
            SegError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            SegError::Undefined => Self::Undefined,
        }
    }
}

impl From<GeometryError> for CircleError {
    fn from(error: GeometryError) -> Self {
        match error {
            GeometryError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

fn checked<T>(value: Option<T>) -> CircleResult<T> {
    value.ok_or(CircleError::Undefined)
}

/// `CIRCLE`: public `Radius` and `Center`.
///
/// The members are declared in `circle.h` order (`Radius`, then `Center`), so the derived
/// equality compares them in the order of the defaulted `operator==`. `Copy`/`Clone` are the
/// user-declared copy constructor's memberwise copy ([`Circle::copy_of`]) and the implicit copy
/// assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Circle {
    pub radius: i32,
    pub center: Vector2I,
}

impl Default for Circle {
    /// `CIRCLE()`.
    fn default() -> Self {
        Self::new()
    }
}

/// The calls `circle.cpp` makes into owners that are not yet ported, as one seam so their
/// continuations can be exercised by in-crate tests. [`Pinned`] is the only production
/// implementation.
pub(crate) trait Callees {
    /// `aLineA.ApproxParallel( aLineB )` with its default threshold of 1.
    fn approx_parallel(&self, line_a: &Seg, line_b: &Seg) -> CircleResult<bool>;
    /// `SEG::Center()` (seg.h).
    fn seg_center(&self, seg: &Seg) -> CircleResult<Vector2I>;
    /// `SEG::IntersectLines( const SEG& )` (seg.h).
    fn intersect_lines(&self, seg: &Seg, other: &Seg) -> CircleResult<OptVector2I>;
    /// `CalcArcMid( aStart, aEnd, aCenter, aMinArcAngle )` (trigo.cpp).
    fn calc_arc_mid(
        &self,
        start: Vector2I,
        end: Vector2I,
        center: Vector2I,
        min_arc_angle: bool,
    ) -> CircleResult<Vector2I>;
    /// `RotatePoint( VECTOR2I&, const EDA_ANGLE& )` (trigo.h).
    fn rotate_point(&self, point: &mut Vector2I, angle: EdaAngle) -> CircleResult<()>;
}

/// The pinned callees: the accepted SEG source body, and typed deferrals for the rest.
pub(crate) struct Pinned;

impl Callees for Pinned {
    fn approx_parallel(&self, line_a: &Seg, line_b: &Seg) -> CircleResult<bool> {
        // The accepted seg.cpp body; it reaches its own SEG::Square deferral first.
        Ok(line_a.approx_parallel(line_b, 1)?)
    }

    fn seg_center(&self, _seg: &Seg) -> CircleResult<Vector2I> {
        // UNPORTED(libs.kimath.include.geometry.application.ui.p003): SEG::Center
        Err(CircleError::Unported {
            upstream: "SEG::Center",
            unit: SEG_HEADER_UNIT,
        })
    }

    fn intersect_lines(&self, _seg: &Seg, _other: &Seg) -> CircleResult<OptVector2I> {
        // UNPORTED(libs.kimath.include.geometry.application.ui.p003): SEG::IntersectLines
        Err(CircleError::Unported {
            upstream: "SEG::IntersectLines",
            unit: SEG_HEADER_UNIT,
        })
    }

    fn calc_arc_mid(
        &self,
        _start: Vector2I,
        _end: Vector2I,
        _center: Vector2I,
        _min_arc_angle: bool,
    ) -> CircleResult<Vector2I> {
        // UNPORTED(libs.kimath.src.trigo.application.ui.p001): CalcArcMid
        Err(CircleError::Unported {
            upstream: "CalcArcMid",
            unit: TRIGO_UNIT,
        })
    }

    fn rotate_point(&self, _point: &mut Vector2I, _angle: EdaAngle) -> CircleResult<()> {
        // UNPORTED(libs.kimath.include.application.ui.p001): RotatePoint(VECTOR2I&, EDA_ANGLE)
        Err(CircleError::Unported {
            upstream: "RotatePoint(VECTOR2I&, EDA_ANGLE)",
            unit: TRIGO_HEADER_UNIT,
        })
    }
}

/// `VECTOR2L( a ) - b` with `b` a `VECTOR2I`: `int64_t` components, which cannot overflow.
fn wide_sub(a: Vector2I, b: Vector2I) -> Vector2L {
    Vector2::new(
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    )
}

/// `VECTOR2<int>( int64_t, int64_t )`: each argument converts to `int` (C++20 modular).
fn modular_point(x: i64, y: i64) -> Vector2I {
    Vector2::new(x as i32, y as i32)
}

/// `int( double )` of an implicit conversion: truncation, undefined outside the `int` range or
/// for a NaN.
fn truncate_to_int(value: f64) -> CircleResult<i32> {
    if value.is_nan() {
        return Err(CircleError::Undefined);
    }
    let t = value.trunc();
    if !(-2_147_483_648.0..2_147_483_648.0).contains(&t) {
        return Err(CircleError::Undefined);
    }
    Ok(t as i32)
}

/// `VECTOR2<double> - VECTOR2<int>` (and the reverse): the common type is `double`; the `int`
/// operand converts exactly and only the `double` operand can be a NaN.
fn sub_d_i(a: Vector2D, b: Vector2I) -> Vector2D {
    Vector2::new(fp::sub(a.x, f64::from(b.x)), fp::sub(a.y, f64::from(b.y)))
}

fn sub_i_d(a: Vector2I, b: Vector2D) -> Vector2D {
    Vector2::new(fp::sub(f64::from(a.x), b.x), fp::sub(f64::from(a.y), b.y))
}

/// `VECTOR2<double> + VECTOR2<int>`: `double` components.
fn add_d_i(a: Vector2D, b: Vector2I) -> Vector2D {
    Vector2::new(fp::add(a.x, f64::from(b.x)), fp::add(a.y, f64::from(b.y)))
}

/// The `furthestFromIntersect` lambda of `ConstructFromTanTanPt`: `aPt1` when it is strictly
/// further from `intersectPoint`, otherwise `aPt2`.
fn furthest_from(intersect: Vector2I, pt1: Vector2I, pt2: Vector2I) -> CircleResult<Vector2I> {
    let d1 = pt1.try_sub(intersect)?.euclidean_norm()?;
    let d2 = pt2.try_sub(intersect)?.euclidean_norm()?;
    Ok(if d1 > d2 { pt1 } else { pt2 })
}

/// The `closestToIntersect` lambda: `aPt1` when it is no further from `intersectPoint`.
fn closest_to(intersect: Vector2I, pt1: Vector2I, pt2: Vector2I) -> CircleResult<Vector2I> {
    let d1 = pt1.try_sub(intersect)?.euclidean_norm()?;
    let d2 = pt2.try_sub(intersect)?.euclidean_norm()?;
    Ok(if d1 <= d2 { pt1 } else { pt2 })
}

/// `std::vector::front()` and `back()` of a non-empty result.
fn front_back(points: &[Vector2I]) -> Option<(Vector2I, Vector2I)> {
    Some((*points.first()?, *points.last()?))
}

impl Circle {
    /// `CIRCLE()`: `Center = { 0, 0 }`, then `Radius = 0`.
    pub const fn new() -> Self {
        Self {
            radius: 0,
            center: Vector2::new(0, 0),
        }
    }

    /// `CIRCLE( const VECTOR2I& aCenter, int aRadius )`.
    pub const fn with_center_radius(center: Vector2I, radius: i32) -> Self {
        Self { radius, center }
    }

    /// `CIRCLE( const CIRCLE& aOther )`: copies `Center`, then `Radius`.
    pub const fn copy_of(other: &Circle) -> Self {
        Self {
            radius: other.radius,
            center: other.center,
        }
    }

    /// `CIRCLE::ConstructFromTanTanPt`: makes this circle tangent to both lines and passing
    /// through `p`, choosing the fillet solution. Returns this circle (`*this`).
    pub fn construct_from_tan_tan_pt(
        &mut self,
        line_a: &Seg,
        line_b: &Seg,
        p: Vector2I,
    ) -> CircleResult<&mut Self> {
        self.construct_from_tan_tan_pt_with(&Pinned, line_a, line_b, p)
    }

    pub(crate) fn construct_from_tan_tan_pt_with(
        &mut self,
        callees: &impl Callees,
        line_a: &Seg,
        line_b: &Seg,
        p: Vector2I,
    ) -> CircleResult<&mut Self> {
        //fixme: There might be more efficient / accurate solution than using geometrical constructs

        let mut anglebisector = Seg::default();
        let intersect_point: Vector2I;

        if callees.approx_parallel(line_a, line_b)? {
            // Special case, no intersection point between the two lines
            // The center will be in the line equidistant between the two given lines
            // The radius will be half the distance between the two lines
            // The possible centers can be found by intersection

            let perpendicular_a_to_b = Seg::new(line_a.a, line_b.line_project(line_a.a)?);
            let mid_pt = callees.seg_center(&perpendicular_a_to_b)?;
            self.radius = mid_pt.try_sub(line_a.a)?.euclidean_norm()?;

            anglebisector = line_a.parallel_seg(mid_pt)?;

            self.center = p; // use this circle as a construction to find the actual centers
            let possible_centers = self.intersect_line(&anglebisector)?;

            // wxCHECK_MSG( possibleCenters.size() > 0, *this, wxT( "No solutions exist!" ) )
            let Some((front, back)) = front_back(&possible_centers) else {
                return Ok(self);
            };
            intersect_point = line_a.a; // just for the purpose of deciding which solution to return

            // For the special case of the two segments being parallel, we will return the
            // solution whose center is closest to aLineA.A
            self.center = closest_to(intersect_point, front, back)?;
        } else {
            // General case, using homothety.
            // All circles inscribed in the same angle are homothetic with center at the
            // intersection. In this code, the prefix "h" denotes "the homothetic image"
            let intersect_calc = callees.intersect_lines(line_a, line_b)?;
            // wxCHECK_MSG( intersectCalc, *this, "Lines do not intersect but are not parallel?" )
            let Some(calc) = intersect_calc else {
                return Ok(self);
            };
            intersect_point = calc;

            if p == intersect_point {
                //Special case: The point is at the intersection of the two lines
                self.center = p;
                self.radius = 0;
                return Ok(self);
            }

            // Calculate bisector
            let line_a_pt = furthest_from(intersect_point, line_a.a, line_a.b)?;
            let line_b_pt = furthest_from(intersect_point, line_b.a, line_b.b)?;
            let bisector_pt = callees.calc_arc_mid(line_a_pt, line_b_pt, intersect_point, true)?;

            anglebisector.a = intersect_point;
            anglebisector.b = bisector_pt;

            // Create an arbitrary circle that is tangent to both lines
            let mut h_solution = Circle::new();
            h_solution.center = anglebisector.line_project(p)?;
            h_solution.radius = line_a.line_distance(h_solution.center, false)?;

            // Find the homothetic image of aP in the construction circle (hSolution)
            let through_ap = Seg::new(intersect_point, p);
            let h_projections = h_solution.intersect_line(&through_ap)?;
            // wxCHECK_MSG( hProjections.size() > 0, *this, wxT( "No solutions exist!" ) )
            let Some((front, back)) = front_back(&h_projections) else {
                return Ok(self);
            };

            // We want to create a fillet, so the projection of homothetic projection of aP
            // should be the one closest to the intersection
            let h_selected = closest_to(intersect_point, front, back)?;

            let h_tan_line_a = line_a.line_project(h_solution.center)?;
            let h_tan_line_b = line_b.line_project(h_solution.center)?;

            // To minimise errors, use the furthest away tangent point from aP
            if h_tan_line_a.try_sub(p)?.squared_euclidean_norm()?
                > h_tan_line_b.try_sub(p)?.squared_euclidean_norm()?
            {
                // Find the tangent at line A by homothetic inversion
                let h_t = Seg::new(h_tan_line_a, h_selected);
                let act_tan_a = callees.intersect_lines(&h_t.parallel_seg(p)?, line_a)?;
                // wxCHECK_MSG( actTanA, *this, wxT( "No solutions exist!" ) )
                let Some(act_tan_a) = act_tan_a else {
                    return Ok(self);
                };

                // Find circle center by perpendicular intersection with the angle bisector
                let perpendicular_to_tan_a = line_a.perpendicular_seg(act_tan_a)?;
                let act_center =
                    callees.intersect_lines(&perpendicular_to_tan_a, &anglebisector)?;
                // wxCHECK_MSG( actCenter, *this, wxT( "No solutions exist!" ) )
                let Some(act_center) = act_center else {
                    return Ok(self);
                };

                self.center = act_center;
                self.radius = line_a.line_distance(self.center, false)?;
            } else {
                // Find the tangent at line B by inversion
                let h_t = Seg::new(h_tan_line_b, h_selected);
                let act_tan_b = callees.intersect_lines(&h_t.parallel_seg(p)?, line_b)?;
                // wxCHECK_MSG( actTanB, *this, wxT( "No solutions exist!" ) )
                let Some(act_tan_b) = act_tan_b else {
                    return Ok(self);
                };

                // Find circle center by perpendicular intersection with the angle bisector
                let perpendicular_to_tan_b = line_b.perpendicular_seg(act_tan_b)?;
                let act_center =
                    callees.intersect_lines(&perpendicular_to_tan_b, &anglebisector)?;
                // wxCHECK_MSG( actCenter, *this, wxT( "No solutions exist!" ) )
                let Some(act_center) = act_center else {
                    return Ok(self);
                };

                self.center = act_center;
                self.radius = line_b.line_distance(self.center, false)?;
            }
        }

        Ok(self)
    }

    /// `CIRCLE::Contains( const VECTOR2I& aP ) const`: whether `p` lies on the circumference,
    /// within [`MIN_PRECISION_IU`]. C++ selects this overload for a `const` circle; a call on a
    /// non-const lvalue selects [`Circle::contains_mut`] instead.
    pub fn contains(&self, p: Vector2I) -> CircleResult<bool> {
        let distance = wide_sub(p, self.center).euclidean_norm()?;

        Ok(
            distance <= i64::from(self.radius) + i64::from(MIN_PRECISION_IU)
                && distance >= i64::from(self.radius) - i64::from(MIN_PRECISION_IU),
        )
    }

    /// `CIRCLE::NearestPoint( const VECTOR2I& aP ) const`.
    pub fn nearest_point(&self, p: Vector2I) -> CircleResult<Vector2I> {
        let mut vec = p.try_sub(self.center)?;

        // Handle special case where aP is equal to this circle's center
        if vec.x == 0 && vec.y == 0 {
            vec.x = 1; // Arbitrary, to ensure the return value is always on the circumference
        }

        Ok(vec.resize(self.radius)?.try_add(self.center)?)
    }

    /// `CIRCLE::NearestPoint( const VECTOR2D& aP ) const`.
    pub fn nearest_point_d(&self, p: Vector2D) -> CircleResult<Vector2D> {
        let mut vec = sub_d_i(p, self.center);

        // Handle special case where aP is equal to this circle's center
        if vec.x == 0.0 && vec.y == 0.0 {
            vec.x = 1.0; // Arbitrary, to ensure the return value is always on the circumference
        }

        Ok(add_d_i(vec.resize(f64::from(self.radius))?, self.center))
    }

    /// `CIRCLE::FurthestPoint( const VECTOR2I& aP ) const`.
    pub fn furthest_point(&self, p: Vector2I) -> CircleResult<Vector2I> {
        let mut vec = self.center.try_sub(p)?;

        if vec.x == 0 && vec.y == 0 {
            vec.x = 1;
        }

        Ok(vec.resize(self.radius)?.try_add(self.center)?)
    }

    /// `CIRCLE::FurthestPoint( const VECTOR2D& aP ) const`.
    pub fn furthest_point_d(&self, p: Vector2D) -> CircleResult<Vector2D> {
        let mut vec = sub_i_d(self.center, p);

        if vec.x == 0.0 && vec.y == 0.0 {
            vec.x = 1.0;
        }

        Ok(add_d_i(vec.resize(f64::from(self.radius))?, self.center))
    }

    /// `CIRCLE::Intersect( const CIRCLE& aCircle ) const`: zero, one (tangent) or two points.
    pub fn intersect_circle(&self, circle: &Circle) -> CircleResult<Vec<Vector2I>> {
        self.intersect_circle_with(&Pinned, circle)
    }

    pub(crate) fn intersect_circle_with(
        &self,
        callees: &impl Callees,
        circle: &Circle,
    ) -> CircleResult<Vec<Vector2I>> {
        // From https://mathworld.wolfram.com/Circle-CircleIntersection.html: with this circle
        // at (0,0) and aCircle at (d,0), x = (d^2 + r1^2 - r2^2) / (2 * d) and
        // y = sqrt(r1^2 - x^2), then rotated and translated back.

        let mut retval = Vec::new();

        let vec_c_to_c = wide_sub(circle.center, self.center);
        let d: i64 = vec_c_to_c.euclidean_norm()?;
        let r1 = i64::from(self.radius);
        let r2 = i64::from(circle.radius);

        // Neither sum nor difference of two widened ints can overflow.
        if d > (r1 + r2) || d < (r1 - r2).abs() {
            return Ok(retval); //circles do not intersect
        }

        if d == 0 {
            return Ok(retval); // circles are co-centered. Don't return intersection points
        }

        // Equation (3), left to right; every int64_t operation is checked.
        let numerator = checked(
            d.checked_mul(d)
                .and_then(|dd| r1.checked_mul(r1).and_then(|r| dd.checked_add(r)))
                .and_then(|s| r2.checked_mul(r2).and_then(|r| s.checked_sub(r))),
        )?;
        let x = checked(
            2_i64
                .checked_mul(d)
                .and_then(|den| numerator.checked_div(den)),
        )?;
        let r1sq_minus_xsq = checked(
            r1.checked_mul(r1)
                .and_then(|r| x.checked_mul(x).and_then(|xx| r.checked_sub(xx))),
        )?;

        if r1sq_minus_xsq < 0 {
            return Ok(retval); //circles do not intersect
        }

        // Equation (4): the integral sqrt overload converts to double; KiROUND gives an int.
        let y = i64::from(round_i32((r1sq_minus_xsq as f64).sqrt())?);

        // Now correct back to original coordinates
        let rot_angle =
            EdaAngle::from_vector(Vector2::new(vec_c_to_c.x as f64, vec_c_to_c.y as f64))?;
        let mut solution1 = modular_point(x, y);
        callees.rotate_point(&mut solution1, -rot_angle)?;
        solution1.add_assign(self.center)?;
        retval.push(solution1);

        if y != 0 {
            let mut solution2 = modular_point(x, -y);
            callees.rotate_point(&mut solution2, -rot_angle)?;
            solution2.add_assign(self.center)?;
            retval.push(solution2);
        }

        Ok(retval)
    }

    /// `CIRCLE::Intersect( const SEG& aSeg ) const`: the line intersections that `aSeg`
    /// contains (`SEG::Contains` with its default threshold of 3).
    pub fn intersect_seg(&self, seg: &Seg) -> CircleResult<Vec<Vector2I>> {
        let mut retval = Vec::new();

        for intersection in self.intersect_line(seg)? {
            if seg.contains_point(intersection, 3)? {
                retval.push(intersection);
            }
        }

        Ok(retval)
    }

    /// `CIRCLE::IntersectLine( const SEG& aLine ) const`: zero, one (tangent) or two points of
    /// the infinite line through `aLine`.
    pub fn intersect_line(&self, line: &Seg) -> CircleResult<Vec<Vector2I>> {
        let mut retval = Vec::new();

        // Let O be the center of this circle, 1 and 2 the intersection points of the line and
        // M the center of the chord connecting points 1 and 2. M is O projected perpendicularly
        // to the line since a chord is always perpendicular to the radius, and by pythagoras
        // M1 = M2 = sqrt( Radius^2 - OM^2 ).

        let m = line.line_project(self.center)?; // O projected perpendicularly to the line
        let om_dist: i64 = wide_sub(m, self.center).euclidean_norm()?;

        let upper = i64::from(self.radius) + i64::from(MIN_PRECISION_IU);
        let lower = i64::from(self.radius) - i64::from(MIN_PRECISION_IU);
        if om_dist > upper {
            return Ok(retval); // does not intersect
        } else if om_dist <= upper && om_dist >= lower {
            retval.push(m);
            return Ok(retval); //tangent
        }

        let radius_squared = i64::from(self.radius) * i64::from(self.radius);
        // om_dist < Radius - MIN_PRECISION_IU here, so its square is below 2^62.
        let om_dist_squared = checked(om_dist.checked_mul(om_dist))?;

        let m_to_1_dist =
            truncate_to_int((checked(radius_squared.checked_sub(om_dist_squared))? as f64).sqrt())?;

        let m_to_1_vec = line.b.try_sub(line.a)?.resize(m_to_1_dist)?;
        let m_to_2_vec = m_to_1_vec.try_neg()?;

        retval.push(m_to_1_vec.try_add(m)?);
        retval.push(m_to_2_vec.try_add(m)?);

        Ok(retval)
    }

    /// `CIRCLE::Contains( const VECTOR2I& aP )` (non-const): whether `p` is strictly inside.
    /// C++ selects this overload for a non-const lvalue circle; it does not modify the circle.
    pub fn contains_mut(&mut self, p: Vector2I) -> CircleResult<bool> {
        Ok(p.try_sub(self.center)?.euclidean_norm()? < self.radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eda_angle::{ANGLE_0, ANGLE_180, ANGLE_270, ANGLE_90};

    fn v(x: i32, y: i32) -> Vector2I {
        Vector2::new(x, y)
    }

    fn unported(upstream: &'static str, unit: &'static str) -> CircleError {
        CircleError::Unported { upstream, unit }
    }

    /// Test-only callees for exercising continuations behind unported calls. The parallel
    /// decision is the test's declared input; `SEG::Center` and `SEG::IntersectLines` are the
    /// seg.h one-line bodies (`A + ( B - A ) / 2`; `Intersect( aSeg, false, true )`); RotatePoint
    /// implements only trigo.cpp's exact 0/90/180/270 degree branches and defers otherwise.
    /// None of this is production code or oracle evidence.
    struct Transcribed {
        parallel: bool,
    }

    impl Callees for Transcribed {
        fn approx_parallel(&self, _a: &Seg, _b: &Seg) -> CircleResult<bool> {
            Ok(self.parallel)
        }
        fn seg_center(&self, seg: &Seg) -> CircleResult<Vector2I> {
            let half = seg.b.try_sub(seg.a)?;
            Ok(seg.a.try_add(Vector2::new(half.x / 2, half.y / 2))?)
        }
        fn intersect_lines(&self, seg: &Seg, other: &Seg) -> CircleResult<OptVector2I> {
            Ok(seg.intersect(other, false, true)?)
        }
        fn calc_arc_mid(
            &self,
            _s: Vector2I,
            _e: Vector2I,
            _c: Vector2I,
            _m: bool,
        ) -> CircleResult<Vector2I> {
            Pinned.calc_arc_mid(_s, _e, _c, _m)
        }
        fn rotate_point(&self, point: &mut Vector2I, angle: EdaAngle) -> CircleResult<()> {
            let mut angle = angle;
            angle.normalize();
            let (x, y) = (point.x, point.y);
            *point = if angle == ANGLE_0 {
                v(x, y)
            } else if angle == ANGLE_90 {
                v(y, -x)
            } else if angle == ANGLE_180 {
                v(-x, -y)
            } else if angle == ANGLE_270 {
                v(-y, x)
            } else {
                return Pinned.rotate_point(point, angle);
            };
            Ok(())
        }
    }

    #[test]
    fn constructors_and_equality() {
        let d = Circle::default();
        assert_eq!((d.center, d.radius), (v(0, 0), 0));
        let c = Circle::with_center_radius(v(3, -4), 7);
        let copy = Circle::copy_of(&c);
        assert_eq!(copy, c);
        assert_ne!(Circle::with_center_radius(v(3, -4), 8), c);
        assert_ne!(Circle::with_center_radius(v(3, 4), 7), c);
    }

    #[test]
    fn contains_overloads_differ() -> CircleResult<()> {
        let mut c = Circle::with_center_radius(v(0, 0), 100);
        // On the circumference within MIN_PRECISION_IU, inclusive at both ends.
        assert!(c.contains(v(104, 0))?);
        assert!(c.contains(v(96, 0))?);
        assert!(!c.contains(v(105, 0))?);
        assert!(!c.contains(v(95, 0))?);
        assert!(!c.contains(v(0, 0))?);
        // Strictly inside.
        assert!(c.contains_mut(v(0, 0))?);
        assert!(c.contains_mut(v(99, 0))?);
        assert!(!c.contains_mut(v(100, 0))?);
        // int overflow of aP - Center is undefined only in the non-const overload.
        let mut edge = Circle::with_center_radius(v(i32::MIN, 0), 1);
        assert!(!edge.contains(v(i32::MAX, 0))?);
        assert_eq!(
            edge.contains_mut(v(i32::MAX, 0)),
            Err(CircleError::Undefined)
        );
        Ok(())
    }

    #[test]
    fn nearest_and_furthest_points() -> CircleResult<()> {
        let c = Circle::with_center_radius(v(10, 10), 5);
        assert_eq!(c.nearest_point(v(20, 10))?, v(15, 10));
        assert_eq!(c.furthest_point(v(20, 10))?, v(5, 10));
        // At the centre the vector becomes (1, 0) in both.
        assert_eq!(c.nearest_point(v(10, 10))?, v(15, 10));
        assert_eq!(c.furthest_point(v(10, 10))?, v(15, 10));
        let d = c.nearest_point_d(Vector2::new(10.0, -0.0))?;
        assert_eq!((d.x, d.y), (10.0, 5.0));
        let d = c.nearest_point_d(Vector2::new(10.0, 10.0))?;
        assert_eq!((d.x, d.y), (15.0, 10.0));
        let d = c.furthest_point_d(Vector2::new(10.0, 20.0))?;
        assert_eq!((d.x, d.y), (10.0, 5.0));
        let nan = f64::from_bits(0x7ff4_0000_0000_0abc);
        let d = c.nearest_point_d(Vector2::new(nan, 10.0))?;
        assert!(d.x.is_nan());
        Ok(())
    }

    #[test]
    fn circle_intersections_until_rotate_point() {
        let a = Circle::with_center_radius(v(0, 0), 5);
        // Apart, nested and co-centred circles return no points.
        for (other, empty) in [
            (Circle::with_center_radius(v(11, 0), 5), true),
            (Circle::with_center_radius(v(1, 0), 1), true),
            (Circle::with_center_radius(v(0, 0), 5), true),
        ] {
            assert_eq!(a.intersect_circle(&other).map(|p| p.is_empty()), Ok(empty));
        }
        // Points exist: RotatePoint is the first unported call.
        let deferral = unported("RotatePoint(VECTOR2I&, EDA_ANGLE)", TRIGO_HEADER_UNIT);
        assert_eq!(
            a.intersect_circle(&Circle::with_center_radius(v(8, 0), 5)),
            Err(deferral)
        );
        // d = 3037000499 passes both tests, but d * d overflows int64_t: undefined.
        let far = Circle::with_center_radius(v(i32::MAX, i32::MAX), i32::MAX);
        let near = Circle::with_center_radius(v(0, 0), i32::MAX);
        assert_eq!(far.intersect_circle(&near), Err(CircleError::Undefined));
    }

    #[test]
    fn circle_intersection_continuation_after_cardinal_rotation() -> CircleResult<()> {
        let t = Transcribed { parallel: false };
        let a = Circle::with_center_radius(v(0, 0), 5);
        // vecCtoC along +x: angle 0. x = 4, y = 3.
        let points = a.intersect_circle_with(&t, &Circle::with_center_radius(v(8, 0), 5))?;
        assert_eq!(points, vec![v(4, 3), v(4, -3)]);
        // External tangency: y == 0, one point.
        let points = a.intersect_circle_with(&t, &Circle::with_center_radius(v(10, 0), 5))?;
        assert_eq!(points, vec![v(5, 0)]);
        // vecCtoC along +y: rotation by -90 degrees, then translation.
        let b = Circle::with_center_radius(v(100, 100), 5);
        let points = b.intersect_circle_with(&t, &Circle::with_center_radius(v(100, 108), 5))?;
        assert_eq!(points, vec![v(97, 104), v(103, 104)]);
        Ok(())
    }

    #[test]
    fn line_and_segment_intersections() -> CircleResult<()> {
        let c = Circle::with_center_radius(v(0, 0), 100);
        let line = Seg::new(v(-1000, 60), v(1000, 60));
        assert_eq!(c.intersect_line(&line)?, vec![v(80, 60), v(-80, 60)]);
        // Only the end inside the segment survives Intersect( SEG ).
        let half = Seg::new(v(0, 60), v(1000, 60));
        assert_eq!(c.intersect_seg(&half)?, vec![v(80, 60)]);
        // Tangent within MIN_PRECISION_IU, and a miss.
        let tangent = Seg::new(v(-1000, 103), v(1000, 103));
        assert_eq!(c.intersect_line(&tangent)?, vec![v(0, 103)]);
        let miss = Seg::new(v(-1000, 105), v(1000, 105));
        assert!(c.intersect_line(&miss)?.is_empty());
        // A degenerate line projects to its point; the zero direction resizes to zero, so
        // both "intersections" are that point.
        let point = Seg::new(v(30, 40), v(30, 40));
        assert_eq!(c.intersect_line(&point)?, vec![v(30, 40), v(30, 40)]);
        Ok(())
    }

    #[test]
    fn tan_tan_pt_defers_at_its_first_unported_call() {
        let mut c = Circle::with_center_radius(v(1, 2), 3);
        let a = Seg::new(v(0, 0), v(100, 0));
        let b = Seg::new(v(0, 50), v(100, 50));
        assert_eq!(
            c.construct_from_tan_tan_pt(&a, &b, v(10, 25)).map(|c| *c),
            Err(unported("SEG::Square", SEG_HEADER_UNIT))
        );
    }

    #[test]
    fn tan_tan_pt_parallel_continuation() -> CircleResult<()> {
        let t = Transcribed { parallel: true };
        let mut c = Circle::new();
        let a = Seg::new(v(0, 0), v(100, 0));
        let b = Seg::new(v(0, 50), v(100, 50));
        let out = *c.construct_from_tan_tan_pt_with(&t, &a, &b, v(40, 10))?;
        // Radius is half the separation; the centre is on y = 25 at distance 25 from aP,
        // the one closer to aLineA.A.
        assert_eq!(out, c);
        assert_eq!(c.radius, 25);
        assert_eq!(c.center.y, 25);
        assert!(c.center.x < 40);
        Ok(())
    }

    #[test]
    fn tan_tan_pt_general_continuation_until_calc_arc_mid() -> CircleResult<()> {
        let t = Transcribed { parallel: false };
        let a = Seg::new(v(0, 0), v(100, 0));
        let b = Seg::new(v(0, 0), v(0, 100));
        // aP at the intersection: centre aP, radius 0.
        let mut c = Circle::with_center_radius(v(7, 7), 9);
        c.construct_from_tan_tan_pt_with(&t, &a, &b, v(0, 0))?;
        assert_eq!(c, Circle::with_center_radius(v(0, 0), 0));
        // Otherwise CalcArcMid is reached and defers.
        let mut c = Circle::new();
        assert_eq!(
            c.construct_from_tan_tan_pt_with(&t, &a, &b, v(10, 20))
                .map(|c| *c),
            Err(unported("CalcArcMid", TRIGO_UNIT))
        );
        Ok(())
    }
}
