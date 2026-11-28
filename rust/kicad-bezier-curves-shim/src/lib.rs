// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.bezier_curves.application.ui.p001`
//! (`libs/kimath/src/bezier_curves.cpp`, ported as `kicad_hash::bezier_curves`). It mirrors
//! `port/oracle/shims/kimath_src_bezier_curves.cpp`: `int` values as JSON integers, `double`
//! values as exact IEEE bits, angles as the bits of their value in degrees, one operation per
//! request, the same validation order and messages, and the same refusals: `invalid_input`
//! "undefined bezier operation" where the pinned evaluation is undefined, and "outside the
//! verified domain" beyond the oracle's loop and curve budgets ([`STEP_BUDGET`],
//! [`MAX_BEZIERS`]), which the port counts exactly.
//!
//! A `wxASSERT` reached by the port defers as `wxOnAssert` (the oracle records the call and
//! continues), so every successful reply has an empty `assertions` list.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_hash::bezier_curves::{
    self as bc, BezierError, EllipseBezierCoordinate, DEFAULT_MAX_ERROR_D, DEFAULT_MAX_ERROR_I,
    UNIT,
};
use kicad_hash::{Bezier, BezierPoly};
use kicad_math::eda_angle::{AngleUnit, EdaAngle};
use kicad_math::geometry_headers::Ellipse;
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined bezier operation";
const OUTSIDE: &str = "outside the verified domain";

/// The most control points, prior output points or prior curves a request may give.
pub const MAX_POINTS: usize = 4096;

/// The most loop iterations one `GetPoly` call may run (the oracle's guard budget).
pub const STEP_BUDGET: u64 = 1 << 20;

