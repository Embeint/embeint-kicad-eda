// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/geometry_predicates.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `KIGEOM` triangle predicates (libs/kimath/include/geometry/geometry_predicates.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! Every body is defined for every `VECTOR2I` input: the differences are widened to `int64_t`
//! before subtracting, the exact `KI_INT128` determinant is only evaluated when every difference
//! is at most `1.5e9` in magnitude (where it cannot overflow), and the `double` evaluations stay
//! finite (no NaN can arise from integer-valued operands of at most `2^33`). The checked `i128`
//! arithmetic below therefore never fails; it only keeps the crate panic-free.
//!
//! `double` arithmetic is evaluated in source order without contraction. With finite operands
//! and no NaN, the IEEE results are the reference results bit for bit.
use crate::vector::{Vector2, Vector2I, Vector2L};
use crate::wide_int::{cross_wide, KiInt128};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

/// A checked operation that cannot fail for any `VECTOR2I` input failed (kept for panic-free
/// arithmetic only; never produced).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredicateError {
    Overflow,
}

pub type PredicateResult<T> = Result<T, PredicateError>;

fn diff(a: i32, b: i32) -> i64 {
    i64::from(a) - i64::from(b)
}

/// `KIGEOM::OrientationSign`: +1 counter-clockwise, -1 clockwise, 0 collinear.
pub fn orientation_sign(a: Vector2I, b: Vector2I, c: Vector2I) -> PredicateResult<i32> {
    let ab_x = diff(b.x, a.x);
    let ab_y = diff(b.y, a.y);
    let ac_x = diff(c.x, a.x);
    let ac_y = diff(c.y, a.y);

    let ab: Vector2L = Vector2::new(ab_x, ab_y);
    let ac: Vector2L = Vector2::new(ac_x, ac_y);
    let cross: KiInt128 = cross_wide(&ab, &ac).map_err(|_| PredicateError::Overflow)?;

    Ok(if cross > 0 {
        1
    } else if cross < 0 {
        -1
    } else {
        0
    })
}

/// `KIGEOM::detail::inCircleLegalDouble`: the filtered-double legality test with its tie margin.
pub fn in_circle_legal_double(a: Vector2I, b: Vector2I, c: Vector2I, p: Vector2I) -> bool {
    let d = |u: i32, v: i32| f64::from(u) - f64::from(v);
    let (pa_x, pa_y) = (d(a.x, p.x), d(a.y, p.y));
    let (pb_x, pb_y) = (d(b.x, p.x), d(b.y, p.y));
    let (pc_x, pc_y) = (d(c.x, p.x), d(c.y, p.y));

    let pa_sq = pa_x * pa_x + pa_y * pa_y;
    let pb_sq = pb_x * pb_x + pb_y * pb_y;
    let pc_sq = pc_x * pc_x + pc_y * pc_y;

    let det = pa_x * (pb_y * pc_sq - pb_sq * pc_y) - pa_y * (pb_x * pc_sq - pb_sq * pc_x)
        + pa_sq * (pb_x * pc_y - pb_y * pc_x);
    let sum_sq = pa_sq + pb_sq + pc_sq;

    det <= 1e-13 * sum_sq * sum_sq
}

/// `KIGEOM::InCircleDelaunayLegal`: true when `p` is outside the circumcircle of the CCW
/// triangle `(a, b, c)`. Exact below the `kMaxSafeDiff` bound, the filtered double test above it.
pub fn in_circle_delaunay_legal(
    a: Vector2I,
    b: Vector2I,
    c: Vector2I,
    p: Vector2I,
) -> PredicateResult<bool> {
    let (pa_x, pa_y) = (diff(a.x, p.x), diff(a.y, p.y));
    let (pb_x, pb_y) = (diff(b.x, p.x), diff(b.y, p.y));
    let (pc_x, pc_y) = (diff(c.x, p.x), diff(c.y, p.y));

    // Beyond this the degree-4 determinant (~12*M^4) can exceed signed int128.
    const K_MAX_SAFE_DIFF: i64 = 1_500_000_000;

    // |difference| < 2^33, so abs never overflows.
    if [pa_x, pa_y, pb_x, pb_y, pc_x, pc_y]
        .iter()
        .any(|v| v.abs() > K_MAX_SAFE_DIFF)
    {
        return Ok(in_circle_legal_double(a, b, c, p));
    }

    let w = KiInt128::from;
    let (pa_x, pa_y, pb_x, pb_y, pc_x, pc_y) =
        (w(pa_x), w(pa_y), w(pb_x), w(pb_y), w(pc_x), w(pc_y));
    let o = |v: Option<KiInt128>| v.ok_or(PredicateError::Overflow);
    let mul = |x: KiInt128, y: KiInt128| o(x.checked_mul(y));
    let add = |x: KiInt128, y: KiInt128| o(x.checked_add(y));
    let sub = |x: KiInt128, y: KiInt128| o(x.checked_sub(y));

    let pa_sq = add(mul(pa_x, pa_x)?, mul(pa_y, pa_y)?)?;
    let pb_sq = add(mul(pb_x, pb_x)?, mul(pb_y, pb_y)?)?;
    let pc_sq = add(mul(pc_x, pc_x)?, mul(pc_y, pc_y)?)?;

    let first = mul(pa_x, sub(mul(pb_y, pc_sq)?, mul(pb_sq, pc_y)?)?)?;
    let second = mul(pa_y, sub(mul(pb_x, pc_sq)?, mul(pb_sq, pc_x)?)?)?;
    let third = mul(pa_sq, sub(mul(pb_x, pc_y)?, mul(pb_y, pc_x)?)?)?;
    let det = add(sub(first, second)?, third)?;

    Ok(det <= 0)
}

