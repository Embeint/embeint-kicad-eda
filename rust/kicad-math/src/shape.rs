// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape.h.
// Copyright (C) 2013 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_TYPE`, `SHAPE_BASE`, `SHAPE` and `SHAPE_LINE_CHAIN_BASE`
//! (libs/kimath/include/geometry/shape.h) at pin 4cf939778df04d2b747bb2c4714fc5014a7565c7,
//! owned by `libs.kimath.include.geometry.application.ui.p003`.
//!
//! The C++ hierarchy `SHAPE_BASE` <- `SHAPE` <- concrete shapes is one object-safe trait,
//! [`Shape`]: `SHAPE_BASE`'s and `SHAPE`'s virtual members are trait methods, their inline
//! bodies the trait's default methods, and each pure virtual a required method. An override is
//! the implementing type's method. `SHAPE_BASE::m_type` is fixed per concrete class (every
//! constructor passes a constant), so [`Shape::shape_type`] is that constant. `dynamic_cast`
//! is [`Shape::as_any`]. `SHAPE_LINE_CHAIN_BASE` is the [`ShapeLineChainBase`] subtrait.
//!
//! A C++ default argument of a virtual member is bound by the static type at the call site;
//! every declaration in these headers repeats the same defaults (0, `nullptr`, `{ 0, 0 }`), so
//! Rust callers pass them explicitly. Out-parameter pointers are `Option<&mut T>`.
//!
//! Out-of-line members of `SHAPE` (`GetClearance`, `Distance`, `SquaredDistance`,
//! `NearestPoints`, `PointInside`, `Parse`, `Format`) belong to shape.cpp and are not part of
//! this port. The two `SHAPE::Collide( const SHAPE* ... )` bodies (shape_collisions.cpp) are
//! reached by inline overrides here and return [`ShapeError::Unported`] naming their owner.
use core::any::Any;

use crate::approximation::ErrorLoc;
use crate::box2::Box2I;
use crate::circle::CircleError;
use crate::eda_angle::EdaAngle;
use crate::seg::{Seg, SegError};
use crate::vector::{Vector2, Vector2I, VectorDomainError};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.ui.p003";
/// Owner of shape_collisions.cpp (`SHAPE::Collide( const SHAPE*, ... )`).
pub const SHAPE_COLLISIONS_UNIT: &str =
    "libs.kimath.src.geometry.shape_collisions.application.ui.p001";

/// `SHAPE::MIN_PRECISION_IU`: the minimum precision for all the points in a shape.
pub const MIN_PRECISION_IU: i32 = 4;

/// Why a shape operation produced no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeError {
    /// The pinned C++ evaluation is undefined for this input (signed overflow, an out-of-range
    /// floating-to-integer conversion).
    Undefined,
    /// A dependency call that is not yet ported.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

pub type ShapeResult<T> = Result<T, ShapeError>;

impl From<VectorDomainError> for ShapeError {
    fn from(error: VectorDomainError) -> Self {
        match error {
            VectorDomainError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            _ => Self::Undefined,
        }
    }
}

impl From<SegError> for ShapeError {
    fn from(error: SegError) -> Self {
        match error {
            SegError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            SegError::Undefined => Self::Undefined,
        }
    }
}

impl From<CircleError> for ShapeError {
    fn from(error: CircleError) -> Self {
        match error {
            CircleError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            CircleError::Undefined => Self::Undefined,
        }
    }
}

/// A call into another unit's body that is not ported yet.
pub(crate) const fn unported<T>(upstream: &'static str, unit: &'static str) -> ShapeResult<T> {
    Err(ShapeError::Unported { upstream, unit })
}

