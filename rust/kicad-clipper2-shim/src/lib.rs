// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `kicad-clipper2`, mirroring `port/oracle/shims/clipper2_core_engine.cpp`
//! request for request. Doubles travel as 16 hex digit bit patterns, points as `[x, y, z]`.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_clipper2::clipper_engine::{
    poly_path_base_count, Active, ActiveId, ClipType, Clipper64, ClipperD, HorzJoin, HorzSegment,
    IntersectNode, LocalMinima, OutPt, OutPtId, OutRec, OutRecId, PathType, PolyPathId, PolyTree,
    PolyTree64, PolyTreeD, StripeIndex, Vertex, VertexFlags, VertexId, ZCallbackD, ZCallbackKind,
};
use kicad_clipper2::*;
use kicad_math::{FmtFlags, StreamState};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;

pub use kicad_clipper2::UNIT;

const STREAM_BOUND: (i64, i64) = (-64, 512);
const MAX_TREE_OPS: usize = 1024;
/// The host budget for every tree the shim builds: the root plus one node per operation, and
/// far more points than one request line carries in practice.
const TREE_BUDGET: Budget = Budget {
    nodes: MAX_TREE_OPS + 1,
    points: 1 << 22,
};

enum E {
    Invalid(String),
    Clip(ClipperError),
}

impl From<ClipperError> for E {
    fn from(error: ClipperError) -> Self {
        Self::Clip(error)
    }
}

type R<T> = Result<T, E>;

fn invalid<T>(what: &str) -> R<T> {
    Err(E::Invalid(format!("invalid input: {what}")))
}

fn keys(v: &Value, fields: &[&str]) -> R<()> {
    match v.as_object() {
        Some(o) if o.len() == fields.len() && fields.iter().all(|f| o.contains_key(*f)) => Ok(()),
        _ => invalid("fields"),
    }
}

fn at<'a>(v: &'a Value, name: &str) -> R<&'a Value> {
    match v.as_object().and_then(|o| o.get(name)) {
        Some(value) => Ok(value),
        None => invalid(name),
    }
}

fn text<'a>(v: &'a Value, name: &str) -> R<&'a str> {
    match at(v, name)?.as_str() {
        Some(s) => Ok(s),
        None => invalid(name),
    }
}

fn boolean(v: &Value) -> R<bool> {
    match v.as_bool() {
        Some(b) => Ok(b),
        None => invalid("boolean"),
    }
}

fn i64_of(v: &Value) -> R<i64> {
    match v.as_i64() {
        Some(i) => Ok(i),
        None => invalid("int64"),
    }
}

fn u64_of(v: &Value) -> R<u64> {
    // Arbitrary-precision parsing keeps integer -0, which the C++ accepts as zero.
    match v
        .as_u64()
        .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
    {
        Some(u) => Ok(u),
        None => invalid("uint64"),
    }
}

fn i32_of(v: &Value) -> R<i32> {
    match i32::try_from(i64_of(v)?) {
        Ok(i) => Ok(i),
        Err(_) => invalid("int"),
    }
}

fn f64_of(v: &Value) -> R<f64> {
    let Some(s) = v.as_str() else {
        return invalid("double");
    };
    if s.len() != 16
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return invalid("double");
    }
    match u64::from_str_radix(s, 16) {
        Ok(bits) => Ok(f64::from_bits(bits)),
        Err(_) => invalid("double"),
    }
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

/// The wire form of a coordinate type.
trait Wire: Coord {
    fn read(v: &Value) -> R<Self>;
    fn write(self) -> Value;
}

impl Wire for i64 {
    fn read(v: &Value) -> R<Self> {
        i64_of(v)
    }
    fn write(self) -> Value {
        json!(self)
    }
}

impl Wire for f64 {
    fn read(v: &Value) -> R<Self> {
        f64_of(v)
    }
    fn write(self) -> Value {
        bits(self)
    }
}

fn read_point<T: Wire>(v: &Value) -> R<Point<T>> {
    match v.as_array() {
        Some(a) if a.len() == 3 => Ok(Point {
            x: T::read(&a[0])?,
            y: T::read(&a[1])?,
            z: i64_of(&a[2])?,
        }),
        _ => invalid("point"),
    }
}

fn write_point<T: Wire>(p: &Point<T>) -> Value {
    json!([p.x.write(), p.y.write(), p.z])
}

fn read_path<T: Wire>(v: &Value) -> R<Path<T>> {
    let Some(a) = v.as_array() else {
        return invalid("path");
    };
    a.iter().map(read_point::<T>).collect()
}

fn write_path<T: Wire>(p: &[Point<T>]) -> Value {
    Value::Array(p.iter().map(write_point).collect())
}

fn read_paths<T: Wire>(v: &Value) -> R<Paths<T>> {
    let Some(a) = v.as_array() else {
        return invalid("paths");
    };
    a.iter().map(read_path::<T>).collect()
}

fn write_paths<T: Wire>(p: &[Path<T>]) -> Value {
    Value::Array(p.iter().map(|path| write_path(path)).collect())
}

fn read_rect<T: Wire>(v: &Value) -> R<Rect<T>> {
    match v.as_array() {
        Some(a) if a.len() == 4 => Ok(Rect::new(
            T::read(&a[0])?,
            T::read(&a[1])?,
            T::read(&a[2])?,
            T::read(&a[3])?,
        )),
        _ => invalid("rect"),
    }
}

fn write_rect<T: Wire>(r: &Rect<T>) -> Value {
    json!([
        r.left.write(),
        r.top.write(),
        r.right.write(),
        r.bottom.write()
    ])
}

fn thrown(error: &Clipper2Exception) -> Value {
    json!({"thrown": error.what()})
}

fn pip_name(result: PointInPolygonResult) -> &'static str {
    match result {
        PointInPolygonResult::IsOn => "IsOn",
        PointInPolygonResult::IsInside => "IsInside",
        PointInPolygonResult::IsOutside => "IsOutside",
    }
}

const FLAG_NAMES: [&str; 15] = [
    "boolalpha",
    "dec",
    "fixed",
    "hex",
    "internal",
    "left",
    "oct",
    "right",
    "scientific",
    "showbase",
    "showpoint",
    "showpos",
    "skipws",
    "unitbuf",
    "uppercase",
];

fn stream_state(v: &Value) -> R<StreamState> {
    keys(v, &["flags", "precision", "width", "fill"])?;
    let Some(names) = v["flags"].as_array() else {
        return invalid("state");
    };
    let mut flags = FmtFlags::default();
    let mut seen = Vec::new();
    for name in names {
        let Some(name) = name.as_str() else {
            return invalid("state");
        };
        if seen.contains(&name) || !FLAG_NAMES.contains(&name) {
            return invalid("state");
        }
        seen.push(name);
        let flag = match name {
            "boolalpha" => &mut flags.boolalpha,
            "dec" => &mut flags.dec,
            "fixed" => &mut flags.fixed,
            "hex" => &mut flags.hex,
            "internal" => &mut flags.internal,
            "left" => &mut flags.left,
            "oct" => &mut flags.oct,
            "right" => &mut flags.right,
            "scientific" => &mut flags.scientific,
            "showbase" => &mut flags.showbase,
            "showpoint" => &mut flags.showpoint,
            "showpos" => &mut flags.showpos,
            "skipws" => &mut flags.skipws,
            "unitbuf" => &mut flags.unitbuf,
            _ => &mut flags.uppercase,
        };
        *flag = true;
    }
    let bounded = |name: &str| -> R<i64> {
        let value = i64_of(&v[name])?;
        if (STREAM_BOUND.0..=STREAM_BOUND.1).contains(&value) {
            Ok(value)
        } else {
            invalid("state")
        }
    };
    let precision = bounded("precision")?;
    let width = bounded("width")?;
    let Some(fill) = v["fill"]
        .as_str()
        .filter(|t| t.len() == 1 && t.bytes().all(|c| (0x20..0x7f).contains(&c)))
        .and_then(|t| t.chars().next())
    else {
        return invalid("state");
    };
    Ok(StreamState {
        flags,
        precision,
        width,
        fill,
    })
}

