// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 (Copyright Angus Johnson 2010-2025), Boost Software
// License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! Faithful port of `thirdparty/clipper2/Clipper2Lib/src/clipper.rectclip.cpp` (lines 1-1027)
//! at KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7, unit [`UNIT`].
//!
//! Every body in that file is ported: the file-local helpers (`Path1ContainsPath2`,
//! `GetLocation`, `IsHorizontal`, `GetSegmentIntersection`, `GetIntersection`,
//! `GetAdjacentLocation`, `HeadingClockwise`, `AreOpposites`, `IsClockwise`, `UnlinkOp`,
//! `UnlinkOpBack`, `GetEdgesForPt`, `IsHeadingClockwise`, `HasHorzOverlap`, `HasVertOverlap`,
//! `AddToEdge`, `UncoupleEdge`, `SetNewOwner`, `StartLocsAreClockwise`), the `RectClip64`
//! members (`Add`, both `AddCorner` overloads, `GetNextLocation`, `ExecuteInternal`,
//! `CheckEdges`, `TidyEdges`, `GetPath`, `Execute`) and the `RectClipLines64` members
//! (`Execute`, `ExecuteInternal`, `GetPath`).
//!
//! # Minimum co-port of `clipper.rectclip.h`
//!
//! The class layouts, `OutPt2`, `enum class Location` and the two inline constructors are
//! written in `clipper.rectclip.h`, owned by
//! `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`. The bodies here
//! cannot exist without them, so this module carries the minimum co-port: [`Location`] with
//! its declared order, the private `OutPt2` record and the constructors
//! [`RectClip64::new`]/[`RectClipLines64::new`] (`rect_(rect)`, `rect_as_path_(rect.AsPath())`,
//! `rect_mp_(rect.MidPoint())` in member order). The header unit's `RectClip`/`RectClipLines`
//! wrappers in `clipper.h` call exactly these two constructors and `Execute` members.
//!
//! # Pointers and storage
//!
//! `OutPt2*` becomes an index into a per-object arena (`op_container_`, a `std::deque` whose
//! element addresses are stable, so indices are equivalent); `OutPt2List*` (always one of
//! `edges_[0..8]`) becomes the edge-list index; `nullptr` is `None`. Unlinked nodes keep their
//! stale `next`/`prev` links exactly as in the C++.
//!
//! # Undefined behaviour and resources
//!
//! Arithmetic goes through the accepted core port, which refuses signed overflow
//! (`CrossProductSign`, `IsCollinear`, `Rect64::MidPoint`) and out-of-range `double` to
//! `int64_t` conversion (`GetLineIntersectPt`) with [`ClipperError::Undefined`]. The only
//! other undefined expression in this file is `rect_as_path_[static_cast<size_t>(loc)]` with
//! `loc == Location::Inside` (index 4 of a 4-point vector) in `AddCorner`, refused at that
//! subscript. Every other subscript and pointer is checked as well and refused the same way,
//! though the reference's invariants keep them in range.
//!
//! The C++ loops are not all bounded by their input (a corner walk towards
//! `Location::Inside`, or `TidyEdges` revisiting the same slots, would never end). The port
//! bounds the work of one `Execute` call by a host [`RectClipBudget`]: arena nodes,
//! recorded start locations and loop iterations. Exceeding it is
//! [`ClipperError::ResourceLimit`] and leaves no result; within the budget results are
//! unchanged. Every allocation sized by caller input is fallible.

use crate::clipper_core::{
    cross_product_sign, get_bounds_path, get_line_intersect_pt, is_collinear, point_in_polygon,
    Path64, Paths64, Point64, PointInPolygonResult, Rect64,
};
use crate::{copy_of, push, reserve, ClipperError, Result};

/// This port unit.
pub const UNIT: &str = "thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003";

/// `enum class Location { Left, Top, Right, Bottom, Inside }`; the order is significant
/// (`StartLocsAreClockwise`, `GetAdjacentLocation`, corner indices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Left = 0,
    Top = 1,
    Right = 2,
    Bottom = 3,
    Inside = 4,
}

impl Location {
    /// `static_cast<int>(loc)`.
    pub fn as_int(self) -> i32 {
        self as i32
    }

    /// `static_cast<Location>(value)` for the values this file produces (0..=4).
    pub fn from_int(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Top),
            2 => Some(Self::Right),
            3 => Some(Self::Bottom),
            4 => Some(Self::Inside),
            _ => None,
        }
    }
}

/// Host limits on the work of one `Execute` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectClipBudget {
    /// `OutPt2` nodes in `op_container_`; also bounds `start_locs_`.
    pub ops: usize,
    /// Loop iterations over all loops of the call.
    pub steps: u64,
}

const INDEX: &str = "out-of-range OutPt2 index";

// --- Miscellaneous methods -------------------------------------------------------------------

/// `inline bool Path1ContainsPath2(const Path64& path1, const Path64& path2)`.
pub fn path1_contains_path2(path1: &[Point64], path2: &[Point64]) -> Result<bool> {
    let mut io_count: i32 = 0;
    // precondition: no (significant) overlap
    for pt in path2 {
        match point_in_polygon(pt, path1)? {
            PointInPolygonResult::IsOutside => io_count += 1,
            PointInPolygonResult::IsInside => io_count -= 1,
            PointInPolygonResult::IsOn => continue,
        }
        if io_count.abs() > 1 {
            break;
        }
    }
    Ok(io_count <= 0)
}

/// `inline bool GetLocation(const Rect64& rec, const Point64& pt, Location& loc)`: `false`
/// for a point on the rectangle's boundary.
pub fn get_location(rec: &Rect64, pt: &Point64, loc: &mut Location) -> bool {
    if pt.x == rec.left && pt.y >= rec.top && pt.y <= rec.bottom {
        *loc = Location::Left;
        return false;
    } else if pt.x == rec.right && pt.y >= rec.top && pt.y <= rec.bottom {
        *loc = Location::Right;
        return false;
    } else if pt.y == rec.top && pt.x >= rec.left && pt.x <= rec.right {
        *loc = Location::Top;
        return false;
    } else if pt.y == rec.bottom && pt.x >= rec.left && pt.x <= rec.right {
        *loc = Location::Bottom;
        return false;
    } else if pt.x < rec.left {
        *loc = Location::Left;
    } else if pt.x > rec.right {
        *loc = Location::Right;
    } else if pt.y < rec.top {
        *loc = Location::Top;
    } else if pt.y > rec.bottom {
        *loc = Location::Bottom;
    } else {
        *loc = Location::Inside;
    }
    true
}

