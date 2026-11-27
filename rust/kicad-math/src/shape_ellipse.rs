// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_ellipse.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_ELLIPSE` inline members (libs/kimath/include/geometry/shape_ellipse.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p003`.
//!
//! Inline: the defaulted copy constructor and copy assignment, `operator==`, `Clone`, the
//! getters, `IsArc`, `IsSolid` and the `Collide( const SHAPE* ... )` forwarder. Every
//! constructor that builds an ellipse, the setters and the geometry are in shape_ellipse.cpp
//! (`libs.kimath.src.geometry.shape_ellipse.application.ui.p001`, not ported) and return
//! [`crate::shape::ShapeError::Unported`]. So no ellipse can be constructed by ported code yet;
//! the inline bodies are exercised by this unit's tests on memberwise state only.
use core::any::Any;

use crate::box2::Box2I;
use crate::eda_angle::EdaAngle;
use crate::geometry_headers::Ellipse;
use crate::seg::Seg;
use crate::shape::{collide_shape_base, unported, Shape, ShapeResult, ShapeType};
use crate::vector::Vector2I;

/// Owner of shape_ellipse.cpp.
pub const SHAPE_ELLIPSE_UNIT: &str = "libs.kimath.src.geometry.shape_ellipse.application.ui.p001";

/// `SHAPE_ELLIPSE`: an ellipse or elliptical arc. `Clone`/`Copy` are the defaulted (inline)
/// memberwise copy constructor and copy assignment.
#[derive(Clone, Copy, Debug)]
pub struct ShapeEllipse {
    /// `m_ellipse`.
    ellipse: Ellipse<i32>,
    /// `m_isArc`.
    is_arc: bool,
    /// `m_sinRot`, `m_cosRot`, `m_invMajorRSq`, `m_invMinorRSq`: the trigonometry cache
    /// shape_ellipse.cpp keeps; only its unported bodies read it.
    #[allow(dead_code)]
    sin_rot: f64,
    #[allow(dead_code)]
    cos_rot: f64,
    #[allow(dead_code)]
    inv_major_r_sq: f64,
    #[allow(dead_code)]
    inv_minor_r_sq: f64,
}

impl PartialEq for ShapeEllipse {
    /// `operator==`: "The cached trigonometry is derived from these fields, so it is not
    /// compared." `m_isArc`, then the centre, radii and the three angles, short-circuiting in
    /// that order; `EDA_ANGLE::operator==` compares the degree values.
    fn eq(&self, other: &Self) -> bool {
        self.is_arc == other.is_arc
            && self.ellipse.center == other.ellipse.center
            && self.ellipse.major_radius == other.ellipse.major_radius
            && self.ellipse.minor_radius == other.ellipse.minor_radius
            && self.ellipse.rotation == other.ellipse.rotation
            && self.ellipse.start_angle == other.ellipse.start_angle
            && self.ellipse.end_angle == other.ellipse.end_angle
    }
}