/// The most curves one `TransformEllipseToBeziers` call may add.
pub const MAX_BEZIERS: u64 = 4096;

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
impl From<BezierError> for Error {
    fn from(e: BezierError) -> Self {
        match e {
            BezierError::Undefined => Self::Invalid(UNDEFINED.into()),
            BezierError::LimitExceeded => Self::Invalid(OUTSIDE.into()),
            BezierError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}

/// `checkKeys( aInput, required, optional )`.
fn keys(v: &Map<String, Value>, required: &[&str], optional: &[&str]) -> Result<(), String> {
    if required.iter().any(|f| !v.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    let present = required.len() + optional.iter().filter(|f| v.contains_key(**f)).count();
    if v.len() != present {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn object(v: &Value) -> Result<&Map<String, Value>, String> {
    v.as_object().ok_or_else(|| "expected object".to_string())
}
fn text<'a>(v: &'a Map<String, Value>, name: &str) -> Result<&'a str, String> {
    v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn get<'a>(v: &'a Map<String, Value>, name: &str) -> Result<&'a Value, String> {
    v.get(name).ok_or_else(|| format!("missing: {name}"))
}
fn read_int(v: &Value) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
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
fn read_array<'a>(v: &'a Map<String, Value>, name: &str) -> Result<&'a Vec<Value>, String> {
    let array = v
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected array: {name}"))?;
    if array.len() > MAX_POINTS {
        return Err(format!("too many entries: {name}"));
    }
    Ok(array)
}
fn read_points_i(v: &Map<String, Value>, name: &str) -> Result<Vec<Vector2I>, String> {
    read_array(v, name)?.iter().map(read_point_i).collect()
}
fn read_points_d(v: &Map<String, Value>, name: &str) -> Result<Vec<Vector2D>, String> {
    read_array(v, name)?.iter().map(read_point_d).collect()
}
/// `EDA_ANGLE( value, DEGREES_T )`.
fn read_angle(v: &Value) -> Result<EdaAngle, String> {
    Ok(EdaAngle::new(decode_bits(v)?, AngleUnit::Degrees))
}
fn point_i(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn point_d(p: Vector2D) -> Value {
    json!([bits(p.x), bits(p.y)])
}
fn points_d(points: &[Vector2D]) -> Value {
    Value::Array(points.iter().map(|p| point_d(*p)).collect())
}
fn points_i(points: &[Vector2I]) -> Value {
    Value::Array(points.iter().map(|p| point_i(*p)).collect())
}

/// The members after the call.
fn state(poly: &BezierPoly) -> Map<String, Value> {
    let mut reply = Map::new();
    reply.insert("min_seg_len".into(), bits(poly.min_seg_len()));
    reply.insert("ctrl_pts".into(), points_d(poly.ctrl_pts()));
    reply
}

/// The BEZIER_POLY a request names, through the constructor its "form" selects.
fn read_poly(v: &Map<String, Value>) -> Result<BezierPoly, String> {
    match text(v, "form")? {
        "points4" => {
            let points = read_array(v, "points")?;
            let [a, b, c, d] = points.as_slice() else {
                return Err("expected four points".into());
            };
            Ok(BezierPoly::from_points(
                read_point_i(a)?,
                read_point_i(b)?,
                read_point_i(c)?,
                read_point_i(d)?,
            ))
        }
        "vector_i" => Ok(BezierPoly::from_vector2i(&read_points_i(v, "points")?)),
        "vector_d" => Ok(BezierPoly::from_vector2d(&read_points_d(v, "points")?)),
        _ => Err("unknown form".into()),
    }
}

/// The two `ELLIPSE<T>` request encodings.
trait EllipseCoordinate: EllipseBezierCoordinate + Default {
    fn read_point(v: &Value) -> Result<Vector2<Self>, String>;
    fn read_scalar(v: &Value) -> Result<Self, String>;
    fn point(p: Vector2<Self>) -> Value;
}
impl EllipseCoordinate for i32 {
    fn read_point(v: &Value) -> Result<Vector2I, String> {
        read_point_i(v)
    }
    fn read_scalar(v: &Value) -> Result<i32, String> {
        read_int(v)
    }
    fn point(p: Vector2I) -> Value {
        point_i(p)
    }
}
impl EllipseCoordinate for f64 {
    fn read_point(v: &Value) -> Result<Vector2D, String> {
        read_point_d(v)
    }
    fn read_scalar(v: &Value) -> Result<f64, String> {
        decode_bits(v)
    }
    fn point(p: Vector2D) -> Value {
        point_d(p)
    }
}

fn ellipse_to_beziers<T: EllipseCoordinate>(v: &Map<String, Value>) -> Result<Value, Error> {
    keys(
        v,
        &[
            "op", "type", "center", "major", "minor", "rotation", "start", "end", "output",
        ],
        &[],
    )?;
    let ellipse = Ellipse {
        center: T::read_point(get(v, "center")?)?,
        major_radius: T::read_scalar(get(v, "major")?)?,
        minor_radius: T::read_scalar(get(v, "minor")?)?,
        rotation: read_angle(get(v, "rotation")?)?,
        start_angle: read_angle(get(v, "start")?)?,
        end_angle: read_angle(get(v, "end")?)?,
    };
    let mut beziers = Vec::new();
    for curve in read_array(v, "output")? {
        let Some([a, b, c, d]) = curve.as_array().map(Vec::as_slice) else {
            return Err("expected curve".into());
        };
        beziers.push(Bezier {
            start: T::read_point(a)?,
            c1: T::read_point(b)?,
            c2: T::read_point(c)?,
            end: T::read_point(d)?,
        });
    }
    bc::transform_ellipse_to_beziers(&ellipse, &mut beziers, MAX_BEZIERS)?;
    let out: Vec<Value> = beziers
        .iter()
        .map(|b| {
            json!([
                T::point(b.start),
                T::point(b.c1),
                T::point(b.c2),
                T::point(b.end)
            ])
        })
        .collect();
    Ok(json!({ "output": out }))
}

fn with(mut reply: Map<String, Value>, entries: Vec<(&str, Value)>) -> Value {
    for (key, value) in entries {
        reply.insert(key.into(), value);
    }
    Value::Object(reply)
}

fn run(input: &Value) -> Result<Value, Error> {
    let v = object(input)?;
    let op = text(v, "op")?;
    let no_assertions = || json!([]);
    match op {
        "construct" => {
            keys(v, &["op", "form", "points"], &[])?;
            Ok(Value::Object(state(&read_poly(v)?)))
        }
        "get_poly_i" => {
            keys(v, &["op", "form", "points", "output"], &["max_error"])?;
            let poly = read_poly(v)?;
            let mut output = read_points_i(v, "output")?;
            let max_error = match v.get("max_error") {
                None => DEFAULT_MAX_ERROR_I,
                Some(value) => read_int(value)?,
            };
            poly.get_poly_i(&mut output, max_error, STEP_BUDGET)?;
            Ok(with(
                state(&poly),
                vec![
                    ("output", points_i(&output)),
                    ("assertions", no_assertions()),
                ],
            ))
        }
        "get_poly_d" => {
            keys(v, &["op", "form", "points", "output"], &["max_error"])?;
            let poly = read_poly(v)?;
            let mut output = read_points_d(v, "output")?;
            let max_error = match v.get("max_error") {
                None => DEFAULT_MAX_ERROR_D,
                Some(value) => decode_bits(value)?,
            };
            poly.get_poly_d(&mut output, max_error, STEP_BUDGET)?;
            Ok(with(
                state(&poly),
                vec![
                    ("output", points_d(&output)),
                    ("assertions", no_assertions()),
                ],
            ))
        }
        "is_nan" => {
            keys(v, &["op", "points"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let value = poly.is_nan();
            Ok(with(state(&poly), vec![("value", json!(value))]))
        }
        "is_flat" => {
            keys(v, &["op", "points", "max_error"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let max_error = decode_bits(get(v, "max_error")?)?;
            let value = poly.is_flat(max_error)?;
            Ok(with(
                state(&poly),
                vec![("value", json!(value)), ("assertions", no_assertions())],
            ))
        }
        "eval" => {
            keys(v, &["op", "points", "t"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let t = decode_bits(get(v, "t")?)?;
            let value = poly.eval(t)?;
            Ok(with(
                state(&poly),
                vec![("value", point_d(value)), ("assertions", no_assertions())],
            ))
        }
        "subdivide" => {
            keys(v, &["op", "points", "t", "left", "right"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let mut left = BezierPoly::from_vector2d(&read_points_d(v, "left")?);
            let mut right = BezierPoly::from_vector2d(&read_points_d(v, "right")?);
            let t = decode_bits(get(v, "t")?)?;
            poly.subdivide(t, &mut left, &mut right)?;
            Ok(with(
                state(&poly),
                vec![
                    ("left", points_d(left.ctrl_pts())),
                    ("right", points_d(right.ctrl_pts())),
                    ("assertions", no_assertions()),
                ],
            ))
        }
        "number_of_inflection_points" => {
            keys(v, &["op", "points"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let value = poly.number_of_inflection_points()?;
            Ok(with(state(&poly), vec![("value", json!(value))]))
        }
        "find_inflection_points" => {
            keys(v, &["op", "points", "t1", "t2"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let mut t1 = decode_bits(get(v, "t1")?)?;
            let mut t2 = decode_bits(get(v, "t2")?)?;
            let value = poly.find_inflection_points(&mut t1, &mut t2)?;
            Ok(with(
                state(&poly),
                vec![("value", json!(value)), ("t1", bits(t1)), ("t2", bits(t2))],
            ))
        }
        "third_control_point_deviation" => {
            keys(v, &["op", "points"], &[])?;
            let poly = BezierPoly::from_vector2d(&read_points_d(v, "points")?);
            let value = poly.third_control_point_deviation()?;
            Ok(with(state(&poly), vec![("value", bits(value))]))
        }
        "ellipse_to_beziers" => match text(v, "type")? {
            "double" => ellipse_to_beziers::<f64>(v),
            "int" => ellipse_to_beziers::<i32>(v),
            _ => Err("unknown type".into()),
        },
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
    let shaped = request
        .as_object()
        .is_some_and(|o| keys(o, &["unit", "case", "input"], &[]).is_ok());
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
