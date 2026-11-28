// SPDX-License-Identifier: GPL-3.0-or-later
//! Continuations behind the host assertion effect, the step and curve limits, the compiled NaN
//! orders, and replies of the GCC 13.3 oracle for a few public cases.
use super::*;
use core::cell::RefCell;

/// A host whose assertion handler records the site and returns, as wx's default handler does
/// without an application.
#[derive(Default)]
struct Recording {
    sites: RefCell<Vec<WxAssert>>,
}

impl Host for Recording {
    fn wx_assert(&self, site: &WxAssert) -> BezierResult<()> {
        self.sites.borrow_mut().push(*site);
        Ok(())
    }
}

fn d(x: f64, y: f64) -> Vector2D {
    V::new(x, y)
}

fn i(x: i32, y: i32) -> Vector2I {
    V::new(x, y)
}

fn nan(payload: u64) -> f64 {
    f64::from_bits(0x7ff8_0000_0000_0000 | payload)
}

fn poly(points: &[(f64, f64)]) -> BezierPoly {
    let points: Vec<Vector2D> = points.iter().map(|&(x, y)| d(x, y)).collect();
    BezierPoly::from_vector2d(&points)
}

const UNPORTED_ASSERT: BezierError = BezierError::Unported {
    upstream: WX_ON_ASSERT,
    unit: UNIT,
};

#[test]
fn constructors_convert_exactly() {
    let p = BezierPoly::from_points(i(i32::MIN, 1), i(2, 3), i(4, 5), i(6, i32::MAX));
    assert_eq!(p.ctrl_pts()[0], d(-2_147_483_648.0, 1.0));
    assert_eq!(p.ctrl_pts()[3], d(6.0, 2_147_483_647.0));
    assert_eq!(p.min_seg_len().to_bits(), 0);
    let v = BezierPoly::from_vector2i(&[i(1, 2), i(-3, 4), i(5, -6)]);
    assert_eq!(v.ctrl_pts(), &[d(1.0, 2.0), d(-3.0, 4.0), d(5.0, -6.0)]);
    assert!(BezierPoly::from_vector2i(&[]).ctrl_pts().is_empty());
}

#[test]
fn production_host_defers_every_assertion_site() {
    let two = poly(&[(0.0, 0.0), (1.0, 1.0)]);
    assert_eq!(two.is_flat(1.0), Err(UNPORTED_ASSERT));
    assert_eq!(two.eval(0.5), Err(UNPORTED_ASSERT));
    let (mut l, mut r) = (zeros4(), zeros4());
    assert_eq!(two.subdivide(0.5, &mut l, &mut r), Err(UNPORTED_ASSERT));
    let mut out = Vec::new();
    assert_eq!(two.get_poly_d(&mut out, 1.0, 1 << 20), Err(UNPORTED_ASSERT));
    let mut out_i = vec![i(9, 9)];
    assert_eq!(
        BezierPoly::from_vector2i(&[i(0, 0)]).get_poly_i(&mut out_i, 1, 1 << 20),
        Err(UNPORTED_ASSERT)
    );
}

#[test]
fn assertion_continuations_follow_the_cpp() {
    let host = Recording::default();
    let five = poly(&[(0.0, 0.0); 5]);
    assert_eq!(five.is_flat_with(&host, 1.0), Ok(false));
    let value = five.eval_with(&host, 0.25);
    assert_eq!(value.map(|p| (p.x.to_bits(), p.y.to_bits())), Ok((0, 0)));
    let (mut l, mut r) = (poly(&[(7.0, 7.0)]), poly(&[]));
    assert_eq!(five.subdivide_with(&host, 0.5, &mut l, &mut r), Ok(()));
    assert_eq!(l.ctrl_pts(), &[d(7.0, 7.0)]);
    assert!(r.ctrl_pts().is_empty());
    let mut out = vec![d(1.0, 2.0)];
    assert_eq!(five.get_poly_d_with(&host, &mut out, 1.0, 1 << 20), Ok(()));
    assert_eq!(out, vec![d(1.0, 2.0)]);
    let mut out_i = vec![i(1, 2)];
    assert_eq!(five.get_poly_i_with(&host, &mut out_i, 1, 1 << 20), Ok(()));
    assert!(
        out_i.is_empty(),
        "GetPoly( VECTOR2I ) clears its output first"
    );
    assert_eq!(
        *host.sites.borrow(),
        vec![
            WX_ASSERT_IS_FLAT,
            WX_ASSERT_EVAL,
            WX_ASSERT_SUBDIVIDE,
            WX_ASSERT_GET_POLY,
            WX_ASSERT_GET_POLY
        ]
    );
}

