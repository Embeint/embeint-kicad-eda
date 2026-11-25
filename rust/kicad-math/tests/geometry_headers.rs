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
fn default_ellipse_is_zero_length_and_callee_owners_are_preserved() {
    let e = Ellipse::<i32>::default();
    assert_eq!((e.major_radius, e.minor_radius), (0, 0));
    assert_eq!(e.end_angle, ANGLE_0);
    assert!(matches!(
        e.arc_end_point(),
        Err(GeometryError::Unported {
            unit: "libs.kimath.src.geometry.ellipse.application.ui.p001",
            ..
        })
    ));
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
    assert!(matches!(
        ANGLE_90.is_parallel_to(-ANGLE_90),
        Err(GeometryError::Unported {
            unit: UTIL_HEADER,
            ..
        })
    ));
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
    assert!(matches!(
        index.contains(Vector2I::new(101, 50), Some(2)),
        Err(GeometryError::Unported {
            upstream: "SEG::Square",
            ..
        })
    ));
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