impl ShapeEllipse {
    /// `SHAPE_ELLIPSE()` (shape_ellipse.cpp).
    pub fn new() -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::SHAPE_ELLIPSE()
        unported("SHAPE_ELLIPSE::SHAPE_ELLIPSE()", SHAPE_ELLIPSE_UNIT)
    }

    /// `SHAPE_ELLIPSE( const VECTOR2I& aCenter, int aMajorRadius, int aMinorRadius,
    /// const EDA_ANGLE& aRotation )` (shape_ellipse.cpp).
    pub fn from_radii(
        _center: Vector2I,
        _major_radius: i32,
        _minor_radius: i32,
        _rotation: EdaAngle,
    ) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::SHAPE_ELLIPSE(center, major, minor, rotation)
        unported(
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, int, int, const EDA_ANGLE&)",
            SHAPE_ELLIPSE_UNIT,
        )
    }

    /// `SHAPE_ELLIPSE( const VECTOR2I& aCenter, int aMajorRadius, int aMinorRadius,
    /// const EDA_ANGLE& aRotation, const EDA_ANGLE& aStartAngle, const EDA_ANGLE& aEndAngle )`
    /// (shape_ellipse.cpp).
    pub fn arc_from_radii(
        _center: Vector2I,
        _major_radius: i32,
        _minor_radius: i32,
        _rotation: EdaAngle,
        _start_angle: EdaAngle,
        _end_angle: EdaAngle,
    ) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::SHAPE_ELLIPSE(center, major, minor, rotation, start, end)
        unported(
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, int, int, const EDA_ANGLE&, const EDA_ANGLE&, const EDA_ANGLE&)",
            SHAPE_ELLIPSE_UNIT,
        )
    }

    /// `SHAPE_ELLIPSE( const VECTOR2I& aCenter, const VECTOR2I& aMajorEndpoint, double aRatio )`
    /// (shape_ellipse.cpp).
    pub fn from_major_endpoint(
        _center: Vector2I,
        _major_endpoint: Vector2I,
        _ratio: f64,
    ) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::SHAPE_ELLIPSE(center, majorEndpoint, ratio)
        unported(
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, const VECTOR2I&, double)",
            SHAPE_ELLIPSE_UNIT,
        )
    }

    /// `SHAPE_ELLIPSE( const VECTOR2I& aCenter, const VECTOR2I& aMajorEndpoint, double aRatio,
    /// const EDA_ANGLE& aStartAngle, const EDA_ANGLE& aEndAngle )` (shape_ellipse.cpp).
    pub fn arc_from_major_endpoint(
        _center: Vector2I,
        _major_endpoint: Vector2I,
        _ratio: f64,
        _start_angle: EdaAngle,
        _end_angle: EdaAngle,
    ) -> ShapeResult<Self> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::SHAPE_ELLIPSE(center, majorEndpoint, ratio, start, end)
        unported(
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, const VECTOR2I&, double, const EDA_ANGLE&, const EDA_ANGLE&)",
            SHAPE_ELLIPSE_UNIT,
        )
    }

    /// `operator=( const SHAPE_ELLIPSE& ) = default`: every member, the cache included.
    pub fn assign_from(&mut self, other: &ShapeEllipse) -> &mut Self {
        *self = *other;
        self
    }

    /// `GetCenter`.
    pub const fn get_center(&self) -> Vector2I {
        self.ellipse.center
    }

    /// `GetMajorRadius`.
    pub const fn get_major_radius(&self) -> i32 {
        self.ellipse.major_radius
    }

    /// `GetMinorRadius`.
    pub const fn get_minor_radius(&self) -> i32 {
        self.ellipse.minor_radius
    }

    /// `GetRotation`.
    pub const fn get_rotation(&self) -> EdaAngle {
        self.ellipse.rotation
    }

    /// `GetStartAngle`.
    pub const fn get_start_angle(&self) -> EdaAngle {
        self.ellipse.start_angle
    }

    /// `GetEndAngle`.
    pub const fn get_end_angle(&self) -> EdaAngle {
        self.ellipse.end_angle
    }

    /// `IsArc`.
    pub const fn is_arc(&self) -> bool {
        self.is_arc
    }

    /// The trigonometry cache; for tests of this unit.
    #[cfg(test)]
    pub(crate) fn cache(&self) -> [f64; 4] {
        [
            self.sin_rot,
            self.cos_rot,
            self.inv_major_r_sq,
            self.inv_minor_r_sq,
        ]
    }

    /// Memberwise state for in-crate tests of the inline bodies; the C++ reaches it only
    /// through shape_ellipse.cpp's constructors.
    #[cfg(test)]
    pub(crate) fn with_members(ellipse: Ellipse<i32>, is_arc: bool, cache: [f64; 4]) -> Self {
        let [sin_rot, cos_rot, inv_major_r_sq, inv_minor_r_sq] = cache;
        Self {
            ellipse,
            is_arc,
            sin_rot,
            cos_rot,
            inv_major_r_sq,
            inv_minor_r_sq,
        }
    }
}

impl Shape for ShapeEllipse {
    fn shape_type(&self) -> ShapeType {
        ShapeType::Ellipse
    }

    /// `Clone`: `new SHAPE_ELLIPSE( *this )`, the defaulted copy constructor.
    fn clone_shape(&self) -> ShapeResult<Option<Box<dyn Shape>>> {
        Ok(Some(Box::new(*self)))
    }

    /// `Collide( const SHAPE* ... )`: `SHAPE::Collide( aShape, aClearance, aActual, aLocation )`.
    fn collide_shape(
        &self,
        shape: &dyn Shape,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        collide_shape_base(self, shape, clearance, actual, location)
    }

    /// `Collide( const SEG& ... )` (shape_ellipse.cpp).
    fn collide_seg(
        &self,
        _seg: &Seg,
        _clearance: i32,
        _actual: Option<&mut i32>,
        _location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::Collide(const SEG&, int, int*, VECTOR2I*)
        unported(
            "SHAPE_ELLIPSE::Collide(const SEG&, int, int*, VECTOR2I*)",
            SHAPE_ELLIPSE_UNIT,
        )
    }

    /// `BBox` (shape_ellipse.cpp).
    fn bbox(&self, _clearance: i32) -> ShapeResult<Box2I> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::BBox
        unported("SHAPE_ELLIPSE::BBox", SHAPE_ELLIPSE_UNIT)
    }

    /// `Rotate` (shape_ellipse.cpp).
    fn rotate(&mut self, _angle: EdaAngle, _center: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::Rotate
        unported("SHAPE_ELLIPSE::Rotate", SHAPE_ELLIPSE_UNIT)
    }

    /// `Move` (shape_ellipse.cpp).
    fn move_by(&mut self, _vector: Vector2I) -> ShapeResult<()> {
        // UNPORTED(libs.kimath.src.geometry.shape_ellipse.application.ui.p001): SHAPE_ELLIPSE::Move
        unported("SHAPE_ELLIPSE::Move", SHAPE_ELLIPSE_UNIT)
    }

    /// `IsSolid`: `!m_isArc`.
    fn is_solid(&self) -> ShapeResult<bool> {
        Ok(!self.is_arc)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
