// SPDX-License-Identifier: GPL-3.0-or-later
//! Native checks of the offset port. Exact agreement with the pinned C++ is established by the
//! unit's difftest corpus (`rust/kicad-clipper2-p002-shim`).

use super::*;
use std::cell::Cell;

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
    let cb: DeltaCallback64 = Box::new(|_, _, _, _| Ok(5.0));
    assert!(co.execute_with_delta_callback(Some(cb), &mut out).is_ok());
    let first = out.clone();
    // A later plain Execute still uses the callback, as the C++ member keeps it.
    assert!(co.execute(50.0, &mut out).is_ok());
    assert_eq!(out, first);
}

/// A polygon whose offset union calls the Z callback: the overlapping second ring has zero
/// Z values the proxy cannot resolve on its own.
fn z_scene(co: &mut ClipperOffset) {
    let paths = vec![
        vec![
            Point64 { x: 0, y: 0, z: 4 },
            Point64 { x: 50, y: 0, z: 4 },
            Point64 { x: 50, y: 50, z: 9 },
            Point64 { x: 0, y: 50, z: 9 },
        ],
        vec![
            Point64 { x: 25, y: 25, z: 4 },
            Point64 { x: 75, y: 25, z: 0 },
            Point64 { x: 75, y: 75, z: 0 },
        ],
    ];
    assert!(co
        .add_paths(&paths, JoinType::Square, EndType::Polygon)
        .is_ok());
}

#[test]
fn delta_callback_error_keeps_state_and_partial_paths() {
    let mut co = ClipperOffset::with_defaults(BUDGET);
    assert!(co
        .add_paths(
            &vec![square(100), square(10)],
            JoinType::Miter,
            EndType::Polygon
        )
        .is_ok());
    let calls = Rc::new(Cell::new(0usize));
    let seen = Rc::clone(&calls);
    let cb: DeltaCallback64 = Box::new(move |_, _, _, _| {
        seen.set(seen.get() + 1);
        if seen.get() == 6 {
            Err(ClipperError::Callback(6))
        } else {
            Ok(5.0)
        }
    });
    let mut out = Paths64::new();
    // The first square's four vertices succeed; the second's second vertex throws.
    assert_eq!(
        co.execute_with_delta_callback(Some(cb), &mut out),
        Err(ClipperError::Callback(6))
    );
    assert_eq!(out.len(), 1, "the first path stays appended, as in the C++");
    assert_eq!(co.groups().len(), 1);
    assert_eq!(co.norms().len(), 4);
    // The callback stays assigned and the groups are intact for the next execution.
    assert!(co.execute(1.0, &mut out).is_ok());
    assert_eq!(calls.get(), 14);
    // The union merges the nested square into the outer one.
    assert_eq!(out.len(), 1);
}

#[test]
fn z_callback_error_keeps_the_callback() {
    let mut co = ClipperOffset::with_defaults(BUDGET);
    z_scene(&mut co);
    let calls = Rc::new(Cell::new(0usize));
    let seen = Rc::clone(&calls);
    let cb: FallibleZCallback64 = Box::new(move |_, _, _, _, ip| {
        seen.set(seen.get() + 1);
        if seen.get() == 1 {
            return Err(ClipperError::Callback(1));
        }
        ip.z = 7;
        Ok(())
    });
    assert!(co.set_fallible_z_callback(Some(cb)).is_ok());
    let mut out = Paths64::new();
    assert_eq!(co.execute(10.0, &mut out), Err(ClipperError::Callback(1)));
    assert!(
        out.is_empty(),
        "the union clears the solution before it throws"
    );
    assert!(co.execute(10.0, &mut out).is_ok());
    assert!(
        calls.get() >= 2,
        "the callback is invoked again after its exception"
    );
    assert_eq!(out.len(), 1);
}

#[test]
#[allow(clippy::panic)]
fn unwinding_callbacks_keep_groups_and_callbacks() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // A native unwind out of the delta callback leaves the groups in place.
    let mut co = ClipperOffset::with_defaults(BUDGET);
    assert!(co
        .add_path(&square(100), JoinType::Miter, EndType::Polygon)
        .is_ok());
    let cb: DeltaCallback64 = Box::new(|_, _, _, _| panic!("delta callback unwinds"));
    co.set_delta_callback(Some(cb));
    let mut out = Paths64::new();
    assert!(catch_unwind(AssertUnwindSafe(|| co.execute(5.0, &mut out))).is_err());
    assert_eq!(co.groups().len(), 1);
    co.set_delta_callback(None);
    assert!(co.execute(5.0, &mut out).is_ok());
    assert_eq!(out.len(), 1);

    // A native unwind out of the Z callback leaves it assigned.
    let mut co = ClipperOffset::with_defaults(BUDGET);
    z_scene(&mut co);
    let calls = Rc::new(Cell::new(0usize));
    let seen = Rc::clone(&calls);
    let cb: ZCallback64 = Box::new(move |_, _, _, _, ip| {
        seen.set(seen.get() + 1);
        if seen.get() == 1 {
            panic!("z callback unwinds");
        }
        ip.z = 7;
    });
    assert!(co.set_z_callback(Some(cb)).is_ok());
    assert!(catch_unwind(AssertUnwindSafe(|| co.execute(10.0, &mut out))).is_err());
    assert!(co.execute(10.0, &mut out).is_ok());
    assert!(calls.get() >= 2);
    assert_eq!(co.groups().len(), 1);
}
