// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.application.ui.p001` (the inline bodies of trigo.h).
//! It mirrors `port/oracle/shims/trigo_header_p001.cpp`: `int` and `long long` values as JSON
//! integers, `double` values as exact IEEE bits, angles as the bits of their value in degrees,
//! one operation per request, and the same refusals: `invalid_input` "undefined trigo.h
//! operation" where the pinned evaluation is undefined, and "outside the verified domain" for a
//! `long long` template angle beyond the adapter's loop bound (a harness bound, not an undefined
//! domain).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::eda_angle::{AngleUnit, EdaAngle};
use kicad_math::trigo::TrigoError;
use kicad_math::trigo_header::{self as header, UNIT};
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined trigo.h operation";

/// The largest `long long` angle magnitude the oracle adapter evaluates (`3600LL << 24`, at most
/// 2^24 passes of the compiled template loops).
pub const LOOP_BOUND: i64 = 3600 << 24;

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
fn read_int(v: &Value) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| "integer out of range".into())
}
fn read_long_long(v: &Value) -> Result<i64, String> {
    v.as_i64().ok_or_else(|| "integer out of range".into())
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
/// The template's integer type: `false` for "int", `true` for "long long".
fn read_wide(v: &Map<String, Value>) -> Result<bool, String> {
    match text(v, "type")? {
        "int" => Ok(false),
        "long long" => Ok(true),
        _ => Err("unknown type".into()),
    }
}
fn bounded_long_long(v: &Value) -> Result<i64, String> {
    let angle = read_long_long(v)?;
    if !(-LOOP_BOUND..=LOOP_BOUND).contains(&angle) {
        return Err("outside the verified domain".into());
    }
    Ok(angle)
}

/// One template operation on either instantiation.
fn template<T: header::AngleInt>(op: &str, angle: T) -> T {
    match op {
        "normalize_angle_pos" => header::normalize_angle_pos(angle),
        "normalize_angle_180" => header::normalize_angle_180(angle),
        _ => {
            let mut angle = angle;
            header::normalize_angle_pos_in_place(&mut angle);
            angle
        }
    }
}

fn run(input: &Value) -> Result<Value, Error> {
    let v = object(input)?;
    let op = text(v, "op")?;
    match op {
        "rotate_i" | "get_rotated_i" => {
            keys(v, &["op", "point", "angle"])?;
            let mut p = read_point_i(get(v, "point")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            if op == "get_rotated_i" {
                return Ok(json!({"point": point_i(header::get_rotated(p, angle)?)}));
            }
            header::rotate_point(&mut p, angle)?;
            Ok(json!({"point": point_i(p)}))
        }
        "rotate_i_about" | "get_rotated_i_about" => {
            keys(v, &["op", "point", "center", "angle"])?;
            let mut p = read_point_i(get(v, "point")?)?;
            let c = read_point_i(get(v, "center")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            if op == "get_rotated_i_about" {
                return Ok(json!({"point": point_i(header::get_rotated_about(p, c, angle)?)}));
            }
            header::rotate_point_about(&mut p, c, angle)?;
            Ok(json!({"point": point_i(p)}))
        }
        "rotate_d" | "get_rotated_d" => {
            keys(v, &["op", "point", "angle"])?;
            let mut p = read_point_d(get(v, "point")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            if op == "get_rotated_d" {
                return Ok(json!({"point": point_d(header::get_rotated_d(p, angle))}));
            }
            header::rotate_point_d(&mut p, angle);
            Ok(json!({"point": point_d(p)}))
        }
        "rotate_d_about" => {
            keys(v, &["op", "point", "center", "angle"])?;
            let mut p = read_point_d(get(v, "point")?)?;
            let c = read_point_d(get(v, "center")?)?;
            header::rotate_point_d_about(&mut p, c, read_angle(get(v, "angle")?)?);
            Ok(json!({"point": point_d(p)}))
        }
        "deg2rad" | "rad2deg" | "rad2decideg" => {
            keys(v, &["op", "value"])?;
            let value = decode_bits(get(v, "value")?)?;
            let result = match op {
                "deg2rad" => header::deg2rad(value),
                "rad2deg" => header::rad2deg(value),
                _ => header::rad2decideg(value),
            };
            Ok(json!({"value": bits(result)}))
        }
        "normalize_angle_pos" | "normalize_angle_pos_ref" | "normalize_angle_180" => {
            keys(v, &["op", "type", "angle"])?;
            if read_wide(v)? {
                let angle = bounded_long_long(get(v, "angle")?)?;
                return Ok(json!({"angle": template(op, angle)}));
            }
            let angle = read_int(get(v, "angle")?)?;
            Ok(json!({"angle": template(op, angle)}))
        }
        "intercepts_positive_x" | "intercepts_negative_x" => {
            keys(v, &["op", "start", "end"])?;
            let start = decode_bits(get(v, "start")?)?;
            let end = decode_bits(get(v, "end")?)?;
            let value = if op == "intercepts_positive_x" {
                header::intercepts_positive_x(start, end)
            } else {
                header::intercepts_negative_x(start, end)
            };
            Ok(json!({"value": value}))
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
