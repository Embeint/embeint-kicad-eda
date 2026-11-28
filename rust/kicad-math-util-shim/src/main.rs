// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_math::math_util::{
    self, FormatPiece, LogArgument, LogEvent, MathUtilError, RecordingLog, UNIT, WX_LOG_DEBUG,
    WX_LOG_MAX,
};
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
        .and_then(|value| {
            // Arbitrary-precision parsing preserves integer -0. The C++ adapter accepts
            // signed integer zero too; floating-point spellings must still be refused.
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|signed| u64::try_from(signed).ok()))
        })
        .ok_or_else(|| format!("integer out of range: {name}"))
}
fn text<'a>(input: &'a Value, name: &str) -> Result<&'a str, String> {
    input
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn bits(input: &Value, name: &str) -> Result<f64, String> {
    let text = text(input, name)?;
    if text.len() != 16
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("invalid hex: {name}"));
    }
    u64::from_str_radix(text, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("invalid hex: {name}"))
}
/// The adapter's closed format grammar: printable ASCII and newline text, `%%`, `%lld`, `%f`.
fn format_pieces(format: &str) -> Result<Vec<FormatPiece<'_>>, String> {
    if !format
        .bytes()
        .all(|b| (0x20..=0x7e).contains(&b) || b == b'\n')
    {
        return Err("unsupported format".into());
    }
    math_util::parse_format(format).ok_or_else(|| "unsupported format".into())
}
/// `args`: up to four `{"lld": i64}` or `{"f": hex bits}` objects, as the oracle builds them.
fn log_args(input: &Value) -> Result<Vec<LogArgument<'static>>, String> {
    let list = input
        .get("args")
        .and_then(Value::as_array)
        .filter(|list| list.len() <= 4)
        .ok_or("invalid args")?;
    list.iter()
        .map(|arg| match arg.as_object() {
            Some(object) if object.len() == 1 && object.contains_key("lld") => {
                signed(arg, "lld", i64::MIN, i64::MAX).map(LogArgument::Signed)
            }
            Some(object) if object.len() == 1 && object.contains_key("f") => {
                bits(arg, "f").map(LogArgument::Float)
            }
            _ => Err("invalid args".into()),
        })
        .collect()
}
/// Every conversion has an argument of its type; excess arguments are ignored.
fn args_match(pieces: &[FormatPiece<'_>], args: &[LogArgument<'_>]) -> bool {
    let mut next = args.iter();
    pieces.iter().all(|piece| match piece {
        FormatPiece::LongLong => matches!(next.next(), Some(LogArgument::Signed(_))),
        FormatPiece::Double => matches!(next.next(), Some(LogArgument::Float(_))),
        FormatPiece::Literal(_) | FormatPiece::Percent => true,
    })
}
/// The recorded wx host calls, as the oracle reports them.
fn log_json(events: &[LogEvent]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|event| match event {
                LogEvent::Assert(call) => json!({"call": "wxOnAssert", "line": call.line,
                    "function": call.function, "condition": call.condition,
                    "message": call.message}),
                LogEvent::Log(call) => json!({"call": "wxLog", "level": call.level,
                    "message": call.message, "line": call.line, "function": call.function,
                    "component": call.component}),
            })
            .collect(),
    )
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
            keys(input, &["op", "format", "args", "log_level", "logging"])?;
            let format = text(input, "format")?;
            let pieces = format_pieces(format)?;
            let args = log_args(input)?;
            let level = signed(input, "log_level", 0, WX_LOG_MAX.into())? as u32;
            let logging = input
                .get("logging")
                .and_then(Value::as_bool)
                .ok_or_else(|| Error::Invalid("expected boolean: logging".into()))?;
            // Formatting happens only past both level queries; a mismatch is undefined there.
            if logging && level >= WX_LOG_DEBUG && !args_match(&pieces, &args) {
                return Err(Error::Invalid("undefined format arguments".into()));
            }
            let log = RecordingLog::new(level, logging);
            math_util::kimath_log_debug(&log, format, &args)?;
            Ok(json!({"kimath_log": log_json(&log.events())}))
        }
        "log_overflow" => {
            keys(input, &["op", "bits", "type_name"])?;
            let value = bits(input, "bits")?;
            let type_name = text(input, "type_name")?;
            let log = RecordingLog::default();
            math_util::kimath_log_overflow(&log, value, type_name)?;
            Ok(json!({"kimath_log": log_json(&log.events())}))
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
                MathUtilError::UndefinedFormat => "undefined format arguments",
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
    fn literal_integer_negative_zero_is_valid_but_float_zero_is_not() {
        for (input, value) in [
            (r#"{"op":"hypot_wide","x":-0,"y":4}"#, 4),
            (r#"{"op":"hypot_wide","x":3,"y":-0}"#, 3),
            (r#"{"op":"hypot_wide","x":-0,"y":-0}"#, 0),
        ] {
            let request = format!(r#"{{"unit":"{UNIT}","case":"literal","input":{input}}}"#);
            assert_eq!(
                dispatch(&request),
                json!({"unit":UNIT,"case":"literal","result":{"value":value}})
            );
        }
        for input in [
            r#"{"op":"hypot_wide","x":-0.0,"y":4}"#,
            r#"{"op":"hypot_wide","x":3,"y":-0.0}"#,
            r#"{"op":"hypot_wide","x":-1,"y":4}"#,
            r#"{"op":"hypot_wide","x":3,"y":-1}"#,
        ] {
            let request = format!(r#"{{"unit":"{UNIT}","case":"literal","input":{input}}}"#);
            assert_eq!(dispatch(&request)["error"]["kind"], "invalid_input");
        }
    }

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
        assert_eq!(counts, [10656 + 949, 0, 0]);
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
        assert_eq!((refusals, deferrals), (453, 0));
        Ok(())
    }
}
