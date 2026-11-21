// SPDX-License-Identifier: GPL-3.0-or-later
//! Obstacle-avoiding track router for the live PCB tools: a port of
//! `tools/live_schematic_mcp/board_router.py`.
//!
//! Pure geometry, no KiCad calls: [`crate::board_design`] gathers the board into a
//! [`RouteProblem`].  Routes run on a grid over the copper the new track may use: inside the board
//! outline less the edge clearance, away from every other net's copper by that pair's clearance, and
//! out of keepouts.  Moves are horizontal, vertical and 45 degrees; a via changes layer where a via
//! fits.  Each finished segment is checked against the region, and the caller confirms the result
//! with KiCad's DRC.  Units are millimetres.
//!
//! Python uses shapely (GEOS); this module uses the `geo` crate (its buffers and boolean operations
//! come from `i_overlay`) and does not reproduce GEOS's polygons in general.  Where the router's
//! decisions depend on the exact polygon (the clearance zones around pads, vias and tracks, which are
//! convex), the few lines of GEOS's offset curve that produce them are followed here, so grid points
//! fall the same way unless they are within rounding of a boundary.  Other shapes use `geo`'s buffer
//! or the true distance, which differ from GEOS's by the sag of its arc segments (a few
//! micrometres).  The comparison with the Python router is behavioural (rust/MCP_PARITY_PLAN.md).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::f64::consts::PI;

use embeint_ipc::client::Error;
use geo::algorithm::buffer::{BufferStyle, LineCap, LineJoin};
use geo::algorithm::kernels::{Kernel, Orientation, RobustKernel};
use geo::coordinate_position::CoordPos;
use geo::{
    BooleanOps, BoundingRect, Buffer, Contains, Coord, CoordinatePosition, Distance, Euclidean,
    Geometry, Intersects, Line, LineString, MultiLineString, MultiPolygon, Point, Polygon, Rect,
};

pub(crate) type C = Coord<f64>;

pub(crate) const SQRT2: f64 = std::f64::consts::SQRT_2;
const DIRECTIONS: [(i64, i64); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
/// shapely's default `quad_segs` for `geometry.buffer(...)`.
pub(crate) const QUAD_SEGS: u32 = 16;
/// The tolerance of `allowed.buffer(1e-6).covers(...)`.
const EPS: f64 = 1e-6;

fn c(x: f64, y: f64) -> C {
    Coord { x, y }
}

// ---- Python's arithmetic ------------------------------------------------------------------------

/// frexp's exponent: `x = m * 2**e` with 0.5 <= m < 1 (x positive and finite).
fn frexp_exponent(x: f64) -> i32 {
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    if exponent == 0 {
        // subnormal: scale into the normal range first
        return frexp_exponent(x * 2f64.powi(64)) - 64;
    }
    exponent - 1022
}

/// CPython's `vector_norm` (math.hypot and math.dist, 3.12): the norm of finite non-negative
/// values, correctly rounded in nearly every case, so distances are the same bits as Python's.
fn vector_norm(values: &[f64], max: f64) -> f64 {
    if max.is_infinite() {
        return max;
    }
    if values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    if max == 0.0 || values.len() <= 1 {
        return max;
    }
    let max_e = frexp_exponent(max);
    if max_e < -1023 {
        let scaled: Vec<f64> = values.iter().map(|v| v / f64::MIN_POSITIVE).collect();
        return f64::MIN_POSITIVE * vector_norm(&scaled, max / f64::MIN_POSITIVE);
    }
    let scale = 2f64.powi(-max_e);
    let (mut csum, mut frac1, mut frac2) = (1.0_f64, 0.0_f64, 0.0_f64);
    for value in values {
        let x = value * scale;
        // dl_mul: an error-free product (fma), then dl_fast_sum
        let hi = x * x;
        let lo = x.mul_add(x, -hi);
        let sum = csum + hi;
        frac2 += (csum - sum) + hi;
        csum = sum;
        frac1 += lo;
    }
    let mut h = (csum - 1.0 + (frac1 + frac2)).sqrt();
    let hi = -h * h;
    let lo = (-h).mul_add(h, -hi);
    let sum = csum + hi;
    frac2 += (csum - sum) + hi;
    csum = sum;
    frac1 += lo;
    let x = csum - 1.0 + (frac1 + frac2);
    h += x / (2.0 * h);
    h / scale
}

/// `math.hypot(dx, dy)`.
pub(crate) fn py_hypot(dx: f64, dy: f64) -> f64 {
    let (x, y) = (dx.abs(), dy.abs());
    // the largest value that is not NaN, as CPython finds it
    let max = [x, y]
        .into_iter()
        .filter(|v| !v.is_nan())
        .fold(0.0, f64::max);
    vector_norm(&[x, y], max)
}

/// `math.dist(a, b)` for 2-D points.
pub(crate) fn py_dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    py_hypot(a[0] - b[0], a[1] - b[1])
}

/// Python's float `%`: the result has the sign of the divisor.
fn py_mod(x: f64, y: f64) -> f64 {
    let m = x % y;
    if m != 0.0 {
        if (y < 0.0) != (m < 0.0) {
            m + y
        } else {
            m
        }
    } else {
        0.0_f64.copysign(y)
    }
}

/// numpy's index check: a negative index counts from the end, anything else out of range is an
/// IndexError.
fn np_index(index: i64, size: i64, axis: usize) -> Result<usize, Error> {
    let wrapped = if index < 0 { index + size } else { index };
    if wrapped < 0 || wrapped >= size {
        return Err(Error::Value(format!(
            "index {index} is out of bounds for axis {axis} with size {}",
            size.max(0)
        )));
    }
    Ok(wrapped as usize)
}

/// CPython's set of small non-negative ints, for the iteration order of `joined` in
/// [`spanning_order`] (table order, which decides ties).
struct PyIntSet {
    table: Vec<Option<usize>>,
    fill: usize,
}

impl PyIntSet {
    fn new() -> Self {
        PyIntSet {
            table: vec![None; 8],
            fill: 0,
        }
    }

    fn insert_clean(table: &mut [Option<usize>], key: usize) {
        let mask = table.len() - 1;
        let (mut i, mut perturb) = (key & mask, key);
        loop {
            if table[i].is_none() {
                table[i] = Some(key);
                return;
            }
            if i + 9 <= mask {
                for j in 1..=9 {
                    if table[i + j].is_none() {
                        table[i + j] = Some(key);
                        return;
                    }
                }
            }
            perturb >>= 5;
            i = (i * 5 + 1 + perturb) & mask;
        }
    }

    fn add(&mut self, key: usize) {
        if self.contains(key) {
            return;
        }
        let mask = self.table.len() - 1;
        let (mut i, mut perturb) = (key & mask, key);
        'probe: loop {
            let probes = if i + 9 <= mask { 9 } else { 0 };
            for j in 0..=probes {
                if self.table[i + j].is_none() {
                    self.table[i + j] = Some(key);
                    break 'probe;
                }
            }
            perturb >>= 5;
            i = (i * 5 + 1 + perturb) & mask;
        }
        self.fill += 1;
        if self.fill * 5 >= mask * 3 {
            let used = self.fill;
            let minused = if used > 50000 { used * 2 } else { used * 4 };
            let mut size = 8;
            while size <= minused {
                size <<= 1;
            }
            let mut table = vec![None; size];
            for key in self.table.iter().flatten() {
                Self::insert_clean(&mut table, *key);
            }
            self.table = table;
        }
    }

    fn contains(&self, key: usize) -> bool {
        self.table.contains(&Some(key))
    }

    fn len(&self) -> usize {
        self.fill
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.table.iter().flatten().copied()
    }
}

// ---- GEOS's buffers of simple shapes ------------------------------------------------------------

/// GEOS's OffsetSegmentString: offset curve points, skipping a point within `distance * 1e-6` of
/// the previous one.
struct SegList {
    points: Vec<C>,
    min_distance: f64,
}

impl SegList {
    fn new(distance: f64) -> Self {
        SegList {
            points: Vec::new(),
            min_distance: distance * 1e-6,
        }
    }

    fn add(&mut self, p: C) {
        if let Some(last) = self.points.last() {
            let (dx, dy) = (p.x - last.x, p.y - last.y);
            if (dx * dx + dy * dy).sqrt() < self.min_distance {
                return;
            }
        }
        self.points.push(p);
    }

    fn close(&mut self) -> Vec<C> {
        if let (Some(&first), Some(&last)) = (self.points.first(), self.points.last()) {
            if first != last {
                self.points.push(first);
            }
        }
        std::mem::take(&mut self.points)
    }

    /// `addDirectedFillet`: points on the circle round `p` from `start` (included) towards `end`
    /// (excluded), `direction` -1 clockwise or 1 counter-clockwise.
    fn fillet(&mut self, p: C, start: f64, end: f64, direction: i32, radius: f64, quad_segs: u32) {
        let quantum = PI / 2.0 / f64::from(quad_segs);
        let total = (start - end).abs();
        let n = (total / quantum + 0.5) as i32;
        if n < 1 {
            return;
        }
        let increment = total / f64::from(n);
        for i in 0..n {
            let angle = start + f64::from(direction * i) * increment;
            self.add(c(p.x + radius * angle.cos(), p.y + radius * angle.sin()));
        }
    }

    /// `addCornerFillet`.
    fn corner_fillet(&mut self, p: C, p0: C, p1: C, direction: i32, radius: f64, quad_segs: u32) {
        let mut start = (p0.y - p.y).atan2(p0.x - p.x);
        let end = (p1.y - p.y).atan2(p1.x - p.x);
        if direction < 0 {
            if start <= end {
                start += 2.0 * PI;
            }
        } else if start >= end {
            start -= 2.0 * PI;
        }
        self.add(p0);
        self.fillet(p, start, end, direction, radius, quad_segs);
        self.add(p1);
    }
}

