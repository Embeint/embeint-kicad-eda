// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.geometry.application.pure.p001` (the inline bodies and
//! types of the kimath geometry headers direction45.h, geometry_predicates.h,
//! fracture_edge_index_utils.h, geometry_utils.h, half_line.h, line.h, the adopted
//! arc_chord_params.h, arc_construction.h and circle.h co-ports, and the type-only headers).
//!
//! It mirrors `port/oracle/shims/kimath_include_geometry_p001.cpp`: `int` values and `size_t`
//! values as JSON integers, `double` values as exact IEEE bits, one operation per request, and
//! the same refusals: `invalid_input` "undefined geometry header operation" where the pinned
//! evaluation is undefined (decided by the Rust port itself). The three-point `SHAPE_ARC`
//! constructor is shape_arc.cpp's, which the Rust port defers to its owner for every arc; the
//! oracle refuses the arcs whose construction is undefined, after that deferral point.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::arc_chord_params::ArcChordParams;
use kicad_math::arc_construction::ArcSolution;
use kicad_math::circle::Circle;
use kicad_math::corner_operations::{ChamferParams, ChamferResult, DogboneResult};
use kicad_math::direction45::{AngleType, CornerMode, Direction45, Direction45Error};
use kicad_math::fracture_edge_index_utils as fracture;
use kicad_math::geometry_utils::{self as utils, GeometryUtilsError, LeaderMode};
use kicad_math::half_line::HalfLine;
use kicad_math::intersection::IntersectionContact;
use kicad_math::line::Line;
use kicad_math::seg_header::Seg;
use kicad_math::shape::{Shape, ShapeError};
use kicad_math::shape_arc::ShapeArc;
use kicad_math::{approximation, corner_strategy, geometry_predicates as predicates, oval};
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Map, Value};

/// This unit.
pub const UNIT: &str = "libs.kimath.include.geometry.application.pure.p001";

const UNDEFINED: &str = "undefined geometry header operation";

