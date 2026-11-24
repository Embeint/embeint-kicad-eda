// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_math::vector::{Vector2CastFrom, Vector2Scalar};
use kicad_math::{Vector2, Vector3, VectorDomainError as E};

#[test]
fn scalar_alias_reads_follow_each_write() {
    let mut v = Vector2::new(2_i32, 3);
    assert!(v.mul_assign_operand(Vector2Scalar::X).is_ok());
    assert_eq!(v, Vector2::new(4, 12));
    let mut v = Vector2::new(2_i32, 3);
    assert!(v.add_assign_operand(Vector2Scalar::X).is_ok());
    assert_eq!(v, Vector2::new(4, 7));
    let mut v = Vector2::new(2_i32, 3);
    assert!(v.sub_assign_operand(Vector2Scalar::X).is_ok());
    assert_eq!(v, Vector2::new(0, 3));
    let mut v = Vector2::new(2_i32, 3);
    assert!(v.mul_assign_operand(Vector2Scalar::Y).is_ok());
    assert_eq!(v, Vector2::new(6, 9));
}
#[test]
fn checked_intermediates_and_mutation_after_failure() {
    let mut v = Vector2::new(2_i32, i32::MAX);
    assert_eq!(v.add_assign(Vector2::new(1, 1)), Err(E::IntegerOverflow));
    assert_eq!(v, Vector2::new(3, i32::MAX));
    let mut v = Vector3::new(2_i32, 3, i32::MAX);
    assert_eq!(v.mul_assign_scalar(2), Err(E::IntegerOverflow));
    assert_eq!(v, Vector3::new(4, 6, i32::MAX));
    let a = Vector3::new(0_i32, 50_000, 50_000);
    assert_eq!(a.cross(a), Err(E::IntegerOverflow));
    let a = Vector2::new(i64::MAX, 0_i64);
    assert_eq!(a.cross(a), Ok(0));
    let a = Vector2::new(i64::MAX, i64::MAX);
    assert_eq!(a.cross(a), Err(E::IntegerOverflow));
    assert_eq!(
        Vector2::new(i32::MIN, 0).perpendicular(),
        Ok(Vector2::new(0, i32::MIN))
    );
    assert_eq!(
        Vector2::new(0, i32::MIN).perpendicular(),
        Err(E::IntegerOverflow)
    );
    assert_eq!(
        Vector2::new(i32::MIN, i32::MIN).squared_euclidean_norm(),
        Ok(i64::MAX)
    );
    assert_eq!(
        Vector2::new(i64::MIN, i64::MIN).squared_euclidean_norm(),
        Ok(i64::MAX)
    );
    assert_eq!(
        Vector2::new(i32::MIN, i32::MIN).dot(Vector2::new(i32::MIN, i32::MIN)),
        Err(E::IntegerOverflow)
    );
}
#[test]
fn actual_cast_intervals_and_distances() {
    let upper = 9_223_372_036_854_775_808.0_f64;
    assert_eq!(i64::cast2(f64::INFINITY), Err(E::CastOutOfRange));
    assert_eq!(i64::cast2(upper), Err(E::CastOutOfRange));
    assert_eq!(
        i64::cast2(f64::from_bits(upper.to_bits() - 1)),
        Ok(9_223_372_036_854_774_784)
    );
    assert_eq!(i64::cast2(-upper), Ok(i64::MIN));
    assert_eq!(
        Vector3::new(-0.75, 0.0, 1.0).try_cast::<u32>(),
        Ok(Vector3::new(0, 0, 1))
    );
    assert_eq!(
        Vector3::new(-1.0, 0.0, 1.0).try_cast::<u32>(),
        Err(E::CastOutOfRange)
    );
    assert_eq!(
        Vector2::new(9_007_199_254_740_992_i64, 0).distance(Vector2::new(9_007_199_254_740_993, 0)),
        Ok(1.0)
    );
    assert_eq!(
        Vector2::new(0_i32, 0).resize(i32::MIN),
        Ok(Vector2::new(0, 0))
    );
}
#[test]
fn normalize_returns_copy_and_check_failure_preserves_bits() {
    let mut v = Vector3::new(65_536_u32, 0, 0);
    assert_eq!(v.euclidean_norm(), Ok(0));
    assert_eq!(v.normalize(), Ok(Vector3::new(65_536, 0, 0)));
    let mut v = Vector3::new(65_536_u32, 2, 0);
    assert_eq!(v.normalize(), Ok(Vector3::new(32_768, 1, 0)));
    let mut v = Vector3::new(3.0_f64, 4.0, 0.0);
    assert_eq!(v.normalize(), Ok(Vector3::new(0.6, 0.8, 0.0)));
    assert_eq!(v, Vector3::new(0.6, 0.8, 0.0));
    let n = f64::from_bits(0x7ff0_0000_0000_0123);
    let mut v = Vector3::new(n, -0.0, 2.0);
    assert_eq!(v.normalize().map(|copy| copy.x.to_bits()), Ok(n.to_bits()));
    assert_eq!(v.x.to_bits(), n.to_bits());
    assert_eq!(v.y.to_bits(), (-0.0_f64).to_bits());
    let mut tiny = Vector3::new(f64::from_bits(1), -0.0, 0.0);
    assert_eq!(tiny.normalize().map(|copy| copy.x.to_bits()), Ok(1));
    assert_eq!(tiny.y.to_bits(), (-0.0_f64).to_bits());
}
#[test]
fn unordered_compare_and_second_reference_ties() {
    let a = Vector2::new(f64::from_bits(0x7ff0_0000_0000_0123), 1.0);
    let b = Vector2::new(f64::from_bits(0xfff8_0000_0000_0456), 2.0);
    assert!(!a.less(&b) && !a.less_equal(&b) && !a.greater(&b) && !a.greater_equal(&b));
    assert_eq!(a.lexicographical_compare(&b), -1);
    assert_eq!(a.partial_cmp(&b), None);
    assert!(std::ptr::eq(a.lexicographical_min(&b), &b));
    assert!(std::ptr::eq(a.lexicographical_max(&b), &b));
    let a = Vector2::new(0.0_f64, 0.0);
    let b = Vector2::new(-0.0, -0.0);
    assert!(std::ptr::eq(a.lexicographical_min(&b), &b));
    assert_eq!(a.lexicographical_min(&b).x.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(
        Vector2::<f64>::ECOORD_MIN.to_bits(),
        f64::MIN_POSITIVE.to_bits()
    );
}

#[test]
fn arithmetic_preserves_a_unique_quieted_nan_and_defers_ambiguous_sites() {
    let n = f64::from_bits(0xfff0_0000_0000_0123);
    let q = n.to_bits() | 0x0008_0000_0000_0000;
    assert_eq!(
        Vector2::new(n, 2.0)
            .try_add(Vector2::new(1.0, n))
            .map(|v| (v.x.to_bits(), v.y.to_bits())),
        Ok((q, q))
    );
    assert_eq!(
        Vector2::new(n, n)
            .dot(Vector2::new(1.0, 1.0))
            .map(f64::to_bits),
        Ok(q)
    );
    assert_eq!(
        Vector2::new(n, n).dot(Vector2::new(n, n)).map(f64::to_bits),
        Ok(q)
    );
    assert_eq!(
        Vector2::new(0.0, 1.0)
            .try_mul_scalar(f64::INFINITY)
            .map(|v| v.x.to_bits()),
        Ok(0xfff8_0000_0000_0000)
    );
    let other = f64::from_bits(0x7ff8_0000_0000_0456);
    assert!(matches!(
        Vector2::new(n, 1.0).try_add(Vector2::new(other, 1.0)),
        Err(E::Unported {
            unit: "libs.kimath.include.math.application.ui.p002",
            ..
        })
    ));
    assert_eq!(
        Vector3::new(n, 0.0, 1.0).euclidean_norm().map(f64::to_bits),
        Ok(q)
    );
}