#[test]
fn subscripts_outside_the_vectors_are_undefined() {
    let three = poly(&[(0.0, 0.0), (1.0, 2.0), (3.0, 0.0)]);
    assert_eq!(
        three.number_of_inflection_points(),
        Err(BezierError::Undefined)
    );
    let (mut a, mut b) = (0.5, 0.5);
    assert_eq!(
        three.find_inflection_points(&mut a, &mut b),
        Err(BezierError::Undefined)
    );
    assert_eq!((a, b), (0.5, 0.5), "refused before the outputs are written");
    assert_eq!(
        poly(&[(0.0, 0.0), (1.0, 1.0)]).third_control_point_deviation(),
        Err(BezierError::Undefined)
    );
    let (mut l, mut r) = (poly(&[(5.0, 5.0); 3]), poly(&[(6.0, 6.0); 2]));
    assert_eq!(
        three.subdivide(0.5, &mut l, &mut r),
        Err(BezierError::Undefined)
    );
    assert_eq!(l.ctrl_pts(), &[d(5.0, 5.0); 3], "nothing written");
}

#[test]
fn three_point_is_flat_refuses_a_nan_parameter() {
    // D31 == 0 and D21 . D31 == 0: t = 0 / 0.
    let p = poly(&[(1.0, 1.0), (4.0, 5.0), (1.0, 1.0)]);
    assert_eq!(p.is_flat(1.0), Err(BezierError::Undefined));
    // The projection clamps to [0, 1]: P1 beyond the chord's end is measured from P2.
    let q = poly(&[(0.0, 0.0), (20.0, 0.0), (10.0, 0.0)]);
    assert_eq!(q.is_flat(4.9), Ok(false));
    assert_eq!(q.is_flat(5.0), Ok(true));
}

#[test]
fn eval_keeps_the_compiled_nan_operands() {
    let (a, b, t) = (nan(0xa), nan(0xb), nan(0x7));
    // Three points: omt2 * P0 has P0 as the destination, so P0's NaN wins over t's.
    let q = poly(&[(a, 0.0), (1.0, 1.0), (2.0, 0.0)]);
    assert_eq!(q.eval(t).map(|p| p.x.to_bits()), Ok(a.to_bits()));
    // The coefficient is the destination of the P1 product: t's NaN wins over P1's.
    let q = poly(&[(0.0, 0.0), (b, 1.0), (2.0, 0.0)]);
    assert_eq!(q.eval(t).map(|p| p.x.to_bits()), Ok(t.to_bits()));
    // Four points: the P1 term is the destination of the first sum.
    let c = poly(&[(a, 0.0), (b, 0.0), (2.0, 0.0), (3.0, 0.0)]);
    assert_eq!(c.eval(0.5).map(|p| p.x.to_bits()), Ok(b.to_bits()));
    // P2 and P3 are the destinations of their products.
    let c = poly(&[(0.0, 0.0), (1.0, 0.0), (a, 0.0), (3.0, 0.0)]);
    assert_eq!(c.eval(t).map(|p| p.x.to_bits()), Ok(t.to_bits()));
    let c = poly(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (a, 0.0)]);
    assert_eq!(c.eval(0.5).map(|p| p.x.to_bits()), Ok(a.to_bits()));
}

#[test]
fn subdivide_keeps_the_compiled_nan_operands() {
    let (a, t) = (nan(0xa), nan(0x7));
    let c = poly(&[(0.0, 0.0), (a, 0.0), (2.0, 0.0), (3.0, 0.0)]);
    let (mut l, mut r) = (zeros4(), zeros4());
    assert_eq!(c.subdivide(t, &mut l, &mut r), Ok(()));
    // left_ctrl1 = ( P1 - P0 ) * t + P0: the difference (P1's NaN) is the destination.
    assert_eq!(l.ctrl_pts()[1].x.to_bits(), a.to_bits());
    // shared = t * ( rc1 - lc2 ) + lc2: t is the destination.
    assert_eq!(l.ctrl_pts()[3].x.to_bits(), t.to_bits());
    assert_eq!(r.ctrl_pts()[0].x.to_bits(), t.to_bits());
    assert_eq!(r.ctrl_pts()[3], d(3.0, 0.0));
}

