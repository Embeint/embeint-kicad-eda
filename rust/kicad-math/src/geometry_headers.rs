// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad ellipse.h / grid_geometry.h, copyright The KiCad Developers.
//! Header-owned bodies only. Out-of-line operations retain their actual unit owners.
use crate::eda_angle::{missing, EdaAngle, GeometryResult};
use crate::{Vector2, Vector2D};

#[derive(Clone, Copy, Debug)]
pub struct Ellipse<T = i32> {
    pub center: Vector2<T>,
    pub major_radius: T,
    pub minor_radius: T,
    pub rotation: EdaAngle,
    pub start_angle: EdaAngle,
    pub end_angle: EdaAngle,
}
impl<T: Default> Default for Ellipse<T> {
    fn default() -> Self {
        Self {
            center: Vector2::new(T::default(), T::default()),
            major_radius: T::default(),
            minor_radius: T::default(),
            rotation: EdaAngle::default(),
            start_angle: EdaAngle::default(),
            end_angle: EdaAngle::default(),
        }
    }
}
impl<T> Ellipse<T> {
    pub fn arc_start_point(&self) -> GeometryResult<Vector2<T>> {
        self.point_at_angle(self.start_angle)
    }
    pub fn arc_end_point(&self) -> GeometryResult<Vector2<T>> {
        self.point_at_angle(self.end_angle)
    }
    fn point_at_angle(&self, _angle: EdaAngle) -> GeometryResult<Vector2<T>> {
        // UNPORTED(libs.kimath.src.geometry.ellipse.application.ui.p001): ELLIPSE::GetPointAtAngle
        missing(
            "ELLIPSE::GetPointAtAngle",
            "libs.kimath.src.geometry.ellipse.application.ui.p001",
        )
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridKind {
    #[default]
    Cartesian,
    Polar,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct GridGeometry {
    pub kind: GridKind,
    pub origin: Vector2D,
    pub pitch: Vector2D,
    pub orientation: f64,
    pub extent: Vector2D,
    pub priority: u32,
}
impl GridGeometry {
    pub fn takes_precedence_over(&self, other: &Self) -> GeometryResult<bool> {
        if self.priority != other.priority {
            return Ok(self.priority > other.priority);
        }
        Ok(self.area()? < other.area()?)
    }
    fn area(&self) -> GeometryResult<f64> {
        // UNPORTED(libs.kimath.src.geometry.grid_geometry.application.ui.p001): GRID_GEOMETRY::Area
        missing(
            "GRID_GEOMETRY::Area",
            "libs.kimath.src.geometry.grid_geometry.application.ui.p001",
        )
    }
}
