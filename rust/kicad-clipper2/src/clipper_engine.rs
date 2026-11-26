// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.engine.h (Copyright Angus Johnson 2010-2024), Boost
// Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `clipper.engine.h`: the clipping engine's data structures and its inline members.
//!
//! The C++ links engine records with raw pointers. This port keeps them in arenas owned by
//! [`ClipperBase`] and links them with typed indices ([`VertexId`], [`OutPtId`], ...); a null
//! pointer is `None`. [`PolyTree`] is a slot arena whose first slot is the tree root. Members whose
//! bodies are written in `clipper.engine.cpp` are owned by the engine source units: lines 1-2271
//! are ported in [`crate::engine_p001`]; the inline bodies here call them, and calls into lines
//! 2272 onward return [`ClipperError::Unported`] at that call.

use crate::clipper_core::{
    area, check_precision_range, scale_path, scale_paths, FillRule, Path, Path64, PathD, Paths64,
    PathsD, Point64, PointD, PointInPolygonResult, Rect64,
};
use crate::engine_p001::VertexListSlot;
use crate::{copy_of, push, reserve, Budget, ClipperError, Coord, Result, ENGINE_SRC_P002};
use std::cell::RefCell;
use std::collections::BinaryHeap;
use std::rc::Rc;

/// `ClipType`. "All clipping operations except for Difference are commutative."
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipType {
    NoClip,
    Intersection,
    Union,
    Difference,
    Xor,
}

/// `PathType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathType {
    Subject,
    Clip,
}

/// `JoinWith`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JoinWith {
    #[default]
    NoJoin,
    Left,
    Right,
}

/// `enum class VertexFlags : uint32_t`. Any `uint32_t` is a valid value of the enumeration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VertexFlags(pub u32);

impl VertexFlags {
    pub const EMPTY: Self = Self(0);
    pub const OPEN_START: Self = Self(1);
    pub const OPEN_END: Self = Self(2);
    pub const LOCAL_MAX: Self = Self(4);
    pub const LOCAL_MIN: Self = Self(8);
}

impl ::core::ops::BitAnd for VertexFlags {
    type Output = Self;
    /// `constexpr VertexFlags operator &(VertexFlags a, VertexFlags b)`.
    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl ::core::ops::BitOr for VertexFlags {
    type Output = Self;
    /// `constexpr VertexFlags operator |(VertexFlags a, VertexFlags b)`.
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

macro_rules! arena_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub usize);
    };
}

/// A `Vertex*`: vertex `index` of the `list`-th vertex array its owner holds (the C++ allocates
/// one array per `AddPaths_` call). Within [`ClipperBase`] the list indexes its vertex list
/// slots, which include the arrays borrowed from a `ReuseableDataContainer64`, so two minima on
/// one vertex share one identity as their pointers do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexId {
    pub list: usize,
    pub index: usize,
}
arena_id!(
    /// An `OutPt*` into [`ClipperBase::out_pts`].
    OutPtId
);
arena_id!(
    /// An `OutRec*` into [`ClipperBase::outrec_list`].
    OutRecId
);
arena_id!(
    /// An `Active*` into [`ClipperBase::active_edges`].
    ActiveId
);
arena_id!(
    /// A `LocalMinima*`: the record's stable slot in the minima store, which `Reset`'s sort of
    /// the (pointer) list never moves.
    LocalMinimaId
);
arena_id!(
    /// A `HorzSegment*` into [`ClipperBase::horz_seg_list`].
    HorzSegmentId
);
/// A `PolyPath*` (a node of a [`PolyTree`]). A destroyed node's slot is reused, so the
/// handle carries the slot's generation: a handle to a node that `Clear` destroyed never
/// aliases a later node, and its use is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolyPathId {
    slot: usize,
    generation: u64,
}

/// `struct Vertex`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub pt: Point64,
    pub next: Option<VertexId>,
    pub prev: Option<VertexId>,
    pub flags: VertexFlags,
}

/// `struct OutPt`.
#[derive(Clone, Copy, Debug)]
pub struct OutPt {
    pub pt: Point64,
    pub next: OutPtId,
    pub prev: OutPtId,
    pub outrec: Option<OutRecId>,
    pub horz: Option<HorzSegmentId>,
}

impl OutPt {
    /// `OutPt(const Point64& pt_, OutRec* outrec_)`: `next = prev = this`, `horz = nullptr`.
    /// `this` is the arena slot the new point will occupy.
    pub fn new(this: OutPtId, pt: &Point64, outrec: Option<OutRecId>) -> Self {
        Self {
            pt: *pt,
            next: this,
            prev: this,
            outrec,
            horz: None,
        }
    }
}

/// `StripeIndex::StripeEdge`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StripeEdge {
    pub a: Point64,
    pub b: Point64,
}

/// `struct StripeIndex`: buckets a ring's edges into horizontal Y stripes so a point in
/// polygon query only scans the stripe spanning `pt.y`. Its builders and query are written in
/// `clipper.engine.cpp`.
// The private members are read by the `clipper.engine.cpp` bodies (engine source units).
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct StripeIndex {
    pub(crate) edges: Vec<StripeEdge>,
    pub(crate) stripes: Vec<Vec<i32>>,
    pub(crate) y_min: i64,
    pub(crate) stripe_height: i64,
    pub(crate) stripe_count: i32,
}

impl Default for StripeIndex {
    /// The default member initializers: no edges, `y_min_ = 0`, `stripe_height_ = 1`,
    /// `stripe_count_ = 1`.
    fn default() -> Self {
        Self {
            edges: Vec::new(),
            stripes: Vec::new(),
            y_min: 0,
            stripe_height: 1,
            stripe_count: 1,
        }
    }
}

impl StripeIndex {
    /// `size_t EdgeCount() const`.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// `bool Empty() const`.
    pub fn empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// `void BuildFromPath(const Path64& path)` (`clipper.engine.cpp`, [`crate::engine_p001`]).
    pub fn build_from_path(&mut self, path: &Path64) -> Result<()> {
        self.build_from_path_impl(path)
    }

