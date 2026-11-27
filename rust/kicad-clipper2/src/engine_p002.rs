// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.engine.cpp (Copyright Angus Johnson 2010-2026), Boost
// Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `clipper.engine.cpp` lines 2272-3337: the engine half of unit
//! `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002` (its other half, `clipper.offset.cpp`,
//! is [`crate::offset`]).
//!
//! The scanline loop (`ExecuteInternal`), intersections, horizontals, maxima, joins and
//! splits, horizontal join processing, ownership nesting and the `Clipper64`/`ClipperD` output
//! builders, in the reference's order, on the arenas, minima and vertex stores of
//! [`crate::engine_p001`]. `delete` of an `Active` empties its arena slot, so any later use is
//! refused as the undefined behaviour it is; identities (`ActiveId`) stay comparable.
//!
//! Every loop that follows links is bounded by the arena it walks; the two recursions of the
//! reference (`CheckSplitOwner`, `RecursiveCheckOwners`) run on explicit, fallibly grown
//! stacks with the same visiting order, and a chain deeper than the records involved (a cycle,
//! on which the reference recurses forever) is refused. The ambiguity fallback of
//! `Path2ContainsPath1` calls the `clipper.h` template owned by
//! `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`; it stays a typed
//! deferral at that call, inside [`crate::engine_p001`].

use crate::clipper_core::FillRule;
use crate::clipper_core::{
    get_bounds_path, get_closest_point_on_segment, get_line_intersect_pt, is_collinear,
    perpendic_dist_from_line_sqrd, Path64, PathD, Paths64, PathsD, Point64, PointD,
};
use crate::clipper_engine::{
    ActiveId, ClipType, ClipperBase, HorzJoin, HorzSegment, HorzSegmentId, IntersectNode, JoinWith,
    OutPtId, OutRecId, PolyPathId, PolyTree64, PolyTreeD, VertexId,
};
use crate::engine_p001::{
    add64, intersect_list_sort, is_horizontal, is_hot_edge, is_joined, some, stable_sort, top_x,
    Walk, DELETED, ENDLESS, NULL,
};
use crate::{push, reserve, with_capacity, ClipperError, Result};

/// This port unit.
pub const UNIT: &str = crate::ENGINE_SRC_P002;

/// A `std::sort` comparator that is not a strict weak ordering (impossible for
/// `IntersectListSort`, which is one).
const BAD_ORDERING: ClipperError = ClipperError::Undefined("std::sort with an invalid ordering");
/// `node_iter2` ran past the end of `intersect_nodes_`.
const NODE_RANGE: ClipperError = ClipperError::Undefined("intersect node iterator out of range");

/// The polygon trees `RecursiveCheckOwners` adds to through the virtual
/// `PolyPath::AddChild(const Path64&)`: `PolyPath64` stores the path, `PolyPathD` scales it by
/// its own scale.
pub trait PolyTreeSink {
    /// `PolyPath* AddChild(const Path64& path)` on node `id`.
    fn add_child64(&mut self, id: PolyPathId, path: &Path64) -> Result<PolyPathId>;
}

impl PolyTreeSink for PolyTree64 {
    fn add_child64(&mut self, id: PolyPathId, path: &Path64) -> Result<PolyPathId> {
        self.add_child(id, path)
    }
}

impl PolyTreeSink for PolyTreeD {
    fn add_child64(&mut self, id: PolyPathId, path: &Path64) -> Result<PolyPathId> {
        self.add_child(id, path)
    }
}

/// `inline bool SetHorzSegHeadingForward(HorzSegment& hs, OutPt* opP, OutPt* opN)` on the two
/// points' x coordinates.
pub fn set_horz_seg_heading_forward(
    hs: &mut HorzSegment,
    op_p: OutPtId,
    op_p_x: i64,
    op_n: OutPtId,
    op_n_x: i64,
) -> bool {
    if op_p_x == op_n_x {
        return false;
    }
    if op_p_x < op_n_x {
        hs.left_op = Some(op_p);
        hs.right_op = Some(op_n);
        hs.left_to_right = true;
    } else {
        hs.left_op = Some(op_n);
        hs.right_op = Some(op_p);
        hs.left_to_right = false;
    }
    true
}

/// One frame of the explicit `CheckSplitOwner` recursion: the record whose `splits` list is
/// walked, the loop index, and where the frame resumes.
#[derive(Clone, Copy)]
struct SplitFrame {
    list: OutRecId,
    idx: usize,
    stage: SplitStage,
    split: OutRecId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SplitStage {
    /// Read `(*splits)[idx]` and maybe recurse into its own splits (`#942`).
    Element,
    /// `split = GetRealOutRec(split)`, the loop guard and maybe the second recursion.
    Real,
    /// The bounds and containment tests.
    Contains,
}

impl ClipperBase {
    /// `delete &e` for an edge: its arena slot is emptied.
    fn delete_active(&mut self, e: ActiveId) -> Result<()> {
        let slot = self.active_edges.get_mut(e.0).ok_or(DELETED)?;
        if slot.take().is_none() {
            return Err(DELETED);
        }
        Ok(())
    }

    /// `inline void DeleteFromAEL(Active& e)`.
    pub fn delete_from_ael(&mut self, e: ActiveId) -> Result<()> {
        let a = *self.active(e)?;
        let (prev, next) = (a.prev_in_ael, a.next_in_ael);
        if prev.is_none() && next.is_none() && Some(e) != self.actives {
            return Ok(()); // already deleted
        }
        match prev {
            Some(p) => self.ae_mut(p)?.next_in_ael = next,
            None => self.actives = next,
        }
        if let Some(n) = next {
            self.ae_mut(n)?.prev_in_ael = prev;
        }
        self.delete_active(e)
    }

    /// `inline void AdjustCurrXAndCopyToSEL(const int64_t top_y)`.
    pub fn adjust_curr_x_and_copy_to_sel(&mut self, top_y: i64) -> Result<()> {
        let mut e = self.actives;
        self.sel = e;
        let mut walk = self.ael_walk();
        while let Some(x) = e {
            walk.step()?;
            let a = self.ae_mut(x)?;
            a.prev_in_sel = a.prev_in_ael;
            a.next_in_sel = a.next_in_ael;
            a.jump = a.next_in_sel;
            // it is safe to ignore 'joined' edges here because
            // if necessary they will be split in IntersectEdges()
            let curr_x = top_x(self.active(x)?, top_y)?;
            self.ae_mut(x)?.curr_x = curr_x;
            e = self.active(x)?.next_in_ael;
        }
        Ok(())
    }

    /// `bool ExecuteInternal(ClipType ct, FillRule fillrule, bool use_polytrees)`.
    pub fn execute_internal(
        &mut self,
        ct: ClipType,
        fillrule: FillRule,
        use_polytrees: bool,
    ) -> Result<bool> {
        self.cliptype = ct;
        self.fillrule = fillrule;
        self.using_polytree = use_polytrees;
        self.reset()?;
        if ct == ClipType::NoClip {
            return Ok(true);
        }
        let Some(mut y) = self.pop_scanline() else {
            return Ok(true);
        };

        while self.succeeded {
            self.insert_local_minima_into_ael(y)?;
            while let Some(e) = self.pop_horz()? {
                self.do_horizontal(e)?;
            }
            if !self.horz_seg_list.is_empty() {
                self.convert_horz_segs_to_joins()?;
                self.horz_seg_list.clear();
            }
            self.bot_y = y; // bot_y_ == bottom of scanbeam
            match self.pop_scanline() {
                Some(top) => y = top, // y new top of scanbeam
                None => break,
            }
            self.do_intersections(y)?;
            self.do_top_of_scanbeam(y)?;
            while let Some(e) = self.pop_horz()? {
                self.do_horizontal(e)?;
            }
        }
        if self.succeeded {
            self.process_horz_joins()?;
        }
        Ok(self.succeeded)
    }

