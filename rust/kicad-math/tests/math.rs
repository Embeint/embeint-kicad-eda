// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour checks for the portable crate. Exact parity with the C++ is established by the
//! live difftest (`port/cases/libs.kimath.include.math.application.pure.p001.jsonl`); these
//! tests guard internal consistency and documented edge cases.

use kicad_math::matrix3x3::format_stream_double;
use kicad_math::wide_int::{
    cross_wide, dot_wide, to_double, to_double_via_words, words_to_double, WideIntError,
};
use kicad_math::{Matrix3x3D, Vector2, Vector2L, Vector3};

/// SplitMix64, fixed seed: deterministic generator for broad internal consistency checks.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[test]
fn both_to_double_branches_round_every_value_identically() {
    let mut values = vec![
        0_i128,
        1,
        -1,
        i128::MAX,
        i128::MIN,
        i128::MIN + 1,
        1 << 64,
        -(1 << 64),
        (1 << 64) - 1,
        (1 << 53) + 1,
        (1_i128 << 117) + (1 << 64),
        (1_i128 << 117) + (1 << 64) + 1,
        -((1_i128 << 117) + (1 << 64)),
        i64::MIN.into(),
        u64::MAX.into(),
    ];
    let mut random = SplitMix(0x4b69_4361_6420_6d61);
    for _ in 0..200_000 {
        let value = (i128::from(random.next() as i64) << 64) | i128::from(random.next());
        let shift = random.next() % 128;
        values.push(value >> shift);
    }
    for value in values {
        assert_eq!(
            to_double_via_words(value).to_bits(),
            to_double(value).to_bits(),
            "{value}"
        );
    }
}

#[test]
fn words_to_double_handles_negation_carries_and_sticky_bits() {
    assert_eq!(words_to_double(0, 0).to_bits(), 0);
    assert_eq!(words_to_double(-1, u64::MAX), -1.0);
    assert_eq!(words_to_double(-1, 0), -18_446_744_073_709_551_616.0);
    assert_eq!(words_to_double(i64::MIN, 0), -(2.0_f64.powi(127)));
    assert_eq!(words_to_double(i64::MAX, u64::MAX), 2.0_f64.powi(127));
    // 2^64 + 2^11 is a tie between 2^64 and 2^64 + 2^12 and rounds to even; one more low bit
    // breaks the tie upwards through the sticky bit.
    assert_eq!(words_to_double(1, 1 << 11), 2.0_f64.powi(64));
    assert_eq!(words_to_double(1, (1 << 11) + 1), 2.0_f64.powi(64) + 4096.0);
}

#[test]
fn wide_products_are_exact_and_only_the_documented_dot_overflows() {
    let min = Vector2L::new(i64::MIN, i64::MIN);
    let max = Vector2L::new(i64::MAX, i64::MAX);
    assert_eq!(cross_wide(&min, &min), Ok(0));
    assert_eq!(
        cross_wide(&min, &Vector2L::new(i64::MAX, i64::MIN)),
        Ok(i128::MAX - (1 << 63) + 1)
    );
    assert_eq!(
        dot_wide(&max, &max),
        Ok(2 * i128::from(i64::MAX) * i128::from(i64::MAX))
    );
    assert_eq!(
        dot_wide(&min, &min),
        Err(WideIntError::Overflow {
            upstream: "DotWide"
        })
    );
    assert_eq!(
        dot_wide(&min, &Vector2L::new(i64::MIN, i64::MIN + 1)),
        Ok(i128::MAX - (1 << 63) + 1)
    );
}

#[test]
fn stream_doubles_follow_default_ostream_g6() {
    for (value, text) in [
        (0.0, "0"),
        (-0.0, "-0"),
        (1.0, "1"),
        (0.5, "0.5"),
        (0.125, "0.125"),
        (1.0 / 3.0, "0.333333"),
        (123456.0, "123456"),
        (1234565.0, "1.23456e+06"),
        (1234575.0, "1.23458e+06"),
        (9.9999950, "10"),
        (999999.5, "1e+06"),
        (0.0001, "0.0001"),
        (0.00001, "1e-05"),
        (0.000123456500000000, "0.000123457"),
        (1e100, "1e+100"),
        (-2.5e-300, "-2.5e-300"),
        (f64::MIN_POSITIVE, "2.22507e-308"),
        (f64::from_bits(1), "4.94066e-324"),
        (f64::MAX, "1.79769e+308"),
        (f64::INFINITY, "inf"),
        (f64::NEG_INFINITY, "-inf"),
        (f64::from_bits(0x7ff8_0000_0000_0000), "nan"),
        (f64::from_bits(0xfff8_0000_0000_0000), "-nan"),
    ] {
        assert_eq!(format_stream_double(value), text, "{value:e}");
    }
}

#[test]
fn matrix_bodies_keep_unrelated_members_and_reference_semantics() {
    let mut matrix = Matrix3x3D::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    matrix.set_translation(Vector2::new(-1.0, -2.0));
    matrix.set_scale(Vector2::new(10.0, 20.0));
    assert_eq!(
        matrix.data,
        [[10.0, 2.0, -1.0], [4.0, 20.0, -2.0], [7.0, 8.0, 9.0]]
    );
    assert_eq!(matrix.get_translation().x, -1.0);
    assert_eq!(matrix.get_scale().y, 20.0);
    matrix.set_identity();
    assert!(matrix == Matrix3x3D::from_elements(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0));

    let rows = Matrix3x3D::from_vectors(
        Vector3::new(1.0, 2.0, 3.0),
        Vector3::new(4.0, 5.0, 6.0),
        Vector3::new(7.0, 8.0, 10.0),
    );
    assert_eq!(rows.determinant(), -3.0);
    assert_eq!(rows.transpose().data[0], [1.0, 4.0, 7.0]);
    assert_eq!((2.0 * rows).data, (rows * 2.0).data);
    // First adjugate row (2, 4, -3) times 1.0 / Determinant(), as Inverse() computes it.
    assert_eq!(
        rows.inverse().data[0],
        [2.0, 4.0, -3.0].map(|cofactor: f64| cofactor * (1.0 / -3.0))
    );

    let mut nan = Matrix3x3D::new();
    nan.data[1][1] = f64::NAN;
    assert!(!nan.equals(&nan) && nan.not_equals(&nan));
    let mut negative_zero = Matrix3x3D::new();
    negative_zero.data[0][0] = -0.0;
    assert!(negative_zero == Matrix3x3D::new());

    let transformed = rows * Vector2::new(1.0, 1.0);
    assert_eq!((transformed.x, transformed.y), (6.0, 15.0));
    let transformed = rows * Vector3::new(1.0, 0.0, -1.0);
    assert_eq!(
        (transformed.x, transformed.y, transformed.z),
        (-2.0, -2.0, -3.0)
    );
}