    /// `PointInPolygonResult PointInPolygon(const Point64& pt) const`
    /// (`clipper.engine.cpp`, [`crate::engine_p001`]).
    pub fn point_in_polygon(&self, pt: &Point64) -> Result<PointInPolygonResult> {
        self.point_in_polygon_impl(pt)
    }
}

/// `struct OutRec`: a path in the clipping solution. Its destructor deletes `splits` (a
/// `Vec` here) but not the split records, which belong to `ClipperBase::outrec_list_`.
#[derive(Clone, Debug, Default)]
pub struct OutRec {
    pub idx: usize,
    pub owner: Option<OutRecId>,
    pub front_edge: Option<ActiveId>,
    pub back_edge: Option<ActiveId>,
    pub pts: Option<OutPtId>,
    pub polypath: Option<PolyPathId>,
    pub splits: Option<Vec<OutRecId>>,
    pub recursive_split: Option<OutRecId>,
    /// `Rect64 bounds = {}`: value-initialized through `Rect(true)`, all zero.
    pub bounds: Rect64,
    pub path: Path64,
    pub is_open: bool,
    /// "Lazily built during PolyTree nesting; only valid once pts is stable."
    pub pip_index: Option<Box<StripeIndex>>,
}

/// `struct Active`: an edge in the active edge list. UP and DOWN are premised on Y-axis
/// positive down displays.
#[derive(Clone, Copy, Debug)]
pub struct Active {
    pub bot: Point64,
    pub top: Point64,
    /// Current x, updated at every new scanline.
    pub curr_x: i64,
    pub dx: f64,
    /// 1 or -1 depending on winding direction.
    pub wind_dx: i32,
    pub wind_cnt: i32,
    /// Winding count of the opposite polytype.
    pub wind_cnt2: i32,
    pub outrec: Option<OutRecId>,
    /// AEL: the active edge list, left to right within the current scanbeam.
    pub prev_in_ael: Option<ActiveId>,
    pub next_in_ael: Option<ActiveId>,
    /// SEL: the sorted edge list, also reused to process horizontals.
    pub prev_in_sel: Option<ActiveId>,
    pub next_in_sel: Option<ActiveId>,
    pub jump: Option<ActiveId>,
    pub vertex_top: Option<VertexId>,
    /// The bottom of an edge bound.
    pub local_min: Option<LocalMinimaId>,
    pub is_left_bound: bool,
    pub join_with: JoinWith,
}

impl Default for Active {
    /// The default member initializers (`wind_dx = 1`, everything else zero or null).
    fn default() -> Self {
        Self {
            bot: Point64::default(),
            top: Point64::default(),
            curr_x: 0,
            dx: 0.0,
            wind_dx: 1,
            wind_cnt: 0,
            wind_cnt2: 0,
            outrec: None,
            prev_in_ael: None,
            next_in_ael: None,
            prev_in_sel: None,
            next_in_sel: None,
            jump: None,
            vertex_top: None,
            local_min: None,
            is_left_bound: false,
            join_with: JoinWith::NoJoin,
        }
    }
}

/// `struct LocalMinima`.
#[derive(Clone, Copy, Debug)]
pub struct LocalMinima {
    pub vertex: VertexId,
    pub polytype: PathType,
    pub is_open: bool,
}

impl LocalMinima {
    /// `LocalMinima(Vertex* v, PathType pt, bool open)`.
    pub fn new(vertex: VertexId, polytype: PathType, is_open: bool) -> Self {
        Self {
            vertex,
            polytype,
            is_open,
        }
    }
}

/// `struct IntersectNode`.
#[derive(Clone, Copy, Debug)]
pub struct IntersectNode {
    pub pt: Point64,
    pub edge1: Option<ActiveId>,
    pub edge2: Option<ActiveId>,
}

impl Default for IntersectNode {
    /// `IntersectNode() : pt(Point64(0,0)), edge1(NULL), edge2(NULL)`.
    fn default() -> Self {
        Self {
            pt: Point64::xy(0, 0),
            edge1: None,
            edge2: None,
        }
    }
}

impl IntersectNode {
    /// `IntersectNode(Active* e1, Active* e2, Point64& pt_)`.
    pub fn new(edge1: Option<ActiveId>, edge2: Option<ActiveId>, pt: &Point64) -> Self {
        Self {
            pt: *pt,
            edge1,
            edge2,
        }
    }
}

/// `struct HorzSegment`.
#[derive(Clone, Copy, Debug)]
pub struct HorzSegment {
    pub left_op: Option<OutPtId>,
    pub right_op: Option<OutPtId>,
    pub left_to_right: bool,
}

impl Default for HorzSegment {
    /// `HorzSegment() : left_op(nullptr)` with `right_op = nullptr`, `left_to_right = true`.
    fn default() -> Self {
        Self {
            left_op: None,
            right_op: None,
            left_to_right: true,
        }
    }
}

impl HorzSegment {
    /// `explicit HorzSegment(OutPt* op) : left_op(op)`.
    pub fn new(op: Option<OutPtId>) -> Self {
        Self {
            left_op: op,
            ..Self::default()
        }
    }
}

/// `struct HorzJoin`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HorzJoin {
    pub op1: Option<OutPtId>,
    pub op2: Option<OutPtId>,
}

impl HorzJoin {
    /// `explicit HorzJoin(OutPt* ltr, OutPt* rtl) : op1(ltr), op2(rtl)`.
    pub fn new(ltr: Option<OutPtId>, rtl: Option<OutPtId>) -> Self {
        Self { op1: ltr, op2: rtl }
    }
}

/// `ZCallback64`: `void(e1bot, e1top, e2bot, e2top, Point64& pt)`.
pub type ZCallback64 = Box<dyn FnMut(&Point64, &Point64, &Point64, &Point64, &mut Point64)>;
/// `ZCallbackD`: `void(e1bot, e1top, e2bot, e2top, PointD& pt)`.
pub type ZCallbackD = Box<dyn FnMut(&PointD, &PointD, &PointD, &PointD, &mut PointD)>;

/// `ClipperD::zCallbackD_`, shared with the proxy that `CheckCallback` binds, so the engine
/// reads the callback the `ClipperD` holds at the time of the call, as
/// `std::bind(&ClipperD::ZCB, this, ...)` does.
pub(crate) type SharedZCallbackD = Rc<RefCell<Option<ZCallbackD>>>;

