// SPDX-License-Identifier: GPL-3.0-or-later
//! In-crate tests of the shape_line_chain.h, shape_index.h and shape_index_list.h bodies,
//! including states the C++ reaches only through out-of-line members (chains holding arcs).
use std::cell::RefCell;
use std::rc::Rc;

use crate::box2::Box2I;
use crate::seg::Seg;
use crate::shape::{Shape, ShapeError, ShapeLineChainBase, ShapeType};
use crate::shape_arc::{ShapeArc, SHAPE_ARC_UNIT};
use crate::shape_circle::ShapeCircle;
use crate::shape_index::{
    accept_visitor, bounding_box, collide, deferred_assert, query_callback, shape_functor,
    IndexForest, ShapeIndex, ShapeIndexError, WxCheck, WX_CHECK_BOUNDING_BOX, WX_CHECK_COLLIDE,
    WX_CHECK_SHAPE_FUNCTOR,
};
use crate::shape_index_list::{ListIterator, QueryIterator, ShapeIndexList, ShapeItem};
use crate::shape_line_chain::{
    ClipperZValue, CompareOriginDistance, Intersection, ReserveError, ShapeLineChain,
    SHAPES_ARE_PT, SHAPE_LINE_CHAIN_P001_UNIT, SHAPE_LINE_CHAIN_P002_UNIT, UNIT,
};
use crate::vector::{Vector2, Vector2I};
use kicad_rtree_cow_packed::Budget;

fn v(x: i32, y: i32) -> Vector2I {
    Vector2::new(x, y)
}

fn chain(points: &[(i32, i32)]) -> ShapeLineChain {
    let mut c = ShapeLineChain::new();
    for &(x, y) in points {
        assert_eq!(c.append_xy(x, y, false), Ok(()));
    }
    c
}

fn unported(upstream: &'static str, unit: &'static str) -> ShapeError {
    ShapeError::Unported { upstream, unit }
}

#[test]
fn clipper_z_value_offsets_only_non_negative_indices() {
    assert_eq!(
        ClipperZValue::new(),
        ClipperZValue {
            first_arc_idx: -1,
            second_arc_idx: -1
        }
    );
    let z = ClipperZValue::from_indices((3, -1), 10);
    assert_eq!(
        z,
        Ok(ClipperZValue {
            first_arc_idx: 13,
            second_arc_idx: -1
        })
    );
    assert_eq!(
        ClipperZValue::from_indices((0, -5), -3),
        Ok(ClipperZValue {
            first_arc_idx: -3,
            second_arc_idx: -5
        })
    );
    assert_eq!(
        ClipperZValue::from_indices((i64::MAX, -1), 1),
        Err(ShapeError::Undefined)
    );
    assert_eq!(
        ClipperZValue::from_indices((-1, i64::MAX), 1),
        Err(ShapeError::Undefined)
    );
}

#[test]
fn intersection_and_origin_distance() {
    let i = Intersection::default();
    assert_eq!((i.p, i.index_our, i.index_their), (v(0, 0), -1, -1));
    assert!(!i.is_corner_our && !i.is_corner_their && !i.valid);

    let cmp = CompareOriginDistance::new(v(0, 0));
    let a = Intersection {
        p: v(3, 4),
        ..Intersection::default()
    };
    let b = Intersection {
        p: v(6, 0),
        ..Intersection::default()
    };
    assert_eq!(cmp.compare(&a, &b), Ok(true));
    assert_eq!(cmp.compare(&b, &a), Ok(false));
    assert_eq!(cmp.compare(&a, &a), Ok(false));
    let far = Intersection {
        p: v(i32::MIN, 0),
        ..Intersection::default()
    };
    assert_eq!(
        CompareOriginDistance::new(v(1, 0)).compare(&far, &a),
        Err(ShapeError::Undefined)
    );
}

