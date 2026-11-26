// SPDX-License-Identifier: GPL-3.0-or-later
//! Native checks of the offset port. Exact agreement with the pinned C++ is established by the
//! unit's difftest corpus (`rust/kicad-clipper2-p002-shim`).

use super::*;

const BUDGET: Budget = Budget {
    nodes: 256,
    points: 1 << 16,
};

fn square(size: i64) -> Path64 {
    vec![
        Point64::xy(0, 0),
        Point64::xy(size, 0),
        Point64::xy(size, size),
        Point64::xy(0, size),
    ]
}

#[test]
fn std_min_keeps_the_first_operand_on_nan() {
    assert!(std_min(f64::NAN, 1.0).is_nan());
    assert_eq!(std_min(1.0, f64::NAN), 1.0);
    assert_eq!(std_min(2.0, 1.0), 1.0);
}

#[test]
fn miter_square_grows_by_delta() {
    let mut co = ClipperOffset::new(2.0, 0.0, false, false, BUDGET);
    assert!(co
        .add_path(&square(100), JoinType::Miter, EndType::Polygon)
        .is_ok());
    let mut out = Paths64::new();
    assert!(co.execute(10.0, &mut out).is_ok());
    assert_eq!(out.len(), 1);
    let xs: Vec<i64> = out.iter().flatten().map(|p| p.x).collect();
    assert_eq!(xs.iter().min(), Some(&-10));
    assert_eq!(xs.iter().max(), Some(&110));
}

#[test]
fn single_round_point_defers_to_the_header_unit() {
    let mut co = ClipperOffset::with_defaults(BUDGET);
    assert!(co
        .add_path(&vec![Point64::xy(5, 5)], JoinType::Round, EndType::Polygon)
        .is_ok());
    let mut out = Paths64::new();
    assert_eq!(
        co.execute(4.0, &mut out),
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::Ellipse<int64_t>",
            unit: INCLUDE_P002,
        })
    );
}

#[test]
fn point_budget_is_enforced() {
    let small = Budget {
        nodes: 4,
        points: 8,
    };
    let mut co = ClipperOffset::new(2.0, 0.0, false, false, small);
    assert!(co
        .add_path(&square(1000), JoinType::Round, EndType::Polygon)
        .is_ok());
    let mut out = Paths64::new();
    assert_eq!(
        co.execute(100.0, &mut out),
        Err(ClipperError::ResourceLimit)
    );
}

#[test]
fn delta_callback_stays_assigned() {
    let mut co = ClipperOffset::with_defaults(BUDGET);
    assert!(co
        .add_path(&square(100), JoinType::Miter, EndType::Polygon)
        .is_ok());
    let mut out = Paths64::new();
    let cb: DeltaCallback64 = Box::new(|_, _, _, _| 5.0);
    assert!(co.execute_with_delta_callback(Some(cb), &mut out).is_ok());
    let first = out.clone();
    // A later plain Execute still uses the callback, as the C++ member keeps it.
    assert!(co.execute(50.0, &mut out).is_ok());
    assert_eq!(out, first);
}
