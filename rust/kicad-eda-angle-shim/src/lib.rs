// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.eda_angle.application.pure.p001`. It mirrors
//! `port/oracle/shims/kimath_src_eda_angle.cpp`: exact IEEE bits in, returned values out.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_math::eda_angle::{AngleUnit, EdaAngle};
use kicad_math::eda_angle_source::UNIT;
use serde_json::{json, Value};

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
fn run(v: &Value) -> Result<Value, String> {
    // Same validation order as the C++ adapter: operation text, exact keys, operand.
    let op = text(v, "op")?;
    keys(v, &["op", "a"])?;
    let angle = EdaAngle::new(number(v, "a")?, AngleUnit::Degrees);
    match op {
        "keep_upright" => Ok(bits(angle.keep_upright().as_degrees())),
        "is_cardinal" => Ok(json!(angle.is_cardinal())),
        "is_cardinal90" => Ok(json!(angle.is_cardinal90())),
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
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_public_case_succeeds_without_deferral() {
        let mut count = 0;
        for line in include_str!(
            "../../../port/cases/libs.kimath.src.geometry.eda_angle.application.pure.p001.jsonl"
        )
        .lines()
        {
            let reply = dispatch(line);
            assert!(reply.get("result").is_some(), "{reply}");
            count += 1;
        }
        assert_eq!(count, CASES);
    }
    const CASES: usize = 4704;
    #[test]
    fn malformed_inputs_are_refused_before_any_operation() {
        let unit = UNIT;
        for (input, message) in [
            (json!({"op": "is_cardinal"}), "unexpected or missing field"),
            (
                json!({"op": "is_cardinal", "a": "0", "b": "0"}),
                "unexpected or missing field",
            ),
            (json!({"op": "is_cardinal", "a": "4056800000000000"}), ""),
            (
                json!({"op": "is_cardinal", "a": "4056800000000000x"}),
                "invalid hex: a",
            ),
            (
                json!({"op": "is_cardinal", "a": "4056800000000ABC"}),
                "invalid hex: a",
            ),
            (
                json!({"op": "sideways", "a": "4056800000000000"}),
                "unknown operation",
            ),
            (
                json!({"op": 1, "a": "4056800000000000"}),
                "expected string: op",
            ),
        ] {
            let reply = dispatch(&json!({"unit": unit, "case": "c", "input": input}).to_string());
            if message.is_empty() {
                assert_eq!(reply["result"], json!(true), "{reply}");
            } else {
                assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
                assert_eq!(reply["error"]["message"], message, "{reply}");
            }
        }
    }
}
