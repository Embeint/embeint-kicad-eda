// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_math::math_util::{
    kimath_log_debug, kimath_log_overflow, rescale_i32, rescale_i64_int128, rounded_hypot_wide,
    MathUtilError, UNIT,
};

#[test]
fn hypot_rounds_and_preserves_both_unsigned_wraps() {
    assert_eq!(rounded_hypot_wide(3, 4), 5);
    assert_eq!(rounded_hypot_wide(1, 1), 1);
    assert_eq!(rounded_hypot_wide(2, 2), 3);
    assert_eq!(
        rounded_hypot_wide(1 << 63, 1 << 63),
        13_043_817_825_332_782_212
    );
    assert_eq!(rounded_hypot_wide(u64::MAX, (1 << 32) - 1), u64::MAX);
    assert_eq!(rounded_hypot_wide(u64::MAX, 1 << 32), 0);
    assert_eq!(
        rounded_hypot_wide(u64::MAX, u64::MAX),
        18_446_744_073_709_551_614
    );
    assert_eq!(rounded_hypot_wide(0, 0), 0);
}

#[test]
fn rescale_negative_halfways_odd_divisors_and_amplification() {
    for (n, v, d, expected) in [
        (1, 1, 2, 1),
        (-1, 1, 2, -1),
        (1, 1, -2, -1),
        (-1, 1, -2, 1),
        (1, -4, 3, -1),
        (1, 4, -3, -1),
        (7, 20, 2, 70),
    ] {
        assert_eq!(rescale_i32(n, v, d), Ok(expected));
        assert_eq!(
            rescale_i64_int128(n.into(), v.into(), d.into()),
            Ok(expected.into())
        );
    }
}

#[test]
fn signed_narrowing_is_observable_not_saturated_or_refused() {
    assert_eq!(rescale_i32(i32::MIN, -1, 1), Ok(i32::MIN));
    assert_eq!(rescale_i32(i32::MIN, i32::MIN, 1), Ok(0));
    assert_eq!(rescale_i64_int128(i64::MIN, -1, 1), Ok(i64::MIN));
    assert_eq!(rescale_i64_int128(i64::MIN, i64::MIN, 1), Ok(0));
    assert_eq!(rescale_i64_int128(i64::MIN, 1, i64::MIN), Ok(1));
}

#[test]
fn undefined_divisions_and_missing_host_effects_are_typed() {
    assert_eq!(rescale_i32(0, 0, 0), Err(MathUtilError::ZeroDenominator));
    assert_eq!(
        rescale_i64_int128(i64::MIN, i64::MIN, 0),
        Err(MathUtilError::ZeroDenominator)
    );
    assert_eq!(
        kimath_log_debug("%lld", &[]),
        Err(MathUtilError::Unported {
            upstream: "kimathLogDebug",
            unit: UNIT
        })
    );
    assert_eq!(
        kimath_log_overflow(f64::INFINITY, "i"),
        Err(MathUtilError::Unported {
            upstream: "kimathLogOverflow",
            unit: UNIT
        })
    );
}