// --- typed core operations -----------------------------------------------------------------

fn run_typed<T: Wire>(op: &str, v: &Value) -> R<Value> {
    match op {
        "point_default" => {
            keys(v, &["op", "type"])?;
            Ok(write_point(&Point::<T>::default()))
        }
        "point_scale" => {
            keys(v, &["op", "type", "point", "scale"])?;
            let p = read_point::<T>(&v["point"])?;
            Ok(write_point(&p.scaled(f64_of(&v["scale"])?)?))
        }
        "point_unary" => {
            keys(v, &["op", "type", "point", "kind", "z"])?;
            let mut p = read_point::<T>(&v["point"])?;
            let kind = text(v, "kind")?;
            let z = i64_of(&v["z"])?;
            match kind {
                "negated" => Ok(write_point(&p.negated()?)),
                "negate" => {
                    p.negate()?;
                    Ok(write_point(&p))
                }
                "set_z" => {
                    p.set_z(z);
                    Ok(write_point(&p))
                }
                _ => invalid("kind"),
            }
        }
        "point_binary" => {
            keys(v, &["op", "type", "a", "b", "kind"])?;
            let a = read_point::<T>(&v["a"])?;
            let b = read_point::<T>(&v["b"])?;
            match text(v, "kind")? {
                "plus" => Ok(write_point(&a.plus(&b)?)),
                "minus" => Ok(write_point(&a.minus(&b)?)),
                "eq" => Ok(json!(a.point_eq(&b))),
                "ne" => Ok(json!(a.point_ne(&b))),
                _ => invalid("kind"),
            }
        }
        "midpoint" => {
            keys(v, &["op", "type", "a", "b"])?;
            let a = read_point::<T>(&v["a"])?;
            let b = read_point::<T>(&v["b"])?;
            Ok(write_point(&mid_point(&a, &b)?))
        }
        "rect_construct" => {
            keys(v, &["op", "type", "kind", "rect"])?;
            let kind = text(v, "kind")?;
            if kind == "values" {
                return Ok(write_rect(&read_rect::<T>(&v["rect"])?));
            }
            if !v["rect"].is_null() {
                return invalid("rect");
            }
            match kind {
                "default" => Ok(write_rect(&Rect::<T>::default())),
                "valid" => Ok(write_rect(&Rect::<T>::with_validity(true))),
                "invalid" => Ok(write_rect(&Rect::<T>::with_validity(false))),
                "invalid_rect" => Ok(write_rect(&Rect::<T>::invalid_rect())),
                "invalid_constant" => Ok(write_rect(&Rect::<T>::invalid_rect())),
                _ => invalid("kind"),
            }
        }
        "rect_query" => {
            keys(v, &["op", "type", "rect", "kind"])?;
            let r = read_rect::<T>(&v["rect"])?;
            match text(v, "kind")? {
                "is_valid" => Ok(json!(r.is_valid())),
                "is_empty" => Ok(json!(r.is_empty())),
                "width" => Ok(r.width()?.write()),
                "height" => Ok(r.height()?.write()),
                "mid_point" => Ok(write_point(&r.mid_point()?)),
                "as_path" => Ok(write_path(&r.as_path())),
                _ => invalid("kind"),
            }
        }
        "rect_set" => {
            keys(v, &["op", "type", "rect", "kind", "value"])?;
            let mut r = read_rect::<T>(&v["rect"])?;
            let kind = text(v, "kind")?;
            let value = T::read(&v["value"])?;
            match kind {
                "width" => r.set_width(value)?,
                "height" => r.set_height(value)?,
                _ => return invalid("kind"),
            }
            Ok(write_rect(&r))
        }
        "rect_contains" => {
            keys(v, &["op", "type", "rect", "kind", "other"])?;
            let r = read_rect::<T>(&v["rect"])?;
            match text(v, "kind")? {
                "point" => Ok(json!(r.contains_point(&read_point::<T>(&v["other"])?))),
                "rect" => Ok(json!(r.contains_rect(&read_rect::<T>(&v["other"])?))),
                _ => invalid("kind"),
            }
        }
        "rect_binary" => {
            keys(v, &["op", "type", "a", "b", "kind"])?;
            let mut a = read_rect::<T>(&v["a"])?;
            let b = read_rect::<T>(&v["b"])?;
            match text(v, "kind")? {
                "intersects" => Ok(json!(a.intersects(&b))),
                "eq" => Ok(json!(a.rect_eq(&b))),
                "plus" => Ok(write_rect(&a.plus(&b))),
                "add_assign" => {
                    a.add_assign(&b);
                    Ok(json!({"rect": write_rect(&a), "returns_self": true}))
                }
                _ => invalid("kind"),
            }
        }
        "rect_scale" => {
            keys(v, &["op", "type", "rect", "scale"])?;
            let mut r = read_rect::<T>(&v["rect"])?;
            r.scale(f64_of(&v["scale"])?)?;
            Ok(write_rect(&r))
        }
        "sqr" => {
            keys(v, &["op", "type", "value"])?;
            Ok(bits(sqr(T::read(&v["value"])?)))
        }
        "near_equal" => {
            keys(v, &["op", "type", "a", "b", "max"])?;
            let a = read_point::<T>(&v["a"])?;
            let b = read_point::<T>(&v["b"])?;
            Ok(json!(near_equal(&a, &b, f64_of(&v["max"])?)?))
        }
        "strip_near_equal" => {
            keys(v, &["op", "type", "shape", "value", "max", "closed"])?;
            let shape = text(v, "shape")?;
            let max = f64_of(&v["max"])?;
            let closed = boolean(&v["closed"])?;
            match shape {
                "path" => Ok(write_path(&strip_near_equal_path(
                    &read_path::<T>(&v["value"])?,
                    max,
                    closed,
                )?)),
                "paths" => Ok(write_paths(&strip_near_equal_paths(
                    &read_paths::<T>(&v["value"])?,
                    max,
                    closed,
                )?)),
                _ => invalid("shape"),
            }
        }
        "strip_duplicates" => {
            keys(v, &["op", "type", "shape", "value", "closed"])?;
            let shape = text(v, "shape")?;
            let closed = boolean(&v["closed"])?;
            match shape {
                "path" => {
                    let mut path = read_path::<T>(&v["value"])?;
                    strip_duplicates_path(&mut path, closed);
                    Ok(write_path(&path))
                }
                "paths" => {
                    let mut paths = read_paths::<T>(&v["value"])?;
                    strip_duplicates_paths(&mut paths, closed);
                    Ok(write_paths(&paths))
                }
                _ => invalid("shape"),
            }
        }
        "cross_product_sign" | "is_collinear" | "cross3" | "dot3" | "perpendic_dist" => {
            keys(v, &["op", "type", "points"])?;
            let pts = read_path::<T>(&v["points"])?;
            let [a, b, c] = pts.as_slice() else {
                return invalid("points");
            };
            Ok(match op {
                "cross_product_sign" => json!(cross_product_sign(a, b, c)?),
                "is_collinear" => json!(is_collinear(a, b, c)?),
                "cross3" => bits(cross_product3(a, b, c)?),
                "dot3" => bits(dot_product3(a, b, c)?),
                _ => bits(perpendic_dist_from_line_sqrd(a, b, c)?),
            })
        }
        "cross2" | "dot2" | "distance_sqr" => {
            keys(v, &["op", "type", "points"])?;
            let pts = read_path::<T>(&v["points"])?;
            let [a, b] = pts.as_slice() else {
                return invalid("points");
            };
            Ok(bits(match op {
                "cross2" => cross_product2(a, b)?,
                "dot2" => dot_product2(a, b)?,
                _ => distance_sqr(a, b)?,
            }))
        }
        "area" | "is_positive" => {
            keys(v, &["op", "type", "shape", "value"])?;
            match text(v, "shape")? {
                "path" => {
                    let path = read_path::<T>(&v["value"])?;
                    if op == "is_positive" {
                        Ok(json!(is_positive(&path)?))
                    } else {
                        Ok(bits(area(&path)?))
                    }
                }
                "paths" if op == "area" => Ok(bits(area_paths(&read_paths::<T>(&v["value"])?)?)),
                _ => invalid("shape"),
            }
        }
        "line_intersect" => {
            keys(v, &["op", "type", "points", "ip"])?;
            let pts = read_path::<T>(&v["points"])?;
            let [a1, a2, b1, b2] = pts.as_slice() else {
                return invalid("points");
            };
            let mut ip = read_point::<T>(&v["ip"])?;
            let result = get_line_intersect_pt(a1, a2, b1, b2, &mut ip)?;
            Ok(json!({"result": result, "ip": write_point(&ip)}))
        }
        "translate_point" => {
            keys(v, &["op", "type", "point", "dx", "dy"])?;
            let p = read_point::<T>(&v["point"])?;
            Ok(write_point(&translate_point(
                &p,
                f64_of(&v["dx"])?,
                f64_of(&v["dy"])?,
            )?))
        }
        "reflect_point" => {
            keys(v, &["op", "type", "point", "pivot"])?;
            let p = read_point::<T>(&v["point"])?;
            let pivot = read_point::<T>(&v["pivot"])?;
            Ok(write_point(&reflect_point(&p, &pivot)?))
        }
        "get_sign" => {
            keys(v, &["op", "type", "value"])?;
            Ok(json!(get_sign(T::read(&v["value"])?)))
        }
        "closest_point" => {
            keys(v, &["op", "type", "points"])?;
            let pts = read_path::<T>(&v["points"])?;
            let [off, s1, s2] = pts.as_slice() else {
                return invalid("points");
            };
            Ok(write_point(&get_closest_point_on_segment(off, s1, s2)?))
        }
        "point_in_polygon" => {
            keys(v, &["op", "type", "point", "polygon"])?;
            let p = read_point::<T>(&v["point"])?;
            let polygon = read_path::<T>(&v["polygon"])?;
            Ok(json!(pip_name(point_in_polygon(&p, &polygon)?)))
        }
        "stream" => {
            keys(v, &["op", "type", "shape", "value", "state"])?;
            let shape = text(v, "shape")?;
            let mut state = if v["state"].is_null() {
                StreamState::default()
            } else {
                stream_state(&v["state"])?
            };
            let text = match shape {
                "point" => read_point::<T>(&v["value"])?.insert(&mut state)?,
                "rect" => read_rect::<T>(&v["value"])?.insert(&mut state)?,
                "path" => insert_path(&read_path::<T>(&v["value"])?, &mut state)?,
                "paths" => insert_paths(&read_paths::<T>(&v["value"])?, &mut state)?,
                _ => return invalid("shape"),
            };
            Ok(json!({"text": text, "width_after": state.width}))
        }
        _ => invalid("op"),
    }
}