/// The content of `ClipperBase::zCallback_` (a `std::function`).
#[derive(Default)]
pub enum ZCallbackSlot {
    /// An empty `std::function`.
    #[default]
    Empty,
    /// A user callback (`Clipper64::SetZCallback`).
    User(ZCallback64),
    /// `std::bind(&ClipperD::ZCB, this, ...)`, assigned by `ClipperD::CheckCallback`: the bound
    /// object's `invScale_` and `zCallbackD_`.
    ClipperDProxy {
        inv_scale: f64,
        callback: SharedZCallbackD,
    },
}

impl ::core::fmt::Debug for ZCallbackSlot {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Empty",
            Self::User(_) => "User",
            Self::ClipperDProxy { .. } => "ClipperDProxy",
        })
    }
}

/// Which callback, if any, `ClipperBase::zCallback_` holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZCallbackKind {
    Empty,
    User,
    ClipperDProxy,
}

/// `class ClipperBase`: engine state. Fields follow the header's declarations and default
/// member initializers; the arenas `out_pts` and `active_edges` hold the records the C++
/// allocates individually with `new`. The private members are read by the
/// `clipper.engine.cpp` bodies (engine source units).
#[allow(dead_code)]
#[derive(Debug)]
pub struct ClipperBase {
    pub(crate) cliptype: ClipType,
    pub(crate) fillrule: FillRule,
    pub(crate) fillpos: FillRule,
    pub(crate) bot_y: i64,
    pub(crate) minima_list_sorted: bool,
    pub(crate) using_polytree: bool,
    pub(crate) actives: Option<ActiveId>,
    pub(crate) sel: Option<ActiveId>,
    /// The `LocalMinima` records `minima_list_`'s `unique_ptr`s own, in creation order.
    pub(crate) minima_store: Vec<LocalMinima>,
    /// `minima_list_`: the order of the records, which `Reset` sorts.
    pub(crate) minima_list: Vec<LocalMinimaId>,
    /// `current_locmin_iter_` as an index into `minima_list`. It is singular until the first
    /// `Reset` or `Clear`, and `AddPaths`/`AddReuseableData` invalidate it (`emplace_back`);
    /// the engine resets it before use, as the C++ does.
    pub(crate) current_locmin_iter: usize,
    /// `vertex_lists_` (owned arrays), plus the arrays of a `ReuseableDataContainer64` that
    /// `AddReuseableData` minima point into (borrowed, never freed here).
    pub(crate) vertex_lists: Vec<VertexListSlot>,
    pub(crate) scanline_list: BinaryHeap<i64>,
    pub(crate) intersect_nodes: Vec<IntersectNode>,
    pub(crate) horz_seg_list: Vec<HorzSegment>,
    pub(crate) horz_join_list: Vec<HorzJoin>,
    /// Every `OutPt` allocated with `new`; `None` once deleted.
    pub(crate) out_pts: Vec<Option<OutPt>>,
    /// Every `Active` allocated with `new`; `None` once deleted.
    pub(crate) active_edges: Vec<Option<Active>>,
    pub(crate) preserve_collinear: bool,
    pub(crate) reverse_solution: bool,
    pub(crate) error_code: i32,
    pub(crate) has_open_paths: bool,
    pub(crate) succeeded: bool,
    /// "Ring edge count below which index build does not pay off; brute force instead."
    /// A `size_t` on the 64-bit reference: `u64` keeps every value representable on wasm32.
    pub(crate) pip_index_threshold: u64,
    pub(crate) outrec_list: Vec<OutRec>,
    pub(crate) z_callback: ZCallbackSlot,
    /// `int64_t DefaultZ = 0` (public).
    pub default_z: i64,
}

impl Default for ClipperBase {
    fn default() -> Self {
        Self {
            cliptype: ClipType::NoClip,
            fillrule: FillRule::EvenOdd,
            fillpos: FillRule::Positive,
            bot_y: 0,
            minima_list_sorted: false,
            using_polytree: false,
            actives: None,
            sel: None,
            minima_store: Vec::new(),
            minima_list: Vec::new(),
            current_locmin_iter: 0,
            vertex_lists: Vec::new(),
            scanline_list: BinaryHeap::new(),
            intersect_nodes: Vec::new(),
            horz_seg_list: Vec::new(),
            horz_join_list: Vec::new(),
            out_pts: Vec::new(),
            active_edges: Vec::new(),
            preserve_collinear: true,
            reverse_solution: false,
            error_code: 0,
            has_open_paths: false,
            succeeded: true,
            pip_index_threshold: 64,
            outrec_list: Vec::new(),
            z_callback: ZCallbackSlot::Empty,
            default_z: 0,
        }
    }
}

impl ClipperBase {
    /// `int ErrorCode() const`.
    pub fn error_code(&self) -> i32 {
        self.error_code
    }

    /// `void PreserveCollinear(bool val)`.
    pub fn set_preserve_collinear(&mut self, val: bool) {
        self.preserve_collinear = val;
    }

    /// `bool PreserveCollinear() const`.
    pub fn preserve_collinear(&self) -> bool {
        self.preserve_collinear
    }

    /// `void ReverseSolution(bool val)`.
    pub fn set_reverse_solution(&mut self, val: bool) {
        self.reverse_solution = val;
    }

    /// `bool ReverseSolution() const`.
    pub fn reverse_solution(&self) -> bool {
        self.reverse_solution
    }

    /// `void PipIndexThreshold(size_t val)`: "Testing knob. High forces brute-force nesting,
    /// low forces stripe indexing."
    pub fn set_pip_index_threshold(&mut self, val: u64) {
        self.pip_index_threshold = val;
    }

    /// `size_t PipIndexThreshold() const`.
    pub fn pip_index_threshold(&self) -> u64 {
        self.pip_index_threshold
    }