enum Error {
    Invalid(String),
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}
impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Invalid(v)
    }
}
impl From<&str> for Error {
    fn from(v: &str) -> Self {
        Self::Invalid(v.into())
    }
}
impl From<Direction45Error> for Error {
    fn from(_: Direction45Error) -> Self {
        Self::Invalid(UNDEFINED.into())
    }
}
impl From<GeometryUtilsError> for Error {
    fn from(_: GeometryUtilsError) -> Self {
        Self::Invalid(UNDEFINED.into())
    }
}
impl From<predicates::PredicateError> for Error {
    fn from(_: predicates::PredicateError) -> Self {
        Self::Invalid(UNDEFINED.into())
    }
}
impl From<ShapeError> for Error {
    fn from(e: ShapeError) -> Self {
        match e {
            ShapeError::Undefined => Self::Invalid(UNDEFINED.into()),
            ShapeError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}

type R<T> = Result<T, Error>;

fn object(v: &Value) -> Result<&Map<String, Value>, String> {
    v.as_object().ok_or_else(|| "expected object".to_string())
}
fn keys(v: &Value, fields: &[&str]) -> Result<(), String> {
    let o = object(v)?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
/// Like [`keys`], with the optional fields allowed to be absent.
fn keys_optional(v: &Value, fields: &[&str], optional: &[&str]) -> Result<(), String> {
    let o = object(v)?;
    if fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    let present = fields.len() + optional.iter().filter(|f| o.contains_key(**f)).count();
    if o.len() != present {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn get<'a>(v: &'a Value, name: &str) -> Result<&'a Value, String> {
    v.as_object()
        .and_then(|o| o.get(name))
        .ok_or_else(|| format!("missing: {name}"))
}
fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str, String> {
    v.as_object()
        .and_then(|o| o.get(name))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn read_bool(v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| "expected boolean".into())
}
fn read_int(v: &Value) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| "integer out of range".into())
}
fn read_u64(v: &Value) -> Result<u64, String> {
    v.as_u64().ok_or_else(|| "integer out of range".into())
}
fn read_u32(v: &Value) -> Result<u32, String> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| "integer out of range".into())
}
fn decode_bits(v: &Value) -> Result<f64, String> {
    let s = v.as_str().ok_or("expected hex")?;
    if s.len() != 16 || !s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err("invalid hex".into());
    }
    u64::from_str_radix(s, 16)
        .map(f64::from_bits)
        .map_err(|_| "invalid hex".into())
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn pair(v: &Value) -> Result<(&Value, &Value), String> {
    match v.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok((x, y)),
        _ => Err("expected point".into()),
    }
}
fn read_point_i(v: &Value) -> Result<Vector2I, String> {
    let (x, y) = pair(v)?;
    Ok(Vector2::new(read_int(x)?, read_int(y)?))
}
fn read_point_d(v: &Value) -> Result<Vector2D, String> {
    let (x, y) = pair(v)?;
    Ok(Vector2::new(decode_bits(x)?, decode_bits(y)?))
}
fn point_i(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn point_d(p: Vector2D) -> Value {
    json!([bits(p.x), bits(p.y)])
}
fn seg_state(s: &Seg) -> Value {
    json!({"a": point_i(s.a), "b": point_i(s.b), "index": s.index()})
}
/// The template's type `T` (or `in_type` / `ret_type`): true for `double`.
fn read_floating(v: &Value, name: &str) -> Result<bool, String> {
    match text(v, name)? {
        "int" => Ok(false),
        "double" => Ok(true),
        _ => Err("unknown type".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// DIRECTION_45.

fn dir_state(d: &Direction45) -> Value {
    json!({"dir": d.dir(), "deg90": d.is_90deg()})
}

fn build_arc(spec: &Value) -> R<ShapeArc> {
    match text(spec, "ctor")? {
        "default" => {
            keys(spec, &["ctor"])?;
            Ok(ShapeArc::new())
        }
        "three_point" => {
            keys(spec, &["ctor", "start", "mid", "end", "width"])?;
            let start = read_point_i(get(spec, "start")?)?;
            let mid = read_point_i(get(spec, "mid")?)?;
            let end = read_point_i(get(spec, "end")?)?;
            let width = read_int(get(spec, "width")?)?;
            // The constructor is shape_arc.cpp's: every three-point arc defers to its owner.
            Ok(ShapeArc::from_three_points(start, mid, end, width)?)
        }
        _ => Err("unknown arc constructor".into()),
    }
}

/// The optional `a90` argument (default false).
fn a90(spec: &Value) -> Result<bool, String> {
    match spec.as_object().and_then(|o| o.get("a90")) {
        Some(v) => read_bool(v),
        None => Ok(false),
    }
}

fn build_direction(spec: &Value) -> R<Direction45> {
    match text(spec, "ctor")? {
        "default" => {
            keys(spec, &["ctor"])?;
            Ok(Direction45::default())
        }
        "directions" => {
            keys(spec, &["ctor", "value"])?;
            Ok(Direction45::new(read_int(get(spec, "value")?)?))
        }
        "vector" => {
            keys_optional(spec, &["ctor", "vec"], &["a90"])?;
            let vec = read_point_i(get(spec, "vec")?)?;
            let a90 = a90(spec)?;
            Ok(Direction45::from_vector(vec, a90)?)
        }
        "seg" => {
            keys_optional(spec, &["ctor", "a", "b"], &["a90"])?;
            let seg = Seg::new(
                read_point_i(get(spec, "a")?)?,
                read_point_i(get(spec, "b")?)?,
            );
            let a90 = a90(spec)?;
            Ok(Direction45::from_seg(&seg, a90)?)
        }
        "arc" => {
            keys_optional(spec, &["ctor", "arc"], &["a90"])?;
            let arc = build_arc(get(spec, "arc")?)?;
            let a90 = a90(spec)?;
            Ok(Direction45::from_arc(&arc, a90)?)
        }
        _ => Err("unknown direction constructor".into()),
    }
}

fn run_direction(input: &Value) -> R<Value> {
    let member = text(input, "member")?;
    let binary = matches!(member, "angle" | "is_obtuse" | "eq" | "ne");
    if binary {
        keys(input, &["op", "member", "dir", "other"])?;
    } else {
        keys(input, &["op", "member", "dir"])?;
    }

    let dir = build_direction(get(input, "dir")?)?;
    let mut result = Map::new();
    result.insert("state".into(), dir_state(&dir));

    if binary {
        let other = build_direction(get(input, "other")?)?;
        result.insert("other".into(), dir_state(&other));
        let value = match member {
            "angle" => json!(dir.angle(&other)? as i32),
            "is_obtuse" => json!(dir.is_obtuse(&other)?),
            "eq" => json!(dir == other),
            _ => json!(dir != other),
        };
        result.insert("value".into(), value);
        return Ok(Value::Object(result));
    }

    let value = match member {
        "state" => return Ok(Value::Object(result)),
        "format" => json!(dir.format()),
        "opposite" => dir_state(&dir.opposite()?),
        "is_diagonal" => json!(dir.is_diagonal()),
        "is_defined" => json!(dir.is_defined()),
        "right" => dir_state(&dir.right()?),
        "left" => dir_state(&dir.left()?),
        "to_vector" => point_i(dir.to_vector()),
        "mask" => json!(dir.mask()?),
        "all_directions_mask" => json!(Direction45::all_directions_mask()),
        _ => return Err("unknown member".into()),
    };
    result.insert("value".into(), value);
    Ok(Value::Object(result))
}

// ---------------------------------------------------------------------------------------------
// HALF_LINE and LINE.

fn build_seg(spec: &Value) -> Result<Seg, String> {
    keys(spec, &["a", "b", "index"])?;
    Ok(Seg::with_index(
        read_point_i(get(spec, "a")?)?,
        read_point_i(get(spec, "b")?)?,
        read_int(get(spec, "index")?)?,
    ))
}

/// A `HALF_LINE` or `LINE` constructor: from a `SEG`, or from two points.
fn build_ray<T>(spec: &Value, from_seg: fn(Seg) -> T, new: fn(Vector2I, Vector2I) -> T) -> R<T> {
    match text(spec, "ctor")? {
        "seg" => {
            keys(spec, &["ctor", "seg"])?;
            Ok(from_seg(build_seg(get(spec, "seg")?)?))
        }
        "points" => {
            keys(spec, &["ctor", "a", "b"])?;
            Ok(new(
                read_point_i(get(spec, "a")?)?,
                read_point_i(get(spec, "b")?)?,
            ))
        }
        _ => Err("unknown line constructor".into()),
    }
}

fn run_half_line(input: &Value) -> R<Value> {
    keys(input, &["op", "line", "other"])?;
    let line = build_ray(get(input, "line")?, HalfLine::from_seg, HalfLine::new)?;
    let other = build_ray(get(input, "other")?, HalfLine::from_seg, HalfLine::new)?;
    Ok(json!({
        "start": point_i(*line.get_start()),
        "contained_point": point_i(*line.get_contained_point()),
        "seg": seg_state(line.get_contained_seg()),
        "other_seg": seg_state(other.get_contained_seg()),
        "eq": line == other,
    }))
}

fn run_line(input: &Value) -> R<Value> {
    keys(input, &["op", "line", "other"])?;
    let line = build_ray(get(input, "line")?, Line::from_seg, Line::new)?;
    let other = build_ray(get(input, "other")?, Line::from_seg, Line::new)?;
    Ok(json!({
        "seg": seg_state(line.get_contained_seg()),
        "other_seg": seg_state(other.get_contained_seg()),
        "eq": line == other,
    }))
}

// ---------------------------------------------------------------------------------------------
// geometry_utils.h templates.

fn run_snapped(input: &Value, is45: bool) -> R<Value> {
    if is45 {
        keys_optional(input, &["op", "type", "vec"], &["only45"])?;
    } else {
        keys(input, &["op", "type", "vec"])?;
    }
    let floating = read_floating(input, "type")?;
    // The default argument only45 = false.
    let only45 = match input.get("only45").filter(|_| is45) {
        Some(v) => read_bool(v)?,
        None => false,
    };

    if floating {
        let vec = read_point_d(get(input, "vec")?)?;
        let snapped = if is45 {
            utils::get_vector_snapped45_d(vec, only45)
        } else {
            utils::get_vector_snapped90_d(vec)
        };
        return Ok(json!({"vec": point_d(snapped)}));
    }

    let vec = read_point_i(get(input, "vec")?)?;
    let snapped = if is45 {
        utils::get_vector_snapped45_i(vec, only45)?
    } else {
        utils::get_vector_snapped90_i(vec)?
    };
    Ok(json!({"vec": point_i(snapped)}))
}

fn run_clamped(input: &Value) -> R<Value> {
    keys_optional(input, &["op", "in", "ret", "coords"], &["padding"])?;
    let in_floating = read_floating(input, "in")?;
    let ret_floating = read_floating(input, "ret")?;
    // The default argument aPadding = 1u.
    let padding = match input.get("padding") {
        Some(v) => read_u32(v)?,
        None => 1,
    };

    if !in_floating && ret_floating {
        return Err("unsupported instantiation".into());
    }

    if !in_floating {
        let coords = read_point_i(get(input, "coords")?)?;
        return Ok(json!({"coords": point_i(utils::get_clamped_coords_i(coords, padding)?)}));
    }

    let coords = read_point_d(get(input, "coords")?)?;
    if ret_floating {
        return Ok(json!({"coords": point_d(utils::get_clamped_coords_d(coords, padding)?)}));
    }
    Ok(json!({"coords": point_i(utils::get_clamped_coords_d_to_i(coords, padding)?)}))
}

fn run_safe(input: &Value) -> R<Value> {
    keys(input, &["op", "type", "vec"])?;
    let value = if read_floating(input, "type")? {
        utils::is_vec2_safe_xy_d(read_point_d(get(input, "vec")?)?)
    } else {
        utils::is_vec2_safe_xy_i(read_point_i(get(input, "vec")?)?)
    };
    Ok(json!({ "value": value }))
}

// ---------------------------------------------------------------------------------------------
// geometry_predicates.h.

fn run_predicate(input: &Value, op: &str) -> R<Value> {
    if op == "orientation_sign" || op == "is_sliver_triangle" {
        keys(input, &["op", "a", "b", "c"])?;
        let a = read_point_i(get(input, "a")?)?;
        let b = read_point_i(get(input, "b")?)?;
        let c = read_point_i(get(input, "c")?)?;
        if op == "orientation_sign" {
            return Ok(json!({"value": predicates::orientation_sign(a, b, c)?}));
        }
        return Ok(json!({"value": predicates::is_sliver_triangle(a, b, c)}));
    }

    keys(input, &["op", "a", "b", "c", "p"])?;
    let a = read_point_i(get(input, "a")?)?;
    let b = read_point_i(get(input, "b")?)?;
    let c = read_point_i(get(input, "c")?)?;
    let p = read_point_i(get(input, "p")?)?;
    if op == "in_circle_legal_double" {
        return Ok(json!({"value": predicates::in_circle_legal_double(a, b, c, p)}));
    }
    Ok(json!({"value": predicates::in_circle_delaunay_legal(a, b, c, p)?}))
}

// ---------------------------------------------------------------------------------------------
// fracture_edge_index_utils.h.

fn run_fracture(input: &Value, op: &str) -> R<Value> {
    let u = |name: &str| -> Result<u64, String> { read_u64(get(input, name)?) };
    let i = |name: &str| -> Result<i32, String> { read_int(get(input, name)?) };
    let u32_of = |name: &str| -> Result<u32, String> { read_u32(get(input, name)?) };
    match op {
        "stripe_count_for" => {
            keys(input, &["op", "edge_count"])?;
            Ok(json!({"value": fracture::stripe_count_for(u("edge_count")?)}))
        }
        "map_y_to_stripe" => {
            keys(input, &["op", "y", "min_y", "max_y", "stripe_count"])?;
            Ok(json!({"value": fracture::map_y_to_stripe(
                i("y")?,
                i("min_y")?,
                i("max_y")?,
                u32_of("stripe_count")?
            )}))
        }
        "stripe_span" => {
            keys(input, &["op", "y1", "y2", "min_y", "max_y", "stripe_count"])?;
            let (first, second) = fracture::stripe_span(
                i("y1")?,
                i("y2")?,
                i("min_y")?,
                i("max_y")?,
                u32_of("stripe_count")?,
            );
            Ok(json!({ "value": [first, second] }))
        }
        "checked_add" => {
            keys(input, &["op", "total", "count", "element_size"])?;
            let mut total = u("total")?;
            let value = fracture::checked_add(&mut total, u("count")?, u("element_size")?);
            Ok(json!({"value": value, "total": total}))
        }
        "capacity_budget" => {
            keys(input, &["op", "edge_count", "edge_size"])?;
            Ok(json!({"value": fracture::capacity_budget(u("edge_count")?, u("edge_size")?)}))
        }
        "capacity_fits" => {
            keys(
                input,
                &[
                    "op",
                    "bucket_ids",
                    "long_ids",
                    "stripe_count",
                    "hole_count",
                    "node_size",
                    "budget",
                ],
            )?;
            Ok(json!({"value": fracture::capacity_fits(
                u("bucket_ids")?,
                u("long_ids")?,
                u("stripe_count")?,
                u("hole_count")?,
                u("node_size")?,
                u("budget")?
            )}))
        }
        "actual_capacity_fits" => {
            keys_optional(
                input,
                &[
                    "op",
                    "bucket_ids",
                    "long_ids",
                    "offsets",
                    "scratch",
                    "heads",
                    "nodes",
                    "node_size",
                    "budget",
                ],
                &["bytes"],
            )?;
            let args = [
                u("bucket_ids")?,
                u("long_ids")?,
                u("offsets")?,
                u("scratch")?,
                u("heads")?,
                u("nodes")?,
                u("node_size")?,
                u("budget")?,
            ];
            let [bucket_ids, long_ids, offsets, scratch, heads, nodes, node_size, budget] = args;
            if input.get("bytes").is_none() {
                let value = fracture::actual_capacity_fits(
                    bucket_ids, long_ids, offsets, scratch, heads, nodes, node_size, budget, None,
                );
                return Ok(json!({ "value": value }));
            }
            let mut bytes = u("bytes")?;
            let value = fracture::actual_capacity_fits(
                bucket_ids,
                long_ids,
                offsets,
                scratch,
                heads,
                nodes,
                node_size,
                budget,
                Some(&mut bytes),
            );
            Ok(json!({"value": value, "bytes": bytes}))
        }
        "should_index" => {
            keys(input, &["op", "estimated_visits", "hole_count"])?;
            Ok(json!({"value": fracture::should_index(u("estimated_visits")?, u("hole_count")?)}))
        }
        _ => Err("unknown operation".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// The adopted co-ports and the header types.

fn chord_state(p: &ArcChordParams) -> Value {
    json!({
        "valid": p.is_valid(),
        "chord_length": bits(p.get_chord_length()),
        "half_chord": bits(p.get_half_chord()),
        "chord_unit_vec": point_d(p.get_chord_unit_vec()),
        "normal_unit_vec": point_d(p.get_normal_unit_vec()),
        "sagitta": bits(p.get_sagitta()),
        "radius": bits(p.get_radius()),
        "center_offset": bits(p.get_center_offset()),
        "chord_midpoint": point_d(p.get_chord_midpoint()),
        "ux": bits(p.get_ux()),
        "uy": bits(p.get_uy()),
        "nx": bits(p.get_nx()),
        "ny": bits(p.get_ny()),
        "mid_x": bits(p.get_mid_x()),
        "mid_y": bits(p.get_mid_y()),
    })
}

fn run_arc_chord(input: &Value) -> R<Value> {
    keys(input, &["op", "computes"])?;
    let computes = match get(input, "computes")?.as_array() {
        Some(c) if c.len() <= 4 => c,
        _ => return Err("expected at most four computes".into()),
    };
    let mut params = ArcChordParams::default();
    let mut returns = Vec::new();
    for triple in computes {
        let [s, m, e] = match triple.as_array().map(Vec::as_slice) {
            Some([s, m, e]) => [s, m, e],
            _ => return Err("expected three points".into()),
        };
        let (s, m, e) = (read_point_i(s)?, read_point_i(m)?, read_point_i(e)?);
        returns.push(json!(params.compute(&s, &m, &e)));
    }
    Ok(json!({"returns": returns, "state": chord_state(&params)}))
}

fn build_circle(spec: &Value) -> R<Circle> {
    match text(spec, "ctor")? {
        "default" => {
            keys(spec, &["ctor"])?;
            Ok(Circle::new())
        }
        "center_radius" => {
            keys(spec, &["ctor", "center", "radius"])?;
            Ok(Circle::with_center_radius(
                read_point_i(get(spec, "center")?)?,
                read_int(get(spec, "radius")?)?,
            ))
        }
        _ => Err("unknown circle constructor".into()),
    }
}

fn run_circle_eq(input: &Value) -> R<Value> {
    keys(input, &["op", "a", "b"])?;
    let a = build_circle(get(input, "a")?)?;
    let b = build_circle(get(input, "b")?)?;
    // CIRCLE assigned = CIRCLE(); assigned = a; (the implicit copy assignment, `Copy` here).
    let assigned: Circle = a;
    Ok(json!({
        "a": {"radius": a.radius, "center": point_i(a.center)},
        "b": {"radius": b.radius, "center": point_i(b.center)},
        "eq": a == b,
        "assigned_eq": assigned == a,
    }))
}

fn optional_seg(s: &Option<Seg>) -> Value {
    s.as_ref().map_or(Value::Null, seg_state)
}

fn run_header_types(input: &Value) -> R<Value> {
    use approximation::ErrorLoc;
    use corner_strategy::CornerStrategy;
    keys(input, &["op"])?;

    let solution = ArcSolution::default();
    let contact = IntersectionContact::default();
    let chamfer_params = ChamferParams::default();
    let chamfer = ChamferResult::default();
    let dogbone = DogboneResult::default();

    Ok(json!({
        "error_loc": [ErrorLoc::ErrorOutside as i32, ErrorLoc::ErrorInside as i32],
        "corner_strategy": [
            CornerStrategy::AllowAcuteCorners as i32,
            CornerStrategy::ChamferAcuteCorners as i32,
            CornerStrategy::RoundAcuteCorners as i32,
            CornerStrategy::ChamferAllCorners as i32,
            CornerStrategy::RoundAllCorners as i32,
        ],
        "leader_mode": [LeaderMode::Direct as i32, LeaderMode::Deg45 as i32, LeaderMode::Deg90 as i32],
        "oval_key_points": [
            oval::OVAL_CENTER,
            oval::OVAL_CAP_TIPS,
            oval::OVAL_CAP_CENTERS,
            oval::OVAL_SIDE_MIDPOINTS,
            oval::OVAL_SIDE_ENDS,
            oval::OVAL_CARDINAL_EXTREMES,
            oval::OVAL_ALL_KEY_POINTS,
        ],
        "directions": [
            Direction45::N,
            Direction45::NE,
            Direction45::E,
            Direction45::SE,
            Direction45::S,
            Direction45::SW,
            Direction45::W,
            Direction45::NW,
            Direction45::LAST,
            Direction45::UNDEFINED,
        ],
        "corner_mode": [
            CornerMode::Mitered45 as i32,
            CornerMode::Rounded45 as i32,
            CornerMode::Mitered90 as i32,
            CornerMode::Rounded90 as i32,
        ],
        "angle_type": [
            AngleType::AngObtuse as i32,
            AngleType::AngRight as i32,
            AngleType::AngAcute as i32,
            AngleType::AngStraight as i32,
            AngleType::AngHalfFull as i32,
            AngleType::AngUndefined as i32,
        ],
        "fracture_constants": [
            u64::from(fracture::MAX_STRIPES),
            u64::from(fracture::MAX_BUCKET_SPAN),
            fracture::EDGE_SET_BUDGET_MULTIPLE,
            fracture::MIN_EDGE_VISITS,
            fracture::MIN_HOLE_COUNT,
        ],
        "arc_solution": {
            "valid": solution.valid,
            "start": point_i(solution.start),
            "mid": point_i(solution.mid),
            "end": point_i(solution.end),
            "center": point_d(solution.center),
            "radius": bits(solution.radius),
        },
        "intersection_contact": {"tangent": contact.tangent, "overlapping": contact.overlapping},
        "chamfer_params": [chamfer_params.chamfer_setback_a, chamfer_params.chamfer_setback_b],
        "chamfer_result": {
            "chamfer": seg_state(&chamfer.chamfer),
            "updated_seg_a": optional_seg(&chamfer.updated_seg_a),
            "updated_seg_b": optional_seg(&chamfer.updated_seg_b),
        },
        "dogbone_result": {
            "arc": [
                point_i(dogbone.arc.get_p0()),
                point_i(dogbone.arc.get_arc_mid()),
                point_i(dogbone.arc.get_p1()),
                dogbone.arc.get_width(),
            ],
            "updated_seg_a": optional_seg(&dogbone.updated_seg_a),
            "updated_seg_b": optional_seg(&dogbone.updated_seg_b),
            "small_arc_mouth": dogbone.small_arc_mouth,
        },
    }))
}

fn run(input: &Value) -> R<Value> {
    let op = text(input, "op")?;
    match op {
        "direction45" => run_direction(input),
        "half_line" => run_half_line(input),
        "line" => run_line(input),
        "snapped45" => run_snapped(input, true),
        "snapped90" => run_snapped(input, false),
        "clamped_coords" => run_clamped(input),
        "is_vec2_safe_xy" => run_safe(input),
        "orientation_sign"
        | "is_sliver_triangle"
        | "in_circle_legal_double"
        | "in_circle_delaunay_legal" => run_predicate(input, op),
        "stripe_count_for"
        | "map_y_to_stripe"
        | "stripe_span"
        | "checked_add"
        | "capacity_budget"
        | "capacity_fits"
        | "actual_capacity_fits"
        | "should_index" => run_fracture(input, op),
        "arc_chord_params" => run_arc_chord(input),
        "circle_eq" => run_circle_eq(input),
        "header_types" => run_header_types(input),
        _ => Err("unknown operation".into()),
    }
}

/// One request line to one reply value, in the shared shim protocol.
pub fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case":case,"error":{"kind":kind,"message":message},"unit":unit});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                unit,
                case,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    if let Some(value) = request.get("unit").filter(|v| v.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|v| v.is_string()) {
        case = value.clone();
    }
    let shaped = keys(&request, &["unit", "case", "input"]).is_ok();
    if !shaped
        || unit.as_str().is_none_or(str::is_empty)
        || case.as_str().is_none_or(str::is_empty)
        || !request["input"].is_object()
    {
        return failure(
            unit,
            case,
            "invalid_request",
            "expected unit, case, input object",
        );
    }
    if unit != UNIT {
        return failure(unit, case, "unknown_unit", "unit has no registered shim");
    }
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(Error::Unported {
            upstream,
            unit: owner,
        }) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