/// `KIGEOM::IsSliverTriangle`: the longest edge exceeds ten times the shortest.
pub fn is_sliver_triangle(a: Vector2I, b: Vector2I, c: Vector2I) -> bool {
    let d = |u: i32, v: i32| f64::from(u) - f64::from(v);
    let (ab_x, ab_y) = (d(b.x, a.x), d(b.y, a.y));
    let (bc_x, bc_y) = (d(c.x, b.x), d(c.y, b.y));
    let (ca_x, ca_y) = (d(a.x, c.x), d(a.y, c.y));

    let ab_squared = ab_x * ab_x + ab_y * ab_y;
    let bc_squared = bc_x * bc_x + bc_y * bc_y;
    let ca_squared = ca_x * ca_x + ca_y * ca_y;

    // std::max / std::min over an initializer list: the values are never NaN, so the selected
    // element's value does not depend on tie handling.
    let longest_squared = crate::util::max(crate::util::max(ab_squared, bc_squared), ca_squared);
    let shortest_squared = crate::util::min(crate::util::min(ab_squared, bc_squared), ca_squared);

    shortest_squared > 0.0 && longest_squared > 100.0 * shortest_squared
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> Vector2I {
        Vector2::new(x, y)
    }

    #[test]
    fn pinned_qa_checks() {
        // qa/tests/libs/kimath/math/test_wide_int.cpp: OrientationSignBeyondDoublePrecision and
        // InCircleDelaunayLegalExact.
        let (a, b, c) = (
            v(-1_500_000_000, -1_500_000_000),
            v(1_500_000_001, 1_500_000_000),
            v(1_500_000_002, 1_500_000_001),
        );
        assert_eq!(orientation_sign(a, b, c), Ok(1));
        assert_eq!(orientation_sign(a, c, b), Ok(-1));
        let (a, b, c) = (v(0, 0), v(1_000_000, 0), v(0, 1_000_000));
        assert_eq!(
            in_circle_delaunay_legal(a, b, c, v(1_000_000, 1_000_000)),
            Ok(true)
        );
        assert_eq!(
            in_circle_delaunay_legal(a, b, c, v(999_999, 999_999)),
            Ok(false)
        );
        assert_eq!(
            in_circle_delaunay_legal(a, b, c, v(1_000_001, 1_000_001)),
            Ok(true)
        );
        // test_poly_triangulation.cpp: the precondition of the spike test and
        // PredicatesHandleFullCoordinateRange.
        assert_eq!(
            in_circle_delaunay_legal(
                v(0, 0),
                v(284_000, 42_000),
                v(276_000, 68_000),
                v(38_000, 126_000)
            ),
            Ok(true)
        );
        let (a, b, c) = (
            v(-2_000_000_000, 0),
            v(2_000_000_000, 0),
            v(2_000_000_000, 1000),
        );
        assert_eq!(orientation_sign(a, b, c), Ok(1));
        assert_eq!(orientation_sign(a, c, b), Ok(-1));
        assert!(is_sliver_triangle(a, b, c));
        assert!(!is_sliver_triangle(v(0, 0), v(0, 0), v(1000, 5)));
    }

    #[test]
    fn extreme_inputs_are_defined() {
        let lo = v(i32::MIN, i32::MIN);
        let hi = v(i32::MAX, i32::MAX);
        let mixed = v(i32::MIN, i32::MAX);
        assert!(orientation_sign(lo, hi, mixed).is_ok());
        assert!(in_circle_delaunay_legal(lo, hi, mixed, v(0, 0)).is_ok());
        let m = 1_500_000_000;
        assert!(in_circle_delaunay_legal(v(m, -m), v(-m, m), v(m, m), v(0, 0)).is_ok());
    }
}