    /// `inline void FixOutRecPts(OutRec* outrec)`.
    pub fn fix_outrec_pts(&mut self, outrec: OutRecId) -> Result<()> {
        let start = some(self.outrec(outrec)?.pts)?;
        let mut op = start;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            self.op_mut(op)?.outrec = Some(outrec);
            op = self.out_pt(op)?.next;
            if Some(op) == self.outrec(outrec)?.pts {
                return Ok(());
            }
        }
    }

    /// `inline bool UpdateHorzSegment(HorzSegment& hs)`. `slot` is the segment's position in
    /// `horz_seg_list_`, recorded as the `OutPt::horz` pointer (only its nullness is ever read).
    pub fn update_horz_segment(&mut self, hs: &mut HorzSegment, slot: usize) -> Result<bool> {
        let op = some(hs.left_op)?;
        let outrec = some(self.get_real_outrec(self.out_pt(op)?.outrec)?)?;
        let outrec_has_edges = self.outrec(outrec)?.front_edge.is_some();
        let curr_y = self.out_pt(op)?.pt.y;
        let (mut op_p, mut op_n) = (op, op);
        let mut walk = self.ring_walk();
        if outrec_has_edges {
            let op_a = some(self.outrec(outrec)?.pts)?;
            let op_z = self.out_pt(op_a)?.next;
            while op_p != op_z && self.out_pt(self.out_pt(op_p)?.prev)?.pt.y == curr_y {
                walk.step()?;
                op_p = self.out_pt(op_p)?.prev;
            }
            while op_n != op_a && self.out_pt(self.out_pt(op_n)?.next)?.pt.y == curr_y {
                walk.step()?;
                op_n = self.out_pt(op_n)?.next;
            }
        } else {
            while self.out_pt(op_p)?.prev != op_n
                && self.out_pt(self.out_pt(op_p)?.prev)?.pt.y == curr_y
            {
                walk.step()?;
                op_p = self.out_pt(op_p)?.prev;
            }
            while self.out_pt(op_n)?.next != op_p
                && self.out_pt(self.out_pt(op_n)?.next)?.pt.y == curr_y
            {
                walk.step()?;
                op_n = self.out_pt(op_n)?.next;
            }
        }
        let (p_x, n_x) = (self.out_pt(op_p)?.pt.x, self.out_pt(op_n)?.pt.x);
        let result = set_horz_seg_heading_forward(hs, op_p, p_x, op_n, n_x)
            && self.out_pt(some(hs.left_op)?)?.horz.is_none();

        if result {
            let left = some(hs.left_op)?;
            self.op_mut(left)?.horz = Some(HorzSegmentId(slot));
        } else {
            hs.right_op = None; // (for sorting)
        }
        Ok(result)
    }

    /// `void ConvertHorzSegsToJoins()`.
    pub fn convert_horz_segs_to_joins(&mut self) -> Result<()> {
        // std::count_if visits every segment.
        let mut j = 0usize;
        for i in 0..self.horz_seg_list.len() {
            let mut hs = *self.horz_seg_list.get(i).ok_or(NULL)?;
            let updated = self.update_horz_segment(&mut hs, i)?;
            *self.horz_seg_list.get_mut(i).ok_or(NULL)? = hs;
            if updated {
                j += 1;
            }
        }
        if j < 2 {
            return Ok(());
        }

        // std::stable_sort(horz_seg_list_, HorzSegSorter()), on the sorter's keys.
        let mut keyed: Vec<(bool, i64, HorzSegment)> = with_capacity(self.horz_seg_list.len())?;
        for hs in &self.horz_seg_list {
            let has_right = hs.right_op.is_some();
            let left_x = if has_right {
                self.out_pt(some(hs.left_op)?)?.pt.x
            } else {
                0
            };
            keyed.push((has_right, left_x, *hs));
        }
        stable_sort(&mut keyed, |hs1, hs2| {
            if !hs1.0 || !hs2.0 {
                hs1.0
            } else {
                hs2.1 > hs1.1
            }
        })?;
        for (slot, (_, _, hs)) in self.horz_seg_list.iter_mut().zip(keyed) {
            *slot = hs;
        }

        let x = |base: &Self, op: OutPtId| -> Result<i64> { Ok(base.out_pt(op)?.pt.x) };
        let y = |base: &Self, op: OutPtId| -> Result<i64> { Ok(base.out_pt(op)?.pt.y) };
        let next = |base: &Self, op: OutPtId| -> Result<OutPtId> { Ok(base.out_pt(op)?.next) };
        let prev = |base: &Self, op: OutPtId| -> Result<OutPtId> { Ok(base.out_pt(op)?.prev) };
        for i1 in 0..j - 1 {
            for i2 in i1 + 1..j {
                let hs1 = *self.horz_seg_list.get(i1).ok_or(NULL)?;
                let hs2 = *self.horz_seg_list.get(i2).ok_or(NULL)?;
                let (mut l1, r1) = (some(hs1.left_op)?, some(hs1.right_op)?);
                let (mut l2, r2) = (some(hs2.left_op)?, some(hs2.right_op)?);
                if x(self, l2)? >= x(self, r1)?
                    || hs2.left_to_right == hs1.left_to_right
                    || x(self, r2)? <= x(self, l1)?
                {
                    continue;
                }
                let curr_y = y(self, l1)?;
                let mut walk = self.ring_walk();
                let join = if hs1.left_to_right {
                    while y(self, next(self, l1)?)? == curr_y
                        && x(self, next(self, l1)?)? <= x(self, l2)?
                    {
                        walk.step()?;
                        l1 = next(self, l1)?;
                    }
                    while y(self, prev(self, l2)?)? == curr_y
                        && x(self, prev(self, l2)?)? <= x(self, l1)?
                    {
                        walk.step()?;
                        l2 = prev(self, l2)?;
                    }
                    // The two insertions commute (each links a new point beside its own
                    // anchor), so the unspecified argument evaluation order cannot change
                    // the rings.
                    let op1 = self.duplicate_op(l1, true)?;
                    let op2 = self.duplicate_op(l2, false)?;
                    HorzJoin::new(Some(op1), Some(op2))
                } else {
                    while y(self, prev(self, l1)?)? == curr_y
                        && x(self, prev(self, l1)?)? <= x(self, l2)?
                    {
                        walk.step()?;
                        l1 = prev(self, l1)?;
                    }
                    while y(self, next(self, l2)?)? == curr_y
                        && x(self, next(self, l2)?)? <= x(self, l1)?
                    {
                        walk.step()?;
                        l2 = next(self, l2)?;
                    }
                    let op1 = self.duplicate_op(l2, true)?;
                    let op2 = self.duplicate_op(l1, false)?;
                    HorzJoin::new(Some(op1), Some(op2))
                };
                self.horz_seg_list.get_mut(i1).ok_or(NULL)?.left_op = Some(l1);
                self.horz_seg_list.get_mut(i2).ok_or(NULL)?.left_op = Some(l2);
                push(&mut self.horz_join_list, join)?;
            }
        }
        Ok(())
    }

