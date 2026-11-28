// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_arc.h.
// Copyright (C) 2018 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_ARC` inline members (libs/kimath/include/geometry/shape_arc.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p003`.
//!
//! The header defines the default constructor, `Clone`, `GetP0`, `GetP1`, `GetArcMid`,
//! `GetStart`, `GetEnd`, `SetWidth`, `GetWidth`, `IsSolid`, `GetChord`, `DefaultAccuracyForPCB`,
//! `operator==`, `IsCCW`, `IsClockwise` and the `Collide( const SHAPE* ... )` forwarder. Every
//! other member, including the remaining constructors and the copy constructor that `Clone`
//! calls, is in shape_arc.cpp (`libs.kimath.src.geometry.shape_arc.application.ui.p001`, not
//! ported): those calls return [`crate::shape::ShapeError::Unported`] naming that unit.
use core::any::Any;

use crate::box2::Box2I;
use crate::eda_angle::EdaAngle;
use crate::seg::Seg;
use crate::shape::{collide_shape_base, unported, Shape, ShapeResult, ShapeType};
use crate::vector::{Vector2, Vector2I};

/// Owner of shape_arc.cpp.
pub const SHAPE_ARC_UNIT: &str = "libs.kimath.src.geometry.shape_arc.application.ui.p001";

/// `pcbIUScale.mmToIU( mm )` (base_units.h): `(int)( mm < 0 ? mm * IU_PER_MM - 0.5 :
/// mm * IU_PER_MM + 0.5 )` with `PCB_IU_PER_MM = 1e6`.
const fn pcb_mm_to_iu(mm: f64) -> i32 {
    const PCB_IU_PER_MM: f64 = 1e6;
    (if mm < 0.0 {
        mm * PCB_IU_PER_MM - 0.5
    } else {
        mm * PCB_IU_PER_MM + 0.5
    }) as i32
}

/// `ARC_HIGH_DEF_MM` (base_units.h).
pub const ARC_HIGH_DEF_MM: f64 = 0.005;
/// `ARC_HIGH_DEF = pcbIUScale.mmToIU( ARC_HIGH_DEF_MM )`: 5000.
pub const ARC_HIGH_DEF: i32 = pcb_mm_to_iu(ARC_HIGH_DEF_MM);

/// `SHAPE_ARC`: a circular arc through start, mid and end points.
///
/// `bbox`, `center` and `radius` are the values shape_arc.cpp's `update_values` computes; the
/// inline members never read them. There is no `Clone`: the C++ copy constructor is
/// out-of-line ([`ShapeArc::copy_of`]).
#[derive(Debug)]
pub struct ShapeArc {
    start: Vector2I,
    mid: Vector2I,
    end: Vector2I,
    width: i32,
    // Read only by the unported shape_arc.cpp bodies.
    #[allow(dead_code)]
    bbox: Box2I,
    #[allow(dead_code)]
    center: Vector2I,
    #[allow(dead_code)]
    radius: f64,
}

impl Default for ShapeArc {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for ShapeArc {
    /// `operator==`: start, end, mid and width, in that order; the calculated members are not
    /// compared.
    fn eq(&self, arc: &Self) -> bool {
        arc.start == self.start
            && arc.end == self.end
            && arc.mid == self.mid
            && arc.width == self.width
    }
}

impl ShapeArc {
    /// `SHAPE_ARC()`: `SHAPE( SH_ARC )`, `m_width( 0 )`, `m_radius( 0 )`; the points, the
    /// centre and the box are value-initialised (`BOX2()` is (0, 0) with size (0, 0)).
    pub fn new() -> Self {
        Self {
            start: Vector2::default(),
            mid: Vector2::default(),
            end: Vector2::default(),
            width: 0,
            bbox: Box2I::default(),
            center: Vector2::default(),
            radius: 0.0,
        }
    }

    /// `SHAPE_ARC( const VECTOR2I& aArcCenter, const VECTOR2I& aArcStartPoint,
    /// const EDA_ANGLE& aCenterAngle, int aWidth = 0 )` (shape_arc.cpp).
    pub fn from_center_start_angle(
        _center: Vector2I,
        _start: Vector2I,
        _angle: EdaAngle,
        _width: i32,
    ) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::SHAPE_ARC(center, start, angle, width)
        unported(
            "SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const EDA_ANGLE&, int)",
            SHAPE_ARC_UNIT,
        )
    }

