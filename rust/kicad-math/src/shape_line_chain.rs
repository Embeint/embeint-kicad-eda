// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_line_chain.h.
// Copyright (C) 2013 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `CLIPPER_Z_VALUE` and the inline members of `SHAPE_LINE_CHAIN`
//! (libs/kimath/include/geometry/shape_line_chain.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p004`.
//!
//! [`ShapeLineChain`] keeps the C++ members: the vertices, the per-vertex shape indices, the arcs,
//! the arc accuracy, the closed flag, the width and the cached bounding box. Every inline body of
//! the header is a method here, with the C++ evaluation order; out-of-line members it calls
//! (shape_line_chain.cpp, `libs.kimath.src.geometry.shape_line_chain.application.ui.p001` and
//! `p002`; shape_arc.cpp, `libs.kimath.src.geometry.shape_arc.application.ui.p001`) return
//! [`ShapeError::Unported`] naming their owner, after any mutation the C++ has made before the call.
//!
//! Undefined C++ evaluations (signed overflow, an index outside a vector, a floating-to-integer
//! conversion out of range) are refused with [`ShapeError::Undefined`] at the first such operation.
//!
//! `m_bbox` is `mutable` in C++ and `GenerateBBoxCache` is `const`; here it takes `&mut self`,
//! and the non-`const` pointer `GetCachedBBox` returns is [`ShapeLineChain::cached_bbox_mut`].
//!
//! `size_t` and `ssize_t` are 64-bit on the reference (Linux x86-64): they are `u64` and `i64`.
use core::any::Any;

use crate::box2::Box2I;
use crate::eda_angle::EdaAngle;
use crate::seg::Seg;
use crate::shape::{unported, Shape, ShapeError, ShapeLineChainBase, ShapeResult, ShapeType};
use crate::shape_arc::{ShapeArc, SHAPE_ARC_UNIT};
use crate::vector::{Vector2, Vector2I};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.ui.p004";
/// Owner of shape_line_chain.cpp lines 1 to 2027 (`Segment`, `Remove( int, int )`,
/// `mergeFirstLastPointIfNeeded`, `amendArc`, `Collide`, `Rotate`,
/// `SHAPE_LINE_CHAIN_BASE::SquaredDistance`, the `SHAPE_IS_PT` and `SHAPES_ARE_PT` definitions).
pub const SHAPE_LINE_CHAIN_P001_UNIT: &str =
    "libs.kimath.src.geometry.shape_line_chain.application.ui.p001";
/// Owner of shape_line_chain.cpp lines 2028 to 3308 (`Clone`, `IsSharedPt`).
pub const SHAPE_LINE_CHAIN_P002_UNIT: &str =
    "libs.kimath.src.geometry.shape_line_chain.application.ui.p002";

/// `SHAPE_LINE_CHAIN::SHAPE_IS_PT` (shape_line_chain.cpp:42): the shape index of a plain point.
/// A minimum co-port of the constant `libs.kimath.src.geometry.shape_line_chain.application.ui.p001`
/// defines; `Append` stores it.
pub const SHAPE_IS_PT: i64 = -1;
/// `SHAPE_LINE_CHAIN::SHAPES_ARE_PT` (shape_line_chain.cpp:43): `{ SHAPE_IS_PT, SHAPE_IS_PT }`.
pub const SHAPES_ARE_PT: (i64, i64) = (SHAPE_IS_PT, SHAPE_IS_PT);

/// `PTRDIFF_MAX / sizeof( VECTOR2I )`: `std::vector<VECTOR2I>::max_size()` in libstdc++.
pub const POINTS_MAX_SIZE: u64 = (i64::MAX as u64) / 8;
/// `PTRDIFF_MAX / sizeof( std::pair<ssize_t, ssize_t> )`: `max_size()` of `m_shapes`.
pub const SHAPES_MAX_SIZE: u64 = (i64::MAX as u64) / 16;