/// `SHAPE_TYPE`: all supported shapes, with the C++ enumerator values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(i32)]
pub enum ShapeType {
    /// axis-aligned rectangle
    Rect = 0,
    /// line segment
    Segment,
    /// line chain (polyline)
    LineChain,
    /// circle
    Circle,
    /// simple polygon
    Simple,
    /// set of polygons (with holes, etc.)
    PolySet,
    /// compound shape, consisting of multiple simple shapes
    Compound,
    /// circular arc
    Arc,
    /// empty shape (no shape...)
    Null,
    /// a single triangle belonging to a POLY_SET triangulation
    PolySetTriangle,
    /// ellipse or elliptical arc
    Ellipse,
}

impl ShapeType {
    /// Every enumerator, in value order.
    pub const ALL: [ShapeType; 11] = [
        ShapeType::Rect,
        ShapeType::Segment,
        ShapeType::LineChain,
        ShapeType::Circle,
        ShapeType::Simple,
        ShapeType::PolySet,
        ShapeType::Compound,
        ShapeType::Arc,
        ShapeType::Null,
        ShapeType::PolySetTriangle,
        ShapeType::Ellipse,
    ];

    /// The enumerator value.
    pub const fn value(self) -> i32 {
        self as i32
    }
}

/// Any value a `SHAPE_TYPE` object can hold, named or not.
///
/// `SHAPE_TYPE` is an unscoped enumeration without a fixed underlying type whose enumerators
/// span 0 to 10, so its values are those of the smallest bit-field that holds them
/// ([dcl.enum]/8): 0 to 15. 11 to 15 are defined values without an enumerator (a
/// `static_cast<SHAPE_TYPE>` of them is defined); converting anything outside 0 to 15 is
/// undefined, so [`ShapeTypeValue::new`] refuses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapeTypeValue(u8);

impl ShapeTypeValue {
    /// The largest value of the enumeration's value range.
    pub const MAX: i32 = 15;

    /// `static_cast<SHAPE_TYPE>( value )`, or `None` when that conversion is undefined.
    pub const fn new(value: i32) -> Option<Self> {
        if value < 0 || value > Self::MAX {
            return None;
        }
        Some(Self(value as u8))
    }

    /// The value of an enumerator.
    pub const fn of(t: ShapeType) -> Self {
        Self(t as i32 as u8)
    }

    /// The integer value.
    pub const fn value(self) -> i32 {
        self.0 as i32
    }

    /// The enumerator with this value, if there is one.
    pub const fn named(self) -> Option<ShapeType> {
        match self.0 {
            0 => Some(ShapeType::Rect),
            1 => Some(ShapeType::Segment),
            2 => Some(ShapeType::LineChain),
            3 => Some(ShapeType::Circle),
            4 => Some(ShapeType::Simple),
            5 => Some(ShapeType::PolySet),
            6 => Some(ShapeType::Compound),
            7 => Some(ShapeType::Arc),
            8 => Some(ShapeType::Null),
            9 => Some(ShapeType::PolySetTriangle),
            10 => Some(ShapeType::Ellipse),
            _ => None,
        }
    }
}

impl From<ShapeType> for ShapeTypeValue {
    fn from(t: ShapeType) -> Self {
        Self::of(t)
    }
}

/// `SHAPE_TYPE_asString`. The C++ `switch` names every enumerator; an unnamed value of the
/// enumeration (11 to 15) falls through to `return wxEmptyString`.
pub const fn shape_type_as_string(a: ShapeTypeValue) -> &'static str {
    match a.named() {
        Some(ShapeType::Rect) => "SH_RECT",
        Some(ShapeType::Segment) => "SH_SEGMENT",
        Some(ShapeType::LineChain) => "SH_LINE_CHAIN",
        Some(ShapeType::Circle) => "SH_CIRCLE",
        Some(ShapeType::Simple) => "SH_SIMPLE",
        Some(ShapeType::PolySet) => "SH_POLY_SET",
        Some(ShapeType::Compound) => "SH_COMPOUND",
        Some(ShapeType::Arc) => "SH_ARC",
        Some(ShapeType::Null) => "SH_NULL",
        Some(ShapeType::PolySetTriangle) => "SH_POLY_SET_TRIANGLE",
        Some(ShapeType::Ellipse) => "SH_ELLIPSE",
        None => "", // Just to quiet GCC.
    }
}