/// `inline bool IsHorizontal(const Point64& pt1, const Point64& pt2)`.
pub fn is_horizontal(pt1: &Point64, pt2: &Point64) -> bool {
    pt1.y == pt2.y
}

/// `bool GetSegmentIntersection(p1, p2, p3, p4, Point64& ip)`. `ip` is assigned on some
/// `false` returns (collinear segments) and left unchanged on others, as in the C++.
pub fn get_segment_intersection(
    p1: &Point64,
    p2: &Point64,
    p3: &Point64,
    p4: &Point64,
    ip: &mut Point64,
) -> Result<bool> {
    let res1 = cross_product_sign(p1, p3, p4)?;
    let res2 = cross_product_sign(p2, p3, p4)?;
    if res1 == 0 {
        *ip = *p1;
        if res2 == 0 {
            return Ok(false); // segments are collinear
        } else if p1 == p3 || p1 == p4 {
            return Ok(true);
        } else if is_horizontal(p3, p4) {
            return Ok((p1.x > p3.x) == (p1.x < p4.x));
        } else {
            return Ok((p1.y > p3.y) == (p1.y < p4.y));
        }
    } else if res2 == 0 {
        *ip = *p2;
        if p2 == p3 || p2 == p4 {
            return Ok(true);
        } else if is_horizontal(p3, p4) {
            return Ok((p2.x > p3.x) == (p2.x < p4.x));
        } else {
            return Ok((p2.y > p3.y) == (p2.y < p4.y));
        }
    }
    if (res1 > 0) == (res2 > 0) {
        return Ok(false);
    }

    let res3 = cross_product_sign(p3, p1, p2)?;
    let res4 = cross_product_sign(p4, p1, p2)?;
    if res3 == 0 {
        *ip = *p3;
        if p3 == p1 || p3 == p2 {
            return Ok(true);
        } else if is_horizontal(p1, p2) {
            return Ok((p3.x > p1.x) == (p3.x < p2.x));
        } else {
            return Ok((p3.y > p1.y) == (p3.y < p2.y));
        }
    } else if res4 == 0 {
        *ip = *p4;
        if p4 == p1 || p4 == p2 {
            return Ok(true);
        } else if is_horizontal(p1, p2) {
            return Ok((p4.x > p1.x) == (p4.x < p2.x));
        } else {
            return Ok((p4.y > p1.y) == (p4.y < p2.y));
        }
    }
    if (res3 > 0) == (res4 > 0) {
        return Ok(false);
    }

    // segments must intersect to get here
    get_line_intersect_pt(p1, p2, p3, p4, ip)
}

/// `rectPath[n]` for the constant corner indices 0..=3 of `GetIntersection`.
fn corner(rect_path: &[Point64], n: usize) -> Result<&Point64> {
    rect_path
        .get(n)
        .ok_or(ClipperError::Undefined("rectPath index out of range"))
}

