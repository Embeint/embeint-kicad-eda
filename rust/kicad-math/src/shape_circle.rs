// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad libs/kimath/include/geometry/shape_circle.h.
// Copyright (C) 2013 CERN, author Tomasz Wlostowski.
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.
//! `SHAPE_CIRCLE` (libs/kimath/include/geometry/shape_circle.h) at pin
//! 4cf939778df04d2b747bb2c4714fc5014a7565c7, owned by
//! `libs.kimath.include.geometry.application.ui.p003`.
//!
//! Every member is inline except `Format` and `TransformToPolygon` (shape_circle.cpp, not
//! ported here). The wrapped [`Circle`] uses the accepted circle.cpp port, and `Collide( SEG )`
//! the accepted seg.cpp `NearestPoint`. `Rotate` calls trigo.h's
//! `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` ([`crate::trigo_header`]).
use core::any::Any;

use crate::box2::Box2I;
use crate::circle::Circle;
use crate::eda_angle::EdaAngle;
use crate::seg::Seg;
use crate::shape::{Shape, ShapeError, ShapeResult, ShapeType};
use crate::util;
use crate::vector::{Vector2, Vector2I};

/// Owner of trigo.h (`RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )`).
pub const TRIGO_HEADER_UNIT: &str = crate::trigo_header::UNIT;

/// `SHAPE_CIRCLE`: a solid circle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeCircle {
    circle: Circle,
}

impl Default for ShapeCircle {
    /// `SHAPE_CIRCLE()`: `m_circle()`.
    fn default() -> Self {
        Self::new()
    }
}

/// `int( double )` of a C-style cast: truncation, undefined for a NaN or outside `int`.
fn truncate_to_int(value: f64) -> ShapeResult<i32> {
    if value.is_nan() {
        return Err(ShapeError::Undefined);
    }
    let t = value.trunc();
    if !(-2_147_483_648.0..2_147_483_648.0).contains(&t) {
        return Err(ShapeError::Undefined);
    }
    Ok(t as i32)
}

impl ShapeCircle {
    /// `SHAPE_CIRCLE()`.
    pub const fn new() -> Self {
        Self {
            circle: Circle::new(),
        }
    }

    /// `SHAPE_CIRCLE( const VECTOR2I& aCenter, int aRadius )`.
    pub const fn with_center_radius(center: Vector2I, radius: i32) -> Self {
        Self {
            circle: Circle::with_center_radius(center, radius),
        }
    }

    /// `SHAPE_CIRCLE( const CIRCLE& aCircle )`: the `CIRCLE` copy constructor.
    pub const fn from_circle(circle: &Circle) -> Self {
        Self {
            circle: Circle::copy_of(circle),
        }
    }

    /// `SHAPE_CIRCLE( const SHAPE_CIRCLE& aOther )`: `SHAPE( SH_CIRCLE )` and the `CIRCLE`
    /// copy constructor of `aOther.m_circle`.
    pub const fn copy_of(other: &ShapeCircle) -> Self {
        Self::from_circle(&other.circle)
    }

    /// `SHAPE_CIRCLE& operator=( const SHAPE_CIRCLE& ) = default`: `SHAPE`'s implicit
    /// assignment (`m_type`, `SH_CIRCLE` on both sides) and `CIRCLE`'s.
    pub fn assign_from(&mut self, other: &ShapeCircle) -> &mut Self {
        self.circle = other.circle;
        self
    }

    /// `SetRadius`.
    pub fn set_radius(&mut self, radius: i32) {
        self.circle.radius = radius;
    }

    /// `SetCenter`.
    pub fn set_center(&mut self, center: Vector2I) {
        self.circle.center = center;
    }

    /// `GetRadius`.
    pub const fn get_radius(&self) -> i32 {
        self.circle.radius
    }

    /// `GetCenter`.
    pub const fn get_center(&self) -> Vector2I {
        self.circle.center
    }

    /// `GetCircle`: a copy of the wrapped circle.
    pub const fn get_circle(&self) -> Circle {
        Circle::copy_of(&self.circle)
    }
}

impl Shape for ShapeCircle {
    fn shape_type(&self) -> ShapeType {
        ShapeType::Circle
    }

    /// `Clone`: `new SHAPE_CIRCLE( *this )`.
    fn clone_shape(&self) -> ShapeResult<Option<Box<dyn Shape>>> {
        Ok(Some(Box::new(ShapeCircle::copy_of(self))))
    }

    /// `BBox`: `rc( Radius + aClearance, Radius + aClearance )`, then
    /// `BOX2I( Center - rc, rc * 2 )`. Every `int` operation is checked; the `BOX2` constructor
    /// normalises a negative size.
    fn bbox(&self, clearance: i32) -> ShapeResult<Box2I> {
        let r = self
            .circle
            .radius
            .checked_add(clearance)
            .ok_or(ShapeError::Undefined)?;
        let rc: Vector2I = Vector2::new(r, r);
        let pos = self.circle.center.try_sub(rc)?;
        let size = rc.try_mul_scalar(2)?;
        Ok(Box2I::new(
            pos,
            Vector2::new(i64::from(size.x), i64::from(size.y)),
        )?)
    }

    /// `Collide( const SEG& aSeg, int aClearance, int* aActual, VECTOR2I* aLocation )`.
    fn collide_seg(
        &self,
        seg: &Seg,
        clearance: i32,
        actual: Option<&mut i32>,
        location: Option<&mut Vector2I>,
    ) -> ShapeResult<bool> {
        let min_dist = clearance
            .checked_add(self.circle.radius)
            .ok_or(ShapeError::Undefined)?;
        let pn = seg.nearest_point_point(self.circle.center)?;
        let dist_sq = pn.try_sub(self.circle.center)?.squared_euclidean_norm()?;

        if dist_sq == 0 || dist_sq < Seg::square(min_dist) {
            if let Some(location) = location {
                let pts = self.circle.intersect_seg(seg)?;
                // The C++ intersects again for the stored point; the result is the same.
                *location = if !pts.is_empty() && dist_sq == 0 {
                    let again = self.circle.intersect_seg(seg)?;
                    *again.first().ok_or(ShapeError::Undefined)?
                } else {
                    pn
                };
            }

            if let Some(actual) = actual {
                // sqrt( int64_t ) converts to double first; (int) truncates.
                let root = truncate_to_int((dist_sq as f64).sqrt())?;
                let diff = root
                    .checked_sub(self.circle.radius)
                    .ok_or(ShapeError::Undefined)?;
                *actual = util::max(0, diff);
            }

            return Ok(true);
        }

        Ok(false)
    }

    /// `Move`: `m_circle.Center += aVector`.
    fn move_by(&mut self, vector: Vector2I) -> ShapeResult<()> {
        self.circle.center.add_assign(vector)?;
        Ok(())
    }

    /// `Rotate`: `RotatePoint( m_circle.Center, aCenter, aAngle )`.
    fn rotate(&mut self, angle: EdaAngle, center: Vector2I) -> ShapeResult<()> {
        // The trigo.h wrapper (libs.kimath.include.application.ui.p001).
        crate::trigo_header::rotate_point_about(&mut self.circle.center, center, angle).map_err(
            |e| match e {
                crate::trigo::TrigoError::Undefined => ShapeError::Undefined,
                crate::trigo::TrigoError::Unported { upstream, unit } => {
                    ShapeError::Unported { upstream, unit }
                }
            },
        )
    }

    fn is_solid(&self) -> ShapeResult<bool> {
        Ok(true)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
