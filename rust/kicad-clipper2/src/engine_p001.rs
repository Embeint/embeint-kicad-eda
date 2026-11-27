// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.engine.cpp (Copyright Angus Johnson 2010-2026), Boost
// Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `clipper.engine.cpp` lines 1-2271: unit `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001`.
//!
//! The file-local helpers, `StripeIndex`, the point-in-polygon tests on output rings,
//! `AddPaths_`, `ReuseableDataContainer64` and the `ClipperBase` members from the destructor
//! to `IntersectEdges`, in the reference's order. Lines 2272 onward belong to
//! `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002` and are ported in
//! [`crate::engine_p002`]; the bodies here call them (`Split`, `CheckJoinLeft`,
//! `CheckJoinRight`, `SwapPositionsInAEL`) directly.
//! The cleaned-path fallback of `Path2ContainsPath1` calls the `clipper.h` template, owned by
//! `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`, and is deferred to it
//! the same way.
//!
//! Pointers are arena indices (see [`crate::clipper_engine`]). `new` pushes a record and
//! `delete` empties its slot, so a later use of a deleted record is refused as the undefined
//! behaviour it is. Every loop that follows links is bounded by the size of the arena it walks,
//! so a malformed ring or chain (which the reference would follow forever) is refused instead.
//! Integer arithmetic is checked: signed overflow, `abs` of the minimum value and out-of-range
//! `double` to `int64_t` conversions are refused ([`ClipperError::Undefined`]). Floating point
//! follows the oracle's `-ffp-contract=off`.

use crate::clipper_core::{
    cast_f64_to_i64, cross_product_sign, dot_product3, get_line_intersect_pt, is_collinear,
    nearbyint, segments_intersect, FillRule, Path64, Paths64, Point64, PointInPolygonResult,
};
use crate::clipper_engine::{
    Active, ActiveId, ClipType, ClipperBase, HorzSegment, IntersectNode, JoinWith, LocalMinima,
    LocalMinimaId, OutPt, OutPtId, OutRec, OutRecId, PathType, StripeEdge, StripeIndex, Vertex,
    VertexFlags, VertexId, ZCallbackSlot,
};
use crate::{push, reserve, with_capacity, ClipperError, Result};
use std::cell::Cell;
use std::rc::Rc;

/// This port unit.
pub const UNIT: &str = crate::ENGINE_SRC_P001;
/// Owner of `clipper.h` (`Path2ContainsPath1<T>(const Path<T>&, const Path<T>&)`).
pub const INCLUDE_P002: &str =
    "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002";

pub(crate) const NULL: ClipperError = ClipperError::Undefined("null pointer dereference");
pub(crate) const DELETED: ClipperError = ClipperError::Undefined("use of a deleted engine record");
const DANGLING_VERTEX: ClipperError =
    ClipperError::Undefined("use of a vertex its ReuseableDataContainer64 freed");
pub(crate) const ENDLESS: ClipperError = ClipperError::Undefined("non-terminating pointer walk");
pub(crate) const INT_OVERFLOW: ClipperError = ClipperError::Undefined("int overflow");
const I64_OVERFLOW: ClipperError = ClipperError::Undefined("int64_t overflow");
const STALE_ITERATOR: ClipperError = ClipperError::Undefined("use of an invalidated iterator");

/// `-std::numeric_limits<double>::max()` / `max()`: the dx of horizontal edges.
const MAX_DBL: f64 = f64::MAX;

pub(crate) fn some<T>(value: Option<T>) -> Result<T> {
    value.ok_or(NULL)
}

/// A vertex's `next`/`prev` link, read from vertex `from`. Links never leave their array, so
/// the target is in `from`'s list: a borrowed `ReuseableDataContainer64` array keeps the
/// container's list numbers in its links, which are rebound to the list `from` was reached
/// through.
fn rebind(from: VertexId, link: Option<VertexId>) -> Option<VertexId> {
    link.map(|l| VertexId {
        list: from.list,
        index: l.index,
    })
}

/// A walk over `limit` records at most; the reference loops forever on a malformed link
/// structure, which is refused here.
pub(crate) struct Walk(usize);

impl Walk {
    pub(crate) fn new(records: usize) -> Self {
        Self(records.saturating_add(2))
    }

    /// A walk allowing at most `steps` steps.
    pub(crate) fn limit(steps: usize) -> Self {
        Self(steps)
    }

    /// A walk allowing `records * records` steps (nested restarts such as `FixSelfIntersects`).
    pub(crate) fn squared(records: usize) -> Self {
        let n = records.saturating_add(2);
        Self(n.saturating_mul(n))
    }

    pub(crate) fn step(&mut self) -> Result<()> {
        self.0 = self.0.checked_sub(1).ok_or(ENDLESS)?;
        Ok(())
    }
}

/// `int64_t` subtraction.
pub(crate) fn sub64(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or(I64_OVERFLOW)
}

/// `int64_t` addition.
pub(crate) fn add64(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or(I64_OVERFLOW)
}

/// `int` addition.
fn add32(a: i32, b: i32) -> Result<i32> {
    a.checked_add(b).ok_or(INT_OVERFLOW)
}

/// `int` subtraction.
fn sub32(a: i32, b: i32) -> Result<i32> {
    a.checked_sub(b).ok_or(INT_OVERFLOW)
}

/// `int` multiplication.
fn mul32(a: i32, b: i32) -> Result<i32> {
    a.checked_mul(b).ok_or(INT_OVERFLOW)
}

/// `abs(int)`.
fn abs32(a: i32) -> Result<i32> {
    a.checked_abs().ok_or(INT_OVERFLOW)
}

/// Unary `-` on an `int`.
fn neg32(a: i32) -> Result<i32> {
    a.checked_neg().ok_or(INT_OVERFLOW)
}

// --- Scanline and the sort functors -------------------------------------------------------

/// `struct Scanline` (declared in the reference but not used by its engine).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scanline {
    pub y: i64,
    /// `Scanline* next`: no scanline record is ever linked; always null.
    pub next: Option<usize>,
}

impl Scanline {
    /// `explicit Scanline(int64_t y_) : y(y_) {}`.
    pub fn new(y: i64) -> Self {
        Self { y, next: None }
    }
}

/// `HorzSegSorter::operator()(hs1, hs2)`: segments without `right_op` sort last; otherwise by
/// the `left_op` x coordinate.
pub fn horz_seg_sorter(base: &ClipperBase, hs1: &HorzSegment, hs2: &HorzSegment) -> Result<bool> {
    if hs1.right_op.is_none() || hs2.right_op.is_none() {
        return Ok(hs1.right_op.is_some());
    }
    let hs2_x = base.out_pt(some(hs2.left_op)?)?.pt.x;
    let hs1_x = base.out_pt(some(hs1.left_op)?)?.pt.x;
    Ok(hs2_x > hs1_x)
}

/// `LocMinSorter::operator()(locMin1, locMin2)` on the two minima's vertex points: descending
/// y, then ascending x.
pub fn loc_min_sorter(loc_min1: &Point64, loc_min2: &Point64) -> bool {
    if loc_min2.y != loc_min1.y {
        loc_min2.y < loc_min1.y
    } else {
        loc_min2.x > loc_min1.x
    }
}

/// `IntersectListSort::operator()(a, b)`: "note different inequality tests".
pub fn intersect_list_sort(a: &IntersectNode, b: &IntersectNode) -> bool {
    if a.pt.y == b.pt.y {
        a.pt.x < b.pt.x
    } else {
        a.pt.y > b.pt.y
    }
}

/// `std::stable_sort` with a strict weak ordering `less`: a bottom-up merge sort whose scratch
/// space is reserved fallibly. Stability makes the permutation identical to the reference's.
pub(crate) fn stable_sort<T: Copy>(items: &mut [T], less: impl Fn(&T, &T) -> bool) -> Result<()> {
    let n = items.len();
    if n < 2 {
        return Ok(());
    }
    let mut scratch: Vec<T> = with_capacity(n)?;
    let mut width = 1usize;
    while width < n {
        scratch.clear();
        let mut start = 0usize;
        while start < n {
            let mid = start.saturating_add(width).min(n);
            let end = start.saturating_add(width.saturating_mul(2)).min(n);
            let (mut i, mut j) = (start, mid);
            while i < mid && j < end {
                // Take the right item first only when it is strictly less (stability).
                if less(&items[j], &items[i]) {
                    scratch.push(items[j]);
                    j += 1;
                } else {
                    scratch.push(items[i]);
                    i += 1;
                }
            }
            scratch.extend_from_slice(&items[i..mid]);
            scratch.extend_from_slice(&items[j..end]);
            start = end;
        }
        items.copy_from_slice(&scratch);
        width = width.saturating_mul(2);
    }
    Ok(())
}

// --- File-local helpers (lines 62-494) ----------------------------------------------------

/// `inline bool IsOdd(int val)`.
pub fn is_odd(val: i32) -> bool {
    (val & 1) != 0
}

/// `inline double GetDx(const Point64& pt1, const Point64& pt2)`.
///
/// ```text
/// Dx:                             0(90deg)
///                                  |
///               +inf (180deg) <--- o ---> -inf (0deg)
/// ```
pub fn get_dx(pt1: &Point64, pt2: &Point64) -> Result<f64> {
    let dy = sub64(pt2.y, pt1.y)? as f64;
    if dy != 0.0 {
        Ok(sub64(pt2.x, pt1.x)? as f64 / dy)
    } else if pt2.x > pt1.x {
        Ok(-MAX_DBL)
    } else {
        Ok(MAX_DBL)
    }
}

/// `inline int64_t TopX(const Active& ae, const int64_t currentY)`. "std::nearbyint (or
/// std::round) substantially *improves* performance here as it greatly improves the
/// likelihood of edge adjacency in ProcessIntersectList()."
pub fn top_x(ae: &Active, current_y: i64) -> Result<i64> {
    if current_y == ae.top.y || ae.top.x == ae.bot.x {
        Ok(ae.top.x)
    } else if current_y == ae.bot.y {
        Ok(ae.bot.x)
    } else {
        let offset = ae.dx * sub64(current_y, ae.bot.y)? as f64;
        let rounded = cast_f64_to_i64(nearbyint(offset), "TopX conversion to int64_t")?;
        add64(ae.bot.x, rounded)
    }
}

/// `inline bool IsHorizontal(const Active& e)`.
pub fn is_horizontal(e: &Active) -> bool {
    e.top.y == e.bot.y
}

/// `inline bool IsHeadingRightHorz(const Active& e)`.
pub fn is_heading_right_horz(e: &Active) -> bool {
    e.dx == -MAX_DBL
}

/// `inline bool IsHeadingLeftHorz(const Active& e)`.
pub fn is_heading_left_horz(e: &Active) -> bool {
    e.dx == MAX_DBL
}

/// `inline void SwapActives(Active*& e1, Active*& e2)`.
pub fn swap_actives(e1: &mut Option<ActiveId>, e2: &mut Option<ActiveId>) {
    ::core::mem::swap(e1, e2);
}

/// `inline bool IsMaxima(const Vertex& v)`.
pub fn is_maxima_vertex(v: &Vertex) -> bool {
    (v.flags & VertexFlags::LOCAL_MAX) != VertexFlags::EMPTY
}

/// `inline bool IsOpenEnd(const Vertex& v)`.
pub fn is_open_end_vertex(v: &Vertex) -> bool {
    (v.flags & (VertexFlags::OPEN_START | VertexFlags::OPEN_END)) != VertexFlags::EMPTY
}

/// `inline double AreaTriangle(pt1, pt2, pt3)`.
pub fn area_triangle(pt1: &Point64, pt2: &Point64, pt3: &Point64) -> Result<f64> {
    let a = add64(pt3.y, pt1.y)? as f64 * sub64(pt3.x, pt1.x)? as f64;
    let b = add64(pt1.y, pt2.y)? as f64 * sub64(pt1.x, pt2.x)? as f64;
    let c = add64(pt2.y, pt3.y)? as f64 * sub64(pt2.x, pt3.x)? as f64;
    Ok(a + b + c)
}

/// `inline bool PtsReallyClose(const Point64& pt1, const Point64& pt2)`: `std::llabs` of each
/// coordinate difference below 2.
pub fn pts_really_close(pt1: &Point64, pt2: &Point64) -> Result<bool> {
    let llabs = |v: i64| v.checked_abs().ok_or(I64_OVERFLOW);
    if llabs(sub64(pt1.x, pt2.x)?)? >= 2 {
        return Ok(false);
    }
    Ok(llabs(sub64(pt1.y, pt2.y)?)? < 2)
}

/// `inline bool IsJoined(const Active& e)`.
pub fn is_joined(e: &Active) -> bool {
    e.join_with != JoinWith::NoJoin
}

/// `inline bool IsHotEdge(const Active& e)`.
pub fn is_hot_edge(e: &Active) -> bool {
    e.outrec.is_some()
}

// --- StripeIndex (lines 496-627) ----------------------------------------------------------

/// The edges a `StripeIndex` builder collects.
pub type StripeEdgeList = Vec<StripeEdge>;

/// The edges of `StripeIndex::BuildFromPath`: each closing pair `path[j] -> path[i]` whose x
/// or y differ, for paths of at least three points.
pub(crate) fn stripe_edges_from_path(path: &Path64) -> Result<StripeEdgeList> {
    let mut edges = Vec::new();
    if path.len() >= 3 {
        reserve(&mut edges, path.len())?;
        let mut j = path.len() - 1;
        for (i, point) in path.iter().enumerate() {
            let prior = path.get(j).ok_or(NULL)?;
            if prior.x != point.x || prior.y != point.y {
                push(
                    &mut edges,
                    StripeEdge {
                        a: *prior,
                        b: *point,
                    },
                )?;
            }
            j = i;
        }
    }
    Ok(edges)
}

impl StripeIndex {
    /// `int YToStripe(int64_t y) const`.
    pub fn y_to_stripe(&self, y: i64) -> Result<i32> {
        let s = sub64(y, self.y_min)?
            .checked_div(self.stripe_height)
            .ok_or(ClipperError::Undefined("int64_t division"))?;
        if s < 0 {
            return Ok(0);
        }
        if s >= i64::from(self.stripe_count) {
            return sub32(self.stripe_count, 1);
        }
        i32::try_from(s).map_err(|_| INT_OVERFLOW)
    }

