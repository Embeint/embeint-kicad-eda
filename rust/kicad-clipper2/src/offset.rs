// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.offset.cpp and clipper.offset.h (Copyright Angus
// Johnson 2010-2025), Boost Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint,
// GPL-3.0-or-later.

//! `clipper.offset.cpp` (path offsetting: inflate/shrink), the offset half of unit
//! `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002`, with KiCad's `USINGZ`.
//!
//! The class itself is declared in `clipper.offset.h`, owned by unit
//! `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`. This module carries
//! the minimum co-port of that header the bodies need: [`JoinType`], [`EndType`],
//! [`DeltaCallback64`], [`Group`]'s members and [`ClipperOffset`]'s members, constructor,
//! accessors, `Clear`, `SetZCallback` and `SetDeltaCallback`, plus an explicit host [`Budget`]
//! on the points one execution may produce (a huge delta with a fine arc tolerance otherwise
//! asks for unbounded memory). Both callbacks may throw in C++: a [`DeltaCallback64`] or
//! [`FallibleZCallback64`] returns that exception as an error, which propagates out of
//! `Execute` at once and leaves the state the unwinding C++ leaves (the groups and both
//! callbacks stay in the object, never moved out, so a native unwind keeps them too). `Ellipse` (`clipper.h`), used for a single point with round
//! joins, stays owned by that header unit and returns [`ClipperError::Unported`] at its call.
//!
//! Floating point follows the oracle's `-ffp-contract=off` in source order. `std::sin`,
//! `std::cos`, `std::acos` and `std::atan2` are the pinned glibc 2.39 bodies from
//! [`kicad_math::reference_libm`]; `std::sqrt` and `std::ceil` are exact. `std::min` keeps its
//! `(b < a) ? b : a` NaN behaviour. Conversions the C++ leaves undefined (out-of-range or NaN
//! `double` to integer, signed overflow, out-of-range indexing) are refused with
//! [`ClipperError::Undefined`].

use crate::clipper_core::{
    area, cross_product2, dot_product2, get_line_intersect_pt, reflect_point,
    strip_duplicates_path, translate_point, FillRule, Path64, PathD, Paths64, Point64, PointD,
    Rect64,
};
use crate::clipper_engine::{ClipType, Clipper64, PolyTree64, ZCallback64, ZCallbackSlot};
use crate::{copy_of, push, reserve, Budget, ClipperError, Result};
use kicad_math::reference_libm::{acos, atan2, cos, sin};
use std::cell::RefCell;
use std::rc::Rc;

/// This port unit.
pub const UNIT: &str = crate::ENGINE_SRC_P002;
/// Owner of `clipper.h` and `clipper.offset.h`.
pub const INCLUDE_P002: &str =
    "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002";

/// `const double floating_point_tolerance = 1e-12;`
pub const FLOATING_POINT_TOLERANCE: f64 = 1e-12;
/// `const double arc_const = 0.002; // <-- 1/500`. "When the user doesn't define an arc
/// tolerance (ie leaves alone the 0 default value), the calculated default arc tolerance
/// (offset_radius / 500) generally produces good (smooth) arc approximations without
/// producing excessively small segment lengths."
pub const ARC_CONST: f64 = 0.002;
/// `PI` (`clipper.core.h`: `3.141592653589793238`, the nearest double to pi).
pub const PI: f64 = ::core::f64::consts::PI;
/// `MAX_DBL` (`clipper.core.h`).
const MAX_DBL: f64 = f64::MAX;

const INDEX: ClipperError = ClipperError::Undefined("vector index out of range");
const I64_OVERFLOW: ClipperError = ClipperError::Undefined("int64_t overflow");
/// The offset's `zCallback64_` reached from inside its own call (no ported caller can).
const REENTRANT: ClipperError = ClipperError::Undefined("re-entrant ClipperOffset Z callback");
/// `groups_` changed while an execution reads them (no ported caller can).
const GROUPS_IN_USE: ClipperError = ClipperError::Undefined("ClipperOffset groups in use");

/// `enum class JoinType { Square, Bevel, Round, Miter }`. "Square: Joins are 'squared' at
/// exactly the offset distance (more complex code). Bevel: Similar to Square, but the offset
/// distance varies with angle (simple code & faster)."
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinType {
    Square,
    Bevel,
    Round,
    Miter,
}

/// `enum class EndType {Polygon, Joined, Butt, Square, Round}`. "Butt: offsets both sides of
/// a path, with square blunt ends; Square: ... with square extended ends; Round: ... with
/// round extended ends; Joined: ... with joined ends; Polygon: offsets only one side of a
/// closed path."
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndType {
    Polygon,
    Joined,
    Butt,
    Square,
    Round,
}

/// `DeltaCallback64`: `double(const Path64& path, const PathD& path_normals, size_t curr_idx,
/// size_t prev_idx)`. The `std::function` may throw: an `Err` (normally
/// [`ClipperError::Callback`]) is that exception. It leaves `group_delta_` unassigned and
/// propagates out of `Execute` at once, with the state the unwinding C++ leaves: the groups,
/// both callbacks, the normals and the paths already appended to the caller's `Paths64`.
pub type DeltaCallback64 = Box<dyn FnMut(&Path64, &PathD, usize, usize) -> Result<f64>>;

