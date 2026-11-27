// SPDX-License-Identifier: GPL-3.0-or-later
//! In-crate tests of the seg.h and shape header port
//! (`libs.kimath.include.geometry.application.ui.p003`). The oracle corpus is the reference
//! evidence; these tests pin the branches it cannot reach (memberwise arc and ellipse state, a
//! shape whose `Clone` is the base `nullptr`) and the typed deferrals.
use std::rc::Rc;

use crate::box2::Box2I;
use crate::circle::Circle;
use crate::eda_angle::EdaAngle;
use crate::geometry_headers::Ellipse;
use crate::seg::{Seg, SegError};
use crate::stream::{StreamError, StreamInsert, StreamState};

use crate::shape::{
    shape_type_as_string, Shape, ShapeError, ShapeLineChainBase, ShapeResult, ShapeType,
    ShapeTypeValue, MIN_PRECISION_IU, SHAPE_COLLISIONS_UNIT,
};
use crate::shape_arc::{ShapeArc, ARC_HIGH_DEF, SHAPE_ARC_UNIT};
use crate::shape_circle::{ShapeCircle, TRIGO_HEADER_UNIT};
use crate::shape_compound::{ShapeCompound, SHAPE_COMPOUND_UNIT};
use crate::shape_ellipse::{ShapeEllipse, SHAPE_ELLIPSE_UNIT};
use crate::vector::{Vector2, Vector2I};

fn v(x: i32, y: i32) -> Vector2I {
    Vector2::new(x, y)
}

/// A box as (position, size).
fn parts(b: Box2I) -> (Vector2I, Vector2<i64>) {
    (b.position(), b.size())
}

fn deferred(upstream: &'static str, unit: &'static str) -> ShapeError {
    ShapeError::Unported { upstream, unit }
}

#[test]
fn seg_square_side_and_lengths() -> Result<(), SegError> {
    assert_eq!(Seg::square(i32::MIN), 1_i64 << 62);
    assert_eq!(Seg::square(-3), 9);
    let s = Seg::new(v(0, 0), v(10, 0));
    assert_eq!(s.side(v(5, 5))?, 1);
    assert_eq!(s.side(v(5, -5))?, -1);
    assert_eq!(s.side(v(20, 0))?, 0);
    assert_eq!(s.length()?, 10);
    assert_eq!(Seg::new(v(0, 0), v(3, 4)).squared_length()?, 25);
    assert_eq!(s.tcoef(v(3, 9))?, 30);
    // INT_MIN - INT_MAX overflows int.
    let wide = Seg::new(v(i32::MIN, 0), v(i32::MAX, 0));
    assert_eq!(wide.length(), Err(SegError::Undefined));
    assert_eq!(wide.squared_length(), Err(SegError::Undefined));
    assert_eq!(wide.side(v(0, 0)), Err(SegError::Undefined));
    // Large but defined: the squared norm saturates at ECOORD_MAX, the norm at INT_MAX.
    let big = Seg::new(
        v(-1_073_741_824, -1_073_741_824),
        v(1_073_741_823, 1_073_741_823),
    );
    assert_eq!(big.length()?, i32::MAX);
    Ok(())
}

#[test]
fn seg_canonical_collinear_and_containment() -> Result<(), SegError> {
    let s = Seg::new(v(0, 0), v(10, 10));
    assert_eq!(s.canonical_coefs()?, (-10, 10, 0));
    assert!(s.collinear(&Seg::new(v(20, 20), v(-5, -5)))?);
    assert!(!s.collinear(&Seg::new(v(20, 22), v(-5, -5)))?);
    assert!(s.contains_seg(&Seg::new(v(2, 2), v(8, 8)))?);
    assert!(!s.contains_seg(&Seg::new(v(2, 2), v(18, 18)))?);
    assert!(s.contains_seg(&Seg::new(v(5, 5), v(5, 5)))?);
    assert!(s.overlaps(&Seg::new(v(8, 8), v(18, 18)))?);
    // (11, 11) is within the default squared threshold of 3; (12, 12) is not.
    assert!(s.overlaps(&Seg::new(v(11, 11), v(18, 18)))?);
    assert!(!s.overlaps(&Seg::new(v(12, 12), v(18, 18)))?);
    // A point at an end is not an overlap.
    assert!(!s.overlaps(&Seg::new(v(10, 10), v(10, 10)))?);
    assert!(s.approx_overlaps(&Seg::new(v(8, 9), v(18, 19)), 3)?);
    // qC = A.x * B.y - A.y * B.x in value, so the extremes stay defined; the line values of
    // Collinear can overflow int64_t.
    let far = Seg::new(v(i32::MIN, i32::MIN), v(i32::MAX, i32::MAX));
    assert_eq!(far.canonical_coefs()?, (-4_294_967_295, 4_294_967_295, 0));
    assert_eq!(
        far.collinear(&Seg::new(v(i32::MAX, i32::MIN), v(0, 0))),
        Err(SegError::Undefined)
    );
    Ok(())
}