    /// Which callback the protected `zCallback_` holds (the oracle reads it through a
    /// derived probe class).
    pub fn z_callback_kind(&self) -> ZCallbackKind {
        match self.z_callback {
            ZCallbackSlot::Empty => ZCallbackKind::Empty,
            ZCallbackSlot::User(_) => ZCallbackKind::User,
            ZCallbackSlot::ClipperDProxy { .. } => ZCallbackKind::ClipperDProxy,
        }
    }

    /// `bool ExecuteInternal(ClipType ct, FillRule ft, bool use_polytrees)`.
    pub fn execute_internal(
        &mut self,
        _ct: ClipType,
        _ft: FillRule,
        _use_polytrees: bool,
    ) -> Result<bool> {
        // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002): Clipper2Lib::ClipperBase::ExecuteInternal
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::ClipperBase::ExecuteInternal",
            unit: ENGINE_SRC_P002,
        })
    }
}

// --- PolyPath / PolyTree -----------------------------------------------------------------

#[derive(Debug)]
struct PolyNode<T> {
    parent: Option<PolyPathId>,
    childs: Vec<PolyPathId>,
    polygon: Path<T>,
    /// `PolyPathD::scale_` (1.0 and unused for `PolyPath64`).
    scale: f64,
}

/// One arena slot: `node` is `None` once the owning `childs_` vector destroyed it, and
/// `generation` advances at each destruction so stale handles stay detectable.
#[derive(Debug)]
struct Slot<T> {
    generation: u64,
    node: Option<PolyNode<T>>,
}

/// A `PolyPath64`/`PolyPathD` tree: "intended as a READ-ONLY data structure for CLOSED paths
/// returned by clipping operations", preserving which paths contain (own) others.
///
/// Node [`PolyTree::ROOT`] is the object the caller constructed; `AddChild` appends nodes.
/// `Clear` destroys every descendant as the C++ does: their polygons are dropped and their
/// slots are reused by later children. Using a node after an ancestor's `Clear` destroyed it
/// is undefined in the C++ (a dangling pointer) and is refused here. The live nodes and their
/// points are bounded by the host's [`Budget`].
#[derive(Debug)]
pub struct PolyTree<T> {
    slots: Vec<Slot<T>>,
    /// Destroyed slots available for reuse. Its capacity always covers every slot, so `Clear`
    /// never allocates.
    free: Vec<usize>,
    budget: Budget,
    live_nodes: usize,
    live_points: usize,
}

/// `PolyTree64` (= `PolyPath64`).
pub type PolyTree64 = PolyTree<i64>;
/// `PolyTreeD` (= `PolyPathD`).
pub type PolyTreeD = PolyTree<f64>;

const DESTROYED: &str = "use of a destroyed PolyPath";

impl<T: Coord> PolyTree<T> {
    /// The constructed object itself.
    pub const ROOT: PolyPathId = PolyPathId {
        slot: 0,
        generation: 0,
    };

    /// `explicit PolyPath64(PolyPath64* parent = nullptr)` / `PolyPathD(nullptr)` (scale 1),
    /// within the host's `budget`.
    pub fn new(budget: Budget) -> Result<Self> {
        Self::with_polygon(budget, Vec::new())
    }

    /// `explicit PolyPath64(nullptr, const Path64& path)` / `PolyPathD(nullptr, const PathD&)`.
    pub fn with_polygon(budget: Budget, polygon: Path<T>) -> Result<Self> {
        if budget.nodes < 1 || polygon.len() > budget.points {
            return Err(ClipperError::ResourceLimit);
        }
        let mut slots = Vec::new();
        reserve(&mut slots, 1)?;
        let mut free = Vec::new();
        reserve(&mut free, 1)?;
        let live_points = polygon.len();
        slots.push(Slot {
            generation: 0,
            node: Some(PolyNode {
                parent: None,
                childs: Vec::new(),
                polygon,
                scale: 1.0,
            }),
        });
        Ok(Self {
            slots,
            free,
            budget,
            live_nodes: 1,
            live_points,
        })
    }

    fn node(&self, id: PolyPathId) -> Result<&PolyNode<T>> {
        self.slots
            .get(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_ref())
            .ok_or(ClipperError::Undefined(DESTROYED))
    }

    fn node_mut(&mut self, id: PolyPathId) -> Result<&mut PolyNode<T>> {
        self.slots
            .get_mut(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
            .ok_or(ClipperError::Undefined(DESTROYED))
    }

    /// `unsigned Level() const`: the number of ancestors.
    pub fn level(&self, id: PolyPathId) -> Result<u32> {
        let mut result: u32 = 0;
        let mut p = self.node(id)?.parent;
        while let Some(parent) = p {
            result = result.wrapping_add(1);
            p = self.node(parent)?.parent;
        }
        Ok(result)
    }

    /// `bool IsHole() const`: "Even levels except level 0".
    pub fn is_hole(&self, id: PolyPathId) -> Result<bool> {
        let lvl = self.level(id)?;
        Ok(lvl != 0 && lvl & 1 == 0)
    }

    /// `const PolyPath* Parent() const`.
    pub fn parent(&self, id: PolyPathId) -> Result<Option<PolyPathId>> {
        Ok(self.node(id)?.parent)
    }

    /// `size_t Count() const override`.
    pub fn count(&self, id: PolyPathId) -> Result<usize> {
        Ok(self.node(id)?.childs.len())
    }

    /// `Child(size_t index) const`; also `operator[]`. Out of range is undefined.
    pub fn child(&self, id: PolyPathId, index: usize) -> Result<PolyPathId> {
        self.node(id)?
            .childs
            .get(index)
            .copied()
            .ok_or(ClipperError::Undefined("PolyPath child index out of range"))
    }

    /// `begin()`/`end()`: the children in order.
    pub fn children(&self, id: PolyPathId) -> Result<&[PolyPathId]> {
        Ok(&self.node(id)?.childs)
    }

    /// `const Path& Polygon() const`.
    pub fn polygon(&self, id: PolyPathId) -> Result<&Path<T>> {
        Ok(&self.node(id)?.polygon)
    }

    /// `void Clear() override`: `childs_.resize(0)` destroys every descendant. Each destroyed
    /// node's polygon is dropped and its slot freed for reuse, without allocating: the walk
    /// descends through the remaining children and climbs back by parent links.
    pub fn clear(&mut self, id: PolyPathId) -> Result<()> {
        let mut current = match self.node_mut(id)?.childs.pop() {
            Some(child) => child,
            None => return Ok(()),
        };
        loop {
            while let Some(child) = self.node_mut(current)?.childs.pop() {
                current = child;
            }
            let parent = self.node(current)?.parent;
            self.destroy(current)?;
            current = match parent {
                Some(parent) if parent != id => parent,
                _ => match self.node_mut(id)?.childs.pop() {
                    Some(child) => child,
                    None => return Ok(()),
                },
            };
        }
    }

    /// Destroy one childless node: drop its payload, advance its generation and free its slot.
    fn destroy(&mut self, id: PolyPathId) -> Result<()> {
        let slot = self
            .slots
            .get_mut(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .ok_or(ClipperError::Undefined(DESTROYED))?;
        let node = slot.node.take().ok_or(ClipperError::Undefined(DESTROYED))?;
        self.live_nodes = self.live_nodes.saturating_sub(1);
        self.live_points = self.live_points.saturating_sub(node.polygon.len());
        // A slot whose generation cannot advance is retired rather than reused.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            if self.free.len() < self.free.capacity() {
                self.free.push(id.slot);
            }
        }
        Ok(())
    }