#[test]
fn default_chain_and_counts() {
    let c = ShapeLineChain::new();
    assert_eq!(c.shape_type(), ShapeType::LineChain);
    assert_eq!(c.point_count(), 0);
    assert_eq!(c.segment_count(), 0);
    assert_eq!(c.get_point_count(), 0);
    assert!(!c.is_closed());
    assert_eq!(c.width(), 0);
    assert_eq!(c.get_width(), 0);
    assert_eq!(c.is_solid(), Ok(false));
    assert_eq!(c.cpoint(0), Err(ShapeError::Undefined));
    assert_eq!(c.clast_point(), Err(ShapeError::Undefined));
    assert!(c.get_cached_bbox().is_some_and(|b| !b.is_valid()));
    let b = c.bbox(0).ok();
    assert!(b.is_some_and(|b| !b.is_valid()));
}

#[test]
fn append_skips_repeats_and_merges_the_cache() {
    let mut c = chain(&[(1, 2), (1, 2), (5, -3)]);
    assert_eq!(c.cpoints(), &[v(1, 2), v(5, -3)]);
    assert_eq!(c.cshapes(), &[SHAPES_ARE_PT, SHAPES_ARE_PT]);
    let cached = *c
        .get_cached_bbox()
        .ok_or("box")
        .unwrap_or(&Box2I::default());
    assert_eq!(cached.position(), v(1, -3));
    assert_eq!(cached.size(), Vector2::new(4i64, 5));
    assert_eq!(c.append(v(5, -3), true), Ok(()));
    assert_eq!(c.point_count(), 3);
    assert_eq!(c.segment_count(), 2);
    assert_eq!(c.cpoint(-1), Ok(v(5, -3)));
    assert_eq!(c.cpoint(3), Ok(v(1, 2)));
    assert_eq!(c.cpoint(-4), Err(ShapeError::Undefined));
    assert_eq!(c.cpoint(6), Err(ShapeError::Undefined));
    assert_eq!(c.clast_point(), Ok(v(5, -3)));
}

#[test]
fn bbox_inflates_by_clearance_plus_width() {
    let mut c = chain(&[(0, 0), (10, 4)]);
    c.set_width(2);
    let b = c.bbox(3);
    assert!(b.is_ok_and(|b| b.position() == v(-5, -5) && b.size() == Vector2::new(20i64, 14)));
    c.set_width(i32::MAX);
    assert_eq!(c.bbox(1).err(), Some(ShapeError::Undefined));
    assert_eq!(
        c.bbox(-1).map(|b| b.position()),
        Ok(v(-2147483646, -2147483646))
    );
    // The cache: Compute leaves an empty chain's cache unchanged.
    let mut e = ShapeLineChain::new();
    e.set_width(4);
    *e.cached_bbox_mut() = Box2I::new(v(7, 7), Vector2::new(1, 1)).unwrap_or_default();
    assert_eq!(e.generate_bbox_cache(), Ok(()));
    let b = *e.get_cached_bbox().unwrap_or(&Box2I::default());
    assert_eq!((b.position(), b.size()), (v(3, 3), Vector2::new(9i64, 9)));
}

#[test]
fn move_points_then_arcs_then_cache() {
    let mut c = chain(&[(0, 0), (1, 1)]);
    assert_eq!(c.move_by(v(10, -10)), Ok(()));
    assert_eq!(c.cpoints(), &[v(10, -10), v(11, -9)]);
    let b = *c.get_cached_bbox().unwrap_or(&Box2I::default());
    assert_eq!(b.position(), v(10, -10));
    assert_eq!(c.move_by(v(i32::MAX, 0)), Err(ShapeError::Undefined));

    let mut arcs =
        ShapeLineChain::with_members(vec![v(0, 0)], vec![(0, -1)], vec![ShapeArc::new()], false);
    assert_eq!(
        arcs.move_by(v(1, 1)),
        Err(unported("SHAPE_ARC::Move", SHAPE_ARC_UNIT))
    );
    assert_eq!(arcs.cpoints(), &[v(1, 1)]);
}

