// SPDX-License-Identifier: GPL-3.0-or-later
//! Behaviour checks for the portable crate. Exact parity with the C++ is established by the
//! live difftest (`port/cases/libs.kimath.include.math.application.pure.p001.jsonl`); these
//! tests guard internal consistency and documented edge cases.

use kicad_math::stream::{insert_str, StreamError, StreamInsert, StreamState, MAX_STREAM_FIELD};
use kicad_math::wide_int::{
    cross_wide, dot_wide, to_double, to_double_via_words, words_to_double, WideIntError,
};
use kicad_math::{Matrix3x3, Matrix3x3D, Vector2, Vector2L, Vector3};

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
        assert_eq!(
            value.insert(&mut StreamState::default()),
            Ok(text.into()),
            "{value:e}"
        );
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

fn state(precision: i64, apply: impl FnOnce(&mut kicad_math::FmtFlags)) -> StreamState {
    let mut state = StreamState {
        precision,
        ..StreamState::default()
    };
    apply(&mut state.flags);
    state
}

#[test]
fn stream_state_controls_double_formatting() {
    let eighth = Matrix3x3D::from_elements(0.125, 0.0, 0.0, 0.0, 0.125, 0.0, 0.0, 0.0, 0.125);
    let mut fixed2 = state(2, |flags| flags.fixed = true);
    assert_eq!(
        eighth.stream_text(&mut fixed2),
        Ok("| 0.12 0.00 0.00 |\n| 0.00 0.12 0.00 |\n| 0.00 0.00 0.12 |\n".into())
    );
    for (precision, apply, value, text) in [
        (
            3,
            (|f: &mut kicad_math::FmtFlags| f.scientific = true) as fn(&mut _),
            1234.5,
            "1.234e+03",
        ),
        (
            3,
            |f| {
                f.scientific = true;
                f.uppercase = true;
            },
            1234.5,
            "1.234E+03",
        ),
        (0, |f| f.fixed = true, 2.5, "2"),
        (
            0,
            |f| {
                f.fixed = true;
                f.showpoint = true;
            },
            2.5,
            "2.",
        ),
        (0, |_| (), 0.000123456, "0.0001"),
        (-1, |_| (), 1.0 / 3.0, "0.333333"),
        (4, |f| f.showpoint = true, 1.0, "1.000"),
        (6, |f| f.showpos = true, 0.0, "+0"),
        (6, |f| f.showpos = true, f64::NAN, "+nan"),
        (
            6,
            |f| {
                f.fixed = true;
                f.uppercase = true;
            },
            f64::INFINITY,
            "inf",
        ),
        (
            6,
            |f| {
                f.fixed = true;
                f.scientific = true;
            },
            1.0,
            "0x1p+0",
        ),
        (
            6,
            |f| {
                f.fixed = true;
                f.scientific = true;
                f.showpoint = true;
            },
            0.125,
            "0x1.p-3",
        ),
        (
            6,
            |f| {
                f.fixed = true;
                f.scientific = true;
                f.uppercase = true;
            },
            -3.0,
            "-0X1.8P+1",
        ),
        (
            6,
            |f| {
                f.fixed = true;
                f.scientific = true;
            },
            f64::from_bits(1),
            "0x0.0000000000001p-1022",
        ),
        (
            6,
            |f| {
                f.fixed = true;
                f.scientific = true;
            },
            0.0,
            "0x0p+0",
        ),
    ] {
        assert_eq!(
            value.insert(&mut state(precision, apply)),
            Ok(text.into()),
            "{value:e}"
        );
    }
}