// --- conversions between coordinate types ------------------------------------------------------

fn run_mixed<T1: Wire, T2: Wire>(op: &str, v: &Value) -> R<Value> {
    match op {
        "point_construct" => {
            keys(v, &["op", "to", "from", "x", "y", "z"])?;
            let x = T2::read(&v["x"])?;
            let y = T2::read(&v["y"])?;
            let z = if v["z"].is_null() {
                0
            } else {
                i64_of(&v["z"])?
            };
            Ok(write_point(&Point::<T1>::new(x, y, z)?))
        }
        "point_convert" => {
            keys(v, &["op", "to", "from", "point", "z"])?;
            let p = read_point::<T2>(&v["point"])?;
            if v["z"].is_null() {
                Ok(write_point(&Point::<T1>::from_point(&p)?))
            } else {
                Ok(write_point(&Point::<T1>::from_point_with_z(
                    &p,
                    i64_of(&v["z"])?,
                )?))
            }
        }
        "point_init" => {
            keys(v, &["op", "to", "from", "point", "x", "y", "z"])?;
            let mut p = read_point::<T1>(&v["point"])?;
            let x = T2::read(&v["x"])?;
            let y = T2::read(&v["y"])?;
            // `Init(x_, y_)` uses its default argument `z_ = 0`.
            let z = if v["z"].is_null() {
                0
            } else {
                i64_of(&v["z"])?
            };
            p.init(x, y, z)?;
            Ok(write_point(&p))
        }
        "append_point" => {
            keys(v, &["op", "to", "from", "path", "point"])?;
            let mut path = read_path::<T1>(&v["path"])?;
            let p = read_point::<T2>(&v["point"])?;
            append_point(&mut path, &p)?;
            Ok(json!({"path": write_path(&path), "returns_self": true}))
        }
        "scale_rect" => {
            keys(v, &["op", "to", "from", "rect", "scale"])?;
            let r = read_rect::<T2>(&v["rect"])?;
            Ok(write_rect(&scale_rect::<T1, T2>(&r, f64_of(&v["scale"])?)?))
        }
        "get_bounds" => {
            keys(v, &["op", "to", "from", "shape", "value"])?;
            match text(v, "shape")? {
                "path" => Ok(write_rect(&get_bounds_path::<T1, T2>(&read_path::<T2>(
                    &v["value"],
                )?)?)),
                "paths" => Ok(write_rect(&get_bounds_paths::<T1, T2>(&read_paths::<T2>(
                    &v["value"],
                )?)?)),
                _ => invalid("shape"),
            }
        }
        "scale_path" | "scale_paths" => {
            keys(
                v,
                &[
                    "op",
                    "to",
                    "from",
                    "value",
                    "scale_x",
                    "scale_y",
                    "error_code",
                ],
            )?;
            let sx = f64_of(&v["scale_x"])?;
            let uniform = v["scale_y"].is_null();
            let sy = if uniform { sx } else { f64_of(&v["scale_y"])? };
            let mut error_code = i32_of(&v["error_code"])?;
            let result = if op == "scale_path" {
                let path = read_path::<T2>(&v["value"])?;
                let scaled = if uniform {
                    scale_path::<T1, T2>(&path, sx, &mut error_code)
                } else {
                    scale_path_xy::<T1, T2>(&path, sx, sy, &mut error_code)
                };
                scaled.map(|p| write_path(&p))
            } else {
                let paths = read_paths::<T2>(&v["value"])?;
                let scaled = if uniform {
                    scale_paths::<T1, T2>(&paths, sx, &mut error_code)
                } else {
                    scale_paths_xy::<T1, T2>(&paths, sx, sy, &mut error_code)
                };
                scaled.map(|p| write_paths(&p))
            };
            match result {
                Ok(value) => Ok(json!({"value": value, "error_code": error_code})),
                Err(ClipperError::Exception(e)) => {
                    Ok(json!({"thrown": e.what(), "error_code": error_code}))
                }
                Err(e) => Err(e.into()),
            }
        }
        "transform_path" => {
            keys(v, &["op", "to", "from", "value"])?;
            Ok(write_path(&transform_path::<T1, T2>(&read_path::<T2>(
                &v["value"],
            )?)?))
        }
        "transform_paths" => {
            keys(v, &["op", "to", "from", "value"])?;
            Ok(write_paths(&transform_paths::<T1, T2>(&read_paths::<T2>(
                &v["value"],
            )?)?))
        }
        _ => invalid("op"),
    }
}

