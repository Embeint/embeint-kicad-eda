// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad geometry/arc_construction.h / arc_construction.cpp, copyright The KiCad
// Developers.
//! `KIGEOM` arc construction helpers at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Unit `libs.kimath.src.geometry.arc_construction.application.ui.p001` owns every definition of
//! `geometry/arc_construction.cpp`: the file-local `crossZ`, `toIU` and `buildOnChord`, and the
//! public `ProjectToChordBisector`, `ArcThroughPoints`, `ArcFromStartEndMidDrag`,
//! `ArcFromStartEndCenterDrag` and `ArcFromStartTangentEnd`. The `ARC_SOLUTION` aggregate and its
//! default member initializers are declared in `geometry/arc_construction.h`, owned by
//! `libs.kimath.include.geometry.application.pure.p001`; [`ArcSolution`] co-ports those fields
//! and defaults only, as recorded in the owning unit's notes.
//!
//! Arithmetic follows the source expression order without contraction. `crossZ` calls `std::fma`
//! explicitly, so it uses the correctly rounded [`f64::mul_add`], which is the single IEEE fused
//! multiply-add result on every target. `VECTOR2<double>` operations go through the canonical
//! vector bodies (`EuclideanNorm`, `SquaredEuclideanNorm`, `Dot`, `operator+`,
//! `operator*( VECTOR2, scalar )`); the direct `std::hypot` call uses the shared glibc 2.39
//! finite-input backend, which the reference also selects for finite operands. Every value fed to
//! that direct call is finite (it derives from 32-bit integer points).
//!
//! A `VECTOR2<double>::EuclideanNorm` of a non-finite tangent still reaches the vector owner's
//! deferral (`UNPORTED(libs.kimath.include.math.application.ui.p002)`); the error is returned
//! unchanged and the C++ continuation after it (the tangent length tests) is implemented.
use crate::util::round_i32;
use crate::{hypot, Vector2, Vector2D, Vector2I, VectorDomainError};

pub const UNIT: &str = "libs.kimath.src.geometry.arc_construction.application.ui.p001";

pub type Result<T> = core::result::Result<T, VectorDomainError>;

/// `KIGEOM::ARC_SOLUTION`. Every field is observable: a refused construction can still carry the
/// given start and end and, for an apex on the chord, the rounded midpoint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArcSolution {
    /// False for degenerate input (collinear points, zero chord, etc).
    pub valid: bool,
    pub start: Vector2I,
    /// The point on the arc halfway along the sweep.
    pub mid: Vector2I,
    pub end: Vector2I,
    pub center: Vector2D,
    pub radius: f64,
}

/// `crossZ`: the cross product z with the rounding error compensated, for near-collinear tests
/// on large coordinates. Both `std::fma` calls are explicit in the source.
fn cross_z(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let p = ay * bx;
    let e = ay.mul_add(bx, -p);
    ax.mul_add(by, -p) - e
}

/// `toIU`: rounds to internal units, failing when the value does not fit. `None` is the C++
/// `false` return, which leaves the output untouched.
fn to_iu(pt: Vector2D) -> Result<Option<Vector2I>> {
    // std::numeric_limits<int>::max() - 1 is an int expression, converted exactly to double.
    const LIMIT: f64 = (i32::MAX - 1) as f64;

    if !pt.x.is_finite() || !pt.y.is_finite() || pt.x.abs() > LIMIT || pt.y.abs() > LIMIT {
        return Ok(None);
    }

    // KiROUND( x ) then KiROUND( y ); both are within the non-logging range here.
    Ok(Some(Vector2::new(round_i32(pt.x)?, round_i32(pt.y)?)))
}

/// `VECTOR2D chord( double( aEnd.x ) - aStart.x, double( aEnd.y ) - aStart.y )`.
fn chord_of(start: Vector2I, end: Vector2I) -> Vector2D {
    Vector2::new(
        f64::from(end.x) - f64::from(start.x),
        f64::from(end.y) - f64::from(start.y),
    )
}

/// `VECTOR2D chordMid( ( double( aStart.x ) + aEnd.x ) * 0.5, ( double( aStart.y ) + aEnd.y ) * 0.5 )`.
fn chord_mid_of(start: Vector2I, end: Vector2I) -> Vector2D {
    Vector2::new(
        (f64::from(start.x) + f64::from(end.x)) * 0.5,
        (f64::from(start.y) + f64::from(end.y)) * 0.5,
    )
}

/// `buildOnChord`. `side` is +1 for an arc on the (-dy, dx) side of start to end, and
/// `center_t` is the center offset along that normal.
fn build_on_chord(
    start: Vector2I,
    end: Vector2I,
    side: f64,
    center_t: f64,
    radius: f64,
) -> Result<ArcSolution> {
    let mut sol = ArcSolution {
        start,
        end,
        ..ArcSolution::default()
    };

    let chord = chord_of(start, end);
    let len = chord.euclidean_norm()?;

    if len <= 0.0 || !radius.is_finite() || radius <= 0.0 {
        return Ok(sol);
    }

    let normal = Vector2::new(-chord.y / len, chord.x / len);
    let chord_mid = chord_mid_of(start, end);
    let half = len * 0.5;
    let offset = center_t.abs();

    // radius - offset cancels for shallow arcs, so use the equivalent half^2 / ( radius + offset )
    let height = if side * center_t <= 0.0 {
        half * half / (radius + offset)
    } else {
        radius + offset
    };

    match to_iu(chord_mid.try_add(normal.try_mul_scalar(side * height)?)?)? {
        Some(mid) => sol.mid = mid,
        None => return Ok(sol),
    }

    // An apex within rounding of the chord cannot be stored as an arc
    if cross_z(
        chord.x,
        chord.y,
        f64::from(sol.mid.x) - f64::from(start.x),
        f64::from(sol.mid.y) - f64::from(start.y),
    ) == 0.0
    {
        return Ok(sol);
    }

    sol.center = chord_mid.try_add(normal.try_mul_scalar(center_t)?)?;
    sol.radius = radius;
    sol.valid = true;
    Ok(sol)
}