    /// Admit one more node holding `points` points under the budget, before any allocation.
    fn admit(&self, id: PolyPathId, points: usize) -> Result<()> {
        self.node(id)?;
        let nodes = self.live_nodes.checked_add(1);
        let total = self.live_points.checked_add(points);
        match (nodes, total) {
            (Some(nodes), Some(total))
                if nodes <= self.budget.nodes && total <= self.budget.points =>
            {
                Ok(())
            }
            _ => Err(ClipperError::ResourceLimit),
        }
    }

    fn push_child(&mut self, id: PolyPathId, polygon: Path<T>, scale: f64) -> Result<PolyPathId> {
        self.admit(id, polygon.len())?;
        reserve(&mut self.node_mut(id)?.childs, 1)?;
        let points = polygon.len();
        let node = PolyNode {
            parent: Some(id),
            childs: Vec::new(),
            polygon,
            scale,
        };
        let child = match self.free.pop() {
            Some(index) => {
                let slot = self
                    .slots
                    .get_mut(index)
                    .ok_or(ClipperError::Undefined(DESTROYED))?;
                slot.node = Some(node);
                PolyPathId {
                    slot: index,
                    generation: slot.generation,
                }
            }
            None => {
                reserve(&mut self.slots, 1)?;
                // Keep room to free every slot, so `Clear` never allocates.
                let room = self.slots.len().saturating_add(1);
                let additional = room.saturating_sub(self.free.len());
                reserve(&mut self.free, additional)?;
                let index = self.slots.len();
                self.slots.push(Slot {
                    generation: 0,
                    node: Some(node),
                });
                PolyPathId {
                    slot: index,
                    generation: 0,
                }
            }
        };
        self.node_mut(id)?.childs.push(child);
        self.live_nodes = self.live_nodes.saturating_add(1);
        self.live_points = self.live_points.saturating_add(points);
        Ok(child)
    }

    /// `double Area() const`: `std::accumulate` over the children, starting from this
    /// polygon's area, adding each child's (recursive) area in order.
    pub fn area(&self, id: PolyPathId) -> Result<f64> {
        // Explicit post-order stack: (node, running sum, next child index).
        let mut stack = Vec::new();
        push(&mut stack, (id, area(&self.node(id)?.polygon)?, 0usize))?;
        loop {
            let Some(top) = stack.last_mut() else {
                return Err(ClipperError::Undefined(DESTROYED));
            };
            let (node, _, next) = *top;
            let childs = &self.node(node)?.childs;
            if let Some(&child) = childs.get(next) {
                top.2 = next + 1;
                push(&mut stack, (child, area(&self.node(child)?.polygon)?, 0))?;
                continue;
            }
            let Some((_, sum, _)) = stack.pop() else {
                return Err(ClipperError::Undefined(DESTROYED));
            };
            match stack.last_mut() {
                Some(parent) => parent.1 += sum,
                None => return Ok(sum),
            }
        }
    }
}

impl PolyTree<i64> {
    /// `PolyPath64* AddChild(const Path64& path) override`.
    pub fn add_child(&mut self, id: PolyPathId, path: &Path64) -> Result<PolyPathId> {
        self.admit(id, path.len())?;
        self.push_child(id, copy_of(path)?, 1.0)
    }
}

impl PolyTree<f64> {
    /// `explicit PolyPathD(nullptr, const Path64& path)`: the root keeps scale 1.0.
    pub fn with_polygon64(budget: Budget, path: &Path64) -> Result<Self> {
        if path.len() > budget.points {
            return Err(ClipperError::ResourceLimit);
        }
        let mut error_code = 0;
        Self::with_polygon(budget, scale_path::<f64, i64>(path, 1.0, &mut error_code)?)
    }

    /// `void SetScale(double value)`.
    pub fn set_scale(&mut self, id: PolyPathId, value: f64) -> Result<()> {
        self.node_mut(id)?.scale = value;
        Ok(())
    }

    /// `double Scale() const`.
    pub fn scale(&self, id: PolyPathId) -> Result<f64> {
        Ok(self.node(id)?.scale)
    }

    /// `PolyPathD* AddChild(const Path64& path) override`: the child takes the parent's
    /// scale and stores `ScalePath<double, int64_t>(path, scale_, error_code)`. A zero scale
    /// throws from the child's constructor, so no child is added.
    pub fn add_child(&mut self, id: PolyPathId, path: &Path64) -> Result<PolyPathId> {
        let scale = self.node(id)?.scale;
        self.admit(id, path.len())?;
        let mut error_code = 0;
        let polygon = scale_path::<f64, i64>(path, scale, &mut error_code)?;
        self.push_child(id, polygon, scale)
    }

    /// `PolyPathD* AddChild(const PathD& path)`.
    pub fn add_child_d(&mut self, id: PolyPathId, path: &PathD) -> Result<PolyPathId> {
        let scale = self.node(id)?.scale;
        self.admit(id, path.len())?;
        self.push_child(id, copy_of(path)?, scale)
    }
}