    /// `void Build(std::vector<StripeEdge>&& edges)`. "Decline rings whose edges each span
    /// many stripes (tall "comb" edges); indexing loses to brute force there. Empty edges_
    /// signals the fallback to callers."
    pub fn build(&mut self, edges: StripeEdgeList) -> Result<()> {
        self.edges = edges;
        self.stripes.clear();

        let Some(first) = self.edges.first() else {
            self.y_min = 0;
            self.stripe_height = 1;
            self.stripe_count = 1;
            return Ok(());
        };

        self.y_min = first.a.y;
        let mut y_max = first.a.y;
        for e in &self.edges {
            self.y_min = self.y_min.min(e.a.y.min(e.b.y));
            y_max = y_max.max(e.a.y.max(e.b.y));
        }

        // static_cast<int>(std::sqrt(static_cast<double>(edges_.size()))): sqrt is correctly
        // rounded; the truncation is undefined at or above 2^31.
        let root = (self.edges.len() as f64).sqrt();
        if root >= 2_147_483_648.0 {
            return Err(INT_OVERFLOW);
        }
        // if (stripe_count_ < 1) stripe_count_ = 1; if (stripe_count_ > 65536) ... = 65536;
        self.stripe_count = (root as i32).clamp(1, 65536);

        let y_range = sub64(y_max, self.y_min)?;
        self.stripe_height = if y_range <= 0 {
            1
        } else {
            sub64(add64(y_range, i64::from(self.stripe_count))?, 1)? / i64::from(self.stripe_count)
        };

        let mut total_refs: i64 = 0;
        for e in &self.edges {
            let span =
                self.y_to_stripe(e.a.y.max(e.b.y))? - self.y_to_stripe(e.a.y.min(e.b.y))? + 1;
            total_refs = add64(total_refs, i64::from(span))?;
        }

        let limit = i64::try_from(self.edges.len())
            .ok()
            .and_then(|n| n.checked_mul(8))
            .ok_or(I64_OVERFLOW)?;
        if total_refs > limit {
            self.edges.clear();
            return Ok(());
        }

        let count = usize::try_from(self.stripe_count).map_err(|_| INT_OVERFLOW)?;
        reserve(&mut self.stripes, count)?;
        self.stripes.resize_with(count, Vec::new);

        for i in 0..self.edges.len() {
            let e = self.edges[i];
            let s_min = self.y_to_stripe(e.a.y.min(e.b.y))?;
            let s_max = self.y_to_stripe(e.a.y.max(e.b.y))?;
            for s in s_min..=s_max {
                let stripe = usize::try_from(s)
                    .ok()
                    .and_then(|s| self.stripes.get_mut(s))
                    .ok_or(ClipperError::Undefined("stripe index out of range"))?;
                // static_cast<int32_t>(i): modular, as C++20 defines it.
                push(stripe, i as i32)?;
            }
        }
        Ok(())
    }

    /// `void BuildFromPath(const Path64& path)`.
    pub(crate) fn build_from_path_impl(&mut self, path: &Path64) -> Result<()> {
        let edges = stripe_edges_from_path(path)?;
        self.build(edges)
    }

    /// `void BuildFromRing(const OutPt* op)` on a ring of `base`.
    pub fn build_from_ring(&mut self, base: &ClipperBase, op: Option<OutPtId>) -> Result<()> {
        let edges = base.stripe_edges_from_ring(op)?;
        self.build(edges)
    }

    /// `PointInPolygonResult PointInPolygon(const Point64& pt) const`.
    pub(crate) fn point_in_polygon_impl(&self, pt: &Point64) -> Result<PointInPolygonResult> {
        if self.edges.is_empty() {
            return Ok(PointInPolygonResult::IsOutside);
        }
        let s = self.y_to_stripe(pt.y)?;
        let stripe = usize::try_from(s)
            .ok()
            .and_then(|s| self.stripes.get(s))
            .ok_or(ClipperError::Undefined("stripe index out of range"))?;
        let mut inside = false;
        for &idx in stripe {
            let e = usize::try_from(idx)
                .ok()
                .and_then(|i| self.edges.get(i))
                .ok_or(ClipperError::Undefined("edge index out of range"))?;
            let cross = cross_product_sign(&e.a, &e.b, pt)?;
            // "On-segment needs the bbox check; collinear (cross == 0) alone catches points on
            // the edge's infinite line but off the segment."
            if cross == 0
                && pt.x >= e.a.x.min(e.b.x)
                && pt.x <= e.a.x.max(e.b.x)
                && pt.y >= e.a.y.min(e.b.y)
                && pt.y <= e.a.y.max(e.b.y)
            {
                return Ok(PointInPolygonResult::IsOn);
            }
            // "Half-open crossing so horizontal edges never affect parity (IsOn only)."
            if (e.a.y > pt.y) != (e.b.y > pt.y) {
                let toggles = if e.b.y > pt.y { cross > 0 } else { cross < 0 };
                if toggles {
                    inside = !inside;
                }
            }
        }
        Ok(if inside {
            PointInPolygonResult::IsInside
        } else {
            PointInPolygonResult::IsOutside
        })
    }

    /// The private `edges_`.
    pub fn edges(&self) -> &[StripeEdge] {
        &self.edges
    }

    /// The private `stripes_`.
    pub fn stripes(&self) -> &[Vec<i32>] {
        &self.stripes
    }

    /// The private `y_min_`.
    pub fn y_min(&self) -> i64 {
        self.y_min
    }

    /// The private `stripe_height_`.
    pub fn stripe_height(&self) -> i64 {
        self.stripe_height
    }

    /// The private `stripe_count_`.
    pub fn stripe_count(&self) -> i32 {
        self.stripe_count
    }
}

// --- Vertex arrays, AddLocMin and AddPaths_ (lines 786-898) -----------------------------

/// One `new Vertex[total_vertex_count]` array. Its vertices are written while `AddPaths_`
/// builds it and only their flags change afterwards (`AddLocMin`). `alive` turns false when
/// the owning `ReuseableDataContainer64` frees the array while a `ClipperBase` still points
/// into it.
#[derive(Debug)]
pub struct VertexChunk {
    vertices: Vec<Cell<Vertex>>,
    alive: Cell<bool>,
}

impl VertexChunk {
    /// The array's length (`total_vertex_count` of its `AddPaths_` call).
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// Whether the array is empty (never, for an array `AddPaths_` allocated).
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Whether the array has not been freed.
    pub fn is_alive(&self) -> bool {
        self.alive.get()
    }

    /// Vertex `index`.
    pub fn get(&self, index: usize) -> Result<Vertex> {
        if !self.alive.get() {
            return Err(DANGLING_VERTEX);
        }
        self.vertices
            .get(index)
            .map(Cell::get)
            .ok_or(DANGLING_VERTEX)
    }

    fn set_flags(&self, index: usize, flags: VertexFlags) -> Result<()> {
        if !self.alive.get() {
            return Err(DANGLING_VERTEX);
        }
        let cell = self.vertices.get(index).ok_or(DANGLING_VERTEX)?;
        let mut vertex = cell.get();
        vertex.flags = flags;
        cell.set(vertex);
        Ok(())
    }
}

/// One `ClipperBase` vertex list: an array it allocated (`owned`, in `vertex_lists_`), or a
/// `ReuseableDataContainer64` array that `AddReuseableData` minima point into.
#[derive(Clone, Debug)]
pub struct VertexListSlot {
    pub(crate) chunk: Rc<VertexChunk>,
    pub(crate) owned: bool,
}

impl VertexListSlot {
    /// The array.
    pub fn chunk(&self) -> &Rc<VertexChunk> {
        &self.chunk
    }

    /// Whether `vertex_lists_` holds (and `delete[]`s) this array.
    pub fn owned(&self) -> bool {
        self.owned
    }
}

/// `void AddLocMin(LocalMinimaList& list, Vertex& vert, PathType polytype, bool is_open)`:
/// "make sure the vertex is added only once ..."
fn add_loc_min_to(
    list: &mut Vec<LocalMinima>,
    vertices: &mut [Vertex],
    vert: VertexId,
    polytype: PathType,
    is_open: bool,
) -> Result<()> {
    let v = vertices.get_mut(vert.index).ok_or(NULL)?;
    if (VertexFlags::LOCAL_MIN & v.flags) != VertexFlags::EMPTY {
        return Ok(());
    }
    v.flags = v.flags | VertexFlags::LOCAL_MIN;
    push(list, LocalMinima::new(vert, polytype, is_open))
}

/// `void AddPaths_(paths, polytype, is_open, vertexLists, locMinList)`: builds the array the
/// call would push onto `vertexLists` (as list number `list`) and the minima it would append
/// to `locMinList`, in order. `None` when the paths hold no vertex (nothing is allocated).
fn add_paths_(
    paths: &Paths64,
    polytype: PathType,
    is_open: bool,
    list: usize,
) -> Result<Option<(VertexChunk, Vec<LocalMinima>)>> {
    let mut total_vertex_count = 0usize;
    for path in paths {
        total_vertex_count = total_vertex_count
            .checked_add(path.len())
            .ok_or(ClipperError::Allocation)?;
    }
    if total_vertex_count == 0 {
        return Ok(None);
    }

    let id = |index: usize| VertexId { list, index };
    // `new Vertex[n]`: every member takes its default initializer.
    let mut vs: Vec<Vertex> = with_capacity(total_vertex_count)?;
    vs.resize(total_vertex_count, Vertex::default());
    let mut minima: Vec<LocalMinima> = Vec::new();
    let next =
        |vs: &[Vertex], i: usize| -> Result<usize> { Ok(some(vs.get(i).ok_or(NULL)?.next)?.index) };
    let prev =
        |vs: &[Vertex], i: usize| -> Result<usize> { Ok(some(vs.get(i).ok_or(NULL)?.prev)?.index) };
    let at = |vs: &[Vertex], i: usize| -> Result<Vertex> { vs.get(i).copied().ok_or(NULL) };

    let mut v = 0usize;
    for path in paths {
        // for each path create a circular double linked list of vertices
        let v0 = v;
        let mut curr_v = v;
        let mut prev_v: Option<usize> = None;

        if path.is_empty() {
            continue;
        }

        vs.get_mut(v).ok_or(NULL)?.prev = None;
        let mut cnt: i32 = 0;
        for pt in path {
            if let Some(p) = prev_v {
                if at(&vs, p)?.pt == *pt {
                    continue; // ie skips duplicates
                }
                vs.get_mut(p).ok_or(NULL)?.next = Some(id(curr_v));
            }
            let curr = vs.get_mut(curr_v).ok_or(NULL)?;
            curr.prev = prev_v.map(id);
            curr.pt = *pt;
            curr.flags = VertexFlags::EMPTY;
            prev_v = Some(curr_v);
            curr_v += 1;
            cnt = add32(cnt, 1)?;
        }
        let Some(mut last) = prev_v else { continue };
        if at(&vs, last)?.prev.is_none() {
            continue;
        }
        if !is_open && at(&vs, last)?.pt == at(&vs, v0)?.pt {
            last = prev(&vs, last)?;
        }
        vs.get_mut(last).ok_or(NULL)?.next = Some(id(v0));
        vs.get_mut(v0).ok_or(NULL)?.prev = Some(id(last));
        v = curr_v; // ie get ready for next path
        if cnt < 2 || (cnt == 2 && !is_open) {
            continue;
        }

        // now find and assign local minima
        let mut walk = Walk::new(total_vertex_count);
        let mut going_up;
        let y0 = at(&vs, v0)?.pt.y;
        if is_open {
            let mut cv = next(&vs, v0)?;
            while cv != v0 && at(&vs, cv)?.pt.y == y0 {
                walk.step()?;
                cv = next(&vs, cv)?;
            }
            going_up = at(&vs, cv)?.pt.y <= y0;
            if going_up {
                vs.get_mut(v0).ok_or(NULL)?.flags = VertexFlags::OPEN_START;
                add_loc_min_to(&mut minima, &mut vs, id(v0), polytype, true)?;
            } else {
                vs.get_mut(v0).ok_or(NULL)?.flags =
                    VertexFlags::OPEN_START | VertexFlags::LOCAL_MAX;
            }
        } else {
            // closed path
            let mut pv = prev(&vs, v0)?;
            while pv != v0 && at(&vs, pv)?.pt.y == y0 {
                walk.step()?;
                pv = prev(&vs, pv)?;
            }
            if pv == v0 {
                continue; // only open paths can be completely flat
            }
            going_up = at(&vs, pv)?.pt.y > y0;
        }

        let going_up0 = going_up;
        let mut pv = v0;
        let mut cv = next(&vs, v0)?;
        let mut walk = Walk::new(total_vertex_count);
        while cv != v0 {
            walk.step()?;
            let (cy, py) = (at(&vs, cv)?.pt.y, at(&vs, pv)?.pt.y);
            if cy > py && going_up {
                let p = vs.get_mut(pv).ok_or(NULL)?;
                p.flags = p.flags | VertexFlags::LOCAL_MAX;
                going_up = false;
            } else if cy < py && !going_up {
                going_up = true;
                add_loc_min_to(&mut minima, &mut vs, id(pv), polytype, is_open)?;
            }
            pv = cv;
            cv = next(&vs, cv)?;
        }

        if is_open {
            let p = vs.get_mut(pv).ok_or(NULL)?;
            p.flags = p.flags | VertexFlags::OPEN_END;
            if going_up {
                p.flags = p.flags | VertexFlags::LOCAL_MAX;
            } else {
                add_loc_min_to(&mut minima, &mut vs, id(pv), polytype, is_open)?;
            }
        } else if going_up != going_up0 {
            if going_up0 {
                add_loc_min_to(&mut minima, &mut vs, id(pv), polytype, false)?;
            } else {
                let p = vs.get_mut(pv).ok_or(NULL)?;
                p.flags = p.flags | VertexFlags::LOCAL_MAX;
            }
        }
    } // end processing current path

    let mut cells: Vec<Cell<Vertex>> = with_capacity(vs.len())?;
    cells.extend(vs.into_iter().map(Cell::new));
    Ok(Some((
        VertexChunk {
            vertices: cells,
            alive: Cell::new(true),
        },
        minima,
    )))
}

// --- ReuseableDataContainer64 (lines 900-929) ---------------------------------------------

/// `class ReuseableDataContainer64`: paths converted once to vertices and minima, which
/// several clippers can then share through `AddReuseableData`. "reuseable_data will continue
/// to own the vertices and remains responsible for their clean up."
#[derive(Debug, Default)]
pub struct ReuseableDataContainer64 {
    minima_list: Vec<LocalMinima>,
    vertex_lists: Vec<Rc<VertexChunk>>,
}

impl ReuseableDataContainer64 {
    /// `void AddLocMin(Vertex& vert, PathType polytype, bool is_open)` (private): "make sure
    /// the vertex is added only once ..."
    pub fn add_loc_min(&mut self, vert: VertexId, polytype: PathType, is_open: bool) -> Result<()> {
        let chunk = self.vertex_lists.get(vert.list).ok_or(DANGLING_VERTEX)?;
        let v = chunk.get(vert.index)?;
        if (VertexFlags::LOCAL_MIN & v.flags) != VertexFlags::EMPTY {
            return Ok(());
        }
        reserve(&mut self.minima_list, 1)?;
        chunk.set_flags(vert.index, v.flags | VertexFlags::LOCAL_MIN)?;
        self.minima_list
            .push(LocalMinima::new(vert, polytype, is_open));
        Ok(())
    }

    /// `void AddPaths(const Paths64& paths, PathType polytype, bool is_open)`.
    pub fn add_paths(&mut self, paths: &Paths64, polytype: PathType, is_open: bool) -> Result<()> {
        let list = self.vertex_lists.len();
        if let Some((chunk, minima)) = add_paths_(paths, polytype, is_open, list)? {
            reserve(&mut self.vertex_lists, 1)?;
            reserve(&mut self.minima_list, minima.len())?;
            self.minima_list.extend(minima);
            self.vertex_lists.push(Rc::new(chunk));
        }
        Ok(())
    }

    /// `void Clear()`: frees every vertex array. A `ClipperBase` whose minima still point into
    /// them holds dangling pointers, so its later reads of them are refused.
    pub fn clear(&mut self) {
        self.minima_list.clear();
        for chunk in &self.vertex_lists {
            chunk.alive.set(false);
        }
        self.vertex_lists.clear();
    }

