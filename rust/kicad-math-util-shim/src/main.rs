// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_math::math_util::{self, MathUtilError, UNIT};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

enum Error {
    Invalid(String),
    Arithmetic(MathUtilError),
}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}
impl From<MathUtilError> for Error {
    fn from(error: MathUtilError) -> Self {
        Self::Arithmetic(error)
    }
}
fn keys(value: &Value, fields: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected object")?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn signed(input: &Value, name: &str, min: i64, max: i64) -> Result<i64, String> {
    input
        .get(name)
        .and_then(Value::as_i64)
        .filter(|v| *v >= min && *v <= max)
        .ok_or_else(|| format!("integer out of range: {name}"))
}
fn unsigned(input: &Value, name: &str) -> Result<u64, String> {
    input
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("integer out of range: {name}"))
}
fn text<'a>(input: &'a Value, name: &str) -> Result<&'a str, String> {
    input
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn bits(input: &Value) -> Result<f64, String> {
    let text = text(input, "bits")?;
    if text.len() != 16
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid hex: bits".into());
    }
    u64::from_str_radix(text, 16)
        .map(f64::from_bits)
        .map_err(|_| "invalid hex: bits".into())
}
fn run(input: &Value) -> Result<Value, Error> {
    let op = text(input, "op")?;
    match op {
        "hypot_wide" => {
            keys(input, &["op", "x", "y"])?;
            let x = unsigned(input, "x")?;
            let y = unsigned(input, "y")?;
            Ok(json!({"value": math_util::rounded_hypot_wide(x, y)}))
        }
        "rescale_i32" => {
            keys(input, &["op", "numerator", "value", "denominator"])?;
            let n = signed(input, "numerator", i32::MIN.into(), i32::MAX.into())? as i32;
            let v = signed(input, "value", i32::MIN.into(), i32::MAX.into())? as i32;
            let d = signed(input, "denominator", i32::MIN.into(), i32::MAX.into())? as i32;
            Ok(json!({"value": math_util::rescale_i32(n, v, d)?}))
        }
        "rescale_i64_int128" => {
            keys(input, &["op", "numerator", "value", "denominator"])?;
            let n = signed(input, "numerator", i64::MIN, i64::MAX)?;
            let v = signed(input, "value", i64::MIN, i64::MAX)?;
            let d = signed(input, "denominator", i64::MIN, i64::MAX)?;
            Ok(json!({"value": math_util::rescale_i64_int128(n, v, d)?}))
        }
        "log_debug" => {
            keys(input, &["op", "format"])?;
            math_util::kimath_log_debug(text(input, "format")?, &[])?;
            Err(Error::Invalid("host hook unexpectedly returned".into()))
        }
        "log_overflow" => {
            keys(input, &["op", "bits", "type_name"])?;
            let value = bits(input)?;
            math_util::kimath_log_overflow(value, text(input, "type_name")?)?;
            Err(Error::Invalid("host hook unexpectedly returned".into()))
        }
        _ => Err(Error::Invalid("unknown operation".into())),
    }
}
fn dispatch(line: &str) -> Value {
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
        Err(Error::Arithmetic(MathUtilError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
        Err(Error::Arithmetic(error)) => failure(
            unit,
            case,
            "invalid_input",
            match error {
                MathUtilError::ZeroDenominator => "zero denominator",
                _ => "intermediate overflow",
            },
        ),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch(&line?))?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corpus_success_refusal_and_deferral_counts_are_explicit() -> Result<(), serde_json::Error> {
        let mut counts = [0; 3];
        for line in
            include_str!("../../../port/cases/libs.kimath.src.math.util.application.ui.p001.jsonl")
                .lines()
        {
            let reply = dispatch(line);
            if reply.get("result").is_some() {
                counts[0] += 1;
            } else if reply["error"]["kind"] == "unported" {
                counts[2] += 1;
            } else {
                counts[1] += 1;
            }
        }
        assert_eq!(counts, [10656, 0, 0]);
        let controls = include_str!("../../../port/oracle/fixtures/math-util-controls.jsonl");
        let mut refusals = 0;
        let mut deferrals = 0;
        for line in controls.lines() {
            let control: Value = serde_json::from_str(line)?;
            let reply = dispatch(&control["request"].to_string());
            assert_eq!(reply["error"], control["error"], "{}", control["request"]);
            if reply["error"]["kind"] == "unported" {
                deferrals += 1;
            } else {
                refusals += 1;
            }
        }
        assert_eq!((refusals, deferrals), (411, 7));
        Ok(())
    }
}
