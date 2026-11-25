// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.arc_chord_params.application.pure.p001`. It
//! mirrors `port/oracle/shims/kimath_src_arc_chord_params.cpp`: 32-bit integer points in,
//! every observable value out as exact IEEE bits.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_math::arc_chord_params::{ArcChordParams, UNIT};
use kicad_math::eda_angle::GeometryError;
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Value};

/// Bounds the number of `Compute` calls applied to one object in a request.
const MAX_COMPUTES: usize = 8;

enum Error {
    Invalid(String),
    Arithmetic(GeometryError),
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
impl From<GeometryError> for Error {
    fn from(v: GeometryError) -> Self {
        Self::Arithmetic(v)
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
fn array<'a>(v: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    v.get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected array: {name}"))
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn vector(v: Vector2D) -> Value {
    json!([bits(v.x), bits(v.y)])
}
fn point(v: &Value) -> Result<Vector2I, String> {
    let a = v
        .as_array()
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
fn state(p: &ArcChordParams) -> Result<Value, Error> {
    Ok(json!({
        "valid": p.is_valid(),
        "chord_length": bits(p.get_chord_length()),
        "half_chord": bits(p.get_half_chord()),
        "chord_unit": vector(p.get_chord_unit_vec()),
        "normal_unit": vector(p.get_normal_unit_vec()),
        "sagitta": bits(p.get_sagitta()),
        "radius": bits(p.get_radius()),
        "center_offset": bits(p.get_center_offset()),
        "chord_midpoint": vector(p.get_chord_midpoint()),
        "raw": [bits(p.get_ux()), bits(p.get_uy()), bits(p.get_nx()), bits(p.get_ny()),
                bits(p.get_mid_x()), bits(p.get_mid_y())],
        "center": vector(p.get_center_point()),
        "start_angle": bits(p.get_start_angle()?.as_degrees()),
        "end_angle": bits(p.get_end_angle()?.as_degrees()),
        "arc_angle": bits(p.get_arc_angle()),
    }))
}
fn run(v: &Value) -> Result<Value, Error> {
    match text(v, "op")? {
        "default" => {
            keys(v, &["op"])?;
            state(&ArcChordParams::default())
        }
        "compute" => {
            // One object receives every Compute in order, as in the C++ adapter.
            keys(v, &["op", "computes"])?;
            let computes = array(v, "computes")?;
            if computes.is_empty() || computes.len() > MAX_COMPUTES {
                return Err("computes out of range".into());
            }
            let mut params = ArcChordParams::default();
            let mut steps = Vec::new();
            for points in computes {
                let points = points
                    .as_array()
                    .filter(|p| p.len() == 3)
                    .ok_or("expected start, mid, end")?;
                let mut read = points.iter().map(point);
                let mut next = || read.next().ok_or("expected start, mid, end");
                let start = next()??;
                let mid = next()??;
                let end = next()??;
                let returned = params.compute(&start, &mid, &end);
                steps.push(json!({"returned": returned, "state": state(&params)?}));
            }
            Ok(Value::Array(steps))
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
        Err(Error::Arithmetic(GeometryError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
        Err(Error::Arithmetic(_)) => failure(unit, case, "invalid_input", "intermediate overflow"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CASES: usize = 1612;
    #[test]
    fn every_public_case_succeeds_without_deferral() {
        let mut count = 0;
        for line in include_str!(
            "../../../port/cases/libs.kimath.src.geometry.arc_chord_params.application.pure.p001.jsonl"
        )
        .lines()
        {
            let reply = dispatch(line);
            assert!(reply.get("result").is_some(), "{reply}");
            count += 1;
        }
        assert_eq!(count, CASES);
    }
    #[test]
    fn malformed_inputs_are_refused_before_any_operation() {
        let p = json!([[0, 0], [1, 1], [2, 0]]);
        for (input, message) in [
            (
                json!({"op": "default", "x": 1}),
                "unexpected or missing field",
            ),
            (json!({"op": "compute"}), "unexpected or missing field"),
            (
                json!({"op": "compute", "computes": []}),
                "computes out of range",
            ),
            (
                json!({"op": "compute", "computes": vec![p.clone(); 9]}),
                "computes out of range",
            ),
            (
                json!({"op": "compute", "computes": 1}),
                "expected array: computes",
            ),
            (
                json!({"op": "compute", "computes": [[[0, 0], [1, 1]]]}),
                "expected start, mid, end",
            ),
            (
                json!({"op": "compute", "computes": [[[0, 0], [1, 1], [2147483648i64, 0]]]}),
                "point out of range",
            ),
            (
                json!({"op": "compute", "computes": [[[0, 0], [1.0, 1], [2, 0]]]}),
                "point out of range",
            ),
            (
                json!({"op": "compute", "computes": [[[0, 0], [1], [2, 0]]]}),
                "expected point",
            ),
            (json!({"op": "turn"}), "unknown operation"),
        ] {
            let reply = dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
            assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
            assert_eq!(reply["error"]["message"], message, "{reply}");
        }
        let reply = dispatch(
            &json!({"unit": UNIT, "case": "c", "input": {"op": "compute", "computes": vec![p; 8]}})
                .to_string(),
        );
        assert!(
            reply["result"].as_array().is_some_and(|s| s.len() == 8),
            "{reply}"
        );
    }
}
