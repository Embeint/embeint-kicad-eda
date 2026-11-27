// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::eda_angle::{AngleUnit, ANGLE_45};

fn v(x: i32, y: i32) -> Vector2I {
    Vector2::new(x, y)
}

fn d(x: f64, y: f64) -> Vector2D {
    Vector2::new(x, y)
}

/// Test-only header callees: the bodies `trigo.h` inlines, forwarding to this unit's
/// out-of-line `RotatePoint` overloads, so the continuations after the header calls can be
/// exercised. Not production code and not oracle evidence by itself.
struct Forwarding;

impl HeaderCallees for Forwarding {
    fn rotate_vector2i_about(
        &self,
        point: &mut Vector2I,
        centre: Vector2I,
        angle: EdaAngle,
    ) -> TrigoResult<()> {
        rotate_point_i_about(&mut point.x, &mut point.y, centre.x, centre.y, angle)
    }

    fn rotate_vector2d(&self, point: &mut Vector2D, angle: EdaAngle) -> TrigoResult<()> {
        rotate_point_d(&mut point.x, &mut point.y, angle);
        Ok(())
    }
}

#[test]
fn minmax3_follows_libstdcxx_order() {
    assert_eq!(minmax3(1.0, 2.0, 3.0), (1.0, 3.0));
    assert_eq!(minmax3(3.0, 2.0, 1.0), (1.0, 3.0));
    assert_eq!(minmax3(2.0, 3.0, 1.0), (1.0, 3.0));
    // Ties: the first minimum and the last maximum survive (visible through signed zeros).
    let (min, max) = minmax3(0.0, -0.0, 0.0);
    assert_eq!(min.to_bits(), 0.0_f64.to_bits());
    assert_eq!(max.to_bits(), 0.0_f64.to_bits());
    let (min, max) = minmax3(-0.0, 0.0, -0.0);
    assert_eq!(min.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(max.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn fma_is_fused_and_nan_rules_are_explicit() {
    // 1 + 2^-52 squared minus 1 keeps the 2^-104 term only when fused.
    let x = 1.0 + f64::EPSILON;
    assert_eq!(
        fma(x, x, -1.0),
        2.0 * f64::EPSILON + f64::EPSILON * f64::EPSILON
    );
    assert_eq!(
        fma(f64::INFINITY, 0.0, 1.0).to_bits(),
        0xfff8_0000_0000_0000
    );
    assert_eq!(
        fma(1.0, f64::from_bits(0x7ff0_0000_0000_0001), 2.0).to_bits(),
        0x7ff8_0000_0000_0001
    );
    assert_eq!(fma(2.0, 3.0, f64::INFINITY), f64::INFINITY);
}

#[test]
fn point_on_segment() -> TrigoResult<()> {
    assert!(is_point_on_segment(v(0, 0), v(10, 10), v(5, 5))?);
    assert!(is_point_on_segment(v(0, 0), v(10, 10), v(10, 10))?);
    assert!(!is_point_on_segment(v(0, 0), v(10, 10), v(11, 11))?);
    assert!(!is_point_on_segment(v(0, 0), v(10, 10), v(5, 6))?);
    // Behind the start: the dot product is negative, below the squared norm.
    assert!(!is_point_on_segment(v(0, 0), v(10, 10), v(-1, -1))?);
    // A differences overflow int.
    assert_eq!(
        is_point_on_segment(v(i32::MIN, 0), v(1, 0), v(0, 0)),
        Err(TrigoError::Undefined)
    );
    // Collinear INT_MIN differences: the dot sum reaches 2^63.
    assert_eq!(
        is_point_on_segment(v(0, 0), v(i32::MIN, i32::MIN), v(i32::MIN, i32::MIN)),
        Err(TrigoError::Undefined)
    );
    Ok(())
}

#[test]
fn segments_intersect() -> TrigoResult<()> {
    let mut p = v(7, 7);
    assert!(segment_intersects_segment(
        v(0, 0),
        v(10, 10),
        v(0, 10),
        v(10, 0),
        Some(&mut p)
    )?);
    assert_eq!(p, v(5, 5));
    // Parallel: false and the output untouched.
    let mut q = v(7, 7);
    assert!(!segment_intersects_segment(
        v(0, 0),
        v(10, 0),
        v(0, 1),
        v(10, 1),
        Some(&mut q)
    )?);
    assert_eq!(q, v(7, 7));
    // Lines cross outside the segments: the point is still written. The pinned y offset is
    // dY_a * num_b / den (u_b, not u_a), so the written point is ( 5, 1 ), not the true
    // crossing ( 5, 5 ) (port/upstream-issues/trigo-segment-intersection-point-y.md).
    let mut r = v(0, 0);
    assert!(!segment_intersects_segment(
        v(0, 0),
        v(1, 1),
        v(10, 0),
        v(0, 10),
        Some(&mut r)
    )?);
    assert_eq!(r, v(5, 1));
    assert!(segment_intersects_segment(
        v(0, 0),
        v(10, 10),
        v(0, 10),
        v(10, 0),
        None
    )?);
    // den = -2^62 - 2^62: its negation is undefined.
    assert_eq!(
        segment_intersects_segment(
            v(0, -1),
            v(i32::MIN, i32::MAX),
            v(0, 0),
            v(i32::MIN, i32::MIN),
            None
        ),
        Err(TrigoError::Undefined)
    );
    Ok(())
}

#[test]
fn segment_hit() -> TrigoResult<()> {
    assert!(test_segment_hit(v(5, 1), v(0, 0), v(10, 0), 1)?);
    assert!(!test_segment_hit(v(5, 2), v(0, 0), v(10, 0), 1)?);
    assert!(test_segment_hit(v(1, 5), v(0, 0), v(0, 10), 1)?);
    // The SEG path: squared distance 2 against ( 1 + 1 )^2.
    assert!(test_segment_hit(v(11, 1), v(0, 0), v(10, 0), 1)?);
    assert!(!test_segment_hit(v(12, 1), v(0, 0), v(10, 0), 1)?);
    assert_eq!(
        test_segment_hit(v(0, 0), v(1, 1), v(2, 3), i32::MAX),
        Err(TrigoError::Undefined)
    );
    Ok(())
}

#[test]
fn rotations() -> TrigoResult<()> {
    let (mut x, mut y) = (3, 4);
    rotate_point_i(&mut x, &mut y, ANGLE_90)?;
    assert_eq!((x, y), (4, -3));
    rotate_point_i(&mut x, &mut y, ANGLE_270)?;
    assert_eq!((x, y), (3, 4));
    rotate_point_i(&mut x, &mut y, EdaAngle::new(-180.0, AngleUnit::Degrees))?;
    assert_eq!((x, y), (-3, -4));
    let (mut x, mut y) = (100, 0);
    rotate_point_i(&mut x, &mut y, ANGLE_45)?;
    assert_eq!((x, y), (71, -71));
    let (mut x, mut y) = (i32::MIN, 0);
    assert_eq!(
        rotate_point_i(&mut x, &mut y, ANGLE_90),
        Err(TrigoError::Undefined)
    );
    assert_eq!((x, y), (i32::MIN, 0));
    let (mut x, mut y) = (10, 0);
    rotate_point_i_about(&mut x, &mut y, 5, 5, ANGLE_180)?;
    assert_eq!((x, y), (0, 10));
    let (mut x, mut y) = (1.0, 0.0);
    rotate_point_d(&mut x, &mut y, ANGLE_90);
    assert_eq!((x.to_bits(), y.to_bits()), (0, (-1.0_f64).to_bits()));
    let (mut x, mut y) = (2.0, 1.0);
    rotate_point_d_about(&mut x, &mut y, 1.0, 1.0, ANGLE_180);
    assert_eq!((x, y), (0.0, 1.0));
    Ok(())
}

#[test]
fn arc_mid_defers_at_the_header_and_continues() -> TrigoResult<()> {
    let deferral = Err(TrigoError::Unported {
        upstream: ROTATE_VECTOR2I_ABOUT,
        unit: TRIGO_HEADER_UNIT,
    });
    assert_eq!(calc_arc_mid(v(100, 0), v(0, 100), v(0, 0), true), deferral);
    assert_eq!(
        calc_arc_mid_with(&Forwarding, v(100, 0), v(0, 100), v(0, 0), true)?,
        v(71, 71)
    );
    assert_eq!(
        calc_arc_mid_with(&Forwarding, v(100, 0), v(0, 100), v(0, 0), false)?,
        v(-71, -71)
    );
    // The differences come before the header call.
    assert_eq!(
        calc_arc_mid(v(i32::MIN, 0), v(0, 0), v(1, 0), true),
        Err(TrigoError::Undefined)
    );
    Ok(())
}

#[test]
fn arc_center_from_angle() -> TrigoResult<()> {
    // A zero angle returns the chord midpoint before the header call.
    let c = calc_arc_center_angle(d(0.0, 0.0), d(10.0, 0.0), ANGLE_0)?;
    assert_eq!((c.x, c.y), (5.0, 0.0));
    assert_eq!(
        calc_arc_center_angle(d(0.0, 0.0), d(10.0, 0.0), ANGLE_90),
        Err(TrigoError::Unported {
            upstream: ROTATE_VECTOR2D,
            unit: TRIGO_HEADER_UNIT
        })
    );
    // 180 degrees: the center is the chord midpoint (d is cot 90 = 0 here).
    let c = calc_arc_center_angle_with(&Forwarding, d(0.0, 0.0), d(10.0, 0.0), ANGLE_180)?;
    assert_eq!((c.x, c.y), (5.0, 0.0));
    Ok(())
}

#[test]
fn arc_centers() -> TrigoResult<()> {
    assert_eq!(
        calc_arc_center_i(v(-100, 0), v(0, 100), v(100, 0))?,
        v(0, 0)
    );
    let c = calc_arc_center_d(d(-100.0, 0.0), d(0.0, 100.0), d(100.0, 0.0))?;
    assert_eq!((c.x, c.y), (0.0, 0.0));
    // Cluster: the mean of the three points.
    let c = calc_arc_center_d(d(0.0, 0.0), d(1.0, 1.0), d(2.0, 0.0))?;
    assert_eq!((c.x, c.y), (1.0, div(1.0, 3.0)));
    // Collinear: 1e17 off the chord midpoint, on the (y, -x) side.
    let c = calc_arc_center_d(d(0.0, 0.0), d(50.0, 0.0), d(100.0, 0.0))?;
    assert_eq!((c.x, c.y), (50.0, -1e17));
    assert_eq!(
        calc_arc_center_i(v(0, 0), v(50, 0), v(100, 0))?,
        v(50, i32::MIN + 100)
    );
    assert!(is_straight_arc(v(0, 0), v(50, 0), v(100, 0))?);
    assert!(!is_straight_arc(v(0, 0), v(50, 0), v(0, 0))?);
    assert!(!is_straight_arc(v(-100, 0), v(0, 100), v(100, 0))?);
    // Near-collinear: the center falls outside the clamp.
    assert!(is_straight_arc(v(0, 0), v(1_000_000, 1), v(2_000_000, 0))?);
    Ok(())
}