/// `CLIPPER_Z_VALUE`: the arc indices of a point after a Clipper2 operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipperZValue {
    /// `m_FirstArcIdx`.
    pub first_arc_idx: i64,
    /// `m_SecondArcIdx`.
    pub second_arc_idx: i64,
}

impl Default for ClipperZValue {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipperZValue {
    /// `CLIPPER_Z_VALUE()`: both indices -1.
    pub const fn new() -> Self {
        Self {
            first_arc_idx: -1,
            second_arc_idx: -1,
        }
    }

    /// `CLIPPER_Z_VALUE( const std::pair<ssize_t, ssize_t> aShapeIndices, ssize_t aOffset = 0 )`:
    /// each non-negative index is offset, the first then the second. A `ssize_t` overflow of
    /// `aVal += aOffset` is undefined.
    pub fn from_indices(shape_indices: (i64, i64), offset: i64) -> ShapeResult<Self> {
        let offset_val = |value: i64| -> ShapeResult<i64> {
            if value >= 0 {
                value.checked_add(offset).ok_or(ShapeError::Undefined)
            } else {
                Ok(value)
            }
        };
        let first_arc_idx = offset_val(shape_indices.0)?;
        let second_arc_idx = offset_val(shape_indices.1)?;
        Ok(Self {
            first_arc_idx,
            second_arc_idx,
        })
    }
}

/// `SHAPE_LINE_CHAIN::INTERSECTION`: an intersection between two line segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intersection {
    /// Point of intersection between our and their.
    pub p: Vector2I,
    /// Index of the intersecting corner/segment in the 'our' (== this) line.
    pub index_our: i32,
    /// Index of the intersecting corner/segment in the 'their' line.
    pub index_their: i32,
    /// When true, the corner `[index_our]` of the 'our' line lies exactly on 'their' line.
    pub is_corner_our: bool,
    /// When true, the corner `[index_their]` of the 'their' line lies exactly on 'our' line.
    pub is_corner_their: bool,
    /// Auxiliary flag used by the refining code to reject false intersection points.
    pub valid: bool,
}

impl Default for Intersection {
    /// `INTERSECTION()`: `p` value-initialised (`VECTOR2I()` is (0, 0)), both indices -1, every
    /// flag false.
    fn default() -> Self {
        Self {
            p: Vector2::default(),
            index_our: -1,
            index_their: -1,
            is_corner_our: false,
            is_corner_their: false,
            valid: false,
        }
    }
}

/// `SHAPE_LINE_CHAIN::compareOriginDistance`: orders intersections by their distance from an
/// origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompareOriginDistance {
    /// `m_origin`.
    pub origin: Vector2I,
}

impl CompareOriginDistance {
    /// `compareOriginDistance( const VECTOR2I& aOrigin )`.
    pub const fn new(origin: Vector2I) -> Self {
        Self { origin }
    }

    /// `operator()( const INTERSECTION& aA, const INTERSECTION& aB )`:
    /// `( m_origin - aA.p ).EuclideanNorm() < ( m_origin - aB.p ).EuclideanNorm()`. Each
    /// `VECTOR2I` difference is an `int` subtraction per component (undefined on overflow); the
    /// left operand is evaluated first.
    pub fn compare(&self, a: &Intersection, b: &Intersection) -> ShapeResult<bool> {
        let left = self.origin.checked_sub(a.p)?.euclidean_norm()?;
        let right = self.origin.checked_sub(b.p)?.euclidean_norm()?;
        Ok(left < right)
    }
}

/// Why `ReservePoints` threw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReserveError {
    /// `std::length_error( "vector::reserve" )`: the size exceeds a vector's `max_size()`.
    LengthError,
    /// The allocation failed (`std::bad_alloc` in C++, whose occurrence depends on the host).
    BadAlloc,
}

