// SPDX-License-Identifier: GPL-3.0-or-later
//! NaN operand-priority regressions for the `MATRIX3x3<double>` arithmetic.
//!
//! `tests/data/nan_priority_witnesses.txt` holds one witness for every observable node of
//! the seven generated sites (136 of 143 nodes; the other 7 are proven equivalent, see
//! `port/tools/kimath_nan/README.md`). Each input was replayed through the pinned oracle
//! (sha256 `0bac7567…`); its genuine output is the expected value. Swapping the operand order
//! of the witnessed node alone gives the recorded flipped output, which differs, so each
//! line fails if that one node's order regresses. Every public entry point and operator for
//! the site is checked.

use kicad_math::{Matrix3x3D, Vector2, Vector3};

const WITNESSES: &str = include_str!("data/nan_priority_witnesses.txt");

fn words(text: &str) -> Vec<u64> {
    text.split_whitespace()
        .map(|word| {
            let parsed = u64::from_str_radix(word, 16);
            assert!(parsed.is_ok(), "malformed word {word:?}");
            parsed.unwrap_or_default()
        })
        .collect()
}

fn matrix(w: &[u64]) -> Matrix3x3D {
    let f = |i: usize| f64::from_bits(w[i]);
    Matrix3x3D::from_elements(f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7), f(8))
}

fn matrix_bits(m: &Matrix3x3D) -> Vec<u64> {
    m.data.iter().flatten().map(|v| v.to_bits()).collect()
}

/// Every public way of evaluating `site` on `input`, as output words.
fn evaluate(site: &str, input: &[u64]) -> Vec<Vec<u64>> {
    let f = |i: usize| f64::from_bits(input[i]);
    match site {
        "determinant-doubleops" => vec![vec![matrix(input).determinant().to_bits()]],
        "inverse" => vec![matrix_bits(&matrix(input).inverse())],
        "mul_matrix-doubleops" => {
            let (a, b) = (matrix(&input[..9]), matrix(&input[9..]));
            vec![matrix_bits(&a.mul_matrix(&b)), matrix_bits(&(a * b))]
        }
        "mul_vector2-doubleops" => {
            let (m, v) = (matrix(input), Vector2::new(f(9), f(10)));
            [m.mul_vector2(&v), m * v]
                .iter()
                .map(|r| vec![r.x.to_bits(), r.y.to_bits()])
                .collect()
        }
        "mul_vector3-doubleops" => {
            let (m, v) = (matrix(input), Vector3::new(f(9), f(10), f(11)));
            [m.mul_vector3(&v), m * v]
                .iter()
                .map(|r| vec![r.x.to_bits(), r.y.to_bits(), r.z.to_bits()])
                .collect()
        }
        "mul_scalar-doubleops" => {
            let m = matrix(input);
            vec![matrix_bits(&m.mul_scalar(f(9))), matrix_bits(&(m * f(9)))]
        }
        "scalar_mul-doubleops" => {
            let m = matrix(input);
            vec![
                matrix_bits(&Matrix3x3D::scalar_mul(f(9), &m)),
                matrix_bits(&(f(9) * m)),
            ]
        }
        _ => Vec::new(),
    }
}

#[test]
fn every_observable_node_keeps_the_oracle_operand_order() {
    let mut per_site = std::collections::BTreeMap::<&str, usize>::new();
    for line in WITNESSES.lines().filter(|line| !line.starts_with('#')) {
        let parts: Vec<&str> = line.split(" | ").collect();
        assert_eq!(parts.len(), 4, "{line}");
        let head: Vec<&str> = parts[0].split_whitespace().collect();
        assert_eq!(head.len(), 3, "{line}");
        let (input, oracle, flipped) = (words(parts[1]), words(parts[2]), words(parts[3]));
        assert_ne!(
            oracle, flipped,
            "witness does not distinguish the flip: {line}"
        );
        let results = evaluate(head[0], &input);
        assert!(!results.is_empty(), "unknown site: {line}");
        for got in results {
            assert_eq!(
                got.iter().map(|w| format!("{w:016x}")).collect::<Vec<_>>(),
                oracle
                    .iter()
                    .map(|w| format!("{w:016x}"))
                    .collect::<Vec<_>>(),
                "{} node {} case {}",
                head[0],
                head[1],
                head[2]
            );
        }
        *per_site.entry(head[0]).or_default() += 1;
    }
    let expected = [
        ("determinant-doubleops", 9),
        ("inverse", 41),
        ("mul_matrix-doubleops", 45),
        ("mul_scalar-doubleops", 9),
        ("mul_vector2-doubleops", 8),
        ("mul_vector3-doubleops", 15),
        ("scalar_mul-doubleops", 9),
    ];
    assert_eq!(per_site.into_iter().collect::<Vec<_>>(), expected);
}

#[test]
fn integer_scalars_meet_at_most_one_nan_per_product() {
    // The converted int is never a NaN: each member keeps its own quieted NaN, and an
    // invalid `inf * 0` gives the SSE2 default NaN, in either operand order.
    let m = Matrix3x3D::from_elements(
        f64::from_bits(0x7ff0_0000_0000_0001),
        f64::from_bits(0xfff8_0000_0000_0002),
        f64::INFINITY,
        1.5,
        -0.0,
        f64::NEG_INFINITY,
        2.0,
        3.0,
        4.0,
    );
    let expected = |scalar: i32| {
        let s = f64::from(scalar);
        let product = |v: f64| {
            if v.is_nan() {
                v.to_bits() | 0x0008_0000_0000_0000
            } else if (v * s).is_nan() {
                0xfff8_0000_0000_0000
            } else {
                (v * s).to_bits()
            }
        };
        m.data
            .iter()
            .flatten()
            .map(|&v| product(v))
            .collect::<Vec<_>>()
    };
    for scalar in [0, 1, -3, i32::MAX, i32::MIN] {
        assert_eq!(matrix_bits(&m.mul_scalar_int(scalar)), expected(scalar));
        assert_eq!(
            matrix_bits(&Matrix3x3D::scalar_mul_int(scalar, &m)),
            expected(scalar)
        );
    }
}