#[test]
fn stream_width_applies_to_the_first_insertion_only() {
    let identity = Matrix3x3D::from_elements(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
    let mut padded = StreamState {
        width: 5,
        fill: '*',
        ..StreamState::default()
    };
    assert_eq!(
        identity.stream_text(&mut padded),
        Ok("***| 1 0 0 |\n| 0 1 0 |\n| 0 0 1 |\n".into())
    );
    assert_eq!(padded.width, 0);
    let mut left = state(6, |f| f.left = true);
    left.width = 4;
    left.fill = '_';
    assert_eq!(
        identity
            .stream_text(&mut left)
            .as_deref()
            .map(|s| s.get(..5)),
        Ok(Some("| __1"))
    );
}

#[test]
fn stream_state_controls_integer_formatting() {
    let ints = Matrix3x3::<i32>::from_elements(-1, 255, 0, 8, i32::MIN, i32::MAX, 1, 2, 3);
    let mut hex = state(6, |f| {
        f.dec = false;
        f.hex = true;
        f.showbase = true;
        f.uppercase = true;
    });
    assert_eq!(
        ints.stream_text(&mut hex),
        Ok("| 0XFFFFFFFF 0XFF 0 |\n| 0X8 0X80000000 0X7FFFFFFF |\n| 0X1 0X2 0X3 |\n".into())
    );
    let mut oct = state(6, |f| {
        f.dec = false;
        f.oct = true;
        f.showbase = true;
        f.showpos = true;
    });
    assert_eq!(
        ints.stream_text(&mut oct).as_deref().map(|s| s.get(..21)),
        Ok(Some("| 037777777777 0377 0"))
    );
    let mut showpos = state(6, |f| f.showpos = true);
    assert_eq!(
        ints.stream_text(&mut showpos)
            .as_deref()
            .map(|s| s.get(..13)),
        Ok(Some("| -1 +255 +0 "))
    );
}

#[test]
fn public_stream_resource_limits_return_errors_without_mutating_state() {
    let ones = Matrix3x3D::from_elements(1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
    for requested in [MAX_STREAM_FIELD + 1, 65_536, i64::MAX] {
        for field in ["precision", "width"] {
            let mut state = state(2, |f| f.fixed = true);
            state.width = 5;
            if field == "precision" {
                state.precision = requested;
            } else {
                state.width = requested;
            }
            let original = state;
            let expected = Err(StreamError::ResourceLimit {
                field,
                requested,
                maximum: MAX_STREAM_FIELD,
            });
            assert_eq!(ones.stream_text(&mut state), expected);
            assert_eq!(state, original);
            assert_eq!(1.0_f64.insert(&mut state), expected);
            assert_eq!(state, original);
            assert_eq!(1_i32.insert(&mut state), expected);
            assert_eq!(state, original);
            assert_eq!(insert_str("text", &mut state), expected);
            assert_eq!(state, original);
        }
    }

    // The supported upper boundary still formats every requested digit, with no clamp.
    let mut boundary = state(MAX_STREAM_FIELD, |f| f.fixed = true);
    let expected = format!("1.{}", "0".repeat(MAX_STREAM_FIELD as usize));
    assert_eq!(1.0_f64.insert(&mut boundary), Ok(expected));
    let mut negative = state(i64::MIN, |_| ());
    negative.width = i64::MIN;
    assert_eq!(1.0_f64.insert(&mut negative), Ok("1".into()));
    assert_eq!(negative.width, 0);
}

#[test]
fn integer_instantiation_matches_the_pinned_qa_test_and_refuses_undefined_inputs() {
    let qa = Matrix3x3::<i32>::from_vectors(
        Vector3::new(1, 1, 1),
        Vector3::new(2, 2, 2),
        Vector3::new(3, 3, 3),
    );
    let product = qa.mul_vector3(&Vector3::new(5, 5, 5));
    assert!(matches!(product, Ok(v) if (v.x, v.y, v.z) == (15, 30, 45)));
    let scaled = qa.mul_scalar(5);
    let expected = Matrix3x3::<i32>::from_elements(5, 5, 5, 10, 10, 10, 15, 15, 15);
    assert!(matches!(scaled, Ok(m) if m == expected));

    let mut rotated = Matrix3x3::<i32>::new();
    rotated.set_rotation(0);
    assert_eq!(rotated.data[0], [1, 0, 0]);
    rotated.set_rotation(165_707_065); // cos rounds to exactly -1.0
    assert_eq!(rotated.data[0][0], -1);

    let halves = Matrix3x3::<i32>::from_elements(2, 0, 0, 0, 2, 0, 0, 0, 1);
    assert!(matches!(halves.inverse(), Ok(m) if m.data == [[0, 0, 0], [0, 0, 0], [0, 0, 1]]));
    let singular = Matrix3x3::<i32>::from_elements(1, 2, 3, 2, 4, 6, 1, 1, 1);
    assert!(matches!(
        singular.inverse(),
        Err(kicad_math::MatrixError::Conversion { .. })
    ));
    let huge = Matrix3x3::<i32>::from_elements(i32::MAX, 0, 0, 0, 2, 0, 0, 0, 1);
    assert!(matches!(
        huge.determinant(),
        Err(kicad_math::MatrixError::Overflow {
            upstream: "MATRIX3x3<int>::Determinant"
        })
    ));
    assert!(huge.mul_scalar(2).is_err());
    assert!(matches!(
        huge.mul_scalar_double(-0.5),
        Ok(m) if m.data[0][0] == -1_073_741_823
    ));
}