fn dispatch_from<T1: Wire>(op: &str, v: &Value) -> R<Value> {
    match text(v, "from")? {
        "i64" => run_mixed::<T1, i64>(op, v),
        "f64" => run_mixed::<T1, f64>(op, v),
        _ => invalid("from"),
    }
}

// --- engine ------------------------------------------------------------------------------------

fn vertex_json(v: &Vertex) -> Value {
    json!({"pt": write_point(&v.pt), "next_null": v.next.is_none(), "prev_null": v.prev.is_none(),
           "flags": v.flags.0})
}

fn join_name(join: kicad_clipper2::clipper_engine::JoinWith) -> &'static str {
    use kicad_clipper2::clipper_engine::JoinWith;
    match join {
        JoinWith::NoJoin => "NoJoin",
        JoinWith::Left => "Left",
        JoinWith::Right => "Right",
    }
}

fn engine_defaults() -> Value {
    let v = Vertex::default();
    let rec = OutRec::default();
    let e = Active::default();
    let node = IntersectNode::default();
    let seg = HorzSegment::default();
    let join = HorzJoin::default();
    let index = StripeIndex::default();
    json!({
        "vertex": vertex_json(&v),
        "outrec": {"idx": rec.idx, "owner_null": rec.owner.is_none(),
                   "front_edge_null": rec.front_edge.is_none(), "back_edge_null": rec.back_edge.is_none(),
                   "pts_null": rec.pts.is_none(), "polypath_null": rec.polypath.is_none(),
                   "splits_null": rec.splits.is_none(), "recursive_split_null": rec.recursive_split.is_none(),
                   "bounds": write_rect(&rec.bounds), "path": write_path(&rec.path),
                   "is_open": rec.is_open, "pip_index_null": rec.pip_index.is_none()},
        "active": {"bot": write_point(&e.bot), "top": write_point(&e.top), "curr_x": e.curr_x,
                   "dx": bits(e.dx), "wind_dx": e.wind_dx, "wind_cnt": e.wind_cnt, "wind_cnt2": e.wind_cnt2,
                   "outrec_null": e.outrec.is_none(),
                   "links_null": e.prev_in_ael.is_none() && e.next_in_ael.is_none() && e.prev_in_sel.is_none()
                       && e.next_in_sel.is_none() && e.jump.is_none(),
                   "vertex_top_null": e.vertex_top.is_none(), "local_min_null": e.local_min.is_none(),
                   "is_left_bound": e.is_left_bound, "join_with": join_name(e.join_with)},
        "intersect_node": {"pt": write_point(&node.pt), "edge1_null": node.edge1.is_none(),
                           "edge2_null": node.edge2.is_none()},
        "horz_segment": {"left_op_null": seg.left_op.is_none(), "right_op_null": seg.right_op.is_none(),
                         "left_to_right": seg.left_to_right},
        "horz_join": {"op1_null": join.op1.is_none(), "op2_null": join.op2.is_none()},
        "stripe_index": {"edge_count": index.edge_count(), "empty": index.empty()},
    })
}

fn clip_type(v: &Value) -> R<ClipType> {
    Ok(match v.as_str() {
        Some("NoClip") => ClipType::NoClip,
        Some("Intersection") => ClipType::Intersection,
        Some("Union") => ClipType::Union,
        Some("Difference") => ClipType::Difference,
        Some("Xor") => ClipType::Xor,
        _ => return invalid("clip_type"),
    })
}

fn fill_rule(v: &Value) -> R<FillRule> {
    Ok(match v.as_str() {
        Some("EvenOdd") => FillRule::EvenOdd,
        Some("NonZero") => FillRule::NonZero,
        Some("Positive") => FillRule::Positive,
        Some("Negative") => FillRule::Negative,
        _ => return invalid("fill_rule"),
    })
}

/// `ClipperD` construction: `precision` null is the default argument.
fn make_d(precision: &Value) -> R<Result<ClipperD, Clipper2Exception>> {
    let result = if precision.is_null() {
        ClipperD::with_default_precision()
    } else {
        ClipperD::new(i32_of(precision)?)
    };
    match result {
        Ok(clipper) => Ok(Ok(clipper)),
        Err(ClipperError::Exception(e)) => Ok(Err(e)),
        Err(e) => Err(e.into()),
    }
}

/// A recording `ZCallbackD`: stores the descaled arguments, writes `z` and a decoy `x`.
fn recorder(z: i64, calls: Rc<RefCell<Vec<Value>>>) -> ZCallbackD {
    Box::new(move |a, b, c, d, pt| {
        calls.borrow_mut().push(json!([
            write_point(a),
            write_point(b),
            write_point(c),
            write_point(d),
            write_point(pt)
        ]));
        pt.z = z;
        pt.x = -pt.x;
    })
}

fn settings(base: &kicad_clipper2::clipper_engine::ClipperBase) -> Value {
    json!({"error_code": base.error_code(), "preserve_collinear": base.preserve_collinear(),
           "reverse_solution": base.reverse_solution(), "pip_index_threshold": base.pip_index_threshold(),
           "default_z": base.default_z})
}

fn apply_settings(base: &mut kicad_clipper2::clipper_engine::ClipperBase, v: &Value) -> R<()> {
    if !v["preserve_collinear"].is_null() {
        base.set_preserve_collinear(boolean(&v["preserve_collinear"])?);
    }
    if !v["reverse_solution"].is_null() {
        base.set_reverse_solution(boolean(&v["reverse_solution"])?);
    }
    if !v["pip_index_threshold"].is_null() {
        base.set_pip_index_threshold(u64_of(&v["pip_index_threshold"])?);
    }
    Ok(())
}

/// `PolyPath` operation sequences; nodes are numbered by creation (root 0).
trait TreeOps: Sized {
    const D: bool;
    fn make(root: &Value, root_type: &str) -> R<PolyTree<Self::T>>;
    type T: Wire;
    fn add(
        tree: &mut PolyTree<Self::T>,
        node: PolyPathId,
        path_type: &str,
        path: &Value,
    ) -> R<Result<PolyPathId, Clipper2Exception>>;
    fn set_scale(tree: &mut PolyTree<Self::T>, node: PolyPathId, scale: f64) -> R<()>;
    fn scale(tree: &PolyTree<Self::T>, node: PolyPathId) -> R<Option<f64>>;
}

fn exception_or<T>(result: Result<T, ClipperError>) -> R<Result<T, Clipper2Exception>> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(ClipperError::Exception(e)) => Ok(Err(e)),
        Err(e) => Err(e.into()),
    }
}

struct Tree64;
impl TreeOps for Tree64 {
    const D: bool = false;
    type T = i64;
    fn make(root: &Value, root_type: &str) -> R<PolyTree64> {
        if root.is_null() {
            return Ok(PolyTree64::new(TREE_BUDGET)?);
        }
        match root_type {
            "i64" => Ok(PolyTree64::with_polygon(
                TREE_BUDGET,
                read_path::<i64>(root)?,
            )?),
            _ => invalid("root_type"),
        }
    }
    fn add(
        tree: &mut PolyTree64,
        node: PolyPathId,
        path_type: &str,
        path: &Value,
    ) -> R<Result<PolyPathId, Clipper2Exception>> {
        match path_type {
            "i64" => {
                let path = read_path::<i64>(path)?;
                exception_or(tree.add_child(node, &path))
            }
            _ => invalid("path_type"),
        }
    }
    fn set_scale(_: &mut PolyTree64, _: PolyPathId, _: f64) -> R<()> {
        invalid("op")
    }
    fn scale(_: &PolyTree64, _: PolyPathId) -> R<Option<f64>> {
        Ok(None)
    }
}