#[test]
fn copies_and_moves() {
    let mut a = chain(&[(1, 1), (2, 2)]);
    a.set_width(3);
    let b = ShapeLineChain::copy_of(&a);
    assert!(b
        .as_ref()
        .is_ok_and(|b| b.cpoints() == a.cpoints() && b.width() == 3));

    let mut c = chain(&[(9, 9)]);
    assert_eq!(c.assign_from(&a), Ok(()));
    assert_eq!(c.ne(&a), Ok(false));
    assert_eq!(c.width(), 3);

    let mut d = ShapeLineChain::new();
    d.move_assign_from(&mut a);
    assert_eq!(d.cpoints(), &[v(1, 1), v(2, 2)]);
    assert_eq!(a.point_count(), 0);
    assert_eq!(a.width(), 3);
    assert!(a.get_cached_bbox().is_some_and(|b| b.is_valid()));

    let arcs =
        ShapeLineChain::with_members(vec![v(0, 0)], vec![(0, -1)], vec![ShapeArc::new()], false);
    assert_eq!(
        ShapeLineChain::copy_of(&arcs).err(),
        Some(unported(
            "SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)",
            SHAPE_ARC_UNIT
        ))
    );
    let mut e = ShapeLineChain::new();
    assert_eq!(
        e.assign_from(&arcs),
        Err(unported(
            "SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)",
            SHAPE_ARC_UNIT
        ))
    );
    // Assigning over as many arcs uses SHAPE_ARC's implicit copy assignment.
    let mut f =
        ShapeLineChain::with_members(vec![], vec![], vec![ShapeArc::new(), ShapeArc::new()], true);
    assert_eq!(f.assign_from(&arcs), Ok(()));
    assert_eq!((f.arc_count(), f.is_closed()), (1, false));
    assert_eq!(f.arc(0).map(|a| a.get_p0()), Ok(v(0, 0)));
    assert_eq!(f.arc(1).err(), Some(ShapeError::Undefined));
}

/// libstdc++'s `vector::operator=` chooses between reallocating (copy-constructing every arc from
/// the first) and assigning the existing arcs before constructing the rest, by capacity; a
/// shrinking assignment keeps the capacity. GCC 13.3 reference: an instrumented element type
/// in the same vector sequence sees the existing element already assigned at the first copy
/// construction when the capacity suffices, and unchanged when it does not.
#[test]
fn arc_assignment_follows_the_vector_capacity_branches() {
    let arc = |width| ShapeArc::with_points(v(0, 0), v(1, 1), v(2, 0), width);
    let width = |c: &ShapeLineChain, i| c.arc(i).map(|a| a.get_width());
    let copy_ctor = unported("SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)", SHAPE_ARC_UNIT);

    // Shrinking: assign in order, destroy the rest, keep the capacity.
    let mut d = ShapeLineChain::with_members(vec![], vec![], vec![arc(1), arc(2), arc(3)], false);
    let one = ShapeLineChain::with_members(vec![], vec![], vec![arc(77)], false);
    assert_eq!(d.assign_from(&one), Ok(()));
    assert_eq!((d.arc_count(), d.arcs_capacity()), (1, 3));
    assert_eq!(width(&d, 0), Ok(77));

    // Growing within the capacity: the existing arc is assigned before the first copy
    // construction, which defers; the size is unchanged and the later members not assigned.
    let two = ShapeLineChain::with_members(
        vec![v(5, 5)],
        vec![SHAPES_ARE_PT],
        vec![arc(9), arc(9)],
        true,
    );
    d.set_width(4);
    assert_eq!(d.assign_from(&two), Err(copy_ctor));
    assert_eq!((d.arc_count(), d.arcs_capacity()), (1, 3));
    assert_eq!(width(&d, 0), Ok(9));
    assert_eq!(d.cpoints(), &[v(5, 5)]);
    assert_eq!((d.is_closed(), d.width()), (false, 4));

    // Growing past the capacity: the first copy construction is of the source's first arc, before
    // anything in the destination's arcs changes.
    let mut e = ShapeLineChain::with_members(vec![], vec![], vec![arc(77)], false);
    assert_eq!(e.arcs_capacity(), 1);
    assert_eq!(e.assign_from(&two), Err(copy_ctor));
    assert_eq!((e.arc_count(), e.arcs_capacity()), (1, 1));
    assert_eq!(width(&e, 0), Ok(77));
    assert_eq!(e.cpoints(), &[v(5, 5)]);

    // Moving takes the capacity with the arcs; clearing keeps it.
    let mut f = ShapeLineChain::new();
    f.move_assign_from(&mut d);
    assert_eq!((f.arc_count(), f.arcs_capacity()), (1, 3));
    assert_eq!((d.arc_count(), d.arcs_capacity()), (0, 0));
    f.clear();
    assert_eq!((f.arc_count(), f.arcs_capacity()), (0, 3));
    assert_eq!(f.assign_from(&one), Err(copy_ctor));
    assert_eq!(f.arc_count(), 0);
}

