// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::eda_angle::{AngleUnit, ANGLE_180, ANGLE_270, ANGLE_90};
use crate::trigo::TrigoError;
use crate::vector::Vector2;

/// `f64::next_up` and `next_down` are newer than the workspace minimum Rust (1.85); step by bits.
fn next_up(value: f64) -> f64 {
    if value.is_nan() || value == f64::INFINITY {
        value
    } else if value == 0.0 {
        f64::from_bits(1)
    } else if value > 0.0 {
        f64::from_bits(value.to_bits() + 1)
    } else {
        f64::from_bits(value.to_bits() - 1)
    }
}

fn next_down(value: f64) -> f64 {
    -next_up(-value)
}

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

/// The `double` loops exactly as written, with a pass limit for the test.
fn literal_d(mut angle: f64, half: bool, limit: u64) -> Option<(f64, u64)> {
    let mut passes = 0;
    let (low, high) = if half {
        (-1800.0, 1800.0)
    } else {
        (0.0, 3600.0)
    };
    let below = |a: f64| if half { a <= low } else { a < low };
    let above = |a: f64| if half { a > high } else { a >= high };
    while below(angle) {
        angle += 3600.0;
        passes += 1;
        if passes > limit {
            return None;
        }
    }
    while above(angle) {
        angle -= 3600.0;
        passes += 1;
        if passes > limit {
            return None;
        }
    }
    Some((angle, passes))
}

fn check_d(angle: f64) {
    let limit = 50_000_000;
    let pos = literal_d(angle, false, limit).map(|r| r.0.to_bits());
    let half = literal_d(angle, true, limit).map(|r| r.0.to_bits());
    assert!(
        pos.is_some() && half.is_some(),
        "{angle:e}: too many passes"
    );
    assert_eq!(
        normalize_angle_pos_d(angle).map(f64::to_bits).ok(),
        pos,
        "{angle:e}"
    );
    assert_eq!(
        normalize_angle_180_d(angle).map(f64::to_bits).ok(),
        half,
        "{angle:e}"
    );
    let mut in_place = angle;
    assert_eq!(normalize_angle_pos_in_place_d(&mut in_place), Ok(()));
    assert_eq!(Some(in_place.to_bits()), pos);
}

#[test]
fn double_templates_equal_the_loops() {
    let tiny = [
        0.0,
        -0.0,
        5e-324,
        -5e-324,
        -1e-300,
        -1e-310,
        f64::from_bits(0xbd50_0000_0000_0000), // -2^-42: rounds to 3600, then 0
        f64::from_bits(0xbd40_0000_0000_0000), // -2^-43
        f64::from_bits(0xbd58_0000_0000_0000), // -1.5 * 2^-42: rounds below 3600
        -0.1,
        -0.25,
        0.25,
    ];
    for a in tiny {
        check_d(a);
    }
    let edges = [-1800.0, 0.0, 1800.0, 3600.0];
    // Near the shim's 2^24-pass budget (3600 * 2^24 = 60397977600).
    for a in [60_397_977_600.5, 60_397_977_600.0, 60_397_981_199.75] {
        check_d(a);
        check_d(-a);
    }
    for base in [-7.2e7, -36000.0, -3600.0, 0.0, 3600.0, 36000.0, 7.2e7] {
        for edge in edges {
            for step in -2..=2 {
                let a = base + edge + f64::from(step) * 3600.0;
                for b in [a, next_up(a), next_down(a), a + 0.25, a - 0.75, -a] {
                    check_d(b);
                }
            }
        }
    }
    // Exact results observed from GCC 13.3 (`-O3 -ffp-contract=off`) on the pinned header.
    let observed: [(u64, u64, u64); 12] = [
        (
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
        ),
        (0x81a5_6e1f_c2f8_f359, 0, 0x81a5_6e1f_c2f8_f359), // -1e-300
        (
            0x422c_2000_0001_0000,
            0x3fe0_0000_0000_0000,
            0x3fe0_0000_0000_0000,
        ),
        (0xbd50_0000_0000_0000, 0, 0xbd50_0000_0000_0000),
        (
            0xbd58_0000_0000_0000,
            0x40ac_1fff_ffff_ffff,
            0xbd58_0000_0000_0000,
        ),
        (
            0xbfb9_9999_9999_999a,
            0x40ac_1fcc_cccc_cccd,
            0xbfb9_9999_9999_999a,
        ),
        (
            0xc0ac_2033_3333_3333,
            0x40ac_1fcc_cccc_cccd,
            0xbfb9_9999_9999_8000,
        ),
        (0xc0ac_2000_0000_0000, 0, 0),
        (
            0x40bc_2080_0000_0000,
            0x3fe0_0000_0000_0000,
            0x3fe0_0000_0000_0000,
        ),
        (
            0xc0bc_2080_0000_0000,
            0x40ac_1f00_0000_0000,
            0xbfe0_0000_0000_0000,
        ),
        (0x8000_0000_0000_0001, 0, 0x8000_0000_0000_0001),
        (
            0xc22c_2000_0001_0000,
            0x40ac_1f00_0000_0000,
            0xbfe0_0000_0000_0000,
        ),
    ];
    for (input, pos, half) in observed {
        let a = f64::from_bits(input);
        assert_eq!(normalize_angle_pos_d(a).map(f64::to_bits), Ok(pos), "{a:e}");
        assert_eq!(
            normalize_angle_180_d(a).map(f64::to_bits),
            Ok(half),
            "{a:e}"
        );
    }
    // A NaN fails every test and keeps its bits, signalling or not.
    for nan in [
        0x7ff8_0000_0000_0000_u64,
        0xfff4_0000_0000_00ee,
        0x7ff0_0000_0000_0001,
    ] {
        let a = f64::from_bits(nan);
        assert_eq!(normalize_angle_pos_d(a).map(f64::to_bits), Ok(nan));
        assert_eq!(normalize_angle_180_d(a).map(f64::to_bits), Ok(nan));
    }
}

