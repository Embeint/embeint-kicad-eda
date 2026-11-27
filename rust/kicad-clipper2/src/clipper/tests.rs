// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit checks of the `clipper.h` port. Values marked "oracle" were produced by the pinned C++
//! adapter (port/oracle/shims/clipper2_include_p002.cpp); the full comparison is the corpus.
use super::*;
use crate::clipper_core::Point64;
use crate::clipper_minkowski::{minkowski, minkowski_sum_64};
use crate::clipper_triangulation::{triangulate_64, CLIPPER2_VERSION, TRIANGULATION_SRC};

fn p(x: i64, y: i64) -> Point64 {
    Point::xy(x, y)
}

fn xy(path: &[Point64]) -> Vec<(i64, i64, i64)> {
    path.iter().map(|q| (q.x, q.y, q.z)).collect()
}

#[test]
fn ellipse_matches_oracle() {
    // oracle: Ellipse(Point64(0, 0, 7), 10.0) -> 9 steps, z = 0 on every point.
    let center = Point {
        x: 0i64,
        y: 0,
        z: 7,
    };
    let path = ellipse(&center, 10.0, 0.0, 0, 1 << 20);
    assert_eq!(
        path.map(|q| xy(&q)),
        Ok(vec![
            (10, 0, 0),
            (8, 6, 0),
            (2, 10, 0),
            (-5, 9, 0),
            (-9, 3, 0),
            (-9, -3, 0),
            (-5, -9, 0),
            (2, -10, 0),
            (8, -6, 0)
        ])
    );
    assert_eq!(ellipse(&center, 0.0, 5.0, 0, 16), Ok(Vec::new()));
    assert_eq!(
        ellipse(&center, 10.0, 0.0, 0, 8),
        Err(ClipperError::ResourceLimit)
    );
    // A NaN radius with computed steps converts NaN to size_t: undefined.
    assert!(matches!(
        ellipse(&center, 1.0, f64::NAN, 0, 16),
        Err(ClipperError::Undefined(_))
    ));
    // steps 0 after the formula still emits the first point.
    let tiny = ellipse(
        &Point {
            x: 1.0f64,
            y: 2.0,
            z: 0,
        },
        0.1,
        0.0,
        0,
        16,
    );
    assert_eq!(tiny.map(|q| q.len()), Ok(1));
}

#[test]
fn get_next_and_prior_refuse_overruns() {
    let flags = [true, false, true, true];
    assert_eq!(get_next(1, 3, &flags), Ok(1));
    assert_eq!(get_next(2, 3, &flags), Ok(1));
    assert_eq!(get_prior(1, 3, &flags), Ok(1));
    assert_eq!(get_prior(0, 3, &flags), Ok(1));
    assert!(matches!(
        get_next(0, 1, &[true, true]),
        Err(ClipperError::Undefined(_))
    ));
    assert!(matches!(
        get_prior(0, 1, &[true, true]),
        Err(ClipperError::Undefined(_))
    ));
}

#[test]
fn rdp_detects_endless_recursion_only_with_nan() {
    // Collinear points and a NaN epsilon: RDP(0, end) calls itself.
    let line: Vec<Point64> = (0..6).map(|i| p(i, 0)).collect();
    assert!(matches!(
        ramer_douglas_peucker(&line, f64::NAN, 64),
        Err(ClipperError::Undefined(_))
    ));
    // RDP(0, 1) with a NaN epsilon ends: it flags index 0 and makes no call.
    let mut flags = vec![false; 2];
    assert_eq!(rdp(&line, 0, 1, f64::NAN, &mut flags, 8), Ok(()));
    assert_eq!(flags, vec![true, false]);
    // A finite epsilon keeps the end points and the farthest vertex.
    let bump = vec![p(0, 0), p(1, 0), p(2, 5), p(3, 0), p(4, 0)];
    assert_eq!(
        ramer_douglas_peucker(&bump, 1.0, 64).map(|q| xy(&q)),
        Ok(vec![(0, 0, 0), (2, 5, 0), (4, 0, 0)])
    );
}

#[test]
fn trim_collinear_cases() {
    let square_with_extra = vec![p(5, 0), p(10, 0), p(10, 10), p(0, 10), p(0, 0)];
    assert_eq!(
        trim_collinear_64(&square_with_extra, false).map(|q| xy(&q)),
        Ok(vec![(10, 0, 0), (10, 10, 0), (0, 10, 0), (0, 0, 0)])
    );
    assert_eq!(
        trim_collinear_64(&vec![p(0, 0), p(5, 0), p(10, 0)], false),
        Ok(Vec::new())
    );
    assert_eq!(
        trim_collinear_64(&vec![p(0, 0), p(5, 0), p(10, 0)], true).map(|q| xy(&q)),
        Ok(vec![(0, 0, 0), (10, 0, 0)])
    );
    assert_eq!(
        trim_collinear_64(&vec![p(1, 1), p(1, 1)], true),
        Ok(Vec::new())
    );
}