    /// `void MoveSplits(OutRec* fromOr, OutRec* toOr)`.
    pub fn move_splits(&mut self, from_or: OutRecId, to_or: OutRecId) -> Result<()> {
        if from_or == to_or {
            // Appending to the vector being iterated invalidates its iterators.
            return Err(ClipperError::Undefined("MoveSplits onto its own splits"));
        }
        let items = crate::copy_of(some(self.outrec(from_or)?.splits.as_ref())?)?;
        let to = self.or_mut(to_or)?;
        let splits = to.splits.get_or_insert_with(Vec::new);
        for item in items {
            if item != to_or {
                // #987
                push(splits, item)?;
            }
        }
        if let Some(splits) = self.or_mut(from_or)?.splits.as_mut() {
            splits.clear();
        }
        Ok(())
    }

    /// `void ProcessHorzJoins()`.
    pub fn process_horz_joins(&mut self) -> Result<()> {
        for index in 0..self.horz_join_list.len() {
            let j = *self.horz_join_list.get(index).ok_or(NULL)?;
            let (j_op1, j_op2) = (some(j.op1)?, some(j.op2)?);
            let or1 = self.get_real_outrec(self.out_pt(j_op1)?.outrec)?;
            let or2 = self.get_real_outrec(self.out_pt(j_op2)?.outrec)?;

            let op1b = self.out_pt(j_op1)?.next;
            let op2b = self.out_pt(j_op2)?.prev;
            self.op_mut(j_op1)?.next = j_op2;
            self.op_mut(j_op2)?.prev = j_op1;
            self.op_mut(op1b)?.prev = op2b;
            self.op_mut(op2b)?.next = op1b;

            if or1 == or2 {
                // 'join' is really a split
                let or1 = some(or1)?;
                // or2 = NewOutRec();
                let new_or2 = self.new_outrec()?;
                self.or_mut(new_or2)?.pts = Some(op1b);
                self.fix_outrec_pts(new_or2)?;

                // if or1->pts has moved to or2 then update or1->pts!!
                let or1_pts = some(self.outrec(or1)?.pts)?;
                if self.out_pt(or1_pts)?.outrec == Some(new_or2) {
                    self.or_mut(or1)?.pts = Some(j_op1);
                    self.op_mut(j_op1)?.outrec = Some(or1);
                }

                if self.using_polytree {
                    // #498, #520, #584, D#576, #618
                    let or1_pts = some(self.outrec(or1)?.pts)?;
                    let or2_pts = some(self.outrec(new_or2)?.pts)?;
                    if self.path2_contains_path1(or1_pts, Some(or2_pts))? {
                        // swap or1's & or2's pts
                        self.or_mut(or1)?.pts = Some(or2_pts);
                        self.or_mut(new_or2)?.pts = Some(or1_pts);
                        self.fix_outrec_pts(or1)?;
                        self.fix_outrec_pts(new_or2)?;
                        // or2 is now inside or1
                        self.or_mut(new_or2)?.owner = Some(or1);
                    } else if self.path2_contains_path1(or2_pts, Some(or1_pts))? {
                        self.or_mut(new_or2)?.owner = Some(or1);
                    } else {
                        let owner = self.outrec(or1)?.owner;
                        self.or_mut(new_or2)?.owner = owner;
                    }

                    let rec = self.or_mut(or1)?;
                    let splits = rec.splits.get_or_insert_with(Vec::new);
                    push(splits, new_or2)?;
                } else {
                    self.or_mut(new_or2)?.owner = Some(or1);
                }
            } else {
                // joining, not splitting
                let (or1, or2) = (some(or1)?, some(or2)?);
                self.or_mut(or2)?.pts = None;
                if self.using_polytree {
                    self.set_owner(or2, or1)?;
                    if self.outrec(or2)?.splits.is_some() {
                        self.move_splits(or2, or1)?; // #618
                    }
                } else {
                    self.or_mut(or2)?.owner = Some(or1);
                }
            }
        }
        Ok(())
    }

    /// `void DoIntersections(const int64_t top_y)`.
    pub fn do_intersections(&mut self, top_y: i64) -> Result<()> {
        if self.build_intersect_list(top_y)? {
            self.process_intersect_list()?;
            self.intersect_nodes.clear();
        }
        Ok(())
    }

    /// `void AddNewIntersectNode(Active& e1, Active& e2, int64_t top_y)`.
    pub fn add_new_intersect_node(&mut self, e1: ActiveId, e2: ActiveId, top_y: i64) -> Result<()> {
        let a1 = *self.active(e1)?;
        let a2 = *self.active(e2)?;
        let mut ip = Point64::default();
        if !get_line_intersect_pt(&a1.bot, &a1.top, &a2.bot, &a2.top, &mut ip)? {
            ip = Point64::xy(a1.curr_x, top_y); // parallel edges
        }

        // rounding errors can occasionally place the calculated intersection
        // point either below or above the scanbeam, so check and correct ...
        if ip.y > self.bot_y || ip.y < top_y {
            let abs_dx1 = a1.dx.abs();
            let abs_dx2 = a2.dx.abs();
            if abs_dx1 > 100.0 && abs_dx2 > 100.0 {
                if abs_dx1 > abs_dx2 {
                    ip = get_closest_point_on_segment(&ip, &a1.bot, &a1.top)?;
                } else {
                    ip = get_closest_point_on_segment(&ip, &a2.bot, &a2.top)?;
                }
            } else if abs_dx1 > 100.0 {
                ip = get_closest_point_on_segment(&ip, &a1.bot, &a1.top)?;
            } else if abs_dx2 > 100.0 {
                ip = get_closest_point_on_segment(&ip, &a2.bot, &a2.top)?;
            } else {
                if ip.y < top_y {
                    ip.y = top_y;
                } else {
                    ip.y = self.bot_y;
                }
                if abs_dx1 < abs_dx2 {
                    ip.x = top_x(&a1, ip.y)?;
                } else {
                    ip.x = top_x(&a2, ip.y)?;
                }
            }
        }
        push(
            &mut self.intersect_nodes,
            IntersectNode::new(Some(e1), Some(e2), &ip),
        )
    }

    /// `bool BuildIntersectList(const int64_t top_y)`: a stable merge sort of the SEL by
    /// `curr_x` that records every adjacent swap as an intersection.
    pub fn build_intersect_list(&mut self, top_y: i64) -> Result<bool> {
        let Some(head) = self.actives else {
            return Ok(false);
        };
        if self.active(head)?.next_in_ael.is_none() {
            return Ok(false);
        }

        // Calculate edge positions at the top of the current scanbeam, and from this
        // we will determine the intersections required to reach these new positions.
        self.adjust_curr_x_and_copy_to_sel(top_y)?;

        // Inversions are recorded once each and every pass is linear, so the whole sort
        // takes fewer than 4 (n + 2)^2 steps.
        let n = self.active_edges.len().saturating_add(2);
        let mut walk = Walk::limit(n.saturating_mul(n).saturating_mul(4));
        let mut left = self.sel;
        loop {
            match left {
                Some(l) if self.active(l)?.jump.is_some() => {}
                _ => break,
            }
            walk.step()?;
            let mut prev_base: Option<ActiveId> = None;
            loop {
                let l = match left {
                    Some(l) if self.active(l)?.jump.is_some() => l,
                    _ => break,
                };
                walk.step()?;
                let mut curr_base = l;
                let first_right = some(self.active(l)?.jump)?;
                let mut right = Some(first_right);
                let mut l_end = right;
                let r_end = self.active(first_right)?.jump;
                self.ae_mut(l)?.jump = r_end;
                let mut left_cur = Some(l);
                while left_cur != l_end && right != r_end {
                    walk.step()?;
                    let lc = some(left_cur)?;
                    let rc = some(right)?;
                    if self.active(rc)?.curr_x < self.active(lc)?.curr_x {
                        let mut tmp = some(self.active(rc)?.prev_in_sel)?;
                        loop {
                            walk.step()?;
                            self.add_new_intersect_node(tmp, rc, top_y)?;
                            if tmp == lc {
                                break;
                            }
                            tmp = some(self.active(tmp)?.prev_in_sel)?;
                        }

                        let tmp = rc;
                        right = self.extract_from_sel(tmp)?;
                        l_end = right;
                        self.insert1_before2_in_sel(tmp, lc)?;
                        if lc == curr_base {
                            curr_base = tmp;
                            self.ae_mut(curr_base)?.jump = r_end;
                            match prev_base {
                                None => self.sel = Some(curr_base),
                                Some(pb) => self.ae_mut(pb)?.jump = Some(curr_base),
                            }
                        }
                    } else {
                        left_cur = self.active(lc)?.next_in_sel;
                    }
                }
                prev_base = Some(curr_base);
                left = r_end;
            }
            left = self.sel;
        }
        Ok(!self.intersect_nodes.is_empty())
    }