/// GEOS's `computeOffsetSegment`: the segment moved `distance` to its left (`side` 1) or right
/// (-1).
fn offset_segment(p0: C, p1: C, side: f64, distance: f64) -> (C, C) {
    let (dx, dy) = (p1.x - p0.x, p1.y - p0.y);
    let len = (dx * dx + dy * dy).sqrt();
    let ux = side * distance * dx / len;
    let uy = side * distance * dy / len;
    (c(p0.x - uy, p0.y + ux), c(p1.x - uy, p1.y + ux))
}

fn orientation(p: C, q: C, r: C) -> i32 {
    match RobustKernel::orient2d(p, q, r) {
        Orientation::CounterClockwise => 1,
        Orientation::Clockwise => -1,
        Orientation::Collinear => 0,
    }
}

/// `Point(p).buffer(r, quad_segs)`: GEOS's circle, starting at angle 0 and going clockwise.
pub(crate) fn circle(p: C, radius: f64, quad_segs: u32) -> Option<Polygon<f64>> {
    if radius.is_nan() || radius <= 0.0 {
        return None;
    }
    let mut list = SegList::new(radius);
    list.add(c(p.x + radius, p.y));
    list.fillet(p, 0.0, 2.0 * PI, -1, radius, quad_segs);
    Some(Polygon::new(LineString::new(list.close()), vec![]))
}

/// `LineString([p0, p1]).buffer(r, quad_segs)`: GEOS's round-capped stadium (a circle when the
/// points are the same).
pub(crate) fn segment_buffer(p0: C, p1: C, radius: f64, quad_segs: u32) -> Option<Polygon<f64>> {
    if radius.is_nan() || radius <= 0.0 {
        return None;
    }
    if p0 == p1 {
        return circle(p0, radius, quad_segs);
    }
    let mut list = SegList::new(radius);
    let end_cap = |list: &mut SegList, a: C, b: C| {
        let (_, left) = offset_segment(a, b, 1.0, radius);
        let (_, right) = offset_segment(a, b, -1.0, radius);
        let angle = (b.y - a.y).atan2(b.x - a.x);
        list.add(left);
        list.fillet(b, angle + PI / 2.0, angle - PI / 2.0, -1, radius, quad_segs);
        list.add(right);
    };
    list.add(offset_segment(p0, p1, 1.0, radius).1);
    end_cap(&mut list, p0, p1);
    list.add(offset_segment(p1, p0, 1.0, radius).1);
    end_cap(&mut list, p1, p0);
    Some(Polygon::new(LineString::new(list.close()), vec![]))
}

/// A closed ring without repeated consecutive points (GEOS removes them before buffering).
fn clean_ring(ring: &LineString<f64>) -> Vec<C> {
    let mut points: Vec<C> = Vec::new();
    for p in ring.coords() {
        if points.last() != Some(p) {
            points.push(*p);
        }
    }
    if points.len() > 1 && points.first() != points.last() {
        points.push(points[0]);
    }
    points
}

/// Twice the signed area of a closed ring (positive counter-clockwise).
fn ring_area2(ring: &[C]) -> f64 {
    ring.windows(2)
        .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
        .sum()
}

/// Is a closed ring convex and simple (every turn the same way, once round)?
fn is_convex_ring(ring: &[C]) -> bool {
    let n = ring.len();
    if n < 4 {
        return false;
    }
    let mut sign = 0;
    let mut turning = 0.0;
    for i in 0..n - 1 {
        let a = ring[(i + n - 2) % (n - 1)];
        let b = ring[i];
        let cc = ring[(i + 1) % (n - 1)];
        let o = orientation(a, b, cc);
        if o != 0 {
            if sign != 0 && o != sign {
                return false;
            }
            sign = o;
        }
        let d0 = (b.y - a.y).atan2(b.x - a.x);
        let d1 = (cc.y - b.y).atan2(cc.x - b.x);
        let mut turn = d1 - d0;
        while turn > PI {
            turn -= 2.0 * PI;
        }
        while turn < -PI {
            turn += 2.0 * PI;
        }
        turning += turn;
    }
    sign != 0 && (turning.abs() - 2.0 * PI).abs() < 1e-6
}

/// The intersection of two segments, as GEOS's LineIntersector finds it for an inside turn (None
/// when they do not meet).
fn segment_intersection(a0: C, a1: C, b0: C, b1: C) -> Option<C> {
    let (o1, o2) = (orientation(a0, a1, b0), orientation(a0, a1, b1));
    let (o3, o4) = (orientation(b0, b1, a0), orientation(b0, b1, a1));
    if o1 * o2 > 0 || o3 * o4 > 0 || (o1 == 0 && o2 == 0) {
        return None;
    }
    // Axis-parallel lines meet exactly where their fixed coordinates say
    let x = if a0.x == a1.x {
        Some(a0.x)
    } else if b0.x == b1.x {
        Some(b0.x)
    } else {
        None
    };
    let y = if a0.y == a1.y {
        Some(a0.y)
    } else if b0.y == b1.y {
        Some(b0.y)
    } else {
        None
    };
    let (da, db) = (c(a1.x - a0.x, a1.y - a0.y), c(b1.x - b0.x, b1.y - b0.y));
    let denominator = da.x * db.y - da.y * db.x;
    if denominator == 0.0 {
        return None;
    }
    let t = ((b0.x - a0.x) * db.y - (b0.y - a0.y) * db.x) / denominator;
    Some(c(
        x.unwrap_or(a0.x + t * da.x),
        y.unwrap_or(a0.y + t * da.y),
    ))
}

/// `polygon.buffer(distance, quad_segs)` of a convex polygon as GEOS builds it: outwards, the
/// offset edges joined by round fillets; inwards (negative distance), the offset edges meeting at
/// their intersections.  None when the inward offset needs more than that (a fillet too tight for
/// it), for the caller to buffer otherwise.
fn convex_buffer(ring: &[C], distance: f64, quad_segs: u32) -> Option<Option<Polygon<f64>>> {
    if distance == 0.0 {
        return Some(Some(Polygon::new(LineString::new(ring.to_vec()), vec![])));
    }
    let offset = distance.abs();
    if distance < 0.0 {
        // GEOS's isErodedCompletely: nothing is left of a polygon narrower than the inset
        let rect = LineString::new(ring.to_vec()).bounding_rect()?;
        if 2.0 * offset > rect.width().min(rect.height()) {
            return Some(None);
        }
    }
    // Outwards a clockwise ring is offset on its left, a counter-clockwise one on its right;
    // inwards the other way
    let ccw = ring_area2(ring) > 0.0;
    let mut side = if ccw { -1.0 } else { 1.0 };
    if distance < 0.0 {
        side = -side;
    }
    let n = ring.len() - 1;
    let mut list = SegList::new(offset);
    let (mut s1, mut s2) = (ring[n - 1], ring[0]);
    for (i, &next) in ring.iter().enumerate().skip(1) {
        let add_start = i != 1;
        let s0 = s1;
        s1 = s2;
        s2 = next;
        let offset0 = offset_segment(s0, s1, side, offset);
        let offset1 = offset_segment(s1, s2, side, offset);
        if s1 == s2 {
            continue;
        }
        let turn = orientation(s0, s1, s2);
        let outside = (turn < 0 && side > 0.0) || (turn > 0 && side < 0.0);
        if turn == 0 {
            continue;
        }
        if outside {
            let (dx, dy) = (offset1.0.x - offset0.1.x, offset1.0.y - offset0.1.y);
            if (dx * dx + dy * dy).sqrt() < offset * 1e-3 {
                list.add(offset0.1);
                continue;
            }
            if add_start {
                list.add(offset0.1);
            }
            list.corner_fillet(s1, offset0.1, offset1.0, turn, offset, quad_segs);
            list.add(offset1.0);
        } else {
            list.add(segment_intersection(
                offset0.0, offset0.1, offset1.0, offset1.1,
            )?);
        }
    }
    let points = list.close();
    if points.len() < 4 {
        return Some(None);
    }
    Some(Some(Polygon::new(LineString::new(points), vec![])))
}

/// A geometry and, when it is one convex polygon, its ring (whose buffers follow GEOS).
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    pub geometry: Geometry<f64>,
    convex: Option<Vec<C>>,
    bounds: Option<Rect<f64>>,
}

impl Shape {
    pub(crate) fn new(geometry: Geometry<f64>) -> Self {
        let geometry = match geometry {
            Geometry::MultiPolygon(mut mp) if mp.0.len() == 1 => Geometry::Polygon(mp.0.remove(0)),
            other => other,
        };
        let convex = match &geometry {
            Geometry::Polygon(p) if p.interiors().is_empty() => {
                let ring = clean_ring(p.exterior());
                is_convex_ring(&ring).then_some(ring)
            }
            _ => None,
        };
        let bounds = geometry.bounding_rect();
        Shape {
            geometry,
            convex,
            bounds,
        }
    }

    pub(crate) fn polygon(polygon: Polygon<f64>) -> Self {
        Shape::new(Geometry::Polygon(polygon))
    }

    pub(crate) fn multi(area: MultiPolygon<f64>) -> Self {
        Shape::new(Geometry::MultiPolygon(area))
    }

    pub(crate) fn line(points: Vec<C>) -> Self {
        Shape::new(Geometry::LineString(LineString::new(points)))
    }

    pub(crate) fn empty() -> Self {
        Shape::multi(MultiPolygon::new(vec![]))
    }

    /// shapely's `is_empty`.
    pub(crate) fn is_empty(&self) -> bool {
        match &self.geometry {
            Geometry::Polygon(p) => p.exterior().0.is_empty(),
            Geometry::MultiPolygon(mp) => mp.0.iter().all(|p| p.exterior().0.is_empty()),
            Geometry::LineString(l) => l.0.is_empty(),
            _ => self.bounds.is_none(),
        }
    }

    /// One convex polygon (whose buffers follow GEOS).
    pub(crate) fn is_convex(&self) -> bool {
        self.convex.is_some()
    }

    pub(crate) fn bounds(&self) -> Option<Rect<f64>> {
        self.bounds
    }

    /// The polygons of an area (none for a line).
    pub(crate) fn polygons(&self) -> Vec<Polygon<f64>> {
        match &self.geometry {
            Geometry::Polygon(p) => vec![p.clone()],
            Geometry::MultiPolygon(mp) => mp.0.clone(),
            _ => vec![],
        }
    }

