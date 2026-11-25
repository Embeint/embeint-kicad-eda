// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Native JSONL adapter for unit `libs.kimath.include.math.application.ui.p002`.
//! It mirrors `port/oracle/shims/kimath_header_hash.cpp` operation for operation,
//! including validation order and messages. The schema is in README.md.

mod operations;

use operations::{keys, run, UNIT};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"unit":unit,"case":case,"error":{"kind":kind,"message":message}});
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
    if let Some(value) = request.get("unit").filter(|value| value.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|value| value.is_string()) {
        case = value.clone();
    }
    if keys(&request, &["unit", "case", "input"], &[]).is_err()
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
        Ok(result) => json!({"unit":unit,"case":case,"result":result}),
        Err(operations::Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(operations::Error::Domain(kicad_math::VectorDomainError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"unit":unit,"case":case,"error":{"kind":"unported","upstream":upstream,"unit":owner}})
        }
        Err(operations::Error::Domain(_)) => {
            failure(unit, case, "invalid_input", "undefined vector operation")
        }
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch(&line?))?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::dispatch;
    use serde_json::{json, Value};
    #[test]
    fn undefined_calls_refuse_and_later_requests_survive() -> Result<(), Box<dyn std::error::Error>>
    {
        for line in include_str!("../../../port/oracle/vector-refusals.jsonl").lines() {
            let request: Value = serde_json::from_str(line)?;
            assert_eq!(
                dispatch(line),
                json!({"unit":request["unit"],"case":request["case"],"error":{"kind":"invalid_input","message":"undefined vector operation"}})
            );
            let input = json!({"unit":request["unit"],"case":"after","input":{"dim":2,"type":"i32","op":"new","a":[2,3]}});
            assert_eq!(
                dispatch(&input.to_string())["result"],
                json!({"value":[2,3]})
            );
        }
        let controls: Vec<(Value, String)> = serde_json::from_str(include_str!(
            "../../../port/oracle/vector-invalid-inputs.json"
        ))?;
        for (input, message) in controls {
            let request = json!({"unit":super::UNIT,"case":"invalid","input":input});
            assert_eq!(
                dispatch(&request.to_string())["error"],
                json!({"kind":"invalid_input","message":message})
            );
        }
        Ok(())
    }
}