/// `SHAPE_LINE_CHAIN`: a chain of connected line and/or arc segments.
#[derive(Debug)]
pub struct ShapeLineChain {
    /// `m_points`: the vertices.
    points: Vec<Vector2I>,
    /// `m_shapes`: per vertex, the indices of up to two arcs the point belongs to.
    shapes: Vec<(i64, i64)>,
    /// `m_arcs`.
    arcs: Vec<ShapeArc>,
    /// `m_accuracy`: the maxError to use when converting arcs to points. No inline body reads it.
    accuracy: i32,
    /// `m_closed`.
    closed: bool,
    /// `m_width`.
    width: i32,
    /// `m_bbox`: the cached bounding box.
    bbox: Box2I,
}

impl Default for ShapeLineChain {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapeLineChain {
    /// `SHAPE_LINE_CHAIN()`: `SHAPE_LINE_CHAIN_BASE( SH_LINE_CHAIN )`, `m_accuracy( 0 )`,
    /// `m_closed( false )`, `m_width( 0 )`; the vectors are empty and `m_bbox` is a default
    /// `BOX2I`.
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            shapes: Vec::new(),
            arcs: Vec::new(),
            accuracy: 0,
            closed: false,
            width: 0,
            bbox: Box2I::default(),
        }
    }

    /// The `SHAPE_ARC` copy constructor (shape_arc.cpp), for each copied arc.
    fn copy_arc(_arc: &ShapeArc) -> ShapeResult<ShapeArc> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)
        unported("SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)", SHAPE_ARC_UNIT)
    }

    /// `SHAPE_LINE_CHAIN( const SHAPE_LINE_CHAIN& aShape )`: the members in declaration order;
    /// each arc is copied by the out-of-line `SHAPE_ARC` copy constructor.
    pub fn copy_of(shape: &ShapeLineChain) -> ShapeResult<Self> {
        let points = shape.points.clone();
        let shapes = shape.shapes.clone();
        let mut arcs = Vec::with_capacity(shape.arcs.len());
        for arc in &shape.arcs {
            arcs.push(Self::copy_arc(arc)?);
        }
        Ok(Self {
            points,
            shapes,
            arcs,
            accuracy: shape.accuracy,
            closed: shape.closed,
            width: shape.width,
            bbox: shape.bbox,
        })
    }

    /// `operator=( const SHAPE_LINE_CHAIN& ) = default`: memberwise in declaration order (the
    /// base's `m_type` is `SH_LINE_CHAIN` on both sides). Assigning `m_arcs` assigns the existing
    /// elements with `SHAPE_ARC`'s implicit copy assignment and copy-constructs any further ones
    /// (libstdc++ `vector::operator=`), so it defers at the `SHAPE_ARC` copy constructor exactly
    /// when the source holds more arcs than this chain. The points and shapes are assigned by then.
    pub fn assign_from(&mut self, other: &ShapeLineChain) -> ShapeResult<()> {
        self.points.clone_from(&other.points);
        self.shapes.clone_from(&other.shapes);

        if other.arcs.len() > self.arcs.len() {
            let first = other
                .arcs
                .get(self.arcs.len())
                .ok_or(ShapeError::Undefined)?;
            Self::copy_arc(first)?;
        }
        self.arcs.truncate(other.arcs.len());
        for (arc, source) in self.arcs.iter_mut().zip(other.arcs.iter()) {
            arc.assign_from(source);
        }

        self.accuracy = other.accuracy;
        self.closed = other.closed;
        self.width = other.width;
        self.bbox = other.bbox;
        Ok(())
    }

    /// `operator=( SHAPE_LINE_CHAIN&& aOther ) noexcept`: the vectors are moved (`aOther`'s are
    /// left empty, as libstdc++'s move assignment leaves them), the other members copied. The
    /// C++ self-assignment check cannot be reached: Rust's borrows exclude `this == &aOther`.
    pub fn move_assign_from(&mut self, other: &mut ShapeLineChain) {
        self.points = core::mem::take(&mut other.points);
        self.shapes = core::mem::take(&mut other.shapes);
        self.arcs = core::mem::take(&mut other.arcs);

        self.accuracy = other.accuracy;
        self.closed = other.closed;
        self.width = other.width;
        self.bbox = other.bbox;
    }

    /// `Clear`: remove all points (and arcs and shapes) and open the chain. The width and the
    /// cached box are kept.
    pub fn clear(&mut self) {
        self.points.clear();
        self.arcs.clear();
        self.shapes.clear();
        self.closed = false;
    }

    /// `SetClosed( bool aClosed )`: `m_closed = aClosed`, then the out-of-line
    /// `mergeFirstLastPointIfNeeded()`. The flag is set when the call defers.
    pub fn set_closed(&mut self, closed: bool) -> ShapeResult<()> {
        self.closed = closed;
        self.merge_first_last_point_if_needed()
    }

    /// `mergeFirstLastPointIfNeeded` (shape_line_chain.cpp).
    fn merge_first_last_point_if_needed(&mut self) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::mergeFirstLastPointIfNeeded
        unported(
            "SHAPE_LINE_CHAIN::mergeFirstLastPointIfNeeded",
            SHAPE_LINE_CHAIN_P001_UNIT,
        )
    }

    /// `Width`.
    pub const fn width(&self) -> i32 {
        self.width
    }

    /// `SegmentCount`: `int c = m_points.size() - 1` (the `size_t` difference converted to
    /// `int`, so -1 for an empty chain), plus one when closed, at least 0.
    pub fn segment_count(&self) -> i32 {
        let mut c = (self.points.len() as u64).wrapping_sub(1) as i32;

        if self.closed {
            // A wrapping c++ would need more than INT_MAX points.
            c = c.wrapping_add(1);
        }

        core::cmp::max(0, c)
    }

    /// `PointCount`: `m_points.size()` converted to `int`.
    pub fn point_count(&self) -> i32 {
        self.points.len() as i32
    }

    /// `Segment( int aIndex )` (shape_line_chain.cpp).
    pub fn segment(&self, _index: i32) -> ShapeResult<Seg> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::Segment
        unported("SHAPE_LINE_CHAIN::Segment", SHAPE_LINE_CHAIN_P001_UNIT)
    }

    /// `CSegment( int aIndex )`: `Segment( aIndex )`.
    pub fn csegment(&self, index: i32) -> ShapeResult<Seg> {
        self.segment(index)
    }

    /// `CPoint( int aIndex )`: a negative index counts from the back and an index at or past
    /// the end wraps once; `m_points[aIndex]` outside the vector is undefined.
    pub fn cpoint(&self, index: i32) -> ShapeResult<Vector2I> {
        let count = self.point_count();
        // Neither adjustment can overflow: count >= 0.
        let index = if index < 0 {
            index.wrapping_add(count)
        } else if index >= count {
            index.wrapping_sub(count)
        } else {
            index
        };
        let i = usize::try_from(index).map_err(|_| ShapeError::Undefined)?;
        self.points.get(i).copied().ok_or(ShapeError::Undefined)
    }

    /// `CPoints`.
    pub fn cpoints(&self) -> &[Vector2I] {
        &self.points
    }

    /// `CLastPoint`: `m_points[static_cast<size_t>( PointCount() ) - 1]`, undefined when empty.
    pub fn clast_point(&self) -> ShapeResult<Vector2I> {
        let index = (self.point_count() as i64 as u64).wrapping_sub(1);
        let i = usize::try_from(index).map_err(|_| ShapeError::Undefined)?;
        self.points.get(i).copied().ok_or(ShapeError::Undefined)
    }

    /// `CArcs`.
    pub fn carcs(&self) -> &[ShapeArc] {
        &self.arcs
    }

    /// `CShapes`.
    pub fn cshapes(&self) -> &[(i64, i64)] {
        &self.shapes
    }

    /// `GenerateBBoxCache`: `m_bbox.Compute( m_points )` (no change for an empty chain), then
    /// `m_bbox.Inflate( m_width )` when the width is not 0.
    pub fn generate_bbox_cache(&mut self) -> ShapeResult<()> {
        self.bbox.compute(&self.points)?;

        if self.width != 0 {
            self.bbox.inflate_uniform(self.width)?;
        }
        Ok(())
    }

    /// The `BOX2I*` `GetCachedBBox` returns, for writing through it.
    pub fn cached_bbox_mut(&mut self) -> &mut Box2I {
        &mut self.bbox
    }

    /// `ReservePoints( size_t aSize )`: `m_points.reserve( aSize )`, then
    /// `m_shapes.reserve( aSize )`. A size above a vector's `max_size()` throws
    /// `std::length_error`; the capacity is not otherwise observable.
    pub fn reserve_points(&mut self, size: u64) -> Result<(), ReserveError> {
        fn reserve<T>(v: &mut Vec<T>, size: u64, max_size: u64) -> Result<(), ReserveError> {
            if size > max_size {
                return Err(ReserveError::LengthError);
            }
            let size = usize::try_from(size).map_err(|_| ReserveError::BadAlloc)?;
            v.try_reserve(size.saturating_sub(v.len()))
                .map_err(|_| ReserveError::BadAlloc)
        }
        reserve(&mut self.points, size, POINTS_MAX_SIZE)?;
        reserve(&mut self.shapes, size, SHAPES_MAX_SIZE)
    }

    /// `Append( int aX, int aY, bool aAllowDuplication = false )`.
    pub fn append_xy(&mut self, x: i32, y: i32, allow_duplication: bool) -> ShapeResult<()> {
        let v = Vector2::new(x, y);
        self.append(v, allow_duplication)
    }

    /// `Append( const VECTOR2I& aP, bool aAllowDuplication = false )`: the first point resets
    /// the cached box to `BOX2I( aP, VECTOR2I( 0, 0 ) )`; a point is appended (as a plain point,
    /// merged into the cached box) unless it repeats the last one and duplication is not allowed.
    pub fn append(&mut self, p: Vector2I, allow_duplication: bool) -> ShapeResult<()> {
        if self.points.is_empty() {
            self.bbox = Box2I::new(p, Vector2::new(0, 0))?;
        }

        if self.points.is_empty() || allow_duplication || self.clast_point()? != p {
            self.points.push(p);
            self.shapes.push(SHAPES_ARE_PT);
            self.bbox.merge_point(p)?;
        }
        Ok(())
    }

    /// `Remove( int aStartIndex, int aEndIndex )` (shape_line_chain.cpp).
    pub fn remove_range(&mut self, _start_index: i32, _end_index: i32) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::Remove(int, int)
        unported(
            "SHAPE_LINE_CHAIN::Remove(int, int)",
            SHAPE_LINE_CHAIN_P001_UNIT,
        )
    }

    /// `Remove( int aIndex )`: `Remove( aIndex, aIndex )`.
    pub fn remove(&mut self, index: i32) -> ShapeResult<()> {
        self.remove_range(index, index)
    }

    /// `operator!=`: different point counts, or the first index whose points differ.
    pub fn ne(&self, rhs: &ShapeLineChain) -> ShapeResult<bool> {
        if self.point_count() != rhs.point_count() {
            return Ok(true);
        }

        for i in 0..self.point_count() {
            if self.cpoint(i)? != rhs.cpoint(i)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// `ArcCount`.
    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }

    /// `IsSharedPt( size_t aIndex )` (shape_line_chain.cpp).
    pub fn is_shared_pt(&self, _index: u64) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p002): SHAPE_LINE_CHAIN::IsSharedPt
        unported("SHAPE_LINE_CHAIN::IsSharedPt", SHAPE_LINE_CHAIN_P002_UNIT)
    }

    /// `m_shapes[aSegment]`, undefined outside the vector.
    fn shape_at(&self, segment: u64) -> ShapeResult<(i64, i64)> {
        let i = usize::try_from(segment).map_err(|_| ShapeError::Undefined)?;
        self.shapes.get(i).copied().ok_or(ShapeError::Undefined)
    }

    /// `ArcIndex( size_t aSegment )`: the second index of a shared point, else the first.
    pub fn arc_index(&self, segment: u64) -> ShapeResult<i64> {
        if self.is_shared_pt(segment)? {
            Ok(self.shape_at(segment)?.1)
        } else {
            Ok(self.shape_at(segment)?.0)
        }
    }

    /// `reversedArcIndex( size_t aSegment )` (protected): the first index of a shared point,
    /// else the second.
    pub fn reversed_arc_index(&self, segment: u64) -> ShapeResult<i64> {
        if self.is_shared_pt(segment)? {
            Ok(self.shape_at(segment)?.0)
        } else {
            Ok(self.shape_at(segment)?.1)
        }
    }

    /// `Arc( size_t aArc )`: `m_arcs[aArc]`, undefined outside the vector.
    pub fn arc(&self, arc: u64) -> ShapeResult<&ShapeArc> {
        let i = usize::try_from(arc).map_err(|_| ShapeError::Undefined)?;
        self.arcs.get(i).ok_or(ShapeError::Undefined)
    }

    /// `amendArc( size_t aArcIndex, const VECTOR2I& aNewStart, const VECTOR2I& aNewEnd )`
    /// (shape_line_chain.cpp).
    fn amend_arc(
        &mut self,
        _arc_index: u64,
        _new_start: Vector2I,
        _new_end: Vector2I,
    ) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::amendArc
        unported("SHAPE_LINE_CHAIN::amendArc", SHAPE_LINE_CHAIN_P001_UNIT)
    }

    /// `amendArcStart( size_t aArcIndex, const VECTOR2I& aNewStart )` (protected):
    /// `amendArc( aArcIndex, aNewStart, m_arcs[aArcIndex].GetP1() )`. The argument
    /// `m_arcs[aArcIndex]` is evaluated before the call, so an index outside the arcs is
    /// undefined first.
    pub fn amend_arc_start(&mut self, arc_index: u64, new_start: Vector2I) -> ShapeResult<()> {
        let end = self.arc(arc_index)?.get_p1();
        self.amend_arc(arc_index, new_start, end)
    }

    /// `amendArcEnd( size_t aArcIndex, const VECTOR2I& aNewEnd )` (protected):
    /// `amendArc( aArcIndex, m_arcs[aArcIndex].GetP0(), aNewEnd )`.
    pub fn amend_arc_end(&mut self, arc_index: u64, new_end: Vector2I) -> ShapeResult<()> {
        let start = self.arc(arc_index)?.get_p0();
        self.amend_arc(arc_index, start, new_end)
    }

    /// `SHAPE_LINE_CHAIN_BASE::SquaredDistance( const VECTOR2I& aP, bool aOutlineOnly )`
    /// (shape_line_chain.cpp).
    pub fn squared_distance(&self, _p: Vector2I, _outline_only: bool) -> ShapeResult<i64> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN_BASE::SquaredDistance
        unported(
            "SHAPE_LINE_CHAIN_BASE::SquaredDistance",
            SHAPE_LINE_CHAIN_P001_UNIT,
        )
    }

    /// `Distance( const VECTOR2I& aP, bool aOutlineOnly )`: `sqrt` of the `SEG::ecoord` squared
    /// distance (converted to `double`), returned as `int` by truncation, which is undefined
    /// at or above 2^31.
    pub fn distance(&self, p: Vector2I, outline_only: bool) -> ShapeResult<i32> {
        let squared = self.squared_distance(p, outline_only)?;
        let root = (squared as f64).sqrt().trunc();
        if root.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&root) {
            return Err(ShapeError::Undefined);
        }
        Ok(root as i32)
    }

    /// Memberwise state with arcs, for in-crate tests: the C++ reaches arcs only through
    /// out-of-line members (shape_line_chain.cpp and shape_arc.cpp).
    #[cfg(test)]
    pub(crate) fn with_members(
        points: Vec<Vector2I>,
        shapes: Vec<(i64, i64)>,
        arcs: Vec<ShapeArc>,
        closed: bool,
    ) -> Self {
        Self {
            points,
            shapes,
            arcs,
            closed,
            ..Self::new()
        }
    }
}