    pub(crate) fn area(&self) -> MultiPolygon<f64> {
        MultiPolygon::new(self.polygons())
    }

    /// `shape.buffer(distance, quad_segs)`: GEOS's own polygons for points, two-point lines and
    /// convex polygons, `geo`'s buffer otherwise.
    pub(crate) fn buffer(&self, distance: f64, quad_segs: u32) -> Shape {
        if let Geometry::LineString(line) = &self.geometry {
            let mut points: Vec<C> = Vec::new();
            for p in line.coords() {
                if points.last() != Some(p) {
                    points.push(*p);
                }
            }
            if points.len() <= 2 && !points.is_empty() {
                let last = *points.last().expect("not empty");
                return segment_buffer(points[0], last, distance, quad_segs)
                    .map_or_else(Shape::empty, Shape::polygon);
            }
        }
        if let Some(ring) = &self.convex {
            if let Some(result) = convex_buffer(ring, distance, quad_segs) {
                return result.map_or_else(Shape::empty, Shape::polygon);
            }
        }
        Shape::multi(geo_buffer(&self.geometry, distance, quad_segs))
    }
}

/// `geo`'s buffer with round joins and caps of about the angle GEOS's `quad_segs` gives.
pub(crate) fn geo_buffer(
    geometry: &Geometry<f64>,
    distance: f64,
    quad_segs: u32,
) -> MultiPolygon<f64> {
    if distance == 0.0 {
        return match geometry {
            Geometry::Polygon(p) => MultiPolygon::new(vec![p.clone()]),
            Geometry::MultiPolygon(mp) => mp.clone(),
            _ => MultiPolygon::new(vec![]),
        };
    }
    let angle = PI / 2.0 / f64::from(quad_segs);
    let style = BufferStyle::new(distance)
        .line_join(LineJoin::Round(angle))
        .line_cap(LineCap::Round(angle));
    geometry.buffer_with_style(style)
}

/// `area.buffer(-distance, join_style="mitre")` (mitre limit 5, about 23 degrees).
fn mitre_buffer(area: &MultiPolygon<f64>, distance: f64) -> MultiPolygon<f64> {
    let style = BufferStyle::new(distance).line_join(LineJoin::Miter(2.0 * 0.2_f64.asin()));
    area.buffer_with_style(style)
}

fn rect_polygon(x1: f64, y1: f64, x2: f64, y2: f64) -> Polygon<f64> {
    Polygon::new(
        LineString::new(vec![c(x1, y1), c(x2, y1), c(x2, y2), c(x1, y2), c(x1, y1)]),
        vec![],
    )
}

/// `arc_points(start, mid, end, segments)`: points along the circular arc through start, mid and
/// end (start and end exactly, so arcs join the lines they meet).
pub(crate) fn arc_points(
    start: [f64; 2],
    mid: [f64; 2],
    end: [f64; 2],
    segments: u32,
) -> Vec<[f64; 2]> {
    let ([x1, y1], [x2, y2], [x3, y3]) = (start, mid, end);
    let d = 2.0 * (x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2));
    if d.abs() < 1e-12 {
        return vec![start, end];
    }
    let sq = |x: f64, y: f64| x.powf(2.0) + y.powf(2.0);
    let ux = (sq(x1, y1) * (y2 - y3) + sq(x2, y2) * (y3 - y1) + sq(x3, y3) * (y1 - y2)) / d;
    let uy = (sq(x1, y1) * (x3 - x2) + sq(x2, y2) * (x1 - x3) + sq(x3, y3) * (x2 - x1)) / d;
    let r = py_hypot(x1 - ux, y1 - uy);
    let angle = |p: [f64; 2]| (p[1] - uy).atan2(p[0] - ux);
    let (a1, a2, a3) = (angle(start), angle(mid), angle(end));
    let ccw = |a: f64, b: f64| py_mod(b - a, 2.0 * PI);
    let mut sweep = ccw(a1, a3);
    if ccw(a1, a2) > sweep {
        sweep -= 2.0 * PI;
    }
    let n = f64::from(segments);
    let mut points: Vec<[f64; 2]> = (0..=segments)
        .map(|i| {
            let a = a1 + sweep * f64::from(i) / n;
            [ux + r * a.cos(), uy + r * a.sin()]
        })
        .collect();
    points[0] = start;
    let last = points.len() - 1;
    points[last] = end;
    points
}

// ---- The board outline --------------------------------------------------------------------------

/// The board area and whether shapely would have it as one Polygon (with an `exterior`).
#[derive(Clone, Debug)]
pub(crate) struct Outline {
    pub area: MultiPolygon<f64>,
    pub single: bool,
}

/// GEOS's `makePrecise` on the 1e-4 grid of `set_precision(..., 1e-4)`: rounds half up.
fn grid_key(value: f64) -> i64 {
    let scaled = value * 10000.0;
    let n = scaled.trunc();
    let f = (scaled - n).abs();
    let rounded = if scaled >= 0.0 {
        if f < 0.5 {
            scaled.floor()
        } else if f > 0.5 {
            scaled.ceil()
        } else {
            n + 1.0
        }
    } else if f < 0.5 {
        scaled.ceil()
    } else if f > 0.5 {
        scaled.floor()
    } else {
        n
    };
    rounded as i64
}

fn key_coord(key: (i64, i64)) -> C {
    c(key.0 as f64 / 10000.0, key.1 as f64 / 10000.0)
}