    /// `void ProcessIntersectList()`.
    pub fn process_intersect_list(&mut self) -> Result<()> {
        // We now have a list of intersections required so that edges will be
        // correctly positioned at the top of the scanbeam. However, it's important
        // that edge intersections are processed from the bottom up, but it's also
        // crucial that intersections only occur between adjacent edges.

        // First we do a quicksort so intersections proceed in a bottom up order ...
        // (libstdc++'s introsort: equal points keep the order it leaves them in.)
        kicad_core::stdalgo::sort::<_, _, ::core::convert::Infallible>(
            &mut self.intersect_nodes,
            &mut intersect_list_sort,
        )
        .map_err(|_| BAD_ORDERING)?;
        // Now as we process these intersections, we must sometimes adjust the order
        // to ensure that intersecting edges are always adjacent ...
        for i in 0..self.intersect_nodes.len() {
            let node = *self.intersect_nodes.get(i).ok_or(NODE_RANGE)?;
            if !self.edges_adjacent_in_ael(&node)? {
                let mut i2 = i + 1;
                loop {
                    let node2 = self.intersect_nodes.get(i2).ok_or(NODE_RANGE)?;
                    if self.edges_adjacent_in_ael(node2)? {
                        break;
                    }
                    i2 += 1;
                }
                self.intersect_nodes.swap(i, i2);
            }

            let node = *self.intersect_nodes.get(i).ok_or(NODE_RANGE)?;
            let (e1, e2) = (some(node.edge1)?, some(node.edge2)?);
            self.intersect_edges(e1, e2, &node.pt)?;
            self.swap_positions_in_ael(e1, e2)?;

            self.ae_mut(e1)?.curr_x = node.pt.x;
            self.ae_mut(e2)?.curr_x = node.pt.x;
            self.check_join_left(e2, &node.pt, true)?;
            self.check_join_right(e1, &node.pt, true)?;
        }
        Ok(())
    }

    /// `void SwapPositionsInAEL(Active& e1, Active& e2)`: "preconditon: e1 must be
    /// immediately to the left of e2".
    pub fn swap_positions_in_ael(&mut self, e1: ActiveId, e2: ActiveId) -> Result<()> {
        let next = self.active(e2)?.next_in_ael;
        if let Some(n) = next {
            self.ae_mut(n)?.prev_in_ael = Some(e1);
        }
        let prev = self.active(e1)?.prev_in_ael;
        if let Some(p) = prev {
            self.ae_mut(p)?.next_in_ael = Some(e2);
        }
        let b = self.ae_mut(e2)?;
        b.prev_in_ael = prev;
        b.next_in_ael = Some(e1);
        let a = self.ae_mut(e1)?;
        a.prev_in_ael = Some(e2);
        a.next_in_ael = next;
        if self.active(e2)?.prev_in_ael.is_none() {
            self.actives = Some(e2);
        }
        Ok(())
    }

    /// `inline OutPt* GetLastOp(const Active& hot_edge)`.
    pub fn get_last_op(&self, hot_edge: ActiveId) -> Result<OutPtId> {
        let outrec = some(self.active(hot_edge)?.outrec)?;
        let rec = self.outrec(outrec)?;
        let mut result = some(rec.pts)?;
        if Some(hot_edge) != rec.front_edge {
            result = self.out_pt(result)?.next;
        }
        Ok(result)
    }

    /// `void AddTrialHorzJoin(OutPt* op)`.
    pub fn add_trial_horz_join(&mut self, op: OutPtId) -> Result<()> {
        let outrec = some(self.out_pt(op)?.outrec)?;
        if self.outrec(outrec)?.is_open {
            return Ok(());
        }
        push(&mut self.horz_seg_list, HorzSegment::new(Some(op)))
    }

    /// `bool ResetHorzDirection(const Active& horz, const Vertex* max_vertex,
    /// int64_t& horz_left, int64_t& horz_right)`.
    pub fn reset_horz_direction(
        &self,
        horz: ActiveId,
        max_vertex: Option<VertexId>,
        horz_left: &mut i64,
        horz_right: &mut i64,
    ) -> Result<bool> {
        let h = self.active(horz)?;
        if h.bot.x == h.top.x {
            // the horizontal edge is going nowhere ...
            *horz_left = h.curr_x;
            *horz_right = h.curr_x;
            let mut e = h.next_in_ael;
            let mut walk = self.ael_walk();
            while let Some(x) = e {
                if self.active(x)?.vertex_top == max_vertex {
                    break;
                }
                walk.step()?;
                e = self.active(x)?.next_in_ael;
            }
            Ok(e.is_some())
        } else if h.curr_x < h.top.x {
            *horz_left = h.curr_x;
            *horz_right = h.top.x;
            Ok(true)
        } else {
            *horz_left = h.top.x;
            *horz_right = h.curr_x;
            Ok(false) // right to left
        }
    }

    /// The number of vertices in every vertex list: a bound on walks along bounds.
    fn vertex_total(&self) -> usize {
        self.vertex_lists
            .iter()
            .fold(0usize, |n, s| n.saturating_add(s.chunk.len()))
    }