struct TreeD;
impl TreeOps for TreeD {
    const D: bool = true;
    type T = f64;
    fn make(root: &Value, root_type: &str) -> R<PolyTreeD> {
        if root.is_null() {
            return Ok(PolyTreeD::new(TREE_BUDGET)?);
        }
        match root_type {
            "i64" => Ok(PolyTreeD::with_polygon64(
                TREE_BUDGET,
                &read_path::<i64>(root)?,
            )?),
            "f64" => Ok(PolyTreeD::with_polygon(
                TREE_BUDGET,
                read_path::<f64>(root)?,
            )?),
            _ => invalid("root_type"),
        }
    }
    fn add(
        tree: &mut PolyTreeD,
        node: PolyPathId,
        path_type: &str,
        path: &Value,
    ) -> R<Result<PolyPathId, Clipper2Exception>> {
        match path_type {
            "i64" => {
                let path = read_path::<i64>(path)?;
                exception_or(tree.add_child(node, &path))
            }
            "f64" => {
                let path = read_path::<f64>(path)?;
                exception_or(tree.add_child_d(node, &path))
            }
            _ => invalid("path_type"),
        }
    }
    fn set_scale(tree: &mut PolyTreeD, node: PolyPathId, scale: f64) -> R<()> {
        Ok(tree.set_scale(node, scale)?)
    }
    fn scale(tree: &PolyTreeD, node: PolyPathId) -> R<Option<f64>> {
        Ok(Some(tree.scale(node)?))
    }
}

/// The creation number of a live node. Destroyed nodes' slots are reused by later children,
/// as freed addresses are in the C++, so the lookup considers live nodes only.
fn number_of(ids: &[PolyPathId], live: &[bool], id: PolyPathId) -> R<usize> {
    ids.iter()
        .zip(live)
        .position(|(candidate, alive)| *alive && *candidate == id)
        .ok_or_else(|| E::Invalid("unknown PolyPath".into()))
}

fn run_tree<K: TreeOps>(v: &Value) -> R<Value> {
    let Some(ops) = at(v, "ops")?
        .as_array()
        .filter(|ops| ops.len() <= MAX_TREE_OPS)
    else {
        return invalid("ops");
    };
    let root_type = text(v, "root_type")?;
    let mut tree = K::make(&v["root"], root_type)?;
    // Nodes are numbered by creation; `ids[n]` is node n's handle.
    let mut ids = vec![PolyTree::<K::T>::ROOT];
    let mut parents: Vec<Option<usize>> = vec![None];
    let mut live = vec![true];
    let mut events = Vec::new();
    let node_of = |index: &Value, ids: &[PolyPathId], live: &[bool]| -> R<(usize, PolyPathId)> {
        let index = u64_of(index)?;
        let Some(index) = usize::try_from(index).ok().filter(|i| *i < live.len()) else {
            return invalid("node");
        };
        if !live[index] {
            return Err(E::Clip(ClipperError::Undefined("destroyed PolyPath")));
        }
        Ok((index, ids[index]))
    };
    for op in ops {
        match text(op, "op")? {
            "add" => {
                keys(op, &["op", "node", "path_type", "path"])?;
                let (index, parent) = node_of(&op["node"], &ids, &live)?;
                let path_type = text(op, "path_type")?;
                match K::add(&mut tree, parent, path_type, &op["path"])? {
                    Ok(child) => {
                        ids.push(child);
                        parents.push(Some(index));
                        live.push(true);
                        events.push(json!({"added": ids.len() - 1}));
                    }
                    Err(e) => events.push(thrown(&e)),
                }
            }
            "clear" => {
                keys(op, &["op", "node"])?;
                let (index, node) = node_of(&op["node"], &ids, &live)?;
                tree.clear(node)?;
                for i in 1..live.len() {
                    let mut p = parents[i];
                    while let Some(ancestor) = p {
                        if ancestor == index {
                            live[i] = false;
                            break;
                        }
                        p = parents[ancestor];
                    }
                }
                events.push(json!({"cleared": index}));
            }
            "child" => {
                keys(op, &["op", "node", "index", "subscript"])?;
                let (_, node) = node_of(&op["node"], &ids, &live)?;
                let index = u64_of(&op["index"])?;
                let _subscript = boolean(&op["subscript"])?;
                let Ok(index) = usize::try_from(index) else {
                    return Err(E::Clip(ClipperError::Undefined("child index")));
                };
                // `operator[]` and `Child` share one body.
                let child = tree.child(node, index)?;
                events.push(json!({"child": number_of(&ids, &live, child)?}));
            }
            "set_scale" => {
                keys(op, &["op", "node", "scale"])?;
                if !K::D {
                    return invalid("op");
                }
                let (index, node) = node_of(&op["node"], &ids, &live)?;
                K::set_scale(&mut tree, node, f64_of(&op["scale"])?)?;
                events.push(json!({"scaled": index}));
            }
            _ => return invalid("op"),
        }
    }
    let mut report = Vec::new();
    for (&id, &alive) in ids.iter().zip(&live) {
        if !alive {
            report.push(Value::Null);
            continue;
        }
        let mut children = Vec::new();
        for &child in tree.children(id)? {
            children.push(number_of(&ids, &live, child)?);
        }
        let parent = match tree.parent(id)? {
            Some(parent) => Some(number_of(&ids, &live, parent)?),
            None => None,
        };
        let mut entry = json!({
            "area": bits(tree.area(id)?),
            "level": tree.level(id)?,
            "is_hole": tree.is_hole(id)?,
            "parent": parent,
            "count": tree.count(id)?,
            "children": children,
            "polygon": write_path(tree.polygon(id)?),
        });
        if let Some(scale) = K::scale(&tree, id)? {
            entry["scale"] = bits(scale);
        }
        report.push(entry);
    }
    Ok(json!({"events": events, "nodes": report}))
}

/// The C++ adapter's `Tree64`/`TreeD` serialization of an Execute output tree:
/// `{"polygon", "children"}` per node (and `"scale"` for `PolyPathD`), built with an explicit
/// post-order stack so deep trees cannot exhaust the native stack.
fn write_tree<K: TreeOps>(tree: &PolyTree<K::T>) -> R<Value> {
    let mut stack: Vec<(PolyPathId, usize, Vec<Value>)> =
        vec![(PolyTree::<K::T>::ROOT, 0, Vec::new())];
    loop {
        let Some(top) = stack.last_mut() else {
            return invalid("tree");
        };
        if let Some(&child) = tree.children(top.0)?.get(top.1) {
            top.1 += 1;
            stack.push((child, 0, Vec::new()));
            continue;
        }
        let Some((id, _, children)) = stack.pop() else {
            return invalid("tree");
        };
        let mut node = json!({"polygon": write_path(tree.polygon(id)?), "children": children});
        if let Some(scale) = K::scale(tree, id)? {
            node["scale"] = bits(scale);
        }
        match stack.last_mut() {
            Some(parent) => parent.2.push(node),
            None => return Ok(node),
        }
    }
}

