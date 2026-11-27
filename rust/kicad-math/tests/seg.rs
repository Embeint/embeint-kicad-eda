// SPDX-License-Identifier: GPL-3.0-or-later
//! SEG behaviour checked against hand-worked geometry. The exact C++ comparison is the
//! `kicad-seg-shim` difftest; these pin the type co-port, the deferrals and the refusals.
use kicad_math::seg::{sgn, Seg, SegError, HEADER_UNIT};
use kicad_math::{Vector2, Vector2I};

fn p(x: i32, y: i32) -> Vector2I {
    Vector2::new(x, y)
}

#[test]
fn type_co_port_matches_the_header() {
    let s = Seg::with_index(p(1, 2), p(3, 4), 7);
    assert_eq!(s.index(), 7);
    assert_eq!(Seg::new(p(1, 2), p(3, 4)).index(), -1);
    assert_eq!(Seg::from_coords(1, 2, 3, 4).index(), -1);
    assert_eq!(Seg::default().index(), -1);
    // operator== compares the points only.
    assert_eq!(s, Seg::new(p(1, 2), p(3, 4)));
    assert_ne!(s, Seg::new(p(3, 4), p(1, 2)));
}

#[test]
fn hand_worked_results() -> Result<(), SegError> {
    let s = Seg::new(p(0, 0), p(10, 0));
    assert_eq!(s.squared_distance_point(p(5, 3))?, 9);
    assert_eq!(s.squared_distance_point(p(13, 4))?, 25);
    assert_eq!(s.nearest_point_point(p(4, 9))?, p(4, 0));
    assert_eq!(s.reflect_point(p(4, 9))?, p(4, -9));
    assert_eq!(s.line_project(p(-4, 9))?, p(-4, 0));
    assert_eq!(s.line_distance(p(3, -4), true)?, -4);
    assert_eq!(
        s.intersect(&Seg::new(p(5, -5), p(5, 5)), false, false)?,
        Some(p(5, 0))
    );
    assert_eq!(
        s.intersect(&Seg::new(p(10, 0), p(20, 0)), true, false)?,
        None
    );
    let mut actual = -1;
    assert!(!s.collide(&Seg::new(p(0, 5), p(10, 5)), 5, Some(&mut actual))?);
    assert_eq!(actual, 5);
    assert_eq!(sgn(-3_i64), -1);
    assert_eq!(sgn(0_i64), 0);
    Ok(())
}

#[test]
fn header_bodies_are_wired() -> Result<(), SegError> {
    // SEG::Square and SEG::SquaredLength are the seg.h bodies of the header unit.
    assert_eq!(
        HEADER_UNIT,
        "libs.kimath.include.geometry.application.ui.p003"
    );
    let s = Seg::new(p(0, 0), p(10, 0));
    assert!(s.approx_collinear(&Seg::new(p(20, 1), p(30, 1)), 1)?);
    assert!(!s.approx_collinear(&Seg::new(p(20, 2), p(30, 2)), 1)?);
    assert!(s.approx_parallel(&Seg::new(p(0, 50), p(10, 50)), 1)?);
    assert!(s.approx_perpendicular(&Seg::new(p(3, 0), p(3, 9)))?);
    // ApproxParallel's SquaredLength: A - B overflows int.
    let wide = Seg::new(p(i32::MIN, 0), p(i32::MAX, 0));
    assert_eq!(wide.approx_parallel(&s, 1), Err(SegError::Undefined));
    Ok(())
}

#[test]
fn undefined_evaluations_are_refused() {
    let wide = Seg::new(p(i32::MIN, 0), p(i32::MAX, 0));
    // B - A overflows int.
    assert_eq!(wide.perpendicular_seg(p(0, 0)), Err(SegError::Undefined));
    // static_cast<ecoord>( NaN * 1000 ).
    let mut out = p(1, 1);
    assert_eq!(
        Seg::new(p(0, 0), p(1, 1)).intersects_line(f64::NAN, 0.0, &mut out),
        Err(SegError::Undefined)
    );
    assert_eq!(out, p(1, 1));
}