    /// `void DoHorizontal(Active& horz)`.
    ///
    /// "Horizontal edges (HEs) at scanline intersections (ie at the top or bottom of a
    /// scanbeam) are processed as if layered. The order in which HEs are processed doesn't
    /// matter. HEs intersect with the bottom vertices of other HEs and with non-horizontal
    /// edges. Once these intersections are completed, intermediate HEs are 'promoted' to the
    /// next edge in their bounds, and they in turn may be intersected by other HEs."
    pub fn do_horizontal(&mut self, horz: ActiveId) -> Result<()> {
        let horz_is_open = self.is_open(horz)?;
        let y = self.active(horz)?.bot.y;
        let vertex_max = if horz_is_open {
            self.get_curr_y_maxima_vertex_open(horz)?
        } else {
            self.get_curr_y_maxima_vertex(horz)?
        };

        // Both are assigned by ResetHorzDirection before any read.
        let (mut horz_left, mut horz_right) = (0i64, 0i64);
        let mut is_left_to_right =
            self.reset_horz_direction(horz, vertex_max, &mut horz_left, &mut horz_right)?;

        if is_hot_edge(self.active(horz)?) {
            let h = *self.active(horz)?;
            let op = self.add_out_pt(
                horz,
                &Point64 {
                    x: h.curr_x,
                    y,
                    z: h.bot.z,
                },
            )?;
            self.add_trial_horz_join(op)?;
        }

        let mut bound_walk = Walk::new(self.vertex_total());
        loop {
            // loop through consec. horizontal edges
            bound_walk.step()?;
            let mut e = if is_left_to_right {
                self.active(horz)?.next_in_ael
            } else {
                self.active(horz)?.prev_in_ael
            };

            let mut walk = self.ael_walk();
            while let Some(ex) = e {
                walk.step()?;
                if self.active(ex)?.vertex_top == vertex_max {
                    if is_hot_edge(self.active(horz)?) && is_joined(self.active(ex)?) {
                        let top = self.active(ex)?.top;
                        self.split(ex, &top)?;
                    }

                    if is_hot_edge(self.active(horz)?) {
                        let mut climb = Walk::new(self.vertex_total());
                        while self.active(horz)?.vertex_top != vertex_max {
                            climb.step()?;
                            let top = self.active(horz)?.top;
                            self.add_out_pt(horz, &top)?;
                            self.update_edge_into_ael(horz)?;
                        }
                        let top = self.active(horz)?.top;
                        if is_left_to_right {
                            self.add_local_max_poly(horz, ex, &top)?;
                        } else {
                            self.add_local_max_poly(ex, horz, &top)?;
                        }
                    }
                    self.delete_from_ael(ex)?;
                    self.delete_from_ael(horz)?;
                    return Ok(());
                }

                // if horzEdge is a maxima, keep going until we reach
                // its maxima pair, otherwise check for break conditions
                if vertex_max != self.active(horz)?.vertex_top || self.is_open_end(horz)? {
                    // otherwise stop when 'ae' is beyond the end of the horizontal line
                    let ea = *self.active(ex)?;
                    if (is_left_to_right && ea.curr_x > horz_right)
                        || (!is_left_to_right && ea.curr_x < horz_left)
                    {
                        break;
                    }

                    if ea.curr_x == self.active(horz)?.top.x && !is_horizontal(&ea) {
                        let pt = self.vertex(some(self.next_vertex(horz)?)?)?.pt;
                        let open_other = self.is_open(ex)?
                            && !self.is_same_poly_type(ex, horz)?
                            && !is_hot_edge(&ea);
                        if is_left_to_right {
                            // with open paths we'll only break once past horz's end
                            if open_other {
                                if top_x(&ea, pt.y)? > pt.x {
                                    break;
                                }
                            }
                            // otherwise we'll only break when horz's outslope is greater
                            // than e's
                            else if top_x(&ea, pt.y)? >= pt.x {
                                break;
                            }
                        } else if open_other {
                            if top_x(&ea, pt.y)? < pt.x {
                                break;
                            }
                        } else if top_x(&ea, pt.y)? <= pt.x {
                            break;
                        }
                    }
                }

                let pt = Point64::xy(self.active(ex)?.curr_x, self.active(horz)?.bot.y);
                if is_left_to_right {
                    self.intersect_edges(horz, ex, &pt)?;
                    self.swap_positions_in_ael(horz, ex)?;
                    self.check_join_left(ex, &pt, false)?;
                    let curr_x = self.active(ex)?.curr_x;
                    self.ae_mut(horz)?.curr_x = curr_x;
                    e = self.active(horz)?.next_in_ael;
                } else {
                    self.intersect_edges(ex, horz, &pt)?;
                    self.swap_positions_in_ael(ex, horz)?;
                    self.check_join_right(ex, &pt, false)?;
                    let curr_x = self.active(ex)?.curr_x;
                    self.ae_mut(horz)?.curr_x = curr_x;
                    e = self.active(horz)?.prev_in_ael;
                }

                if self.active(horz)?.outrec.is_some() {
                    // nb: The outrec containing the op returned by IntersectEdges
                    // above may no longer be associated with horzEdge.
                    let op = self.get_last_op(horz)?;
                    self.add_trial_horz_join(op)?;
                }
            }

            // check if we've finished with (consecutive) horizontals ...
            if horz_is_open && self.is_open_end(horz)? {
                // ie open at top
                if is_hot_edge(self.active(horz)?) {
                    let top = self.active(horz)?.top;
                    self.add_out_pt(horz, &top)?;
                    let outrec = self.outrec_of(horz)?;
                    if self.is_front(horz)? {
                        self.or_mut(outrec)?.front_edge = None;
                    } else {
                        self.or_mut(outrec)?.back_edge = None;
                    }
                    self.ae_mut(horz)?.outrec = None;
                }
                self.delete_from_ael(horz)?;
                return Ok(());
            } else if self.vertex(some(self.next_vertex(horz)?)?)?.pt.y != self.active(horz)?.top.y
            {
                break;
            }

            // still more horizontals in bound to process ...
            if is_hot_edge(self.active(horz)?) {
                let top = self.active(horz)?.top;
                self.add_out_pt(horz, &top)?;
            }
            self.update_edge_into_ael(horz)?;

            is_left_to_right =
                self.reset_horz_direction(horz, vertex_max, &mut horz_left, &mut horz_right)?;
        }

        if is_hot_edge(self.active(horz)?) {
            let top = self.active(horz)?.top;
            let op = self.add_out_pt(horz, &top)?;
            self.add_trial_horz_join(op)?;
        }

        self.update_edge_into_ael(horz) // end of an intermediate horiz.
    }

    /// `void DoTopOfScanbeam(const int64_t y)`.
    pub fn do_top_of_scanbeam(&mut self, y: i64) -> Result<()> {
        self.sel = None; // sel_ is reused to flag horizontals (see PushHorz below)
        let mut e = self.actives;
        let mut walk = self.ael_walk();
        while let Some(x) = e {
            walk.step()?;
            // nb: 'e' will never be horizontal here
            let a = *self.active(x)?;
            if a.top.y == y {
                self.ae_mut(x)?.curr_x = a.top.x;
                if self.is_maxima(x)? {
                    e = self.do_maxima(x)?; // TOP OF BOUND (MAXIMA)
                    continue;
                }
                // INTERMEDIATE VERTEX ...
                if is_hot_edge(&a) {
                    self.add_out_pt(x, &a.top)?;
                }
                self.update_edge_into_ael(x)?;
                if is_horizontal(self.active(x)?) {
                    self.push_horz(x)?; // horizontals are processed later
                }
            } else {
                // i.e. not the top of the edge
                let curr_x = top_x(&a, y)?;
                self.ae_mut(x)?.curr_x = curr_x;
            }
            e = self.active(x)?.next_in_ael;
        }
        Ok(())
    }