/// Harness guard mirroring `GUARD::StripeIndex` in the C++ adapter. `StripeIndex::BuildFromPath`
/// and `PointInPolygon` belong to the engine source unit and stay deferred in the port; the
/// adapter refuses the inputs on which their pinned arithmetic is undefined before calling them,
/// so this shim refuses the same inputs for the same reason. It computes no result.
fn stripe_index_guard(path: &[Point64], query: Option<&Point64>) -> R<()> {
    const OVERFLOW: E = E::Clip(ClipperError::Undefined("StripeIndex int64_t overflow"));
    let mut edges = Vec::new();
    if path.len() >= 3 {
        let mut j = path.len() - 1;
        for i in 0..path.len() {
            if path[j].x != path[i].x || path[j].y != path[i].y {
                edges.push((path[j], path[i]));
            }
            j = i;
        }
    }
    let Some(first) = edges.first() else {
        return Ok(());
    };
    let (mut y_min, mut y_max) = (first.0.y, first.0.y);
    for (a, b) in &edges {
        y_min = y_min.min(a.y.min(b.y));
        y_max = y_max.max(a.y.max(b.y));
    }
    // `static_cast<int>(std::sqrt(static_cast<double>(edges_.size())))`, clamped to 1..65536.
    let count = ((edges.len() as f64).sqrt() as i64).clamp(1, 65536);
    let range = y_max.checked_sub(y_min).ok_or(OVERFLOW)?;
    let height = if range <= 0 {
        1
    } else {
        range
            .checked_add(count)
            .and_then(|sum| sum.checked_sub(1))
            .ok_or(OVERFLOW)?
            / count
    };
    let stripe = |y: i64| -> R<i64> {
        let s = y.checked_sub(y_min).ok_or(OVERFLOW)? / height;
        Ok(if s < 0 {
            0
        } else if s >= count {
            count - 1
        } else {
            s
        })
    };
    let mut total: i64 = 0;
    for (a, b) in &edges {
        let refs = stripe(a.y.max(b.y))?
            .checked_sub(stripe(a.y.min(b.y))?)
            .and_then(|span| span.checked_add(1))
            .ok_or(OVERFLOW)?;
        total = total.checked_add(refs).ok_or(OVERFLOW)?;
    }
    let declined = i64::try_from(edges.len())
        .ok()
        .and_then(|n| n.checked_mul(8))
        .is_none_or(|limit| total > limit);
    let Some(query) = query.filter(|_| !declined) else {
        return Ok(());
    };
    let s = stripe(query.y)?;
    for (a, b) in &edges {
        if s < stripe(a.y.min(b.y))? || s > stripe(a.y.max(b.y))? {
            continue;
        }
        let cross = cross_product_sign(a, b, query)?;
        if cross == 0
            && query.x >= a.x.min(b.x)
            && query.x <= a.x.max(b.x)
            && query.y >= a.y.min(b.y)
            && query.y <= a.y.max(b.y)
        {
            return Ok(());
        }
    }
    Ok(())
}

/// Harness domain mirroring `GUARD::EngineDomain` in the C++ adapter: Clipper64 hands caller
/// coordinates straight to the pinned engine, so the adapter runs it only within Clipper2's
/// declared range `MIN_COORD..MAX_COORD`. This is not a claim that every coordinate outside it
/// is undefined; the engine units own the precise guards.
fn engine_domain(op: &str, paths: &Paths64) -> R<()> {
    let outside = |v: i64| !(MIN_COORD..=MAX_COORD).contains(&v);
    if paths.iter().flatten().any(|p| outside(p.x) || outside(p.y)) {
        return Err(E::Invalid(format!(
            "outside the guarded engine domain: {op}"
        )));
    }
    Ok(())
}