/// `virtual size_t PolyPath::Count() const { return 0; }`, the base class body that the
/// concrete trees override.
pub fn poly_path_base_count() -> usize {
    0
}

// --- Clipper64 / ClipperD ----------------------------------------------------------------

/// `class Clipper64 : public ClipperBase`.
#[derive(Debug, Default)]
pub struct Clipper64 {
    pub base: ClipperBase,
}

impl Clipper64 {
    /// `void SetZCallback(ZCallback64 cb)`; `None` is an empty `std::function`.
    pub fn set_z_callback(&mut self, cb: Option<ZCallback64>) {
        self.base.z_callback = cb.map_or(ZCallbackSlot::Empty, ZCallbackSlot::User);
    }

    /// `void AddSubject(const Paths64& subjects)`.
    pub fn add_subject(&mut self, subjects: &Paths64) -> Result<()> {
        self.base.add_paths(subjects, PathType::Subject, false)
    }

    /// `void AddOpenSubject(const Paths64& open_subjects)`.
    pub fn add_open_subject(&mut self, open_subjects: &Paths64) -> Result<()> {
        self.base.add_paths(open_subjects, PathType::Subject, true)
    }

    /// `void AddClip(const Paths64& clips)`.
    pub fn add_clip(&mut self, clips: &Paths64) -> Result<()> {
        self.base.add_paths(clips, PathType::Clip, false)
    }

    /// `bool Execute(ClipType, FillRule, Paths64& closed_paths)`.
    pub fn execute(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        closed_paths: &mut Paths64,
    ) -> Result<bool> {
        let mut dummy = Paths64::new();
        self.execute_with_open(clip_type, fill_rule, closed_paths, &mut dummy)
    }

    /// `bool Execute(ClipType, FillRule, Paths64& closed_paths, Paths64& open_paths)`.
    pub fn execute_with_open(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        closed_paths: &mut Paths64,
        open_paths: &mut Paths64,
    ) -> Result<bool> {
        closed_paths.clear();
        open_paths.clear();
        if self.base.execute_internal(clip_type, fill_rule, false)? {
            self.build_paths64(closed_paths, Some(open_paths))?;
        }
        self.base.clean_up()?;
        Ok(self.base.succeeded)
    }

    /// `bool Execute(ClipType, FillRule, PolyTree64& polytree)`.
    pub fn execute_tree(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        polytree: &mut PolyTree64,
    ) -> Result<bool> {
        let mut dummy = Paths64::new();
        self.execute_tree_with_open(clip_type, fill_rule, polytree, &mut dummy)
    }

    /// `bool Execute(ClipType, FillRule, PolyTree64& polytree, Paths64& open_paths)`.
    pub fn execute_tree_with_open(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        polytree: &mut PolyTree64,
        open_paths: &mut Paths64,
    ) -> Result<bool> {
        if self.base.execute_internal(clip_type, fill_rule, true)? {
            open_paths.clear();
            polytree.clear(PolyTree64::ROOT)?;
            self.build_tree64(polytree, open_paths)?;
        }
        self.base.clean_up()?;
        Ok(self.base.succeeded)
    }

    /// `void BuildPaths64(Paths64& solutionClosed, Paths64* solutionOpen)`.
    fn build_paths64(&mut self, _closed: &mut Paths64, _open: Option<&mut Paths64>) -> Result<()> {
        // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002): Clipper2Lib::Clipper64::BuildPaths64
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::Clipper64::BuildPaths64",
            unit: ENGINE_SRC_P002,
        })
    }

    /// `void BuildTree64(PolyPath64& polytree, Paths64& open_paths)`.
    fn build_tree64(&mut self, _polytree: &mut PolyTree64, _open: &mut Paths64) -> Result<()> {
        // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002): Clipper2Lib::Clipper64::BuildTree64
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::Clipper64::BuildTree64",
            unit: ENGINE_SRC_P002,
        })
    }
}

/// `class ClipperD : public ClipperBase`: clips `double` paths by scaling them to integers.
pub struct ClipperD {
    pub base: ClipperBase,
    scale: f64,
    inv_scale: f64,
    z_callback_d: SharedZCallbackD,
}

impl ::core::fmt::Debug for ClipperD {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter
            .debug_struct("ClipperD")
            .field("base", &self.base)
            .field("scale", &self.scale)
            .field("inv_scale", &self.inv_scale)
            .field(
                "z_callback_d",
                &self.z_callback_d.try_borrow().map(|cb| cb.is_some()),
            )
            .finish()
    }
}

/// A `zCallbackD_` call that re-entered the `ClipperD` it belongs to.
const REENTRANT: &str = "re-entrant ClipperD Z callback";

/// `void ClipperD::ZCB(e1bot, e1top, e2bot, e2top, Point64& pt)` for the object whose
/// `invScale_` and `zCallbackD_` are given: descales the points, calls the user callback and
/// copies back only `z`. An empty callback throws `std::bad_function_call`.
pub(crate) fn clipper_d_zcb(
    inv_scale: f64,
    z_callback_d: &SharedZCallbackD,
    e1bot: &Point64,
    e1top: &Point64,
    e2bot: &Point64,
    e2top: &Point64,
    pt: &mut Point64,
) -> Result<()> {
    let mut tmp = PointD::from_point(pt)?.scaled(inv_scale)?;
    let e1b = PointD::from_point(e1bot)?.scaled(inv_scale)?;
    let e1t = PointD::from_point(e1top)?.scaled(inv_scale)?;
    let e2b = PointD::from_point(e2bot)?.scaled(inv_scale)?;
    let e2t = PointD::from_point(e2top)?.scaled(inv_scale)?;
    let mut slot = z_callback_d
        .try_borrow_mut()
        .map_err(|_| ClipperError::Undefined(REENTRANT))?;
    let callback = slot.as_mut().ok_or(ClipperError::BadFunctionCall)?;
    callback(&e1b, &e1t, &e2b, &e2t, &mut tmp);
    pt.z = tmp.z; // only update 'z'
    Ok(())
}