/// A `ZCallback64` for [`ClipperOffset`] that may throw: an `Err` is the exception, which ends
/// the finishing union and `Execute` at once (see [`ClipperOffset::set_fallible_z_callback`]).
pub type FallibleZCallback64 =
    Box<dyn FnMut(&Point64, &Point64, &Point64, &Point64, &mut Point64) -> Result<()>>;

/// `ClipperOffset::zCallback64_`, shared with the proxy `ExecuteInternal` binds
/// (`std::bind(&ZCB, this, ...)`), so the clipper reads the callback the offset holds at the
/// time of the call. The callback is only borrowed for its call, never moved out, so it stays
/// assigned however the call ends.
pub type SharedOffsetZCallback = Rc<RefCell<Option<FallibleZCallback64>>>;

/// `std::min(a, b)`: `(b < a) ? b : a`.
fn std_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// `path[index]` on a vector, refusing an index the C++ would read out of range.
fn at<T>(items: &[T], index: usize) -> Result<&T> {
    items.get(index).ok_or(INDEX)
}

/// `static_cast<int>(value)` for an integral `value` (the result of `std::ceil`).
fn cast_int(value: f64) -> Result<i32> {
    if value.is_nan() || value < -2_147_483_648.0 || value >= 2_147_483_648.0 {
        return Err(ClipperError::Undefined(
            "double to int conversion out of range",
        ));
    }
    Ok(value as i32)
}

/// `static_cast<size_t>(value)` for an integral `value` on the 64-bit reference.
fn cast_size_t(value: f64) -> Result<u64> {
    if value.is_nan() || value <= -1.0 || value >= 18_446_744_073_709_551_616.0 {
        return Err(ClipperError::Undefined(
            "double to size_t conversion out of range",
        ));
    }
    Ok(value as u64)
}

/// A fallible copy of every path.
fn copy_paths(paths: &Paths64) -> Result<Paths64> {
    let mut copy = crate::with_capacity(paths.len())?;
    for path in paths {
        copy.push(copy_of(path)?);
    }
    Ok(copy)
}

/// `vector::reserve(total)`: room for `total` elements in all.
fn reserve_total<T>(v: &mut Vec<T>, total: usize) -> Result<()> {
    reserve(v, total.saturating_sub(v.len()))
}

// --- Miscellaneous methods ---------------------------------------------------------------

/// `void GetLowestClosedPathInfo(const Paths64& paths, std::optional<size_t>& idx,
/// bool& is_neg_area)`. `is_neg_area` is only written when a lowest point is found.
pub fn get_lowest_closed_path_info(
    paths: &Paths64,
    idx: &mut Option<usize>,
    is_neg_area: &mut bool,
) -> Result<()> {
    *idx = None;
    let mut bot_pt = Point64::xy(i64::MAX, i64::MIN);
    for (i, path) in paths.iter().enumerate() {
        let mut a = MAX_DBL;
        for pt in path {
            if (pt.y < bot_pt.y) || ((pt.y == bot_pt.y) && (pt.x >= bot_pt.x)) {
                continue;
            }
            if a == MAX_DBL {
                a = area(path)?;
                if a == 0.0 {
                    break; // invalid closed path, so break from inner loop
                }
                *is_neg_area = a < 0.0;
            }
            *idx = Some(i);
            bot_pt.x = pt.x;
            bot_pt.y = pt.y;
        }
    }
    Ok(())
}

/// `inline double Hypot(double x, double y)`: `std::sqrt(x * x + y * y)`, "given that this is
/// an internal function, and given the x and y parameters will always be coordinate values (or
/// the difference between coordinate values)".
pub fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

/// `static PointD GetUnitNormal(const Point64& pt1, const Point64& pt2)`.
pub fn get_unit_normal(pt1: &Point64, pt2: &Point64) -> Result<PointD> {
    if pt1 == pt2 {
        return Ok(PointD::xy(0.0, 0.0));
    }
    let mut dx = pt2.x.checked_sub(pt1.x).ok_or(I64_OVERFLOW)? as f64;
    let mut dy = pt2.y.checked_sub(pt1.y).ok_or(I64_OVERFLOW)? as f64;
    let inverse_hypot = 1.0 / hypot(dx, dy);
    dx *= inverse_hypot;
    dy *= inverse_hypot;
    Ok(PointD::xy(dy, -dx))
}

/// `inline bool AlmostZero(double value, double epsilon = 0.001)`.
pub fn almost_zero(value: f64, epsilon: f64) -> bool {
    value.abs() < epsilon
}

/// `inline PointD NormalizeVector(const PointD& vec)`.
pub fn normalize_vector(vec: &PointD) -> PointD {
    let h = hypot(vec.x, vec.y);
    if almost_zero(h, 0.001) {
        return PointD::xy(0.0, 0.0);
    }
    let inverse_hypot = 1.0 / h;
    PointD::xy(vec.x * inverse_hypot, vec.y * inverse_hypot)
}

/// `inline PointD GetAvgUnitVector(const PointD& vec1, const PointD& vec2)`.
pub fn get_avg_unit_vector(vec1: &PointD, vec2: &PointD) -> PointD {
    normalize_vector(&PointD::xy(vec1.x + vec2.x, vec1.y + vec2.y))
}

/// `inline bool IsClosedPath(EndType et)`.
pub fn is_closed_path(et: EndType) -> bool {
    et == EndType::Polygon || et == EndType::Joined
}