    /// `SHAPE_ARC( const VECTOR2I& aArcStart, const VECTOR2I& aArcMid,
    /// const VECTOR2I& aArcEnd, int aWidth )` (shape_arc.cpp).
    pub fn from_three_points(
        _start: Vector2I,
        _mid: Vector2I,
        _end: Vector2I,
        _width: i32,
    ) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::SHAPE_ARC(start, mid, end, width)
        unported(
            "SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const VECTOR2I&, int)",
            SHAPE_ARC_UNIT,
        )
    }

    /// `SHAPE_ARC( const SHAPE_ARC& aOther )` (shape_arc.cpp).
    pub fn copy_of(_other: &ShapeArc) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)
        unported("SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)", SHAPE_ARC_UNIT)
    }

    /// `SHAPE_ARC`'s implicit copy assignment operator (no assignment is declared, so the
    /// compiler defines it inline): `SHAPE`'s (`m_type` is `SH_ARC` on both sides), then every
    /// member in declaration order. Co-ported by `libs.kimath.include.geometry.application.ui.p004`
    /// for `SHAPE_LINE_CHAIN`'s copy assignment of its arcs.
    pub fn assign_from(&mut self, other: &ShapeArc) -> &mut Self {
        self.start = other.start;
        self.mid = other.mid;
        self.end = other.end;
        self.width = other.width;
        self.bbox = other.bbox;
        self.center = other.center;
        self.radius = other.radius;
        self
    }

    /// `GetP0`.
    pub const fn get_p0(&self) -> Vector2I {
        self.start
    }

    /// `GetP1`.
    pub const fn get_p1(&self) -> Vector2I {
        self.end
    }

    /// `GetArcMid`.
    pub const fn get_arc_mid(&self) -> Vector2I {
        self.mid
    }

    /// `GetChord`: `SEG( m_start, m_end )`.
    pub const fn get_chord(&self) -> Seg {
        Seg::new(self.start, self.end)
    }

    /// `static DefaultAccuracyForPCB`: `ARC_HIGH_DEF`.
    pub const fn default_accuracy_for_pcb() -> i32 {
        ARC_HIGH_DEF
    }

    /// `IsCCW`: `VECTOR2L mid = m_mid`, `v1 = m_end - mid`, `v2 = m_start - mid` (exact in
    /// `int64_t`), then `v1.Cross( v2 ) > 0`, whose `int64_t` products can overflow.
    pub fn is_ccw(&self) -> ShapeResult<bool> {
        let mid = Vector2::new(i64::from(self.mid.x), i64::from(self.mid.y));
        let v1 = Vector2::new(i64::from(self.end.x) - mid.x, i64::from(self.end.y) - mid.y);
        let v2 = Vector2::new(
            i64::from(self.start.x) - mid.x,
            i64::from(self.start.y) - mid.y,
        );
        Ok(v1.cross(v2)? > 0)
    }

    /// `IsClockwise`: `!IsCCW()`.
    pub fn is_clockwise(&self) -> ShapeResult<bool> {
        Ok(!self.is_ccw()?)
    }

    /// Memberwise state for in-crate tests of the inline bodies: the C++ can reach these
    /// values only through shape_arc.cpp's constructors.
    #[cfg(test)]
    pub(crate) fn with_points(start: Vector2I, mid: Vector2I, end: Vector2I, width: i32) -> Self {
        Self {
            start,
            mid,
            end,
            width,
            ..Self::new()
        }
    }
}

impl Shape for ShapeArc {
    fn shape_type(&self) -> ShapeType {
        ShapeType::Arc
    }

    /// `Clone`: `new SHAPE_ARC( *this )`, through the out-of-line copy constructor.
    fn clone_shape(&self) -> ShapeResult<Option<Box<dyn Shape>>> {
        let copy = ShapeArc::copy_of(self)?;
        Ok(Some(Box::new(copy)))
    }

    /// `Collide( const SHAPE* ... )`: `SHAPE::Collide( aShape, aClearance, aActual,
    /// aLocation )`.
    fn collide_shape(
        &self,
        shape: &dyn Shape,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        collide_shape_base(self, shape, clearance, actual, location)
    }

    /// `Collide( const VECTOR2I& ... )` (shape_arc.cpp).
    fn collide_point(
        &self,
        _p: Vector2I,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::Collide(const VECTOR2I&, int, int*, VECTOR2I*)
        unported(
            "SHAPE_ARC::Collide(const VECTOR2I&, int, int*, VECTOR2I*)",
            SHAPE_ARC_UNIT,
        )
    }

    /// `Collide( const SEG& ... )` (shape_arc.cpp).
    fn collide_seg(
        &self,
        _seg: &Seg,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::Collide(const SEG&, int, int*, VECTOR2I*)
        unported(
            "SHAPE_ARC::Collide(const SEG&, int, int*, VECTOR2I*)",
            SHAPE_ARC_UNIT,
        )
    }

    /// `BBox` (shape_arc.cpp).
    fn bbox(&self, _clearance: i32) -> ShapeResult<Box2I> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::BBox
        unported("SHAPE_ARC::BBox", SHAPE_ARC_UNIT)
    }

    /// `Rotate` (shape_arc.cpp).
    fn rotate(&mut self, _angle: EdaAngle, _center: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::Rotate
        unported("SHAPE_ARC::Rotate", SHAPE_ARC_UNIT)
    }

    /// `GetStart`: `m_start`.
    fn get_start(&self) -> Vector2I {
        self.start
    }

    /// `GetEnd`: `m_end`.
    fn get_end(&self) -> Vector2I {
        self.end
    }

    /// `GetWidth`: `m_width`.
    fn get_width(&self) -> i32 {
        self.width
    }

    /// `SetWidth`: `m_width = aWidth`.
    fn set_width(&mut self, width: i32) {
        self.width = width;
    }

    /// `Move` (shape_arc.cpp).
    fn move_by(&mut self, _vector: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::Move
        unported("SHAPE_ARC::Move", SHAPE_ARC_UNIT)
    }

    /// `IsSolid`: `true`.
    fn is_solid(&self) -> ShapeResult<bool> {
        Ok(true)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
