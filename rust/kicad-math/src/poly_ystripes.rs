// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad poly_ystripes_index.h, copyright The KiCad Developers.
//! Y stripe index over a borrowed snapshot of SHAPE_POLY_SET contours.
//! The snapshot is a transport boundary, not a replacement SHAPE_POLY_SET implementation.
use crate::eda_angle::{GeometryError, GeometryResult};
use crate::{math_util, Vector2I, VectorDomainError};

/// One outline followed by its holes; vertex and outline order are significant.
pub struct PolygonContours<'a> {
    pub outline: &'a [Vector2I],
    pub holes: &'a [Vec<Vector2I>],
}
#[derive(Clone, Copy, Debug)]
struct Edge {
    p1: Vector2I,
    p2: Vector2I,
    outline: usize,
    hole: bool,
}
#[derive(Debug)]
pub struct PolyYstripesIndex {
    edges: Vec<Edge>,
    stripes: Vec<Vec<usize>>,
    y_min: i64,
    y_max: i64,
    stripe_height: i64,
    stripe_count: i32,
    outline_count: usize,
}
impl PolyYstripesIndex {
    /// Like the C++ default constructor, starts empty. Height is 1 even before Build.
    pub fn new() -> Self {
        Self {
            stripe_height: 1,
            ..Self::default()
        }
    }
    pub fn build(&mut self, polygons: &[PolygonContours<'_>]) -> GeometryResult<()> {
        self.outline_count = polygons.len();
        self.edges.clear();
        self.stripes.clear();
        let mut total_edges = 0_i32;
        for polygon in polygons {
            for points in
                core::iter::once(polygon.outline).chain(polygon.holes.iter().map(Vec::as_slice))
            {
                let count =
                    i32::try_from(points.len()).map_err(|_| GeometryError::IntegerOverflow)?;
                total_edges = total_edges
                    .checked_add(count)
                    .ok_or(GeometryError::IntegerOverflow)?;
            }
        }
        if total_edges == 0 {
            return Ok(());
        }
        i32::try_from(self.outline_count).map_err(|_| GeometryError::IntegerOverflow)?;
        self.edges
            .try_reserve(total_edges as usize)
            .map_err(|_| GeometryError::ResourceLimit)?;
        let mut y_min = i64::from(i32::MAX);
        let mut y_max = i64::from(i32::MIN);
        for (outline, polygon) in polygons.iter().enumerate() {
            for (hole, points) in core::iter::once((false, polygon.outline))
                .chain(polygon.holes.iter().map(|p| (true, p.as_slice())))
            {
                if points.len() < 3 {
                    continue;
                }
                // Pair each vertex with its cyclic successor without unchecked indexing.
                for (&p1, &p2) in points.iter().zip(points.iter().cycle().skip(1)) {
                    if p1.y == p2.y {
                        continue;
                    }
                    self.edges.push(Edge {
                        p1,
                        p2,
                        outline,
                        hole,
                    });
                    y_min = y_min.min(i64::from(p1.y.min(p2.y)));
                    y_max = y_max.max(i64::from(p1.y.max(p2.y)));
                }
            }
        }
        if self.edges.is_empty() {
            return Ok(());
        }
        let mut stripe_count = (self.edges.len() as f64).sqrt() as i32;
        stripe_count = stripe_count.clamp(1, 65536);
        self.y_min = y_min;
        self.y_max = y_max;
        let y_range = y_max - y_min;
        if y_range == 0 {
            stripe_count = 1;
            self.stripe_height = 1;
        } else {
            self.stripe_height = (y_range + i64::from(stripe_count) - 1) / i64::from(stripe_count);
        }
        self.stripe_count = stripe_count;
        self.stripes
            .try_reserve(stripe_count as usize)
            .map_err(|_| GeometryError::ResourceLimit)?;
        self.stripes.resize_with(stripe_count as usize, Vec::new);
        for (index, edge) in self.edges.iter().enumerate() {
            let s_min = self.y_to_stripe(i64::from(edge.p1.y.min(edge.p2.y)));
            let s_max = self.y_to_stripe(i64::from(edge.p1.y.max(edge.p2.y)));
            for s in s_min..=s_max {
                if let Some(stripe) = self.stripes.get_mut(s as usize) {
                    stripe
                        .try_reserve(1)
                        .map_err(|_| GeometryError::ResourceLimit)?;
                    stripe.push(index);
                }
            }
        }
        Ok(())
    }
    // The only inputs are i32 y coordinates plus/minus i32 accuracy. The division
    // can exceed int on an out-of-range query, so preserve Linux signed narrowing.
    fn y_to_stripe(&self, y: i64) -> i32 {
        let offset = y - self.y_min;
        let s = (offset / self.stripe_height) as i32;
        s.min(self.stripe_count - 1).max(0)
    }
    /// Omitted aAccuracy is 0, represented by None.
    pub fn contains(&self, point: Vector2I, accuracy: Option<i32>) -> GeometryResult<bool> {
        if self.edges.is_empty() {
            return Ok(false);
        }
        let accuracy = accuracy.unwrap_or(0);
        let stripe = self.y_to_stripe(i64::from(point.y));
        if stripe >= 0 && stripe < self.stripe_count {
            let mut stack = [0_i32; 8];
            let mut heap = Vec::new();
            let crossings = if self.outline_count > 8 {
                heap.try_reserve(self.outline_count)
                    .map_err(|_| GeometryError::ResourceLimit)?;
                heap.resize(self.outline_count, 0_i32);
                heap.as_mut_slice()
            } else {
                &mut stack[..]
            };
            if let Some(indices) = self.stripes.get(stripe as usize) {
                for &index in indices {
                    let Some(edge) = self.edges.get(index) else {
                        continue;
                    };
                    let (p1, p2) = (edge.p1, edge.p2);
                    if (p1.y >= point.y) == (p2.y >= point.y) {
                        continue;
                    }
                    let diff = p2.try_sub(p1).map_err(vector_error)?;
                    let y = point
                        .y
                        .checked_sub(p1.y)
                        .ok_or(GeometryError::IntegerOverflow)?;
                    let d = math_util::rescale_i32(diff.x, y, diff.y)
                        .map_err(|_| GeometryError::IntegerOverflow)?;
                    let x = point
                        .x
                        .checked_sub(p1.x)
                        .ok_or(GeometryError::IntegerOverflow)?;
                    if x < d {
                        if let Some(count) = crossings.get_mut(edge.outline) {
                            *count = count.checked_add(1).ok_or(GeometryError::IntegerOverflow)?;
                        }
                    }
                }
            }
            for count in crossings.iter().take(self.outline_count) {
                if count & 1 != 0 {
                    return Ok(true);
                }
            }
        }
        if accuracy > 1 {
            let s_min = self.y_to_stripe(i64::from(point.y) - i64::from(accuracy));
            let s_max = self.y_to_stripe(i64::from(point.y) + i64::from(accuracy));
            let accuracy_sq = seg_square(accuracy)?;
            for s in s_min..=s_max {
                if s < 0 || s >= self.stripe_count {
                    continue;
                }
                if let Some(indices) = self.stripes.get(s as usize) {
                    for &index in indices {
                        let Some(edge) = self.edges.get(index) else {
                            continue;
                        };
                        if edge.hole {
                            continue;
                        }
                        if seg_squared_distance(edge.p1, edge.p2, point)? <= accuracy_sq {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}
fn vector_error(error: VectorDomainError) -> GeometryError {
    match error {
        VectorDomainError::Unported { upstream, unit } => {
            GeometryError::Unported { upstream, unit }
        }
        _ => GeometryError::IntegerOverflow,
    }
}
/// `SEG::Square( aAccuracy )` (seg.h, `libs.kimath.include.geometry.application.ui.p003`):
/// `ecoord( a ) * a`, which always fits `int64_t`.
fn seg_square(accuracy: i32) -> GeometryResult<i64> {
    Ok(crate::seg::Seg::square(accuracy))
}
/// `SEG( e.A, e.B ).SquaredDistance( aPoint )`.
fn seg_squared_distance(a: Vector2I, b: Vector2I, point: Vector2I) -> GeometryResult<i64> {
    crate::seg::Seg::new(a, b)
        .squared_distance_point(point)
        .map_err(|error| match error {
            crate::seg::SegError::Unported { upstream, unit } => {
                GeometryError::Unported { upstream, unit }
            }
            crate::seg::SegError::Undefined => GeometryError::IntegerOverflow,
        })
}

impl Default for PolyYstripesIndex {
    fn default() -> Self {
        Self {
            edges: Vec::new(),
            stripes: Vec::new(),
            y_min: 0,
            y_max: 0,
            stripe_height: 1,
            stripe_count: 0,
            outline_count: 0,
        }
    }
}
