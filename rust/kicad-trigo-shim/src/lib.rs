// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.trigo.application.ui.p001`. It mirrors
//! `port/oracle/shims/trigo_source_p001.cpp`: `int` values as JSON integers, `double` values as
//! exact IEEE bits, angles as the bits of their value in degrees, one operation per request, and
//! the same `invalid_input` refusal where the pinned evaluation is undefined (including a NaN
//! coordinate passed to the three-point `VECTOR2D` `CalcArcCenter`, whose `std::minmax` requires a
//! strict weak ordering).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::eda_angle::{AngleUnit, EdaAngle};
use kicad_math::trigo::{self, TrigoError, UNIT};
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined trigo operation";

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
impl From<TrigoError> for Error {
    fn from(e: TrigoError) -> Self {
        match e {
            TrigoError::Undefined => Self::Invalid(UNDEFINED.into()),
            TrigoError::Unported { upstream, unit } => Self::Unported { upstream, unit },
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
fn read_bool(v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| "expected boolean".into())
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
fn has_nan(p: Vector2D) -> bool {
    p.x.is_nan() || p.y.is_nan()
}

fn run(input: &Value) -> Result<Value, Error> {
    let v = object(input)?;
    let op = text(v, "op")?;
    match op {
        "point_on_segment" => {
            keys(v, &["op", "start", "end", "point"])?;
            let start = read_point_i(get(v, "start")?)?;
            let end = read_point_i(get(v, "end")?)?;
            let point = read_point_i(get(v, "point")?)?;
            Ok(json!({"value": trigo::is_point_on_segment(start, end, point)?}))
        }
        "segment_intersects" => {
            // "point" is null for the default nullptr, otherwise the output's initial value.
            keys(v, &["op", "a1", "a2", "b1", "b2", "point"])?;
            let a1 = read_point_i(get(v, "a1")?)?;
            let a2 = read_point_i(get(v, "a2")?)?;
            let b1 = read_point_i(get(v, "b1")?)?;
            let b2 = read_point_i(get(v, "b2")?)?;
            let point = get(v, "point")?;
            if point.is_null() {
                let value = trigo::segment_intersects_segment(a1, a2, b1, b2, None)?;
                return Ok(json!({"value": value, "point": null}));
            }
            let mut point = read_point_i(point)?;
            let value = trigo::segment_intersects_segment(a1, a2, b1, b2, Some(&mut point))?;
            Ok(json!({"value": value, "point": point_i(point)}))
        }
        "segment_hit" => {
            keys(v, &["op", "ref", "start", "end", "dist"])?;
            let reference = read_point_i(get(v, "ref")?)?;
            let start = read_point_i(get(v, "start")?)?;
            let end = read_point_i(get(v, "end")?)?;
            let dist = read_int(get(v, "dist")?)?;
            Ok(json!({"value": trigo::test_segment_hit(reference, start, end, dist)?}))
        }
        "arc_mid" => {
            // Without "min_arc_angle" the call uses the default argument, true.
            let min_arc = if v.contains_key("min_arc_angle") {
                keys(v, &["op", "start", "end", "center", "min_arc_angle"])?;
                read_bool(get(v, "min_arc_angle")?)?
            } else {
                keys(v, &["op", "start", "end", "center"])?;
                true
            };
            let start = read_point_i(get(v, "start")?)?;
            let end = read_point_i(get(v, "end")?)?;
            let center = read_point_i(get(v, "center")?)?;
            Ok(json!({"point": point_i(trigo::calc_arc_mid(start, end, center, min_arc)?)}))
        }
        "rotate_i" => {
            keys(v, &["op", "point", "angle"])?;
            let mut p = read_point_i(get(v, "point")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            trigo::rotate_point_i(&mut p.x, &mut p.y, angle)?;
            Ok(json!({"point": point_i(p)}))
        }
        "rotate_i_about" => {
            keys(v, &["op", "point", "center", "angle"])?;
            let mut p = read_point_i(get(v, "point")?)?;
            let c = read_point_i(get(v, "center")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            trigo::rotate_point_i_about(&mut p.x, &mut p.y, c.x, c.y, angle)?;
            Ok(json!({"point": point_i(p)}))
        }
        "rotate_d" => {
            keys(v, &["op", "point", "angle"])?;
            let mut p = read_point_d(get(v, "point")?)?;
            trigo::rotate_point_d(&mut p.x, &mut p.y, read_angle(get(v, "angle")?)?);
            Ok(json!({"point": point_d(p)}))
        }
        "rotate_d_about" => {
            keys(v, &["op", "point", "center", "angle"])?;
            let mut p = read_point_d(get(v, "point")?)?;
            let c = read_point_d(get(v, "center")?)?;
            trigo::rotate_point_d_about(
                &mut p.x,
                &mut p.y,
                c.x,
                c.y,
                read_angle(get(v, "angle")?)?,
            );
            Ok(json!({"point": point_d(p)}))
        }
        "arc_center_angle" => {
            keys(v, &["op", "start", "end", "angle"])?;
            let start = read_point_d(get(v, "start")?)?;
            let end = read_point_d(get(v, "end")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            Ok(json!({"center": point_d(trigo::calc_arc_center_angle(start, end, angle)?)}))
        }
        "arc_center_d" => {
            keys(v, &["op", "start", "mid", "end"])?;
            let start = read_point_d(get(v, "start")?)?;
            let mid = read_point_d(get(v, "mid")?)?;
            let end = read_point_d(get(v, "end")?)?;
            if has_nan(start) || has_nan(mid) || has_nan(end) {
                return Err(Error::Invalid(UNDEFINED.into()));
            }
            Ok(json!({"center": point_d(trigo::calc_arc_center_d(start, mid, end)?)}))
        }
        "arc_center_i" | "is_straight_arc" => {
            keys(v, &["op", "start", "mid", "end"])?;
            let start = read_point_i(get(v, "start")?)?;
            let mid = read_point_i(get(v, "mid")?)?;
            let end = read_point_i(get(v, "end")?)?;
            if op == "arc_center_i" {
                return Ok(json!({"center": point_i(trigo::calc_arc_center_i(start, mid, end)?)}));
            }
            Ok(json!({"value": trigo::is_straight_arc(start, mid, end)?}))
        }
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
