// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/nearest.h.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `NEARABLE_GEOM` (libs/kimath/include/geometry/nearest.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.pure.p001`.
//!
//! Both `GetNearestPoint` overloads are only declared here; their bodies are in nearest.cpp
//! (`libs.kimath.src.geometry.nearest.application.pure.p001`).
use crate::box2::Box2I;
use crate::circle::Circle;
use crate::half_line::HalfLine;
use crate::line::Line;
use crate::seg_header::Seg;
use crate::shape_arc::ShapeArc;
use crate::shape_ellipse::ShapeEllipse;
use crate::vector::Vector2I;

/// `NEARABLE_GEOM`: `std::variant<LINE, HALF_LINE, SEG, CIRCLE, SHAPE_ARC, SHAPE_ELLIPSE, BOX2I,
/// VECTOR2I>`, in that alternative order.
#[derive(Debug)]
pub enum NearableGeom {
    Line(Line),
    HalfLine(HalfLine),
    Seg(Seg),
    Circle(Circle),
    ShapeArc(ShapeArc),
    ShapeEllipse(ShapeEllipse),
    Box2I(Box2I),
    Vector2I(Vector2I),
}
