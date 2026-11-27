// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
use kicad_math::eda_angle::*;
use kicad_math::geometry_headers::*;
use kicad_math::poly_ystripes::*;
use kicad_math::Vector2I;

#[test]
fn normalization_preserves_signed_zero_but_orientation_canonicalizes_it() {
    let mut a = EdaAngle::degrees(-0.0);
    assert_eq!(a.normalize().as_degrees().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(EdaOrientation::new(a).get_angle().as_degrees().to_bits(), 0);
    for (input, positive, negative, n90, n180, n720) in [
        (-720., 0., 0., 0., 0., -360.),
        (-540., 180., -180., 0., 180., -180.),
        (-360., 0., -0., 0., 0., -360.),
        (-180., 180., -180., 0., 180., -180.),
        (-90., 270., -90., -90., -90., -90.),
        (90., 90., -270., 90., 90., 90.),
        (180., 180., -180., 0., 180., 180.),
        (360., 0., 0., 0., 0., 0.),
        (720., 0., 0., 0., 0., 0.),
    ] {
        let angle = EdaAngle::degrees(input);
        assert_eq!(angle.normalized().as_degrees(), positive);
        let mut a = angle;
        assert_eq!(a.normalize_negative().as_degrees(), negative);
        let mut a = angle;
        assert_eq!(a.normalize90().as_degrees(), n90);
        let mut a = angle;
        assert_eq!(a.normalize180().as_degrees(), n180);
        let mut a = angle;
        assert_eq!(a.normalize720().as_degrees(), n720);
    }
    for value in [
        f64::MAX,
        -f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ] {
        let a = EdaAngle::degrees(value).normalized().as_degrees();
        assert!((0.0..360.0).contains(&a));
    }
}
#[test]
fn default_ellipse_endpoints_are_its_center_and_callee_owners_are_preserved() {
    let e = Ellipse::<i32>::default();
    assert_eq!((e.major_radius, e.minor_radius), (0, 0));
    assert_eq!(e.end_angle, ANGLE_0);
    // GetPointAtAngle (libs.kimath.src.geometry.ellipse.application.ui.p001) is wired.
    assert_eq!(e.arc_end_point(), Ok(Vector2I::new(0, 0)));
    assert_eq!(e.arc_start_point(), Ok(Vector2I::new(0, 0)));
    let a = GridGeometry {
        priority: 1,
        ..GridGeometry::default()
    };
    assert_eq!(a.takes_precedence_over(&GridGeometry::default()), Ok(true));
    assert!(matches!(
        a.takes_precedence_over(&a),
        Err(GeometryError::Unported {
            unit: "libs.kimath.src.geometry.grid_geometry.application.ui.p001",
            ..
        })
    ));
    assert_eq!(
        EdaAngle::degrees(1.).snapped(EdaAngle::degrees(-1.)),
        Ok(EdaAngle::degrees(1.))
    );
    assert_eq!(ANGLE_90.is_parallel_to(-ANGLE_90), Ok(true));
}
#[test]
fn stripes_preserve_holes_union_and_rebuild_early_returns() -> GeometryResult<()> {
    let square = vec![
        Vector2I::new(0, 0),
        Vector2I::new(100, 0),
        Vector2I::new(100, 100),
        Vector2I::new(0, 100),
    ];
    let holes = vec![vec![
        Vector2I::new(25, 25),
        Vector2I::new(75, 25),
        Vector2I::new(75, 75),
        Vector2I::new(25, 75),
    ]];
    let view = PolygonContours {
        outline: &square,
        holes: &holes,
    };
    let mut index = PolyYstripesIndex::new();
    index.build(&[view])?;
    assert_eq!(index.contains(Vector2I::new(10, 10), None), Ok(true));
    assert_eq!(index.contains(Vector2I::new(50, 50), None), Ok(false));
    assert_eq!(index.contains(Vector2I::new(10, 10), Some(10)), Ok(true));
    // The accuracy fallback (SEG::Square, then each non-hole edge's squared distance).
    assert_eq!(index.contains(Vector2I::new(101, 50), Some(2)), Ok(true));
    assert_eq!(index.contains(Vector2I::new(102, 50), Some(2)), Ok(true));
    assert_eq!(index.contains(Vector2I::new(103, 50), Some(2)), Ok(false));
    // Inside the hole: its edges (25 away) are skipped, the outline is 50 away.
    assert_eq!(index.contains(Vector2I::new(50, 50), Some(30)), Ok(false));
    assert_eq!(index.contains(Vector2I::new(50, 50), Some(50)), Ok(true));
    assert_eq!(
        index.contains(Vector2I::new(50, 50), Some(i32::MAX)),
        Ok(true)
    );
    let polys: Vec<_> = (0..9)
        .map(|_| PolygonContours {
            outline: &square,
            holes: &[],
        })
        .collect();
    index.build(&polys)?;
    assert_eq!(index.contains(Vector2I::new(50, 50), None), Ok(true));
    index.build(&[])?;
    assert_eq!(index.contains(Vector2I::new(50, 50), Some(100)), Ok(false));
    index.build(&[PolygonContours {
        outline: &square[..2],
        holes: &[],
    }])?;
    assert_eq!(index.contains(Vector2I::new(50, 50), None), Ok(false));
    Ok(())
}
#[test]
fn stripes_refuse_the_actual_signed_difference_overflow() -> GeometryResult<()> {
    let points = [
        Vector2I::new(i32::MIN, 0),
        Vector2I::new(i32::MAX, 100),
        Vector2I::new(0, 0),
    ];
    let mut index = PolyYstripesIndex::new();
    index.build(&[PolygonContours {
        outline: &points,
        holes: &[],
    }])?;
    assert_eq!(
        index.contains(Vector2I::new(0, 50), None),
        Err(GeometryError::IntegerOverflow)
    );
    Ok(())
}

#[test]
fn named_assignments_return_the_mutated_destination() {
    let mut a = ANGLE_45;
    let original = core::ptr::from_ref(&a);
    assert!(core::ptr::eq(
        original,
        core::ptr::from_mut(a.add_assign(ANGLE_90))
    ));
    assert_eq!(a, ANGLE_135);
    assert!(core::ptr::eq(
        original,
        core::ptr::from_mut(a.sub_assign(ANGLE_45))
    ));
    assert_eq!(a, ANGLE_90);
    let mut o = EdaOrientation::default();
    let original = core::ptr::from_ref(&o);
    assert!(core::ptr::eq(
        original,
        core::ptr::from_mut(o.assign_angle(EdaAngle::degrees(-90.)))
    ));
    assert_eq!(o.get_angle(), ANGLE_270);
    assert!(core::ptr::eq(
        original,
        core::ptr::from_mut(o.add_assign(ANGLE_180))
    ));
    assert_eq!(o.get_angle(), ANGLE_90);
    assert!(core::ptr::eq(
        original,
        core::ptr::from_mut(o.sub_assign(ANGLE_180))
    ));
    assert_eq!(o.get_angle(), ANGLE_270);
}