impl Shape for ShapeLineChain {
    fn shape_type(&self) -> ShapeType {
        ShapeType::LineChain
    }

    /// `Clone` (shape_line_chain.cpp).
    fn clone_shape(&self) -> ShapeResult<Option<Box<dyn Shape>>> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p002): SHAPE_LINE_CHAIN::Clone
        unported("SHAPE_LINE_CHAIN::Clone", SHAPE_LINE_CHAIN_P002_UNIT)
    }

    /// `Collide( const VECTOR2I& ... )` (shape_line_chain.cpp).
    fn collide_point(
        &self,
        _p: Vector2I,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::Collide(const VECTOR2I&, int, int*, VECTOR2I*)
        unported(
            "SHAPE_LINE_CHAIN::Collide(const VECTOR2I&, int, int*, VECTOR2I*)",
            SHAPE_LINE_CHAIN_P001_UNIT,
        )
    }

    /// `Collide( const SEG& ... )` (shape_line_chain.cpp).
    fn collide_seg(
        &self,
        _seg: &Seg,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::Collide(const SEG&, int, int*, VECTOR2I*)
        unported(
            "SHAPE_LINE_CHAIN::Collide(const SEG&, int, int*, VECTOR2I*)",
            SHAPE_LINE_CHAIN_P001_UNIT,
        )
    }

    /// `BBox( int aClearance = 0 )`: a fresh `BOX2I` computed from the points (left default for
    /// an empty chain), inflated by `aClearance + m_width` (an `int` sum, undefined on overflow)
    /// unless both are 0.
    fn bbox(&self, clearance: i32) -> ShapeResult<Box2I> {
        let mut bbox = Box2I::default();
        bbox.compute(&self.points)?;

        if clearance != 0 || self.width != 0 {
            let delta = clearance
                .checked_add(self.width)
                .ok_or(ShapeError::Undefined)?;
            bbox.inflate_uniform(delta)?;
        }

        Ok(bbox)
    }

    /// `Rotate` (shape_line_chain.cpp).
    fn rotate(&mut self, _angle: EdaAngle, _center: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_line_chain.application.ui.p001): SHAPE_LINE_CHAIN::Rotate
        unported("SHAPE_LINE_CHAIN::Rotate", SHAPE_LINE_CHAIN_P001_UNIT)
    }

    /// `SetWidth( int aWidth )`. `GetWidth` is not overridden: it stays `SHAPE`'s 0.
    fn set_width(&mut self, width: i32) {
        self.width = width;
    }

    /// `Move( const VECTOR2I& aVector )`: every point (`+=`, `x` then `y`, undefined on
    /// overflow), then every arc (`SHAPE_ARC::Move`, shape_arc.cpp), then the cached box's
    /// origin.
    fn move_by(&mut self, vector: Vector2I) -> ShapeResult<()> {
        for pt in &mut self.points {
            pt.add_assign(vector)?;
        }

        for arc in &mut self.arcs {
            arc.move_by(vector)?;
        }

        self.bbox.move_by(vector)?;
        Ok(())
    }

    /// `IsSolid`: `false`.
    fn is_solid(&self) -> ShapeResult<bool> {
        Ok(false)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ShapeLineChainBase for ShapeLineChain {
    /// `GetPoint( int aIndex )`: `CPoint( aIndex )`.
    fn get_point(&self, index: i32) -> ShapeResult<Vector2I> {
        self.cpoint(index)
    }

    /// `GetSegment( int aIndex )`: `CSegment( aIndex )`.
    fn get_segment(&self, index: i32) -> ShapeResult<Seg> {
        self.csegment(index)
    }

    /// `GetPointCount`: `PointCount()` converted to `size_t`.
    fn get_point_count(&self) -> usize {
        self.point_count() as usize
    }

    /// `GetSegmentCount`: `SegmentCount()` (never negative) converted to `size_t`.
    fn get_segment_count(&self) -> usize {
        self.segment_count() as usize
    }

    /// `IsClosed`.
    fn is_closed(&self) -> bool {
        self.closed
    }

    /// `GetCachedBBox`: `&m_bbox`.
    fn get_cached_bbox(&self) -> Option<&Box2I> {
        Some(&self.bbox)
    }
}
