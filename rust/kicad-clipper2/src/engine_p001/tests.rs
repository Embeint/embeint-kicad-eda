// SPDX-License-Identifier: GPL-3.0-or-later
//! Checks that need no oracle: properties the oracle cannot observe (the bounded walks, the
//! freed-array refusal, the sort's stability) and the typed deferrals at the calls into lines
//! 2272 onward. Exact results are compared with the pinned C++ by the difftest corpus.

use super::*;

fn square() -> Path64 {
    vec![
        Point64::xy(0, 0),
        Point64::xy(10, 0),
        Point64::xy(10, 10),
        Point64::xy(0, 10),
    ]
}

/// A ring of `points` in `base`, owned by a new output record.
fn ring(base: &mut ClipperBase, points: &[Point64]) -> Result<OutRecId> {
    let o = base.new_outrec()?;
    let mut first: Option<OutPtId> = None;
    let mut last: Option<OutPtId> = None;
    for pt in points {
        let op = base.new_out_pt(pt, Some(o))?;
        if let (Some(f), Some(l)) = (first, last) {
            base.out_pt_mut(op)?.prev = l;
            base.out_pt_mut(op)?.next = f;
            base.out_pt_mut(l)?.next = op;
            base.out_pt_mut(f)?.prev = op;
        } else {
            first = Some(op);
        }
        last = Some(op);
    }
    base.outrec_mut(o)?.pts = first;
    Ok(o)
}

#[test]
fn stable_sort_keeps_equal_keys_in_order() {
    let mut items: Vec<(i32, usize)> = (0..200).map(|i| ((i * 7919) % 13, i as usize)).collect();
    let mut expected = items.clone();
    expected.sort_by_key(|item| item.0); // std's sort_by_key is stable
    assert!(stable_sort(&mut items, |a, b| a.0 < b.0).is_ok());
    assert_eq!(items, expected);
}

#[test]
fn loc_min_sorter_is_descending_y_then_ascending_x() {
    let (a, b) = (Point64::xy(0, 5), Point64::xy(1, 5));
    assert!(loc_min_sorter(&a, &b));
    assert!(!loc_min_sorter(&b, &a));
    assert!(loc_min_sorter(&Point64::xy(9, 6), &a));
    assert!(!loc_min_sorter(&a, &a));
}

#[test]
fn add_paths_finds_the_square_minimum_and_reset_orders_scanlines() -> Result<()> {
    let mut base = ClipperBase::default();
    base.add_paths(&vec![square()], PathType::Subject, false)?;
    assert_eq!(base.minima_list().len(), 1);
    let lm = *base.local_minima(base.minima_list()[0])?;
    // Y axis positive down: the minimum is the bottom (largest y) of the left edge.
    assert_eq!(base.vertex(lm.vertex)?.pt, Point64::xy(0, 10));
    base.reset()?;
    assert_eq!(base.scanlines()?, vec![10]);
    assert_eq!(base.pop_scanline(), Some(10));
    assert_eq!(base.pop_scanline(), None);
    Ok(())
}

#[test]
fn add_paths_accepts_the_whole_int64_range() -> Result<()> {
    // AddPaths_ and Reset only compare coordinates.
    let mut base = ClipperBase::default();
    let extreme = vec![
        Point64::xy(i64::MIN, i64::MIN),
        Point64::xy(i64::MAX, i64::MIN),
        Point64::xy(i64::MAX, i64::MAX),
    ];
    base.add_paths(&vec![extreme], PathType::Clip, true)?;
    base.reset()?;
    assert!(base.has_open_paths());
    assert!(!base.minima_list().is_empty());
    Ok(())
}

#[test]
fn reused_arrays_keep_one_identity_and_refuse_use_after_clear() -> Result<()> {
    let mut reuse = ReuseableDataContainer64::default();
    reuse.add_paths(&vec![square()], PathType::Subject, false)?;
    let mut base = ClipperBase::default();
    base.add_reuseable_data(&reuse)?;
    base.add_reuseable_data(&reuse)?;
    assert!(!base.succeeded()); // AddReuseableData clears succeeded_
    let (a, b) = (base.minima_list()[0], base.minima_list()[1]);
    let (va, vb) = (base.local_minima(a)?.vertex, base.local_minima(b)?.vertex);
    assert_eq!(va, vb); // the same vertex, as the C++ pointers are
    assert_eq!(base.vertex_list_slots().len(), 1);
    // The bound's links are rebound to the list the base reached the array through.
    let e = base.new_bound(a, false)?;
    assert_eq!(base.next_vertex(e)?.map(|v| v.list), Some(va.list));
    reuse.clear();
    assert_eq!(base.vertex(va).map(|v| v.pt), Err(DANGLING_VERTEX));
    assert_eq!(base.reset(), Err(DANGLING_VERTEX));
    Ok(())
}

