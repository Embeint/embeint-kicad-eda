// SPDX-License-Identifier: GPL-3.0-or-later
//! The `VECTOR2` arithmetic and conversion co-ported for the `BEZIER<T>` bodies of unit
//! `libs.kimath.include.application.pure.p001`, at the instantiations they use (`TYPE_SUPPORT.md`).
//! Floats are compared by bits; integer conversion is checked at its range boundaries.

use kicad_math::{Vector2, Vector2D, Vector2I, VectorDomainError};

const INDEFINITE: u64 = 0xfff8_0000_0000_0000;

fn bits(vector: Vector2D) -> [u64; 2] {
    [vector.x.to_bits(), vector.y.to_bits()]
}

#[test]
fn int_difference_is_an_int_subtraction_with_overflow_refused() {
    let a = Vector2I::new(5, -7);
    let b = Vector2I::new(-3, 11);
    let difference = a.checked_sub(b);
    assert_eq!(difference.map(|v| (v.x, v.y)), Ok((8, -18)));
    let extremes = Vector2I::new(i32::MAX, i32::MIN).checked_sub(Vector2I::new(-1, 0));
    assert_eq!(
        extremes.map(|v| (v.x, v.y)),
        Err(VectorDomainError::IntegerOverflow)
    );
    // Either component overflowing is undefined.
    let y_only = Vector2I::new(0, i32::MIN).checked_sub(Vector2I::new(0, 1));
    assert_eq!(
        y_only.map(|v| (v.x, v.y)),
        Err(VectorDomainError::IntegerOverflow)
    );
    let edge = Vector2I::new(i32::MAX, i32::MIN).checked_sub(Vector2I::new(0, 0));
    assert_eq!(edge.map(|v| (v.x, v.y)), Ok((i32::MAX, i32::MIN)));
}

#[test]
fn double_operations_are_componentwise_ieee() {
    let a = Vector2D::new(0.1, -0.0);
    let b = Vector2D::new(0.2, 0.0);
    assert_eq!(bits(a + b), [(0.1_f64 + 0.2).to_bits(), 0]);
    assert_eq!(
        bits(a - b),
        [(0.1_f64 - 0.2).to_bits(), 0x8000_0000_0000_0000]
    );
    assert_eq!(
        bits(3.0 * a),
        [(3.0 * 0.1_f64).to_bits(), 0x8000_0000_0000_0000]
    );
    assert_eq!(
        bits(f64::MAX * Vector2D::new(2.0, -2.0)),
        [f64::INFINITY.to_bits(), f64::NEG_INFINITY.to_bits()]
    );
}

#[test]
fn int_components_promote_exactly_to_double() {
    let ints = Vector2I::new(i32::MIN, i32::MAX);
    assert_eq!(
        bits(0.5 * ints),
        [
            (-1_073_741_824.0_f64).to_bits(),
            1_073_741_823.5_f64.to_bits()
        ]
    );
    let sum = ints + Vector2D::new(0.25, -0.25);
    assert_eq!(
        bits(sum),
        [
            (-2_147_483_647.75_f64).to_bits(),
            2_147_483_646.75_f64.to_bits()
        ]
    );
    // 0.0 * int keeps the sign of the product: -0.0 for a negative component.
    assert_eq!(bits(0.0 * Vector2I::new(-1, 1)), [0x8000_0000_0000_0000, 0]);
}

#[test]
/// One NaN operand and generated NaNs are the oracle platform's; two NaN operands keep the left
/// one (an explicit choice the compiled reference does not follow consistently).
fn nan_results_follow_the_explicit_rules() {
    let left = f64::from_bits(0x7ff8_0000_0000_0011);
    let right = f64::from_bits(0xfff0_0000_0000_0022); // signalling
    let sum = Vector2D::new(left, 1.0) + Vector2D::new(right, right);
    assert_eq!(bits(sum), [0x7ff8_0000_0000_0011, 0xfff8_0000_0000_0022]);
    let product = right * Vector2D::new(left, 0.0);
    assert_eq!(
        bits(product),
        [0xfff8_0000_0000_0022, 0xfff8_0000_0000_0022]
    );
    let invalid = Vector2D::new(f64::INFINITY, 0.0) - Vector2D::new(f64::INFINITY, -0.0);
    assert_eq!(bits(invalid), [INDEFINITE, 0]);
    assert_eq!(
        bits(f64::INFINITY * Vector2I::new(0, -3)),
        [INDEFINITE, f64::NEG_INFINITY.to_bits()]
    );
}

fn convert(x: f64, y: f64) -> Result<(i32, i32), VectorDomainError> {
    Vector2::<i32>::try_from(Vector2D::new(x, y)).map(|v| (v.x, v.y))
}

#[test]
fn double_to_int_clamps_then_truncates() {
    assert_eq!(convert(2.9, -2.9), Ok((2, -2)));
    assert_eq!(convert(-0.0, 0.5), Ok((0, 0)));
    assert_eq!(convert(5e-324, -5e-324), Ok((0, 0)));
    // The clamp bounds are exactly double( INT_MIN ) and double( INT_MAX ).
    assert_eq!(
        convert(2_147_483_647.0, -2_147_483_648.0),
        Ok((i32::MAX, i32::MIN))
    );
    assert_eq!(
        convert(2_147_483_647.9, -2_147_483_648.9),
        Ok((i32::MAX, i32::MIN))
    );
    assert_eq!(
        convert(2_147_483_648.0, -2_147_483_649.0),
        Ok((i32::MAX, i32::MIN))
    );
    assert_eq!(
        convert(f64::INFINITY, f64::NEG_INFINITY),
        Ok((i32::MAX, i32::MIN))
    );
    assert_eq!(convert(f64::MAX, -f64::MAX), Ok((i32::MAX, i32::MIN)));
}

#[test]
fn nan_to_int_is_refused() {
    assert_eq!(convert(f64::NAN, 0.0), Err(VectorDomainError::NanToInteger));
    assert_eq!(
        convert(1.0, f64::from_bits(0x7ff0_0000_0000_0001)),
        Err(VectorDomainError::NanToInteger)
    );
}
