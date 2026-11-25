// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_math::{box2::*, util, Vector2, VectorDomainError};
#[test]
fn clamp_continuation_and_logging_boundary_are_separate() {
    assert_eq!(util::ki_round::<_, i32>(f64::NAN, true), Ok(0));
    assert_eq!(util::ki_round::<_, i32>(f64::INFINITY, true), Ok(i32::MAX));
    assert_eq!(util::ki_round::<_, i32>(-0.5, false), Ok(-1));
    assert!(matches!(
        util::ki_round::<_, i32>(f64::INFINITY, false),
        Err(VectorDomainError::Unported {
            unit: "libs.kimath.src.math.util.application.ui.p001",
            ..
        })
    ));
    assert_eq!(
        util::ki_round::<_, i64>(9_007_199_254_740_993_i64, false),
        Ok(9_007_199_254_740_993)
    );
}
#[test]
fn wide_hypot_and_integer_root_boundaries() {
    assert_eq!(
        util::rounded_hypot(i64::MIN, i64::MIN),
        Ok(13_043_817_825_332_782_212)
    );
    assert_eq!(util::isqrt(u64::MAX), Ok(4_294_967_295));
    assert_eq!(util::isqrt(-1_i64), Ok(3_037_000_499));
    assert_eq!(util::ct_sqrt(0_i32), Ok(0));
}
#[test]
fn negative_size_inflate_and_uninitialized_merge() -> util::Result<()> {
    let mut b = Box2I::default();
    b.set_origin(Vector2::new(10, 20));
    b.set_size(Vector2::new(-4, -8));
    b.inflate(-100, -100)?;
    assert_eq!(b.origin(), Vector2::new(12, 24));
    assert_eq!(b.size(), Vector2::new(0, 0));
    let mut empty = Box2I::default();
    empty.merge(&Box2I::default())?;
    assert!(!empty.is_valid());
    empty.merge_point(Vector2::new(1, 2))?;
    assert!(empty.is_valid());
    Ok(())
}
#[test]
fn outward_saturation_and_unsigned_box_domain_are_not_cast_ub() -> util::Result<()> {
    let mut b = Box2I::new(Vector2::new(i32::MAX - 1, 0), Vector2::new(1, 1))?;
    b.inflate(i32::MAX, i32::MAX)?;
    assert_eq!(b.right()?, i32::MAX);
    // Generic KiCheckedCast<long,int> is a defined narrowing conversion on Linux.
    let b = Box2I::new(Vector2::new(i32::MAX, 0), Vector2::new(1, 0))?;
    assert_eq!(b.right()?, i32::MIN);
    let mut l = Box2L::default();
    l.set_origin(Vector2::new(i64::MAX, 0));
    assert_eq!(
        l.offset(Vector2::new(1, 0)),
        Err(VectorDomainError::IntegerOverflow)
    );
    Ok(())
}