    /// `Active* DoMaxima(Active& e)`.
    pub fn do_maxima(&mut self, e: ActiveId) -> Result<Option<ActiveId>> {
        let prev_e = self.active(e)?.prev_in_ael;
        let mut next_e = self.active(e)?.next_in_ael;
        if self.is_open_end(e)? {
            if is_hot_edge(self.active(e)?) {
                let top = self.active(e)?.top;
                self.add_out_pt(e, &top)?;
            }
            if !is_horizontal(self.active(e)?) {
                if is_hot_edge(self.active(e)?) {
                    let outrec = self.outrec_of(e)?;
                    if self.is_front(e)? {
                        self.or_mut(outrec)?.front_edge = None;
                    } else {
                        self.or_mut(outrec)?.back_edge = None;
                    }
                    self.ae_mut(e)?.outrec = None;
                }
                self.delete_from_ael(e)?;
            }
            return Ok(next_e);
        }

        let Some(max_pair) = self.get_maxima_pair(e)? else {
            return Ok(next_e); // eMaxPair is horizontal
        };

        if is_joined(self.active(e)?) {
            let top = self.active(e)?.top;
            self.split(e, &top)?;
        }
        if is_joined(self.active(max_pair)?) {
            let top = self.active(max_pair)?.top;
            self.split(max_pair, &top)?;
        }

        // only non-horizontal maxima here.
        // process any edges between maxima pair ...
        let mut walk = self.ael_walk();
        while next_e != Some(max_pair) {
            walk.step()?;
            let n = some(next_e)?;
            let top = self.active(e)?.top;
            self.intersect_edges(e, n, &top)?;
            self.swap_positions_in_ael(e, n)?;
            next_e = self.active(e)?.next_in_ael;
        }

        if self.is_open(e)? {
            if is_hot_edge(self.active(e)?) {
                let top = self.active(e)?.top;
                self.add_local_max_poly(e, max_pair, &top)?;
            }
            self.delete_from_ael(max_pair)?;
            self.delete_from_ael(e)?;
            return match prev_e {
                Some(p) => Ok(self.active(p)?.next_in_ael),
                None => Ok(self.actives),
            };
        }

        // e.next_in_ael== max_pair ...
        if is_hot_edge(self.active(e)?) {
            let top = self.active(e)?.top;
            self.add_local_max_poly(e, max_pair, &top)?;
        }

        self.delete_from_ael(e)?;
        self.delete_from_ael(max_pair)?;
        match prev_e {
            Some(p) => Ok(self.active(p)?.next_in_ael),
            None => Ok(self.actives),
        }
    }

    /// `void Split(Active& e, const Point64& pt)`.
    pub fn split(&mut self, e: ActiveId, pt: &Point64) -> Result<()> {
        if self.active(e)?.join_with == JoinWith::Right {
            self.ae_mut(e)?.join_with = JoinWith::NoJoin;
            let next = some(self.active(e)?.next_in_ael)?;
            self.ae_mut(next)?.join_with = JoinWith::NoJoin;
            self.add_local_min_poly(e, next, pt, true)?;
        } else {
            self.ae_mut(e)?.join_with = JoinWith::NoJoin;
            let prev = some(self.active(e)?.prev_in_ael)?;
            self.ae_mut(prev)?.join_with = JoinWith::NoJoin;
            self.add_local_min_poly(prev, e, pt, true)?;
        }
        Ok(())
    }

    /// `void CheckJoinLeft(Active& e, const Point64& pt, bool check_curr_x = false)`.
    pub fn check_join_left(&mut self, e: ActiveId, pt: &Point64, check_curr_x: bool) -> Result<()> {
        let a = *self.active(e)?;
        let Some(prev) = a.prev_in_ael else {
            return Ok(());
        };
        let p = *self.active(prev)?;
        if !is_hot_edge(&a)
            || !is_hot_edge(&p)
            || is_horizontal(&a)
            || is_horizontal(&p)
            || self.is_open(e)?
            || self.is_open(prev)?
        {
            return Ok(());
        }
        if (pt.y < add64(a.top.y, 2)? || pt.y < add64(p.top.y, 2)?)
            && (a.bot.y > pt.y || p.bot.y > pt.y)
        {
            return Ok(()); // avoid trivial joins
        }

        if check_curr_x {
            if perpendic_dist_from_line_sqrd(pt, &p.bot, &p.top)? > 0.25 {
                return Ok(());
            }
        } else if a.curr_x != p.curr_x {
            return Ok(());
        }
        if !is_collinear(&a.top, pt, &p.top)? {
            return Ok(());
        }

        let e_idx = self.outrec(some(a.outrec)?)?.idx;
        let prev_idx = self.outrec(some(p.outrec)?)?.idx;
        if e_idx == prev_idx {
            self.add_local_max_poly(prev, e, pt)?;
        } else if e_idx < prev_idx {
            self.join_outrec_paths(e, prev)?;
        } else {
            self.join_outrec_paths(prev, e)?;
        }
        self.ae_mut(prev)?.join_with = JoinWith::Right;
        self.ae_mut(e)?.join_with = JoinWith::Left;
        Ok(())
    }

    /// `void CheckJoinRight(Active& e, const Point64& pt, bool check_curr_x = false)`.
    pub fn check_join_right(
        &mut self,
        e: ActiveId,
        pt: &Point64,
        check_curr_x: bool,
    ) -> Result<()> {
        let a = *self.active(e)?;
        let Some(next) = a.next_in_ael else {
            return Ok(());
        };
        let n = *self.active(next)?;
        if !is_hot_edge(&a)
            || !is_hot_edge(&n)
            || is_horizontal(&a)
            || is_horizontal(&n)
            || self.is_open(e)?
            || self.is_open(next)?
        {
            return Ok(());
        }
        if (pt.y < add64(a.top.y, 2)? || pt.y < add64(n.top.y, 2)?)
            && (a.bot.y > pt.y || n.bot.y > pt.y)
        {
            return Ok(()); // avoid trivial joins
        }

        if check_curr_x {
            if perpendic_dist_from_line_sqrd(pt, &n.bot, &n.top)? > 0.35 {
                return Ok(());
            }
        } else if a.curr_x != n.curr_x {
            return Ok(());
        }
        if !is_collinear(&a.top, pt, &n.top)? {
            return Ok(());
        }

        let e_idx = self.outrec(some(a.outrec)?)?.idx;
        let next_idx = self.outrec(some(n.outrec)?)?.idx;
        if e_idx == next_idx {
            self.add_local_max_poly(e, next, pt)?;
        } else if e_idx < next_idx {
            self.join_outrec_paths(e, next)?;
        } else {
            self.join_outrec_paths(next, e)?;
        }

        self.ae_mut(e)?.join_with = JoinWith::Right;
        self.ae_mut(next)?.join_with = JoinWith::Left;
        Ok(())
    }

    /// `inline bool GetHorzExtendedHorzSeg(OutPt*& op, OutPt*& op2)`: returns the result and
    /// the updated `op` and `op2`. (The reference engine never calls it.)
    pub fn get_horz_extended_horz_seg(&self, op: OutPtId) -> Result<(bool, OutPtId, OutPtId)> {
        let outrec = some(self.get_real_outrec(self.out_pt(op)?.outrec)?)?;
        let rec = self.outrec(outrec)?;
        let mut op = op;
        let mut op2 = op;
        let mut walk = self.ring_walk();
        let pt = |id: OutPtId| -> Result<Point64> { Ok(self.out_pt(id)?.pt) };
        if rec.front_edge.is_some() {
            let pts = rec.pts;
            while Some(self.out_pt(op)?.prev) != pts && pt(self.out_pt(op)?.prev)?.y == pt(op)?.y {
                walk.step()?;
                op = self.out_pt(op)?.prev;
            }
            while Some(op2) != pts && pt(self.out_pt(op2)?.next)?.y == pt(op2)?.y {
                walk.step()?;
                op2 = self.out_pt(op2)?.next;
            }
            Ok((op2 != op, op, op2))
        } else {
            while self.out_pt(op)?.prev != op2 && pt(self.out_pt(op)?.prev)?.y == pt(op)?.y {
                walk.step()?;
                op = self.out_pt(op)?.prev;
            }
            while self.out_pt(op2)?.next != op && pt(self.out_pt(op2)?.next)?.y == pt(op2)?.y {
                walk.step()?;
                op2 = self.out_pt(op2)?.next;
            }
            Ok((op2 != op && self.out_pt(op2)?.next != op, op, op2))
        }
    }