/// Does a segment (grid keys) pass through the hot pixel (the grid cell) round `p`?
fn crosses_pixel(a: (i64, i64), b: (i64, i64), p: (i64, i64)) -> bool {
    let (ax, ay) = (a.0 as f64, a.1 as f64);
    let (bx, by) = (b.0 as f64, b.1 as f64);
    let (px, py) = (p.0 as f64, p.1 as f64);
    if px < ax.min(bx) - 0.5
        || px > ax.max(bx) + 0.5
        || py < ay.min(by) - 0.5
        || py > ay.max(by) + 0.5
    {
        return false;
    }
    // Liang-Barsky against the half-open square [p - 0.5, p + 0.5)
    let (dx, dy) = (bx - ax, by - ay);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (q, d) in [
        (ax - (px - 0.5), -dx),
        ((px + 0.5) - ax, dx),
        (ay - (py - 0.5), -dy),
        ((py + 0.5) - ay, dy),
    ] {
        if d == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / d;
            if d < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t0 <= t1
}

/// A point on the 1e-4 grid, and a directed edge between two.
type Key = (i64, i64);
type Edge = (Key, Key);

/// A ring of the face tracing: its grid keys (closed) and signed area.
struct Ring {
    keys: Vec<(i64, i64)>,
    area: f64,
}

impl Ring {
    fn coords(&self) -> Vec<C> {
        self.keys.iter().map(|k| key_coord(*k)).collect()
    }

    fn envelope(&self) -> (i64, i64, i64, i64) {
        let xs = self.keys.iter().map(|k| k.0);
        let ys = self.keys.iter().map(|k| k.1);
        (
            xs.clone().min().unwrap_or(0),
            ys.clone().min().unwrap_or(0),
            xs.max().unwrap_or(0),
            ys.max().unwrap_or(0),
        )
    }
}

/// `polygonize(unary_union(lines))` on the 1e-4 grid: the faces the line work encloses, each with
/// the outer boundaries of the parts nested in it as holes, as GEOS's Polygonizer assigns them.
fn polygonize(lines: &[Vec<[f64; 2]>]) -> Vec<(Polygon<f64>, f64)> {
    // Snap, then node with the hot pixels of every vertex and crossing (snap rounding)
    let mut segments: Vec<Edge> = Vec::new();
    for line in lines {
        let keys: Vec<(i64, i64)> = line
            .iter()
            .map(|p| (grid_key(p[0]), grid_key(p[1])))
            .collect();
        for w in keys.windows(2) {
            if w[0] != w[1] {
                segments.push((w[0], w[1]));
            }
        }
    }
    let mut pixels: Vec<(i64, i64)> = segments.iter().flat_map(|s| [s.0, s.1]).collect();
    for (i, a) in segments.iter().enumerate() {
        for b in &segments[i + 1..] {
            let p = segment_intersection(
                key_coord(a.0),
                key_coord(a.1),
                key_coord(b.0),
                key_coord(b.1),
            );
            if let Some(p) = p {
                pixels.push((grid_key(p.x), grid_key(p.y)));
            }
        }
    }
    pixels.sort_unstable();
    pixels.dedup();
    let mut edges: Vec<Edge> = Vec::new();
    for &(a, b) in &segments {
        let mut on: Vec<(f64, (i64, i64))> = pixels
            .iter()
            .filter(|p| crosses_pixel(a, b, **p))
            .map(|p| {
                let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
                let t = ((p.0 - a.0) as f64 * dx + (p.1 - a.1) as f64 * dy) / (dx * dx + dy * dy);
                (t, *p)
            })
            .collect();
        on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(Ordering::Equal));
        for w in on.windows(2) {
            let (u, v) = (w[0].1, w[1].1);
            if u != v {
                edges.push(if u < v { (u, v) } else { (v, u) });
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    faces(edges)
}

/// The faces of a planar graph of grid-key edges (dangles and cut edges removed), as polygons with
/// their area.
fn faces(mut edges: Vec<Edge>) -> Vec<(Polygon<f64>, f64)> {
    use std::collections::{BTreeMap, HashMap};
    loop {
        // Dangles: edges at a vertex of degree one
        loop {
            let mut degree: HashMap<(i64, i64), usize> = HashMap::new();
            for (a, b) in &edges {
                *degree.entry(*a).or_default() += 1;
                *degree.entry(*b).or_default() += 1;
            }
            let before = edges.len();
            edges.retain(|(a, b)| degree[a] > 1 && degree[b] > 1);
            if edges.len() == before {
                break;
            }
        }
        // Outgoing edges at each vertex by angle (counter-clockwise)
        let mut out: BTreeMap<(i64, i64), Vec<(i64, i64)>> = BTreeMap::new();
        for (a, b) in &edges {
            out.entry(*a).or_default().push(*b);
            out.entry(*b).or_default().push(*a);
        }
        let angle = |from: (i64, i64), to: (i64, i64)| {
            ((to.1 - from.1) as f64).atan2((to.0 - from.0) as f64)
        };
        for (v, list) in out.iter_mut() {
            let v = *v;
            list.sort_by(|p, q| {
                angle(v, *p)
                    .partial_cmp(&angle(v, *q))
                    .unwrap_or(Ordering::Equal)
            });
        }
        // Trace each directed edge once, the face on its left
        let mut ring_of: HashMap<Edge, usize> = HashMap::new();
        let mut rings: Vec<Ring> = Vec::new();
        for (a, b) in edges.iter().flat_map(|(a, b)| [(*a, *b), (*b, *a)]) {
            if ring_of.contains_key(&(a, b)) {
                continue;
            }
            let index = rings.len();
            let mut keys = vec![a];
            let (mut u, mut v) = (a, b);
            loop {
                ring_of.insert((u, v), index);
                keys.push(v);
                let list = &out[&v];
                let k = list.iter().position(|w| *w == u).unwrap_or(0);
                let w = list[(k + list.len() - 1) % list.len()];
                u = v;
                v = w;
                if (u, v) == (a, b) || ring_of.contains_key(&(u, v)) {
                    break;
                }
            }
            let area = ring_area2(&keys.iter().map(|k| key_coord(*k)).collect::<Vec<_>>()) / 2.0;
            rings.push(Ring { keys, area });
        }
        // Cut edges have the same ring on both sides: remove them and trace again
        let cut: Vec<Edge> = edges
            .iter()
            .filter(|(a, b)| ring_of.get(&(*a, *b)) == ring_of.get(&(*b, *a)))
            .copied()
            .collect();
        if !cut.is_empty() {
            edges.retain(|e| !cut.contains(e));
            continue;
        }
        // Bounded faces (counter-clockwise here) are shells; the others are holes, each in the
        // smallest shell whose envelope contains (and is not) its own and that contains a point of
        // it not on the shell
        let shells: Vec<usize> = (0..rings.len()).filter(|&i| rings[i].area > 0.0).collect();
        let mut holes_of: Vec<Vec<usize>> = vec![Vec::new(); rings.len()];
        for (h, hole) in rings.iter().enumerate().filter(|(_, r)| r.area < 0.0) {
            let he = hole.envelope();
            let mut best: Option<usize> = None;
            for &s in &shells {
                let se = rings[s].envelope();
                if se == he || !(se.0 <= he.0 && se.1 <= he.1 && se.2 >= he.2 && se.3 >= he.3) {
                    continue;
                }
                let shell = Polygon::new(LineString::new(rings[s].coords()), vec![]);
                let Some(point) = hole.keys.iter().find(|k| !rings[s].keys.contains(k)) else {
                    continue;
                };
                if shell.coordinate_position(&key_coord(*point)) != CoordPos::Inside {
                    continue;
                }
                let smaller = best.is_none_or(|b| {
                    let be = rings[b].envelope();
                    be.0 <= se.0 && be.1 <= se.1 && be.2 >= se.2 && be.3 >= se.3
                });
                if smaller {
                    best = Some(s);
                }
            }
            if let Some(s) = best {
                holes_of[s].push(h);
            }
        }
        return shells
            .iter()
            .map(|&s| {
                let holes: Vec<LineString<f64>> = holes_of[s]
                    .iter()
                    .map(|&h| LineString::new(rings[h].coords()))
                    .collect();
                let area = rings[s].area - holes_of[s].iter().map(|&h| -rings[h].area).sum::<f64>();
                (
                    Polygon::new(LineString::new(rings[s].coords()), holes),
                    area,
                )
            })
            .collect();
    }
}

/// GEOS's InteriorPointArea: the middle of the widest stretch of a horizontal line through the
/// polygon at a height between vertices near its middle.
fn interior_point(polygon: &Polygon<f64>) -> Option<C> {
    let rect = polygon.bounding_rect()?;
    let centre = (rect.min().y + rect.max().y) / 2.0;
    let rings: Vec<&LineString<f64>> = std::iter::once(polygon.exterior())
        .chain(polygon.interiors())
        .collect();
    let (mut hi, mut lo) = (rect.max().y, rect.min().y);
    for ring in &rings {
        for p in ring.coords() {
            if p.y <= centre {
                if p.y > lo {
                    lo = p.y;
                }
            } else if p.y < hi {
                hi = p.y;
            }
        }
    }
    let y = (hi + lo) / 2.0;
    let mut crossings: Vec<f64> = Vec::new();
    for ring in &rings {
        for w in ring.0.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                crossings.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
    }
    crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    crossings
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .max_by(|p, q| {
            (p[1] - p[0])
                .partial_cmp(&(q[1] - q[0]))
                .unwrap_or(Ordering::Equal)
        })
        .map(|pair| c((pair[0] + pair[1]) / 2.0, y))
}

/// `outline_from_lines(lines)`: the board area from Edge.Cuts line work, the largest closed region
/// less the regions inside it.
pub(crate) fn outline_from_lines(lines: &[Vec<[f64; 2]>]) -> Option<Outline> {
    if lines.is_empty() {
        return None;
    }
    let mut polygons = polygonize(lines);
    if polygons.is_empty() {
        return None;
    }
    // sorted(..., key=area, reverse=True) is stable
    polygons.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    let outer = Polygon::new(polygons[0].0.exterior().clone(), vec![]);
    let cutouts: Vec<Polygon<f64>> = polygons[1..]
        .iter()
        .filter(|(p, _)| interior_point(p).is_some_and(|point| outer.contains(&point)))
        .map(|(p, _)| p.clone())
        .collect();
    if cutouts.is_empty() {
        return Some(Outline {
            area: MultiPolygon::new(vec![outer]),
            single: true,
        });
    }
    let union = geo::unary_union(cutouts.iter());
    let area = outer.difference(&union);
    let single = area.0.len() <= 1;
    Some(Outline { area, single })
}

// ---- The problem --------------------------------------------------------------------------------

/// Copper the router avoids: its area, its net ("" for copper with no net, avoided by every net),
/// what it is (copper | pad | track | via | hole) and, for a pad, its id.
#[derive(Clone, Debug)]
pub(crate) struct Obstacle {
    pub geometry: Shape,
    pub net: String,
    pub kind: &'static str,
    pub key: String,
}

/// A pad of the board: its net, position, copper per layer (in the order KiCad reported them),
/// number, parent footprint and type.
#[derive(Clone, Debug)]
pub(crate) struct PadInfo {
    pub net: String,
    pub position: [f64; 2],
    pub layers: Vec<(String, Shape)>,
    pub number: String,
    pub parent: String,
}

/// Everything the router avoids, read from the live board (`RouteProblem`).
#[derive(Clone, Debug, Default)]
pub(crate) struct RouteProblem {
    /// copper layer names the board has, top to bottom
    pub layers: Vec<String>,
    pub outline: Option<Outline>,
    pub edge_clearance: f64,
    pub min_clearance: f64,
    /// layer -> obstacles, in insertion order
    pub obstacles: Vec<(String, Vec<Obstacle>)>,
    pub track_keepouts: Vec<(String, Vec<Shape>)>,
    pub via_keepouts: Vec<Shape>,
    pub net_clearance: Vec<(String, f64)>,
    /// pad id -> pad, in insertion order
    pub pads: Vec<(String, PadInfo)>,
}

fn entry<'a, V: Default>(list: &'a mut Vec<(String, V)>, key: &str) -> &'a mut V {
    let index = match list.iter().position(|(k, _)| k == key) {
        Some(index) => index,
        None => {
            list.push((key.to_owned(), V::default()));
            list.len() - 1
        }
    };
    &mut list[index].1
}

impl RouteProblem {
    /// `clearance(net_a, net_b)`: the larger net class clearance, at least the board minimum.
    pub(crate) fn clearance(&self, a: &str, b: &str) -> f64 {
        let of = |net: &str| {
            self.net_clearance
                .iter()
                .find(|(n, _)| n == net)
                .map_or(0.0, |(_, v)| *v)
        };
        // max(min, a, b) returns the first of equal values; the value is what matters
        let mut value = self.min_clearance;
        for v in [of(a), of(b)] {
            if v > value {
                value = v;
            }
        }
        value
    }

    pub(crate) fn obstacles_on(&self, layer: &str) -> &[Obstacle] {
        self.obstacles
            .iter()
            .find(|(l, _)| l == layer)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    pub(crate) fn add_obstacle(&mut self, layer: &str, obstacle: Obstacle) {
        entry(&mut self.obstacles, layer).push(obstacle);
    }

    pub(crate) fn track_keepouts_on(&self, layer: &str) -> &[Shape] {
        self.track_keepouts
            .iter()
            .find(|(l, _)| l == layer)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    pub(crate) fn add_track_keepout(&mut self, layer: &str, area: Shape) {
        entry(&mut self.track_keepouts, layer).push(area);
    }

    pub(crate) fn set_net_clearance(&mut self, net: &str, value: f64) {
        *entry(&mut self.net_clearance, net) = value;
    }

    pub(crate) fn pad(&self, id: &str) -> Option<&PadInfo> {
        self.pads.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    pub(crate) fn pad_mut(&mut self, id: &str) -> Option<&mut PadInfo> {
        self.pads.iter_mut().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    /// `problem.pads[id] = {...}`: a new pad goes last, a known one keeps its place.
    pub(crate) fn set_pad(&mut self, id: &str, info: PadInfo) {
        match self.pad_mut(id) {
            Some(existing) => *existing = info,
            None => self.pads.push((id.to_owned(), info)),
        }
    }
}

/// A route: connected polylines per layer, via positions, its length and whether every segment
/// passed the region check.
#[derive(Clone, Debug)]
pub(crate) struct Route {
    pub segments: Vec<(String, Vec<[f64; 2]>)>,
    pub vias: Vec<[f64; 2]>,
    pub length: f64,
    pub checked: bool,
}

/// Why a route failed: `NoRoute` (Python's exception of that name), or another error.
#[derive(Clone, Debug)]
pub(crate) enum RouteError {
    NoRoute(String),
    Other(Error),
}

impl From<Error> for RouteError {
    fn from(error: Error) -> Self {
        RouteError::Other(error)
    }
}

impl RouteError {
    pub(crate) fn into_error(self) -> Error {
        match self {
            RouteError::NoRoute(message) => Error::Runtime(message),
            RouteError::Other(error) => error,
        }
    }
}

// ---- Regions ------------------------------------------------------------------------------------

/// A blocked area: points strictly inside an emulated GEOS buffer polygon (convex), or nearer than
/// `r` to a geometry.
enum Zone {
    Convex(Polygon<f64>, Rect<f64>),
    Near(Geometry<f64>, f64, Rect<f64>),
}

impl Zone {
    fn new(shape: &Shape, distance: f64, quad_segs: u32) -> Option<Zone> {
        let convex_line = matches!(&shape.geometry, Geometry::LineString(l) if l.0.len() <= 2);
        if shape.convex.is_some() || convex_line {
            let buffered = shape.buffer(distance, quad_segs);
            if let Geometry::Polygon(p) = &buffered.geometry {
                let ring = clean_ring(p.exterior());
                if is_convex_ring(&ring) {
                    let mut ring = ring;
                    if ring_area2(&ring) < 0.0 {
                        ring.reverse();
                    }
                    let polygon = Polygon::new(LineString::new(ring), vec![]);
                    let rect = polygon.bounding_rect()?;
                    return Some(Zone::Convex(polygon, rect));
                }
            }
        }
        let rect = shape.bounds()?;
        let rect = Rect::new(
            c(rect.min().x - distance, rect.min().y - distance),
            c(rect.max().x + distance, rect.max().y + distance),
        );
        Some(Zone::Near(shape.geometry.clone(), distance, rect))
    }

    fn rect(&self) -> Rect<f64> {
        match self {
            Zone::Convex(_, r) | Zone::Near(_, _, r) => *r,
        }
    }

    /// The point is in the zone's interior (`closed`: or on its boundary).
    fn blocks(&self, p: C, closed: bool) -> bool {
        match self {
            Zone::Convex(polygon, _) => match polygon.coordinate_position(&p) {
                CoordPos::Inside => true,
                CoordPos::OnBoundary => closed,
                CoordPos::Outside => false,
            },
            Zone::Near(geometry, r, _) => {
                let d = Euclidean.distance(&Point(p), geometry);
                d < *r || (closed && d == *r)
            }
        }
    }

    /// Does the segment from `a` to `b` reach deeper than [`EPS`] into the zone?
    fn penetrates(&self, a: C, b: C) -> bool {
        match self {
            Zone::Convex(polygon, _) => cyrus_beck(polygon.exterior(), a, b, EPS).is_some(),
            Zone::Near(geometry, r, _) => {
                let d = if a == b {
                    Euclidean.distance(&Point(a), geometry)
                } else {
                    Euclidean.distance(&Line::new(a, b), geometry)
                };
                d < r - EPS
            }
        }
    }
}

/// The open interval of t in [0, 1] where `a + t (b - a)` is more than `offset` inside a convex
/// counter-clockwise ring (a negative offset grows it); None when it is empty.  A point (a == b)
/// gives (0, 0) when inside.
fn cyrus_beck(ring: &LineString<f64>, a: C, b: C, offset: f64) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for w in ring.0.windows(2) {
        let (s, e) = (w[0], w[1]);
        let (dx, dy) = (e.x - s.x, e.y - s.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            continue;
        }
        let c0 = (dx * (a.y - s.y) - dy * (a.x - s.x)) / len - offset;
        let c1 = (dx * (b.y - a.y) - dy * (b.x - a.x)) / len;
        if c1 == 0.0 {
            if c0 <= 0.0 && !(a == b && offset < 0.0 && c0 == 0.0) {
                return None;
            }
            continue;
        }
        let t = -c0 / c1;
        if c1 > 0.0 {
            lo = lo.max(t);
        } else {
            hi = hi.min(t);
        }
    }
    if a == b {
        return Some((0.0, 0.0));
    }
    (lo < hi).then_some((lo, hi))
}

/// Own pad copper the track may run inside (`pad.buffer(-half)`), convex or not.
enum Inset {
    Convex(Polygon<f64>),
    Area(MultiPolygon<f64>, MultiPolygon<f64>),
}

impl Inset {
    fn new(shape: &Shape, half: f64) -> Option<Inset> {
        let inset = shape.buffer(-half, QUAD_SEGS);
        if inset.is_empty() {
            return None;
        }
        if let (Geometry::Polygon(p), true) = (&inset.geometry, inset.convex.is_some()) {
            let mut ring = clean_ring(p.exterior());
            if ring_area2(&ring) < 0.0 {
                ring.reverse();
            }
            return Some(Inset::Convex(Polygon::new(LineString::new(ring), vec![])));
        }
        let area = inset.area();
        let grown = geo_buffer(&Geometry::MultiPolygon(area.clone()), EPS, QUAD_SEGS);
        Some(Inset::Area(area, grown))
    }

    fn rect(&self) -> Option<Rect<f64>> {
        match self {
            Inset::Convex(p) => p.bounding_rect(),
            Inset::Area(a, _) => a.bounding_rect(),
        }
    }

    fn covers(&self, p: C) -> bool {
        match self {
            Inset::Convex(polygon) => polygon.coordinate_position(&p) != CoordPos::Outside,
            Inset::Area(area, _) => area.intersects(&p),
        }
    }

    /// The parts (t intervals) of the segment within [`EPS`] of the inset.
    fn intervals(&self, a: C, b: C) -> Vec<(f64, f64)> {
        match self {
            Inset::Convex(polygon) => cyrus_beck(polygon.exterior(), a, b, -EPS)
                .into_iter()
                .collect(),
            Inset::Area(_, grown) => {
                if a == b {
                    return if grown.intersects(&a) {
                        vec![(0.0, 0.0)]
                    } else {
                        vec![]
                    };
                }
                let line = MultiLineString::new(vec![LineString::new(vec![a, b])]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let t = |p: &C| ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy);
                grown
                    .clip(&line, false)
                    .0
                    .iter()
                    .filter_map(|piece| {
                        let ts: Vec<f64> = piece.coords().map(t).collect();
                        let lo = ts.iter().copied().fold(f64::INFINITY, f64::min);
                        let hi = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        (lo <= hi).then_some((lo.max(0.0), hi.min(1.0)))
                    })
                    .collect()
            }
        }
    }
}

/// `box(*region)`: shapely's box spans the coordinates whichever way round they are.
fn region_rect(region: [f64; 4]) -> [f64; 4] {
    [
        region[0].min(region[2]),
        region[1].min(region[3]),
        region[0].max(region[2]),
        region[1].max(region[3]),
    ]
}

/// `box(*region).intersection(outline.buffer(...))`, as a test: the box and the shrunk outline.
struct RegionArea {
    rect: [f64; 4],
    outline: Option<MultiPolygon<f64>>,
}

impl RegionArea {
    fn polygon(&self) -> MultiPolygon<f64> {
        let [x1, y1, x2, y2] = self.rect;
        let rect = MultiPolygon::new(vec![rect_polygon(x1, y1, x2, y2)]);
        match &self.outline {
            Some(outline) => rect.intersection(outline),
            None => rect,
        }
    }

    fn rect_covers(&self, p: C, margin: f64) -> bool {
        let [x1, y1, x2, y2] = self.rect;
        p.x >= x1 - margin && p.x <= x2 + margin && p.y >= y1 - margin && p.y <= y2 + margin
    }
}

/// `allowed_geometry[layer]`: where the centre line of a track of this net and width may run on
/// one layer (`_allowed_region`), kept as the parts the region is made of.
struct LayerRegion {
    area: RegionArea,
    /// The outline shrunk by EPS less, for `buffer(1e-6).covers(...)`
    area_eps: Option<MultiPolygon<f64>>,
    zones: Vec<Zone>,
    extra: Vec<Inset>,
}

impl LayerRegion {
    /// `allowed.buffer(1e-6).covers(segment a-b)` (a point when a == b).
    fn covers(&self, a: C, b: C) -> bool {
        // The parts within the own pads' copper are always allowed
        let mut inside: Vec<(f64, f64)> =
            self.extra.iter().flat_map(|e| e.intervals(a, b)).collect();
        if a == b {
            if !inside.is_empty() {
                return true;
            }
            return self.covers_piece(a, a);
        }
        inside.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(Ordering::Equal));
        let mut pieces: Vec<(f64, f64)> = Vec::new();
        let mut t = 0.0_f64;
        for (lo, hi) in inside {
            if lo > t {
                pieces.push((t, lo));
            }
            t = t.max(hi);
        }
        if t < 1.0 {
            pieces.push((t, 1.0));
        }
        let at = |t: f64| c(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
        pieces
            .iter()
            .all(|&(lo, hi)| self.covers_piece(at(lo), at(hi)))
    }

    fn covers_piece(&self, p: C, q: C) -> bool {
        if !self.area.rect_covers(p, EPS) || !self.area.rect_covers(q, EPS) {
            return false;
        }
        if let Some(outline) = &self.area_eps {
            let outside = if p == q {
                !outline.intersects(&p)
            } else {
                let line = MultiLineString::new(vec![LineString::new(vec![p, q])]);
                outline
                    .clip(&line, true)
                    .0
                    .iter()
                    .any(|piece| piece.0.windows(2).any(|w| w[0] != w[1]))
            };
            if outside {
                return false;
            }
        }
        !self.zones.iter().any(|zone| {
            let r = zone.rect();
            let (minx, maxx) = (p.x.min(q.x), p.x.max(q.x));
            let (miny, maxy) = (p.y.min(q.y), p.y.max(q.y));
            maxx >= r.min().x
                && minx <= r.max().x
                && maxy >= r.min().y
                && miny <= r.max().y
                && zone.penetrates(p, q)
        })
    }
}

/// The grid: x and y of every column and row (`x0 + (k + i_lo) * grid`, as numpy computes them).
struct Grid {
    xs: Vec<f64>,
    ys: Vec<f64>,
}

impl Grid {
    fn nx(&self) -> usize {
        self.xs.len()
    }

    fn ny(&self) -> usize {
        self.ys.len()
    }

    /// The columns (or rows) with values in [lo, hi].
    fn span(values: &[f64], lo: f64, hi: f64) -> std::ops::Range<usize> {
        let start = values.partition_point(|v| *v < lo);
        let end = values.partition_point(|v| *v <= hi);
        start..end.max(start)
    }
}

/// Classifies every grid point against an area: 0 outside, 1 on its boundary, 2 inside.  Rows are
/// scanned for crossings; points within rounding of the boundary are decided by the robust test.
fn raster(area: &MultiPolygon<f64>, grid: &Grid) -> Vec<u8> {
    let (nx, ny) = (grid.nx(), grid.ny());
    let mut out = vec![0u8; nx * ny];
    let Some(rect) = area.bounding_rect() else {
        return out;
    };
    let edges: Vec<(C, C)> = area
        .0
        .iter()
        .flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()))
        .flat_map(|ring| ring.0.windows(2).map(|w| (w[0], w[1])).collect::<Vec<_>>())
        .collect();
    let mut vertex_ys: Vec<f64> = edges.iter().map(|e| e.0.y).collect();
    vertex_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let tolerance = 1e-9;
    let robust = |p: C| match area.coordinate_position(&p) {
        CoordPos::Inside => 2,
        CoordPos::OnBoundary => 1,
        CoordPos::Outside => 0,
    };
    let columns = Grid::span(&grid.xs, rect.min().x - tolerance, rect.max().x + tolerance);
    for j in Grid::span(&grid.ys, rect.min().y - tolerance, rect.max().y + tolerance) {
        let y = grid.ys[j];
        let near_vertex = {
            let k = vertex_ys.partition_point(|v| *v < y - tolerance);
            k < vertex_ys.len() && vertex_ys[k] <= y + tolerance
        };
        if near_vertex {
            for i in columns.clone() {
                out[i * ny + j] = robust(c(grid.xs[i], y));
            }
            continue;
        }
        let mut crossings: Vec<f64> = edges
            .iter()
            .filter(|(a, b)| (a.y <= y && b.y > y) || (b.y <= y && a.y > y))
            .map(|(a, b)| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y))
            .collect();
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let mut k = 0;
        for i in columns.clone() {
            let x = grid.xs[i];
            while k < crossings.len() && crossings[k] < x - tolerance {
                k += 1;
            }
            if k < crossings.len() && crossings[k] <= x + tolerance {
                out[i * ny + j] = robust(c(x, y));
            } else if k % 2 == 1 {
                out[i * ny + j] = 2;
            }
        }
    }
    out
}

/// The cells of the grid within a rectangle.
fn cells(grid: &Grid, rect: Rect<f64>) -> impl Iterator<Item = (usize, usize)> {
    let columns = Grid::span(&grid.xs, rect.min().x, rect.max().x);
    let rows = Grid::span(&grid.ys, rect.min().y, rect.max().y);
    columns.flat_map(move |i| rows.clone().map(move |j| (i, j)))
}

fn intersects_rect(a: Rect<f64>, b: Rect<f64>) -> bool {
    a.max().x >= b.min().x
        && a.min().x <= b.max().x
        && a.max().y >= b.min().y
        && a.min().y <= b.max().y
}

/// `geometry.intersects(area)`.
fn shape_intersects(shape: &Shape, area: &MultiPolygon<f64>, area_rect: Option<Rect<f64>>) -> bool {
    match (shape.bounds(), area_rect) {
        (Some(s), Some(a)) if intersects_rect(s, a) => shape.geometry.intersects(area),
        _ => false,
    }
}

/// `_allowed_region(problem, layer, net, width, region, own, own_keys)` and its grid mask.
fn allowed_region(
    problem: &RouteProblem,
    layer: &str,
    net: &str,
    width: f64,
    region: [f64; 4],
    own: &[(String, Vec<Shape>)],
    own_keys: &[String],
) -> LayerRegion {
    let half = width / 2.0;
    let rect = region_rect(region);
    let shrink = |distance: f64| {
        problem
            .outline
            .as_ref()
            .map(|o| mitre_buffer(&o.area, -distance))
    };
    let area = RegionArea {
        rect,
        outline: shrink(problem.edge_clearance + half),
    };
    let area_eps = shrink(problem.edge_clearance + half - EPS);
    // Obstacles further than 5 mm from the area cannot reach it (`intersects(area.buffer(5))`)
    let reach = geo_buffer(&Geometry::MultiPolygon(area.polygon()), 5.0, QUAD_SEGS);
    let reach_rect = reach.bounding_rect();
    let mut zones = Vec::new();
    for obstacle in problem.obstacles_on(layer) {
        let same_net = !obstacle.net.is_empty() && obstacle.net == net;
        if same_net || (!obstacle.key.is_empty() && own_keys.contains(&obstacle.key)) {
            continue;
        }
        if !shape_intersects(&obstacle.geometry, &reach, reach_rect) {
            continue;
        }
        let distance = problem.clearance(net, &obstacle.net) + half;
        zones.extend(Zone::new(&obstacle.geometry, distance, 6));
    }
    for keepout in problem.track_keepouts_on(layer) {
        zones.extend(Zone::new(keepout, half, 6));
    }
    let extra = own
        .iter()
        .find(|(l, _)| l == layer)
        .map(|(_, shapes)| shapes.iter().filter_map(|g| Inset::new(g, half)).collect())
        .unwrap_or_default();
    LayerRegion {
        area,
        area_eps,
        zones,
        extra,
    }
}

/// The grid mask of a layer region: `contains_xy | intersects_xy`, i.e. covers.
fn region_mask(region: &LayerRegion, grid: &Grid) -> Vec<bool> {
    let (nx, ny) = (grid.nx(), grid.ny());
    let mut mask = vec![false; nx * ny];
    let [x1, y1, x2, y2] = region.area.rect;
    let classes = region.area.outline.as_ref().map(|o| raster(o, grid));
    for i in Grid::span(&grid.xs, x1, x2) {
        for j in Grid::span(&grid.ys, y1, y2) {
            mask[i * ny + j] = classes.as_ref().is_none_or(|k| k[i * ny + j] > 0);
        }
    }
    for zone in &region.zones {
        for (i, j) in cells(grid, zone.rect()) {
            if mask[i * ny + j] && zone.blocks(c(grid.xs[i], grid.ys[j]), false) {
                mask[i * ny + j] = false;
            }
        }
    }
    for inset in &region.extra {
        let Some(rect) = inset.rect() else { continue };
        for (i, j) in cells(grid, rect) {
            if !mask[i * ny + j] && inset.covers(c(grid.xs[i], grid.ys[j])) {
                mask[i * ny + j] = true;
            }
        }
    }
    mask
}

/// `_via_region(problem, net, via_diameter, layers, region, own_keys)` as a grid mask
/// (`contains_xy`: strictly inside).
fn via_mask(
    problem: &RouteProblem,
    net: &str,
    via_diameter: f64,
    region: [f64; 4],
    own_keys: &[String],
    grid: &Grid,
) -> Vec<bool> {
    let r = via_diameter / 2.0;
    let rect = region_rect(region);
    let area = RegionArea {
        rect,
        outline: problem.outline.as_ref().map(|o| {
            geo_buffer(
                &Geometry::MultiPolygon(o.area.clone()),
                -(problem.edge_clearance + r),
                QUAD_SEGS,
            )
        }),
    };
    let polygon = area.polygon();
    let polygon_rect = polygon.bounding_rect();
    let mut zones: Vec<Zone> = problem
        .via_keepouts
        .iter()
        .filter_map(|k| Zone::new(k, r, QUAD_SEGS))
        .collect();
    for layer in &problem.layers {
        for obstacle in problem.obstacles_on(layer) {
            let same_net = !obstacle.net.is_empty() && obstacle.net == net;
            if same_net || (!obstacle.key.is_empty() && own_keys.contains(&obstacle.key)) {
                continue;
            }
            if shape_intersects(&obstacle.geometry, &polygon, polygon_rect) {
                let distance = problem.clearance(net, &obstacle.net) + r;
                zones.extend(Zone::new(&obstacle.geometry, distance, 6));
            }
        }
    }
    let (nx, ny) = (grid.nx(), grid.ny());
    let mut mask = vec![false; nx * ny];
    let classes = area.outline.as_ref().map(|o| raster(o, grid));
    let [x1, y1, x2, y2] = rect;
    for i in Grid::span(&grid.xs, x1, x2) {
        for j in Grid::span(&grid.ys, y1, y2) {
            let (x, y) = (grid.xs[i], grid.ys[j]);
            let strictly = x > x1 && x < x2 && y > y1 && y < y2;
            mask[i * ny + j] = strictly && classes.as_ref().is_none_or(|k| k[i * ny + j] == 2);
        }
    }
    for zone in &zones {
        for (i, j) in cells(grid, zone.rect()) {
            if mask[i * ny + j] && zone.blocks(c(grid.xs[i], grid.ys[j]), true) {
                mask[i * ny + j] = false;
            }
        }
    }
    mask
}

// ---- Search -------------------------------------------------------------------------------------

/// What one route needs besides the problem.
pub(crate) struct RouteRequest<'a> {
    pub net: &'a str,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub start_layers: &'a [String],
    pub end_layers: &'a [String],
    pub width: f64,
    /// the layers the route may use (None: the start and end layers)
    pub layers: Option<&'a [String]>,
    /// (diameter, drill) to allow layer changes
    pub via: Option<(f64, f64)>,
    pub grid: f64,
    /// {layer: [pad copper]} of this net's pads at the ends, inside which the track may always run
    pub own_pads: &'a [(String, Vec<Shape>)],
    pub own_keys: &'a [String],
}