/// `inline bool GetIntersection(const Path64& rectPath, p, p2, Location& loc, Point64& ip)`:
/// the intersection closest to `p`; `loc` is unchanged when the result is `false`.
pub fn get_intersection(
    rect_path: &[Point64],
    p: &Point64,
    p2: &Point64,
    loc: &mut Location,
    ip: &mut Point64,
) -> Result<bool> {
    let r = |n| corner(rect_path, n);
    let seg = |a: usize, b: usize, ip: &mut Point64| -> Result<bool> {
        get_segment_intersection(p, p2, r(a)?, r(b)?, ip)
    };
    match *loc {
        Location::Left => {
            if seg(0, 3, ip)? {
                Ok(true)
            } else if (p.y < r(0)?.y) && seg(0, 1, ip)? {
                *loc = Location::Top;
                Ok(true)
            } else if seg(2, 3, ip)? {
                *loc = Location::Bottom;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Location::Top => {
            if seg(0, 1, ip)? {
                Ok(true)
            } else if (p.x < r(0)?.x) && seg(0, 3, ip)? {
                *loc = Location::Left;
                Ok(true)
            } else if seg(1, 2, ip)? {
                *loc = Location::Right;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Location::Right => {
            if seg(1, 2, ip)? {
                Ok(true)
            } else if (p.y < r(1)?.y) && seg(0, 1, ip)? {
                *loc = Location::Top;
                Ok(true)
            } else if seg(2, 3, ip)? {
                *loc = Location::Bottom;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Location::Bottom => {
            if seg(2, 3, ip)? {
                Ok(true)
            } else if (p.x < r(3)?.x) && seg(0, 3, ip)? {
                *loc = Location::Left;
                Ok(true)
            } else if seg(1, 2, ip)? {
                *loc = Location::Right;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Location::Inside => {
            // default: loc == rInside
            if seg(0, 3, ip)? {
                *loc = Location::Left;
                Ok(true)
            } else if seg(0, 1, ip)? {
                *loc = Location::Top;
                Ok(true)
            } else if seg(1, 2, ip)? {
                *loc = Location::Right;
                Ok(true)
            } else if seg(2, 3, ip)? {
                *loc = Location::Bottom;
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

/// `inline Location GetAdjacentLocation(Location loc, bool isClockwise)`:
/// `(static_cast<int>(loc) + delta) % 4`, so `Inside` maps to `Top` or `Bottom`.
pub fn get_adjacent_location(loc: Location, is_clockwise: bool) -> Location {
    let delta = if is_clockwise { 1 } else { 3 };
    match (loc.as_int() + delta) % 4 {
        0 => Location::Left,
        1 => Location::Top,
        2 => Location::Right,
        _ => Location::Bottom,
    }
}

/// `inline bool HeadingClockwise(Location prev, Location curr)`.
pub fn heading_clockwise(prev: Location, curr: Location) -> bool {
    (prev.as_int() + 1) % 4 == curr.as_int()
}

/// `inline bool AreOpposites(Location prev, Location curr)`: also true for
/// `Inside`/`Right`.
pub fn are_opposites(prev: Location, curr: Location) -> bool {
    (prev.as_int() - curr.as_int()).abs() == 2
}

/// `inline bool IsClockwise(prev, curr, prev_pt, curr_pt, rect_mp)`.
pub fn is_clockwise(
    prev: Location,
    curr: Location,
    prev_pt: &Point64,
    curr_pt: &Point64,
    rect_mp: &Point64,
) -> Result<bool> {
    if are_opposites(prev, curr) {
        Ok(cross_product_sign(prev_pt, rect_mp, curr_pt)? < 0)
    } else {
        Ok(heading_clockwise(prev, curr))
    }
}

/// `inline uint32_t GetEdgesForPt(const Point64& pt, const Rect64& rec)`.
pub fn get_edges_for_pt(pt: &Point64, rec: &Rect64) -> u32 {
    let mut result = 0;
    if pt.x == rec.left {
        result = 1;
    } else if pt.x == rec.right {
        result = 4;
    }
    if pt.y == rec.top {
        result += 2;
    } else if pt.y == rec.bottom {
        result += 8;
    }
    result
}

/// `inline bool IsHeadingClockwise(const Point64& pt1, const Point64& pt2, int edgeIdx)`.
pub fn is_heading_clockwise(pt1: &Point64, pt2: &Point64, edge_idx: i32) -> bool {
    match edge_idx {
        0 => pt2.y < pt1.y,
        1 => pt2.x > pt1.x,
        2 => pt2.y > pt1.y,
        _ => pt2.x < pt1.x,
    }
}

/// `inline bool HasHorzOverlap(left1, right1, left2, right2)`.
pub fn has_horz_overlap(
    left1: &Point64,
    right1: &Point64,
    left2: &Point64,
    right2: &Point64,
) -> bool {
    (left1.x < right2.x) && (right1.x > left2.x)
}

/// `inline bool HasVertOverlap(top1, bottom1, top2, bottom2)`.
pub fn has_vert_overlap(
    top1: &Point64,
    bottom1: &Point64,
    top2: &Point64,
    bottom2: &Point64,
) -> bool {
    (top1.y < bottom2.y) && (bottom1.y > top2.y)
}

/// `bool StartLocsAreClockwise(const std::vector<Location>& startlocs)`.
pub fn start_locs_are_clockwise(startlocs: &[Location]) -> bool {
    let mut result: i64 = 0;
    for pair in startlocs.windows(2) {
        let d = pair[1].as_int() - pair[0].as_int();
        match d {
            -1 => result -= 1,
            1 => result += 1,
            -3 => result += 1,
            3 => result -= 1,
            _ => {}
        }
    }
    result > 0
}

// --- OutPt2 ----------------------------------------------------------------------------------

/// `class OutPt2` with its default member initializers (`owner_idx = 0`, `edge = nullptr`).
#[derive(Clone, Copy, Debug)]
struct OutPt2 {
    pt: Point64,
    owner_idx: usize,
    /// Index of the `edges_` list this node belongs to.
    edge: Option<usize>,
    next: usize,
    prev: usize,
}

type OutPt2List = Vec<Option<usize>>;

// --- RectClip64 ------------------------------------------------------------------------------

/// `class RectClip64`.
#[derive(Debug)]
pub struct RectClip64 {
    rect: Rect64,
    rect_as_path: Path64,
    rect_mp: Point64,
    path_bounds: Rect64,
    op_container: Vec<OutPt2>,
    results: OutPt2List,
    edges: [OutPt2List; 8],
    start_locs: Vec<Location>,
    budget: RectClipBudget,
    steps_left: u64,
}

impl RectClip64 {
    /// `explicit RectClip64(const Rect64& rect)` (co-ported from `clipper.rectclip.h`):
    /// `rect_(rect), rect_as_path_(rect.AsPath()), rect_mp_(rect.MidPoint())`, the other
    /// members value-initialized (`path_bounds_` is `Rect64()`, all zero). `MidPoint` is
    /// undefined when `left + right` or `top + bottom` overflows.
    pub fn new(rect: &Rect64, budget: RectClipBudget) -> Result<Self> {
        let rect_as_path = rect.as_path();
        let rect_mp = rect.mid_point()?;
        Ok(Self {
            rect: *rect,
            rect_as_path,
            rect_mp,
            path_bounds: Rect64::default(),
            op_container: Vec::new(),
            results: Vec::new(),
            edges: Default::default(),
            start_locs: Vec::new(),
            budget,
            steps_left: budget.steps,
        })
    }

    /// One loop iteration against the call's step budget.
    fn tick(&mut self) -> Result<()> {
        self.steps_left = self
            .steps_left
            .checked_sub(1)
            .ok_or(ClipperError::ResourceLimit)?;
        Ok(())
    }

    fn node(&self, id: usize) -> Result<&OutPt2> {
        self.op_container
            .get(id)
            .ok_or(ClipperError::Undefined(INDEX))
    }

    fn node_mut(&mut self, id: usize) -> Result<&mut OutPt2> {
        self.op_container
            .get_mut(id)
            .ok_or(ClipperError::Undefined(INDEX))
    }

    fn pt(&self, id: usize) -> Result<Point64> {
        Ok(self.node(id)?.pt)
    }

    fn next(&self, id: usize) -> Result<usize> {
        Ok(self.node(id)?.next)
    }

    fn prev(&self, id: usize) -> Result<usize> {
        Ok(self.node(id)?.prev)
    }

    fn owner(&self, id: usize) -> Result<usize> {
        Ok(self.node(id)?.owner_idx)
    }

    fn set_next(&mut self, id: usize, next: usize) -> Result<()> {
        self.node_mut(id)?.next = next;
        Ok(())
    }

    fn set_prev(&mut self, id: usize, prev: usize) -> Result<()> {
        self.node_mut(id)?.prev = prev;
        Ok(())
    }

    /// `results_[idx] = value`.
    fn set_result(&mut self, idx: usize, value: Option<usize>) -> Result<()> {
        let slot = self
            .results
            .get_mut(idx)
            .ok_or(ClipperError::Undefined("results_ index out of range"))?;
        *slot = value;
        Ok(())
    }

    /// `edges_[list][idx]`.
    fn edge_at(&self, list: usize, idx: usize) -> Result<Option<usize>> {
        self.edges
            .get(list)
            .and_then(|l| l.get(idx))
            .copied()
            .ok_or(ClipperError::Undefined("OutPt2List index out of range"))
    }

    /// `edges_[list][idx] = value`.
    fn set_edge(&mut self, list: usize, idx: usize, value: Option<usize>) -> Result<()> {
        let slot = self
            .edges
            .get_mut(list)
            .and_then(|l| l.get_mut(idx))
            .ok_or(ClipperError::Undefined("OutPt2List index out of range"))?;
        *slot = value;
        Ok(())
    }

    fn edge_len(&self, list: usize) -> usize {
        self.edges.get(list).map_or(0, Vec::len)
    }

    /// `rect_as_path_[static_cast<size_t>(loc)]`: index 4 (`Inside`) is past the end.
    fn rect_corner(&self, loc: Location) -> Result<Point64> {
        self.rect_as_path
            .get(loc as usize)
            .copied()
            .ok_or(ClipperError::Undefined("rect_as_path_[Location::Inside]"))
    }

    /// `op_container_.emplace_back(OutPt2())` under the arena budget.
    fn new_op(&mut self, pt: Point64) -> Result<usize> {
        if self.op_container.len() >= self.budget.ops {
            return Err(ClipperError::ResourceLimit);
        }
        let id = self.op_container.len();
        push(
            &mut self.op_container,
            OutPt2 {
                pt,
                owner_idx: 0,
                edge: None,
                next: id,
                prev: id,
            },
        )?;
        Ok(id)
    }

    /// `start_locs_.emplace_back(loc)` under the arena budget.
    fn push_start_loc(&mut self, loc: Location) -> Result<()> {
        if self.start_locs.len() >= self.budget.ops {
            return Err(ClipperError::ResourceLimit);
        }
        push(&mut self.start_locs, loc)
    }

    /// `OutPt2* RectClip64::Add(Point64 pt, bool start_new = false)`.
    fn add(&mut self, pt: Point64, start_new: bool) -> Result<usize> {
        // this method is only called by InternalExecute.
        // Later splitting & rejoining won't create additional op's,
        // though they will change the (non-storage) results_ count.
        let mut curr_idx = self.results.len();
        if curr_idx == 0 || start_new {
            reserve(&mut self.results, 1)?;
            // result->next = result; result->prev = result; owner_idx keeps its default 0.
            let result = self.new_op(pt)?;
            self.results.push(Some(result));
            Ok(result)
        } else {
            curr_idx -= 1;
            let prev_op = self
                .results
                .get(curr_idx)
                .copied()
                .flatten()
                .ok_or(ClipperError::Undefined("null results_ entry in Add"))?;
            if self.pt(prev_op)? == pt {
                return Ok(prev_op);
            }
            let prev_next = self.next(prev_op)?;
            let result = self.new_op(pt)?;
            {
                let r = self.node_mut(result)?;
                r.owner_idx = curr_idx;
                r.next = prev_next;
                r.prev = prev_op;
            }
            self.set_prev(prev_next, result)?;
            self.set_next(prev_op, result)?;
            self.set_result(curr_idx, Some(result))?;
            Ok(result)
        }
    }

    /// `void RectClip64::AddCorner(Location prev, Location curr)`.
    fn add_corner_between(&mut self, prev: Location, curr: Location) -> Result<()> {
        if heading_clockwise(prev, curr) {
            let pt = self.rect_corner(prev)?;
            self.add(pt, false)?;
        } else {
            let pt = self.rect_corner(curr)?;
            self.add(pt, false)?;
        }
        Ok(())
    }

    /// `void RectClip64::AddCorner(Location& loc, bool isClockwise)`.
    fn add_corner_step(&mut self, loc: &mut Location, is_clockwise: bool) -> Result<()> {
        if is_clockwise {
            let pt = self.rect_corner(*loc)?;
            self.add(pt, false)?;
            *loc = get_adjacent_location(*loc, true);
        } else {
            *loc = get_adjacent_location(*loc, false);
            let pt = self.rect_corner(*loc)?;
            self.add(pt, false)?;
        }
        Ok(())
    }

    /// `void RectClip64::GetNextLocation(const Path64& path, Location& loc, size_t& i,
    /// size_t highI)`.
    fn get_next_location(
        &mut self,
        path: &[Point64],
        loc: &mut Location,
        i: &mut usize,
        high_i: usize,
    ) -> Result<()> {
        let at = |i: usize| -> Result<&Point64> {
            path.get(i)
                .ok_or(ClipperError::Undefined("path index out of range"))
        };
        let rect = self.rect;
        match *loc {
            Location::Left => {
                while *i <= high_i && at(*i)?.x <= rect.left {
                    self.tick()?;
                    *i += 1;
                }
                if *i > high_i {
                    return Ok(());
                }
                let p = at(*i)?;
                *loc = if p.x >= rect.right {
                    Location::Right
                } else if p.y <= rect.top {
                    Location::Top
                } else if p.y >= rect.bottom {
                    Location::Bottom
                } else {
                    Location::Inside
                };
            }
            Location::Top => {
                while *i <= high_i && at(*i)?.y <= rect.top {
                    self.tick()?;
                    *i += 1;
                }
                if *i > high_i {
                    return Ok(());
                }
                let p = at(*i)?;
                *loc = if p.y >= rect.bottom {
                    Location::Bottom
                } else if p.x <= rect.left {
                    Location::Left
                } else if p.x >= rect.right {
                    Location::Right
                } else {
                    Location::Inside
                };
            }
            Location::Right => {
                while *i <= high_i && at(*i)?.x >= rect.right {
                    self.tick()?;
                    *i += 1;
                }
                if *i > high_i {
                    return Ok(());
                }
                let p = at(*i)?;
                *loc = if p.x <= rect.left {
                    Location::Left
                } else if p.y <= rect.top {
                    Location::Top
                } else if p.y >= rect.bottom {
                    Location::Bottom
                } else {
                    Location::Inside
                };
            }
            Location::Bottom => {
                while *i <= high_i && at(*i)?.y >= rect.bottom {
                    self.tick()?;
                    *i += 1;
                }
                if *i > high_i {
                    return Ok(());
                }
                let p = at(*i)?;
                *loc = if p.y <= rect.top {
                    Location::Top
                } else if p.x <= rect.left {
                    Location::Left
                } else if p.x >= rect.right {
                    Location::Right
                } else {
                    Location::Inside
                };
            }
            Location::Inside => {
                while *i <= high_i {
                    self.tick()?;
                    let p = *at(*i)?;
                    if p.x < rect.left {
                        *loc = Location::Left;
                    } else if p.x > rect.right {
                        *loc = Location::Right;
                    } else if p.y > rect.bottom {
                        *loc = Location::Bottom;
                    } else if p.y < rect.top {
                        *loc = Location::Top;
                    } else {
                        self.add(p, false)?;
                        *i += 1;
                        continue;
                    }
                    break; //inner loop
                }
            }
        }
        Ok(())
    }

    /// `void RectClip64::ExecuteInternal(const Path64& path)`.
    fn execute_internal(&mut self, path: &[Point64]) -> Result<()> {
        if path.is_empty() {
            return Ok(());
        }
        let at = |i: usize| -> Result<Point64> {
            path.get(i)
                .copied()
                .ok_or(ClipperError::Undefined("path index out of range"))
        };

        let high_i = path.len() - 1;
        let mut prev = Location::Inside;
        // `Location loc;` is always assigned by the first GetLocation.
        let mut loc = Location::Inside;
        let mut crossing_loc = Location::Inside;
        let mut first_cross = Location::Inside;
        if !get_location(&self.rect, &at(high_i)?, &mut loc) {
            let mut i = high_i;
            while i > 0 && !get_location(&self.rect, &at(i - 1)?, &mut prev) {
                self.tick()?;
                i -= 1;
            }
            if i == 0 {
                // all of path must be inside fRect
                for pt in path {
                    self.tick()?;
                    self.add(*pt, false)?;
                }
                return Ok(());
            }
            if prev == Location::Inside {
                loc = Location::Inside;
            }
        }
        let starting_loc = loc;

        ///////////////////////////////////////////////////
        let mut i: usize = 0;
        while i <= high_i {
            self.tick()?;
            prev = loc;
            let crossing_prev = crossing_loc;

            self.get_next_location(path, &mut loc, &mut i, high_i)?;

            if i > high_i {
                break;
            }
            let mut ip = Point64::default();
            let mut ip2 = Point64::default();
            let prev_pt = if i != 0 { at(i - 1)? } else { at(high_i)? };
            let path_i = at(i)?;

            crossing_loc = loc;
            if !get_intersection(
                &self.rect_as_path,
                &path_i,
                &prev_pt,
                &mut crossing_loc,
                &mut ip,
            )? {
                // ie remaining outside
                if crossing_prev == Location::Inside {
                    let is_clockw = is_clockwise(prev, loc, &prev_pt, &path_i, &self.rect_mp)?;
                    loop {
                        self.tick()?;
                        self.push_start_loc(prev)?;
                        prev = get_adjacent_location(prev, is_clockw);
                        if prev == loc {
                            break;
                        }
                    }
                    crossing_loc = crossing_prev; // still not crossed
                } else if prev != Location::Inside && prev != loc {
                    let is_clockw = is_clockwise(prev, loc, &prev_pt, &path_i, &self.rect_mp)?;
                    loop {
                        self.tick()?;
                        self.add_corner_step(&mut prev, is_clockw)?;
                        if prev == loc {
                            break;
                        }
                    }
                }
                i += 1;
                continue;
            }

            ////////////////////////////////////////////////////
            // we must be crossing the rect boundary to get here
            ////////////////////////////////////////////////////

            if loc == Location::Inside {
                // path must be entering rect
                if first_cross == Location::Inside {
                    first_cross = crossing_loc;
                    self.push_start_loc(prev)?;
                } else if prev != crossing_loc {
                    let is_clockw =
                        is_clockwise(prev, crossing_loc, &prev_pt, &path_i, &self.rect_mp)?;
                    loop {
                        self.tick()?;
                        self.add_corner_step(&mut prev, is_clockw)?;
                        if prev == crossing_loc {
                            break;
                        }
                    }
                }
            } else if prev != Location::Inside {
                // passing right through rect. 'ip' here will be the second
                // intersect pt but we'll also need the first intersect pt (ip2)
                loc = prev;
                get_intersection(&self.rect_as_path, &prev_pt, &path_i, &mut loc, &mut ip2)?;
                if crossing_prev != Location::Inside && crossing_prev != loc {
                    //579
                    self.add_corner_between(crossing_prev, loc)?;
                }

                if first_cross == Location::Inside {
                    first_cross = loc;
                    self.push_start_loc(prev)?;
                }

                loc = crossing_loc;
                self.add(ip2, false)?;
                if ip == ip2 {
                    // it's very likely that path[i] is on rect
                    get_location(&self.rect, &path_i, &mut loc);
                    self.add_corner_between(crossing_loc, loc)?;
                    crossing_loc = loc;
                    continue;
                }
            } else {
                // path must be exiting rect
                loc = crossing_loc;
                if first_cross == Location::Inside {
                    first_cross = crossing_loc;
                }
            }

            self.add(ip, false)?;
        } //while i <= highI
          ///////////////////////////////////////////////////

        if first_cross == Location::Inside {
            // path never intersects
            if starting_loc != Location::Inside {
                // path is outside rect
                // but being outside, it still may not contain rect
                if self.path_bounds.contains_rect(&self.rect)
                    && path1_contains_path2(path, &self.rect_as_path)?
                {
                    // yep, the path does fully contain rect
                    // so add rect to the solution
                    let is_clockwise_path = start_locs_are_clockwise(&self.start_locs);
                    for j in 0..4usize {
                        let k = if is_clockwise_path { j } else { 3 - j }; // reverses result path
                        let pt = *self
                            .rect_as_path
                            .get(k)
                            .ok_or(ClipperError::Undefined("rect_as_path_ index"))?;
                        self.add(pt, false)?;
                        // we may well need to do some splitting later, so
                        let first = self
                            .results
                            .first()
                            .copied()
                            .flatten()
                            .ok_or(ClipperError::Undefined("results_[0]"))?;
                        self.add_to_edge(k * 2, first)?;
                    }
                }
            }
        } else if loc != Location::Inside && (loc != first_cross || self.start_locs.len() > 2) {
            if !self.start_locs.is_empty() {
                prev = loc;
                for n in 0..self.start_locs.len() {
                    self.tick()?;
                    let loc2 = self.start_locs[n];
                    if prev == loc2 {
                        continue;
                    }
                    let cw = heading_clockwise(prev, loc2);
                    self.add_corner_step(&mut prev, cw)?;
                    prev = loc2;
                }
                loc = prev;
            }
            if loc != first_cross {
                let cw = heading_clockwise(loc, first_cross);
                self.add_corner_step(&mut loc, cw)?;
            }
        }
        Ok(())
    }

    /// `inline void AddToEdge(OutPt2List& edge, OutPt2* op)`.
    fn add_to_edge(&mut self, edge: usize, op: usize) -> Result<()> {
        if self.node(op)?.edge.is_some() {
            return Ok(());
        }
        let list = self
            .edges
            .get_mut(edge)
            .ok_or(ClipperError::Undefined("edges_ index"))?;
        reserve(list, 1)?;
        list.push(Some(op));
        self.node_mut(op)?.edge = Some(edge);
        Ok(())
    }

    /// `inline void UncoupleEdge(OutPt2* op)`.
    fn uncouple_edge(&mut self, op: usize) -> Result<()> {
        let Some(edge) = self.node(op)?.edge else {
            return Ok(());
        };
        for i in 0..self.edge_len(edge) {
            self.tick()?;
            if self.edge_at(edge, i)? == Some(op) {
                self.set_edge(edge, i, None)?;
                break;
            }
        }
        self.node_mut(op)?.edge = None;
        Ok(())
    }

    /// `inline void SetNewOwner(OutPt2* op, size_t new_idx)`.
    fn set_new_owner(&mut self, op: usize, new_idx: usize) -> Result<()> {
        self.node_mut(op)?.owner_idx = new_idx;
        let mut op2 = self.next(op)?;
        while op2 != op {
            self.tick()?;
            self.node_mut(op2)?.owner_idx = new_idx;
            op2 = self.next(op2)?;
        }
        Ok(())
    }

    /// `inline OutPt2* UnlinkOp(OutPt2* op)`.
    fn unlink_op(&mut self, op: usize) -> Result<Option<usize>> {
        let (next, prev) = (self.next(op)?, self.prev(op)?);
        if next == op {
            return Ok(None);
        }
        self.set_next(prev, next)?;
        self.set_prev(next, prev)?;
        Ok(Some(next))
    }

    /// `inline OutPt2* UnlinkOpBack(OutPt2* op)`.
    fn unlink_op_back(&mut self, op: usize) -> Result<Option<usize>> {
        let (next, prev) = (self.next(op)?, self.prev(op)?);
        if next == op {
            return Ok(None);
        }
        self.set_next(prev, next)?;
        self.set_prev(next, prev)?;
        Ok(Some(prev))
    }

    fn collinear_at(&self, op: usize) -> Result<bool> {
        let prev = self.pt(self.prev(op)?)?;
        let next = self.pt(self.next(op)?)?;
        is_collinear(&prev, &self.pt(op)?, &next)
    }

    /// `void RectClip64::CheckEdges()`.
    fn check_edges(&mut self) -> Result<()> {
        for i in 0..self.results.len() {
            self.tick()?;
            let Some(mut op) = self.results[i] else {
                continue;
            };
            let mut op2 = op;
            let became_null = loop {
                self.tick()?;
                if self.collinear_at(op2)? {
                    if op2 == op {
                        match self.unlink_op_back(op2)? {
                            None => break true,
                            Some(n) => {
                                op2 = n;
                                op = self.prev(op2)?;
                            }
                        }
                    } else {
                        match self.unlink_op_back(op2)? {
                            None => break true,
                            Some(n) => op2 = n,
                        }
                    }
                } else {
                    op2 = self.next(op2)?;
                }
                if op2 == op {
                    break false;
                }
            };

            if became_null {
                self.results[i] = None;
                continue;
            }
            self.results[i] = Some(op); // safety first

            let mut edge_set1 = get_edges_for_pt(&self.pt(self.prev(op)?)?, &self.rect);
            op2 = op;
            loop {
                self.tick()?;
                let edge_set2 = get_edges_for_pt(&self.pt(op2)?, &self.rect);
                if edge_set2 != 0 && self.node(op2)?.edge.is_none() {
                    let combined_set = edge_set1 & edge_set2;
                    for j in 0..4i32 {
                        if combined_set & (1 << j) != 0 {
                            let prev_pt = self.pt(self.prev(op2)?)?;
                            let j2 = j as usize * 2;
                            if is_heading_clockwise(&prev_pt, &self.pt(op2)?, j) {
                                self.add_to_edge(j2, op2)?;
                            } else {
                                self.add_to_edge(j2 + 1, op2)?;
                            }
                        }
                    }
                }
                edge_set1 = edge_set2;
                op2 = self.next(op2)?;
                if op2 == op {
                    break;
                }
            }
        }
        Ok(())
    }

    /// `ccw[j]` is null or a two-point loop (`next == prev`).
    fn is_spent(&self, list: usize, idx: usize) -> Result<bool> {
        Ok(match self.edge_at(list, idx)? {
            None => true,
            Some(op) => self.next(op)? == self.prev(op)?,
        })
    }

    /// `void RectClip64::TidyEdges(size_t idx, OutPt2List& cw, OutPt2List& ccw)` with
    /// `cw = edges_[idx * 2]` and `ccw = edges_[idx * 2 + 1]`, its only call.
    fn tidy_edges(&mut self, idx: usize) -> Result<()> {
        let (cw, ccw) = (idx * 2, idx * 2 + 1);
        if self.edge_len(ccw) == 0 {
            return Ok(());
        }
        let is_horz = idx == 1 || idx == 3;
        let cw_is_toward_larger = idx == 1 || idx == 2;
        let (mut i, mut j) = (0usize, 0usize);

        while i < self.edge_len(cw) {
            self.tick()?;
            if self.is_spent(cw, i)? {
                self.set_edge(cw, i, None)?;
                i += 1;
                j = 0;
                continue;
            }

            let j_lim = self.edge_len(ccw);
            while j < j_lim && self.is_spent(ccw, j)? {
                self.tick()?;
                j += 1;
            }

            if j == j_lim {
                i += 1;
                j = 0;
                continue;
            }

            let cw_i = self
                .edge_at(cw, i)?
                .ok_or(ClipperError::Undefined("null cw entry"))?;
            let ccw_j = self
                .edge_at(ccw, j)?
                .ok_or(ClipperError::Undefined("null ccw entry"))?;
            let (p1, p1a, p2, p2a) = if cw_is_toward_larger {
                // p1 >>>> p1a;
                // p2 <<<< p2a;
                (self.prev(cw_i)?, cw_i, ccw_j, self.prev(ccw_j)?)
            } else {
                // p1 <<<< p1a;
                // p2 >>>> p2a;
                (cw_i, self.prev(cw_i)?, self.prev(ccw_j)?, ccw_j)
            };

            let (q1, q1a, q2, q2a) = (self.pt(p1)?, self.pt(p1a)?, self.pt(p2)?, self.pt(p2a)?);
            if (is_horz && !has_horz_overlap(&q1, &q1a, &q2, &q2a))
                || (!is_horz && !has_vert_overlap(&q1, &q1a, &q2, &q2a))
            {
                j += 1;
                continue;
            }

            // to get here we're either splitting or rejoining
            let is_rejoining = self.owner(cw_i)? != self.owner(ccw_j)?;

            if is_rejoining {
                let p2_owner = self.owner(p2)?;
                self.set_result(p2_owner, None)?;
                let p1_owner = self.owner(p1)?;
                self.set_new_owner(p2, p1_owner)?;
            }

            // do the split or re-join
            if cw_is_toward_larger {
                // p1 >> | >> p1a;
                // p2 << | << p2a;
                self.set_next(p1, p2)?;
                self.set_prev(p2, p1)?;
                self.set_prev(p1a, p2a)?;
                self.set_next(p2a, p1a)?;
            } else {
                // p1 << | << p1a;
                // p2 >> | >> p2a;
                self.set_prev(p1, p2)?;
                self.set_next(p2, p1)?;
                self.set_next(p1a, p2a)?;
                self.set_prev(p2a, p1a)?;
            }

            if !is_rejoining {
                let new_idx = self.results.len();
                push(&mut self.results, Some(p1a))?;
                self.set_new_owner(p1a, new_idx)?;
            }

            let (op, op2) = if cw_is_toward_larger {
                (p2, p1a)
            } else {
                (p1, p2a)
            };
            let op_owner = self.owner(op)?;
            self.set_result(op_owner, Some(op))?;
            let op2_owner = self.owner(op2)?;
            self.set_result(op2_owner, Some(op2))?;

            // and now lots of work to get ready for the next loop

            let (op_pt, op_prev_pt) = (self.pt(op)?, self.pt(self.prev(op)?)?);
            let (op2_pt, op2_prev_pt) = (self.pt(op2)?, self.pt(self.prev(op2)?)?);
            let (op_is_larger, op2_is_larger) = if is_horz {
                // X
                (op_pt.x > op_prev_pt.x, op2_pt.x > op2_prev_pt.x)
            } else {
                // Y
                (op_pt.y > op_prev_pt.y, op2_pt.y > op2_prev_pt.y)
            };

            if (self.next(op)? == self.prev(op)?) || (op_pt == op_prev_pt) {
                if op2_is_larger == cw_is_toward_larger {
                    self.set_edge(cw, i, Some(op2))?;
                    self.set_edge(ccw, j, None)?;
                    j += 1;
                } else {
                    self.set_edge(ccw, j, Some(op2))?;
                    self.set_edge(cw, i, None)?;
                    i += 1;
                }
            } else if (self.next(op2)? == self.prev(op2)?) || (op2_pt == op2_prev_pt) {
                if op_is_larger == cw_is_toward_larger {
                    self.set_edge(cw, i, Some(op))?;
                    self.set_edge(ccw, j, None)?;
                    j += 1;
                } else {
                    self.set_edge(ccw, j, Some(op))?;
                    self.set_edge(cw, i, None)?;
                    i += 1;
                }
            } else if op_is_larger == op2_is_larger {
                if op_is_larger == cw_is_toward_larger {
                    self.set_edge(cw, i, Some(op))?;
                    self.uncouple_edge(op2)?;
                    self.add_to_edge(cw, op2)?;
                    self.set_edge(ccw, j, None)?;
                    j += 1;
                } else {
                    self.set_edge(cw, i, None)?;
                    i += 1;
                    self.set_edge(ccw, j, Some(op2))?;
                    self.uncouple_edge(op)?;
                    self.add_to_edge(ccw, op)?;
                    j = 0;
                }
            } else {
                if op_is_larger == cw_is_toward_larger {
                    self.set_edge(cw, i, Some(op))?;
                } else {
                    self.set_edge(ccw, j, Some(op))?;
                }
                if op2_is_larger == cw_is_toward_larger {
                    self.set_edge(cw, i, Some(op2))?;
                } else {
                    self.set_edge(ccw, j, Some(op2))?;
                }
            }
        }
        Ok(())
    }

    /// Collect a circular list from `op` (`result.emplace_back(op->pt)`, then each `next`).
    fn collect(&mut self, op: usize) -> Result<Path64> {
        let mut result = Path64::new();
        push(&mut result, self.pt(op)?)?;
        let mut op2 = self.next(op)?;
        while op2 != op {
            self.tick()?;
            push(&mut result, self.pt(op2)?)?;
            op2 = self.next(op2)?;
        }
        Ok(result)
    }

    /// `Path64 RectClip64::GetPath(OutPt2*& op)`.
    fn get_path(&mut self, op: &mut Option<usize>) -> Result<Path64> {
        let Some(start) = *op else {
            return Ok(Path64::new());
        };
        if self.next(start)? == self.prev(start)? {
            return Ok(Path64::new());
        }

        let mut op2 = Some(self.next(start)?);
        while let Some(o2) = op2 {
            if Some(o2) == *op {
                break;
            }
            self.tick()?;
            if self.collinear_at(o2)? {
                *op = Some(self.prev(o2)?);
                op2 = self.unlink_op(o2)?;
            } else {
                op2 = Some(self.next(o2)?);
            }
        }
        *op = op2; // needed for op cleanup
        let Some(first) = op2 else {
            return Ok(Path64::new());
        };
        self.collect(first)
    }

    /// Reset the per-path state, as at the end of each `Execute` iteration.
    fn clean_up(&mut self, edges: bool) {
        // op_container_ = std::deque<OutPt2>();
        self.op_container = Vec::new();
        self.results.clear();
        if edges {
            for edge in &mut self.edges {
                edge.clear();
            }
        }
        self.start_locs.clear();
    }

    /// `Paths64 RectClip64::Execute(const Paths64& paths)`. On an error the object's per-path
    /// state is reset, so it remains usable.
    pub fn execute(&mut self, paths: &[Path64]) -> Result<Paths64> {
        self.steps_left = self.budget.steps;
        let result = self.execute_paths(paths);
        if result.is_err() {
            self.clean_up(true);
        }
        result
    }

    fn execute_paths(&mut self, paths: &[Path64]) -> Result<Paths64> {
        let mut result = Paths64::new();
        if self.rect.is_empty() {
            return Ok(result);
        }

        for path in paths {
            self.tick()?;
            if path.len() < 3 {
                continue;
            }
            self.path_bounds = get_bounds_path::<i64, i64>(path)?;
            if !self.rect.intersects(&self.path_bounds) {
                continue; // the path must be completely outside rect_
            } else if self.rect.contains_rect(&self.path_bounds) {
                // the path must be completely inside rect_
                reserve(&mut result, 1)?;
                result.push(copy_of(path)?);
                continue;
            }

            self.execute_internal(path)?;
            self.check_edges()?;
            for i in 0..4 {
                self.tidy_edges(i)?;
            }

            for n in 0..self.results.len() {
                let mut op = self.results[n];
                let tmp = self.get_path(&mut op)?;
                self.results[n] = op;
                if !tmp.is_empty() {
                    push(&mut result, tmp)?;
                }
            }

            //clean up after every loop
            self.clean_up(true);
        }
        Ok(result)
    }
}

// --- RectClipLines64 -------------------------------------------------------------------------

/// `class RectClipLines64 : public RectClip64`.
#[derive(Debug)]
pub struct RectClipLines64 {
    base: RectClip64,
}

impl RectClipLines64 {
    /// `explicit RectClipLines64(const Rect64& rect) : RectClip64(rect)` (co-ported from
    /// `clipper.rectclip.h`).
    pub fn new(rect: &Rect64, budget: RectClipBudget) -> Result<Self> {
        Ok(Self {
            base: RectClip64::new(rect, budget)?,
        })
    }

    /// `Paths64 RectClipLines64::Execute(const Paths64& paths)`. On an error the object's
    /// per-path state is reset, so it remains usable.
    pub fn execute(&mut self, paths: &[Path64]) -> Result<Paths64> {
        self.base.steps_left = self.base.budget.steps;
        let result = self.execute_paths(paths);
        if result.is_err() {
            self.base.clean_up(true);
        }
        result
    }

    fn execute_paths(&mut self, paths: &[Path64]) -> Result<Paths64> {
        let mut result = Paths64::new();
        if self.base.rect.is_empty() {
            return Ok(result);
        }

        for path in paths {
            self.base.tick()?;
            let pathrec = get_bounds_path::<i64, i64>(path)?;
            if !self.base.rect.intersects(&pathrec) {
                continue;
            }

            self.execute_internal(path)?;

            for n in 0..self.base.results.len() {
                let mut op = self.base.results[n];
                let tmp = self.get_path(&mut op)?;
                self.base.results[n] = op;
                if !tmp.is_empty() {
                    push(&mut result, tmp)?;
                }
            }
            self.base.results.clear();

            // op_container_ = std::deque<OutPt2>(); start_locs_.clear();
            self.base.op_container = Vec::new();
            self.base.start_locs.clear();
        }
        Ok(result)
    }

    /// `void RectClipLines64::ExecuteInternal(const Path64& path)`.
    fn execute_internal(&mut self, path: &[Point64]) -> Result<()> {
        let base = &mut self.base;
        if base.rect.is_empty() || path.len() < 2 {
            return Ok(());
        }

        base.results.clear();
        base.op_container = Vec::new();
        base.start_locs.clear();

        let at = |i: usize| -> Result<Point64> {
            path.get(i)
                .copied()
                .ok_or(ClipperError::Undefined("path index out of range"))
        };
        let mut i: usize = 1;
        let high_i = path.len() - 1;

        let mut prev = Location::Inside;
        let mut loc = Location::Inside;
        // `Location crossing_loc;` is assigned before every read.
        let mut crossing_loc;
        if !get_location(&base.rect, &at(0)?, &mut loc) {
            while i <= high_i && !get_location(&base.rect, &at(i)?, &mut prev) {
                base.tick()?;
                i += 1;
            }
            if i > high_i {
                // all of path must be inside fRect
                for pt in path {
                    base.tick()?;
                    base.add(*pt, false)?;
                }
                return Ok(());
            }
            if prev == Location::Inside {
                loc = Location::Inside;
            }
            i = 1;
        }
        if loc == Location::Inside {
            base.add(at(0)?, false)?;
        }

        ///////////////////////////////////////////////////
        while i <= high_i {
            base.tick()?;
            prev = loc;
            base.get_next_location(path, &mut loc, &mut i, high_i)?;
            if i > high_i {
                break;
            }
            let mut ip = Point64::default();
            let mut ip2 = Point64::default();
            let prev_pt = at(i - 1)?;
            let path_i = at(i)?;

            crossing_loc = loc;
            if !get_intersection(
                &base.rect_as_path,
                &path_i,
                &prev_pt,
                &mut crossing_loc,
                &mut ip,
            )? {
                // ie remaining outside
                i += 1;
                continue;
            }

            ////////////////////////////////////////////////////
            // we must be crossing the rect boundary to get here
            ////////////////////////////////////////////////////

            if loc == Location::Inside {
                // path must be entering rect
                base.add(ip, true)?;
            } else if prev != Location::Inside {
                // passing right through rect. 'ip' here will be the second
                // intersect pt but we'll also need the first intersect pt (ip2)
                crossing_loc = prev;
                get_intersection(
                    &base.rect_as_path,
                    &prev_pt,
                    &path_i,
                    &mut crossing_loc,
                    &mut ip2,
                )?;
                base.add(ip2, true)?;
                base.add(ip, false)?;
            } else {
                // path must be exiting rect
                base.add(ip, false)?;
            }
        } //while i <= highI
          ///////////////////////////////////////////////////
        Ok(())
    }

    /// `Path64 RectClipLines64::GetPath(OutPt2*& op)`.
    fn get_path(&mut self, op: &mut Option<usize>) -> Result<Path64> {
        let Some(start) = *op else {
            return Ok(Path64::new());
        };
        if start == self.base.next(start)? {
            return Ok(Path64::new());
        }
        let first = self.base.next(start)?; // starting at path beginning
        *op = Some(first);
        self.base.collect(first)
    }
}

#[cfg(test)]
mod tests;