#[test]
fn third_control_point_deviation_keeps_the_compiled_nan_operands() {
    let (a, b) = (nan(0xa), nan(0xb));
    // len_sq = dy * dy + dx * dx with dy the destination, so len carries b; s = ( p0.x - p1.x )
    // / len carries a, and s * p2.y is the destination of the first sum: a wins.
    let p = poly(&[(0.0, 0.0), (a, b), (1.0, 1.0)]);
    assert_eq!(
        p.third_control_point_deviation().map(f64::to_bits),
        Ok(a.to_bits())
    );
    // With only dx a NaN, len and r carry it.
    let p = poly(&[(0.0, 0.0), (a, 1.0), (1.0, 1.0)]);
    assert_eq!(
        p.third_control_point_deviation().map(f64::to_bits),
        Ok(a.to_bits())
    );
    // The first sum is s * p2.y + r * p2.x: the p2.y product wins; abs clears the sign.
    let p = poly(&[
        (0.0, 0.0),
        (3.0, 4.0),
        (f64::from_bits(0xfff8_0000_0000_00aa), b),
    ]);
    assert_eq!(
        p.third_control_point_deviation().map(f64::to_bits),
        Ok(b.to_bits())
    );
    assert_eq!(
        poly(&[(0.0, 0.0), (0.0, 0.0009), (5.0, 5.0)])
            .third_control_point_deviation()
            .map(f64::to_bits),
        Ok(0)
    );
}

#[test]
fn a_nan_control_point_reaches_the_step_limit() {
    // cubicParabolicApprox subdivides a curve whose control point is infinite; the NaN half
    // never becomes flat, so recursiveSegmentation would grow its stack without end.
    let p = poly(&[(0.0, 0.0), (f64::INFINITY, 1.0), (2.0, 0.0), (3.0, 1.0)]);
    let mut out = Vec::new();
    assert_eq!(
        p.get_poly_d(&mut out, 1.0, 1 << 20),
        Err(BezierError::LimitExceeded)
    );
    // A NaN control point stops cubicParabolicApprox at once: only the start point.
    let p = poly(&[(0.0, 0.0), (nan(1), 1.0), (2.0, 0.0), (3.0, 1.0)]);
    let mut out = Vec::new();
    assert_eq!(p.get_poly_d(&mut out, 1.0, 1 << 20), Ok(()));
    assert_eq!(out, vec![d(0.0, 0.0)]);
}

#[test]
fn the_step_limit_is_exact() {
    let p = BezierPoly::from_points(i(0, 0), i(1000, 2000), i(3000, 2000), i(4000, 0));
    let mut used = 0;
    for limit in 0..10_000u64 {
        let mut out = Vec::new();
        if p.get_poly_d(&mut out, 10.0, limit).is_ok() {
            used = limit;
            break;
        }
    }
    assert!(used > 0);
    let mut out = Vec::new();
    assert_eq!(
        p.get_poly_d(&mut out, 10.0, used - 1),
        Err(BezierError::LimitExceeded)
    );
}

#[test]
fn quad_point_count_is_undefined_for_a_nan() {
    let p = poly(&[(0.0, 0.0), (nan(3), 0.0), (1.0, 0.0)]);
    let mut out = Vec::new();
    assert_eq!(
        p.get_poly_d(&mut out, 1.0, 1 << 20),
        Err(BezierError::Undefined)
    );
    // Collinear: cross == 0 makes x0 and x2 infinite or NaN.
    let p = BezierPoly::from_points(i(0, 0), i(1, 1), i(2, 2), i(0, 0));
    let three = BezierPoly::from_vector2d(&p.ctrl_pts()[..3]);
    assert_eq!(
        three.get_poly_d(&mut out, 1.0, 1 << 20),
        Err(BezierError::Undefined)
    );
}