    /// The private `minima_list_`.
    pub fn minima(&self) -> &[LocalMinima] {
        &self.minima_list
    }

    /// The private `vertex_lists_`.
    pub fn vertex_lists(&self) -> &[Rc<VertexChunk>] {
        &self.vertex_lists
    }
}

impl Drop for ReuseableDataContainer64 {
    /// `virtual ~ReuseableDataContainer64()`: `Clear()`.
    fn drop(&mut self) {
        self.clear();
    }
}

// --- ClipperBase ------------------------------------------------------------------------

/// Whether `e` is the front edge of its output record (`IsFront`'s comparison).
fn front_of(outrec: &OutRec, e: ActiveId) -> bool {
    outrec.front_edge == Some(e)
}

impl ClipperBase {
    // Record access. A null pointer is `None`; a deleted record is an empty slot.

    /// The `Active` record `id`.
    pub fn active(&self, id: ActiveId) -> Result<&Active> {
        self.active_edges
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(DELETED)
    }

    pub(crate) fn ae_mut(&mut self, id: ActiveId) -> Result<&mut Active> {
        self.active_edges
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(DELETED)
    }

    /// The `OutPt` record `id`.
    pub fn out_pt(&self, id: OutPtId) -> Result<&OutPt> {
        self.out_pts
            .get(id.0)
            .and_then(Option::as_ref)
            .ok_or(DELETED)
    }

    pub(crate) fn op_mut(&mut self, id: OutPtId) -> Result<&mut OutPt> {
        self.out_pts
            .get_mut(id.0)
            .and_then(Option::as_mut)
            .ok_or(DELETED)
    }

    /// The `OutRec` record `id` (`outrec_list_[id]`).
    pub fn outrec(&self, id: OutRecId) -> Result<&OutRec> {
        self.outrec_list.get(id.0).ok_or(DELETED)
    }

    pub(crate) fn or_mut(&mut self, id: OutRecId) -> Result<&mut OutRec> {
        self.outrec_list.get_mut(id.0).ok_or(DELETED)
    }

    /// The `LocalMinima` record `id`.
    pub fn local_minima(&self, id: LocalMinimaId) -> Result<&LocalMinima> {
        self.minima_store.get(id.0).ok_or(DELETED)
    }

    /// The vertex `id` points to.
    pub fn vertex(&self, id: VertexId) -> Result<Vertex> {
        self.vertex_lists
            .get(id.list)
            .ok_or(DANGLING_VERTEX)?
            .chunk
            .get(id.index)
    }

    /// `minima_list_`, in its current order.
    pub fn minima_list(&self) -> &[LocalMinimaId] {
        &self.minima_list
    }

    /// The vertex list slots (`vertex_lists_` and the borrowed arrays).
    pub fn vertex_list_slots(&self) -> &[VertexListSlot] {
        &self.vertex_lists
    }

    /// `current_locmin_iter_` as an index into `minima_list_`.
    pub fn current_locmin_iter(&self) -> usize {
        self.current_locmin_iter
    }

    /// `actives_`.
    pub fn actives_head(&self) -> Option<ActiveId> {
        self.actives
    }

    /// `sel_`.
    pub fn sel_head(&self) -> Option<ActiveId> {
        self.sel
    }

    /// `outrec_list_`.
    pub fn outrecs(&self) -> &[OutRec] {
        &self.outrec_list
    }

    /// The contents of `scanline_list_` in pop order (largest first, duplicates kept).
    pub fn scanlines(&self) -> Result<Vec<i64>> {
        let mut values: Vec<i64> = with_capacity(self.scanline_list.len())?;
        values.extend(self.scanline_list.iter().copied());
        values.sort_unstable_by(|a, b| b.cmp(a));
        Ok(values)
    }

    /// `succeeded_`.
    pub fn succeeded(&self) -> bool {
        self.succeeded
    }

    /// `has_open_paths_`.
    pub fn has_open_paths(&self) -> bool {
        self.has_open_paths
    }

    /// `minima_list_sorted_`.
    pub fn minima_list_sorted(&self) -> bool {
        self.minima_list_sorted
    }

    /// Set the private `cliptype_` (assigned by `ExecuteInternal`).
    pub fn set_clip_type(&mut self, value: ClipType) {
        self.cliptype = value;
    }

    /// Set the private `fillrule_` (assigned by `ExecuteInternal`).
    pub fn set_fill_rule(&mut self, value: FillRule) {
        self.fillrule = value;
    }

    /// Set the private `using_polytree_` (assigned by `ExecuteInternal`).
    pub fn set_using_polytree(&mut self, value: bool) {
        self.using_polytree = value;
    }

    /// Set the protected `zCallback_`.
    pub fn set_z_callback_slot(&mut self, slot: ZCallbackSlot) {
        self.z_callback = slot;
    }

    /// Whether the protected `zCallback_` is non-empty (`if (zCallback_)`).
    pub fn has_z_callback(&self) -> bool {
        !matches!(self.z_callback, ZCallbackSlot::Empty)
    }

    /// Invoke the protected `zCallback_`; an empty `std::function` throws
    /// `std::bad_function_call`. `ClipperD`'s bound proxy runs its `ZCB`.
    pub fn call_z_callback(
        &mut self,
        e1bot: &Point64,
        e1top: &Point64,
        e2bot: &Point64,
        e2top: &Point64,
        pt: &mut Point64,
    ) -> Result<()> {
        match &mut self.z_callback {
            ZCallbackSlot::Empty => Err(ClipperError::BadFunctionCall),
            ZCallbackSlot::User(callback) => {
                callback(e1bot, e1top, e2bot, e2top, pt);
                Ok(())
            }
            ZCallbackSlot::ClipperDProxy {
                inv_scale,
                callback,
            } => crate::clipper_engine::clipper_d_zcb(
                *inv_scale, callback, e1bot, e1top, e2bot, e2top, pt,
            ),
            ZCallbackSlot::OffsetProxy { callback } => {
                crate::offset::offset_zcb(callback, e1bot, e1top, e2bot, e2top, pt)
            }
        }
    }

    /// The `Active` record `id`, mutably (its fields are public in the C++).
    pub fn active_mut(&mut self, id: ActiveId) -> Result<&mut Active> {
        self.ae_mut(id)
    }

    /// The `OutPt` record `id`, mutably (its fields are public in the C++).
    pub fn out_pt_mut(&mut self, id: OutPtId) -> Result<&mut OutPt> {
        self.op_mut(id)
    }

    /// The `OutRec` record `id`, mutably (its fields are public in the C++).
    pub fn outrec_mut(&mut self, id: OutRecId) -> Result<&mut OutRec> {
        self.or_mut(id)
    }

    fn new_active(&mut self, e: Active) -> Result<ActiveId> {
        let id = ActiveId(self.active_edges.len());
        push(&mut self.active_edges, Some(e))?;
        Ok(id)
    }

    /// `new OutPt(pt, outrec)`: `next = prev = this`.
    pub fn new_out_pt(&mut self, pt: &Point64, outrec: Option<OutRecId>) -> Result<OutPtId> {
        let id = OutPtId(self.out_pts.len());
        push(&mut self.out_pts, Some(OutPt::new(id, pt, outrec)))?;
        Ok(id)
    }

    /// `delete op`.
    pub(crate) fn delete_out_pt(&mut self, id: OutPtId) -> Result<()> {
        let slot = self.out_pts.get_mut(id.0).ok_or(DELETED)?;
        if slot.take().is_none() {
            return Err(DELETED);
        }
        Ok(())
    }

    pub(crate) fn ael_walk(&self) -> Walk {
        Walk::new(self.active_edges.len())
    }

    pub(crate) fn ring_walk(&self) -> Walk {
        Walk::new(self.out_pts.len())
    }

    pub(crate) fn owner_walk(&self) -> Walk {
        Walk::new(self.outrec_list.len())
    }

    pub(crate) fn lm_of(&self, e: ActiveId) -> Result<&LocalMinima> {
        self.local_minima(some(self.active(e)?.local_min)?)
    }

    pub(crate) fn outrec_of(&self, e: ActiveId) -> Result<OutRecId> {
        some(self.active(e)?.outrec)
    }

    // --- file-local helpers that take engine records (lines 68-494) ---------------------

    /// `inline bool IsOpen(const Active& e)`.
    pub fn is_open(&self, e: ActiveId) -> Result<bool> {
        Ok(self.lm_of(e)?.is_open)
    }

    /// `inline bool IsOpenEnd(const Active& ae)`.
    pub fn is_open_end(&self, ae: ActiveId) -> Result<bool> {
        Ok(is_open_end_vertex(
            &self.vertex(some(self.active(ae)?.vertex_top)?)?,
        ))
    }

    /// `inline Active* GetPrevHotEdge(const Active& e)`.
    pub fn get_prev_hot_edge(&self, e: ActiveId) -> Result<Option<ActiveId>> {
        let mut prev = self.active(e)?.prev_in_ael;
        let mut walk = self.ael_walk();
        while let Some(p) = prev {
            if !(self.is_open(p)? || !is_hot_edge(self.active(p)?)) {
                break;
            }
            walk.step()?;
            prev = self.active(p)?.prev_in_ael;
        }
        Ok(prev)
    }

    /// `inline bool IsFront(const Active& e)`.
    pub fn is_front(&self, e: ActiveId) -> Result<bool> {
        Ok(front_of(self.outrec(self.outrec_of(e)?)?, e))
    }

    /// `inline bool IsInvalidPath(OutPt* op)`.
    pub fn is_invalid_path(&self, op: Option<OutPtId>) -> Result<bool> {
        match op {
            None => Ok(true),
            Some(op) => Ok(self.out_pt(op)?.next == op),
        }
    }

    /// `inline PathType GetPolyType(const Active& e)`.
    pub fn get_poly_type(&self, e: ActiveId) -> Result<PathType> {
        Ok(self.lm_of(e)?.polytype)
    }

    /// `inline bool IsSamePolyType(const Active& e1, const Active& e2)`.
    pub fn is_same_poly_type(&self, e1: ActiveId, e2: ActiveId) -> Result<bool> {
        Ok(self.lm_of(e1)?.polytype == self.lm_of(e2)?.polytype)
    }

    /// `inline void SetDx(Active& e)`.
    pub fn set_dx(&mut self, e: ActiveId) -> Result<()> {
        let a = self.active(e)?;
        let dx = get_dx(&a.bot, &a.top)?;
        self.ae_mut(e)?.dx = dx;
        Ok(())
    }

    /// `inline Vertex* NextVertex(const Active& e)`.
    pub fn next_vertex(&self, e: ActiveId) -> Result<Option<VertexId>> {
        let a = self.active(e)?;
        let top_id = some(a.vertex_top)?;
        let top = self.vertex(top_id)?;
        Ok(rebind(
            top_id,
            if a.wind_dx > 0 { top.next } else { top.prev },
        ))
    }

    /// `inline Vertex* PrevPrevVertex(const Active& ae)`: "useful to get the (inverted
    /// Y-axis) top of the alternate edge (ie left or right bound) during edge insertion."
    pub fn prev_prev_vertex(&self, ae: ActiveId) -> Result<Option<VertexId>> {
        let a = self.active(ae)?;
        let top_id = some(a.vertex_top)?;
        let top = self.vertex(top_id)?;
        if a.wind_dx > 0 {
            let prev = some(rebind(top_id, top.prev))?;
            Ok(rebind(prev, self.vertex(prev)?.prev))
        } else {
            let next = some(rebind(top_id, top.next))?;
            Ok(rebind(next, self.vertex(next)?.next))
        }
    }

    /// `inline Active* ExtractFromSEL(Active* ae)`.
    pub fn extract_from_sel(&mut self, ae: ActiveId) -> Result<Option<ActiveId>> {
        let res = self.active(ae)?.next_in_sel;
        let ae_prev = self.active(ae)?.prev_in_sel;
        if let Some(r) = res {
            self.ae_mut(r)?.prev_in_sel = ae_prev;
        }
        self.ae_mut(some(self.active(ae)?.prev_in_sel)?)?
            .next_in_sel = res;
        Ok(res)
    }

    /// `inline void Insert1Before2InSEL(Active* ae1, Active* ae2)`.
    pub fn insert1_before2_in_sel(&mut self, ae1: ActiveId, ae2: ActiveId) -> Result<()> {
        let ae2_prev = self.active(ae2)?.prev_in_sel;
        self.ae_mut(ae1)?.prev_in_sel = ae2_prev;
        if let Some(p) = self.active(ae1)?.prev_in_sel {
            self.ae_mut(p)?.next_in_sel = Some(ae1);
        }
        self.ae_mut(ae1)?.next_in_sel = Some(ae2);
        self.ae_mut(ae2)?.prev_in_sel = Some(ae1);
        Ok(())
    }

    /// `inline bool IsMaxima(const Active& e)`.
    pub fn is_maxima(&self, e: ActiveId) -> Result<bool> {
        Ok(is_maxima_vertex(
            &self.vertex(some(self.active(e)?.vertex_top)?)?,
        ))
    }

    /// `inline Vertex* GetCurrYMaximaVertex_Open(const Active& e)`.
    pub fn get_curr_y_maxima_vertex_open(&self, e: ActiveId) -> Result<Option<VertexId>> {
        let a = self.active(e)?;
        let mut result = some(a.vertex_top)?;
        let stop = VertexFlags::OPEN_END | VertexFlags::LOCAL_MAX;
        let mut walk = Walk::new(
            self.vertex_lists
                .get(result.list)
                .map_or(0, |s| s.chunk.len()),
        );
        loop {
            let r = self.vertex(result)?;
            let s = some(rebind(result, if a.wind_dx > 0 { r.next } else { r.prev }))?;
            if !(self.vertex(s)?.pt.y == r.pt.y && (r.flags & stop) == VertexFlags::EMPTY) {
                break;
            }
            walk.step()?;
            result = s;
        }
        if !is_maxima_vertex(&self.vertex(result)?) {
            return Ok(None); // not a maxima
        }
        Ok(Some(result))
    }

    /// `inline Vertex* GetCurrYMaximaVertex(const Active& e)`.
    pub fn get_curr_y_maxima_vertex(&self, e: ActiveId) -> Result<Option<VertexId>> {
        let a = self.active(e)?;
        let mut result = some(a.vertex_top)?;
        let mut walk = Walk::new(
            self.vertex_lists
                .get(result.list)
                .map_or(0, |s| s.chunk.len()),
        );
        loop {
            let r = self.vertex(result)?;
            let s = some(rebind(result, if a.wind_dx > 0 { r.next } else { r.prev }))?;
            if self.vertex(s)?.pt.y != r.pt.y {
                break;
            }
            walk.step()?;
            result = s;
        }
        if !is_maxima_vertex(&self.vertex(result)?) {
            return Ok(None); // not a maxima
        }
        Ok(Some(result))
    }

    /// `Active* GetMaximaPair(const Active& e)`.
    pub fn get_maxima_pair(&self, e: ActiveId) -> Result<Option<ActiveId>> {
        let top = self.active(e)?.vertex_top;
        let mut e2 = self.active(e)?.next_in_ael;
        let mut walk = self.ael_walk();
        while let Some(x) = e2 {
            if self.active(x)?.vertex_top == top {
                return Ok(Some(x)); // Found!
            }
            walk.step()?;
            e2 = self.active(x)?.next_in_ael;
        }
        Ok(None)
    }