#[test]
fn deferred_out_of_line_calls_keep_earlier_effects() {
    let mut c = chain(&[(0, 0), (1, 0), (0, 0)]);
    assert_eq!(
        c.set_closed(true),
        Err(unported(
            "SHAPE_LINE_CHAIN::mergeFirstLastPointIfNeeded",
            SHAPE_LINE_CHAIN_P001_UNIT
        ))
    );
    assert!(c.is_closed());
    assert_eq!(c.segment_count(), 3);
    assert_eq!(
        c.csegment(0),
        Err(unported(
            "SHAPE_LINE_CHAIN::Segment",
            SHAPE_LINE_CHAIN_P001_UNIT
        ))
    );
    assert_eq!(c.get_segment(0), c.csegment(0));
    assert_eq!(
        c.remove(1),
        Err(unported(
            "SHAPE_LINE_CHAIN::Remove(int, int)",
            SHAPE_LINE_CHAIN_P001_UNIT
        ))
    );
    assert_eq!(
        c.arc_index(0),
        Err(unported(
            "SHAPE_LINE_CHAIN::IsSharedPt",
            SHAPE_LINE_CHAIN_P002_UNIT
        ))
    );
    assert_eq!(c.reversed_arc_index(9), c.arc_index(0));
    assert_eq!(
        c.distance(v(0, 0), false),
        Err(unported(
            "SHAPE_LINE_CHAIN_BASE::SquaredDistance",
            SHAPE_LINE_CHAIN_P001_UNIT
        ))
    );
    assert_eq!(c.amend_arc_start(0, v(1, 1)), Err(ShapeError::Undefined));
    assert_eq!(c.amend_arc_end(0, v(1, 1)), Err(ShapeError::Undefined));
    let mut arcs =
        ShapeLineChain::with_members(vec![v(0, 0)], vec![(0, -1)], vec![ShapeArc::new()], false);
    assert_eq!(
        arcs.amend_arc_end(0, v(1, 1)),
        Err(unported(
            "SHAPE_LINE_CHAIN::amendArc",
            SHAPE_LINE_CHAIN_P001_UNIT
        ))
    );
    assert_eq!(
        c.clone_shape().err(),
        Some(unported(
            "SHAPE_LINE_CHAIN::Clone",
            SHAPE_LINE_CHAIN_P002_UNIT
        ))
    );
}

#[test]
fn segment_count_wraps_like_the_size_t_difference() {
    let mut c = ShapeLineChain::with_members(vec![], vec![], vec![], true);
    assert_eq!(c.segment_count(), 0);
    c.clear();
    assert!(!c.is_closed());
    let one = ShapeLineChain::with_members(vec![v(0, 0)], vec![SHAPES_ARE_PT], vec![], true);
    assert_eq!(one.segment_count(), 1);
}

#[test]
fn reserve_points_reports_length_errors() {
    let mut c = ShapeLineChain::new();
    assert_eq!(c.reserve_points(16), Ok(()));
    assert_eq!(c.reserve_points(u64::MAX), Err(ReserveError::LengthError));
    assert_eq!(c.reserve_points(1 << 60), Err(ReserveError::LengthError));
    assert_eq!(c.reserve_points((1 << 60) - 1), Err(ReserveError::BadAlloc));
}

/// A test item: a shape on a set of layers.
#[derive(Debug)]
struct Item {
    shape: Option<Box<dyn Shape>>,
    layers: Vec<i32>,
}

#[derive(Clone, Copy, Debug)]
struct ItemRef<'a>(&'a Item, usize);

impl PartialEq for ItemRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

impl kicad_rtree_cow_packed::NodeData for ItemRef<'_> {}

