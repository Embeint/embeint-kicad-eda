// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.arc_construction.application.ui.p001`. It mirrors
//! `port/oracle/shims/kimath_src_arc_construction.cpp`: 32-bit integer points and exact IEEE
//! tangent bits in, every `ARC_SOLUTION` field out (doubles as exact IEEE bits).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_math::arc_construction::{
    arc_from_start_end_center_drag, arc_from_start_end_mid_drag, arc_from_start_tangent_end,
    arc_through_points, project_to_chord_bisector, ArcSolution, UNIT,
};
use kicad_math::{Vector2, Vector2D, Vector2I, VectorDomainError};
use serde_json::{json, Value};

enum Error {
    Invalid(String),
    Domain(VectorDomainError),
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
impl From<VectorDomainError> for Error {
    fn from(v: VectorDomainError) -> Self {
        Self::Domain(v)
    }
}
fn keys(v: &Value, fields: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str, String> {
    v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn flag(v: &Value, name: &str) -> Result<bool, String> {
    v.get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("expected boolean: {name}"))
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn int_point(v: Vector2I) -> Value {
    json!([v.x, v.y])
}
fn point(v: &Value, name: &str) -> Result<Vector2I, String> {
    let a = v
        .get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    let mut values = a.iter().map(|n| {
        n.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| "point out of range".to_string())
    });
    Ok(Vector2::new(
        values.next().ok_or("expected point")??,
        values.next().ok_or("expected point")??,
    ))
}
/// A double given as exactly 16 lower-case hexadecimal digits of its IEEE bits.
fn double_bits(v: &Value) -> Result<f64, String> {
    let s = v.as_str().ok_or("expected double bits")?;
    if s.len() != 16
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("expected double bits".into());
    }
    u64::from_str_radix(s, 16)
        .map(f64::from_bits)
        .map_err(|_| "expected double bits".into())
}
fn double_vector(v: &Value, name: &str) -> Result<Vector2D, String> {
    let a = v
        .get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .ok_or("expected vector")?;
    let mut values = a.iter().map(double_bits);
    Ok(Vector2::new(
        values.next().ok_or("expected vector")??,
        values.next().ok_or("expected vector")??,
    ))
}
fn solution(s: &ArcSolution) -> Value {
    json!({
        "valid": s.valid,
        "start": int_point(s.start),
        "mid": int_point(s.mid),
        "end": int_point(s.end),
        "center": [bits(s.center.x), bits(s.center.y)],
        "radius": bits(s.radius),
    })
}
fn run(v: &Value) -> Result<Value, Error> {
    match text(v, "op")? {
        "project" => {
            keys(v, &["op", "start", "end", "cursor"])?;
            let (start, end, cursor) = (point(v, "start")?, point(v, "end")?, point(v, "cursor")?);
            Ok(int_point(project_to_chord_bisector(start, end, cursor)?))
        }
        "through" => {
            keys(v, &["op", "start", "on_arc", "end"])?;
            let (start, on_arc, end) = (point(v, "start")?, point(v, "on_arc")?, point(v, "end")?);
            Ok(solution(&arc_through_points(start, on_arc, end)?))
        }
        "mid_drag" => {
            keys(v, &["op", "start", "end", "cursor"])?;
            let (start, end, cursor) = (point(v, "start")?, point(v, "end")?, point(v, "cursor")?);
            Ok(solution(&arc_from_start_end_mid_drag(start, end, cursor)?))
        }
        "center_drag" => {
            keys(v, &["op", "start", "end", "cursor", "major"])?;
            let (start, end, cursor) = (point(v, "start")?, point(v, "end")?, point(v, "cursor")?);
            let major = flag(v, "major")?;
            Ok(solution(&arc_from_start_end_center_drag(
                start, end, cursor, major,
            )?))
        }
        "tangent" => {
            keys(v, &["op", "start", "tangent", "end"])?;
            let start = point(v, "start")?;
            let tangent = double_vector(v, "tangent")?;
            let end = point(v, "end")?;
            Ok(solution(&arc_from_start_tangent_end(start, tangent, end)?))
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
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(Error::Domain(VectorDomainError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
        // No other vector refusal is reachable from these bodies; if one ever is, it is reported
        // (and fails the exact comparison) rather than hidden.
        Err(Error::Domain(e)) => failure(unit, case, "domain", &e.to_string()),
    }
}

#[cfg(test)]
mod tests;