#[test]
fn simplify_and_contains() {
    let noisy = vec![p(0, 0), p(10, 1), p(20, -1), p(30, 0), p(30, 30), p(0, 30)];
    let simple = simplify_path(&noisy, 2.0, true);
    assert_eq!(
        simple.map(|q| xy(&q)),
        Ok(vec![(0, 0, 0), (30, 0, 0), (30, 30, 0), (0, 30, 0)])
    );
    let outer = vec![p(0, 0), p(100, 0), p(100, 100), p(0, 100)];
    let inner = vec![p(10, 10), p(20, 10), p(20, 20)];
    assert_eq!(path2_contains_path1(&inner, &outer), Ok(true));
    assert_eq!(path2_contains_path1(&outer, &inner), Ok(false));
    assert_eq!(path2_contains_path1::<i64>(&[], &outer), Ok(false));
}

#[test]
fn make_path_forms() {
    assert_eq!(
        make_path(&[1i32, 2, 3, 4]).map(|q| xy(&q)),
        Ok(vec![(1, 2, 0), (3, 4, 0)])
    );
    assert!(matches!(
        make_path(&[1i64, 2, 3]),
        Err(ClipperError::Exception(_))
    ));
    // An unsigned value above INT64_MAX converts modulo 2^64 to int64_t, but exactly to double.
    assert_eq!(
        make_path(&[u64::MAX, 0]).map(|q| xy(&q)),
        Ok(vec![(-1, 0, 0)])
    );
    assert_eq!(
        make_path_d(&[u64::MAX, 0]).map(|q| q[0].x),
        Ok(18_446_744_073_709_551_615.0)
    );
    assert_eq!(
        make_path_z(&[1i64, 2, 3, 4, 5, 6]).map(|q| xy(&q)),
        Ok(vec![(1, 2, 3), (4, 5, 6)])
    );
    assert!(matches!(
        make_path_zd(&[1.0f64, 2.0, f64::NAN]),
        Err(ClipperError::Undefined(_))
    ));
    assert_eq!(make_path_zd(&[1.5f64, 2.5, -3.9]).map(|q| q[0].z), Ok(-3));
}

#[test]
fn translate_measure_and_minkowski() {
    assert_eq!(
        translate_path(
            &[Point {
                x: 1i64,
                y: 2,
                z: 9
            }],
            3,
            4
        )
        .map(|q| xy(&q)),
        Ok(vec![(4, 6, 0)])
    );
    assert!(matches!(
        translate_path(&[p(i64::MAX, 0)], 1, 0),
        Err(ClipperError::Undefined(_))
    ));
    assert_eq!(distance(&p(0, 0), &p(3, 4)), Ok(5.0));
    // cp^2 / (|p1p2|^2 |p2p3|^2) = 25 / 650 ~ 0.0385.
    assert_eq!(
        near_collinear(&p(0, 0), &p(5, 0), &p(10, 1), 0.04),
        Ok(true)
    );
    assert_eq!(length(&[p(0, 0), p(3, 4), p(3, 0)], true), Ok(12.0));
    assert_eq!(
        near_collinear(&p(0, 0), &p(5, 0), &p(10, 1), 0.01),
        Ok(false)
    );
    assert_eq!(
        minkowski(
            &vec![p(0, 0), p(1, 0)],
            &vec![p(0, 0), p(0, 1)],
            true,
            false,
            3
        ),
        Err(ClipperError::ResourceLimit)
    );
    let sum = minkowski_sum_64(
        &vec![p(0, 0), p(2, 0), p(2, 2), p(0, 2)],
        &vec![p(0, 0), p(10, 0)],
        false,
        64,
    );
    assert_eq!(sum.map(|q| q.len()), Ok(1));
}

#[test]
fn polytree_helpers() -> Result<()> {
    let budget = Budget {
        nodes: 64,
        points: 1024,
    };
    let mut tree = PolyTree64::new(budget)?;
    let outer = tree.add_child(
        PolyTree64::ROOT,
        &vec![p(0, 0), p(100, 0), p(100, 100), p(0, 100)],
    )?;
    let hole = tree.add_child(outer, &vec![p(10, 10), p(20, 10), p(20, 20), p(10, 20)])?;
    tree.add_child(hole, &vec![p(12, 12), p(18, 12), p(18, 18)])?;
    let paths = poly_tree_to_paths(&tree, PolyTree64::ROOT)?;
    assert_eq!(paths.len(), 3);
    assert_eq!(
        check_polytree_fully_contains_children(&tree, PolyTree64::ROOT),
        Ok(true)
    );
    tree.add_child(outer, &vec![p(200, 200), p(300, 200), p(300, 300)])?;
    assert_eq!(
        check_polytree_fully_contains_children(&tree, PolyTree64::ROOT),
        Ok(false)
    );
    let mut state = StreamState::default();
    let text = insert_polytree64(&tree, PolyTree64::ROOT, &mut state)?;
    assert_eq!(
        text,
        "\nPolytree with 1 polygon.\n  +- Polygon (0) contains 2 holes.\n    +- Hole (0) contains 1 nested polygon.\n\n\n"
    );
    Ok(())
}

#[test]
fn triangulation_and_version() {
    let mut solution = Paths64::new();
    assert_eq!(
        triangulate_64(&Paths64::new(), &mut solution, true),
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::Triangulate(const Paths64&, Paths64&, bool)",
            unit: TRIANGULATION_SRC,
        })
    );
    assert_eq!(CLIPPER2_VERSION, "2.0.1");
}