fn run_engine(op: &str, v: &Value) -> R<Value> {
    match op {
        "vertex_flags" => {
            keys(v, &["op", "a", "b", "kind"])?;
            let (Ok(a), Ok(b)) = (
                u32::try_from(u64_of(&v["a"])?),
                u32::try_from(u64_of(&v["b"])?),
            ) else {
                return invalid("flags");
            };
            let (a, b) = (VertexFlags(a), VertexFlags(b));
            match text(v, "kind")? {
                "and" => Ok(json!((a & b).0)),
                "or" => Ok(json!((a | b).0)),
                _ => invalid("kind"),
            }
        }
        "engine_defaults" => {
            keys(v, &["op"])?;
            Ok(engine_defaults())
        }
        "engine_construct" => {
            keys(v, &["op", "kind", "point", "flag"])?;
            let kind = text(v, "kind")?;
            let flag = boolean(&v["flag"])?;
            let pt = read_point::<i64>(&v["point"])?;
            let rec = OutRecId(0);
            match kind {
                "outpt" => {
                    let this = OutPtId(0);
                    let op = OutPt::new(this, &pt, flag.then_some(rec));
                    Ok(
                        json!({"pt": write_point(&op.pt), "next_self": op.next == this,
                              "prev_self": op.prev == this, "outrec_arg": op.outrec == Some(rec),
                              "outrec_null": op.outrec.is_none(), "horz_null": op.horz.is_none()}),
                    )
                }
                "local_minima" => {
                    // flag selects the path type; a nonzero point z selects an open path.
                    let vertex = VertexId(0);
                    let polytype = if flag {
                        PathType::Clip
                    } else {
                        PathType::Subject
                    };
                    let lm = LocalMinima::new(vertex, polytype, pt.z != 0);
                    Ok(json!({"vertex_arg": lm.vertex == vertex,
                              "polytype": if lm.polytype == PathType::Clip { "Clip" } else { "Subject" },
                              "is_open": lm.is_open}))
                }
                "intersect_node" => {
                    let (e1, e2) = (ActiveId(0), ActiveId(1));
                    let node = IntersectNode::new(flag.then_some(e1), flag.then_some(e2), &pt);
                    Ok(
                        json!({"pt": write_point(&node.pt), "edge1_arg": node.edge1 == Some(e1),
                              "edge2_arg": node.edge2 == Some(e2), "edge1_null": node.edge1.is_none(),
                              "edge2_null": node.edge2.is_none()}),
                    )
                }
                "horz_segment" => {
                    let op = OutPtId(0);
                    let seg = HorzSegment::new(flag.then_some(op));
                    Ok(
                        json!({"left_op_arg": seg.left_op == Some(op), "left_op_null": seg.left_op.is_none(),
                              "right_op_null": seg.right_op.is_none(), "left_to_right": seg.left_to_right}),
                    )
                }
                "horz_join" => {
                    let (a, b) = (OutPtId(0), OutPtId(1));
                    let join = HorzJoin::new(flag.then_some(a), flag.then_some(b));
                    Ok(
                        json!({"op1_arg": join.op1 == Some(a), "op2_arg": join.op2 == Some(b),
                              "op1_null": join.op1.is_none(), "op2_null": join.op2.is_none()}),
                    )
                }
                _ => invalid("kind"),
            }
        }
        "stripe_index" => {
            keys(v, &["op", "path", "query"])?;
            let path = read_path::<i64>(&v["path"])?;
            let query = match v["query"].is_null() {
                true => None,
                false => Some(read_point::<i64>(&v["query"])?),
            };
            stripe_index_guard(&path, query.as_ref())?;
            let mut index = StripeIndex::default();
            index.build_from_path(&path)?;
            let mut result = json!({"edge_count": index.edge_count(), "empty": index.empty()});
            if let Some(query) = query {
                result["query"] = json!(pip_name(index.point_in_polygon(&query)?));
            }
            Ok(result)
        }
        "clipper_settings" => {
            keys(
                v,
                &[
                    "op",
                    "class",
                    "precision",
                    "preserve_collinear",
                    "reverse_solution",
                    "pip_index_threshold",
                ],
            )?;
            match text(v, "class")? {
                "64" => {
                    if !v["precision"].is_null() {
                        return invalid("precision");
                    }
                    let mut clipper = Clipper64::default();
                    let before = settings(&clipper.base);
                    apply_settings(&mut clipper.base, v)?;
                    let has_z = clipper.base.z_callback_kind() != ZCallbackKind::Empty;
                    Ok(
                        json!({"before": before, "after": settings(&clipper.base), "z_callback": has_z}),
                    )
                }
                "D" => {
                    let mut clipper = match make_d(&v["precision"])? {
                        Ok(clipper) => clipper,
                        Err(e) => return Ok(thrown(&e)),
                    };
                    let before = settings(&clipper.base);
                    apply_settings(&mut clipper.base, v)?;
                    let has_z = clipper.base.z_callback_kind() != ZCallbackKind::Empty;
                    Ok(
                        json!({"before": before, "after": settings(&clipper.base), "z_callback": has_z}),
                    )
                }
                _ => invalid("class"),
            }
        }
        "clipper64_z_callback" => {
            keys(v, &["op", "set"])?;
            let mut clipper = Clipper64::default();
            let before = clipper.base.z_callback_kind() != ZCallbackKind::Empty;
            if boolean(&v["set"])? {
                clipper.set_z_callback(Some(Box::new(|_, _, _, _, _| {})));
            } else {
                clipper.set_z_callback(None);
            }
            let after = clipper.base.z_callback_kind() != ZCallbackKind::Empty;
            Ok(json!({"before": before, "after": after}))
        }
        "clipperd_zcb" | "clipperd_check_callback" => {
            keys(v, &["op", "precision", "callback", "points", "z", "invoke"])?;
            let pts = read_path::<i64>(&v["points"])?;
            let [e1b, e1t, e2b, e2t, pt] = pts.as_slice() else {
                return invalid("points");
            };
            let callback = boolean(&v["callback"])?;
            let invoke = boolean(&v["invoke"])?;
            let z = i64_of(&v["z"])?;
            let mut clipper = match make_d(&v["precision"])? {
                Ok(clipper) => clipper,
                Err(e) => return Ok(thrown(&e)),
            };
            let calls = Rc::new(RefCell::new(Vec::new()));
            if callback {
                clipper.set_z_callback(Some(recorder(z, Rc::clone(&calls))));
            }
            let mut result =
                json!({"before": clipper.base.z_callback_kind() != ZCallbackKind::Empty});
            if op == "clipperd_check_callback" {
                clipper.check_callback();
                result["after"] = json!(clipper.base.z_callback_kind() != ZCallbackKind::Empty);
            }
            if invoke {
                let mut target = *pt;
                let outcome = if op == "clipperd_zcb" {
                    clipper.zcb(e1b, e1t, e2b, e2t, &mut target)
                } else {
                    clipper.call_base_z_callback(e1b, e1t, e2b, e2t, &mut target)
                };
                match outcome {
                    Ok(()) => result["pt"] = write_point(&target),
                    Err(ClipperError::BadFunctionCall) => {
                        result["thrown"] = json!("std::bad_function_call")
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            result["calls"] = Value::Array(calls.borrow().clone());
            Ok(result)
        }
        "clipperd_add" => {
            keys(v, &["op", "precision", "kind", "paths"])?;
            let paths = read_paths::<f64>(&v["paths"])?;
            let kind = text(v, "kind")?;
            let mut clipper = match make_d(&v["precision"])? {
                Ok(clipper) => clipper,
                Err(e) => return Ok(thrown(&e)),
            };
            let outcome = match kind {
                "subject" => clipper.add_subject(&paths),
                "open_subject" => clipper.add_open_subject(&paths),
                "clip" => clipper.add_clip(&paths),
                _ => return invalid("kind"),
            };
            match outcome {
                Ok(()) => Ok(json!({"added": true, "error_code": clipper.base.error_code()})),
                Err(ClipperError::Exception(e)) => {
                    Ok(json!({"thrown": e.what(), "error_code": clipper.base.error_code()}))
                }
                Err(e) => Err(e.into()),
            }
        }
        "clipper64_execute" | "clipperd_execute" => {
            keys(
                v,
                &[
                    "op",
                    "precision",
                    "subjects",
                    "open_subjects",
                    "clips",
                    "clip_type",
                    "fill_rule",
                    "output",
                ],
            )?;
            let ct = clip_type(&v["clip_type"])?;
            let fr = fill_rule(&v["fill_rule"])?;
            let output = text(v, "output")?;
            if !matches!(output, "paths" | "paths_open" | "tree" | "tree_open") {
                return invalid("output");
            }
            if op == "clipper64_execute" {
                if !v["precision"].is_null() {
                    return invalid("precision");
                }
                // A null path group skips that Add call. Every group is checked against the
                // guarded engine domain before the engine sees any of them.
                let mut groups = Vec::new();
                for field in ["subjects", "open_subjects", "clips"] {
                    let group = match v[field].is_null() {
                        true => None,
                        false => Some(read_paths::<i64>(&v[field])?),
                    };
                    if let Some(paths) = &group {
                        engine_domain(op, paths)?;
                    }
                    groups.push(group);
                }
                let mut clipper = Clipper64::default();
                for (kind, group) in groups.iter().enumerate() {
                    if let Some(paths) = group {
                        match kind {
                            0 => clipper.add_subject(paths)?,
                            1 => clipper.add_open_subject(paths)?,
                            _ => clipper.add_clip(paths)?,
                        }
                    }
                }
                let mut closed = vec![vec![Point64::xy(7, 7)]];
                let mut open = vec![vec![Point64::xy(8, 8)]];
                let mut tree = PolyTree64::new(TREE_BUDGET)?;
                let ok = match output {
                    "paths" => clipper.execute(ct, fr, &mut closed)?,
                    "paths_open" => clipper.execute_with_open(ct, fr, &mut closed, &mut open)?,
                    "tree" => clipper.execute_tree(ct, fr, &mut tree)?,
                    _ => clipper.execute_tree_with_open(ct, fr, &mut tree, &mut open)?,
                };
                // Reached only once the engine units land; the shape follows the C++ adapter,
                // which reports the tree for every output kind.
                return Ok(
                    json!({"succeeded": ok, "closed": write_paths(&closed), "open": write_paths(&open),
                           "tree": write_tree::<Tree64>(&tree)?, "error_code": clipper.base.error_code()}),
                );
            }
            let mut clipper = match make_d(&v["precision"])? {
                Ok(clipper) => clipper,
                Err(e) => return Ok(thrown(&e)),
            };
            for (field, kind) in [("subjects", 0), ("open_subjects", 1), ("clips", 2)] {
                if v[field].is_null() {
                    continue;
                }
                let paths = read_paths::<f64>(&v[field])?;
                let added = match kind {
                    0 => clipper.add_subject(&paths),
                    1 => clipper.add_open_subject(&paths),
                    _ => clipper.add_clip(&paths),
                };
                match added {
                    Ok(()) => {}
                    Err(ClipperError::Exception(e)) => return Ok(thrown(&e)),
                    Err(e) => return Err(e.into()),
                }
            }
            let mut closed = PathsD::new();
            let mut open = PathsD::new();
            let mut tree = PolyTreeD::new(TREE_BUDGET)?;
            let ok = match output {
                "paths" => clipper.execute(ct, fr, &mut closed)?,
                "paths_open" => clipper.execute_with_open(ct, fr, &mut closed, &mut open)?,
                "tree" => clipper.execute_tree(ct, fr, &mut tree)?,
                _ => clipper.execute_tree_with_open(ct, fr, &mut tree, &mut open)?,
            };
            Ok(
                json!({"succeeded": ok, "closed": write_paths(&closed), "open": write_paths(&open),
                      "tree": write_tree::<TreeD>(&tree)?, "error_code": clipper.base.error_code()}),
            )
        }
        "polytree" => {
            keys(v, &["op", "type", "root", "root_type", "ops"])?;
            match text(v, "type")? {
                "i64" => run_tree::<Tree64>(v),
                "f64" => run_tree::<TreeD>(v),
                _ => invalid("type"),
            }
        }
        "polypath_base_count" => {
            keys(v, &["op"])?;
            let mut tree = PolyTree64::new(TREE_BUDGET)?;
            tree.add_child(PolyTree64::ROOT, &Vec::new())?;
            Ok(json!({"base": poly_path_base_count(), "derived": tree.count(PolyTree64::ROOT)?}))
        }
        _ => invalid("op"),
    }
}

fn run(v: &Value) -> R<Value> {
    let op = text(v, "op")?;
    match op {
        "constants" => {
            keys(v, &["op"])?;
            Ok(json!({
                "max_coord": MAX_COORD, "min_coord": MIN_COORD, "invalid": INVALID,
                "max_coord_d": bits(MAX_COORD_D), "min_coord_d": bits(MIN_COORD_D), "max_dbl": bits(MAX_DBL),
                "pi": bits(PI), "max_dec_precision": CLIPPER2_MAX_DEC_PRECISION,
                "error_codes": [PRECISION_ERROR_I, SCALE_ERROR_I, NON_PAIR_ERROR_I, UNDEFINED_ERROR_I, RANGE_ERROR_I],
                "messages": [PRECISION_ERROR, RANGE_ERROR, SCALE_ERROR, NON_PAIR_ERROR, UNDEFINED_ERROR],
                "invalid_point64": write_point(&INVALID_POINT64),
                "invalid_pointd": write_point(&INVALID_POINTD),
                "invalid_rect64": write_rect(&invalid_rect64()),
                "invalid_rectd": write_rect(&invalid_rect_d()),
            }))
        }
        "do_error" => {
            keys(v, &["op", "code"])?;
            match do_error(i32_of(&v["code"])?) {
                ClipperError::Exception(e) => Ok(thrown(&e)),
                other => Err(other.into()),
            }
        }
        "exception" => {
            keys(v, &["op", "message"])?;
            let error = Clipper2Exception::new(text(v, "message")?);
            Ok(json!({"what": error.what()}))
        }
        "check_precision" => {
            keys(v, &["op", "precision", "error_code"])?;
            let mut precision = i32_of(&v["precision"])?;
            if v["error_code"].is_null() {
                return match check_precision_range_only(&mut precision) {
                    Ok(()) => Ok(json!({"precision": precision})),
                    Err(ClipperError::Exception(e)) => {
                        Ok(json!({"thrown": e.what(), "precision": precision}))
                    }
                    Err(e) => Err(e.into()),
                };
            }
            let mut error_code = i32_of(&v["error_code"])?;
            match check_precision_range(&mut precision, &mut error_code) {
                Ok(()) => Ok(json!({"precision": precision, "error_code": error_code})),
                Err(ClipperError::Exception(e)) => Ok(
                    json!({"thrown": e.what(), "precision": precision, "error_code": error_code}),
                ),
                Err(e) => Err(e.into()),
            }
        }
        "tri_sign" => {
            keys(v, &["op", "x"])?;
            Ok(json!(tri_sign(i64_of(&v["x"])?)))
        }
        "multiply_u64" => {
            keys(v, &["op", "a", "b"])?;
            let r = multiply_u64(u64_of(&v["a"])?, u64_of(&v["b"])?);
            Ok(json!({"lo": r.lo, "hi": r.hi}))
        }
        "uint128_eq" => {
            keys(v, &["op", "a", "b"])?;
            let words = |value: &Value| -> R<UInt128Struct> {
                match value.as_array() {
                    Some(a) if a.len() == 2 => Ok(UInt128Struct {
                        lo: u64_of(&a[0])?,
                        hi: u64_of(&a[1])?,
                    }),
                    _ => invalid("uint128"),
                }
            };
            Ok(json!(words(&v["a"])?.struct_eq(&words(&v["b"])?)))
        }
        "products_are_equal" => {
            keys(v, &["op", "a", "b", "c", "d"])?;
            Ok(json!(products_are_equal(
                i64_of(&v["a"])?,
                i64_of(&v["b"])?,
                i64_of(&v["c"])?,
                i64_of(&v["d"])?
            )))
        }
        "segments_intersect" => {
            keys(v, &["op", "points", "inclusive"])?;
            let pts = read_path::<i64>(&v["points"])?;
            let [a1, a2, b1, b2] = pts.as_slice() else {
                return invalid("points");
            };
            // The default argument is `inclusive = false`.
            let inclusive = if v["inclusive"].is_null() {
                false
            } else {
                boolean(&v["inclusive"])?
            };
            Ok(json!(segments_intersect(a1, a2, b1, b2, inclusive)?))
        }
        "append_path" => {
            keys(v, &["op", "type", "paths", "path"])?;
            fn append<T: Wire>(v: &Value) -> R<Value> {
                let mut paths = read_paths::<T>(&v["paths"])?;
                let path = read_path::<T>(&v["path"])?;
                append_path(&mut paths, &path)?;
                Ok(json!({"paths": write_paths(&paths), "returns_self": true}))
            }
            match text(v, "type")? {
                "i64" => append::<i64>(v),
                "f64" => append::<f64>(v),
                _ => invalid("type"),
            }
        }
        "point_construct" | "point_convert" | "point_init" | "append_point" | "scale_rect"
        | "get_bounds" | "scale_path" | "scale_paths" | "transform_path" | "transform_paths" => {
            match text(v, "to")? {
                "i64" => dispatch_from::<i64>(op, v),
                "f64" => dispatch_from::<f64>(op, v),
                _ => invalid("to"),
            }
        }
        _ if op != "polytree" && v.get("type").is_some() => match text(v, "type")? {
            "i64" => run_typed::<i64>(op, v),
            "f64" => run_typed::<f64>(op, v),
            _ => invalid("type"),
        },
        _ => run_engine(op, v),
    }
}

/// Answer one JSONL request exactly as the C++ oracle adapter does.
pub fn dispatch(line: &str) -> Value {
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"unit": unit, "case": case, "error": {"kind": kind, "message": message}});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                Value::Null,
                Value::Null,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    let unit = request
        .get("unit")
        .filter(|u| u.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    let case = request
        .get("case")
        .filter(|c| c.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    if keys(&request, &["unit", "case", "input"]).is_err()
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
    let input = &request["input"];
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match run(input) {
        Ok(result) => json!({"unit": unit, "case": case, "result": result}),
        Err(E::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(E::Clip(ClipperError::Undefined(_))) => failure(
            unit,
            case,
            "invalid_input",
            &format!("undefined behaviour: {op}"),
        ),
        Err(E::Clip(ClipperError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"unit": unit, "case": case, "error": {"kind": "unported", "upstream": upstream, "unit": owner}})
        }
        Err(E::Clip(ClipperError::Exception(e))) => {
            failure(unit, case, "execution_error", e.what())
        }
        Err(E::Clip(other)) => failure(unit, case, "execution_error", &other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Execute reply's `"tree"` has the C++ adapter's `Tree64`/`TreeD` shape, nested in
    /// child order, so the envelope matches once the engine units build real trees.
    #[test]
    fn execute_trees_serialize_like_the_adapter() -> Result<(), String> {
        let fail = |_| String::from("tree operation failed");
        let mut tree = PolyTreeD::new(TREE_BUDGET).map_err(fail)?;
        let first = tree
            .add_child(PolyTreeD::ROOT, &vec![Point64::xy(1, 2)])
            .map_err(fail)?;
        tree.add_child_d(first, &vec![PointD::xy(0.5, -0.0)])
            .map_err(fail)?;
        tree.add_child(PolyTreeD::ROOT, &Vec::new()).map_err(fail)?;
        let written = write_tree::<TreeD>(&tree).map_err(|_| String::from("write failed"))?;
        let one = bits(1.0);
        let expected = json!({"polygon": [], "scale": one, "children": [
            {"polygon": [[bits(1.0), bits(2.0), 0]], "scale": one, "children": [
                {"polygon": [[bits(0.5), bits(-0.0), 0]], "scale": one, "children": []}]},
            {"polygon": [], "scale": one, "children": []}]});
        assert_eq!(written, expected);
        let empty = PolyTree64::new(TREE_BUDGET).map_err(fail)?;
        assert_eq!(
            write_tree::<Tree64>(&empty).map_err(|_| String::from("write failed"))?,
            json!({"polygon": [], "children": []})
        );
        Ok(())
    }
}
