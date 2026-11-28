// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/intersection.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! The types of libs/kimath/include/geometry/intersection.h at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! `INTERSECTION_VISITOR` has no inline body: both constructors, which bind its two reference
//! members and the optional contact pointer, and every `operator()` are in intersection.cpp
//! (`libs.kimath.src.geometry.intersection.application.ui.p001`), which defines the visitor
//! together with them.
use crate::box2::Box2I;
use crate::circle::Circle;
use crate::half_line::HalfLine;
use crate::line::Line;
use crate::seg_header::Seg;
use crate::shape_arc::ShapeArc;
use crate::shape_ellipse::ShapeEllipse;

/// `INTERSECTABLE_GEOM`: `std::variant<LINE, HALF_LINE, SEG, CIRCLE, SHAPE_ARC, SHAPE_ELLIPSE,
/// BOX2I>`, in that alternative order.
#[derive(Debug)]
pub enum IntersectableGeom {
    Line(Line),
    HalfLine(HalfLine),
    Seg(Seg),
    Circle(Circle),
    ShapeArc(ShapeArc),
    ShapeEllipse(ShapeEllipse),
    Box2I(Box2I),
}

/// `INTERSECTION_CONTACT`: how two geometries meet. `Default` gives the member initializers
/// (both false).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntersectionContact {
    /// Touch at one point, no crossing.
    pub tangent: bool,
    /// Collinear or concentric, more than one point shared. Arcs are concentric only if the
    /// carriers they rebuild from their three points match exactly.
    pub overlapping: bool,
}