impl<'a> ShapeItem<'a> for ItemRef<'a> {
    fn shape(&self, layer: i32) -> Option<&'a dyn Shape> {
        if self.0.layers.contains(&layer) {
            self.0.shape.as_deref()
        } else {
            None
        }
    }
}

fn circle_item(x: i32, y: i32, r: i32) -> Item {
    Item {
        shape: Some(Box::new(ShapeCircle::with_center_radius(v(x, y), r))),
        layers: vec![-1, 0],
    }
}

#[test]
fn index_helpers_assert_through_the_host() {
    let item = circle_item(0, 0, 5);
    let r = Some(ItemRef(&item, 0));
    let mut calls: Vec<WxCheck> = Vec::new();
    let mut record = |c: &WxCheck| -> Result<(), ShapeIndexError> {
        calls.push(*c);
        Ok(())
    };
    assert!(shape_functor(r, 0, &mut record).is_ok_and(|s| s.is_some()));
    assert!(shape_functor(r, 7, &mut record).is_ok_and(|s| s.is_none()));
    assert!(shape_functor::<ItemRef<'_>>(None, 0, &mut record).is_ok_and(|s| s.is_none()));
    let b = bounding_box(r, 0, &mut record);
    assert!(b.is_ok_and(|b| b.position() == v(-5, -5)));
    let b = bounding_box(r, 3, &mut record);
    assert!(b.is_ok_and(|b| !b.is_valid()));
    assert_eq!(
        collide(r, Seg::new(v(0, 9), v(1, 9)), 0, 5, &mut record),
        Ok(true)
    );
    assert_eq!(
        collide(r, Seg::new(v(0, 9), v(1, 9)), 2, 5, &mut record),
        Ok(false)
    );
    assert_eq!(
        calls,
        vec![
            WX_CHECK_SHAPE_FUNCTOR,
            WX_CHECK_BOUNDING_BOX,
            WX_CHECK_COLLIDE
        ]
    );
    assert_eq!(
        bounding_box::<ItemRef<'_>>(None, 0, &mut deferred_assert).err(),
        Some(ShapeIndexError::Unported {
            upstream: "wxOnAssert",
            unit: UNIT
        })
    );

    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    let mut count = 0;
    let visitor = move |i: usize| {
        o.borrow_mut().push((count, i));
        count += 1;
    };
    accept_visitor(1, visitor.clone());
    assert!(query_callback(2, &visitor));
    assert_eq!(*out.borrow(), vec![(0, 1), (0, 2)]);
}

#[test]
fn shape_index_add_query_remove_reindex() {
    let items = [
        circle_item(0, 0, 5),
        circle_item(100, 0, 5),
        circle_item(50, 50, 1),
    ];
    let refs: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, it)| ItemRef(it, i))
        .collect();
    let mut forest: IndexForest<ItemRef<'_>> = match IndexForest::new(Budget {
        entries: 64,
        trees: 8,
        nodes: 64,
    }) {
        Ok(f) => f,
        Err(e) => return assert_eq!(Some(e), None),
    };
    let mut assert_none =
        |_: &WxCheck| -> Result<(), ShapeIndexError> { Err(ShapeIndexError::Undefined) };
    let Ok(mut index) = ShapeIndex::new(&mut forest, 0) else {
        return assert_eq!(1, 0);
    };
    for r in &refs {
        assert_eq!(index.add(&mut forest, Some(*r), &mut assert_none), Ok(()));
    }
    assert_eq!(index.size(&forest), Ok(3));

    let probe = ShapeCircle::with_center_radius(v(0, 0), 1);
    let mut hits = Vec::new();
    let mut visitor = |i: &Option<ItemRef<'_>>| {
        hits.push(i.map(|r| r.1));
        true
    };
    assert_eq!(index.query(&forest, &probe, 10, &mut visitor), Ok(1));
    assert_eq!(hits, vec![Some(0)]);

    let Ok(clone) = index.clone_index(&mut forest) else {
        return assert_eq!(1, 0);
    };
    assert_eq!(
        index.remove(&mut forest, Some(refs[0]), &mut assert_none),
        Ok(())
    );
    assert_eq!(index.size(&forest), Ok(2));
    assert_eq!(clone.size(&forest), Ok(3));

    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    assert_eq!(
        index.accept(&forest, move |i: Option<ItemRef<'_>>| s
            .borrow_mut()
            .push(i.map(|r| r.1))),
        Ok(())
    );
    let mut seen = seen.borrow().clone();
    seen.sort();
    assert_eq!(seen, vec![Some(1), Some(2)]);

    assert_eq!(index.reindex(&mut forest, &mut assert_none), Ok(()));
    assert_eq!(index.size(&forest), Ok(2));
    let Ok(mut it) = index.begin_iterator(&forest) else {
        return assert_eq!(1, 0);
    };
    assert!(it.is_not_null());
    let first = it.next_item();
    assert!(first.is_ok_and(|f| f.is_some()));
    assert_eq!(it.increment(), Ok(false));
    assert!(it.is_null());
    assert!(it.deref().is_some());

    assert_eq!(index.remove_all(&mut forest), Ok(()));
    assert_eq!(index.size(&forest), Ok(0));
    let Ok(empty) = index.begin_iterator(&forest) else {
        return assert_eq!(1, 0);
    };
    assert!(empty.is_null() && empty.deref().is_none());
    assert_eq!(index.destroy(&mut forest), Ok(()));
    assert_eq!(clone.destroy(&mut forest), Ok(()));
}