/// `static inline Point64 GetPerpendic(const Point64& pt, const PointD& norm, double delta)`.
pub fn get_perpendic(pt: &Point64, norm: &PointD, delta: f64) -> Result<Point64> {
    Point64::new(
        pt.x as f64 + norm.x * delta,
        pt.y as f64 + norm.y * delta,
        pt.z,
    )
}

/// `inline PointD GetPerpendicD(const Point64& pt, const PointD& norm, double delta)`.
pub fn get_perpendic_d(pt: &Point64, norm: &PointD, delta: f64) -> Result<PointD> {
    PointD::new(
        pt.x as f64 + norm.x * delta,
        pt.y as f64 + norm.y * delta,
        pt.z,
    )
}

/// `inline void NegatePath(PathD& path)` (`z` is kept).
pub fn negate_path(path: &mut PathD) {
    for pt in path.iter_mut() {
        pt.x = -pt.x;
        pt.y = -pt.y;
    }
}

/// `template <typename T> Path<T> Ellipse(const Point<T>& center, double radiusX,
/// double radiusY, size_t steps)` (`clipper.h`).
fn ellipse(_center: &Point64, _radius_x: f64, _radius_y: f64, _steps: u64) -> Result<Path64> {
    // UNPORTED(thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002): Clipper2Lib::Ellipse<int64_t>
    Err(ClipperError::Unported {
        upstream: "Clipper2Lib::Ellipse<int64_t>",
        unit: INCLUDE_P002,
    })
}

// --- ClipperOffset::Group ----------------------------------------------------------------

/// `class ClipperOffset::Group` (members from `clipper.offset.h`).
#[derive(Clone, Debug)]
pub struct Group {
    pub paths_in: Paths64,
    pub lowest_path_idx: Option<usize>,
    pub is_reversed: bool,
    pub join_type: JoinType,
    pub end_type: EndType,
}

impl Group {
    /// `Group(const Paths64& _paths, JoinType _join_type, EndType _end_type)`.
    pub fn new(paths: &Paths64, join_type: JoinType, end_type: EndType) -> Result<Self> {
        let mut paths_in = copy_paths(paths)?;
        let is_joined = (end_type == EndType::Polygon) || (end_type == EndType::Joined);
        for p in paths_in.iter_mut() {
            strip_duplicates_path(p, is_joined);
        }

        let (lowest_path_idx, is_reversed) = if end_type == EndType::Polygon {
            let mut is_neg_area = false;
            let mut lowest_path_idx = None;
            get_lowest_closed_path_info(&paths_in, &mut lowest_path_idx, &mut is_neg_area)?;
            // the lowermost path must be an outer path, so if its orientation is negative,
            // then flag the whole group is 'reversed' (will negate delta etc.)
            // as this is much more efficient than reversing every path.
            (lowest_path_idx, lowest_path_idx.is_some() && is_neg_area)
        } else {
            (None, false)
        };
        Ok(Self {
            paths_in,
            lowest_path_idx,
            is_reversed,
            join_type,
            end_type,
        })
    }
}

// --- ClipperOffset ------------------------------------------------------------------------

/// `class ClipperOffset`.
pub struct ClipperOffset {
    error_code: i32,
    delta: f64,
    group_delta: f64,
    temp_lim: f64,
    steps_per_rad: f64,
    step_sin: f64,
    step_cos: f64,
    norms: PathD,
    path_out: Path64,
    /// `groups_`. An execution reads a shared handle instead of moving them out, so they stay
    /// in place however it ends (a callback error or a native unwind), as in the C++.
    groups: Rc<Vec<Group>>,
    join_type: JoinType,
    end_type: EndType,
    miter_limit: f64,
    arc_tolerance: f64,
    preserve_collinear: bool,
    reverse_solution: bool,
    /// `zCallback64_` ([`SharedOffsetZCallback`]).
    z_callback: SharedOffsetZCallback,
    delta_callback: Option<DeltaCallback64>,
    /// Host limit on the points one execution produces (offset paths and their copies).
    budget: Budget,
    points: usize,
}

impl ::core::fmt::Debug for ClipperOffset {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter
            .debug_struct("ClipperOffset")
            .field("error_code", &self.error_code)
            .field("groups", &self.groups.len())
            .field("miter_limit", &self.miter_limit)
            .field("arc_tolerance", &self.arc_tolerance)
            .field("delta_callback", &self.delta_callback.is_some())
            .finish()
    }
}

impl ClipperOffset {
    /// `explicit ClipperOffset(double miter_limit = 2.0, double arc_tolerance = 0.0,
    /// bool preserve_collinear = false, bool reverse_solution = false)`, within the host's
    /// `budget` (its `points` bound the points one execution may produce).
    pub fn new(
        miter_limit: f64,
        arc_tolerance: f64,
        preserve_collinear: bool,
        reverse_solution: bool,
        budget: Budget,
    ) -> Self {
        Self {
            error_code: 0,
            delta: 0.0,
            group_delta: 0.0,
            temp_lim: 0.0,
            steps_per_rad: 0.0,
            step_sin: 0.0,
            step_cos: 0.0,
            norms: PathD::new(),
            path_out: Path64::new(),
            groups: Rc::new(Vec::new()),
            join_type: JoinType::Bevel,
            end_type: EndType::Polygon,
            miter_limit,
            arc_tolerance,
            preserve_collinear,
            reverse_solution,
            z_callback: Rc::new(RefCell::new(None)),
            delta_callback: None,
            budget,
            points: 0,
        }
    }

