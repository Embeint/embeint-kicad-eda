// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.circle.application.ui.p001`. It mirrors
//! `port/oracle/shims/kimath_src_circle.cpp`: `int` values as JSON integers, `double` values as
//! exact IEEE bits, one operation per request, and the same `invalid_input` refusal where the
//! pinned evaluation is undefined.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_math::circle::{Circle, CircleError, UNIT};
use kicad_math::seg::Seg;
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined circle operation";
const OUTSIDE_TAN_TAN: &str = "tan-tan request outside the audited domain";
/// The adapter's bound on ConstructFromTanTanPt coordinates (`TAN_TAN_BOUND`).
const TAN_TAN_BOUND: i64 = 1 << 16;

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
impl From<CircleError> for Error {
    fn from(e: CircleError) -> Self {
        match e {
            CircleError::Undefined => Self::Invalid(UNDEFINED.into()),
            CircleError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}

fn keys(v: &Map<String, Value>, fields: &[&str]) -> Result<(), String> {
    if v.len() != fields.len() || fields.iter().any(|f| !v.contains_key(*f)) {
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
        .filter(|_| v.is_i64() || v.is_u64())
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
fn read_seg(v: &Value) -> Result<Seg, String> {
    match v.as_array().map(Vec::as_slice) {
        Some([a, b]) => Ok(Seg::new(read_point_i(a)?, read_point_i(b)?)),
        _ => Err("expected segment".into()),
    }
}
fn read_circle(v: &Value) -> Result<Circle, String> {
    let c = object(v)?;
    keys(c, &["center", "radius"])?;
    Ok(Circle::with_center_radius(
        read_point_i(get(c, "center")?)?,
        read_int(get(c, "radius")?)?,
    ))
}
fn point_i(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn point_d(p: Vector2D) -> Value {
    json!([bits(p.x), bits(p.y)])
}
fn points(ps: &[Vector2I]) -> Value {
    Value::Array(ps.iter().copied().map(point_i).collect())
}
fn state(c: &Circle) -> Value {
    json!({"center": point_i(c.center), "radius": c.radius})
}

/// The adapter's ConstructFromTanTanPt domain checks that do not depend on a pinned call:
/// every coordinate within [`TAN_TAN_BOUND`] and both lines non-degenerate. The adapter's
/// remaining check (the pinned ApproxParallel, else an angle whose sine is at least 1/8) needs
/// `SEG::Square`, which is unported here; the public corpus only holds requests that pass it.
fn tan_tan_bounds(line_a: &Seg, line_b: &Seg, p: Vector2I) -> Result<(), String> {
    let bounded = |v: Vector2I| {
        i64::from(v.x).abs() <= TAN_TAN_BOUND && i64::from(v.y).abs() <= TAN_TAN_BOUND
    };
    if !bounded(line_a.a)
        || !bounded(line_a.b)
        || !bounded(line_b.a)
        || !bounded(line_b.b)
        || !bounded(p)
        || line_a.a == line_a.b
        || line_b.a == line_b.b
    {
        return Err(OUTSIDE_TAN_TAN.into());
    }
    Ok(())
}

fn run(input: &Value) -> Result<Value, Error> {
    let v = object(input)?;
    let op = text(v, "op")?;
    match op {
        "default" => {
            keys(v, &["op"])?;
            Ok(json!({"circle": state(&Circle::new())}))
        }
        "new" => {
            keys(v, &["op", "center", "radius"])?;
            let center = read_point_i(get(v, "center")?)?;
            let radius = read_int(get(v, "radius")?)?;
            Ok(json!({"circle": state(&Circle::with_center_radius(center, radius))}))
        }
        "copy" => {
            keys(v, &["op", "circle"])?;
            let source = read_circle(get(v, "circle")?)?;
            let copy = Circle::copy_of(&source);
            Ok(json!({"circle": state(&copy), "equal": copy == source}))
        }
        "equals" => {
            keys(v, &["op", "circle", "other"])?;
            let a = read_circle(get(v, "circle")?)?;
            let b = read_circle(get(v, "other")?)?;
            Ok(json!({"equal": a == b, "reverse": b == a}))
        }
        "contains" => {
            keys(v, &["op", "circle", "point", "overload"])?;
            let mut circle = read_circle(get(v, "circle")?)?;
            let p = read_point_i(get(v, "point")?)?;
            let value = match text(v, "overload")? {
                "const" => circle.contains(p)?,
                "non_const" => circle.contains_mut(p)?,
                _ => return Err("unknown overload".into()),
            };
            Ok(json!({"value": value, "circle": state(&circle)}))
        }
        "nearest_i32" | "furthest_i32" => {
            keys(v, &["op", "circle", "point"])?;
            let circle = read_circle(get(v, "circle")?)?;
            let p = read_point_i(get(v, "point")?)?;
            let out = if op == "furthest_i32" {
                circle.furthest_point(p)?
            } else {
                circle.nearest_point(p)?
            };
            Ok(json!({"point": point_i(out)}))
        }
        "nearest_f64" | "furthest_f64" => {
            keys(v, &["op", "circle", "point"])?;
            let circle = read_circle(get(v, "circle")?)?;
            let p = read_point_d(get(v, "point")?)?;
            let out = if op == "furthest_f64" {
                circle.furthest_point_d(p)?
            } else {
                circle.nearest_point_d(p)?
            };
            Ok(json!({"point": point_d(out)}))
        }
        "intersect_circle" => {
            keys(v, &["op", "circle", "other"])?;
            let a = read_circle(get(v, "circle")?)?;
            let b = read_circle(get(v, "other")?)?;
            Ok(json!({"points": points(&a.intersect_circle(&b)?)}))
        }
        "intersect_seg" | "intersect_line" => {
            keys(v, &["op", "circle", "seg"])?;
            let circle = read_circle(get(v, "circle")?)?;
            let seg = read_seg(get(v, "seg")?)?;
            let out = if op == "intersect_seg" {
                circle.intersect_seg(&seg)?
            } else {
                circle.intersect_line(&seg)?
            };
            Ok(json!({"points": points(&out)}))
        }
        "tan_tan_pt" => {
            keys(v, &["op", "circle", "line_a", "line_b", "point"])?;
            let mut circle = read_circle(get(v, "circle")?)?;
            let line_a = read_seg(get(v, "line_a")?)?;
            let line_b = read_seg(get(v, "line_b")?)?;
            let p = read_point_i(get(v, "point")?)?;
            tan_tan_bounds(&line_a, &line_b, p)?;
            let result: *const Circle = circle.construct_from_tan_tan_pt(&line_a, &line_b, p)?;
            let returns_self = core::ptr::eq(result, &circle);
            Ok(json!({"circle": state(&circle), "returns_self": returns_self}))
        }
        _ => Err("unknown operation".into()),
    }
}

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
        .is_some_and(|o| keys(o, &["unit", "case", "input"]).is_ok());
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