/// `ProjectToChordBisector`: projects `cursor` onto the perpendicular bisector of the chord
/// `start` - `end`.
pub fn project_to_chord_bisector(
    start: Vector2I,
    end: Vector2I,
    cursor: Vector2I,
) -> Result<Vector2I> {
    let chord = chord_of(start, end);
    let len = chord.euclidean_norm()?;

    if len <= 0.0 {
        return Ok(cursor);
    }

    let normal = Vector2::new(-chord.y / len, chord.x / len);
    let chord_mid = chord_mid_of(start, end);
    let rel = Vector2::new(
        f64::from(cursor.x) - chord_mid.x,
        f64::from(cursor.y) - chord_mid.y,
    );

    match to_iu(chord_mid.try_add(normal.try_mul_scalar(rel.dot(normal)?)?)?)? {
        Some(projected) => Ok(projected),
        None => Ok(cursor),
    }
}

/// `ArcThroughPoints`: the arc from `start` to `end` that passes through `on_arc`.
pub fn arc_through_points(start: Vector2I, on_arc: Vector2I, end: Vector2I) -> Result<ArcSolution> {
    let b = chord_of(start, end);
    let c = chord_of(start, on_arc);
    let d = cross_z(b.x, b.y, c.x, c.y);

    if d == 0.0 {
        return Ok(ArcSolution::default());
    }

    let bb = b.squared_euclidean_norm()?;
    let cc = c.squared_euclidean_norm()?;

    // Circumcenter relative to the start point
    let rel = Vector2::new(
        (c.y * bb - b.y * cc) / (2.0 * d),
        (b.x * cc - c.x * bb) / (2.0 * d),
    );
    let len = b.euclidean_norm()?;

    // The signed distance of the center along the chord normal, measured from the chord midpoint
    let center_t = (-rel.x * b.y + rel.y * b.x) / len;

    build_on_chord(
        start,
        end,
        if d > 0.0 { 1.0 } else { -1.0 },
        center_t,
        rel.euclidean_norm()?,
    )
}

/// `ArcFromStartEndMidDrag`: the arc from `start` to `end` whose midpoint is `cursor` projected
/// onto the chord bisector. Invalid when the projection lands on the chord.
pub fn arc_from_start_end_mid_drag(
    start: Vector2I,
    end: Vector2I,
    cursor: Vector2I,
) -> Result<ArcSolution> {
    let projected = project_to_chord_bisector(start, end, cursor)?;
    let mut sol = arc_through_points(start, projected, end)?;

    // The circle passes through the projected point exactly, so keep it as the midpoint
    if sol.valid {
        sol.mid = projected;
    }

    Ok(sol)
}

/// `ArcFromStartEndCenterDrag`: the arc from `start` to `end` centered on `cursor` projected onto
/// the chord bisector. `major` false gives the minor arc (sweep up to 180 degrees), true the
/// complementary major arc; a center on the chord gives a semicircle and the flag picks the side.
pub fn arc_from_start_end_center_drag(
    start: Vector2I,
    end: Vector2I,
    cursor: Vector2I,
    major: bool,
) -> Result<ArcSolution> {
    let chord = chord_of(start, end);
    let len = chord.euclidean_norm()?;

    if len <= 0.0 {
        return Ok(ArcSolution::default());
    }

    let normal = Vector2::new(-chord.y / len, chord.x / len);
    let chord_mid = chord_mid_of(start, end);
    let center_t = Vector2::new(
        f64::from(cursor.x) - chord_mid.x,
        f64::from(cursor.y) - chord_mid.y,
    )
    .dot(normal)?;

    // The minor arc bulges away from the center, the major arc toward it
    let mut side = if center_t < 0.0 { -1.0 } else { 1.0 };

    if !major {
        side = -side;
    }

    // std::hypot( len * 0.5, centerT ): both operands are finite here.
    build_on_chord(
        start,
        end,
        side,
        center_t,
        hypot::finite(len * 0.5, center_t),
    )
}

/// `ArcFromStartTangentEnd`: the arc leaving `start` along `tangent` and ending at `end`.
/// Invalid when `end` lies on the tangent line or `tangent` is zero.
pub fn arc_from_start_tangent_end(
    start: Vector2I,
    tangent: Vector2D,
    end: Vector2I,
) -> Result<ArcSolution> {
    let tangent_len = tangent.euclidean_norm()?;

    if tangent_len <= 0.0 || !tangent_len.is_finite() {
        return Ok(ArcSolution::default());
    }

    let t = Vector2::new(tangent.x / tangent_len, tangent.y / tangent_len);
    let d = chord_of(start, end);
    let dd = d.squared_euclidean_norm()?;
    let cross = cross_z(d.x, d.y, t.x, t.y);

    if dd <= 0.0 || cross == 0.0 {
        return Ok(ArcSolution::default());
    }

    // The center sits on the normal to the tangent at the start, equidistant from start and end
    let k = -dd / (2.0 * cross);
    let n = Vector2::new(-t.y, t.x);
    let len = dd.sqrt();
    let chord_normal = Vector2::new(-d.y / len, d.x / len);

    build_on_chord(
        start,
        end,
        if cross > 0.0 { 1.0 } else { -1.0 },
        k * n.dot(chord_normal)?,
        k.abs(),
    )
}