    /// `ClipperOffset()` with the default arguments.
    pub fn with_defaults(budget: Budget) -> Self {
        Self::new(2.0, 0.0, false, false, budget)
    }

    /// `int ErrorCode() const`.
    pub fn error_code(&self) -> i32 {
        self.error_code
    }

    /// `void Clear()`.
    pub fn clear(&mut self) {
        match Rc::get_mut(&mut self.groups) {
            Some(groups) => groups.clear(),
            None => self.groups = Rc::new(Vec::new()),
        }
        self.norms.clear();
    }

    /// `double MiterLimit() const`.
    pub fn miter_limit(&self) -> f64 {
        self.miter_limit
    }

    /// `void MiterLimit(double miter_limit)`.
    pub fn set_miter_limit(&mut self, miter_limit: f64) {
        self.miter_limit = miter_limit;
    }

    /// `double ArcTolerance() const`: "needed for rounded offsets".
    pub fn arc_tolerance(&self) -> f64 {
        self.arc_tolerance
    }

    /// `void ArcTolerance(double arc_tolerance)`.
    pub fn set_arc_tolerance(&mut self, arc_tolerance: f64) {
        self.arc_tolerance = arc_tolerance;
    }

    /// `bool PreserveCollinear() const`.
    pub fn preserve_collinear(&self) -> bool {
        self.preserve_collinear
    }

    /// `void PreserveCollinear(bool preserve_collinear)`.
    pub fn set_preserve_collinear(&mut self, preserve_collinear: bool) {
        self.preserve_collinear = preserve_collinear;
    }

    /// `bool ReverseSolution() const`.
    pub fn reverse_solution(&self) -> bool {
        self.reverse_solution
    }

    /// `void ReverseSolution(bool reverse_solution)`.
    pub fn set_reverse_solution(&mut self, reverse_solution: bool) {
        self.reverse_solution = reverse_solution;
    }

    /// `void SetZCallback(ZCallback64 cb)` for a callback that never throws; `None` is an
    /// empty `std::function`.
    pub fn set_z_callback(&mut self, cb: Option<ZCallback64>) -> Result<()> {
        self.set_fallible_z_callback(cb.map(|mut cb| -> FallibleZCallback64 {
            Box::new(move |bot1, top1, bot2, top2, ip| {
                cb(bot1, top1, bot2, top2, ip);
                Ok(())
            })
        }))
    }

    /// `void SetZCallback(ZCallback64 cb)` for a callback that may throw; `None` is an empty
    /// `std::function`. A callback `Err` ends the finishing union and `Execute` with that
    /// error, as the exception unwinds the C++: the local clipper is discarded (the paths it
    /// already built into the caller's `Paths64` or `PolyTree64` remain), and the groups, both
    /// callbacks and the normals stay as they were.
    pub fn set_fallible_z_callback(&mut self, cb: Option<FallibleZCallback64>) -> Result<()> {
        *self.z_callback.try_borrow_mut().map_err(|_| REENTRANT)? = cb;
        Ok(())
    }

    /// `void SetDeltaCallback(DeltaCallback64 cb)`; `None` is an empty `std::function`.
    pub fn set_delta_callback(&mut self, cb: Option<DeltaCallback64>) {
        self.delta_callback = cb;
    }

    /// The groups added so far (`groups_`).
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// `groups_.emplace_back(...)`.
    fn push_group(&mut self, group: Group) -> Result<()> {
        push(Rc::get_mut(&mut self.groups).ok_or(GROUPS_IN_USE)?, group)
    }

    /// The normals of the last path offset (`norms`).
    pub fn norms(&self) -> &PathD {
        &self.norms
    }

    /// Admit `n` more produced points under the host budget.
    fn admit(&mut self, n: usize) -> Result<()> {
        self.points = self
            .points
            .checked_add(n)
            .filter(|&total| total <= self.budget.points)
            .ok_or(ClipperError::ResourceLimit)?;
        Ok(())
    }

    /// `path_out.emplace_back(pt)`.
    fn emit(&mut self, pt: Point64) -> Result<()> {
        self.admit(1)?;
        push(&mut self.path_out, pt)
    }

    /// `solution->emplace_back(path_out)` (a copy).
    fn push_solution(&mut self, solution: &mut Paths64) -> Result<()> {
        self.admit(self.path_out.len())?;
        let copy = copy_of(&self.path_out)?;
        push(solution, copy)
    }

    /// `void AddPath(const Path64& path, JoinType jt_, EndType et_)`.
    pub fn add_path(&mut self, path: &Path64, jt: JoinType, et: EndType) -> Result<()> {
        let mut paths = Paths64::new();
        push(&mut paths, copy_of(path)?)?;
        let group = Group::new(&paths, jt, et)?;
        self.push_group(group)
    }