#[test]
fn seg_order_reverse_center_and_stream() -> Result<(), SegError> {
    let mut s = Seg::with_index(v(1, 2), v(-3, 4), 7);
    s.reverse();
    assert_eq!((s.a, s.b, s.index()), (v(-3, 4), v(1, 2), 7));
    let r = s.reversed();
    assert_eq!((r.a, r.b, r.index()), (v(1, 2), v(-3, 4), -1));
    // KiROUND( x / 2.0 ) rounds halves away from zero.
    assert_eq!(Seg::new(v(0, 0), v(1, -1)).center()?, v(1, -1));
    assert_eq!(Seg::new(v(-5, 5), v(0, 0)).center()?, v(-2, 2));
    assert!(Seg::new(v(0, 0), v(1, 0)).less(&Seg::new(v(0, 0), v(1, 1))));
    assert!(!Seg::new(v(0, 1), v(0, 0)).less(&Seg::new(v(0, 0), v(9, 9))));
    assert_eq!(
        Seg::new(v(0, 0), v(10, 0)).intersect_lines(&Seg::new(v(5, -5), v(5, -1)))?,
        Some(v(5, 0))
    );
    Ok(())
}

#[test]
fn seg_stream_uses_the_caller_state() -> Result<(), StreamError> {
    let s = Seg::with_index(v(-3, 4), v(1, 2), 7);
    let mut state = StreamState::default();
    assert_eq!(s.insert(&mut state)?, "[ [ -3 | 4 ] - [ 1 | 2 ] ]");
    assert_eq!(state, StreamState::default());
    // Flags reach every coordinate; only the first insertion is padded, and it resets the width.
    let mut state = StreamState {
        width: 5,
        fill: '_',
        ..StreamState::default()
    };
    state.flags.dec = false;
    state.flags.hex = true;
    state.flags.showbase = true;
    state.flags.uppercase = true;
    assert_eq!(
        Seg::new(v(10, 11), v(12, 13)).insert(&mut state)?,
        "___[ [ 0XA | 0XB ] - [ 0XC | 0XD ] ]"
    );
    assert_eq!(state.width, 0);
    // Left adjustment pads after "[ "; showpos marks non-negative decimal values.
    let mut state = StreamState {
        width: 4,
        ..StreamState::default()
    };
    state.flags.left = true;
    state.flags.showpos = true;
    assert_eq!(
        Seg::new(v(0, -1), v(i32::MIN, i32::MAX)).insert(&mut state)?,
        "[   [ +0 | -1 ] - [ -2147483648 | +2147483647 ] ]"
    );
    // Octal and hexadecimal insert the bits as unsigned int.
    let mut state = StreamState::default();
    state.flags.dec = false;
    state.flags.oct = true;
    state.flags.showbase = true;
    assert_eq!(
        Seg::new(v(-1, 0), v(8, 9)).insert(&mut state)?,
        "[ [ 037777777777 | 0 ] - [ 010 | 011 ] ]"
    );
    Ok(())
}

