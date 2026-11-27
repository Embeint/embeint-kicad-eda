// SPDX-License-Identifier: GPL-3.0-or-later
//! Native checks of the engine remainder. Exact agreement with the pinned C++ is established by
//! the unit's difftest corpus (`rust/kicad-clipper2-p002-shim`); these cover structural
//! properties that need no oracle.

use crate::clipper_core::{FillRule, Path64, Paths64, Point64};
use crate::clipper_engine::{ClipType, Clipper64, PolyTree64};
use crate::Budget;

fn square(x: i64, y: i64, size: i64) -> Path64 {
    vec![
        Point64::xy(x, y),
        Point64::xy(x + size, y),
        Point64::xy(x + size, y + size),
        Point64::xy(x, y + size),
    ]
}

#[test]
fn union_of_overlapping_squares_is_one_ring() {
    let mut c = Clipper64::default();
    let subjects: Paths64 = vec![square(0, 0, 10), square(5, 5, 10)];
    assert!(c.add_subject(&subjects).is_ok());
    let mut closed = Paths64::new();
    let done = c.execute(ClipType::Union, FillRule::NonZero, &mut closed);
    assert_eq!(done, Ok(true));
    assert_eq!(closed.len(), 1);
    assert_eq!(closed.first().map(Vec::len), Some(8));
}

#[test]
fn no_clip_returns_no_paths() {
    let mut c = Clipper64::default();
    assert!(c.add_subject(&vec![square(0, 0, 10)]).is_ok());
    let mut closed = Paths64::new();
    assert_eq!(
        c.execute(ClipType::NoClip, FillRule::EvenOdd, &mut closed),
        Ok(true)
    );
    assert!(closed.is_empty());
}

#[test]
fn polytree_nests_a_hole() -> crate::Result<()> {
    let mut c = Clipper64::default();
    c.add_subject(&vec![square(0, 0, 30)])?;
    c.add_clip(&vec![square(10, 10, 10)])?;
    let budget = Budget {
        nodes: 64,
        points: 4096,
    };
    let mut tree = PolyTree64::new(budget)?;
    let done = c.execute_tree(ClipType::Difference, FillRule::NonZero, &mut tree)?;
    assert!(done);
    assert_eq!(tree.count(PolyTree64::ROOT)?, 1);
    let outer = tree.child(PolyTree64::ROOT, 0)?;
    assert_eq!(tree.count(outer)?, 1);
    assert!(!tree.is_hole(outer)?);
    let hole = tree.child(outer, 0)?;
    assert!(tree.is_hole(hole)?);
    Ok(())
}