/// `std::ilogb` of a finite nonzero normal double (the only kind `ClipperD` passes).
fn ilogb_normal(value: f64) -> Result<i32> {
    let exponent = ((value.to_bits() >> 52) & 0x7ff) as i32;
    if exponent == 0 || exponent == 0x7ff {
        return Err(ClipperError::Undefined("ilogb of a non-normal value"));
    }
    Ok(exponent - 1023)
}

/// `std::pow(2, k)` for a result in the normal range: exactly `2^k`.
fn pow2_normal(k: i32) -> Result<f64> {
    if !(-1022..=1023).contains(&k) {
        return Err(ClipperError::Undefined(
            "pow(2, k) outside the normal range",
        ));
    }
    let biased = u64::try_from(k + 1023).map_err(|_| ClipperError::Undefined("pow(2, k)"))?;
    Ok(f64::from_bits(biased << 52))
}

/// `std::pow(10, precision)` for `|precision| <= CLIPPER2_MAX_DEC_PRECISION`. `10^p` is exact
/// for `p >= 0`; for `p < 0` this is the correctly rounded `1 / 10^-p`. Only its binary
/// exponent is used, which no last-place difference of a libm `pow` can change: every
/// `10^-p` here has a significand far from 1 and 2.
fn pow10_exponent(precision: i32) -> Result<i32> {
    let magnitude = precision.unsigned_abs();
    let power = 10u64
        .checked_pow(magnitude)
        .ok_or(ClipperError::Undefined("pow(10, precision)"))?;
    let value = if precision >= 0 {
        power as f64
    } else {
        1.0 / power as f64
    };
    ilogb_normal(value)
}

impl ClipperD {
    /// `explicit ClipperD(int precision = 2)`. Out-of-range precision throws from
    /// `CheckPrecisionRange`. The scale is `2^(ilogb(10^precision) + 1)`: "set the scale to a
    /// power of double's radix (2)".
    pub fn new(precision: i32) -> Result<Self> {
        let mut base = ClipperBase::default();
        let mut precision = precision;
        check_precision_range(&mut precision, &mut base.error_code)?;
        let exponent = pow10_exponent(precision)?
            .checked_add(1)
            .ok_or(ClipperError::Undefined("int overflow"))?;
        let scale = pow2_normal(exponent)?;
        let inv_scale = 1.0 / scale;
        Ok(Self {
            base,
            scale,
            inv_scale,
            z_callback_d: Rc::new(RefCell::new(None)),
        })
    }

    /// `ClipperD()`: the default precision 2.
    pub fn with_default_precision() -> Result<Self> {
        Self::new(2)
    }

    /// `void SetZCallback(ZCallbackD cb)`; `None` is an empty `std::function`.
    pub fn set_z_callback(&mut self, cb: Option<ZCallbackD>) -> Result<()> {
        *self
            .z_callback_d
            .try_borrow_mut()
            .map_err(|_| ClipperError::Undefined(REENTRANT))? = cb;
        Ok(())
    }

    /// `void ZCB(e1bot, e1top, e2bot, e2top, Point64& pt)`: descales the points, calls the
    /// user callback and copies back only `z`. An empty callback throws
    /// `std::bad_function_call`.
    pub fn zcb(
        &mut self,
        e1bot: &Point64,
        e1top: &Point64,
        e2bot: &Point64,
        e2top: &Point64,
        pt: &mut Point64,
    ) -> Result<()> {
        clipper_d_zcb(
            self.inv_scale,
            &self.z_callback_d,
            e1bot,
            e1top,
            e2bot,
            e2top,
            pt,
        )
    }

    /// `void CheckCallback()`: bind the proxy when a user callback is assigned, else clear.
    pub fn check_callback(&mut self) -> Result<()> {
        let assigned = self
            .z_callback_d
            .try_borrow()
            .map_err(|_| ClipperError::Undefined(REENTRANT))?
            .is_some();
        self.base.z_callback = if assigned {
            ZCallbackSlot::ClipperDProxy {
                inv_scale: self.inv_scale,
                callback: Rc::clone(&self.z_callback_d),
            }
        } else {
            ZCallbackSlot::Empty
        };
        Ok(())
    }

    /// Invoke the protected `zCallback_` as the engine's `SetZ` does, dispatching the bound
    /// proxy to [`ClipperD::zcb`]. An empty `std::function` throws `std::bad_function_call`.
    pub fn call_base_z_callback(
        &mut self,
        e1bot: &Point64,
        e1top: &Point64,
        e2bot: &Point64,
        e2top: &Point64,
        pt: &mut Point64,
    ) -> Result<()> {
        self.base.call_z_callback(e1bot, e1top, e2bot, e2top, pt)
    }

    /// `void AddSubject(const PathsD& subjects)`: `ScalePaths<int64_t, double>` into
    /// `error_code_`, then `AddPaths`.
    pub fn add_subject(&mut self, subjects: &PathsD) -> Result<()> {
        let scaled = scale_paths::<i64, f64>(subjects, self.scale, &mut self.base.error_code)?;
        self.base.add_paths(&scaled, PathType::Subject, false)
    }

    /// `void AddOpenSubject(const PathsD& open_subjects)`.
    pub fn add_open_subject(&mut self, open_subjects: &PathsD) -> Result<()> {
        let scaled = scale_paths::<i64, f64>(open_subjects, self.scale, &mut self.base.error_code)?;
        self.base.add_paths(&scaled, PathType::Subject, true)
    }

    /// `void AddClip(const PathsD& clips)`.
    pub fn add_clip(&mut self, clips: &PathsD) -> Result<()> {
        let scaled = scale_paths::<i64, f64>(clips, self.scale, &mut self.base.error_code)?;
        self.base.add_paths(&scaled, PathType::Clip, false)
    }

    /// `bool Execute(ClipType, FillRule, PathsD& closed_paths)`.
    pub fn execute(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        closed_paths: &mut PathsD,
    ) -> Result<bool> {
        let mut dummy = PathsD::new();
        self.execute_with_open(clip_type, fill_rule, closed_paths, &mut dummy)
    }

