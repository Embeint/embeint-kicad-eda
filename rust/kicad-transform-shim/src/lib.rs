// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.transform.application.ui.p001`. It mirrors
//! `port/oracle/shims/kimath_src_transform.cpp`: integer matrices, points and boxes and exact
//! IEEE angle bits in, returned values out. Inputs whose reference evaluation is undefined are
//! refused by the port itself with the same message the C++ adapter's guard reports.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_hash::transform_source::{TransformError, UNIT};
use kicad_hash::{StdHash, Transform};
use kicad_math::box2::Box2I;
use kicad_math::eda_angle::{AngleUnit, EdaAngle};
use kicad_math::{Vector2I, Vector2L};
use serde_json::{json, Value};

/// Refusal messages shared with the C++ adapter.
pub const UNDEFINED_OVERFLOW: &str = "undefined integer overflow";
pub const UNDEFINED_DIVISION: &str = "undefined division by zero";

enum Failure {
    Invalid(String),
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::Invalid(message.into())
    }
}

impl From<TransformError> for Failure {
    fn from(error: TransformError) -> Self {
        match error {
            TransformError::IntegerOverflow => Self::Invalid(UNDEFINED_OVERFLOW.into()),
            TransformError::DivisionByZero => Self::Invalid(UNDEFINED_DIVISION.into()),
            TransformError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            other => Self::Invalid(format!("refused: {other:?}")),
        }
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

/// A fixed-length array of integers in `[min, max]`, validated like the C++ adapter.
fn integers<const N: usize>(v: &Value, name: &str, min: i64, max: i64) -> Result<[i64; N], String> {
    let items = v
        .get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() == N)
        .ok_or_else(|| format!("expected integer array: {name}"))?;
    let mut out = [0_i64; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = item
            .as_i64()
            .filter(|n| (min..=max).contains(n))
            .ok_or_else(|| format!("integer out of range: {name}"))?;
    }
    Ok(out)
}

fn int32(value: i64) -> Result<i32, String> {
    i32::try_from(value).map_err(|_| "integer out of range".to_string())
}

fn transform(v: &Value, name: &str) -> Result<Transform, String> {
    let [x1, y1, x2, y2] = integers::<4>(v, name, i64::from(i32::MIN), i64::from(i32::MAX))?;
    Ok(Transform::new(
        int32(x1)?,
        int32(y1)?,
        int32(x2)?,
        int32(y2)?,
    ))
}

fn point(v: &Value, name: &str) -> Result<Vector2I, String> {
    let [x, y] = integers::<2>(v, name, i64::from(i32::MIN), i64::from(i32::MAX))?;
    Ok(Vector2I::new(int32(x)?, int32(y)?))
}

fn number(v: &Value, name: &str) -> Result<f64, String> {
    let s = text(v, name)?;
    if s.len() != 16
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("invalid hex: {name}"));
    }
    u64::from_str_radix(s, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("invalid hex: {name}"))
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

fn run(v: &Value) -> Result<Value, Failure> {
    // Same validation order as the C++ adapter: operation text, exact keys, operands.
    let op = text(v, "op")?;
    match op {
        "eq" => {
            keys(v, &["op", "a", "b"])?;
            let a = transform(v, "a")?;
            let b = transform(v, "b")?;
            Ok(json!(a == b))
        }
        "hash" => {
            keys(v, &["op", "t"])?;
            Ok(json!(transform(v, "t")?.std_hash().to_string()))
        }
        "point" => {
            keys(v, &["op", "t", "p"])?;
            let t = transform(v, "t")?;
            let p = t.transform_coordinate(point(v, "p")?)?;
            Ok(json!([p.x, p.y]))
        }
        "box" => {
            keys(v, &["op", "t", "origin", "size"])?;
            let t = transform(v, "t")?;
            let origin = point(v, "origin")?;
            let [w, h] = integers::<2>(v, "size", i64::MIN, i64::MAX)?;
            let mut rect = Box2I::default();
            rect.set_origin(origin);
            rect.set_size(Vector2L::new(w, h));
            let out = t.transform_box(&rect)?;
            let (o, s) = (out.origin(), out.size());
            Ok(json!({"origin": [o.x, o.y], "size": [s.x, s.y], "valid": out.is_valid()}))
        }
        "inverse" => {
            keys(v, &["op", "t"])?;
            let inv = transform(v, "t")?.inverse_transform()?;
            Ok(json!([inv.x1, inv.y1, inv.x2, inv.y2]))
        }
        "map_angles" => {
            keys(v, &["op", "t", "a1", "a2"])?;
            let t = transform(v, "t")?;
            let mut a1 = EdaAngle::new(number(v, "a1")?, AngleUnit::Degrees);
            let mut a2 = EdaAngle::new(number(v, "a2")?, AngleUnit::Degrees);
            let swap = t.map_angles(&mut a1, &mut a2)?;
            Ok(json!({"swap": swap, "a1": bits(a1.as_degrees()), "a2": bits(a2.as_degrees())}))
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
        Err(Failure::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(Failure::Unported {
            upstream,
            unit: owner,
        }) => json!({
            "case": case,
            "error": {"kind": "unported", "upstream": upstream, "unit": owner},
            "unit": unit
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASES: &str =
        include_str!("../../../port/cases/libs.kimath.src.transform.application.ui.p001.jsonl");
    const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/transform-controls.jsonl");

    #[test]
    fn every_public_case_succeeds_without_deferral() {
        let mut count = 0;
        for line in CASES.lines() {
            let reply = dispatch(line);
            assert!(reply.get("result").is_some(), "{reply}");
            count += 1;
        }
        assert_eq!(count, PUBLIC_CASES);
    }
    // Kept equal to `generate_cases.py`; the generator --check binds the corpus bytes.
    const PUBLIC_CASES: usize = 7678;

    #[test]
    fn every_control_is_refused_with_its_recorded_message() {
        for line in CONTROLS.lines() {
            let control: Value = serde_json::from_str(line).unwrap_or(Value::Null);
            let request = control["request"].to_string();
            let reply = dispatch(&request);
            assert_eq!(reply["error"], control["error"], "{reply}");
        }
    }

    #[test]
    fn malformed_inputs_are_refused_before_any_operation() {
        for (input, message) in [
            (json!({"op": "hash"}), "unexpected or missing field"),
            (
                json!({"op": "hash", "t": [1, 0, 0]}),
                "expected integer array: t",
            ),
            (
                json!({"op": "hash", "t": [1, 0, 0, 2147483648_i64]}),
                "integer out of range: t",
            ),
            (
                json!({"op": "hash", "t": [1, 0, 0, 1.0]}),
                "integer out of range: t",
            ),
            (
                json!({"op": "map_angles", "t": [1, 0, 0, 1], "a1": "0", "a2": "0"}),
                "invalid hex: a1",
            ),
            (
                json!({"op": "rotate", "t": [1, 0, 0, 1]}),
                "unknown operation",
            ),
            (json!({"op": 1}), "expected string: op"),
        ] {
            let reply = dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
            assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
            assert_eq!(reply["error"]["message"], message, "{reply}");
        }
    }
}
