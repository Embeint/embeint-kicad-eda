// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/src/geometry/seg.cpp.
// Copyright (C) 2013 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SEG` (libs/kimath/src/geometry/seg.cpp) at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Every out-of-line body of seg.cpp is ported in source order of evaluation. Coordinates stay
//! 32-bit integer nanometres and `ecoord` is `int64_t`. Signed overflow, negation of `INT_MIN`
//! and an out-of-range floating-to-integer cast are undefined in the C++; they are refused with
//! [`SegError::Undefined`] at the first such operation instead of being given a value.
//! Conversions the C++ defines (C++20 modular `int64_t -> int`, the clamping `VECTOR2` converting
//! constructor) keep their defined results.
//!
//! The [`Seg`] type itself (members, constructors, `operator==`, `Index`) is a minimum type
//! co-port from `seg.h`, owned by `libs.kimath.include.geometry.application.ui.p003`. That
//! header's function bodies (`Square`, `SquaredLength`, and the rest) stay with their owner:
//! the two this file calls return a typed `Unported` deferral.
use crate::eda_angle::{EdaAngle, GeometryError};
use crate::math_util;
use crate::util::{self, isqrt, ki_round};
use crate::vector::{ki_round_xy, Vector2, Vector2D, Vector2I, Vector2L, VectorDomainError};
use crate::wide_int::{cross_wide, to_double};

/// This unit.
pub const UNIT: &str = "libs.kimath.src.geometry.seg.application.ui.p001";
/// Owner of the `seg.h` inline bodies.
pub const HEADER_UNIT: &str = "libs.kimath.include.geometry.application.ui.p003";

/// `SEG::ecoord`, `VECTOR2I::extended_type`.
pub type Ecoord = i64;
/// `OPT_VECTOR2I`.
pub type OptVector2I = Option<Vector2I>;

/// Why a SEG operation produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegError {
    /// The pinned C++ evaluation is undefined for this input (signed overflow, negation of the
    /// minimum, a floating-to-integer cast out of range or a zero integral divisor).
    Undefined,
    /// A dependency call that is not yet ported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

pub type SegResult<T> = Result<T, SegError>;