/// Replies of the local GCC 13.3 oracle (`port/oracle/shims/kimath_src_bezier_curves.cpp`).
#[test]
fn oracle_replies() {
    let p = BezierPoly::from_points(i(0, 0), i(1000, 2000), i(3000, 2000), i(4000, 0));
    let mut out = vec![i(5, 5)];
    assert_eq!(p.get_poly_i(&mut out, 10, 1 << 20), Ok(()));
    let expected = [
        (0, 0),
        (135, 248),
        (280, 473),
        (433, 674),
        (594, 854),
        (760, 1009),
        (932, 1144),
        (1108, 1255),
        (1287, 1346),
        (1468, 1415),
        (1651, 1464),
        (1834, 1492),
        (2019, 1500),
        (2385, 1456),
        (2745, 1331),
        (3095, 1124),
        (3428, 832),
        (3736, 450),
        (4000, 0),
    ];
    assert_eq!(out, expected.map(|(x, y)| i(x, y)).to_vec());

    let q = BezierPoly::from_vector2i(&[i(0, 0), i(1000, 2000), i(3000, 0)]);
    let mut out = Vec::new();
    assert_eq!(q.get_poly_i(&mut out, DEFAULT_MAX_ERROR_I, 1 << 20), Ok(()));
    let expected = [
        (0, 0),
        (0, 0),
        (224, 381),
        (453, 653),
        (687, 838),
        (929, 950),
        (1185, 998),
        (1463, 981),
        (1774, 890),
        (2127, 712),
        (2533, 424),
        (3000, 0),
    ];
    assert_eq!(out, expected.map(|(x, y)| i(x, y)).to_vec());

    let ellipse = Ellipse {
        center: i(100, 200),
        major_radius: 1000,
        minor_radius: 500,
        rotation: EdaAngle::new(0.0, AngleUnit::Degrees),
        start_angle: EdaAngle::new(0.0, AngleUnit::Degrees),
        end_angle: EdaAngle::new(90.0, AngleUnit::Degrees),
    };
    let mut beziers = Vec::new();
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, 4096),
        Ok(())
    );
    assert_eq!(beziers.len(), 1);
    let b = beziers[0];
    assert_eq!(
        [b.start, b.c1, b.c2, b.end],
        [i(1100, 200), i(1100, -76), i(652, -300), i(100, -300)]
    );
}

#[test]
fn ellipse_curve_count_and_limits() {
    let mut ellipse = Ellipse {
        center: d(0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 1.0,
        rotation: EdaAngle::new(0.0, AngleUnit::Degrees),
        start_angle: EdaAngle::new(0.0, AngleUnit::Degrees),
        end_angle: EdaAngle::new(360.0, AngleUnit::Degrees),
    };
    let mut beziers = vec![Bezier::default()];
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, 4096),
        Ok(())
    );
    assert_eq!(beziers.len(), 5, "appends four curves to the existing one");
    // A NaN angle makes the count undefined; a huge sweep exceeds the curve limit or int.
    ellipse.end_angle = EdaAngle::new(f64::NAN, AngleUnit::Degrees);
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, 4096),
        Err(BezierError::Undefined)
    );
    ellipse.end_angle = EdaAngle::new(-360.0 * 1026.0, AngleUnit::Degrees);
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, 4096),
        Err(BezierError::LimitExceeded)
    );
    ellipse.end_angle = EdaAngle::new(1e300, AngleUnit::Degrees);
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, u64::MAX),
        Err(BezierError::Undefined)
    );
    // `aPoint += Center` has Center as the destination: its NaN wins over the radius's.
    let ellipse = Ellipse {
        center: d(nan(0xc), 0.0),
        major_radius: nan(0xd),
        minor_radius: 1.0,
        rotation: EdaAngle::new(0.0, AngleUnit::Degrees),
        start_angle: EdaAngle::new(0.0, AngleUnit::Degrees),
        end_angle: EdaAngle::new(90.0, AngleUnit::Degrees),
    };
    let mut beziers = Vec::new();
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, 4096),
        Ok(())
    );
    assert_eq!(beziers[0].start.x.to_bits(), nan(0xc).to_bits());
    // An int ellipse never reaches a NaN point: a NaN rotation folds to zero in RotatePoint's
    // Normalize, and the casting constructor clamps a sum beyond int.
    let ellipse = Ellipse {
        center: i(i32::MAX, i32::MIN),
        major_radius: i32::MAX,
        minor_radius: 1,
        rotation: EdaAngle::new(f64::NAN, AngleUnit::Degrees),
        start_angle: EdaAngle::new(0.0, AngleUnit::Degrees),
        end_angle: EdaAngle::new(90.0, AngleUnit::Degrees),
    };
    let mut beziers = Vec::new();
    assert_eq!(
        transform_ellipse_to_beziers(&ellipse, &mut beziers, 4096),
        Ok(())
    );
    assert_eq!(beziers[0].start, i(i32::MAX, i32::MIN));
}
