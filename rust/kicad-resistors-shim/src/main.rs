// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod operations;
use operations::{field, keys, run};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
const UNIT: &str = "pcb_calculator.application.pure.p001";

fn failure(unit: &Value, case: &Value, kind: &str, message: &str) -> Value {
    json!({"unit":unit,"case":case,"error":{"kind":kind,"message":message}})
}
fn dispatch(line: &str) -> Value {
    let request = match kicad_difftest::request::parse(line) {
        Ok(value) => value,
        Err(message) => return failure(&Value::Null, &Value::Null, "invalid_request", &message),
    };
    let identity = |key| {
        request
            .get(key)
            .filter(|value| value.is_string())
            .cloned()
            .unwrap_or(Value::Null)
    };
    let (unit, case) = (identity("unit"), identity("case"));
    if keys(&request, &["unit", "case", "input"], &[]).is_err()
        || unit.as_str().is_none_or(str::is_empty)
        || case.as_str().is_none_or(str::is_empty)
        || !request.get("input").is_some_and(Value::is_object)
    {
        return failure(
            &unit,
            &case,
            "invalid_request",
            "expected unit, case, input object",
        );
    }
    if unit.as_str() != Some(UNIT) {
        return failure(&unit, &case, "unknown_unit", "unit has no registered shim");
    }
    match field(&request, "input").and_then(run) {
        Ok(result) => json!({"unit":unit,"case":case,"result":result}),
        Err(message) => failure(&unit, &case, "invalid_input", &message),
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        serde_json::to_writer(&mut output, &dispatch(&line?))?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_refusals_compare_complete_envelopes() {
        for (input, message) in [
            (json!({}), "missing field"),
            (json!({"op":3}), "expected string"),
            (
                json!({"op":"reader","readers":[{"text":false,"source":"public"}],"parsers":[],"actions":[]}),
                "expected string",
            ),
            (
                json!({"op":"resistance","values":[{"value_bits":0,"name":"zero","clear_parts":1}]}),
                "expected boolean",
            ),
            (
                json!({"op":"calculator","actions":[{"op":"solution"}]}),
                "missing field",
            ),
        ] {
            let request = json!({"unit":UNIT,"case":"typed","input":input});
            assert_eq!(
                dispatch(&request.to_string()),
                failure(&json!(UNIT), &json!("typed"), "invalid_input", message)
            );
        }
    }

    #[test]
    fn unsigned_integer_minus_zero_is_distinct_from_float_minus_zero() {
        for token in ["-0", "0", "18446744073709551615"] {
            let line=format!("{{\"unit\":\"{UNIT}\",\"case\":\"raw\",\"input\":{{\"op\":\"resistance\",\"values\":[{{\"value_bits\":{token},\"name\":\"raw\"}}]}}}}");
            let value = if token == "18446744073709551615" {
                u64::MAX
            } else {
                0
            };
            assert_eq!(
                dispatch(&line),
                json!({"unit":UNIT,"case":"raw","result":[{"value_bits":value,"name":"raw","parts_bits":[value]}]})
            );
        }
        let line=format!("{{\"unit\":\"{UNIT}\",\"case\":\"raw\",\"input\":{{\"op\":\"format\",\"values_bits\":[-0.0]}}}}");
        assert_eq!(
            dispatch(&line),
            failure(
                &json!(UNIT),
                &json!("raw"),
                "invalid_input",
                "expected u64 IEEE754 bits"
            )
        );
    }

    #[test]
    fn numeric_overflow_is_refused_before_any_identity() {
        for token in ["1e1000".to_owned(), "1".to_owned() + &"0".repeat(399)] {
            for line in [
                format!("{{\"unit\":\"{UNIT}\",\"case\":\"overflow\",\"input\":{{\"op\":\"format\",\"values_bits\":[{token}]}}}}"),
                format!("{{\"unit\":{token},\"case\":\"overflow\",\"input\":{{}}}}"),
                format!("{{\"unit\":\"{UNIT}\",\"case\":{token},\"input\":{{}}}}"),
                format!("{{\"unit\":\"unknown\",\"case\":\"overflow\",\"input\":{{\"op\":\"unknown\",\"nested\":[{{\"value\":{token}}}]}}}}"),
                format!("{{\"unit\":\"{UNIT}\",\"case\":\"overflow\",\"input\":{{}},\"extra\":{token}}}"),
                token.clone(),
            ] {
                assert_eq!(
                    dispatch(&line),
                    failure(
                        &Value::Null,
                        &Value::Null,
                        "invalid_request",
                        "invalid JSON or duplicate key"
                    )
                );
            }
        }
        for token in ["18446744073709551616", "1e308", "1e-1000"] {
            let line=format!("{{\"unit\":\"{UNIT}\",\"case\":\"finite\",\"input\":{{\"op\":\"unknown\",\"value\":{token}}}}}");
            assert_eq!(
                dispatch(&line),
                failure(
                    &json!(UNIT),
                    &json!("finite"),
                    "invalid_input",
                    "unknown operation"
                )
            );
        }
    }

    #[test]
    fn protocol_and_count_refusals_never_become_results() {
        for line in [
            "",
            "{",
            "[]",
            r#"{"unit":"a","unit":"b","case":"c","input":{}}"#,
            r#"{"unit":"a","case":"c","input":{"a":1,"a":2}}"#,
        ] {
            assert!(dispatch(line).get("error").is_some());
        }
        for input in [
            json!({"op":"calculator","actions":[{"op":"calculate"}]}),
            json!({"op":"calculator","actions":[{"op":"find","values_bits":[]}]}),
            json!({"op":"calculator","actions":[{"op":"set_series","series":5},{"op":"new","target_bits":0}]}),
            json!({"op":"format","values_bits":vec![0;4097]}),
            json!({"op":"calculator","actions":vec![json!({"op":"state"});65]}),
            json!({"op":"format","values_bits":[-1]}),
            json!({"op":"series","series":2147483648u64,"decades":[]}),
        ] {
            let request = json!({"unit":UNIT,"case":"refusal","input":input});
            assert_eq!(
                dispatch(&request.to_string())["error"]["kind"],
                "invalid_input"
            );
        }
    }

    #[test]
    fn serde_private_marker_keys_stay_ordinary_objects() {
        let number = r#"{"$serde_json::private::Number":"0"}"#;
        let raw =
            r#"{"$serde_json::private::RawValue":"\"pcb_calculator.application.pure.p001\""}"#;
        let request =
            |input: &str| format!(r#"{{"unit":"{UNIT}","case":"marker","input":{input}}}"#);
        let refused = |message| failure(&json!(UNIT), &json!("marker"), "invalid_input", message);
        for (line, expected) in [
            (
                request(&format!(
                    r#"{{"op":"collector","target_bits":{number},"candidates":[]}}"#
                )),
                refused("expected u64 IEEE754 bits"),
            ),
            (
                request(&format!(r#"{{"op":"format","values_bits":[{number}]}}"#)),
                refused("expected u64 IEEE754 bits"),
            ),
            (
                request(&format!(
                    r#"{{"op":"calculator","actions":[{{"op":"set_series","series":{number}}}]}}"#
                )),
                refused("integer out of range: series"),
            ),
            (
                request(r#"{"$serde_json::private::RawValue":"{\"op\":\"unit_scales\"}"}"#),
                refused("missing field"),
            ),
            (
                request(
                    r#"{"op":"format","values_bits":{"$serde_json::private::RawValue":"[0]"}}"#,
                ),
                refused("array bound exceeded or wrong type"),
            ),
            (
                request(r#"{"op":{"$serde_json::private::RawValue":"\"x\""}}"#),
                refused("expected string"),
            ),
            (
                format!(r#"{{"unit":{raw},"case":"marker","input":{{"op":"unit_scales"}}}}"#),
                failure(
                    &Value::Null,
                    &json!("marker"),
                    "invalid_request",
                    "expected unit, case, input object",
                ),
            ),
            (
                request(
                    r#"{"op":"format","values_bits":[{"$serde_json::private::Number":"bad"}]}"#,
                ),
                refused("expected u64 IEEE754 bits"),
            ),
            (
                request(
                    r#"{"op":"format","values_bits":[{"$serde_json::private::Number":"0","$serde_json::private::Number":"0"}]}"#,
                ),
                failure(
                    &Value::Null,
                    &Value::Null,
                    "invalid_request",
                    "invalid JSON or duplicate key",
                ),
            ),
        ] {
            assert_eq!(dispatch(&line), expected, "{line}");
        }
    }

    #[test]
    fn real_cpp_exception_has_a_result_envelope() {
        let request = json!({"unit":UNIT,"case":"nan","input":{"op":"calculator","actions":[
            {"op":"new","target_bits":f64::NAN.to_bits()},{"op":"calculate"}]}});
        let reply = dispatch(&request.to_string());
        assert!(reply.get("error").is_none());
        assert_eq!(
            reply["result"][1]["returned"]["exception"],
            "Empty solution collector"
        );
    }
}