#[test]
fn shape_type_names_and_values() {
    let names: Vec<&str> = ShapeType::ALL
        .iter()
        .map(|t| shape_type_as_string(ShapeTypeValue::of(*t)))
        .collect();
    assert_eq!(
        names,
        [
            "SH_RECT",
            "SH_SEGMENT",
            "SH_LINE_CHAIN",
            "SH_CIRCLE",
            "SH_SIMPLE",
            "SH_POLY_SET",
            "SH_COMPOUND",
            "SH_ARC",
            "SH_NULL",
            "SH_POLY_SET_TRIANGLE",
            "SH_ELLIPSE"
        ]
    );
    assert_eq!(ShapeType::Ellipse.value(), 10);
    // 11 to 15 are unnamed values of the enumeration's range; the switch falls through.
    for value in 0..=15 {
        let t = ShapeTypeValue::new(value);
        assert_eq!(t.map(ShapeTypeValue::value), Some(value));
        assert_eq!(t.and_then(ShapeTypeValue::named).is_some(), value <= 10);
        if value > 10 {
            assert_eq!(t.map(shape_type_as_string), Some(""));
        }
    }
    assert_eq!(ShapeTypeValue::new(-1), None);
    assert_eq!(ShapeTypeValue::new(16), None);
    assert_eq!(MIN_PRECISION_IU, 4);
    assert_eq!(ARC_HIGH_DEF, 5000);
}

#[test]
fn shape_circle_members() -> ShapeResult<()> {
    let mut c = ShapeCircle::with_center_radius(v(10, -20), 5);
    assert_eq!(c.type_name(), "SH_CIRCLE");
    assert!(!c.is_null() && c.is_solid()? && !c.has_indexable_subshapes());
    assert_eq!(
        (c.get_start(), c.get_end(), c.get_width()),
        (v(0, 0), v(0, 0), 0)
    );
    c.set_width(9);
    assert_eq!(c.get_width(), 0);
    assert_eq!(parts(c.bbox(0)?), (v(5, -25), Vector2::new(10, 10)));
    assert_eq!(c.centre()?, v(10, -20));
    // A negative half-size is normalised by the BOX2 constructor.
    assert_eq!(parts(c.bbox(-7)?), (v(8, -22), Vector2::new(4, 4)));
    c.move_by(v(1, 1))?;
    c.set_radius(3);
    assert_eq!(c.get_circle(), Circle::with_center_radius(v(11, -19), 3));
    let copy = c.clone_shape()?;
    let copy = copy
        .as_ref()
        .and_then(|s| s.as_any().downcast_ref::<ShapeCircle>());
    assert_eq!(copy, Some(&c));
    let mut d = ShapeCircle::new();
    d.assign_from(&c);
    assert_eq!(d, c);
    assert_eq!(
        c.rotate(EdaAngle::degrees(90.0), v(0, 0)),
        Err(deferred(
            "RotatePoint(VECTOR2I&, const VECTOR2I&, EDA_ANGLE)",
            TRIGO_HEADER_UNIT
        ))
    );
    // Center - rc overflows int.
    let edge = ShapeCircle::with_center_radius(v(i32::MIN, 0), 1);
    assert_eq!(edge.bbox(0).err(), Some(ShapeError::Undefined));
    let mut m = ShapeCircle::with_center_radius(v(i32::MAX, 0), 1);
    assert_eq!(m.move_by(v(1, 0)), Err(ShapeError::Undefined));
    Ok(())
}