#[test]
fn malformed_rings_and_owner_cycles_are_refused_not_followed() -> Result<()> {
    let mut base = ClipperBase::default();
    let o = ring(&mut base, &square())?;
    let pts = some(base.outrec(o)?.pts)?;
    // A rho-shaped ring: the walk from pts never returns to it.
    let second = base.out_pt(pts)?.next;
    let third = base.out_pt(second)?.next;
    base.out_pt_mut(third)?.next = second;
    assert_eq!(base.area_of(pts), Err(ENDLESS));
    assert_eq!(base.point_count(pts), Err(ENDLESS));
    // An owner cycle of records without points.
    let a = base.new_outrec()?;
    let b = base.new_outrec()?;
    base.outrec_mut(a)?.owner = Some(b);
    base.outrec_mut(b)?.owner = Some(a);
    assert_eq!(base.get_real_outrec(Some(a)), Err(ENDLESS));
    Ok(())
}

#[test]
fn deleted_records_are_refused() -> Result<()> {
    let mut base = ClipperBase::default();
    let o = ring(&mut base, &square())?;
    let pts = some(base.outrec(o)?.pts)?;
    let second = base.out_pt(pts)?.next;
    base.dispose_out_pt(second)?;
    assert_eq!(base.out_pt(second).map(|op| op.pt), Err(DELETED));
    assert_eq!(base.point_count(pts)?, 3);
    base.dispose_out_pts(o)?;
    assert_eq!(base.out_pt(pts).map(|op| op.pt), Err(DELETED));
    Ok(())
}

#[test]
fn clean_up_releases_the_arenas() -> Result<()> {
    let mut base = ClipperBase::default();
    base.add_paths(&vec![square()], PathType::Subject, false)?;
    base.reset()?;
    let e = base.new_bound(base.minima_list()[0], true)?;
    base.insert_left_edge(e)?;
    base.start_open_path(e, &Point64::xy(0, 10))?;
    ring(&mut base, &square())?;
    base.clean_up()?;
    assert!(base.active_edges.is_empty() && base.out_pts.is_empty());
    assert!(base.outrecs().is_empty() && base.actives_head().is_none());
    assert_eq!(base.minima_list().len(), 1); // CleanUp keeps the added paths
    base.clear()?;
    assert!(base.minima_list().is_empty() && base.vertex_list_slots().is_empty());
    Ok(())
}

#[test]
fn calls_into_lines_2272_onward_and_clipper_h_continue() -> Result<()> {
    let mut base = ClipperBase::default();
    base.add_paths(&vec![square()], PathType::Subject, false)?;
    base.reset()?;
    base.set_clip_type(ClipType::Union);
    base.set_fill_rule(FillRule::NonZero);
    // The contributing square reaches CheckJoinLeft (src p002) after AddLocalMinPoly and
    // continues: no left neighbour, so no join, and the right bound's scanline follows.
    base.insert_local_minima_into_ael(10)?;
    assert_eq!(base.outrecs().len(), 1);
    assert!(base.actives_head().is_some() && !base.scanlines()?.is_empty());
    // Path2ContainsPath1<int64_t> (clipper.h, include p002) now runs: every vertex of the
    // same square is on its boundary, so the result is false.
    assert_eq!(path2_contains_path1_paths(&square(), &square()), Ok(false));
    Ok(())
}

#[test]
fn stripe_index_declines_combs_and_refuses_overflow() -> Result<()> {
    // 100 edges make 10 stripes; each tall tooth edge spans all of them, more than 8
    // references per edge.
    let mut comb = Vec::new();
    for k in 0..100 {
        comb.push(Point64::xy(k * 10, if k % 2 == 0 { 0 } else { 1000 }));
    }
    let mut index = StripeIndex::default();
    index.build_from_path(&comb)?;
    assert!(index.empty()); // tall edges each span many stripes
    assert_eq!(
        index.point_in_polygon(&Point64::xy(5, 5))?,
        PointInPolygonResult::IsOutside
    );
    let wide = vec![
        Point64::xy(0, -(1 << 62)),
        Point64::xy(1, 0),
        Point64::xy(0, (1 << 62) - 1),
        Point64::xy(-1, 0),
    ];
    // y_range = 2^63 - 1 fits, but y_range + stripe_count_ - 1 does not.
    assert_eq!(index.build_from_path(&wide), Err(I64_OVERFLOW));
    Ok(())
}

#[test]
fn top_x_rounds_half_to_even() -> Result<()> {
    let e = Active {
        bot: Point64::xy(100, 0),
        top: Point64::xy(101, 1000),
        dx: 0.5,
        ..Active::default()
    };
    assert_eq!(top_x(&e, 1)?, 100); // 0.5 -> 0
    assert_eq!(top_x(&e, 3)?, 102); // 1.5 -> 2
    let horizontal = Active {
        bot: Point64::xy(0, 4),
        top: Point64::xy(9, 4),
        dx: -f64::MAX,
        ..Active::default()
    };
    assert!(matches!(
        top_x(&horizontal, 5),
        Err(ClipperError::Undefined(_))
    ));
    Ok(())
}