const VIA_COST: f64 = 4.0;
const MARGIN: f64 = 6.0;
const MAX_CELLS: i64 = 4_000_000;

fn py_list_repr(items: &[String]) -> String {
    crate::board_design::py_list_str(items)
}

/// `route(problem, net, start, end, start_layers, end_layers, width, layers, via, grid, ...)`:
/// one connection of the net, or NoRoute if nothing fits.
pub(crate) fn route(
    problem: &RouteProblem,
    request: &RouteRequest<'_>,
) -> Result<Route, RouteError> {
    let layers: Vec<String> = match request.layers.filter(|l| !l.is_empty()) {
        Some(layers) => layers.to_vec(),
        None => {
            let mut layers: Vec<String> = Vec::new();
            for l in request.start_layers.iter().chain(request.end_layers) {
                if !layers.contains(l) {
                    layers.push(l.clone());
                }
            }
            layers
        }
    };
    let usable: Vec<String> = layers
        .iter()
        .filter(|l| problem.layers.contains(l) || problem.layers.is_empty())
        .cloned()
        .collect();
    if usable.is_empty() {
        return Err(RouteError::NoRoute(format!(
            "none of the layers {} is a copper layer of this board",
            py_list_repr(&layers)
        )));
    }
    let mut last_error = None;
    for margin in [Some(MARGIN), Some(MARGIN * 3.0), None] {
        let region = search_region(problem, request.start, request.end, margin);
        match route_in(problem, request, &usable, region) {
            Ok(found) => return Ok(found),
            Err(RouteError::NoRoute(message)) => {
                last_error = Some(RouteError::NoRoute(message));
                if margin.is_none() {
                    break;
                }
            }
            Err(other) => return Err(other),
        }
    }
    Err(last_error.expect("an attempt was made"))
}