    /// `bool Execute(ClipType, FillRule, PathsD& closed_paths, PathsD& open_paths)`. Unlike
    /// `Clipper64`, the outputs are not cleared here.
    pub fn execute_with_open(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        closed_paths: &mut PathsD,
        open_paths: &mut PathsD,
    ) -> Result<bool> {
        self.check_callback()?;
        if self.base.execute_internal(clip_type, fill_rule, false)? {
            self.build_paths_d(closed_paths, Some(open_paths))?;
        }
        self.base.clean_up()?;
        Ok(self.base.succeeded)
    }

    /// `bool Execute(ClipType, FillRule, PolyTreeD& polytree)`.
    pub fn execute_tree(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        polytree: &mut PolyTreeD,
    ) -> Result<bool> {
        let mut dummy = PathsD::new();
        self.execute_tree_with_open(clip_type, fill_rule, polytree, &mut dummy)
    }

    /// `bool Execute(ClipType, FillRule, PolyTreeD& polytree, PathsD& open_paths)`.
    pub fn execute_tree_with_open(
        &mut self,
        clip_type: ClipType,
        fill_rule: FillRule,
        polytree: &mut PolyTreeD,
        open_paths: &mut PathsD,
    ) -> Result<bool> {
        self.check_callback()?;
        if self.base.execute_internal(clip_type, fill_rule, true)? {
            polytree.clear(PolyTreeD::ROOT)?;
            polytree.set_scale(PolyTreeD::ROOT, self.inv_scale)?;
            open_paths.clear();
            self.build_tree_d(polytree, open_paths)?;
        }
        self.base.clean_up()?;
        Ok(self.base.succeeded)
    }

    /// `void BuildPathsD(PathsD& solutionClosed, PathsD* solutionOpen)`.
    fn build_paths_d(&mut self, _closed: &mut PathsD, _open: Option<&mut PathsD>) -> Result<()> {
        // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002): Clipper2Lib::ClipperD::BuildPathsD
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::ClipperD::BuildPathsD",
            unit: ENGINE_SRC_P002,
        })
    }

    /// `void BuildTreeD(PolyPathD& polytree, PathsD& open_paths)`.
    fn build_tree_d(&mut self, _polytree: &mut PolyTreeD, _open: &mut PathsD) -> Result<()> {
        // UNPORTED(thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002): Clipper2Lib::ClipperD::BuildTreeD
        Err(ClipperError::Unported {
            upstream: "Clipper2Lib::ClipperD::BuildTreeD",
            unit: ENGINE_SRC_P002,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipper_d_scales_are_radix_powers() {
        let expected = [(-8, -26), (-1, -3), (0, 1), (2, 7), (8, 27)];
        for (precision, exponent) in expected {
            let clipper = ClipperD::new(precision);
            assert!(clipper.is_ok());
            if let Ok(clipper) = clipper {
                assert_eq!(clipper.scale, 2f64.powi(exponent));
                assert_eq!(clipper.inv_scale, 1.0 / 2f64.powi(exponent));
            }
        }
        assert!(matches!(ClipperD::new(9), Err(ClipperError::Exception(_))));
    }

    const BUDGET: Budget = Budget {
        nodes: 8,
        points: 64,
    };

    #[test]
    fn polytree_clear_releases_nodes_and_refuses_stale_handles() {
        let square = vec![
            Point64::xy(0, 0),
            Point64::xy(10, 0),
            Point64::xy(10, 10),
            Point64::xy(0, 10),
        ];
        let tree = PolyTree64::new(BUDGET);
        assert!(tree.is_ok());
        let Ok(mut tree) = tree else { return };
        let mut stale = Vec::new();
        // Far more add/clear rounds than the budget admits live nodes: destroyed nodes are
        // released and their slots reused, so the arena stays at its peak.
        for _ in 0..1000 {
            let child = tree.add_child(PolyTree64::ROOT, &square);
            assert!(child.is_ok());
            if let Ok(child) = child {
                assert!(tree.add_child(child, &square).is_ok());
                stale.push(child);
            }
            assert!(tree.clear(PolyTree64::ROOT).is_ok());
            assert_eq!((tree.live_nodes, tree.live_points), (1, 0));
        }
        assert!(tree.slots.len() <= 3);
        for child in stale {
            assert_eq!(tree.level(child), Err(ClipperError::Undefined(DESTROYED)));
            assert_eq!(
                tree.add_child(child, &square),
                Err(ClipperError::Undefined(DESTROYED))
            );
        }
        // The budget bounds live nodes and points; nothing changes on refusal.
        for _ in 1..BUDGET.nodes {
            assert!(tree.add_child(PolyTree64::ROOT, &Vec::new()).is_ok());
        }
        assert_eq!(
            tree.add_child(PolyTree64::ROOT, &Vec::new()),
            Err(ClipperError::ResourceLimit)
        );
        assert_eq!(tree.count(PolyTree64::ROOT), Ok(BUDGET.nodes - 1));
        assert!(tree.clear(PolyTree64::ROOT).is_ok());
        let many = vec![Point64::xy(0, 0); BUDGET.points + 1];
        assert_eq!(
            tree.add_child(PolyTree64::ROOT, &many),
            Err(ClipperError::ResourceLimit)
        );
        assert!(PolyTree64::with_polygon(BUDGET, many).is_err());
    }

    #[test]
    fn polytree_area_accumulates_in_child_order() {
        let tree = PolyTree64::new(BUDGET);
        assert!(tree.is_ok());
        let Ok(mut tree) = tree else { return };
        let square = vec![
            Point64::xy(0, 0),
            Point64::xy(10, 0),
            Point64::xy(10, 10),
            Point64::xy(0, 10),
        ];
        let child = tree.add_child(PolyTree64::ROOT, &square);
        assert!(child.is_ok());
        if let Ok(child) = child {
            assert!(tree.add_child(child, &square).is_ok());
            assert_eq!(tree.area(PolyTree64::ROOT), Ok(200.0));
            assert_eq!(tree.level(child), Ok(1));
            assert!(tree.clear(PolyTree64::ROOT).is_ok());
            assert!(matches!(tree.level(child), Err(ClipperError::Undefined(_))));
        }
    }
}