impl From<VectorDomainError> for SegError {
    fn from(error: VectorDomainError) -> Self {
        match error {
            VectorDomainError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

impl From<GeometryError> for SegError {
    fn from(error: GeometryError) -> Self {
        match error {
            GeometryError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

impl From<math_util::MathUtilError> for SegError {
    fn from(error: math_util::MathUtilError) -> Self {
        match error {
            math_util::MathUtilError::Unported { upstream, unit } => {
                Self::Unported { upstream, unit }
            }
            _ => Self::Undefined,
        }
    }
}

fn checked<T>(value: Option<T>) -> SegResult<T> {
    value.ok_or(SegError::Undefined)
}

/// `rescale<int64_t>` (util.cpp, Linux `__int128` branch).
fn rescale64(numerator: i64, value: i64, denominator: i64) -> SegResult<i64> {
    Ok(math_util::rescale_i64_int128(
        numerator,
        value,
        denominator,
    )?)
}

/// `template <typename T> int sgn( T aVal )`: `( T( 0 ) < aVal ) - ( aVal < T( 0 ) )`.
pub fn sgn<T: Copy + Default + PartialOrd>(value: T) -> i32 {
    i32::from(T::default() < value) - i32::from(value < T::default())
}

/// `VECTOR2L( v )` from a `VECTOR2I`: exact widening.
fn wide(v: Vector2I) -> Vector2L {
    Vector2::new(i64::from(v.x), i64::from(v.y))
}

/// `VECTOR2L( a ) - b` with `b` a `VECTOR2I`: `int64_t` components, which cannot overflow.
fn wide_sub(a: Vector2I, b: Vector2I) -> Vector2L {
    Vector2::new(
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    )
}

/// The implicit `VECTOR2<ecoord> -> VECTOR2I` conversion of a return statement: the integral
/// converting constructor clamps each component to the `int` range.
fn narrow(x: i64, y: i64) -> Vector2I {
    let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Vector2::new(clamp(x), clamp(y))
}

/// `int( v )` of an `int64_t`: C++20 modular conversion.
fn modular_int(v: i64) -> i32 {
    v as i32
}

/// `isqrt<int64_t>` (util.h).
fn isqrt64(v: i64) -> SegResult<i64> {
    Ok(isqrt(v)?)
}

/// `SEG::Square` (seg.h), owned by the header unit.
fn square(_value: i32) -> SegResult<Ecoord> {
    // UNPORTED(libs.kimath.include.geometry.application.ui.p003): SEG::Square
    Err(SegError::Unported {
        upstream: "SEG::Square",
        unit: HEADER_UNIT,
    })
}

/// `SEG::SquaredLength` (seg.h), owned by the header unit.
fn squared_length(_seg: &Seg) -> SegResult<Ecoord> {
    // UNPORTED(libs.kimath.include.geometry.application.ui.p003): SEG::SquaredLength
    Err(SegError::Unported {
        upstream: "SEG::SquaredLength",
        unit: HEADER_UNIT,
    })
}

/// `SEG`: a segment from `a` to `b`, with the index it has within a parent shape.
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub a: Vector2I,
    pub b: Vector2I,
    index: i32,
}

impl Default for Seg {
    /// `SEG()`: zero points, index -1.
    fn default() -> Self {
        Self::new(Vector2::default(), Vector2::default())
    }
}

impl PartialEq for Seg {
    /// `SEG::operator==` compares the points only, never the index.
    fn eq(&self, other: &Self) -> bool {
        self.a == other.a && self.b == other.b
    }
}

impl Seg {
    /// `SEG( const VECTOR2I& aA, const VECTOR2I& aB )`: index -1.
    pub const fn new(a: Vector2I, b: Vector2I) -> Self {
        Self { a, b, index: -1 }
    }

    /// `SEG( int aX1, int aY1, int aX2, int aY2 )`: index -1.
    pub const fn from_coords(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        Self::new(Vector2::new(x1, y1), Vector2::new(x2, y2))
    }

    /// `SEG( const VECTOR2I& aA, const VECTOR2I& aB, int aIndex )`.
    pub const fn with_index(a: Vector2I, b: Vector2I, index: i32) -> Self {
        Self { a, b, index }
    }

    /// `SEG::Index`.
    pub const fn index(&self) -> i32 {
        self.index
    }

    /// `SEG::SquaredDistance( const SEG& aSeg )`.
    pub fn squared_distance_seg(&self, seg: &Seg) -> SegResult<Ecoord> {
        // Zero-length segments (points) are handled first: the cross product with a zero
        // vector is always zero, which Intersects() would take for an intersection.
        if self.a == self.b {
            return seg.squared_distance_point(self.a);
        }
        if seg.a == seg.b {
            return self.squared_distance_point(seg.a);
        }
        if self.intersects(seg)? {
            return Ok(0);
        }
        // The braced initializer list evaluates its elements in order.
        let pts = [
            seg.nearest_point_point(self.a)?.try_sub(self.a)?,
            seg.nearest_point_point(self.b)?.try_sub(self.b)?,
            self.nearest_point_point(seg.a)?.try_sub(seg.a)?,
            self.nearest_point_point(seg.b)?.try_sub(seg.b)?,
        ];
        let mut m = Vector2I::ECOORD_MAX;
        for p in pts {
            m = util::min(m, p.squared_euclidean_norm()?);
        }
        Ok(m)
    }

    /// `SEG::Angle`.
    pub fn angle(&self, other: &Seg) -> SegResult<EdaAngle> {
        let this_angle =
            EdaAngle::from_vector(to_vector2d(self.a.try_sub(self.b)?))?.normalize180();
        let other_angle =
            EdaAngle::from_vector(to_vector2d(other.a.try_sub(other.b)?))?.normalize180();
        Ok((this_angle - other_angle).normalize180().abs())
    }

    /// `SEG::NearestPoint( const SEG& aSeg )`.
    pub fn nearest_point_seg(&self, seg: &Seg) -> SegResult<Vector2I> {
        if let Some(p) = self.intersect(seg, false, false)? {
            return Ok(p);
        }
        let origin = [
            seg.nearest_point_point(self.a)?,
            seg.nearest_point_point(self.b)?,
            self.nearest_point_point(seg.a)?,
            self.nearest_point_point(seg.b)?,
        ];
        let out = [self.a, self.b, origin[2], origin[3]];
        let dist = [
            origin[0].try_sub(self.a)?.squared_euclidean_norm()?,
            origin[1].try_sub(self.b)?.squared_euclidean_norm()?,
            origin[2].try_sub(seg.a)?.squared_euclidean_norm()?,
            origin[3].try_sub(seg.b)?.squared_euclidean_norm()?,
        ];
        Ok(out[min_index(&dist)])
    }

    /// `SEG::NearestPoints`: always returns true; the out-parameters become the nearest point
    /// of this segment, of `seg` and their squared distance.
    pub fn nearest_points(
        &self,
        seg: &Seg,
        pt_a: &mut Vector2I,
        pt_b: &mut Vector2I,
        dist_sq: &mut i64,
    ) -> SegResult<bool> {
        if let Some(p) = self.intersect(seg, false, false)? {
            *pt_a = p;
            *pt_b = p;
            *dist_sq = 0;
            return Ok(true);
        }
        let origin = [
            seg.nearest_point_point(self.a)?,
            seg.nearest_point_point(self.b)?,
            self.nearest_point_point(seg.a)?,
            self.nearest_point_point(seg.b)?,
        ];
        let a_out = [self.a, self.b, origin[2], origin[3]];
        let b_out = [origin[0], origin[1], seg.a, seg.b];
        let dist = [
            origin[0].try_sub(self.a)?.squared_euclidean_norm()?,
            origin[1].try_sub(self.b)?.squared_euclidean_norm()?,
            origin[2].try_sub(seg.a)?.squared_euclidean_norm()?,
            origin[3].try_sub(seg.b)?.squared_euclidean_norm()?,
        ];
        let min_i = min_index(&dist);
        *pt_a = a_out[min_i];
        *pt_b = b_out[min_i];
        *dist_sq = dist[min_i];
        Ok(true)
    }

    /// `SEG::checkCollinearOverlap` (private): overlap of two collinear segments projected on
    /// the x or y axis, with the midpoint of the overlap written to `pt` when requested.
    fn check_collinear_overlap(
        &self,
        seg: &Seg,
        use_x_axis: bool,
        ignore_endpoints: bool,
        pt: Option<&mut Vector2I>,
    ) -> SegResult<bool> {
        let (seg1_start, seg1_end, seg2_start, seg2_end, coord1_start, coord1_end) = if use_x_axis {
            (self.a.x, self.b.x, seg.a.x, seg.b.x, self.a.y, self.b.y)
        } else {
            (self.a.y, self.b.y, seg.a.y, seg.b.y, self.a.x, self.b.x)
        };
        let seg1_min = util::min(seg1_start, seg1_end);
        let seg1_max = util::max(seg1_start, seg1_end);
        let seg2_min = util::min(seg2_start, seg2_end);
        let seg2_max = util::max(seg2_start, seg2_end);

        let overlaps = seg1_max >= seg2_min && seg2_max >= seg1_min;
        if !overlaps {
            return Ok(false);
        }

        if ignore_endpoints {
            let overlap_start = util::max(seg1_min, seg2_min);
            let overlap_end = util::min(seg1_max, seg2_max);
            // A zero-length overlap that is an endpoint of both segments is only a touch.
            if overlap_start == overlap_end {
                let mut is_endpoint_touch = false;
                if (overlap_start == seg1_min || overlap_start == seg1_max)
                    && (overlap_start == seg2_min || overlap_start == seg2_max)
                {
                    is_endpoint_touch = true;
                }
                if is_endpoint_touch {
                    return Ok(false);
                }
            }
        }

        if let Some(pt) = pt {
            let overlap_start = util::max(seg1_min, seg2_min);
            let overlap_end = util::min(seg1_max, seg2_max);
            // int addition, then int division truncating toward zero.
            let intersection_proj = checked(overlap_start.checked_add(overlap_end))? / 2;

            let intersection_other = if seg1_end != seg1_start {
                // The other coordinate on this segment's line, through rescale<int>.
                let n = checked(intersection_proj.checked_sub(seg1_start))?;
                let v = checked(coord1_end.checked_sub(coord1_start))?;
                let d = checked(seg1_end.checked_sub(seg1_start))?;
                let r = math_util::rescale_i32(n, v, d)?;
                checked(coord1_start.checked_add(r))?
            } else {
                // Degenerate segment (point) or perpendicular to the projection axis.
                coord1_start
            };

            *pt = if use_x_axis {
                Vector2::new(intersection_proj, intersection_other)
            } else {
                Vector2::new(intersection_other, intersection_proj)
            };
        }
        Ok(true)
    }

    /// `SEG::intersects` (private), with its defaults `aIgnoreEndpoints = false`,
    /// `aLines = false` and `aPt = nullptr` taken explicitly.
    fn intersects_impl(
        &self,
        seg: &Seg,
        ignore_endpoints: bool,
        lines: bool,
        pt: Option<&mut Vector2I>,
    ) -> SegResult<bool> {
        // Bounding-box rejection; infinite lines can intersect anywhere.
        if !lines {
            let this_min_x = util::min(self.a.x, self.b.x);
            let this_max_x = util::max(self.a.x, self.b.x);
            let this_min_y = util::min(self.a.y, self.b.y);
            let this_max_y = util::max(self.a.y, self.b.y);
            let other_min_x = util::min(seg.a.x, seg.b.x);
            let other_max_x = util::max(seg.a.x, seg.b.x);
            let other_min_y = util::min(seg.a.y, seg.b.y);
            let other_max_y = util::max(seg.a.y, seg.b.y);
            if this_max_x < other_min_x
                || other_max_x < this_min_x
                || this_max_y < other_min_y
                || other_max_y < this_min_y
            {
                return Ok(false);
            }
        }

        // Parametric form: P1 = A + t * dir1, P2 = aSeg.A + s * dir2.
        let dir1 = wide_sub(self.b, self.a);
        let dir2 = wide_sub(seg.b, seg.a);
        let offset = wide_sub(seg.a, self.a);
        let determinant = dir2.cross(dir1)?;

        if determinant == 0 {
            // Parallel; collinear only when the offset is parallel too.
            let collinear_test = dir1.cross(offset)?;
            if collinear_test != 0 {
                return Ok(false);
            }
            if lines {
                // Infinite collinear lines always intersect; the point is ambiguous.
                if let Some(pt) = pt {
                    *pt = if seg.a == seg.b {
                        seg.a
                    } else if self.a == self.b {
                        self.a
                    } else {
                        // VECTOR2I + VECTOR2I, then operator/( double ): KiROUND( x / 2.0 ).
                        self.a.try_add(seg.a)?.divide(2.0)?
                    };
                }
                return Ok(true);
            }
            // Segments: test the overlap on the axis with the larger range. Neither component
            // of dir1 can be INT64_MIN, so std::abs is defined.
            let use_x_axis = dir1.x.abs() >= dir1.y.abs();
            return self.check_collinear_overlap(seg, use_x_axis, ignore_endpoints, pt);
        }

        // s = param2_num / d and t = param1_num / d.
        let param2_num = dir2.cross(offset)?;
        let param1_num = dir1.cross(offset)?;

        if !lines {
            // Both parameters must lie in [0, 1]; compare numerators by the sign of d.
            if determinant > 0 {
                if param1_num < 0
                    || param1_num > determinant
                    || param2_num < 0
                    || param2_num > determinant
                {
                    return Ok(false);
                }
            } else if param1_num > 0
                || param1_num < determinant
                || param2_num > 0
                || param2_num < determinant
            {
                return Ok(false);
            }

            // Optionally exclude intersections at shared endpoints.
            if ignore_endpoints
                && (param1_num == 0 || param1_num == determinant)
                && (param2_num == 0 || param2_num == determinant)
            {
                return Ok(false);
            }
        }

        if let Some(pt) = pt {
            // intersection = aSeg.A + ( q / d ) * f
            let scaled_x = rescale64(param1_num, dir2.x, determinant)?;
            let scaled_y = rescale64(param1_num, dir2.y, determinant)?;
            let result_x = checked(i64::from(seg.a.x).checked_add(scaled_x))?;
            let result_y = checked(i64::from(seg.a.y).checked_add(scaled_y))?;
            let max_coord = i64::from(i32::MAX);
            let min_coord = i64::from(i32::MIN);
            if result_x > max_coord
                || result_x < min_coord
                || result_y > max_coord
                || result_y < min_coord
            {
                // The intersection exists but its coordinates overflow.
                return Ok(false);
            }
            *pt = Vector2::new(result_x as i32, result_y as i32);
        }
        Ok(true)
    }

    /// `SEG::Intersects`.
    pub fn intersects(&self, seg: &Seg) -> SegResult<bool> {
        self.intersects_impl(seg, false, false, None)
    }

    /// `SEG::Intersect`; the C++ defaults are `aIgnoreEndpoints = false`, `aLines = false`.
    pub fn intersect(
        &self,
        seg: &Seg,
        ignore_endpoints: bool,
        lines: bool,
    ) -> SegResult<OptVector2I> {
        let mut ip = Vector2I::default();
        if self.intersects_impl(seg, ignore_endpoints, lines, Some(&mut ip))? {
            Ok(Some(ip))
        } else {
            Ok(None)
        }
    }

    /// `SEG::IntersectsLine`: the intersection with `y = slope * x + offset`. `intersection` is
    /// written only when the result is true.
    pub fn intersects_line(
        &self,
        slope: f64,
        offset: f64,
        intersection: &mut Vector2I,
    ) -> SegResult<bool> {
        let seg_a = wide(self.a);
        let seg_b = wide(self.b);
        let seg_dir = Vector2::new(seg_b.x - seg_a.x, seg_b.y - seg_a.y);

        if seg_dir.x == 0 {
            // Vertical segment: x = A.x, the y of the line there.
            let intersect_y = slope * f64::from(self.a.x) + offset;
            // Compare before rounding: a miss can land anywhere on the unbounded line.
            let seg_min_y = util::min(self.a.y, self.b.y);
            let seg_max_y = util::max(self.a.y, self.b.y);
            if intersect_y >= f64::from(seg_min_y) && intersect_y <= f64::from(seg_max_y) {
                *intersection = Vector2::new(self.a.x, util::round_i32(intersect_y)?);
                return Ok(true);
            }
            return Ok(false);
        }

        let line_dir = Vector2::new(1000_i64, cast_ecoord(slope * 1000.0)?);
        let cross_product = seg_dir.cross(line_dir)?;

        if cross_product == 0 {
            // Parallel: on the line when A is within half a unit of it.
            let expected_y = slope * f64::from(self.a.x) + offset;
            let diff = (f64::from(self.a.y) - expected_y).abs();
            if diff < 0.5 {
                // Collinear: the midpoint, VECTOR2I + VECTOR2I then KiROUND( x / 2.0 ).
                *intersection = self.a.try_add(self.b)?.divide(2.0)?;
                return Ok(true);
            }
            return Ok(false);
        }

        // t = ( aSlope * segA.x + aOffset - segA.y ) / ( segDir.y - aSlope * segDir.x )
        let numerator = slope * seg_a.x as f64 + offset - seg_a.y as f64;
        let denominator = seg_dir.y as f64 - slope * seg_dir.x as f64;
        let t = numerator / denominator;

        if (0.0..=1.0).contains(&t) {
            *intersection = ki_round_xy(
                seg_a.x as f64 + t * seg_dir.x as f64,
                seg_a.y as f64 + t * seg_dir.y as f64,
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    /// `SEG::PerpendicularSeg`: from `p`, along this segment's direction turned by 90 degrees.
    pub fn perpendicular_seg(&self, p: Vector2I) -> SegResult<Seg> {
        let slope = self.b.try_sub(self.a)?;
        let end_point = slope.perpendicular()?.try_add(p)?;
        Ok(Seg::new(p, end_point))
    }

    /// `SEG::ParallelSeg`: from `p`, along this segment's direction.
    pub fn parallel_seg(&self, p: Vector2I) -> SegResult<Seg> {
        let slope = self.b.try_sub(self.a)?;
        let end_point = slope.try_add(p)?;
        Ok(Seg::new(p, end_point))
    }

    /// `SEG::Collide`; `actual` is the C++ `aActual` out-parameter (default `nullptr`).
    pub fn collide(
        &self,
        seg: &Seg,
        clearance: i32,
        mut actual: Option<&mut i32>,
    ) -> SegResult<bool> {
        let mut set_actual = |value: i32| {
            if let Some(actual) = actual.as_deref_mut() {
                *actual = value;
            }
        };

        if clearance < 0 {
            set_actual(0);
            return Ok(false);
        }

        // Zero-length segments (points) first, as in SquaredDistance.
        if self.a == self.b {
            let dist = seg.distance_point(self.a)?;
            set_actual(dist);
            return Ok(dist == 0 || dist < clearance);
        }
        if seg.a == seg.b {
            let dist = self.distance_point(seg.a)?;
            set_actual(dist);
            return Ok(dist == 0 || dist < clearance);
        }

        if self.intersects_impl(seg, false, false, None)? {
            set_actual(0);
            return Ok(true);
        }

        let clearance_sq = i64::from(clearance) * i64::from(clearance);
        let mut min_dist_sq = Vector2I::ECOORD_MAX;

        // The four endpoint distances, short-circuiting on the first zero.
        let candidates: [&dyn Fn() -> SegResult<Ecoord>; 4] = [
            &|| self.squared_distance_point(seg.a),
            &|| self.squared_distance_point(seg.b),
            &|| seg.squared_distance_point(self.a),
            &|| seg.squared_distance_point(self.b),
        ];
        for candidate in candidates {
            let dist = candidate()?;
            if dist == 0 {
                set_actual(0);
                return Ok(true);
            }
            min_dist_sq = util::min(min_dist_sq, dist);
        }

        if min_dist_sq < clearance_sq {
            set_actual(modular_int(isqrt64(min_dist_sq)?));
            return Ok(true);
        }
        set_actual(modular_int(isqrt64(min_dist_sq)?));
        Ok(false)
    }

    /// `SEG::Contains( const VECTOR2I&, int )`; the C++ default threshold is 3.
    pub fn contains_point(&self, p: Vector2I, sq_distance_threshold: i32) -> SegResult<bool> {
        // Upstream notes that this is not exact for all points (the threshold absorbs rounding).
        Ok(self.squared_distance_point(p)? <= i64::from(sq_distance_threshold))
    }

    /// `SEG::NearestPoint( const VECTOR2I& aP )`.
    pub fn nearest_point_point(&self, p: Vector2I) -> SegResult<Vector2I> {
        let d = wide_sub(self.b, self.a);
        let l_squared = checked(
            d.x.checked_mul(d.x)
                .and_then(|x| d.y.checked_mul(d.y).and_then(|y| x.checked_add(y))),
        )?;
        if l_squared == 0 {
            return Ok(self.a);
        }
        let pa = wide_sub(p, self.a);
        let t = d.dot(pa)?;
        if t < 0 {
            return Ok(self.a);
        } else if t > l_squared {
            return Ok(self.b);
        }
        let xp = rescale64(t, d.x, l_squared)?;
        let yp = rescale64(t, d.y, l_squared)?;
        Ok(narrow(
            checked(i64::from(self.a.x).checked_add(xp))?,
            checked(i64::from(self.a.y).checked_add(yp))?,
        ))
    }

    /// `SEG::ReflectPoint`: the mirror image of `p` in this segment's line.
    pub fn reflect_point(&self, p: Vector2I) -> SegResult<Vector2I> {
        let d = self.b.try_sub(self.a)?;
        let l_squared = d.dot(d)?;
        let t = d.dot(p.try_sub(self.a)?)?;
        let c = if l_squared == 0 {
            wide(p)
        } else {
            let x = rescale64(t, i64::from(d.x), l_squared)?;
            let cx = checked(i64::from(self.a.x).checked_add(x))?;
            let y = rescale64(t, i64::from(d.y), l_squared)?;
            let cy = checked(i64::from(self.a.y).checked_add(y))?;
            Vector2::new(cx, cy)
        };
        let rx = checked(
            c.x.checked_mul(2)
                .and_then(|v| v.checked_sub(i64::from(p.x))),
        )?;
        let ry = checked(
            c.y.checked_mul(2)
                .and_then(|v| v.checked_sub(i64::from(p.y))),
        )?;
        Ok(narrow(rx, ry))
    }

    /// `SEG::LineProject`: the projection of `p` on this segment's line.
    pub fn line_project(&self, p: Vector2I) -> SegResult<Vector2I> {
        let d = self.b.try_sub(self.a)?;
        let l_squared = d.dot(d)?;
        if l_squared == 0 {
            return Ok(self.a);
        }
        let t = d.dot(p.try_sub(self.a)?)?;
        let xp = rescale64(t, i64::from(d.x), l_squared)?;
        let yp = rescale64(t, i64::from(d.y), l_squared)?;
        Ok(narrow(
            checked(i64::from(self.a.x).checked_add(xp))?,
            checked(i64::from(self.a.y).checked_add(yp))?,
        ))
    }

    /// `SEG::Distance( const SEG& )`.
    pub fn distance_seg(&self, seg: &Seg) -> SegResult<i32> {
        Ok(modular_int(isqrt64(self.squared_distance_seg(seg)?)?))
    }

    /// `SEG::Distance( const VECTOR2I& )`.
    pub fn distance_point(&self, p: Vector2I) -> SegResult<i32> {
        Ok(modular_int(isqrt64(self.squared_distance_point(p)?)?))
    }

    /// `SEG::SquaredDistance( const VECTOR2I& aP )`.
    // `!( g >= 0 )` is kept verbatim: it is also true for a NaN.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn squared_distance_point(&self, p: Vector2I) -> SegResult<Ecoord> {
        let ab = wide_sub(self.b, self.a);
        let ap = wide_sub(p, self.a);

        let e = ap.dot(ab)?;
        if e <= 0 {
            return Ok(ap.squared_euclidean_norm()?);
        }

        let f = ab.squared_euclidean_norm()?;
        if e >= f {
            let bp = wide_sub(p, self.b);
            return Ok(bp.dot(bp)?);
        }

        // The Lagrange identity keeps the small result free of cancellation.
        let cross = to_double(cross_wide(&ab, &ap).map_err(|_| SegError::Undefined)?);
        let g = cross * cross / f as f64;

        if !(g >= 0.0) || g > i64::MAX as f64 {
            return Ok(0);
        }
        Ok(ki_round::<f64, i64>(g, false)?)
    }

    /// `SEG::LineDistance`; the C++ default for `aDetermineSide` is false.
    pub fn line_distance(&self, p: Vector2I, determine_side: bool) -> SegResult<i32> {
        let (pc, q, r, l) = canonical(self)?;
        let det = line_det(pc, q, r, p)?;
        let mut dist_sq = 0;
        if l > 0 {
            dist_sq = rescale64(det, det, l)?;
        }
        let dist = isqrt64(dist_sq)?;
        let value = if determine_side {
            checked(i64::from(sgn(det)).checked_mul(dist))?
        } else {
            checked(dist.checked_abs())?
        };
        Ok(modular_int(value))
    }

    /// `SEG::mutualDistanceSquared` (private): signed squared distances of the shorter
    /// segment's endpoints from the longer one's line; `None` when that line is a point.
    fn mutual_distance_squared(&self, seg: &Seg) -> SegResult<Option<(Ecoord, Ecoord)>> {
        let mut a = *self;
        let mut b = *seg;
        if squared_length(&a)? < squared_length(&b)? {
            core::mem::swap(&mut a, &mut b);
        }
        let (p, q, r, l) = canonical(&a)?;
        if l == 0 {
            return Ok(None);
        }
        let det1 = line_det(p, q, r, b.a)?;
        let det2 = line_det(p, q, r, b.b)?;
        let dsq1 = rescale64(det1, det1, l)?;
        let dsq2 = rescale64(det2, det2, l)?;
        let d1 = checked(i64::from(sgn(det1)).checked_mul(dsq1))?;
        let d2 = checked(i64::from(sgn(det2)).checked_mul(dsq2))?;
        Ok(Some((d1, d2)))
    }

    /// `SEG::ApproxCollinear`; the C++ default threshold is 1.
    pub fn approx_collinear(&self, seg: &Seg, distance_threshold: i32) -> SegResult<bool> {
        let threshold_squared = square(distance_threshold)?;
        let Some((d1_sq, d2_sq)) = self.mutual_distance_squared(seg)? else {
            return Ok(false);
        };
        // std::abs of INT64_MIN is undefined; the second abs runs only after the first passes.
        Ok(checked(d1_sq.checked_abs())? <= threshold_squared
            && checked(d2_sq.checked_abs())? <= threshold_squared)
    }

    /// `SEG::ApproxParallel`; the C++ default threshold is 1.
    pub fn approx_parallel(&self, seg: &Seg, distance_threshold: i32) -> SegResult<bool> {
        let threshold_squared = square(distance_threshold)?;
        let Some((d1_sq, d2_sq)) = self.mutual_distance_squared(seg)? else {
            return Ok(false);
        };
        Ok(checked(d1_sq.checked_sub(d2_sq).and_then(i64::checked_abs))? <= threshold_squared)
    }

    /// `SEG::ApproxPerpendicular`.
    pub fn approx_perpendicular(&self, seg: &Seg) -> SegResult<bool> {
        let perp = self.perpendicular_seg(self.a)?;
        seg.approx_parallel(&perp, 1)
    }
}

/// `VECTOR2D( VECTOR2I )`: the floating converting constructor, an exact `static_cast`.
fn to_vector2d(v: Vector2I) -> Vector2D {
    Vector2::new(f64::from(v.x), f64::from(v.y))
}

/// `static_cast<ecoord>( double )`: defined only when the truncated value is representable.
fn cast_ecoord(v: f64) -> SegResult<Ecoord> {
    if v.is_nan() {
        return Err(SegError::Undefined);
    }
    let t = v.trunc();
    // -2^63 is exact; 2^63 is the first value past the range.
    if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&t) {
        return Err(SegError::Undefined);
    }
    Ok(t as i64)
}

/// The first index of the smallest distance (`pts_dist[i] < pts_dist[min_i]`).
fn min_index(dist: &[Ecoord; 4]) -> usize {
    let mut min_i = 0;
    for (i, d) in dist.iter().enumerate() {
        if *d < dist[min_i] {
            min_i = i;
        }
    }
    min_i
}

/// LineDistance's and mutualDistanceSquared's line coefficients, in source order:
/// `p = A.y - B.y`, `q = B.x - A.x`, `r = -p * A.x - q * A.y`, `l = p * p + q * q`.
fn canonical(seg: &Seg) -> SegResult<(Ecoord, Ecoord, Ecoord, Ecoord)> {
    let p = i64::from(seg.a.y) - i64::from(seg.b.y);
    let q = i64::from(seg.b.x) - i64::from(seg.a.x);
    let r = checked(
        p.checked_neg()
            .and_then(|np| np.checked_mul(i64::from(seg.a.x)))
            .and_then(|left| {
                q.checked_mul(i64::from(seg.a.y))
                    .and_then(|right| left.checked_sub(right))
            }),
    )?;
    let l = checked(
        p.checked_mul(p)
            .and_then(|pp| q.checked_mul(q).and_then(|qq| pp.checked_add(qq))),
    )?;
    Ok((p, q, r, l))
}

/// `p * aP.x + q * aP.y + r`, left to right.
fn line_det(p: Ecoord, q: Ecoord, r: Ecoord, point: Vector2I) -> SegResult<Ecoord> {
    checked(
        p.checked_mul(i64::from(point.x))
            .and_then(|x| {
                q.checked_mul(i64::from(point.y))
                    .and_then(|y| x.checked_add(y))
            })
            .and_then(|s| s.checked_add(r)),
    )
}