/// `_search_region(problem, start, end, margin)`.
fn search_region(
    problem: &RouteProblem,
    start: [f64; 2],
    end: [f64; 2],
    margin: Option<f64>,
) -> [f64; 4] {
    let bounds = problem
        .outline
        .as_ref()
        .and_then(|o| o.area.bounding_rect())
        .map(|r| [r.min().x, r.min().y, r.max().x, r.max().y]);
    if let (None, Some(bounds)) = (margin, bounds) {
        return bounds;
    }
    let margin = margin.unwrap_or(50.0);
    let (x1, x2) = if start[0] <= end[0] {
        (start[0], end[0])
    } else {
        (end[0], start[0])
    };
    let (y1, y2) = if start[1] <= end[1] {
        (start[1], end[1])
    } else {
        (end[1], start[1])
    };
    let region = [x1 - margin, y1 - margin, x2 + margin, y2 + margin];
    match bounds {
        // max()/min() of floats as Python takes them
        Some([bx1, by1, bx2, by2]) => [
            if bx1 > region[0] { bx1 } else { region[0] },
            if by1 > region[1] { by1 } else { region[1] },
            if bx2 < region[2] { bx2 } else { region[2] },
            if by2 < region[3] { by2 } else { region[3] },
        ],
        None => region,
    }
}