/// The `SHAPE::Collide( const SHAPE* aShape, int aClearance, VECTOR2I* aActual )` body
/// (shape_collisions.cpp), which overrides name as `SHAPE::Collide`.
pub fn collide_shape_mtv_base<S: Shape + ?Sized>(
    _this: &S,
    _shape: &dyn Shape,
    _clearance: i32,
    _mtv: Option<&mut Vector2I>,
) -> ShapeResult<bool> {
    // UNPORTED(libs.kimath.src.geometry.shape_collisions.application.ui.p001): SHAPE::Collide(const SHAPE*, int, VECTOR2I*)
    unported(
        "SHAPE::Collide(const SHAPE*, int, VECTOR2I*)",
        SHAPE_COLLISIONS_UNIT,
    )
}

/// The `SHAPE::Collide( const SHAPE* aShape, int aClearance, int* aActual,
/// VECTOR2I* aLocation )` body (shape_collisions.cpp), which overrides name as `SHAPE::Collide`.
pub fn collide_shape_base<S: Shape + ?Sized>(
    _this: &S,
    _shape: &dyn Shape,
    _clearance: i32,
    _actual: Option<&mut i32>,
    _location: Option<&mut Vector2I>,
) -> ShapeResult<bool> {
    // UNPORTED(libs.kimath.src.geometry.shape_collisions.application.ui.p001): SHAPE::Collide(const SHAPE*, int, int*, VECTOR2I*)
    unported(
        "SHAPE::Collide(const SHAPE*, int, int*, VECTOR2I*)",
        SHAPE_COLLISIONS_UNIT,
    )
}

/// `SHAPE_BASE` and `SHAPE`: an abstract shape on the 2D plane.
pub trait Shape: Any + core::fmt::Debug {
    /// `SHAPE_BASE::Type`: the `m_type` every constructor of the concrete class sets.
    fn shape_type(&self) -> ShapeType;