#[test]
fn shape_circle_collide() -> ShapeResult<()> {
    let c = ShapeCircle::with_center_radius(v(0, 0), 10);
    let (mut actual, mut location) = (-1, v(7, 7));
    // Crossing the centre: dist_sq == 0, the location is the first intersection.
    let through = Seg::new(v(-100, 0), v(100, 0));
    assert!(c.collide_seg(&through, 0, Some(&mut actual), Some(&mut location))?);
    assert_eq!((actual, location), (0, v(10, 0)));
    // Within the clearance: the location is the nearest point.
    let near = Seg::new(v(-100, 12), v(100, 12));
    assert!(c.collide_seg(&near, 5, Some(&mut actual), Some(&mut location))?);
    assert_eq!((actual, location), (2, v(0, 12)));
    // Clear: the out-parameters are untouched.
    assert!(!c.collide_seg(&near, 0, Some(&mut actual), Some(&mut location))?);
    assert_eq!((actual, location), (2, v(0, 12)));
    // The point overload goes through SEG( aP, aP ).
    assert!(c.collide_point(v(3, 4), 0, None, None)?);
    // aClearance + Radius overflows int.
    assert_eq!(
        c.collide_seg(&near, i32::MAX, None, None),
        Err(ShapeError::Undefined)
    );
    // minDist = INT_MIN admits dist_sq = 2^62 - 44, whose double rounds to 2^62: the root is
    // exactly 2^31 and (int) of it is undefined. Without aActual the collision is defined.
    let point = ShapeCircle::with_center_radius(v(0, 0), 0);
    let far = Seg::new(v(2_147_483_524, 729_778), v(2_147_483_524, 729_778));
    assert!(point.collide_seg(&far, i32::MIN, None, None)?);
    assert_eq!(
        point.collide_seg(&far, i32::MIN, Some(&mut actual), None),
        Err(ShapeError::Undefined)
    );
    Ok(())
}

#[test]
fn shape_arc_inline_members() -> ShapeResult<()> {
    let mut a = ShapeArc::new();
    assert_eq!(
        (a.get_p0(), a.get_p1(), a.get_arc_mid(), a.get_width()),
        (v(0, 0), v(0, 0), v(0, 0), 0)
    );
    assert!(!a.is_ccw()? && a.is_clockwise()? && a.is_solid()?);
    a.set_width(12);
    assert_eq!(a.get_width(), 12);
    assert_ne!(a, ShapeArc::new());
    assert_eq!(ShapeArc::default_accuracy_for_pcb(), 5000);
    let ccw = ShapeArc::with_points(v(10, 0), v(0, 10), v(-10, 0), 1);
    assert!(ccw.is_ccw()?);
    let cw = ShapeArc::with_points(v(-10, 0), v(0, 10), v(10, 0), 1);
    assert!(cw.is_clockwise()?);
    assert_eq!(ccw.get_chord(), Seg::new(v(10, 0), v(-10, 0)));
    assert_eq!((ccw.get_start(), ccw.get_end()), (v(10, 0), v(-10, 0)));
    // m_end - mid spans 2^32 - 1: the int64_t products of Cross overflow.
    let huge = ShapeArc::with_points(
        v(i32::MIN, i32::MAX),
        v(i32::MAX, i32::MIN),
        v(i32::MIN, i32::MIN),
        0,
    );
    assert_eq!(huge.is_ccw(), Err(ShapeError::Undefined));
    let arc_unit = |s| deferred(s, SHAPE_ARC_UNIT);
    assert_eq!(
        a.clone_shape().err(),
        Some(arc_unit("SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)"))
    );
    assert_eq!(a.bbox(0).err(), Some(arc_unit("SHAPE_ARC::BBox")));
    assert_eq!(
        a.collide_shape(&ShapeCircle::new(), 0, None, None),
        Err(deferred(
            "SHAPE::Collide(const SHAPE*, int, int*, VECTOR2I*)",
            SHAPE_COLLISIONS_UNIT
        ))
    );
    assert_eq!(
        ShapeArc::from_three_points(v(0, 0), v(1, 1), v(2, 0), 0).err(),
        Some(arc_unit(
            "SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const VECTOR2I&, int)"
        ))
    );
    Ok(())
}

/// A shape that keeps every `SHAPE` default, so its `Clone` is the base `nullptr`.
#[derive(Debug)]
struct BaseOnly;