#[test]
fn large_double_passes_equal_the_literal_steps() {
    // Across every binade from 2^56 to 2^65, the skipped sequence equals the literal one from
    // starts within a few thousand passes of a binade boundary (both sides) to below it.
    for e in 56..=65_i32 {
        let lo = 1_i128 << e;
        let u = 1_i128 << (e - 52).max(0);
        for t in [0_i128, 1, 2, 3, 7, 1000, 12_345, 99_999] {
            for offset in [0_i128, 3600, 4_000_000] {
                let start = lo + offset / u * u + u * t;
                if start > 1 << 65 {
                    continue;
                }
                let floor = lo - 40_000;
                let mut literal = start as f64;
                assert_eq!(literal as i128, start);
                while literal as i128 > floor {
                    literal -= 3600.0;
                }
                assert_eq!(skip_large(start, floor), Ok(literal as i128), "{start}");
                // Skipping in two stages gives the same member.
                let mid = (start + floor) / 2;
                assert_eq!(
                    skip_large(start, mid).and_then(|m| skip_large(m, floor)),
                    Ok(literal as i128)
                );
            }
        }
    }
    // The steps within a binade: 3600 below 2^57, then 3584 (the 2^57 tie from an even multiple
    // and u = 64 .. 512) and 4096 (u = 1024 .. 4096).
    for (e, step) in [
        (56, 3600),
        (57, 3584),
        (58, 3584),
        (61, 3584),
        (62, 4096),
        (64, 4096),
    ] {
        let m = (1_i128 << e) + (1 << 20) * 64;
        assert_eq!(
            m - step_large(step_large(m)),
            step + (m - step_large(m)),
            "{e}"
        );
    }
    // An odd multiple of 32 above 2^57 steps by 3616 once.
    let odd = (1_i128 << 57) + 32 * 100_001;
    assert_eq!(odd - step_large(odd), 3616);
}

#[test]
fn double_loops_that_never_terminate_are_refused() {
    let undefined = Err(TrigoError::Undefined);
    let two65 = 36_893_488_147_419_103_232.0_f64;
    // Above 2^65 no pass progresses, at every exponent; at 2^65 it does.
    for e in 65..=1023 {
        let base = f64::from_bits(((1023 + e) as u64) << 52);
        for m in [base, next_up(base), base * 1.5, next_up(next_up(base))] {
            if m == two65 || !m.is_finite() {
                continue;
            }
            assert_eq!(m - 3600.0, m, "{m:e}");
            for a in [m, -m] {
                assert_eq!(normalize_angle_pos_d(a), undefined, "{a:e}");
                assert_eq!(normalize_angle_180_d(a), undefined, "{a:e}");
                let mut b = a;
                assert_eq!(
                    normalize_angle_pos_in_place_d(&mut b),
                    Err(TrigoError::Undefined)
                );
                assert_eq!(b.to_bits(), a.to_bits());
            }
        }
    }
    for a in [f64::INFINITY, f64::NEG_INFINITY, f64::MAX, f64::MIN] {
        assert_eq!(normalize_angle_pos_d(a), undefined);
        assert_eq!(normalize_angle_180_d(a), undefined);
    }
    assert_ne!(two65 - 3600.0, two65);
    for a in [two65, -two65, next_down(two65), 1.0e19, -1.0e18] {
        let pos = normalize_angle_pos_d(a);
        let half = normalize_angle_180_d(a);
        assert!(
            matches!(pos, Ok(v) if (0.0..3600.0).contains(&v)),
            "{a:e} {pos:?}"
        );
        assert!(
            matches!(half, Ok(v) if v > -1800.0 && v <= 1800.0),
            "{a:e} {half:?}"
        );
    }
}