#[test]
fn shape_index_list_iterators_and_queries() {
    let items = [
        circle_item(0, 0, 5),
        circle_item(100, 0, 5),
        circle_item(8, 0, 1),
    ];
    let refs: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, it)| ItemRef(it, i))
        .collect();
    let mut list = ShapeIndexList::new();
    // Never allocated: begin() and end() are null iterators.
    assert!(list.begin() == list.end() && list.begin() == ListIterator::null());
    assert_eq!(list.begin().deref().err(), Some(ShapeError::Undefined));
    for r in &refs {
        assert_eq!(list.add(Some(*r)), Ok(()));
    }
    assert_eq!(list.add(None), Err(ShapeError::Undefined));
    assert_eq!(list.size(), 3);
    let mut it = list.begin();
    assert!(it != ListIterator::null());
    assert_eq!(it.deref().map(|r| r.map(|r| r.1)), Ok(Some(0)));
    assert!(it.increment().is_ok());
    assert!(it.increment().is_ok());
    assert!(it.increment().is_ok());
    assert!(it == list.end());
    assert_eq!(it.increment().err(), Some(ShapeError::Undefined));

    let probe = ShapeCircle::with_center_radius(v(0, 0), 1);
    let got = RefCell::new(Vec::new());
    let mut visitor = |i: Option<ItemRef<'_>>| {
        got.borrow_mut().push(i.map(|r| r.1));
        got.borrow().len() < 5
    };
    assert_eq!(list.query(&probe, 6, &mut visitor, false), Ok(2));
    assert_eq!(*got.borrow(), vec![Some(0), Some(2)]);
    assert_eq!(
        list.query(&probe, 3, &mut visitor, true).err(),
        Some(ShapeError::Unported {
            upstream: "SHAPE::Collide(const SHAPE*, int, int*, VECTOR2I*)",
            unit: crate::shape::SHAPE_COLLISIONS_UNIT
        })
    );

    let q = list.qbegin(Some(&probe), 6, false);
    let Ok(mut q) = q else {
        return assert_eq!(1, 0);
    };
    assert_eq!(q.deref().map(|r| r.map(|r| r.1)), Ok(Some(0)));
    assert!(q.increment().is_ok());
    assert_eq!(q.deref().map(|r| r.map(|r| r.1)), Ok(Some(2)));
    assert!(q.increment().is_ok());
    assert!(list.qend().is_ok_and(|e| e == q));
    assert!(QueryIterator::<Option<ItemRef<'_>>>::null()
        .try_copy()
        .is_err());

    list.remove(Some(refs[0]));
    assert_eq!(list.size(), 2);
    list.clear();
    assert_eq!(list.size(), 0);
    // Cleared but allocated: begin() is no longer a null iterator.
    assert!(list.begin() != ListIterator::null() && list.begin() == list.end());
}
