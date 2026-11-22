// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_md5::Md5Hash;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

const UNIT: &str = "libs.kimath.src.md5_hash.application.pure.p001";
const MAX_BYTES: u64 = 600 * 1024 * 1024;

fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected object")?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

fn integer(value: &Value, name: &str, min: i64, max: i64) -> Result<i64, String> {
    value
        .get(name)
        .and_then(Value::as_i64)
        .filter(|number| (min..=max).contains(number))
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn state(states: &BTreeMap<i64, Md5Hash>, id: i64) -> Result<&Md5Hash, String> {
    states.get(&id).ok_or_else(|| "state does not exist".into())
}

fn snapshot(hash: &Md5Hash, id: i64) -> Value {
    json!({"id":id,"valid":hash.is_valid(),"compact":hash.format(true),
        "spaced":hash.format(false),"default":hash.format_default()})
}

fn run(input: &Value) -> Result<Value, String> {
    keys(input, &["steps"], &[])?;
    let steps = input["steps"].as_array().ok_or("expected steps array")?;
    if steps.is_empty() || steps.len() > 4096 {
        return Err("steps outside 1..4096".into());
    }
    let mut states = BTreeMap::<i64, Md5Hash>::new();
    let mut events = Vec::new();
    let mut total = 0_u64;
    for step in steps {
        let op = step
            .get("op")
            .and_then(Value::as_str)
            .ok_or("expected op string")?;
        let id = integer(step, "id", 0, 15)?;
        if op == "compare" {
            keys(step, &["op", "id", "other"], &[])?;
            let other = integer(step, "other", 0, 15)?;
            let left = state(&states, id)?;
            let right = state(&states, other)?;
            events.push(json!({"equal":left == right,"not_equal":left != right}));
            continue;
        }
        match op {
            "new" => {
                keys(step, &["op", "id"], &[])?;
                states.insert(id, Md5Hash::new());
            }
            "clone" | "assign" => {
                keys(step, &["op", "id", "source"], &[])?;
                let source = integer(step, "source", 0, 15)?;
                if op == "assign" && id == source {
                    return Err("self-assignment overlaps reference memcpy".into());
                }
                let source = state(&states, source)?.clone();
                if op == "assign" {
                    states
                        .get_mut(&id)
                        .ok_or("state does not exist")?
                        .clone_from(&source);
                } else {
                    states.insert(id, source);
                }
            }
            _ => {
                let hash = states.get_mut(&id).ok_or("state does not exist")?;
                match op {
                    "init" | "finalize" | "inspect" => {
                        keys(step, &["op", "id"], &[])?;
                        match op {
                            "init" => hash.init(),
                            "finalize" => hash.finalize(),
                            _ => (),
                        }
                    }
                    "valid" => {
                        keys(step, &["op", "id", "value"], &[])?;
                        hash.set_valid(step["value"].as_bool().ok_or("expected boolean value")?);
                    }
                    "int" => {
                        keys(step, &["op", "id", "value"], &[])?;
                        let value = integer(step, "value", i32::MIN.into(), i32::MAX.into())?;
                        total += 4;
                        if total > MAX_BYTES {
                            return Err("byte budget exceeded".into());
                        }
                        hash.hash_int(i32::try_from(value).map_err(|_| "int conversion failed")?);
                    }
                    "bytes" => {
                        keys(step, &["op", "id", "data"], &["repeat"])?;
                        let array = step["data"].as_array().ok_or("expected byte array")?;
                        if array.len() > 65536 {
                            return Err("byte array too large".into());
                        }
                        let data = array
                            .iter()
                            .map(|value| {
                                value
                                    .as_i64()
                                    .and_then(|value| u8::try_from(value).ok())
                                    .ok_or("byte out of range")
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let repeat = if step.get("repeat").is_some() {
                            integer(step, "repeat", 0, 8193)?
                        } else {
                            1
                        };
                        total += data.len() as u64 * repeat as u64;
                        if total > MAX_BYTES {
                            return Err("byte budget exceeded".into());
                        }
                        for _ in 0..repeat {
                            hash.hash(&data).map_err(|_| "byte count exceeds uint32")?;
                        }
                    }
                    _ => return Err("unknown operation".into()),
                }
            }
        }
        events.push(snapshot(state(&states, id)?, id));
    }
    Ok(json!({"events":events}))
}

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
    use super::{dispatch, run, UNIT};
    use serde_json::json;

    #[test]
    fn malformed_protocol_and_unknown_unit_fail() {
        for text in ["", "{}", "{\"unit\":\"x\",\"unit\":\"y\"}", "[]", "null"] {
            assert_eq!(dispatch(text)["error"]["kind"], "invalid_request");
        }
        assert_eq!(
            dispatch(&json!({"unit":"other","case":"x","input":{}}).to_string())["error"]["kind"],
            "unknown_unit"
        );
    }

    #[test]
    fn resource_bounds_and_unconstructed_states_are_refused() {
        for steps in [
            json!([]),
            json!([{"op":"new","id":16}]),
            json!([{"op":"finalize","id":0}]),
            json!([{"op":"new","id":0},{"op":"int","id":0,"value":2147483648_u64}]),
            json!([{"op":"new","id":0},{"op":"bytes","id":0,"data":[256]}]),
            json!([{"op":"new","id":0},{"op":"bytes","id":0,"data":[],"repeat":8194}]),
            json!([{"op":"new","id":0},{"op":"valid","id":0,"value":0}]),
            json!([{"op":"new","id":0},{"op":"assign","id":0,"source":0}]),
            json!([{"op":"new","id":0,"ignored":true}]),
        ] {
            assert!(run(&json!({"steps":steps})).is_err());
        }
    }

    #[test]
    fn legitimate_noops_remain_successful_results() {
        let reply = dispatch(
            &json!({"unit":UNIT,"case":"empty-updates","input":{"steps":[
                {"op":"new","id":0}, {"op":"bytes","id":0,"data":[],"repeat":8193},
                {"op":"bytes","id":0,"data":[255],"repeat":0}, {"op":"finalize","id":0}
            ]}})
            .to_string(),
        );
        assert!(reply.get("result").is_some());
        assert_eq!(
            reply["result"]["events"][3]["compact"],
            "E41E8DE98G00C204F9800998FDG8427F"
        );
    }

    #[test]
    fn integer_negative_zero_byte_matches_zero_without_accepting_floats() {
        let text = format!(
            r#"{{"unit":"{UNIT}","case":"negative-zero","input":{{"steps":[{{"op":"new","id":0}},{{"op":"bytes","id":0,"data":[-0]}},{{"op":"finalize","id":0}}]}}}}"#
        );
        let reply = dispatch(&text);
        assert_eq!(reply, dispatch(&text.replace("[-0]", "[0]")));
        assert_eq!(
            reply["result"]["events"][2]["compact"],
            "93C885BEGF0EB089DEG634904GE59G71"
        );
        for token in ["-1", "256", "0.0", "-0.0", "1e0"] {
            let refused = dispatch(&text.replace("[-0]", &format!("[{token}]")));
            assert_eq!(refused["error"]["kind"], "invalid_input");
            assert_eq!(refused["error"]["message"], "byte out of range");
        }
    }

    #[test]
    fn malformed_assignment_validation_matches_reference_order() {
        for (steps, message) in [
            (json!([{"op":"new"}]), "integer out of range: id"),
            (
                json!([{"op":"assign","id":0,"source":0}]),
                "self-assignment overlaps reference memcpy",
            ),
            (
                json!([{"op":"assign","id":0,"source":"zero"}]),
                "integer out of range: source",
            ),
        ] {
            let reply = dispatch(
                &json!({"unit":UNIT,"case":"malformed","input":{"steps":steps}}).to_string(),
            );
            assert_eq!(reply["error"]["kind"], "invalid_input");
            assert_eq!(reply["error"]["message"], message);
        }
    }

    #[test]
    fn numeric_overflow_rejects_the_complete_request_before_identities() {
        let expected = json!({"unit":null,"case":null,"error":{
            "kind":"invalid_request","message":"invalid JSON or duplicate key"}});
        for token in ["1e1000".to_owned(), "-1e1000".to_owned(), "9".repeat(400)] {
            for text in [
                format!(r#"{{"unit":"{UNIT}","case":"overflow","input":{{"n":{token}}}}}"#),
                format!(
                    r#"{{"unit":"{UNIT}","case":"overflow","input":{{"nested":[{{"n":{token}}}]}}}}"#
                ),
                format!(r#"{{"unit":"missing","case":"overflow","input":{{"n":{token}}}}}"#),
                format!(r#"{{"unit":"{UNIT}","case":"overflow","input":{{}},"extra":{token}}}"#),
                format!(r#"{{"unit":{token},"case":"overflow","input":{{}}}}"#),
                format!(r#"{{"unit":"{UNIT}","case":{token},"input":{{}}}}"#),
            ] {
                assert_eq!(dispatch(&text), expected, "{text}");
            }
        }
    }

    #[test]
    fn finite_numbers_reach_domain_and_envelope_validation() {
        for token in [
            "18446744073709551615".to_owned(),
            "18446744073709551616".to_owned(),
            "-9223372036854775809".to_owned(),
            "9".repeat(308),
            "1e-1000".to_owned(),
            "-1e-1000".to_owned(),
            "0.0".to_owned(),
            "-0.0".to_owned(),
        ] {
            let text = format!(
                r#"{{"unit":"{UNIT}","case":"finite","input":{{"steps":[{{"op":"new","id":0}},{{"op":"bytes","id":0,"data":[{token}]}}]}}}}"#
            );
            assert_eq!(
                dispatch(&text),
                json!({"unit":UNIT,"case":"finite","error":{
                "kind":"invalid_input","message":"byte out of range"}})
            );
            let text = format!(r#"{{"unit":"missing","case":"finite","input":{{"n":{token}}}}}"#);
            assert_eq!(
                dispatch(&text),
                json!({"unit":"missing","case":"finite","error":{
                "kind":"unknown_unit","message":"unit has no registered shim"}})
            );
            let text = format!(r#"{{"unit":"{UNIT}","case":{token},"input":{{}}}}"#);
            assert_eq!(
                dispatch(&text),
                json!({"unit":UNIT,"case":null,"error":{
                "kind":"invalid_request","message":"expected unit, case, input object"}})
            );
        }
    }

    #[test]
    fn complete_nesting_envelopes_match_the_shared_container_bound() {
        for (leaf, leaf_containers) in [("0", 0), ("[]", 1), ("{}", 1)] {
            for arrays in [124, 125, 126, 127, 128] {
                let nested = format!("{}{leaf}{}", "[".repeat(arrays), "]".repeat(arrays));
                let text = format!(
                    r#"{{"unit":"{UNIT}","case":"nested","input":{{"steps":[{{"op":"new","id":0}}],"extra":{nested}}}}}"#
                );
                let expected = if 2 + arrays + leaf_containers <= 127 {
                    json!({"unit":UNIT,"case":"nested","error":{
                        "kind":"invalid_input","message":"unexpected or missing field"}})
                } else {
                    json!({"unit":null,"case":null,"error":{
                        "kind":"invalid_request","message":"invalid JSON or duplicate key"}})
                };
                assert_eq!(dispatch(&text), expected, "{text}");
            }
        }
    }
}