    /// `inline int PointCount(OutPt* op)`.
    pub fn point_count(&self, op: OutPtId) -> Result<i32> {
        let mut op2 = op;
        let mut cnt: i32 = 0;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            op2 = self.out_pt(op2)?.next;
            cnt = add32(cnt, 1)?;
            if op2 == op {
                return Ok(cnt);
            }
        }
    }

    /// `inline OutPt* DuplicateOp(OutPt* op, bool insert_after)`.
    pub fn duplicate_op(&mut self, op: OutPtId, insert_after: bool) -> Result<OutPtId> {
        let o = *self.out_pt(op)?;
        let result = self.new_out_pt(&o.pt, o.outrec)?;
        if insert_after {
            let op_next = self.out_pt(op)?.next;
            self.op_mut(result)?.next = op_next;
            self.op_mut(op_next)?.prev = result;
            self.op_mut(result)?.prev = op;
            self.op_mut(op)?.next = result;
        } else {
            let op_prev = self.out_pt(op)?.prev;
            self.op_mut(result)?.prev = op_prev;
            self.op_mut(op_prev)?.next = result;
            self.op_mut(result)?.next = op;
            self.op_mut(op)?.prev = result;
        }
        Ok(result)
    }

    /// `inline OutPt* DisposeOutPt(OutPt* op)`.
    pub fn dispose_out_pt(&mut self, op: OutPtId) -> Result<OutPtId> {
        let o = *self.out_pt(op)?;
        let result = o.next;
        self.op_mut(o.prev)?.next = o.next;
        let prev = self.out_pt(op)?.prev;
        self.op_mut(self.out_pt(op)?.next)?.prev = prev;
        self.delete_out_pt(op)?;
        Ok(result)
    }

    /// `inline void DisposeOutPts(OutRec* outrec)`: deletes the ring from `outrec->pts` on.
    pub fn dispose_out_pts(&mut self, outrec: OutRecId) -> Result<()> {
        let start = some(self.outrec(outrec)?.pts)?;
        // The reference breaks the ring before its start (`op->prev->next = nullptr`) and
        // deletes forward to the break: collect that ring, then delete it.
        let mut ring = Vec::new();
        let mut op = start;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            push(&mut ring, op)?;
            op = self.out_pt(op)?.next;
            if op == start {
                break;
            }
        }
        for op in ring {
            self.delete_out_pt(op)?;
        }
        self.or_mut(outrec)?.pts = None;
        Ok(())
    }

    /// `inline void SetSides(OutRec& outrec, Active& start_edge, Active& end_edge)`.
    pub fn set_sides(
        &mut self,
        outrec: OutRecId,
        start_edge: ActiveId,
        end_edge: ActiveId,
    ) -> Result<()> {
        let rec = self.or_mut(outrec)?;
        rec.front_edge = Some(start_edge);
        rec.back_edge = Some(end_edge);
        Ok(())
    }

    /// `void SwapOutrecs(Active& e1, Active& e2)`.
    pub fn swap_outrecs(&mut self, e1: ActiveId, e2: ActiveId) -> Result<()> {
        let or1 = self.active(e1)?.outrec;
        let or2 = self.active(e2)?.outrec;
        if or1 == or2 {
            let rec = self.or_mut(some(or1)?)?;
            ::core::mem::swap(&mut rec.front_edge, &mut rec.back_edge);
            return Ok(());
        }
        if let Some(or1) = or1 {
            let rec = self.or_mut(or1)?;
            if front_of(rec, e1) {
                rec.front_edge = Some(e2);
            } else {
                rec.back_edge = Some(e2);
            }
        }
        if let Some(or2) = or2 {
            let rec = self.or_mut(or2)?;
            if front_of(rec, e2) {
                rec.front_edge = Some(e1);
            } else {
                rec.back_edge = Some(e1);
            }
        }
        self.ae_mut(e1)?.outrec = or2;
        self.ae_mut(e2)?.outrec = or1;
        Ok(())
    }

    /// `double Area(OutPt* op)`: the shoelace formula over the ring.
    pub fn area_of(&self, op: OutPtId) -> Result<f64> {
        let mut result = 0.0;
        let mut op2 = op;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            let o = self.out_pt(op2)?;
            let p = self.out_pt(o.prev)?;
            result += add64(p.pt.y, o.pt.y)? as f64 * sub64(p.pt.x, o.pt.x)? as f64;
            op2 = o.next;
            if op2 == op {
                break;
            }
        }
        Ok(result * 0.5)
    }

    /// `void ReverseOutPts(OutPt* op)`.
    pub fn reverse_out_pts(&mut self, op: Option<OutPtId>) -> Result<()> {
        let Some(op) = op else { return Ok(()) };
        let mut op1 = op;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            let o = self.op_mut(op1)?;
            let op2 = o.next;
            ::core::mem::swap(&mut o.next, &mut o.prev);
            op1 = op2;
            if op1 == op {
                return Ok(());
            }
        }
    }

    /// `inline void SwapSides(OutRec& outrec)`.
    pub fn swap_sides(&mut self, outrec: OutRecId) -> Result<()> {
        let pts = some(self.outrec(outrec)?.pts)?;
        let next = self.out_pt(pts)?.next;
        let rec = self.or_mut(outrec)?;
        ::core::mem::swap(&mut rec.front_edge, &mut rec.back_edge);
        rec.pts = Some(next);
        Ok(())
    }

    /// `inline OutRec* GetRealOutRec(OutRec* outrec)`.
    pub fn get_real_outrec(&self, outrec: Option<OutRecId>) -> Result<Option<OutRecId>> {
        let mut outrec = outrec;
        let mut walk = self.owner_walk();
        while let Some(o) = outrec {
            let rec = self.outrec(o)?;
            if rec.pts.is_some() {
                break;
            }
            walk.step()?;
            outrec = rec.owner;
        }
        Ok(outrec)
    }

    /// `inline bool IsValidOwner(OutRec* outrec, OutRec* testOwner)`: "prevent outrec owning
    /// itself either directly or indirectly".
    pub fn is_valid_owner(
        &self,
        outrec: Option<OutRecId>,
        test_owner: Option<OutRecId>,
    ) -> Result<bool> {
        let mut test_owner = test_owner;
        let mut walk = self.owner_walk();
        while let Some(t) = test_owner {
            if Some(t) == outrec {
                break;
            }
            walk.step()?;
            test_owner = self.outrec(t)?.owner;
        }
        Ok(test_owner.is_none())
    }

    /// `inline void UncoupleOutRec(Active ae)` (a copy of the edge).
    pub fn uncouple_outrec(&mut self, ae: ActiveId) -> Result<()> {
        let Some(outrec) = self.active(ae)?.outrec else {
            return Ok(());
        };
        let front = some(self.outrec(outrec)?.front_edge)?;
        self.ae_mut(front)?.outrec = None;
        let back = some(self.outrec(outrec)?.back_edge)?;
        self.ae_mut(back)?.outrec = None;
        let rec = self.or_mut(outrec)?;
        rec.front_edge = None;
        rec.back_edge = None;
        Ok(())
    }

    /// `inline bool IsVerySmallTriangle(const OutPt& op)`.
    pub fn is_very_small_triangle(&self, op: OutPtId) -> Result<bool> {
        let o = self.out_pt(op)?;
        let next = self.out_pt(o.next)?;
        if next.next != o.prev {
            return Ok(false);
        }
        let prev = self.out_pt(o.prev)?;
        Ok(pts_really_close(&prev.pt, &next.pt)?
            || pts_really_close(&o.pt, &next.pt)?
            || pts_really_close(&o.pt, &prev.pt)?)
    }

    /// `inline bool IsValidClosedPath(const OutPt* op)`.
    pub fn is_valid_closed_path(&self, op: Option<OutPtId>) -> Result<bool> {
        let Some(op) = op else { return Ok(false) };
        let o = self.out_pt(op)?;
        Ok(o.next != op && o.next != o.prev && !self.is_very_small_triangle(op)?)
    }

    /// `inline bool OutrecIsAscending(const Active* hotEdge)`.
    pub fn outrec_is_ascending(&self, hot_edge: ActiveId) -> Result<bool> {
        self.is_front(hot_edge)
    }

    /// `inline void SwapFrontBackSides(OutRec& outrec)`.
    pub fn swap_front_back_sides(&mut self, outrec: OutRecId) -> Result<()> {
        self.swap_sides(outrec)
    }

    /// `inline bool EdgesAdjacentInAEL(const IntersectNode& inode)`.
    pub fn edges_adjacent_in_ael(&self, inode: &IntersectNode) -> Result<bool> {
        let e1 = self.active(some(inode.edge1)?)?;
        Ok(e1.next_in_ael == inode.edge2 || e1.prev_in_ael == inode.edge2)
    }

    /// `inline void SetOwner(OutRec* outrec, OutRec* new_owner)`: "precondition1: new_owner is
    /// never null".
    pub fn set_owner(&mut self, outrec: OutRecId, new_owner: OutRecId) -> Result<()> {
        let owner = self.get_real_outrec(self.outrec(new_owner)?.owner)?;
        self.or_mut(new_owner)?.owner = owner;
        let mut tmp = Some(new_owner);
        let mut walk = self.owner_walk();
        while let Some(t) = tmp {
            if t == outrec {
                break;
            }
            walk.step()?;
            tmp = self.outrec(t)?.owner;
        }
        if tmp.is_some() {
            let owner = self.outrec(outrec)?.owner;
            self.or_mut(new_owner)?.owner = owner;
        }
        self.or_mut(outrec)?.owner = Some(new_owner);
        Ok(())
    }

    // --- point in polygon on output rings (lines 629-784) ---------------------------------

    /// The edges of `StripeIndex::BuildFromRing(op)`.
    pub(crate) fn stripe_edges_from_ring(&self, op: Option<OutPtId>) -> Result<StripeEdgeList> {
        let mut edges = Vec::new();
        let Some(op) = op else { return Ok(edges) };
        let o = self.out_pt(op)?;
        if o.next != op && o.prev != o.next {
            let mut cur = op;
            let mut walk = self.ring_walk();
            loop {
                walk.step()?;
                let c = self.out_pt(cur)?;
                let nxt = c.next;
                let n = self.out_pt(nxt)?;
                if c.pt.x != n.pt.x || c.pt.y != n.pt.y {
                    push(&mut edges, StripeEdge { a: c.pt, b: n.pt })?;
                }
                cur = nxt;
                if cur == op {
                    break;
                }
            }
        }
        Ok(edges)
    }

    /// `static PointInPolygonResult PointInOpPolygon(const Point64& pt, OutPt* op)`.
    pub fn point_in_op_polygon(&self, pt: &Point64, op: OutPtId) -> Result<PointInPolygonResult> {
        let mut op = op;
        {
            let o = self.out_pt(op)?;
            if op == o.next || o.prev == o.next {
                return Ok(PointInPolygonResult::IsOutside);
            }
        }

        let mut walk = Walk::new(self.out_pts.len().saturating_mul(2));
        let mut op2 = op;
        loop {
            walk.step()?;
            if self.out_pt(op)?.pt.y != pt.y {
                break;
            }
            op = self.out_pt(op)?.next;
            if op == op2 {
                break;
            }
        }
        if self.out_pt(op)?.pt.y == pt.y {
            // not a proper polygon
            return Ok(PointInPolygonResult::IsOutside);
        }

        let mut is_above = self.out_pt(op)?.pt.y < pt.y;
        let starting_above = is_above;
        let mut val: i32 = 0;
        op2 = self.out_pt(op)?.next;
        while op2 != op {
            walk.step()?;
            if is_above {
                while op2 != op && self.out_pt(op2)?.pt.y < pt.y {
                    walk.step()?;
                    op2 = self.out_pt(op2)?.next;
                }
            } else {
                while op2 != op && self.out_pt(op2)?.pt.y > pt.y {
                    walk.step()?;
                    op2 = self.out_pt(op2)?.next;
                }
            }
            if op2 == op {
                break;
            }

            // must have touched or crossed the pt.Y horizontal
            // and this must happen an even number of times
            let o2 = *self.out_pt(op2)?;
            let p2 = self.out_pt(o2.prev)?.pt;
            if o2.pt.y == pt.y {
                // touching the horizontal
                if o2.pt.x == pt.x || (o2.pt.y == p2.y && (pt.x < p2.x) != (pt.x < o2.pt.x)) {
                    return Ok(PointInPolygonResult::IsOn);
                }
                op2 = o2.next;
                if op2 == op {
                    break;
                }
                continue;
            }

            if pt.x < o2.pt.x && pt.x < p2.x {
                // do nothing because
                // we're only interested in edges crossing on the left
            } else if pt.x > p2.x && pt.x > o2.pt.x {
                val = 1 - val; // toggle val
            } else {
                let i = cross_product_sign(&p2, &o2.pt, pt)?;
                if i == 0 {
                    return Ok(PointInPolygonResult::IsOn);
                }
                if (i < 0) == is_above {
                    val = 1 - val;
                }
            }
            is_above = !is_above;
            op2 = o2.next;
        }

        if is_above != starting_above {
            let o2 = self.out_pt(op2)?;
            let p2 = self.out_pt(o2.prev)?.pt;
            let i = cross_product_sign(&p2, &o2.pt, pt)?;
            if i == 0 {
                return Ok(PointInPolygonResult::IsOn);
            }
            if (i < 0) == is_above {
                val = 1 - val;
            }
        }

        if val == 0 {
            Ok(PointInPolygonResult::IsOutside)
        } else {
            Ok(PointInPolygonResult::IsInside)
        }
    }

    /// `inline Path64 GetCleanPath(OutPt* op)`.
    pub fn get_clean_path(&self, op: OutPtId) -> Result<Path64> {
        let mut result = Path64::new();
        let mut op2 = op;
        let mut walk = Walk::new(self.out_pts.len().saturating_mul(2));
        loop {
            let o = self.out_pt(op2)?;
            if o.next == op {
                break;
            }
            let (n, p) = (self.out_pt(o.next)?.pt, self.out_pt(o.prev)?.pt);
            if !((o.pt.x == n.x && o.pt.x == p.x) || (o.pt.y == n.y && o.pt.y == p.y)) {
                break;
            }
            walk.step()?;
            op2 = o.next;
        }
        push(&mut result, self.out_pt(op2)?.pt)?;
        let mut prev_op = op2;
        op2 = self.out_pt(op2)?.next;
        while op2 != op {
            walk.step()?;
            let o = self.out_pt(op2)?;
            let n = self.out_pt(o.next)?.pt;
            let p = self.out_pt(prev_op)?.pt;
            if (o.pt.x != n.x || o.pt.x != p.x) && (o.pt.y != n.y || o.pt.y != p.y) {
                push(&mut result, o.pt)?;
                prev_op = op2;
            }
            op2 = o.next;
        }
        Ok(result)
    }

    /// `inline bool Path2ContainsPath1(OutPt* op1, OutPt* op2)`: "this function accommodates
    /// rounding errors that can cause path micro intersections".
    pub fn path2_contains_path1(&self, op1: OutPtId, op2: Option<OutPtId>) -> Result<bool> {
        let mut pip = PointInPolygonResult::IsOn;
        let mut op = op1;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            let pt = self.out_pt(op)?.pt;
            match self.point_in_op_polygon(&pt, some(op2)?)? {
                PointInPolygonResult::IsOutside => {
                    if pip == PointInPolygonResult::IsOutside {
                        return Ok(false);
                    }
                    pip = PointInPolygonResult::IsOutside;
                }
                PointInPolygonResult::IsInside => {
                    if pip == PointInPolygonResult::IsInside {
                        return Ok(true);
                    }
                    pip = PointInPolygonResult::IsInside;
                }
                PointInPolygonResult::IsOn => {}
            }
            op = self.out_pt(op)?.next;
            if op == op1 {
                break;
            }
        }
        // result unclear, so try again using cleaned paths
        let path1 = self.get_clean_path(op1)?;
        let path2 = self.get_clean_path(some(op2)?)?;
        path2_contains_path1_paths(&path1, &path2) // (#973)
    }

    /// `static bool Path2ContainsPath1Indexed(OutPt* op1, OutRec* owner, size_t
    /// index_threshold)`: "Path2ContainsPath1 variant using a StripeIndex cached on the
    /// owner. Small owners and the ambiguity fallback defer to the brute-force overload, so
    /// results match."
    pub fn path2_contains_path1_indexed(
        &mut self,
        op1: OutPtId,
        owner: OutRecId,
        index_threshold: u64,
    ) -> Result<bool> {
        let (path_size, owner_pts) = {
            let rec = self.outrec(owner)?;
            (rec.path.len(), rec.pts)
        };
        if (path_size as u64) <= index_threshold {
            return self.path2_contains_path1(op1, owner_pts);
        }

        if self.outrec(owner)?.pip_index.is_none() {
            // "Build into a local first so a throw cannot cache a partial index."
            let mut index = Box::new(StripeIndex::default());
            index.build_from_ring(self, owner_pts)?;
            self.or_mut(owner)?.pip_index = Some(index);
        }

        // "Empty means the index declined this ring (see StripeIndex::Build)."
        if some(self.outrec(owner)?.pip_index.as_ref())?.empty() {
            return self.path2_contains_path1(op1, owner_pts);
        }

        let mut pip = PointInPolygonResult::IsOn;
        let mut op = op1;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            let pt = self.out_pt(op)?.pt;
            let index = some(self.outrec(owner)?.pip_index.as_ref())?;
            match index.point_in_polygon(&pt)? {
                PointInPolygonResult::IsOutside => {
                    if pip == PointInPolygonResult::IsOutside {
                        return Ok(false);
                    }
                    pip = PointInPolygonResult::IsOutside;
                }
                PointInPolygonResult::IsInside => {
                    if pip == PointInPolygonResult::IsInside {
                        return Ok(true);
                    }
                    pip = PointInPolygonResult::IsInside;
                }
                PointInPolygonResult::IsOn => {}
            }
            op = self.out_pt(op)?.next;
            if op == op1 {
                break;
            }
        }

        // ambiguous, defer to the exact cleaned-path test
        let path1 = self.get_clean_path(op1)?;
        let path2 = self.get_clean_path(some(self.outrec(owner)?.pts)?)?;
        path2_contains_path1_paths(&path1, &path2) // (#973)
    }

    // --- ClipperBase methods (lines 935-1091) ----------------------------------------------

    /// `void DeleteEdges(Active*& e)`: deletes the list from `e` on, leaving `e` null.
    pub fn delete_edges(&mut self, e: &mut Option<ActiveId>) -> Result<()> {
        let mut walk = self.ael_walk();
        while let Some(e2) = *e {
            walk.step()?;
            *e = self.active(e2)?.next_in_ael;
            let slot = self.active_edges.get_mut(e2.0).ok_or(DELETED)?;
            *slot = None;
        }
        Ok(())
    }

    /// `void CleanUp()`: "unlike Clear, CleanUp preserves added paths".
    pub fn clean_up(&mut self) -> Result<()> {
        let mut actives = self.actives;
        let result = self.delete_edges(&mut actives);
        self.actives = actives;
        result?;
        self.scanline_list.clear();
        self.intersect_nodes.clear();
        self.dispose_all_outrecs()?;
        self.horz_seg_list.clear();
        self.horz_join_list.clear();
        // Every edge is deleted: release the arena (no live handle remains).
        if self.active_edges.iter().all(Option::is_none) {
            self.active_edges.clear();
        }
        Ok(())
    }

    /// `void Clear()`.
    pub fn clear(&mut self) -> Result<()> {
        self.clean_up()?;
        self.dispose_vertices_and_local_minima();
        self.current_locmin_iter = 0;
        self.minima_list_sorted = false;
        self.has_open_paths = false;
        Ok(())
    }

    /// `void Reset()`.
    pub fn reset(&mut self) -> Result<()> {
        if !self.minima_list_sorted {
            // std::stable_sort(minima_list_, LocMinSorter()) (#594), on the vertices' points.
            let mut keyed: Vec<(Point64, LocalMinimaId)> = with_capacity(self.minima_list.len())?;
            for &id in &self.minima_list {
                keyed.push((self.vertex(self.local_minima(id)?.vertex)?.pt, id));
            }
            stable_sort(&mut keyed, |a, b| loc_min_sorter(&a.0, &b.0))?;
            for (slot, (_, id)) in self.minima_list.iter_mut().zip(keyed) {
                *slot = id;
            }
            self.minima_list_sorted = true;
        }
        for i in (0..self.minima_list.len()).rev() {
            let id = self.minima_list[i];
            let y = self.vertex(self.local_minima(id)?.vertex)?.pt.y;
            self.insert_scanline(y)?;
        }
        self.current_locmin_iter = 0;
        self.actives = None;
        self.sel = None;
        self.succeeded = true;
        Ok(())
    }

    /// `void SetZ(const Active& e1, const Active& e2, Point64& ip)`: "prioritize subject over
    /// clip vertices by passing subject vertices before clip vertices in the callback".
    pub fn set_z(&mut self, e1: ActiveId, e2: ActiveId, ip: &mut Point64) -> Result<()> {
        if !self.has_z_callback() {
            return Ok(());
        }
        let a1 = *self.active(e1)?;
        let a2 = *self.active(e2)?;
        if self.get_poly_type(e1)? == PathType::Subject {
            ip.z = if *ip == a1.bot {
                a1.bot.z
            } else if *ip == a1.top {
                a1.top.z
            } else if *ip == a2.bot {
                a2.bot.z
            } else if *ip == a2.top {
                a2.top.z
            } else {
                self.default_z
            };
            self.call_z_callback(&a1.bot, &a1.top, &a2.bot, &a2.top, ip)
        } else {
            ip.z = if *ip == a2.bot {
                a2.bot.z
            } else if *ip == a2.top {
                a2.top.z
            } else if *ip == a1.bot {
                a1.bot.z
            } else if *ip == a1.top {
                a1.top.z
            } else {
                self.default_z
            };
            self.call_z_callback(&a2.bot, &a2.top, &a1.bot, &a1.top, ip)
        }
    }

    /// `SetZ` on an output point in place.
    fn set_z_out_pt(&mut self, e1: ActiveId, e2: ActiveId, op: OutPtId) -> Result<()> {
        let mut pt = self.out_pt(op)?.pt;
        self.set_z(e1, e2, &mut pt)?;
        self.op_mut(op)?.pt = pt;
        Ok(())
    }

    /// `void AddPath(const Path64& path, PathType polytype, bool is_open)`.
    pub fn add_path(&mut self, path: &Path64, polytype: PathType, is_open: bool) -> Result<()> {
        let mut paths = Paths64::new();
        push(&mut paths, crate::copy_of(path)?)?;
        self.add_paths(&paths, polytype, is_open)
    }

    /// `void AddPaths(const Paths64& paths, PathType polytype, bool is_open)`.
    pub fn add_paths(&mut self, paths: &Paths64, polytype: PathType, is_open: bool) -> Result<()> {
        if is_open {
            self.has_open_paths = true;
        }
        self.minima_list_sorted = false;
        let list = self.vertex_lists.len();
        if let Some((chunk, minima)) = add_paths_(paths, polytype, is_open, list)? {
            reserve(&mut self.vertex_lists, 1)?;
            reserve(&mut self.minima_store, minima.len())?;
            reserve(&mut self.minima_list, minima.len())?;
            for lm in minima {
                self.minima_list
                    .push(LocalMinimaId(self.minima_store.len()));
                self.minima_store.push(lm);
            }
            self.vertex_lists.push(VertexListSlot {
                chunk: Rc::new(chunk),
                owned: true,
            });
        }
        Ok(())
    }

    /// `void AddReuseableData(const ReuseableDataContainer64& reuseable_data)`: "nb:
    /// reuseable_data will continue to own the vertices and remains responsible for their
    /// clean up."
    pub fn add_reuseable_data(&mut self, reuseable_data: &ReuseableDataContainer64) -> Result<()> {
        self.succeeded = false;
        self.minima_list_sorted = false;
        for lm in reuseable_data.minima() {
            let chunk = reuseable_data
                .vertex_lists()
                .get(lm.vertex.list)
                .ok_or(DANGLING_VERTEX)?;
            // The same array keeps one identity, as its pointers do.
            let list = match self
                .vertex_lists
                .iter()
                .position(|slot| !slot.owned && Rc::ptr_eq(&slot.chunk, chunk))
            {
                Some(list) => list,
                None => {
                    push(
                        &mut self.vertex_lists,
                        VertexListSlot {
                            chunk: Rc::clone(chunk),
                            owned: false,
                        },
                    )?;
                    self.vertex_lists.len() - 1
                }
            };
            reserve(&mut self.minima_store, 1)?;
            reserve(&mut self.minima_list, 1)?;
            self.minima_list
                .push(LocalMinimaId(self.minima_store.len()));
            self.minima_store.push(LocalMinima::new(
                VertexId {
                    list,
                    index: lm.vertex.index,
                },
                lm.polytype,
                lm.is_open,
            ));
            if lm.is_open {
                self.has_open_paths = true;
            }
        }
        Ok(())
    }

    /// `inline void InsertScanline(int64_t y)`.
    pub fn insert_scanline(&mut self, y: i64) -> Result<()> {
        self.scanline_list
            .try_reserve(1)
            .map_err(|_| ClipperError::Allocation)?;
        self.scanline_list.push(y);
        Ok(())
    }

    /// `inline bool PopScanline(int64_t& y)`: the largest y, with its duplicates popped.
    pub fn pop_scanline(&mut self) -> Option<i64> {
        let y = self.scanline_list.pop()?;
        while self.scanline_list.peek() == Some(&y) {
            self.scanline_list.pop(); // Pop duplicates.
        }
        Some(y)
    }

    /// `inline bool PopLocalMinima(int64_t y, LocalMinima*& local_minima)`.
    pub fn pop_local_minima(&mut self, y: i64) -> Result<Option<LocalMinimaId>> {
        let Some(&id) = self.minima_list.get(self.current_locmin_iter) else {
            if self.current_locmin_iter == self.minima_list.len() {
                return Ok(None);
            }
            return Err(STALE_ITERATOR);
        };
        if self.vertex(self.local_minima(id)?.vertex)?.pt.y != y {
            return Ok(None);
        }
        self.current_locmin_iter += 1;
        Ok(Some(id))
    }

    /// `void DisposeAllOutRecs()`.
    pub fn dispose_all_outrecs(&mut self) -> Result<()> {
        for i in 0..self.outrec_list.len() {
            if self.outrec_list[i].pts.is_some() {
                self.dispose_out_pts(OutRecId(i))?;
            }
        }
        self.outrec_list.clear();
        // Every ring is deleted with its record: release the arena when no point is left.
        if self.out_pts.iter().all(Option::is_none) {
            self.out_pts.clear();
        }
        Ok(())
    }

    /// `void DisposeVerticesAndLocalMinima()`.
    pub fn dispose_vertices_and_local_minima(&mut self) {
        self.minima_list.clear();
        self.minima_store.clear();
        self.vertex_lists.clear();
    }

    /// `inline void AddLocMin(Vertex& vert, PathType polytype, bool is_open)`: "make sure the
    /// vertex is added only once ..."
    pub fn add_loc_min(&mut self, vert: VertexId, polytype: PathType, is_open: bool) -> Result<()> {
        let v = self.vertex(vert)?;
        if (VertexFlags::LOCAL_MIN & v.flags) != VertexFlags::EMPTY {
            return Ok(());
        }
        reserve(&mut self.minima_store, 1)?;
        reserve(&mut self.minima_list, 1)?;
        self.vertex_lists
            .get(vert.list)
            .ok_or(DANGLING_VERTEX)?
            .chunk
            .set_flags(vert.index, v.flags | VertexFlags::LOCAL_MIN)?;
        self.minima_list
            .push(LocalMinimaId(self.minima_store.len()));
        self.minima_store
            .push(LocalMinima::new(vert, polytype, is_open));
        Ok(())
    }

    // --- winding (lines 1093-1302) ---------------------------------------------------------

    /// `bool IsContributingClosed(const Active& e) const`.
    pub fn is_contributing_closed(&self, e: ActiveId) -> Result<bool> {
        let a = *self.active(e)?;
        match self.fillrule {
            FillRule::EvenOdd => {}
            FillRule::NonZero => {
                if abs32(a.wind_cnt)? != 1 {
                    return Ok(false);
                }
            }
            FillRule::Positive => {
                if a.wind_cnt != 1 {
                    return Ok(false);
                }
            }
            FillRule::Negative => {
                if a.wind_cnt != -1 {
                    return Ok(false);
                }
            }
        }

        Ok(match self.cliptype {
            ClipType::NoClip => false,
            ClipType::Intersection => match self.fillrule {
                FillRule::Positive => a.wind_cnt2 > 0,
                FillRule::Negative => a.wind_cnt2 < 0,
                _ => a.wind_cnt2 != 0,
            },
            ClipType::Union => match self.fillrule {
                FillRule::Positive => a.wind_cnt2 <= 0,
                FillRule::Negative => a.wind_cnt2 >= 0,
                _ => a.wind_cnt2 == 0,
            },
            ClipType::Difference => {
                let result = match self.fillrule {
                    FillRule::Positive => a.wind_cnt2 <= 0,
                    FillRule::Negative => a.wind_cnt2 >= 0,
                    _ => a.wind_cnt2 == 0,
                };
                if self.get_poly_type(e)? == PathType::Subject {
                    result
                } else {
                    !result
                }
            }
            ClipType::Xor => true,
        })
    }

    /// `inline bool IsContributingOpen(const Active& e) const`.
    pub fn is_contributing_open(&self, e: ActiveId) -> Result<bool> {
        let a = self.active(e)?;
        let (is_in_clip, is_in_subj) = match self.fillrule {
            FillRule::Positive => (a.wind_cnt2 > 0, a.wind_cnt > 0),
            FillRule::Negative => (a.wind_cnt2 < 0, a.wind_cnt < 0),
            _ => (a.wind_cnt2 != 0, a.wind_cnt != 0),
        };
        Ok(match self.cliptype {
            ClipType::Intersection => is_in_clip,
            ClipType::Union => !is_in_subj && !is_in_clip,
            _ => !is_in_clip,
        })
    }

    /// `void SetWindCountForClosedPathEdge(Active& e)`. "Wind counts refer to polygon regions
    /// not edges, so here an edge's WindCnt indicates the higher of the wind counts for the two
    /// regions touching the edge. (NB Adjacent regions can only ever have their wind counts
    /// differ by one. Also, open paths have no meaningful wind directions or counts.)"
    pub fn set_wind_count_for_closed_path_edge(&mut self, e: ActiveId) -> Result<()> {
        let mut e2 = self.active(e)?.prev_in_ael;
        // find the nearest closed path edge of the same PolyType in AEL (heading left)
        let pt = self.get_poly_type(e)?;
        let mut walk = self.ael_walk();
        while let Some(x) = e2 {
            if !(self.get_poly_type(x)? != pt || self.is_open(x)?) {
                break;
            }
            walk.step()?;
            e2 = self.active(x)?.prev_in_ael;
        }

        match e2 {
            None => {
                let wind_dx = self.active(e)?.wind_dx;
                self.ae_mut(e)?.wind_cnt = wind_dx;
                e2 = self.actives;
            }
            Some(x) if self.fillrule == FillRule::EvenOdd => {
                let wind_dx = self.active(e)?.wind_dx;
                self.ae_mut(e)?.wind_cnt = wind_dx;
                let wc2 = self.active(x)?.wind_cnt2;
                self.ae_mut(e)?.wind_cnt2 = wc2;
                e2 = self.active(x)?.next_in_ael;
            }
            Some(x) => {
                // NonZero, positive, or negative filling here ...
                // if e's WindCnt is in the SAME direction as its WindDx, then polygon
                // filling will be on the right of 'e'.
                // NB neither e2.WindCnt nor e2.WindDx should ever be 0.
                let b = *self.active(x)?;
                let e_wind_dx = self.active(e)?.wind_dx;
                let wind_cnt = if mul32(b.wind_cnt, b.wind_dx)? < 0 {
                    // opposite directions so 'e' is outside 'e2' ...
                    if abs32(b.wind_cnt)? > 1 {
                        // outside prev poly but still inside another.
                        if mul32(b.wind_dx, e_wind_dx)? < 0 {
                            // reversing direction so use the same WC
                            b.wind_cnt
                        } else {
                            // otherwise keep 'reducing' the WC by 1 (ie towards 0) ...
                            add32(b.wind_cnt, e_wind_dx)?
                        }
                    } else if self.is_open(e)? {
                        // now outside all polys of same polytype so set own WC ...
                        1
                    } else {
                        e_wind_dx
                    }
                } else if mul32(b.wind_dx, e_wind_dx)? < 0 {
                    // 'e' must be inside 'e2'
                    // reversing direction so use the same WC
                    b.wind_cnt
                } else {
                    // otherwise keep 'increasing' the WC by 1 (ie away from 0) ...
                    add32(b.wind_cnt, e_wind_dx)?
                };
                self.ae_mut(e)?.wind_cnt = wind_cnt;
                let wc2 = self.active(x)?.wind_cnt2;
                self.ae_mut(e)?.wind_cnt2 = wc2;
                e2 = self.active(x)?.next_in_ael; // ie get ready to calc WindCnt2
            }
        }

        // update wind_cnt2 ...
        let mut walk = self.ael_walk();
        if self.fillrule == FillRule::EvenOdd {
            while e2 != Some(e) {
                walk.step()?;
                let x = some(e2)?;
                if self.get_poly_type(x)? != pt && !self.is_open(x)? {
                    let a = self.ae_mut(e)?;
                    a.wind_cnt2 = if a.wind_cnt2 == 0 { 1 } else { 0 };
                }
                e2 = self.active(x)?.next_in_ael;
            }
        } else {
            while e2 != Some(e) {
                walk.step()?;
                let x = some(e2)?;
                if self.get_poly_type(x)? != pt && !self.is_open(x)? {
                    let dx = self.active(x)?.wind_dx;
                    let a = self.ae_mut(e)?;
                    a.wind_cnt2 = add32(a.wind_cnt2, dx)?;
                }
                e2 = self.active(x)?.next_in_ael;
            }
        }
        Ok(())
    }

    /// `void SetWindCountForOpenPathEdge(Active& e)`.
    pub fn set_wind_count_for_open_path_edge(&mut self, e: ActiveId) -> Result<()> {
        let mut e2 = self.actives;
        let mut walk = self.ael_walk();
        if self.fillrule == FillRule::EvenOdd {
            let (mut cnt1, mut cnt2): (i32, i32) = (0, 0);
            while e2 != Some(e) {
                walk.step()?;
                let x = some(e2)?;
                if self.get_poly_type(x)? == PathType::Clip {
                    cnt2 = add32(cnt2, 1)?;
                } else if !self.is_open(x)? {
                    cnt1 = add32(cnt1, 1)?;
                }
                e2 = self.active(x)?.next_in_ael;
            }
            let a = self.ae_mut(e)?;
            a.wind_cnt = if is_odd(cnt1) { 1 } else { 0 };
            a.wind_cnt2 = if is_odd(cnt2) { 1 } else { 0 };
        } else {
            while e2 != Some(e) {
                walk.step()?;
                let x = some(e2)?;
                let dx = self.active(x)?.wind_dx;
                if self.get_poly_type(x)? == PathType::Clip {
                    let a = self.ae_mut(e)?;
                    a.wind_cnt2 = add32(a.wind_cnt2, dx)?;
                } else if !self.is_open(x)? {
                    let a = self.ae_mut(e)?;
                    a.wind_cnt = add32(a.wind_cnt, dx)?;
                }
                e2 = self.active(x)?.next_in_ael;
            }
        }
        Ok(())
    }

    // --- AEL insertion (lines 1304-1514) ---------------------------------------------------

    /// `bool IsValidAelOrder(const Active& resident, const Active& newcomer)`.
    pub fn is_valid_ael_order(&self, resident: ActiveId, newcomer: ActiveId) -> Result<bool> {
        let r = *self.active(resident)?;
        let n = *self.active(newcomer)?;
        if n.curr_x != r.curr_x {
            return Ok(n.curr_x > r.curr_x);
        }

        // get the turning direction  a1.top, a2.bot, a2.top
        let i = cross_product_sign(&r.top, &n.bot, &n.top)?;
        if i != 0 {
            return Ok(i < 0);
        }

        // edges must be collinear to get here
        // for starting open paths, place them according to
        // the direction they're about to turn
        if !self.is_maxima(resident)? && r.top.y > n.top.y {
            let nv = self.vertex(some(self.next_vertex(resident)?)?)?;
            return Ok(cross_product_sign(&n.bot, &r.top, &nv.pt)? <= 0);
        } else if !self.is_maxima(newcomer)? && n.top.y > r.top.y {
            let nv = self.vertex(some(self.next_vertex(newcomer)?)?)?;
            return Ok(cross_product_sign(&n.bot, &n.top, &nv.pt)? >= 0);
        }

        let y = n.bot.y;
        let newcomer_is_left = n.is_left_bound;

        if r.bot.y != y || self.vertex(self.lm_of(resident)?.vertex)?.pt.y != y {
            Ok(n.is_left_bound)
        } else if r.is_left_bound != newcomer_is_left {
            // resident must also have just been inserted
            Ok(newcomer_is_left)
        } else {
            let rpp = self.vertex(some(self.prev_prev_vertex(resident)?)?)?;
            if is_collinear(&rpp.pt, &r.bot, &r.top)? {
                Ok(true)
            } else {
                // compare turning direction of the alternate bound
                let npp = self.vertex(some(self.prev_prev_vertex(newcomer)?)?)?;
                Ok((cross_product_sign(&rpp.pt, &n.bot, &npp.pt)? > 0) == newcomer_is_left)
            }
        }
    }

    /// `void InsertLeftEdge(Active& e)`.
    pub fn insert_left_edge(&mut self, e: ActiveId) -> Result<()> {
        match self.actives {
            None => {
                let a = self.ae_mut(e)?;
                a.prev_in_ael = None;
                a.next_in_ael = None;
                self.actives = Some(e);
            }
            Some(head) if !self.is_valid_ael_order(head, e)? => {
                let a = self.ae_mut(e)?;
                a.prev_in_ael = None;
                a.next_in_ael = Some(head);
                self.ae_mut(head)?.prev_in_ael = Some(e);
                self.actives = Some(e);
            }
            Some(head) => {
                let mut e2 = head;
                let mut walk = self.ael_walk();
                while let Some(n) = self.active(e2)?.next_in_ael {
                    if !self.is_valid_ael_order(n, e)? {
                        break;
                    }
                    walk.step()?;
                    e2 = n;
                }
                let mut e2 = Some(e2);
                if let Some(x) = e2 {
                    if self.active(x)?.join_with == JoinWith::Right {
                        e2 = self.active(x)?.next_in_ael;
                    }
                }
                // "should never happen and stops compiler warning :)"
                let Some(e2) = e2 else { return Ok(()) };
                let e2_next = self.active(e2)?.next_in_ael;
                self.ae_mut(e)?.next_in_ael = e2_next;
                if let Some(n) = e2_next {
                    self.ae_mut(n)?.prev_in_ael = Some(e);
                }
                self.ae_mut(e)?.prev_in_ael = Some(e2);
                self.ae_mut(e2)?.next_in_ael = Some(e);
            }
        }
        Ok(())
    }

    /// `void InsertRightEdge(Active& e, Active& e2)`.
    pub fn insert_right_edge(&mut self, e: ActiveId, e2: ActiveId) -> Result<()> {
        let e_next = self.active(e)?.next_in_ael;
        self.ae_mut(e2)?.next_in_ael = e_next;
        if let Some(n) = self.active(e)?.next_in_ael {
            self.ae_mut(n)?.prev_in_ael = Some(e2);
        }
        self.ae_mut(e2)?.prev_in_ael = Some(e);
        self.ae_mut(e)?.next_in_ael = Some(e2);
        Ok(())
    }

    /// A bound of `local_minima` as `InsertLocalMinimaIntoAEL` creates it: the descending
    /// (left, `wind_dx = -1`, top at `vertex->prev`) or ascending (right) one.
    pub fn new_bound(&mut self, local_minima: LocalMinimaId, descending: bool) -> Result<ActiveId> {
        let lm = *self.local_minima(local_minima)?;
        let v = self.vertex(lm.vertex)?;
        let mut a = Active {
            bot: v.pt,
            ..Active::default()
        };
        a.curr_x = a.bot.x;
        a.wind_dx = if descending { -1 } else { 1 };
        a.vertex_top = Some(some(rebind(
            lm.vertex,
            if descending { v.prev } else { v.next },
        ))?);
        a.top = self.vertex(some(a.vertex_top)?)?.pt;
        a.local_min = Some(local_minima);
        let id = self.new_active(a)?;
        self.set_dx(id)?;
        Ok(id)
    }

    /// `void InsertLocalMinimaIntoAEL(int64_t bot_y)`: "Add any local minima (if any) at
    /// BotY ... nb: horizontal local minima edges should contain locMin.vertex.prev".
    pub fn insert_local_minima_into_ael(&mut self, bot_y: i64) -> Result<()> {
        let mut walk = Walk::new(self.minima_list.len());
        while let Some(local_minima) = self.pop_local_minima(bot_y)? {
            walk.step()?;
            let flags = self.vertex(self.local_minima(local_minima)?.vertex)?.flags;
            let mut left_bound = if (flags & VertexFlags::OPEN_START) != VertexFlags::EMPTY {
                None
            } else {
                Some(self.new_bound(local_minima, true)?) // ie descending
            };

            let mut right_bound = if (flags & VertexFlags::OPEN_END) != VertexFlags::EMPTY {
                None
            } else {
                Some(self.new_bound(local_minima, false)?) // ie ascending
            };

            // Currently LeftB is just the descending bound and RightB is the ascending.
            // Now if the LeftB isn't on the left of RightB then we need swap them.
            if let (Some(l), Some(r)) = (left_bound, right_bound) {
                let (la, ra) = (*self.active(l)?, *self.active(r)?);
                if is_horizontal(&la) {
                    if is_heading_right_horz(&la) {
                        swap_actives(&mut left_bound, &mut right_bound);
                    }
                } else if is_horizontal(&ra) {
                    if is_heading_left_horz(&ra) {
                        swap_actives(&mut left_bound, &mut right_bound);
                    }
                } else if la.dx < ra.dx {
                    swap_actives(&mut left_bound, &mut right_bound);
                }
            } else if left_bound.is_none() {
                left_bound = right_bound;
                right_bound = None;
            }

            let left = some(left_bound)?;
            self.ae_mut(left)?.is_left_bound = true;
            self.insert_left_edge(left)?;

            let contributing = if self.is_open(left)? {
                self.set_wind_count_for_open_path_edge(left)?;
                self.is_contributing_open(left)?
            } else {
                self.set_wind_count_for_closed_path_edge(left)?;
                self.is_contributing_closed(left)?
            };

            if let Some(right) = right_bound {
                let l = *self.active(left)?;
                let r = self.ae_mut(right)?;
                r.is_left_bound = false;
                r.wind_cnt = l.wind_cnt;
                r.wind_cnt2 = l.wind_cnt2;
                self.insert_right_edge(left, right)?;
                if contributing {
                    let bot = self.active(left)?.bot;
                    self.add_local_min_poly(left, right, &bot, true)?;
                    if !is_horizontal(self.active(left)?) {
                        let bot = self.active(left)?.bot;
                        self.check_join_left(left, &bot, false)?;
                    }
                }

                let mut walk = self.ael_walk();
                while let Some(next) = self.active(right)?.next_in_ael {
                    if !self.is_valid_ael_order(next, right)? {
                        break;
                    }
                    walk.step()?;
                    let bot = self.active(right)?.bot;
                    self.intersect_edges(right, next, &bot)?;
                    self.swap_positions_in_ael(right, next)?;
                }

                if is_horizontal(self.active(right)?) {
                    self.push_horz(right)?;
                } else {
                    let bot = self.active(right)?.bot;
                    self.check_join_right(right, &bot, false)?;
                    let top_y = self.active(right)?.top.y;
                    self.insert_scanline(top_y)?;
                }
            } else if contributing {
                let bot = self.active(left)?.bot;
                self.start_open_path(left, &bot)?;
            }

            if is_horizontal(self.active(left)?) {
                self.push_horz(left)?;
            } else {
                let top_y = self.active(left)?.top.y;
                self.insert_scanline(top_y)?;
            }
        } // while (PopLocalMinima())
        Ok(())
    }

    /// `inline void PushHorz(Active& e)`.
    pub fn push_horz(&mut self, e: ActiveId) -> Result<()> {
        let sel = self.sel;
        self.ae_mut(e)?.next_in_sel = sel;
        self.sel = Some(e);
        Ok(())
    }

    /// `inline bool PopHorz(Active*& e)`.
    pub fn pop_horz(&mut self) -> Result<Option<ActiveId>> {
        let Some(e) = self.sel else { return Ok(None) };
        self.sel = self.active(e)?.next_in_sel;
        Ok(Some(e))
    }

    // --- output records (lines 1517-1891) --------------------------------------------------

    /// `OutPt* AddLocalMinPoly(Active& e1, Active& e2, const Point64& pt, bool is_new = false)`.
    pub fn add_local_min_poly(
        &mut self,
        e1: ActiveId,
        e2: ActiveId,
        pt: &Point64,
        is_new: bool,
    ) -> Result<OutPtId> {
        let outrec = self.new_outrec()?;
        self.ae_mut(e1)?.outrec = Some(outrec);
        self.ae_mut(e2)?.outrec = Some(outrec);

        if self.is_open(e1)? {
            let rec = self.or_mut(outrec)?;
            rec.owner = None;
            rec.is_open = true;
            if self.active(e1)?.wind_dx > 0 {
                self.set_sides(outrec, e1, e2)?;
            } else {
                self.set_sides(outrec, e2, e1)?;
            }
        } else {
            // e.windDx is the winding direction of the **input** paths
            // and unrelated to the winding direction of output polygons.
            // Output orientation is determined by e.outrec.frontE which is
            // the ascending edge (see AddLocalMinPoly).
            match self.get_prev_hot_edge(e1)? {
                Some(prev_hot_edge) => {
                    if self.using_polytree {
                        let owner = self.outrec_of(prev_hot_edge)?;
                        self.set_owner(outrec, owner)?;
                    }
                    if self.outrec_is_ascending(prev_hot_edge)? == is_new {
                        self.set_sides(outrec, e2, e1)?;
                    } else {
                        self.set_sides(outrec, e1, e2)?;
                    }
                }
                None => {
                    self.or_mut(outrec)?.owner = None;
                    if is_new {
                        self.set_sides(outrec, e1, e2)?;
                    } else {
                        self.set_sides(outrec, e2, e1)?;
                    }
                }
            }
        }

        let op = self.new_out_pt(pt, Some(outrec))?;
        self.or_mut(outrec)?.pts = Some(op);
        Ok(op)
    }

    /// `OutPt* AddLocalMaxPoly(Active& e1, Active& e2, const Point64& pt)`.
    pub fn add_local_max_poly(
        &mut self,
        e1: ActiveId,
        e2: ActiveId,
        pt: &Point64,
    ) -> Result<Option<OutPtId>> {
        if is_joined(self.active(e1)?) {
            self.split(e1, pt)?;
        }
        if is_joined(self.active(e2)?) {
            self.split(e2, pt)?;
        }

        if self.is_front(e1)? == self.is_front(e2)? {
            if self.is_open_end(e1)? {
                let o = self.outrec_of(e1)?;
                self.swap_front_back_sides(o)?;
            } else if self.is_open_end(e2)? {
                let o = self.outrec_of(e2)?;
                self.swap_front_back_sides(o)?;
            } else {
                self.succeeded = false;
                return Ok(None);
            }
        }

        let mut result = self.add_out_pt(e1, pt)?;
        let (or1, or2) = (self.active(e1)?.outrec, self.active(e2)?.outrec);
        if or1 == or2 {
            let outrec = some(or1)?;
            self.or_mut(outrec)?.pts = Some(result);

            if self.using_polytree {
                match self.get_prev_hot_edge(e1)? {
                    None => self.or_mut(outrec)?.owner = None,
                    Some(e) => {
                        let owner = self.outrec_of(e)?;
                        self.set_owner(outrec, owner)?;
                    }
                }
                // nb: outRec.owner here is likely NOT the real
                // owner but this will be checked in RecursiveCheckOwners()
            }

            self.uncouple_outrec(e1)?;
            result = some(self.outrec(outrec)?.pts)?;
            if let Some(owner) = self.outrec(outrec)?.owner {
                if self.outrec(owner)?.front_edge.is_none() {
                    let real = self.get_real_outrec(Some(owner))?;
                    self.or_mut(outrec)?.owner = real;
                }
            }
        }
        // and to preserve the winding orientation of outrec ...
        else if self.is_open(e1)? {
            if self.active(e1)?.wind_dx < 0 {
                self.join_outrec_paths(e1, e2)?;
            } else {
                self.join_outrec_paths(e2, e1)?;
            }
        } else if self.outrec(some(or1)?)?.idx < self.outrec(some(or2)?)?.idx {
            self.join_outrec_paths(e1, e2)?;
        } else {
            self.join_outrec_paths(e2, e1)?;
        }
        Ok(Some(result))
    }

    /// `void JoinOutrecPaths(Active& e1, Active& e2)`: "join e2 outrec path onto e1 outrec
    /// path and then delete e2 outrec path pointers. (NB Only very rarely do the joining ends
    /// share the same coords.)"
    pub fn join_outrec_paths(&mut self, e1: ActiveId, e2: ActiveId) -> Result<()> {
        let or1 = self.outrec_of(e1)?;
        let or2 = self.outrec_of(e2)?;
        let p1_st = some(self.outrec(or1)?.pts)?;
        let p2_st = some(self.outrec(or2)?.pts)?;
        let p1_end = self.out_pt(p1_st)?.next;
        let p2_end = self.out_pt(p2_st)?.next;
        if self.is_front(e1)? {
            self.op_mut(p2_end)?.prev = p1_st;
            self.op_mut(p1_st)?.next = p2_end;
            self.op_mut(p2_st)?.next = p1_end;
            self.op_mut(p1_end)?.prev = p2_st;
            self.or_mut(or1)?.pts = Some(p2_st);
            let front = self.outrec(self.outrec_of(e2)?)?.front_edge;
            let or1 = self.outrec_of(e1)?;
            self.or_mut(or1)?.front_edge = front;
            if let Some(f) = front {
                self.ae_mut(f)?.outrec = Some(or1);
            }
        } else {
            self.op_mut(p1_end)?.prev = p2_st;
            self.op_mut(p2_st)?.next = p1_end;
            self.op_mut(p1_st)?.next = p2_end;
            self.op_mut(p2_end)?.prev = p1_st;
            let back = self.outrec(self.outrec_of(e2)?)?.back_edge;
            let or1 = self.outrec_of(e1)?;
            self.or_mut(or1)?.back_edge = back;
            if let Some(b) = back {
                self.ae_mut(b)?.outrec = Some(or1);
            }
        }

        // after joining, the e2.OutRec must contains no vertices ...
        let or2 = self.outrec_of(e2)?;
        let rec = self.or_mut(or2)?;
        rec.front_edge = None;
        rec.back_edge = None;
        rec.pts = None;

        if self.is_open_end(e1)? {
            let or1 = self.outrec_of(e1)?;
            let pts = self.outrec(or1)?.pts;
            self.or_mut(or2)?.pts = pts;
            self.or_mut(or1)?.pts = None;
        } else {
            let or1 = self.outrec_of(e1)?;
            self.set_owner(or2, or1)?;
        }

        // and e1 and e2 are maxima and are about to be dropped from the Actives list.
        self.ae_mut(e1)?.outrec = None;
        self.ae_mut(e2)?.outrec = None;
        Ok(())
    }

    /// `OutRec* NewOutRec()`.
    pub fn new_outrec(&mut self) -> Result<OutRecId> {
        reserve(&mut self.outrec_list, 1)?;
        let id = OutRecId(self.outrec_list.len());
        self.outrec_list.push(OutRec {
            idx: id.0,
            pts: None,
            owner: None,
            polypath: None,
            is_open: false,
            splits: None,
            ..OutRec::default()
        });
        Ok(id)
    }

    /// `OutPt* AddOutPt(const Active& e, const Point64& pt)`. "Outrec.OutPts: a circular
    /// doubly-linked-list of POutPt where ... op_front[.Prev]* ~~~> op_back & op_back ==
    /// op_front.Next"
    pub fn add_out_pt(&mut self, e: ActiveId, pt: &Point64) -> Result<OutPtId> {
        let outrec = self.outrec_of(e)?;
        let to_front = self.is_front(e)?;
        let op_front = some(self.outrec(outrec)?.pts)?;
        let op_back = self.out_pt(op_front)?.next;

        if to_front {
            if *pt == self.out_pt(op_front)?.pt {
                return Ok(op_front);
            }
        } else if *pt == self.out_pt(op_back)?.pt {
            return Ok(op_back);
        }

        let new_op = self.new_out_pt(pt, Some(outrec))?;
        self.op_mut(op_back)?.prev = new_op;
        let n = self.op_mut(new_op)?;
        n.prev = op_front;
        n.next = op_back;
        self.op_mut(op_front)?.next = new_op;
        if to_front {
            self.or_mut(outrec)?.pts = Some(new_op);
        }
        Ok(new_op)
    }

    /// `void CleanCollinear(OutRec* outrec)`.
    pub fn clean_collinear(&mut self, outrec: Option<OutRecId>) -> Result<()> {
        let Some(outrec) = self.get_real_outrec(outrec)? else {
            return Ok(());
        };
        if self.outrec(outrec)?.is_open {
            return Ok(());
        }
        if !self.is_valid_closed_path(self.outrec(outrec)?.pts)? {
            self.dispose_out_pts(outrec)?;
            return Ok(());
        }

        let mut start_op = some(self.outrec(outrec)?.pts)?;
        let mut op2 = start_op;
        let mut walk = Walk::squared(self.out_pts.len());
        loop {
            walk.step()?;
            // NB if preserveCollinear == true, then only remove 180 deg. spikes
            let o = *self.out_pt(op2)?;
            let (p, n) = (self.out_pt(o.prev)?.pt, self.out_pt(o.next)?.pt);
            if is_collinear(&p, &o.pt, &n)?
                && (o.pt == p
                    || o.pt == n
                    || !self.preserve_collinear
                    || dot_product3(&p, &o.pt, &n)? < 0.0)
            {
                if Some(op2) == self.outrec(outrec)?.pts {
                    self.or_mut(outrec)?.pts = Some(o.prev);
                }

                op2 = self.dispose_out_pt(op2)?;
                if !self.is_valid_closed_path(Some(op2))? {
                    self.dispose_out_pts(outrec)?;
                    return Ok(());
                }
                start_op = op2;
                continue;
            }
            op2 = o.next;
            if op2 == start_op {
                break;
            }
        }
        self.fix_self_intersects(outrec)
    }

    /// `void DoSplitOp(OutRec* outrec, OutPt* splitOp)`: "splitOp.prev -> splitOp &&
    /// splitOp.next -> splitOp.next.next are intersecting".
    pub fn do_split_op(&mut self, outrec: OutRecId, split_op: OutPtId) -> Result<()> {
        let prev_op = self.out_pt(split_op)?.prev;
        let split_next = self.out_pt(split_op)?.next;
        let next_next_op = self.out_pt(split_next)?.next;
        self.or_mut(outrec)?.pts = Some(prev_op);

        let mut ip = Point64::default();
        let (prev_pt, split_pt, split_next_pt, next_next_pt) = (
            self.out_pt(prev_op)?.pt,
            self.out_pt(split_op)?.pt,
            self.out_pt(split_next)?.pt,
            self.out_pt(next_next_op)?.pt,
        );
        get_line_intersect_pt(&prev_pt, &split_pt, &split_next_pt, &next_next_pt, &mut ip)?;

        if self.has_z_callback() {
            self.call_z_callback(&prev_pt, &split_pt, &split_next_pt, &next_next_pt, &mut ip)?;
        }
        let area1 = self.area_of(some(self.outrec(outrec)?.pts)?)?;
        let abs_area1 = area1.abs();
        if abs_area1 < 2.0 {
            self.dispose_out_pts(outrec)?;
            return Ok(());
        }

        let split_next = self.out_pt(split_op)?.next;
        let area2 = area_triangle(
            &ip,
            &self.out_pt(split_op)?.pt,
            &self.out_pt(split_next)?.pt,
        )?;
        let abs_area2 = area2.abs();

        // de-link splitOp and splitOp.next from the path
        // while inserting the intersection point
        if ip == self.out_pt(prev_op)?.pt || ip == self.out_pt(next_next_op)?.pt {
            self.op_mut(next_next_op)?.prev = prev_op;
            self.op_mut(prev_op)?.next = next_next_op;
        } else {
            let prev_outrec = self.out_pt(prev_op)?.outrec;
            let new_op2 = self.new_out_pt(&ip, prev_outrec)?;
            let n = self.op_mut(new_op2)?;
            n.prev = prev_op;
            n.next = next_next_op;
            self.op_mut(next_next_op)?.prev = new_op2;
            self.op_mut(prev_op)?.next = new_op2;
        }

        // area1 is the path's area *before* splitting, whereas area2 is
        // the area of the triangle containing splitOp & splitOp.next.
        // So the only way for these areas to have the same sign is if
        // the split triangle is larger than the path containing prevOp or
        // if there's more than one self-intersection.
        if abs_area2 >= 1.0 && (abs_area2 > abs_area1 || (area2 > 0.0) == (area1 > 0.0)) {
            let new_or = self.new_outrec()?;
            let owner = self.outrec(outrec)?.owner;
            self.or_mut(new_or)?.owner = owner;

            self.op_mut(split_op)?.outrec = Some(new_or);
            let split_next = self.out_pt(split_op)?.next;
            self.op_mut(split_next)?.outrec = Some(new_or);
            let new_op = self.new_out_pt(&ip, Some(new_or))?;
            let split_next = self.out_pt(split_op)?.next;
            let n = self.op_mut(new_op)?;
            n.prev = split_next;
            n.next = split_op;
            self.or_mut(new_or)?.pts = Some(new_op);
            self.op_mut(split_op)?.prev = new_op;
            let split_next = self.out_pt(split_op)?.next;
            self.op_mut(split_next)?.next = new_op;

            if self.using_polytree {
                if self.path2_contains_path1(prev_op, Some(new_op))? {
                    self.or_mut(new_or)?.splits = Some(vec![outrec]);
                } else {
                    let rec = self.or_mut(outrec)?;
                    let splits = rec.splits.get_or_insert_with(Vec::new);
                    push(splits, new_or)?;
                }
            }
        } else {
            let split_next = self.out_pt(split_op)?.next;
            self.delete_out_pt(split_next)?;
            self.delete_out_pt(split_op)?;
        }
        Ok(())
    }

    /// `void FixSelfIntersects(OutRec* outrec)`.
    pub fn fix_self_intersects(&mut self, outrec: OutRecId) -> Result<()> {
        let mut op2 = some(self.outrec(outrec)?.pts)?;
        let triangle = |base: &Self, op: OutPtId| -> Result<bool> {
            let o = base.out_pt(op)?;
            Ok(o.prev == base.out_pt(o.next)?.next)
        };
        if triangle(self, op2)? {
            return Ok(()); // because triangles can't self-intersect
        }
        let mut walk = Walk::squared(self.out_pts.len());
        loop {
            walk.step()?;
            let o = *self.out_pt(op2)?;
            let next_next = self.out_pt(o.next)?.next;
            if segments_intersect(
                &self.out_pt(o.prev)?.pt,
                &o.pt,
                &self.out_pt(o.next)?.pt,
                &self.out_pt(next_next)?.pt,
                false,
            )? {
                let pts = some(self.outrec(outrec)?.pts)?;
                if op2 == pts || o.next == pts {
                    let prev = self.out_pt(pts)?.prev;
                    self.or_mut(outrec)?.pts = Some(prev);
                }
                self.do_split_op(outrec, op2)?;
                let Some(pts) = self.outrec(outrec)?.pts else {
                    break;
                };
                op2 = pts;
                if triangle(self, op2)? {
                    break; // again, because triangles can't self-intersect
                }
                continue;
            } else {
                op2 = o.next;
            }

            if Some(op2) == self.outrec(outrec)?.pts {
                break;
            }
        }
        Ok(())
    }

    /// `inline void UpdateOutrecOwner(OutRec* outrec)`.
    pub fn update_outrec_owner(&mut self, outrec: OutRecId) -> Result<()> {
        let start = some(self.outrec(outrec)?.pts)?;
        let mut op_curr = start;
        let mut walk = self.ring_walk();
        loop {
            walk.step()?;
            self.op_mut(op_curr)?.outrec = Some(outrec);
            op_curr = self.out_pt(op_curr)?.next;
            if Some(op_curr) == self.outrec(outrec)?.pts {
                return Ok(());
            }
        }
    }

    /// `OutPt* StartOpenPath(Active& e, const Point64& pt)`.
    pub fn start_open_path(&mut self, e: ActiveId, pt: &Point64) -> Result<OutPtId> {
        let outrec = self.new_outrec()?;
        self.or_mut(outrec)?.is_open = true;

        if self.active(e)?.wind_dx > 0 {
            let rec = self.or_mut(outrec)?;
            rec.front_edge = Some(e);
            rec.back_edge = None;
        } else {
            let rec = self.or_mut(outrec)?;
            rec.front_edge = None;
            rec.back_edge = Some(e);
        }

        self.ae_mut(e)?.outrec = Some(outrec);

        let op = self.new_out_pt(pt, Some(outrec))?;
        self.or_mut(outrec)?.pts = Some(op);
        Ok(op)
    }

    /// `inline void TrimHorz(Active& horzEdge, bool preserveCollinear)`.
    pub fn trim_horz(&mut self, horz_edge: ActiveId, preserve_collinear: bool) -> Result<()> {
        let mut was_trimmed = false;
        let mut pt = self.vertex(some(self.next_vertex(horz_edge)?)?)?.pt;
        let mut walk = Walk::new(self.vertex_lists.iter().map(|s| s.chunk.len()).sum());
        while pt.y == self.active(horz_edge)?.top.y {
            walk.step()?;
            // always trim 180 deg. spikes (in closed paths)
            // but otherwise break if preserveCollinear = true
            let h = *self.active(horz_edge)?;
            if preserve_collinear && ((pt.x < h.top.x) != (h.bot.x < h.top.x)) {
                break;
            }

            let next = self.next_vertex(horz_edge)?;
            let a = self.ae_mut(horz_edge)?;
            a.vertex_top = next;
            a.top = pt;
            was_trimmed = true;
            if self.is_maxima(horz_edge)? {
                break;
            }
            pt = self.vertex(some(self.next_vertex(horz_edge)?)?)?.pt;
        }

        if was_trimmed {
            self.set_dx(horz_edge)?; // +/-infinity
        }
        Ok(())
    }

    /// `inline void UpdateEdgeIntoAEL(Active* e)`.
    pub fn update_edge_into_ael(&mut self, e: ActiveId) -> Result<()> {
        let top = self.active(e)?.top;
        self.ae_mut(e)?.bot = top;
        let next = self.next_vertex(e)?;
        self.ae_mut(e)?.vertex_top = next;
        let top = self.vertex(some(next)?)?.pt;
        let a = self.ae_mut(e)?;
        a.top = top;
        a.curr_x = a.bot.x;
        self.set_dx(e)?;

        if is_joined(self.active(e)?) {
            let bot = self.active(e)?.bot;
            self.split(e, &bot)?;
        }

        if is_horizontal(self.active(e)?) {
            if !self.is_open(e)? {
                let preserve = self.preserve_collinear;
                self.trim_horz(e, preserve)?;
            }
            return Ok(());
        }

        let top_y = self.active(e)?.top.y;
        self.insert_scanline(top_y)?;
        let bot = self.active(e)?.bot;
        self.check_join_left(e, &bot, false)?;
        self.check_join_right(e, &bot, true) // (#500)
    }

    /// `Active* FindEdgeWithMatchingLocMin(Active* e)`.
    pub fn find_edge_with_matching_loc_min(&self, e: ActiveId) -> Result<Option<ActiveId>> {
        let a = *self.active(e)?;
        let mut result = a.next_in_ael;
        let mut walk = self.ael_walk();
        while let Some(r) = result {
            walk.step()?;
            let ra = self.active(r)?;
            if ra.local_min == a.local_min {
                return Ok(Some(r));
            } else if !is_horizontal(ra) && a.bot != ra.bot {
                result = None;
            } else {
                result = ra.next_in_ael;
            }
        }
        result = a.prev_in_ael;
        while let Some(r) = result {
            walk.step()?;
            let ra = self.active(r)?;
            if ra.local_min == a.local_min {
                return Ok(Some(r));
            } else if !is_horizontal(ra) && a.bot != ra.bot {
                return Ok(None);
            } else {
                result = ra.prev_in_ael;
            }
        }
        Ok(result)
    }

    /// `void IntersectEdges(Active& e1, Active& e2, const Point64& pt)`.
    pub fn intersect_edges(&mut self, e1: ActiveId, e2: ActiveId, pt: &Point64) -> Result<()> {
        // MANAGE OPEN PATH INTERSECTIONS SEPARATELY ...
        if self.has_open_paths && (self.is_open(e1)? || self.is_open(e2)?) {
            if self.is_open(e1)? && self.is_open(e2)? {
                return Ok(());
            }
            let (edge_o, edge_c) = if self.is_open(e1)? {
                (e1, e2)
            } else {
                (e2, e1)
            };
            if is_joined(self.active(edge_c)?) {
                self.split(edge_c, pt)?; // needed for safety
            }

            if abs32(self.active(edge_c)?.wind_cnt)? != 1 {
                return Ok(());
            }
            match self.cliptype {
                ClipType::Union => {
                    if !is_hot_edge(self.active(edge_c)?) {
                        return Ok(());
                    }
                }
                _ => {
                    if self.lm_of(edge_c)?.polytype == PathType::Subject {
                        return Ok(());
                    }
                }
            }

            let wind_cnt = self.active(edge_c)?.wind_cnt;
            match self.fillrule {
                FillRule::Positive => {
                    if wind_cnt != 1 {
                        return Ok(());
                    }
                }
                FillRule::Negative => {
                    if wind_cnt != -1 {
                        return Ok(());
                    }
                }
                _ => {
                    if abs32(wind_cnt)? != 1 {
                        return Ok(());
                    }
                }
            }

            // toggle contribution ...
            let result_op;
            if is_hot_edge(self.active(edge_o)?) {
                result_op = self.add_out_pt(edge_o, pt)?;
                let o = self.outrec_of(edge_o)?;
                if self.is_front(edge_o)? {
                    self.or_mut(o)?.front_edge = None;
                } else {
                    self.or_mut(o)?.back_edge = None;
                }
                self.ae_mut(edge_o)?.outrec = None;
            }
            // horizontal edges can pass under open paths at a LocMins
            else if *pt == self.vertex(self.lm_of(edge_o)?.vertex)?.pt
                && !is_open_end_vertex(&self.vertex(self.lm_of(edge_o)?.vertex)?)
            {
                // find the other side of the LocMin and
                // if it's 'hot' join up with it ...
                let e3 = self.find_edge_with_matching_loc_min(edge_o)?;
                match e3 {
                    Some(e3) if is_hot_edge(self.active(e3)?) => {
                        let o3 = self.outrec_of(e3)?;
                        self.ae_mut(edge_o)?.outrec = Some(o3);
                        if self.active(edge_o)?.wind_dx > 0 {
                            self.set_sides(o3, edge_o, e3)?;
                        } else {
                            self.set_sides(o3, e3, edge_o)?;
                        }
                        return Ok(());
                    }
                    _ => result_op = self.start_open_path(edge_o, pt)?,
                }
            } else {
                result_op = self.start_open_path(edge_o, pt)?;
            }

            if self.has_z_callback() {
                self.set_z_out_pt(edge_o, edge_c, result_op)?;
            }
            return Ok(());
        } // end of an open path intersection

        // MANAGING CLOSED PATHS FROM HERE ON

        if is_joined(self.active(e1)?) {
            self.split(e1, pt)?;
        }
        if is_joined(self.active(e2)?) {
            self.split(e2, pt)?;
        }

        // UPDATE WINDING COUNTS...

        if self.lm_of(e1)?.polytype == self.lm_of(e2)?.polytype {
            if self.fillrule == FillRule::EvenOdd {
                let old_e1_windcnt = self.active(e1)?.wind_cnt;
                let w2 = self.active(e2)?.wind_cnt;
                self.ae_mut(e1)?.wind_cnt = w2;
                self.ae_mut(e2)?.wind_cnt = old_e1_windcnt;
            } else {
                let e2_dx = self.active(e2)?.wind_dx;
                let w1 = self.active(e1)?.wind_cnt;
                self.ae_mut(e1)?.wind_cnt = if add32(w1, e2_dx)? == 0 {
                    neg32(w1)?
                } else {
                    add32(w1, e2_dx)?
                };
                let e1_dx = self.active(e1)?.wind_dx;
                let w2 = self.active(e2)?.wind_cnt;
                self.ae_mut(e2)?.wind_cnt = if sub32(w2, e1_dx)? == 0 {
                    neg32(w2)?
                } else {
                    sub32(w2, e1_dx)?
                };
            }
        } else if self.fillrule != FillRule::EvenOdd {
            let e2_dx = self.active(e2)?.wind_dx;
            let a = self.ae_mut(e1)?;
            a.wind_cnt2 = add32(a.wind_cnt2, e2_dx)?;
            let e1_dx = self.active(e1)?.wind_dx;
            let b = self.ae_mut(e2)?;
            b.wind_cnt2 = sub32(b.wind_cnt2, e1_dx)?;
        } else {
            let a = self.ae_mut(e1)?;
            a.wind_cnt2 = if a.wind_cnt2 == 0 { 1 } else { 0 };
            let b = self.ae_mut(e2)?;
            b.wind_cnt2 = if b.wind_cnt2 == 0 { 1 } else { 0 };
        }

        let (w1, w2) = (self.active(e1)?.wind_cnt, self.active(e2)?.wind_cnt);
        let (old_e1_windcnt, old_e2_windcnt) = match self.fillrule {
            FillRule::EvenOdd | FillRule::NonZero => (abs32(w1)?, abs32(w2)?),
            _ => {
                if self.fillrule == self.fillpos {
                    (w1, w2)
                } else {
                    (neg32(w1)?, neg32(w2)?)
                }
            }
        };

        let e1_windcnt_in_01 = old_e1_windcnt == 0 || old_e1_windcnt == 1;
        let e2_windcnt_in_01 = old_e2_windcnt == 0 || old_e2_windcnt == 1;

        if (!is_hot_edge(self.active(e1)?) && !e1_windcnt_in_01)
            || (!is_hot_edge(self.active(e2)?) && !e2_windcnt_in_01)
        {
            return Ok(());
        }

        // NOW PROCESS THE INTERSECTION ...
        let hot1 = is_hot_edge(self.active(e1)?);
        let hot2 = is_hot_edge(self.active(e2)?);
        // if both edges are 'hot' ...
        if hot1 && hot2 {
            if (old_e1_windcnt != 0 && old_e1_windcnt != 1)
                || (old_e2_windcnt != 0 && old_e2_windcnt != 1)
                || (self.lm_of(e1)?.polytype != self.lm_of(e2)?.polytype
                    && self.cliptype != ClipType::Xor)
            {
                let result_op = self.add_local_max_poly(e1, e2, pt)?;
                if let (true, Some(op)) = (self.has_z_callback(), result_op) {
                    self.set_z_out_pt(e1, e2, op)?;
                }
            } else if self.is_front(e1)? || self.active(e1)?.outrec == self.active(e2)?.outrec {
                // this 'else if' condition isn't strictly needed but
                // it's sensible to split polygons that only touch at
                // a common vertex (not at common edges).
                let result_op = self.add_local_max_poly(e1, e2, pt)?;
                let op2 = self.add_local_min_poly(e1, e2, pt, false)?;
                if let (true, Some(op)) = (self.has_z_callback(), result_op) {
                    self.set_z_out_pt(e1, e2, op)?;
                }
                if self.has_z_callback() {
                    self.set_z_out_pt(e1, e2, op2)?;
                }
            } else {
                let result_op = self.add_out_pt(e1, pt)?;
                let op2 = self.add_out_pt(e2, pt)?;
                if self.has_z_callback() {
                    self.set_z_out_pt(e1, e2, result_op)?;
                    self.set_z_out_pt(e1, e2, op2)?;
                }
                self.swap_outrecs(e1, e2)?;
            }
        } else if hot1 {
            let result_op = self.add_out_pt(e1, pt)?;
            if self.has_z_callback() {
                self.set_z_out_pt(e1, e2, result_op)?;
            }
            self.swap_outrecs(e1, e2)?;
        } else if hot2 {
            let result_op = self.add_out_pt(e2, pt)?;
            if self.has_z_callback() {
                self.set_z_out_pt(e1, e2, result_op)?;
            }
            self.swap_outrecs(e1, e2)?;
        } else {
            let (c1, c2) = (self.active(e1)?.wind_cnt2, self.active(e2)?.wind_cnt2);
            let (e1_wc2, e2_wc2): (i64, i64) = match self.fillrule {
                FillRule::EvenOdd | FillRule::NonZero => {
                    (i64::from(abs32(c1)?), i64::from(abs32(c2)?))
                }
                _ => {
                    if self.fillrule == self.fillpos {
                        (i64::from(c1), i64::from(c2))
                    } else {
                        (i64::from(neg32(c1)?), i64::from(neg32(c2)?))
                    }
                }
            };

            if !self.is_same_poly_type(e1, e2)? {
                let result_op = self.add_local_min_poly(e1, e2, pt, false)?;
                if self.has_z_callback() {
                    self.set_z_out_pt(e1, e2, result_op)?;
                }
            } else if old_e1_windcnt == 1 && old_e2_windcnt == 1 {
                let mut result_op = None;
                match self.cliptype {
                    ClipType::Union => {
                        if e1_wc2 <= 0 && e2_wc2 <= 0 {
                            result_op = Some(self.add_local_min_poly(e1, e2, pt, false)?);
                        }
                    }
                    ClipType::Difference => {
                        let pt1 = self.get_poly_type(e1)?;
                        if (pt1 == PathType::Clip && e1_wc2 > 0 && e2_wc2 > 0)
                            || (pt1 == PathType::Subject && e1_wc2 <= 0 && e2_wc2 <= 0)
                        {
                            result_op = Some(self.add_local_min_poly(e1, e2, pt, false)?);
                        }
                    }
                    ClipType::Xor => {
                        result_op = Some(self.add_local_min_poly(e1, e2, pt, false)?);
                    }
                    _ => {
                        if e1_wc2 > 0 && e2_wc2 > 0 {
                            result_op = Some(self.add_local_min_poly(e1, e2, pt, false)?);
                        }
                    }
                }
                if let (Some(op), true) = (result_op, self.has_z_callback()) {
                    self.set_z_out_pt(e1, e2, op)?;
                }
            }
        }
        Ok(())
    }
}

/// `template <typename T> bool Path2ContainsPath1(const Path<T>& path1, const Path<T>& path2)`
/// (`clipper.h`, unit [`INCLUDE_P002`]), called with the two cleaned paths.
fn path2_contains_path1_paths(path1: &Path64, path2: &Path64) -> Result<bool> {
    crate::clipper::path2_contains_path1(path1, path2)
}

#[cfg(test)]
mod tests;