/// A heap entry ordered as Python compares `(f, g, (layer, i, j))`, smallest first.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    f: f64,
    g: f64,
    node: (usize, i64, i64),
}

impl Eq for Entry {}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        // reversed: BinaryHeap pops the largest
        other
            .f
            .partial_cmp(&self.f)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.g.partial_cmp(&self.g).unwrap_or(Ordering::Equal))
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn zero_division() -> Error {
    Error::Value("float division by zero".into())
}

/// `math.floor(x)` / `math.ceil(x)` of a float, with Python's errors for infinities and NaN.
fn to_int(x: f64) -> Result<i64, Error> {
    if x.is_nan() {
        return Err(Error::Value("cannot convert float NaN to integer".into()));
    }
    if x.is_infinite() {
        return Err(Error::Value(
            "cannot convert float infinity to integer".into(),
        ));
    }
    Ok(x as i64)
}

/// `_route_in(...)`: one search over one region.
fn route_in(
    problem: &RouteProblem,
    request: &RouteRequest<'_>,
    layers: &[String],
    region: [f64; 4],
) -> Result<Route, RouteError> {
    let mut grid = request.grid;
    let count = layers.len() as f64;
    if grid * grid == 0.0 {
        return Err(zero_division().into());
    }
    // A large area gets a coarser grid rather than an endless search
    let cells_needed = (region[2] - region[0]) * (region[3] - region[1]) / (grid * grid) * count;
    if cells_needed > MAX_CELLS as f64 {
        grid *= (cells_needed / MAX_CELLS as f64).sqrt();
    }
    let [x0, y0] = request.start;
    let i_lo = to_int(((region[0] - x0) / grid).floor())?;
    let i_hi = to_int(((region[2] - x0) / grid).ceil())?;
    let j_lo = to_int(((region[1] - y0) / grid).floor())?;
    let j_hi = to_int(((region[3] - y0) / grid).ceil())?;
    let (nx, ny) = (i_hi - i_lo + 1, j_hi - j_lo + 1);
    let total = nx.saturating_mul(ny).saturating_mul(layers.len() as i64);
    if total > MAX_CELLS {
        return Err(RouteError::NoRoute(format!(
            "the search area needs {total} grid cells; use a coarser grid"
        )));
    }
    let axis = |n: i64, lo: i64, origin: f64| -> Vec<f64> {
        (0..n.max(0))
            .map(|k| origin + ((k + lo) as f64) * grid)
            .collect()
    };
    let g = Grid {
        xs: axis(nx, i_lo, x0),
        ys: axis(ny, j_lo, y0),
    };
    let (nxu, nyu) = (g.nx(), g.ny());
    let (nxs, nys) = (nxu as i64, nyu as i64);
    let size = nxu * nyu;
    let mut regions = Vec::new();
    let mut masks: Vec<bool> = Vec::with_capacity(size * layers.len());
    for layer in layers {
        let allowed = allowed_region(
            problem,
            layer,
            request.net,
            request.width,
            region,
            request.own_pads,
            request.own_keys,
        );
        masks.extend(region_mask(&allowed, &g));
        regions.push(allowed);
    }
    let vias = match request.via {
        Some((diameter, _)) if layers.len() > 1 => Some(via_mask(
            problem,
            request.net,
            diameter,
            region,
            request.own_keys,
            &g,
        )),
        _ => None,
    };

    let (si, sj) = (-i_lo, -j_lo);
    let starts: Vec<usize> = request
        .start_layers
        .iter()
        .filter_map(|l| layers.iter().position(|x| x == l))
        .collect();
    if starts.is_empty() {
        return Err(RouteError::NoRoute(
            "the start pad is on none of the routing layers".into(),
        ));
    }
    let cell = |i: i64, j: i64| -> Result<usize, Error> {
        Ok(np_index(i, nxs, 1)? * nyu + np_index(j, nys, 2)?)
    };
    for &l in &starts {
        masks[l * size + cell(si, sj)?] = true;
    }

    // Goal: grid nodes near the end from which a straight run to the end point is legal
    let end = request.end;
    let radius = 2.max(to_int((1.5 * request.width / grid).ceil())? + 1);
    let ei = to_int(((end[0] - x0) / grid).round_ties_even())? - i_lo;
    let ej = to_int(((end[1] - y0) / grid).round_ties_even())? - j_lo;
    let mut goals = vec![false; size * layers.len()];
    let mut any_goal = false;
    for layer in request.end_layers {
        let Some(l) = layers.iter().position(|x| x == layer) else {
            continue;
        };
        for di in -radius..=radius {
            for dj in -radius..=radius {
                let (i, j) = (ei + di, ej + dj);
                if !(0 <= i && i < nxs && 0 <= j && j < nys) {
                    continue;
                }
                let index = l * size + (i as usize) * nyu + j as usize;
                if !masks[index] {
                    continue;
                }
                let p = c(g.xs[i as usize], g.ys[j as usize]);
                let target = c(end[0], end[1]);
                // LineString([p, end]), or Point(end) when p is the end
                let from = if py_dist([p.x, p.y], end) > 1e-9 {
                    p
                } else {
                    target
                };
                if regions[l].covers(from, target) {
                    goals[index] = true;
                    any_goal = true;
                }
            }
        }
    }
    if !any_goal {
        return Err(RouteError::NoRoute(
            "no legal approach to the end pad (it is boxed in by other copper or a keepout)".into(),
        ));
    }

    let heuristic = |i: i64, j: i64| {
        let (dx, dy) = ((i - ei).abs(), (j - ej).abs());
        let (mx, mn) = (dx.max(dy), dx.min(dy));
        ((mx - mn) as f64 + SQRT2 * mn as f64) * grid
    };
    let total_cells = size * layers.len();
    let mut cost = vec![f64::INFINITY; total_cells];
    // came and heading are dicts keyed by the node as pushed (a start index may be negative)
    let mut came: HashMap<(usize, i64, i64), (usize, i64, i64)> = HashMap::new();
    let mut heading: HashMap<(usize, i64, i64), usize> = HashMap::new();
    let mut heap = BinaryHeap::new();
    for &l in &starts {
        cost[l * size + cell(si, sj)?] = 0.0;
        heap.push(Entry {
            f: heuristic(si, sj),
            g: 0.0,
            node: (l, si, sj),
        });
    }
    let turn = 0.3 * grid;
    let in_range = |i: i64, j: i64| 0 <= i && i < nxs && 0 <= j && j < nys;
    let mut found = None;
    while let Some(Entry { g: gcost, node, .. }) = heap.pop() {
        let (l, i, j) = node;
        let here = l * size + cell(i, j)?;
        if gcost > cost[here] {
            continue;
        }
        // goals holds in-range nodes only
        if in_range(i, j) && goals[here] {
            found = Some(node);
            break;
        }
        let last = heading.get(&node).copied();
        for (k, &(di, dj)) in DIRECTIONS.iter().enumerate() {
            let (ni, nj) = (i + di, j + dj);
            if !in_range(ni, nj) || !masks[l * size + (ni as usize) * nyu + nj as usize] {
                continue;
            }
            if di != 0
                && dj != 0
                && !(masks[l * size + cell(i + di, j)?] && masks[l * size + cell(i, j + dj)?])
            {
                continue; // no corner cutting past an obstacle
            }
            let mut step = (if di != 0 && dj != 0 { SQRT2 } else { 1.0 }) * grid;
            if last.is_some_and(|h| h != k) {
                step += turn;
            }
            let next = (l, ni, nj);
            let index = l * size + (ni as usize) * nyu + nj as usize;
            let ng = gcost + step;
            if ng < cost[index] {
                cost[index] = ng;
                came.insert(next, node);
                heading.insert(next, k);
                heap.push(Entry {
                    f: ng + heuristic(ni, nj),
                    g: ng,
                    node: next,
                });
            }
        }
        if let Some(vias) = &vias {
            if vias[cell(i, j)?] {
                for nl in 0..layers.len() {
                    let index = nl * size + cell(i, j)?;
                    if nl == l || !masks[index] {
                        continue;
                    }
                    let next = (nl, i, j);
                    let ng = gcost + VIA_COST;
                    if ng < cost[index] {
                        cost[index] = ng;
                        came.insert(next, node);
                        heading.remove(&next);
                        heap.push(Entry {
                            f: ng + heuristic(i, j),
                            g: ng,
                            node: next,
                        });
                    }
                }
            }
        }
    }
    let Some(found) = found else {
        return Err(RouteError::NoRoute(
            "no path: other copper, keepouts or the board edge block every way through".into(),
        ));
    };
    let mut nodes = vec![found];
    while let Some(previous) = came.get(nodes.last().expect("not empty")) {
        nodes.push(*previous);
    }
    nodes.reverse();
    Ok(build_route(
        &nodes,
        &g,
        layers,
        request.start,
        end,
        &regions,
    ))
}