    /// `void AddPaths(const Paths64 &paths, JoinType jt_, EndType et_)`.
    pub fn add_paths(&mut self, paths: &Paths64, jt: JoinType, et: EndType) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let group = Group::new(paths, jt, et)?;
        self.push_group(group)
    }

    /// `void BuildNormals(const Path64& path)`.
    pub fn build_normals(&mut self, path: &Path64) -> Result<()> {
        self.norms.clear();
        reserve(&mut self.norms, path.len())?;
        let Some(last) = path.len().checked_sub(1) else {
            return Ok(());
        };
        for i in 0..last {
            let normal = get_unit_normal(at(path, i)?, at(path, i + 1)?)?;
            self.norms.push(normal);
        }
        let normal = get_unit_normal(at(path, last)?, at(path, 0)?)?;
        self.norms.push(normal);
        Ok(())
    }

    /// `void DoBevel(const Path64& path, size_t j, size_t k)`.
    pub fn do_bevel(&mut self, path: &Path64, j: usize, k: usize) -> Result<()> {
        let pj = *at(path, j)?;
        let (x, y) = (pj.x as f64, pj.y as f64);
        let (pt1, pt2) = if j == k {
            let abs_delta = self.group_delta.abs();
            let nj = *at(&self.norms, j)?;
            (
                PointD::new(x - abs_delta * nj.x, y - abs_delta * nj.y, pj.z)?,
                PointD::new(x + abs_delta * nj.x, y + abs_delta * nj.y, pj.z)?,
            )
        } else {
            let (nk, nj) = (*at(&self.norms, k)?, *at(&self.norms, j)?);
            let gd = self.group_delta;
            (
                PointD::new(x + gd * nk.x, y + gd * nk.y, pj.z)?,
                PointD::new(x + gd * nj.x, y + gd * nj.y, pj.z)?,
            )
        };
        self.emit(Point64::from_point(&pt1)?)?;
        self.emit(Point64::from_point(&pt2)?)
    }

    /// `void DoSquare(const Path64& path, size_t j, size_t k)`.
    pub fn do_square(&mut self, path: &Path64, j: usize, k: usize) -> Result<()> {
        let nj = *at(&self.norms, j)?;
        let vec = if j == k {
            PointD::xy(nj.y, -nj.x)
        } else {
            let nk = *at(&self.norms, k)?;
            get_avg_unit_vector(&PointD::xy(-nk.y, nk.x), &PointD::xy(nj.y, -nj.x))
        };

        let gd = self.group_delta;
        let abs_delta = gd.abs();

        // now offset the original vertex delta units along unit vector
        let mut pt_q = PointD::from_point(at(path, j)?)?;
        pt_q = translate_point(&pt_q, abs_delta * vec.x, abs_delta * vec.y)?;
        // get perpendicular vertices
        let pt1 = translate_point(&pt_q, gd * vec.y, gd * -vec.x)?;
        let pt2 = translate_point(&pt_q, gd * -vec.y, gd * vec.x)?;
        // get 2 vertices along one edge offset
        let nk = *at(&self.norms, k)?;
        let pt3 = get_perpendic_d(at(path, k)?, &nk, gd)?;
        if j == k {
            let pt4 = PointD::xy(pt3.x + vec.x * gd, pt3.y + vec.y * gd);
            let mut pt = pt_q;
            get_line_intersect_pt(&pt1, &pt2, &pt3, &pt4, &mut pt)?;
            // get the second intersect point through reflecion
            self.emit(Point64::from_point(&reflect_point(&pt, &pt_q)?)?)?;
            self.emit(Point64::from_point(&pt)?)
        } else {
            let pt4 = get_perpendic_d(at(path, j)?, &nk, gd)?;
            let mut pt = pt_q;
            get_line_intersect_pt(&pt1, &pt2, &pt3, &pt4, &mut pt)?;
            self.emit(Point64::from_point(&pt)?)?;
            // get the second intersect point through reflecion
            self.emit(Point64::from_point(&reflect_point(&pt, &pt_q)?)?)
        }
    }

    /// `void DoMiter(const Path64& path, size_t j, size_t k, double cos_a)`.
    pub fn do_miter(&mut self, path: &Path64, j: usize, k: usize, cos_a: f64) -> Result<()> {
        let q = self.group_delta / (cos_a + 1.0);
        let pj = *at(path, j)?;
        let (nk, nj) = (*at(&self.norms, k)?, *at(&self.norms, j)?);
        let pt = Point64::new(
            pj.x as f64 + (nk.x + nj.x) * q,
            pj.y as f64 + (nk.y + nj.y) * q,
            pj.z,
        )?;
        self.emit(pt)
    }

    /// The arc step members for `abs_delta` (shared by `DoRound` and `DoGroupOffset`):
    /// "calculate the number of steps required to approximate a circle".
    fn set_arc_steps(&mut self, abs_delta: f64) {
        let arc_tol = if self.arc_tolerance > FLOATING_POINT_TOLERANCE {
            std_min(abs_delta, self.arc_tolerance)
        } else {
            abs_delta * ARC_CONST
        };
        let steps_per_360 = std_min(PI / acos(1.0 - arc_tol / abs_delta), abs_delta * PI);
        self.step_sin = sin(2.0 * PI / steps_per_360);
        self.step_cos = cos(2.0 * PI / steps_per_360);
        if self.group_delta < 0.0 {
            self.step_sin = -self.step_sin;
        }
        self.steps_per_rad = steps_per_360 / (2.0 * PI);
    }

    /// `void DoRound(const Path64& path, size_t j, size_t k, double angle)`.
    pub fn do_round(&mut self, path: &Path64, j: usize, k: usize, angle: f64) -> Result<()> {
        if self.delta_callback.is_some() {
            // when deltaCallback64_ is assigned, group_delta_ won't be constant,
            // so we'll need to do the following calculations for *every* vertex.
            let abs_delta = self.group_delta.abs();
            self.set_arc_steps(abs_delta);
        }

        let pt = *at(path, j)?;
        let nk = *at(&self.norms, k)?;
        let mut offset_vec = PointD::xy(nk.x * self.group_delta, nk.y * self.group_delta);

        if j == k {
            offset_vec.negate()?;
        }
        self.emit(Point64::new(
            pt.x as f64 + offset_vec.x,
            pt.y as f64 + offset_vec.y,
            pt.z,
        )?)?;
        let steps = cast_int((self.steps_per_rad * angle.abs()).ceil())?; // #448, #456
        for _ in 1..steps {
            // ie 1 less than steps
            offset_vec = PointD::xy(
                offset_vec.x * self.step_cos - self.step_sin * offset_vec.y,
                offset_vec.x * self.step_sin + offset_vec.y * self.step_cos,
            );
            self.emit(Point64::new(
                pt.x as f64 + offset_vec.x,
                pt.y as f64 + offset_vec.y,
                pt.z,
            )?)?;
        }
        let nj = *at(&self.norms, j)?;
        let last = get_perpendic(&pt, &nj, self.group_delta)?;
        self.emit(last)
    }

    /// `void OffsetPoint(Group& group, const Path64& path, size_t j, size_t k)`.
    ///
    /// "Let A = change in angle where edges join. A == 0: ie no change in angle (flat join);
    /// A == PI: edges 'spike'; sin(A) < 0: right turning; cos(A) < 0: change in angle is more
    /// than 90 degree."
    // The reference's clamp is an if/else chain (NaN passes through either way).
    #[allow(clippy::manual_clamp)]
    pub fn offset_point(&mut self, group: &Group, path: &Path64, j: usize, k: usize) -> Result<()> {
        let (pj, pk) = (*at(path, j)?, *at(path, k)?);
        if pj == pk {
            return Ok(());
        }

        let (nj, nk) = (*at(&self.norms, j)?, *at(&self.norms, k)?);
        let mut sin_a = cross_product2(&nj, &nk)?;
        let cos_a = dot_product2(&nj, &nk)?;
        if sin_a > 1.0 {
            sin_a = 1.0;
        } else if sin_a < -1.0 {
            sin_a = -1.0;
        }

        if let Some(cb) = self.delta_callback.as_mut() {
            self.group_delta = cb(path, &self.norms, j, k)?;
            if group.is_reversed {
                self.group_delta = -self.group_delta;
            }
        }
        if self.group_delta.abs() <= FLOATING_POINT_TOLERANCE {
            return self.emit(pj);
        }

        if cos_a > -0.999 && (sin_a * self.group_delta < 0.0) {
            // test for concavity first (#593)
            // is concave
            // by far the simplest way to construct concave joins, especially those joining
            // very short segments, is to insert 3 points that produce negative regions.
            // These regions will be removed later by the finishing union operation. This is
            // also the best way to ensure that path reversals (ie over-shrunk paths) are
            // removed.
            let first =
                Point64::from_point_with_z(&get_perpendic(&pj, &nk, self.group_delta)?, pj.z)?;
            self.emit(first)?;
            self.emit(pj)?; // (#405, #873, #916)
            let last =
                Point64::from_point_with_z(&get_perpendic(&pj, &nj, self.group_delta)?, pj.z)?;
            self.emit(last)
        } else if cos_a > 0.999 && self.join_type != JoinType::Round {
            // almost straight - less than 2.5 degree (#424, #482, #526 & #724)
            self.do_miter(path, j, k, cos_a)
        } else if self.join_type == JoinType::Miter {
            // miter unless the angle is sufficiently acute to exceed ML
            if cos_a > self.temp_lim - 1.0 {
                self.do_miter(path, j, k, cos_a)
            } else {
                self.do_square(path, j, k)
            }
        } else if self.join_type == JoinType::Round {
            self.do_round(path, j, k, atan2(sin_a, cos_a))
        } else if self.join_type == JoinType::Bevel {
            self.do_bevel(path, j, k)
        } else {
            self.do_square(path, j, k)
        }
    }

    /// `void OffsetPolygon(Group& group, const Path64& path)`.
    pub fn offset_polygon(
        &mut self,
        group: &Group,
        path: &Path64,
        solution: &mut Paths64,
    ) -> Result<()> {
        self.path_out.clear();
        // for (j = 0, k = path.size() - 1; j < path.size(); k = j, ++j)
        let mut k = path.len().wrapping_sub(1);
        for j in 0..path.len() {
            self.offset_point(group, path, j, k)?;
            k = j;
        }
        self.push_solution(solution)
    }

    /// `void OffsetOpenJoined(Group& group, const Path64& path)`.
    pub fn offset_open_joined(
        &mut self,
        group: &Group,
        path: &Path64,
        solution: &mut Paths64,
    ) -> Result<()> {
        self.offset_polygon(group, path, solution)?;
        let mut reverse_path = copy_of(path)?;
        reverse_path.reverse();

        // rebuild normals
        self.norms.reverse();
        let first = *at(&self.norms, 0)?;
        push(&mut self.norms, first)?;
        self.norms.remove(0);
        negate_path(&mut self.norms);

        self.offset_polygon(group, &reverse_path, solution)
    }

    /// The start or end cap of an open path at index `i`.
    fn offset_cap(&mut self, path: &Path64, i: usize) -> Result<()> {
        if self.group_delta.abs() <= FLOATING_POINT_TOLERANCE {
            return self.emit(*at(path, i)?);
        }
        match self.end_type {
            EndType::Butt => self.do_bevel(path, i, i),
            EndType::Round => self.do_round(path, i, i, PI),
            _ => self.do_square(path, i, i),
        }
    }

    /// `void OffsetOpenPath(Group& group, const Path64& path)`.
    pub fn offset_open_path(
        &mut self,
        group: &Group,
        path: &Path64,
        solution: &mut Paths64,
    ) -> Result<()> {
        // do the line start cap
        if let Some(cb) = self.delta_callback.as_mut() {
            self.group_delta = cb(path, &self.norms, 0, 0)?;
        }
        self.offset_cap(path, 0)?;

        let high_i = path.len().checked_sub(1).ok_or(INDEX)?;
        // offset the left side going forward
        let mut k = 0usize;
        for j in 1..high_i {
            self.offset_point(group, path, j, k)?;
            k = j;
        }

        // reverse normals
        for i in (1..=high_i).rev() {
            let n = *at(&self.norms, i - 1)?;
            *self.norms.get_mut(i).ok_or(INDEX)? = PointD::xy(-n.x, -n.y);
        }
        let last = *at(&self.norms, high_i)?;
        *self.norms.get_mut(0).ok_or(INDEX)? = last;

        // do the line end cap
        if let Some(cb) = self.delta_callback.as_mut() {
            self.group_delta = cb(path, &self.norms, high_i, high_i)?;
        }
        self.offset_cap(path, high_i)?;

        // for (j = highI - 1, k = highI; j > 0; k = j, --j)
        let mut k = high_i;
        let mut j = high_i.checked_sub(1).ok_or(INDEX)?;
        while j > 0 {
            self.offset_point(group, path, j, k)?;
            k = j;
            j -= 1;
        }
        self.push_solution(solution)
    }

    /// `void DoGroupOffset(Group& group)`.
    pub fn do_group_offset(&mut self, group: &Group, solution: &mut Paths64) -> Result<()> {
        if group.end_type == EndType::Polygon {
            // a straight path (2 points) can now also be 'polygon' offset
            // where the ends will be treated as (180 deg.) joins
            if group.lowest_path_idx.is_none() {
                self.delta = self.delta.abs();
            }
            self.group_delta = if group.is_reversed {
                -self.delta
            } else {
                self.delta
            };
        } else {
            self.group_delta = self.delta.abs(); // *0.5;
        }

        let mut abs_delta = self.group_delta.abs();
        self.join_type = group.join_type;
        self.end_type = group.end_type;

        if group.join_type == JoinType::Round || group.end_type == EndType::Round {
            // arcTol - when arc_tolerance_ is undefined (0) then curve imprecision
            // will be relative to the size of the offset (delta). Obviously very
            // large offsets will almost always require much less precision.
            self.set_arc_steps(abs_delta);
        }

        for path in &group.paths_in {
            let path_len = path.len();
            self.path_out.clear();

            if path_len == 1 {
                // single point
                if let Some(cb) = self.delta_callback.as_mut() {
                    self.group_delta = cb(path, &self.norms, 0, 0)?;
                    if group.is_reversed {
                        self.group_delta = -self.group_delta;
                    }
                    abs_delta = self.group_delta.abs();
                }

                if self.group_delta < 1.0 {
                    continue;
                }
                let pt = *at(path, 0)?;
                // single vertex so build a circle or square ...
                if group.join_type == JoinType::Round {
                    let radius = abs_delta;
                    let steps = if self.steps_per_rad > 0.0 {
                        cast_size_t((self.steps_per_rad * 2.0 * PI).ceil())?
                    } else {
                        0
                    }; // #617
                    self.path_out = ellipse(&pt, radius, radius, steps)?;
                    for p in self.path_out.iter_mut() {
                        p.z = pt.z;
                    }
                } else {
                    let d = i64::from(cast_int(abs_delta.ceil())?);
                    let r = Rect64::new(
                        pt.x.checked_sub(d).ok_or(I64_OVERFLOW)?,
                        pt.y.checked_sub(d).ok_or(I64_OVERFLOW)?,
                        pt.x.checked_add(d).ok_or(I64_OVERFLOW)?,
                        pt.y.checked_add(d).ok_or(I64_OVERFLOW)?,
                    );
                    let mut square = r.as_path();
                    self.admit(square.len())?;
                    for p in square.iter_mut() {
                        p.z = pt.z;
                    }
                    self.path_out = square;
                }

                self.push_solution(solution)?;
                continue;
            } // end of offsetting a single point

            if (path_len == 2) && (group.end_type == EndType::Joined) {
                self.end_type = if group.join_type == JoinType::Round {
                    EndType::Round
                } else {
                    EndType::Square
                };
            }

            self.build_normals(path)?;
            if self.end_type == EndType::Polygon {
                self.offset_polygon(group, path, solution)?;
            } else if self.end_type == EndType::Joined {
                self.offset_open_joined(group, path, solution)?;
            } else {
                self.offset_open_path(group, path, solution)?;
            }
        }
        Ok(())
    }

    /// `void ZCB(const Point64& bot1, const Point64& top1, const Point64& bot2,
    /// const Point64& top2, Point64& ip)`; an error is the user callback's exception.
    pub fn zcb(
        &mut self,
        bot1: &Point64,
        top1: &Point64,
        bot2: &Point64,
        top2: &Point64,
        ip: &mut Point64,
    ) -> Result<()> {
        offset_zcb(&self.z_callback, bot1, top1, bot2, top2, ip)
    }

    /// `size_t CalcSolutionCapacity()`.
    pub fn calc_solution_capacity(&self) -> usize {
        let mut result = 0usize;
        for g in self.groups.iter() {
            result = result.wrapping_add(if g.end_type == EndType::Joined {
                g.paths_in.len().wrapping_mul(2)
            } else {
                g.paths_in.len()
            });
        }
        result
    }

    /// `bool CheckReverseOrientation()`: "nb: this assumes there's consistency in
    /// orientation between groups".
    pub fn check_reverse_orientation(&self) -> bool {
        let mut is_reversed_orientation = false;
        for g in self.groups.iter() {
            if g.end_type == EndType::Polygon {
                is_reversed_orientation = g.is_reversed;
                break;
            }
        }
        is_reversed_orientation
    }

    /// The group loop of `ExecuteInternal`, on a shared handle to `groups_`.
    fn offset_groups(
        &mut self,
        groups: &[Group],
        delta: f64,
        solution: &mut Paths64,
    ) -> Result<()> {
        if delta.abs() < 0.5 {
            // ie: offset is insignificant
            let mut sol_size = 0usize;
            for group in groups {
                sol_size = sol_size.wrapping_add(group.paths_in.len());
            }
            reserve_total(solution, sol_size)?;
            for group in groups {
                for path in &group.paths_in {
                    self.admit(path.len())?;
                    push(solution, copy_of(path)?)?;
                }
            }
        } else {
            self.temp_lim = if self.miter_limit <= 1.0 {
                2.0
            } else {
                2.0 / (self.miter_limit * self.miter_limit)
            };

            self.delta = delta;
            for group in groups {
                self.do_group_offset(group, solution)?;
                if self.error_code == 0 {
                    continue; // all OK
                }
                solution.clear();
            }
        }
        Ok(())
    }

    /// `void ExecuteInternal(double delta)` with `solution` and `solution_tree`.
    fn execute_internal(
        &mut self,
        delta: f64,
        solution: &mut Paths64,
        solution_tree: Option<&mut PolyTree64>,
    ) -> Result<()> {
        self.error_code = 0;
        self.points = 0;
        if self.groups.is_empty() {
            return Ok(());
        }
        reserve_total(solution, self.calc_solution_capacity())?;

        // The handle keeps `groups_` in place: a callback error returns here with them intact.
        let groups = Rc::clone(&self.groups);
        let offset = self.offset_groups(&groups, delta, solution);
        drop(groups);
        offset?;

        if solution.is_empty() {
            return Ok(());
        }

        let paths_reversed = self.check_reverse_orientation();
        // clean up self-intersections ...
        let mut c = Clipper64::default();
        c.base.set_preserve_collinear(self.preserve_collinear);
        // the solution should retain the orientation of the input
        c.base
            .set_reverse_solution(self.reverse_solution != paths_reversed);
        c.base.set_z_callback_slot(ZCallbackSlot::OffsetProxy {
            callback: Rc::clone(&self.z_callback),
        });
        c.add_subject(solution)?;
        let fill_rule = if paths_reversed {
            FillRule::Negative
        } else {
            FillRule::Positive
        };
        match solution_tree {
            Some(tree) => {
                c.execute_tree(ClipType::Union, fill_rule, tree)?;
            }
            None => {
                c.execute(ClipType::Union, fill_rule, solution)?;
            }
        }
        Ok(())
    }

    /// `void Execute(double delta, Paths64& sols_64)`.
    pub fn execute(&mut self, delta: f64, paths64: &mut Paths64) -> Result<()> {
        paths64.clear();
        self.execute_internal(delta, paths64, None)
    }

    /// `void Execute(double delta, PolyTree64& polytree)`.
    pub fn execute_tree(&mut self, delta: f64, polytree: &mut PolyTree64) -> Result<()> {
        polytree.clear(PolyTree64::ROOT)?;
        let mut solution = Paths64::new();
        self.execute_internal(delta, &mut solution, Some(polytree))
    }

    /// `void Execute(DeltaCallback64 delta_cb, Paths64& paths)`: the callback stays assigned
    /// for later executions.
    pub fn execute_with_delta_callback(
        &mut self,
        delta_cb: Option<DeltaCallback64>,
        paths: &mut Paths64,
    ) -> Result<()> {
        self.delta_callback = delta_cb;
        self.execute(1.0, paths)
    }
}

/// `ClipperOffset::ZCB` on the object's `zCallback64_`. The callback is borrowed for the call,
/// so it stays assigned when it returns an error or unwinds; an error propagates unchanged.
pub(crate) fn offset_zcb(
    z_callback: &RefCell<Option<FallibleZCallback64>>,
    bot1: &Point64,
    top1: &Point64,
    bot2: &Point64,
    top2: &Point64,
    ip: &mut Point64,
) -> Result<()> {
    if bot1.z != 0 && ((bot1.z == bot2.z) || (bot1.z == top2.z)) {
        ip.z = bot1.z;
    } else if bot2.z != 0 && (bot2.z == top1.z) {
        ip.z = bot2.z;
    } else if top1.z != 0 && (top1.z == top2.z) {
        ip.z = top1.z;
    } else {
        let mut slot = z_callback.try_borrow_mut().map_err(|_| REENTRANT)?;
        if let Some(cb) = slot.as_mut() {
            cb(bot1, top1, bot2, top2, ip)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
