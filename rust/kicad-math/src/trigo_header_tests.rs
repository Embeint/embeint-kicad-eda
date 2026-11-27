// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::eda_angle::{AngleUnit, ANGLE_180, ANGLE_270, ANGLE_90};
use crate::trigo::TrigoError;
use crate::vector::Vector2;

fn v(x: i32, y: i32) -> Vector2I {
    Vector2::new(x, y)
}

fn deg(value: f64) -> EdaAngle {
    EdaAngle::new(value, AngleUnit::Degrees)
}

/// The template loops exactly as written, for values whose iteration count is small.
fn loop_pos(mut angle: i64) -> i64 {
    while angle < 0 {
        angle += 3600;
    }
    while angle >= 3600 {
        angle -= 3600;
    }
    angle
}

fn loop_180(mut angle: i64) -> i64 {
    while angle <= -1800 {
        angle += 3600;
    }
    while angle > 1800 {
        angle -= 3600;
    }
    angle
}

#[test]
fn closed_forms_equal_the_loops() {
    let edges = [
        -1801, -1800, -1799, -1, 0, 1, 1799, 1800, 1801, 3599, 3600, 3601,
    ];
    for base in [-7_200_000_i64, -36_000, -3600, 0, 3600, 36_000, 7_200_000] {
        for edge in edges {
            for step in -2..=2 {
                let a = base + edge + step * 3600;
                assert_eq!(normalize_angle_pos(a), loop_pos(a), "{a}");
                assert_eq!(normalize_angle_180(a), loop_180(a), "{a}");
                let narrow = a as i32;
                assert_eq!(i64::from(normalize_angle_pos(narrow)), loop_pos(a));
                assert_eq!(i64::from(normalize_angle_180(narrow)), loop_180(a));
            }
        }
    }
    // The int extremes run the loops a few hundred thousand times.
    for a in [i32::MIN, i32::MIN + 1, i32::MAX, i32::MAX - 1] {
        assert_eq!(i64::from(normalize_angle_pos(a)), loop_pos(i64::from(a)));
        assert_eq!(i64::from(normalize_angle_180(a)), loop_180(i64::from(a)));
    }
    // long long extremes, from the residues: -2^63 = 1792 and 2^63 - 1 = 1807 (mod 3600).
    assert_eq!(normalize_angle_pos(i64::MIN), 1792);
    assert_eq!(normalize_angle_pos(i64::MAX), 1807);
    assert_eq!(normalize_angle_180(i64::MIN), 1792);
    assert_eq!(normalize_angle_180(i64::MAX), 1807 - 3600);
    let mut a = -1;
    normalize_angle_pos_in_place(&mut a);
    assert_eq!(a, 3599);
}

#[test]
fn conversions_keep_source_order() {
    assert_eq!(deg2rad(180.0), core::f64::consts::PI);
    assert_eq!(rad2deg(core::f64::consts::PI), 180.0);
    assert_eq!(rad2decideg(core::f64::consts::PI), 1800.0);
    // ( 90 * pi ) / 180 rounds differently from 90 * ( pi / 180 ) only in some cases; the
    // product is formed first here.
    assert_eq!(deg2rad(1.0e308).to_bits(), f64::INFINITY.to_bits());
    assert_eq!(rad2deg(1.0e307).to_bits(), f64::INFINITY.to_bits());
    // A signalling NaN is quieted with its sign and payload kept.
    let snan = f64::from_bits(0xfff0_0000_0000_0abc);
    assert_eq!(deg2rad(snan).to_bits(), 0xfff8_0000_0000_0abc);
    assert_eq!(rad2decideg(snan).to_bits(), 0xfff8_0000_0000_0abc);
    assert_eq!(deg2rad(-0.0).to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn intercepts() {
    // test_kimath.cpp TestInterceptsPositiveX / TestInterceptsNegativeX.
    assert!(!intercepts_positive_x(10.0, 20.0));
    assert!(!intercepts_positive_x(10.0, 120.0));
    assert!(!intercepts_positive_x(10.0, 220.0));
    assert!(!intercepts_positive_x(10.0, 320.0));
    assert!(intercepts_positive_x(20.0, 10.0));
    assert!(intercepts_positive_x(345.0, 15.0));
    assert!(!intercepts_negative_x(10.0, 20.0));
    assert!(!intercepts_negative_x(10.0, 120.0));
    assert!(intercepts_negative_x(10.0, 220.0));
    assert!(intercepts_negative_x(10.0, 320.0));
    assert!(intercepts_negative_x(20.0, 10.0));
    assert!(!intercepts_negative_x(345.0, 15.0));
    assert!(intercepts_negative_x(145.0, 225.0));
    assert!(!intercepts_positive_x(f64::NAN, 10.0));
}

#[test]
fn wrappers_forward_to_trigo_cpp() -> TrigoResult<()> {
    let mut p = v(10, 3);
    rotate_point(&mut p, ANGLE_90)?;
    assert_eq!(p, v(3, -10));
    assert_eq!(get_rotated(v(10, 3), ANGLE_180)?, v(-10, -3));
    let mut p = v(11, 5);
    rotate_point_about(&mut p, v(1, 2), ANGLE_270)?;
    assert_eq!(p, v(-2, 12));
    assert_eq!(get_rotated_about(v(11, 5), v(1, 2), ANGLE_270)?, v(-2, 12));
    // The callee's refusal leaves the point unchanged.
    let mut p = v(i32::MIN, 0);
    assert_eq!(rotate_point(&mut p, ANGLE_180), Err(TrigoError::Undefined));
    assert_eq!(p, v(i32::MIN, 0));
    assert_eq!(
        get_rotated_about(v(i32::MAX, 0), v(-1, 0), deg(0.0)),
        Err(TrigoError::Undefined)
    );
    let mut d = Vector2::new(1.0, 0.0);
    rotate_point_d(&mut d, ANGLE_90);
    assert_eq!((d.x.to_bits(), d.y.to_bits()), (0, (-1.0_f64).to_bits()));
    let mut d = Vector2::new(2.0, 1.0);
    rotate_point_d_about(&mut d, Vector2::new(1.0, 1.0), ANGLE_180);
    assert_eq!((d.x, d.y), (0.0, 1.0));
    let d = get_rotated_d(Vector2::new(1.0, 0.0), ANGLE_270);
    assert_eq!(
        (d.x.to_bits(), d.y.to_bits()),
        ((-0.0_f64).to_bits(), 1.0_f64.to_bits())
    );
    Ok(())
}