/// `_build_route(nodes, xs, ys, layers, start, end, allowed_geometry)`.
fn build_route(
    nodes: &[(usize, i64, i64)],
    grid: &Grid,
    layers: &[String],
    start: [f64; 2],
    end: [f64; 2],
    regions: &[LayerRegion],
) -> Route {
    let at = |values: &[f64], index: i64| {
        // xs[i] with numpy's negative indices
        let n = values.len() as i64;
        values[(if index < 0 { index + n } else { index }) as usize]
    };
    let mut segments: Vec<(usize, Vec<[f64; 2]>)> = Vec::new();
    let mut vias = Vec::new();
    let mut current = nodes[0].0;
    let mut points = vec![start];
    for &(l, i, j) in &nodes[1..] {
        let p = [
            crate::board_client::py_round(at(&grid.xs, i), 6),
            crate::board_client::py_round(at(&grid.ys, j), 6),
        ];
        if l != current {
            let last = *points.last().expect("not empty");
            segments.push((current, std::mem::replace(&mut points, vec![last])));
            vias.push(last);
            current = l;
            continue;
        }
        points.push(p);
    }
    if py_dist(*points.last().expect("not empty"), end) > 1e-9 {
        points.push(end);
    }
    segments.push((current, points));

    let mut route_segments = Vec::new();
    let mut length = 0.0;
    let mut checked = true;
    for (l, pts) in segments {
        let simplified = simplify(&pts);
        if simplified.len() < 2 {
            continue;
        }
        for w in simplified.windows(2) {
            length += py_dist(w[0], w[1]);
            if !regions[l].covers(c(w[0][0], w[0][1]), c(w[1][0], w[1][1])) {
                checked = false;
            }
        }
        route_segments.push((layers[l].clone(), simplified));
    }
    Route {
        segments: route_segments,
        vias,
        length: crate::board_client::py_round(length, 4),
        checked,
    }
}

/// `simplify(points)`: drops repeated points and points in the middle of straight runs.
pub(crate) fn simplify(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    for p in points {
        if out.last().is_some_and(|last| py_dist(*last, *p) < 1e-9) {
            continue;
        }
        out.push(*p);
    }
    let mut changed = true;
    while changed && out.len() > 2 {
        changed = false;
        for k in 1..out.len() - 1 {
            let ([x1, y1], [x2, y2], [x3, y3]) = (out[k - 1], out[k], out[k + 1]);
            if ((x2 - x1) * (y3 - y1) - (y2 - y1) * (x3 - x1)).abs() < 1e-9 {
                out.remove(k);
                changed = true;
                break;
            }
        }
    }
    out
}

/// A point of a cluster for [`spanning_order`]: (x, y, pad id).
pub(crate) type ClusterPoint = (f64, f64, String);

/// `spanning_order(clusters)`: pairs of points (one per cluster) that join all clusters with the
/// shortest total length (Prim), ties going to the first pair found in Python's iteration order.
pub(crate) fn spanning_order(clusters: &[Vec<ClusterPoint>]) -> Vec<(ClusterPoint, ClusterPoint)> {
    if clusters.len() < 2 {
        return Vec::new();
    }
    let mut joined = PyIntSet::new();
    joined.add(0);
    let mut pairs = Vec::new();
    while joined.len() < clusters.len() {
        let mut best: Option<(f64, &ClusterPoint, &ClusterPoint, usize)> = None;
        for a in joined.iter().collect::<Vec<_>>() {
            for (b, cluster) in clusters.iter().enumerate() {
                if joined.contains(b) {
                    continue;
                }
                for p in &clusters[a] {
                    for q in cluster {
                        let d = py_dist([p.0, p.1], [q.0, q.1]);
                        if best.is_none_or(|(bd, ..)| d < bd) {
                            best = Some((d, p, q, b));
                        }
                    }
                }
            }
        }
        let Some((_, p, q, b)) = best else { break };
        pairs.push((p.clone(), q.clone()));
        joined.add(b);
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_pythons() {
        assert_eq!(py_dist([0.0, 0.0], [3.0, 4.0]), 5.0);
        assert_eq!(py_hypot(1e-320, 0.0), 1e-320);
        // math.dist((0.1, 0.2), (0.4, 0.6)) in CPython 3.12
        assert_eq!(py_dist([0.1, 0.2], [0.4, 0.6]), 0.5);
        assert_eq!(py_mod(-1.0, 2.0 * PI), 2.0 * PI - 1.0);
    }

    #[test]
    fn sets_iterate_like_cpython() {
        let mut set = PyIntSet::new();
        for k in [0, 9, 3] {
            set.add(k);
        }
        // {0, 9, 3} in CPython: 9 & 7 == 1, so it comes before 3
        assert_eq!(set.iter().collect::<Vec<_>>(), [0, 9, 3]);
        for k in [1, 2, 4] {
            set.add(k);
        }
        // resized to 32 slots: ascending
        assert_eq!(set.iter().collect::<Vec<_>>(), [0, 1, 2, 3, 4, 9]);
    }

    #[test]
    fn buffers_follow_geos() {
        // Point(0, 0).buffer(1): 64 segments, clockwise from angle 0
        let disc = circle(c(0.0, 0.0), 1.0, 16).unwrap();
        assert_eq!(disc.exterior().0.len(), 65);
        assert_eq!(
            disc.exterior().0[1],
            c(0.9951847266721969, -0.0980171403295606)
        );
        // LineString([(0, 0), (3, 1)]).buffer(0.5).bounds
        let stadium = segment_buffer(c(0.0, 0.0), c(3.0, 1.0), 0.5, 16).unwrap();
        let rect = stadium.bounding_rect().unwrap();
        assert_eq!(stadium.exterior().0.len(), 67);
        assert!(
            (rect.min().x + 0.49981469436854764).abs() < 1e-15,
            "{rect:?}"
        );
        assert!(
            (rect.max().y - 1.4998146943685478).abs() < 1e-15,
            "{rect:?}"
        );
        // Point(0, 0).buffer(1) buffered again by 0.1: 1 + 0.1 cos(pi/64)
        let grown = Shape::polygon(disc).buffer(0.1, 16);
        let rect = grown.bounds().unwrap();
        assert!(
            (rect.max().x - 1.0998795456205173).abs() < 1e-15,
            "{rect:?}"
        );
        let square = Shape::polygon(rect_polygon(0.0, 0.0, 2.0, 1.0));
        let inset = square.buffer(-0.25, 16);
        assert_eq!(
            inset.bounds().unwrap(),
            Rect::new(c(0.25, 0.25), c(1.75, 0.75))
        );
        assert!(square.buffer(-0.6, 16).is_empty());
    }

    #[test]
    fn arcs_and_outlines() {
        let points = arc_points([1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], 4);
        assert_eq!(points.len(), 5);
        assert!((points[2][0]).abs() < 1e-12 && (points[2][1] - 1.0).abs() < 1e-12);
        // A 50 x 30 board drawn as four lines with a 10 x 10 cut-out inside
        let square = |x: f64, y: f64, w: f64, h: f64| {
            vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]
        };
        let outline =
            outline_from_lines(&[square(0.0, 0.0, 50.0, 30.0), square(10.0, 10.0, 10.0, 10.0)])
                .unwrap();
        assert!(outline.single);
        use geo::Area as _;
        assert!((outline.area.unsigned_area() - 1400.0).abs() < 1e-6);
        // Separate segments with a sub-micron gap still close
        let lines = vec![
            vec![[0.0, 0.0], [10.0, 0.0]],
            vec![[10.00002, 0.0], [10.0, 5.0]],
            vec![[10.0, 5.0], [0.0, 5.0], [0.0, 0.0]],
        ];
        let outline = outline_from_lines(&lines).unwrap();
        assert!((outline.area.unsigned_area() - 50.0).abs() < 1e-3);
        assert!(outline_from_lines(&[vec![[0.0, 0.0], [1.0, 1.0]]]).is_none());
    }

    #[test]
    fn simplify_and_spanning_order() {
        assert_eq!(
            simplify(&[[0.0, 0.0], [1.0, 0.0], [1.0, 0.0], [2.0, 0.0], [2.0, 1.0]]),
            [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0]]
        );
        let p = |x: f64, key: &str| (x, 0.0, key.to_owned());
        let pairs = spanning_order(&[vec![p(0.0, "a")], vec![p(5.0, "b")], vec![p(1.0, "c")]]);
        assert_eq!(pairs[0].1 .2, "c");
        assert_eq!(pairs[1].1 .2, "b");
    }
}