    /// `bool BuildPath64(OutPt* op, bool reverse, bool isOpen, Path64& path)`. A `false`
    /// result before `path.resize(0)` leaves `path` unchanged.
    pub fn build_path64(
        &self,
        op: Option<OutPtId>,
        reverse: bool,
        is_open: bool,
        path: &mut Path64,
    ) -> Result<bool> {
        let Some(mut op) = op else {
            return Ok(false);
        };
        let o = *self.out_pt(op)?;
        if o.next == op || (!is_open && o.next == o.prev) {
            return Ok(false);
        }

        path.clear();
        let mut last_pt;
        let mut op2;
        if reverse {
            last_pt = o.pt;
            op2 = o.prev;
        } else {
            op = o.next;
            last_pt = self.out_pt(op)?.pt;
            op2 = self.out_pt(op)?.next;
        }
        push(path, last_pt)?;

        let mut walk = self.ring_walk();
        while op2 != op {
            walk.step()?;
            let o2 = *self.out_pt(op2)?;
            if o2.pt != last_pt {
                last_pt = o2.pt;
                push(path, last_pt)?;
            }
            op2 = if reverse { o2.prev } else { o2.next };
        }

        if !is_open && path.len() == 3 && self.is_very_small_triangle(op2)? {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    /// `bool BuildPathD(OutPt* op, bool reverse, bool isOpen, PathD& path, double inv_scale)`:
    /// `PointD(lastPt.x * inv_scale, lastPt.y * inv_scale, lastPt.z)`. Unlike `BuildPath64`
    /// the closing small-triangle test applies to open paths too.
    pub fn build_path_d(
        &self,
        op: Option<OutPtId>,
        reverse: bool,
        is_open: bool,
        path: &mut PathD,
        inv_scale: f64,
    ) -> Result<bool> {
        let Some(mut op) = op else {
            return Ok(false);
        };
        let o = *self.out_pt(op)?;
        if o.next == op || (!is_open && o.next == o.prev) {
            return Ok(false);
        }

        path.clear();
        let mut last_pt;
        let mut op2;
        if reverse {
            last_pt = o.pt;
            op2 = o.prev;
        } else {
            op = o.next;
            last_pt = self.out_pt(op)?.pt;
            op2 = self.out_pt(op)?.next;
        }
        let scaled = |p: &Point64| -> Result<PointD> {
            PointD::new(p.x as f64 * inv_scale, p.y as f64 * inv_scale, p.z)
        };
        push(path, scaled(&last_pt)?)?;

        let mut walk = self.ring_walk();
        while op2 != op {
            walk.step()?;
            let o2 = *self.out_pt(op2)?;
            if o2.pt != last_pt {
                last_pt = o2.pt;
                push(path, scaled(&last_pt)?)?;
            }
            op2 = if reverse { o2.prev } else { o2.next };
        }
        if path.len() == 3 && self.is_very_small_triangle(op2)? {
            return Ok(false);
        }
        Ok(true)
    }

    /// `bool CheckBounds(OutRec* outrec)`.
    pub fn check_bounds(&mut self, outrec: OutRecId) -> Result<bool> {
        if self.outrec(outrec)?.pts.is_none() {
            return Ok(false);
        }
        if !self.outrec(outrec)?.bounds.is_empty() {
            return Ok(true);
        }
        self.clean_collinear(Some(outrec))?;
        let Some(pts) = self.outrec(outrec)?.pts else {
            return Ok(false);
        };
        let mut path = ::core::mem::take(&mut self.or_mut(outrec)?.path);
        let built = self.build_path64(Some(pts), self.reverse_solution, false, &mut path);
        self.or_mut(outrec)?.path = path;
        if !built? {
            return Ok(false);
        }
        let bounds = get_bounds_path::<i64, i64>(&self.outrec(outrec)?.path)?;
        self.or_mut(outrec)?.bounds = bounds;
        Ok(true)
    }

    /// `bool CheckSplitOwner(OutRec* outrec, OutRecList* splits)` with `splits` the list of
    /// record `splits_of`. "nb: use indexing (not an iterator) in case 'splits' is modified
    /// inside this loop (#1029)": the list is re-read at every step.
    pub fn check_split_owner(&mut self, outrec: OutRecId, splits_of: OutRecId) -> Result<bool> {
        let mut stack: Vec<SplitFrame> = Vec::new();
        push(
            &mut stack,
            SplitFrame {
                list: splits_of,
                idx: 0,
                stage: SplitStage::Element,
                split: splits_of,
            },
        )?;
        while let Some(&frame) = stack.last() {
            let mut f = frame;
            match f.stage {
                SplitStage::Element => {
                    let list = some(self.outrec(f.list)?.splits.as_ref())?;
                    let Some(&split) = list.get(f.idx) else {
                        // This (recursive) call returns false; its caller resumes.
                        stack.pop();
                        continue;
                    };
                    f.split = split;
                    f.stage = SplitStage::Real;
                    let rec = self.outrec(split)?;
                    let recurse = rec.pts.is_none() && rec.splits.is_some();
                    *stack.last_mut().ok_or(NULL)? = f;
                    if recurse {
                        // #942
                        self.push_split_frame(&mut stack, split)?;
                    }
                }
                SplitStage::Real => {
                    let real = self.get_real_outrec(Some(f.split))?;
                    let skip = match real {
                        None => true,
                        Some(s) => s == outrec || self.outrec(s)?.recursive_split == Some(outrec),
                    };
                    if skip {
                        f.idx += 1;
                        f.stage = SplitStage::Element;
                        *stack.last_mut().ok_or(NULL)? = f;
                        continue;
                    }
                    let split = some(real)?;
                    self.or_mut(split)?.recursive_split = Some(outrec); // prevent infinite loops
                    f.split = split;
                    f.stage = SplitStage::Contains;
                    *stack.last_mut().ok_or(NULL)? = f;
                    if self.outrec(split)?.splits.is_some() {
                        self.push_split_frame(&mut stack, split)?;
                    }
                }
                SplitStage::Contains => {
                    let split = f.split;
                    let contains = self.check_bounds(split)?
                        && {
                            let bounds = self.outrec(outrec)?.bounds;
                            self.outrec(split)?.bounds.contains_rect(&bounds)
                        }
                        && {
                            let pts = some(self.outrec(outrec)?.pts)?;
                            self.path2_contains_path1_indexed(pts, split, self.pip_index_threshold)?
                        };
                    if !contains {
                        f.idx += 1;
                        f.stage = SplitStage::Element;
                        *stack.last_mut().ok_or(NULL)? = f;
                        continue;
                    }

                    if !self.is_valid_owner(Some(outrec), Some(split))? {
                        // split is owned by outrec! (#957)
                        let owner = self.outrec(outrec)?.owner;
                        self.or_mut(split)?.owner = owner;
                    }

                    self.or_mut(outrec)?.owner = Some(split);
                    // Every enclosing call returns true at once.
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Enter `CheckSplitOwner(outrec, list->splits)`. A deeper chain than twice the records
    /// can only be a cycle, on which the reference recurses without end.
    fn push_split_frame(&self, stack: &mut Vec<SplitFrame>, list: OutRecId) -> Result<()> {
        let limit = self.outrec_list.len().saturating_add(2).saturating_mul(2);
        if stack.len() >= limit {
            return Err(ENDLESS);
        }
        push(
            stack,
            SplitFrame {
                list,
                idx: 0,
                stage: SplitStage::Element,
                split: list,
            },
        )
    }

    /// `void RecursiveCheckOwners(OutRec* outrec, PolyPath* polypath)`: "pre-condition:
    /// outrec will have valid bounds; post-condition: if a valid path, outrec will have a
    /// polypath". `polypath` is node `root` of `tree`.
    pub fn recursive_check_owners<S: PolyTreeSink>(
        &mut self,
        outrec: OutRecId,
        tree: &mut S,
        root: PolyPathId,
    ) -> Result<()> {
        // (record, resuming after the recursive call on its owner)
        let mut stack: Vec<(OutRecId, bool)> = Vec::new();
        push(&mut stack, (outrec, false))?;
        while let Some(&(o, resumed)) = stack.last() {
            if !resumed {
                let rec = self.outrec(o)?;
                if rec.polypath.is_some() || rec.bounds.is_empty() {
                    stack.pop();
                    continue;
                }
                let mut walk = self.owner_walk();
                while let Some(owner) = self.outrec(o)?.owner {
                    walk.step()?;
                    if self.outrec(owner)?.splits.is_some() && self.check_split_owner(o, owner)? {
                        break;
                    }
                    let owner = some(self.outrec(o)?.owner)?;
                    if self.outrec(owner)?.pts.is_some()
                        && self.check_bounds(owner)?
                        && {
                            let bounds = self.outrec(o)?.bounds;
                            self.outrec(owner)?.bounds.contains_rect(&bounds)
                        }
                        && {
                            let pts = some(self.outrec(o)?.pts)?;
                            self.path2_contains_path1_indexed(pts, owner, self.pip_index_threshold)?
                        }
                    {
                        break;
                    }
                    let owner = some(self.outrec(o)?.owner)?;
                    let next = self.outrec(owner)?.owner;
                    self.or_mut(o)?.owner = next;
                }

                if let Some(owner) = self.outrec(o)?.owner {
                    if self.outrec(owner)?.polypath.is_none() {
                        *stack.last_mut().ok_or(NULL)? = (o, true);
                        if stack.len() > self.outrec_list.len().saturating_add(2) {
                            return Err(ENDLESS);
                        }
                        push(&mut stack, (owner, false))?;
                        continue;
                    }
                } else {
                    let child = tree.add_child64(root, &self.outrec(o)?.path)?;
                    self.or_mut(o)?.polypath = Some(child);
                    stack.pop();
                    continue;
                }
            }
            let owner = some(self.outrec(o)?.owner)?;
            let parent = some(self.outrec(owner)?.polypath)?;
            let child = tree.add_child64(parent, &self.outrec(o)?.path)?;
            self.or_mut(o)?.polypath = Some(child);
            stack.pop();
        }
        Ok(())
    }

    /// `void Clipper64::BuildPaths64(Paths64& solutionClosed, Paths64* solutionOpen)`.
    pub fn build_paths64(
        &mut self,
        solution_closed: &mut Paths64,
        mut solution_open: Option<&mut Paths64>,
    ) -> Result<()> {
        solution_closed.clear();
        reserve(solution_closed, self.outrec_list.len())?;
        if let Some(open) = solution_open.as_deref_mut() {
            open.clear();
            reserve(open, self.outrec_list.len())?;
        }

        // nb: outrec_list_.size() may change in the following
        // while loop because polygons may be split during
        // calls to CleanCollinear which calls FixSelfIntersects
        let mut i = 0usize;
        while i < self.outrec_list.len() {
            let outrec = OutRecId(i);
            i += 1;
            let rec = self.outrec(outrec)?;
            let Some(pts) = rec.pts else { continue };

            let mut path = Path64::new();
            match solution_open.as_deref_mut() {
                Some(open) if rec.is_open => {
                    if self.build_path64(Some(pts), self.reverse_solution, true, &mut path)? {
                        push(open, path)?;
                    }
                }
                _ => {
                    // nb: CleanCollinear can add to outrec_list_
                    self.clean_collinear(Some(outrec))?;
                    // closed paths should always return a Positive orientation
                    let pts = self.outrec(outrec)?.pts;
                    if self.build_path64(pts, self.reverse_solution, false, &mut path)? {
                        push(solution_closed, path)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// `void Clipper64::BuildTree64(PolyPath64& polytree, Paths64& open_paths)`.
    pub fn build_tree64(
        &mut self,
        polytree: &mut PolyTree64,
        open_paths: &mut Paths64,
    ) -> Result<()> {
        polytree.clear(PolyTree64::ROOT)?;
        open_paths.clear();
        if self.has_open_paths {
            reserve(open_paths, self.outrec_list.len())?;
        }

        // outrec_list_.size() is not static here because
        // CheckBounds below can indirectly add additional
        // OutRec (via FixOutRecPts & CleanCollinear)
        let mut i = 0usize;
        while i < self.outrec_list.len() {
            let outrec = OutRecId(i);
            i += 1;
            let rec = self.outrec(outrec)?;
            let Some(pts) = rec.pts else { continue };

            if rec.is_open {
                let mut path = Path64::new();
                if self.build_path64(Some(pts), self.reverse_solution, true, &mut path)? {
                    push(open_paths, path)?;
                }
                continue;
            }

            if self.check_bounds(outrec)? {
                self.recursive_check_owners(outrec, polytree, PolyTree64::ROOT)?;
            }
        }
        Ok(())
    }

    /// `void ClipperD::BuildPathsD(PathsD& solutionClosed, PathsD* solutionOpen)`.
    pub fn build_paths_d(
        &mut self,
        solution_closed: &mut PathsD,
        mut solution_open: Option<&mut PathsD>,
        inv_scale: f64,
    ) -> Result<()> {
        solution_closed.clear();
        reserve(solution_closed, self.outrec_list.len())?;
        if let Some(open) = solution_open.as_deref_mut() {
            open.clear();
            reserve(open, self.outrec_list.len())?;
        }

        // outrec_list_.size() is not static here because
        // CleanCollinear below can indirectly add additional
        // OutRec (via FixOutRecPts)
        let mut i = 0usize;
        while i < self.outrec_list.len() {
            let outrec = OutRecId(i);
            i += 1;
            let rec = self.outrec(outrec)?;
            let Some(pts) = rec.pts else { continue };

            let mut path = PathD::new();
            match solution_open.as_deref_mut() {
                Some(open) if rec.is_open => {
                    if self.build_path_d(
                        Some(pts),
                        self.reverse_solution,
                        true,
                        &mut path,
                        inv_scale,
                    )? {
                        push(open, path)?;
                    }
                }
                _ => {
                    self.clean_collinear(Some(outrec))?;
                    // closed paths should always return a Positive orientation
                    let pts = self.outrec(outrec)?.pts;
                    if self.build_path_d(pts, self.reverse_solution, false, &mut path, inv_scale)? {
                        push(solution_closed, path)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// `void ClipperD::BuildTreeD(PolyPathD& polytree, PathsD& open_paths)`.
    pub fn build_tree_d(
        &mut self,
        polytree: &mut PolyTreeD,
        open_paths: &mut PathsD,
        inv_scale: f64,
    ) -> Result<()> {
        polytree.clear(PolyTreeD::ROOT)?;
        open_paths.clear();
        if self.has_open_paths {
            reserve(open_paths, self.outrec_list.len())?;
        }

        // outrec_list_.size() is not static here because
        // BuildPathD below can indirectly add additional OutRec //#607
        let mut i = 0usize;
        while i < self.outrec_list.len() {
            let outrec = OutRecId(i);
            i += 1;
            let rec = self.outrec(outrec)?;
            let Some(pts) = rec.pts else { continue };
            if rec.is_open {
                let mut path = PathD::new();
                if self.build_path_d(
                    Some(pts),
                    self.reverse_solution,
                    true,
                    &mut path,
                    inv_scale,
                )? {
                    push(open_paths, path)?;
                }
                continue;
            }

            if self.check_bounds(outrec)? {
                self.recursive_check_owners(outrec, polytree, PolyTreeD::ROOT)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