impl Shape for BaseOnly {
    fn shape_type(&self) -> ShapeType {
        ShapeType::Null
    }
    fn collide_seg(
        &self,
        _seg: &Seg,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        Ok(false)
    }
    fn bbox(&self, _clearance: i32) -> ShapeResult<Box2I> {
        Ok(Box2I::new(v(2, 4), Vector2::new(6, 8))?)
    }
    fn rotate(&mut self, _angle: EdaAngle, _center: Vector2I) -> ShapeResult<()> {
        Ok(())
    }
    fn move_by(&mut self, _vector: Vector2I) -> ShapeResult<()> {
        Ok(())
    }
    fn is_solid(&self) -> ShapeResult<bool> {
        Ok(false)
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl ShapeLineChainBase for BaseOnly {
    fn get_point(&self, _index: i32) -> ShapeResult<Vector2I> {
        Ok(v(0, 0))
    }
    fn get_segment(&self, _index: i32) -> ShapeResult<Seg> {
        Ok(Seg::default())
    }
    fn get_point_count(&self) -> usize {
        0
    }
    fn get_segment_count(&self) -> usize {
        0
    }
    fn is_closed(&self) -> bool {
        false
    }
}

#[test]
fn shape_base_defaults() -> ShapeResult<()> {
    let b = BaseOnly;
    assert!(b.is_null());
    assert_eq!(b.type_name(), "SH_NULL");
    assert!(b.clone_shape()?.is_none());
    // SHAPE::Centre is BBox( 0 ).Centre().
    assert_eq!(b.centre()?, v(5, 8));
    let mut out: Vec<Option<&dyn Shape>> = vec![None];
    b.get_indexable_subshapes(&mut out);
    assert_eq!(out.len(), 1);
    assert!(b.get_cached_bbox().is_none());
    let mut buffer = ();
    b.transform_to_polygon(&mut buffer, 5, 0_i32);
    Ok(())
}

fn circle_of(shape: Option<&dyn Shape>) -> Option<ShapeCircle> {
    shape.and_then(|s| s.as_any().downcast_ref::<ShapeCircle>().copied())
}

#[test]
fn shape_compound_add_and_flatten() -> ShapeResult<()> {
    let mut c = ShapeCompound::new();
    assert!(c.empty() && c.is_dirty() && c.has_indexable_subshapes());
    assert!(c.unique_subshape().is_none());
    let a = ShapeCircle::with_center_radius(v(1, 1), 1);
    let b = ShapeCircle::with_center_radius(v(2, 2), 2);
    c.add_shape(Some(Box::new(a)))?;
    assert_eq!(circle_of(c.unique_subshape()), Some(a));
    c.add_shape_shared(Some(Rc::new(b)))?;
    assert_eq!((c.size(), c.get_indexable_subshape_count()), (2, 2));
    assert!(c.unique_subshape().is_none());

    // A nested compound is flattened into clones of its members.
    let mut outer = ShapeCompound::new();
    outer.add_shape(Some(Box::new(ShapeCircle::with_center_radius(v(9, 9), 9))))?;
    outer.add_shape(Some(Box::new(c)))?;
    let circles: Vec<Option<ShapeCircle>> = outer
        .shapes()
        .iter()
        .map(|s| circle_of(s.as_deref()))
        .collect();
    assert_eq!(
        circles,
        [
            Some(ShapeCircle::with_center_radius(v(9, 9), 9)),
            Some(a),
            Some(b)
        ]
    );
    let mut subshapes: Vec<Option<&dyn Shape>> = vec![None, None, None, None, None];
    outer.get_indexable_subshapes(&mut subshapes);
    assert_eq!(subshapes.len(), 3);

    // The shared overload flattens too.
    let mut shared = ShapeCompound::new();
    shared.add_shape_shared(Some(Rc::new(outer)))?;
    assert_eq!(shared.size(), 3);

    // A base Clone's nullptr is stored as given.
    let mut nulls = ShapeCompound::new();
    nulls.add_shape_shared(Some(Rc::new(BaseOnly)))?;
    assert_eq!(nulls.size(), 1);
    assert!(nulls.unique_subshape().is_none());
    assert!(nulls.shapes()[0].is_none());

    // AddShape( (SHAPE*) nullptr ) stores the null pointer: a one-element compound.
    let mut raw_null = ShapeCompound::new();
    raw_null.add_shape(None)?;
    assert!(!raw_null.empty() && raw_null.is_dirty());
    assert_eq!(
        (raw_null.size(), raw_null.get_indexable_subshape_count()),
        (1, 1)
    );
    assert!(raw_null.unique_subshape().is_none());
    let mut subshapes: Vec<Option<&dyn Shape>> = Vec::new();
    raw_null.get_indexable_subshapes(&mut subshapes);
    assert!(subshapes.len() == 1 && subshapes[0].is_none());
    // A null shared_ptr is dereferenced by aShape->Clone(), and flattening a compound holding a
    // null entry dereferences it in subshape->Clone(): both undefined, refused unchanged.
    let mut target = ShapeCompound::new();
    assert_eq!(target.add_shape_shared(None), Err(ShapeError::Undefined));
    assert_eq!(
        target.add_shape(Some(Box::new(raw_null))),
        Err(ShapeError::Undefined)
    );
    assert!(target.empty());

    // An arc member stops the flattening at its out-of-line copy constructor.
    let mut with_arc = ShapeCompound::new();
    with_arc.add_shape(Some(Box::new(ShapeArc::new())))?;
    let mut target = ShapeCompound::new();
    assert_eq!(
        target.add_shape(Some(Box::new(with_arc))),
        Err(deferred(
            "SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)",
            SHAPE_ARC_UNIT
        ))
    );
    assert!(target.empty());

    let unit = |s| deferred(s, SHAPE_COMPOUND_UNIT);
    assert_eq!(
        shared.clone_shape().err(),
        Some(unit("SHAPE_COMPOUND::Clone"))
    );
    assert_eq!(shared.bbox(0).err(), Some(unit("SHAPE_COMPOUND::BBox")));
    assert_eq!(
        shared.is_solid().err(),
        Some(unit("SHAPE_COMPOUND::IsSolid"))
    );
    assert_eq!(
        shared.collide_shape_mtv(&BaseOnly, 0, None).err(),
        Some(deferred(
            "SHAPE::Collide(const SHAPE*, int, VECTOR2I*)",
            SHAPE_COLLISIONS_UNIT
        ))
    );
    Ok(())
}

#[test]
fn shape_ellipse_inline_members() -> ShapeResult<()> {
    let ellipse = Ellipse {
        center: v(3, 4),
        major_radius: 20,
        minor_radius: 10,
        rotation: EdaAngle::degrees(30.0),
        start_angle: EdaAngle::degrees(0.0),
        end_angle: EdaAngle::degrees(90.0),
    };
    let e = ShapeEllipse::with_members(ellipse, true, [0.5, 0.75, 1.0, 2.0]);
    assert_eq!(
        (e.get_center(), e.get_major_radius(), e.get_minor_radius()),
        (v(3, 4), 20, 10)
    );
    assert_eq!(
        (e.get_rotation(), e.get_start_angle(), e.get_end_angle()),
        (
            EdaAngle::degrees(30.0),
            EdaAngle::degrees(0.0),
            EdaAngle::degrees(90.0)
        )
    );
    assert!(e.is_arc() && !e.is_solid()?);
    assert_eq!(e.type_name(), "SH_ELLIPSE");
    // The cache is not compared.
    let other_cache = ShapeEllipse::with_members(ellipse, true, [9.0, 9.0, 9.0, 9.0]);
    assert_eq!(e, other_cache);
    assert_ne!(
        e,
        ShapeEllipse::with_members(ellipse, false, [0.5, 0.75, 1.0, 2.0])
    );
    // EDA_ANGLE equality compares degrees: -0 equals 0.
    let neg_zero = Ellipse {
        start_angle: EdaAngle::degrees(-0.0),
        ..ellipse
    };
    assert_eq!(e, ShapeEllipse::with_members(neg_zero, true, e.cache()));
    let clone = e.clone_shape()?;
    let clone = clone
        .as_ref()
        .and_then(|s| s.as_any().downcast_ref::<ShapeEllipse>());
    assert_eq!(clone.map(ShapeEllipse::cache), Some(e.cache()));
    let mut target = other_cache;
    target.assign_from(&e);
    assert_eq!(target.cache(), e.cache());
    assert_eq!(
        ShapeEllipse::new().err(),
        Some(deferred(
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE()",
            SHAPE_ELLIPSE_UNIT
        ))
    );
    Ok(())
}