    /// `SHAPE_BASE::TypeName`: `SHAPE_TYPE_asString( m_type )`.
    fn type_name(&self) -> &'static str {
        shape_type_as_string(ShapeTypeValue::of(self.shape_type()))
    }

    /// `SHAPE_BASE::HasIndexableSubshapes`.
    fn has_indexable_subshapes(&self) -> bool {
        false
    }

    /// `SHAPE_BASE::GetIndexableSubshapeCount`.
    fn get_indexable_subshape_count(&self) -> usize {
        0
    }

    /// `SHAPE_BASE::GetIndexableSubshapes`: the base leaves `aSubshapes` untouched. A null
    /// `SHAPE*` element is `None`.
    fn get_indexable_subshapes<'a>(&'a self, _subshapes: &mut Vec<Option<&'a dyn Shape>>) {}

    /// `SHAPE::Clone`: a dynamically allocated copy, `None` for `nullptr`. The base body is
    /// `assert( false ); return nullptr;`; the reference build defines `NDEBUG` (CMake
    /// `Release`), so the assertion is empty and the base returns `nullptr`.
    fn clone_shape(&self) -> ShapeResult<Option<Box<dyn Shape>>> {
        Ok(None)
    }

    /// `SHAPE::IsNull`.
    fn is_null(&self) -> bool {
        self.shape_type() == ShapeType::Null
    }

    /// `SHAPE::Collide( const VECTOR2I& aP, int aClearance = 0, int* aActual = nullptr,
    /// VECTOR2I* aLocation = nullptr )`: `Collide( SEG( aP, aP ), ... )`, dispatched virtually.
    fn collide_point(
        &self,
        p: Vector2I,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        self.collide_seg(&Seg::new(p, p), clearance, actual, location)
    }

    /// `SHAPE::Collide( const SHAPE* aShape, int aClearance, VECTOR2I* aActual )`
    /// (shape_collisions.cpp).
    fn collide_shape_mtv(
        &self,
        shape: &dyn Shape,
        clearance: i32,
        mtv: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        collide_shape_mtv_base(self, shape, clearance, mtv)
    }

    /// `SHAPE::Collide( const SHAPE* aShape, int aClearance = 0, int* aActual = nullptr,
    /// VECTOR2I* aLocation = nullptr )` (shape_collisions.cpp).
    fn collide_shape(
        &self,
        shape: &dyn Shape,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        collide_shape_base(self, shape, clearance, actual, location)
    }

    /// `SHAPE::Collide( const SEG& aSeg, int aClearance = 0, int* aActual = nullptr,
    /// VECTOR2I* aLocation = nullptr )`: pure virtual.
    fn collide_seg(
        &self,
        seg: &Seg,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool>;

    /// `SHAPE::BBox( int aClearance = 0 )`: pure virtual.
    fn bbox(&self, clearance: i32) -> ShapeResult<Box2I>;

    /// `SHAPE::Centre`: `BBox( 0 ).Centre()`, "if nothing better is available".
    fn centre(&self) -> ShapeResult<Vector2I> {
        Ok(self.bbox(0)?.centre()?)
    }

    /// `SHAPE::Rotate( const EDA_ANGLE& aAngle, const VECTOR2I& aCenter = { 0, 0 } )`: pure
    /// virtual.
    fn rotate(&mut self, angle: EdaAngle, center: Vector2I) -> ShapeResult<()>;

    /// `SHAPE::GetStart`: `{}`.
    fn get_start(&self) -> Vector2I {
        Vector2::default()
    }

    /// `SHAPE::GetEnd`: `{}`.
    fn get_end(&self) -> Vector2I {
        Vector2::default()
    }

    /// `SHAPE::GetWidth`.
    fn get_width(&self) -> i32 {
        0
    }

    /// `SHAPE::SetWidth`: the base ignores the width.
    fn set_width(&mut self, _width: i32) {}

    /// `SHAPE::Move`: pure virtual.
    fn move_by(&mut self, vector: Vector2I) -> ShapeResult<()>;

    /// `SHAPE::IsSolid`: pure virtual.
    fn is_solid(&self) -> ShapeResult<bool>;

    /// The object as `Any`, for `dynamic_cast`.
    fn as_any(&self) -> &dyn Any;
}

/// `SHAPE_LINE_CHAIN_BASE`: the abstract base of line chains and simple polygons.
///
/// Its out-of-line overrides (`Collide`, `SquaredDistance`, `PointInside`, `PointOnEdge`,
/// `EdgeContainingPoint`) belong to shape_line_chain.cpp; no concrete line chain is owned here.
pub trait ShapeLineChainBase: Shape {
    /// `GetPoint( int aIndex )`: pure virtual.
    fn get_point(&self, index: i32) -> ShapeResult<Vector2I>;
    /// `GetSegment( int aIndex )`: pure virtual.
    fn get_segment(&self, index: i32) -> ShapeResult<Seg>;
    /// `GetPointCount`: pure virtual.
    fn get_point_count(&self) -> usize;
    /// `GetSegmentCount`: pure virtual.
    fn get_segment_count(&self) -> usize;
    /// `IsClosed`: pure virtual.
    fn is_closed(&self) -> bool;

    /// `GetCachedBBox`: `nullptr` unless overridden.
    fn get_cached_bbox(&self) -> Option<&Box2I> {
        None
    }

    /// `TransformToPolygon( SHAPE_POLY_SET& aBuffer, int aError, ERROR_LOC aErrorLoc )`: an
    /// empty body. `SHAPE_POLY_SET` belongs to a unit that is not ported and the body does not
    /// use it, so it is a type parameter here; `ERROR_LOC` is geometry/approximation.h's
    /// [`ErrorLoc`].
    fn transform_to_polygon<B: ?Sized>(&self, _buffer: &mut B, _error: i32, _error_loc: ErrorLoc)
    where
        Self: Sized,
    {
    }
}
