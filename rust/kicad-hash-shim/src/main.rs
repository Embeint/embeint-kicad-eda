// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Native JSONL adapter for unit `libs.kimath.include.application.pure.p001`.
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
        Err(message) => failure(unit, case, "invalid_input", &message),
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
    use super::{dispatch, UNIT};
    use serde_json::{json, Value};

    fn reply(input: Value) -> Value {
        dispatch(&json!({"unit": UNIT, "case": "t", "input": input}).to_string())
    }

    fn message(input: Value) -> Option<String> {
        reply(input)["error"]["message"].as_str().map(str::to_owned)
    }

    #[test]
    fn mmh3_digest_of_foo() {
        let value = reply(json!({"op":"mmh3","steps":[
            {"op":"new"},{"op":"string","hex":"666f6f"},{"op":"digest"}]}));
        assert_eq!(
            value["result"]["events"][2]["digest"]["string"],
            "E271865701F545617EAF87E42BBA7D87"
        );
    }

    #[test]
    fn malformed_inputs_are_refused_with_the_oracle_messages() {
        assert_eq!(
            message(json!({"op":"rotl","x":1,"r":0})).as_deref(),
            Some("integer out of range: r")
        );
        assert_eq!(
            message(json!({"op":"rotl","x":-1,"r":1})).as_deref(),
            Some("integer out of range: x")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"digest"}]})).as_deref(),
            Some("first step must be new")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[]})).as_deref(),
            Some("steps outside 1..4096")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"new"},{"op":"data","hex":"ABCD"}]}))
                .as_deref(),
            Some("invalid hex: hex")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"new","seed":4294967296_u64}]})).as_deref(),
            Some("integer out of range: seed")
        );
        assert_eq!(
            message(json!({"op":"hash_val","values":[
                {"type":"i8","value":1},{"type":"i32","value":1},{"type":"i32","value":1}]}))
            .as_deref(),
            Some("unsupported value type for three values")
        );
        assert_eq!(
            message(json!({"op":"hash_val","values":[{"type":"i8","value":128}]})).as_deref(),
            Some("integer out of range: value")
        );
        assert_eq!(
            message(json!({"op":"hash_val","values":[{"type":"f64","bits":"00"}]})).as_deref(),
            Some("invalid hex: bits")
        );
        assert_eq!(
            message(json!({"op":"transform","a":[1,2,3],"b":null})).as_deref(),
            Some("expected transform: a")
        );
        assert_eq!(
            message(json!({"op":"transform","a":[1,2,3,1.0],"b":null})).as_deref(),
            Some("integer out of range: a")
        );
        assert_eq!(
            message(json!({"op":"md5_valid","steps":[{"op":"new","x":1}]})).as_deref(),
            Some("unexpected or missing field")
        );
        assert_eq!(
            message(json!({"op":"nope"})).as_deref(),
            Some("unknown operation")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"new"},
                {"op":"data","hex":"00","repeat":4097}]}))
            .as_deref(),
            Some("integer out of range: repeat")
        );
    }

    #[test]
    fn transform_inequality_uses_co_ported_equality() {
        let value = reply(json!({"op":"transform","a":null,"b":[1,0,0,1]}));
        assert_eq!(value["result"]["not_equal"], false);
        assert_eq!(value["result"]["a"], json!([1, 0, 0, 1]));
    }
}
