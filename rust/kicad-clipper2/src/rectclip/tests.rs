// SPDX-License-Identifier: GPL-3.0-or-later
//! Hand-checked behaviour of the rectangle clipping port. The exact comparison against the
//! pinned C++ is the corpus in `port/cases/thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003.jsonl`.
use super::*;

const BUDGET: RectClipBudget = RectClipBudget {
    ops: 1 << 16,
    steps: 1 << 24,
};

fn pts(points: &[(i64, i64)]) -> Path64 {
    points.iter().map(|&(x, y)| Point64::xy(x, y)).collect()
}

fn xy(paths: &Paths64) -> Vec<Vec<(i64, i64)>> {
    paths
        .iter()
        .map(|p| p.iter().map(|q| (q.x, q.y)).collect())
        .collect()
}

#[test]
fn square_overlapping_corner() -> Result<()> {
    let rect = Rect64::new(0, 0, 10, 10);
    let mut clip = RectClip64::new(&rect, BUDGET)?;
    let out = clip.execute(&[pts(&[(5, 5), (15, 5), (15, 15), (5, 15)])])?;
    assert_eq!(out.len(), 1);
    let mut got = xy(&out)[0].clone();
    got.sort_unstable();
    assert_eq!(got, vec![(5, 5), (5, 10), (10, 5), (10, 10)]);
    Ok(())
}

#[test]
fn inside_outside_and_containing() -> Result<()> {
    let rect = Rect64::new(0, 0, 10, 10);
    let mut clip = RectClip64::new(&rect, BUDGET)?;
    let inside = pts(&[(1, 1), (2, 1), (2, 2)]);
    assert_eq!(
        xy(&clip.execute(std::slice::from_ref(&inside))?),
        xy(&vec![inside])
    );
    assert!(clip
        .execute(&[pts(&[(20, 20), (30, 20), (30, 30)])])?
        .is_empty());
    let big = pts(&[(-5, -5), (15, -5), (15, 15), (-5, 15)]);
    let out = clip.execute(&[big])?;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].len(), 4);
    // Fewer than three points and empty or inverted rectangles give nothing.
    assert!(clip.execute(&[pts(&[(1, 1), (2, 2)])])?.is_empty());
    let mut empty = RectClip64::new(&Rect64::new(10, 0, 0, 10), BUDGET)?;
    assert!(empty.execute(&[pts(&[(1, 1), (2, 1), (2, 2)])])?.is_empty());
    Ok(())
}

#[test]
fn lines_cross_the_rectangle() -> Result<()> {
    let rect = Rect64::new(0, 0, 10, 10);
    let mut clip = RectClipLines64::new(&rect, BUDGET)?;
    let out = clip.execute(&[pts(&[(-5, 5), (15, 5)])])?;
    assert_eq!(xy(&out), vec![vec![(0, 5), (10, 5)]]);
    let out = clip.execute(&[pts(&[(5, 5), (15, 5), (15, 7), (5, 7)])])?;
    assert_eq!(xy(&out), vec![vec![(5, 5), (10, 5)], vec![(10, 7), (5, 7)]]);
    Ok(())
}

#[test]
fn midpoint_overflow_is_refused() {
    let rect = Rect64::new(i64::MAX - 1, 0, i64::MAX, 10);
    assert!(matches!(
        RectClip64::new(&rect, BUDGET),
        Err(ClipperError::Undefined(_))
    ));
}

#[test]
fn budgets_are_resource_limits() -> Result<()> {
    let rect = Rect64::new(0, 0, 10, 10);
    let path = pts(&[(5, 5), (15, 5), (15, 15), (5, 15)]);
    let mut clip = RectClip64::new(
        &rect,
        RectClipBudget {
            ops: 2,
            steps: 1 << 20,
        },
    )?;
    assert_eq!(
        clip.execute(std::slice::from_ref(&path)),
        Err(ClipperError::ResourceLimit)
    );
    let mut clip = RectClip64::new(
        &rect,
        RectClipBudget {
            ops: 1 << 10,
            steps: 3,
        },
    )?;
    assert_eq!(
        clip.execute(std::slice::from_ref(&path)),
        Err(ClipperError::ResourceLimit)
    );
    // The object stays usable after a refusal.
    let mut clip = RectClip64::new(
        &rect,
        RectClipBudget {
            ops: 2,
            steps: 1 << 20,
        },
    )?;
    let _ = clip.execute(std::slice::from_ref(&path));
    clip.budget = BUDGET;
    assert_eq!(clip.execute(&[path])?.len(), 1);
    Ok(())
}

#[test]
fn location_helpers() {
    assert_eq!(get_adjacent_location(Location::Inside, true), Location::Top);
    assert_eq!(
        get_adjacent_location(Location::Inside, false),
        Location::Bottom
    );
    assert!(are_opposites(Location::Inside, Location::Right));
    assert!(heading_clockwise(Location::Inside, Location::Top));
    assert!(start_locs_are_clockwise(&[Location::Left, Location::Top]));
    assert!(!start_locs_are_clockwise(&[
        Location::Left,
        Location::Bottom
    ]));
}
