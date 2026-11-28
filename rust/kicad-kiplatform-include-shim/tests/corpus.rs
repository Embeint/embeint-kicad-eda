// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts explicit and check
//! each reported value against the request that produced it.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_kiplatform_include_shim::dispatch;
use serde_json::{json, Value};

const CASES: &str = include_str!(
    "../../../port/cases/libs.kiplatform.include.kiplatform.application.pure.p001.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/kiplatform-include-p001-controls.jsonl");
const EXPECTED: &str = include_str!("../expected-replies.json");

/// Integers in their canonical spelling, so a request's `-0` equals a reported 0.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Number(n) => n
            .as_i64()
            .map(Value::from)
            .or_else(|| n.as_u64().map(Value::from))
            .unwrap_or_else(|| value.clone()),
        Value::Array(items) => items.iter().map(canonical).collect(),
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), canonical(v))).collect(),
        _ => value.clone(),
    }
}

/// The handler events a step list must produce: a new handler is constructed before the
/// one it replaces is destroyed, and what is left is destroyed in slot order.
fn handler_events(steps: &[Value]) -> Vec<Value> {
    let mut slots: [Option<u64>; 8] = [None; 8];
    let mut events = Vec::new();
    let mut next = 0u64;
    for step in steps {
        let slot = canonical(&step["slot"]).as_u64().unwrap_or(99) as usize;
        let Some(owner) = slots.get_mut(slot) else {
            continue;
        };
        let old = if step["action"] == "create" {
            events.push(json!({"event": "construct", "id": next}));
            let old = owner.replace(next);
            next += 1;
            old
        } else {
            owner.take()
        };
        if let Some(id) = old {
            events.push(json!({"event": "destroy", "id": id}));
        }
    }
    for owner in &mut slots {
        if let Some(id) = owner.take() {
            events.push(json!({"event": "destroy", "id": id}));
        }
    }
    events
}

fn check(input: &Value, result: &Value, id: &Value) {
    let input = canonical(input);
    match input["op"].as_str().unwrap_or_default() {
        "constants" => {
            assert_eq!(result["cloud_sync_buffer_size"], 524_288, "{id}");
            assert_eq!(
                result["file_lock_states"],
                json!({"NONE": 0, "HELD": 1, "BUSY": 2, "UNSUPPORTED": 3}),
                "{id}"
            );
        }
        "value_init" => {
            assert_eq!(
                result["file_lock_default_init"],
                json!({"fd": -1, "state": 0})
            );
            assert_eq!(
                result["file_lock_value_init"],
                json!({"fd": -1, "state": 0})
            );
        }
        "target_attrs" => {
            let captured = input.get("captured").cloned().unwrap_or(json!(false));
            let attrs = json!({"value": input["value"], "captured": captured});
            assert_eq!(result["braced"], attrs, "{id}");
            assert_eq!(result["copy"], attrs, "{id}");
        }
        "proxy_config" => {
            let assigned = json!({
                "host": input["host"], "username": input["username"], "password": input["password"]
            });
            assert_eq!(result["assigned"], assigned, "{id}");
            assert_eq!(result["copy"], assigned, "{id}");
            assert_eq!(
                result["default"],
                json!({"host": [], "username": [], "password": []})
            );
        }
        "touchpad_callback" => {
            let gestures = input["gestures"].as_array().cloned().unwrap_or_default();
            let calls = result["calls"].as_array().cloned().unwrap_or_default();
            assert_eq!(calls.len(), gestures.len(), "{id}");
            let set = input["callback"] == "set";
            assert_eq!(result["is_set"], set, "{id}");
            for (call, gesture) in calls.iter().zip(&gestures) {
                if set {
                    assert_eq!(call["received"], *gesture, "{id}");
                } else {
                    assert_eq!(*call, json!({"error": "bad_function_call"}), "{id}");
                }
            }
        }
        "touchpad_handlers" => {
            let steps = input["steps"].as_array().cloned().unwrap_or_default();
            assert_eq!(result["events"], json!(handler_events(&steps)), "{id}");
        }
        "mapped_file" => {
            let bytes = input["bytes"].as_str().unwrap_or_default();
            assert_eq!(result["size"], bytes.len() / 2, "{id}");
            if bytes.is_empty() {
                assert_eq!(result["data"], Value::Null, "{id}");
                assert_eq!(result["points_to"], "null", "{id}");
            } else {
                assert_eq!(result["data"], bytes, "{id}");
                let target = if input["state"] == "mapped" {
                    "mapping"
                } else {
                    "fallback_buffer"
                };
                assert_eq!(result["points_to"], target, "{id}");
            }
        }
        "default_arguments" => {
            let function = input["function"].as_str().unwrap_or_default();
            let explicit = input["explicit"] == true;
            let strings = input.get("strings").cloned().unwrap_or(json!([]));
            assert_eq!(result["strings"], strings, "{id}");
            let flags = &result["flags"];
            match function {
                "store_secret_async" => assert_eq!(flags["done_is_empty"], !explicit, "{id}"),
                "get_native_window_handles" => {
                    assert_eq!(flags["window_is_null"], true, "{id}");
                    assert_eq!(flags["handle_is_null"], false, "{id}");
                    assert_eq!(flags["display_is_null"], false, "{id}");
                    assert_eq!(flags["display_type_is_null"], !explicit, "{id}");
                }
                _ => assert_eq!(flags["error_is_null"], !explicit, "{id}"),
            }
            if function == "open_unique_sibling_temp_file" {
                assert_eq!(flags["temp_path_out_is_null"], false, "{id}");
                assert_eq!(result["returned_null"], true, "{id}");
            }
            if input.get("returned").is_some() {
                assert_eq!(result["returned"], input["returned"], "{id}");
            }
            if function == "atomic_write_file" {
                assert_eq!(result["data"], input["data"], "{id}");
            }
        }
        other => assert_eq!(other, "known operation", "{id}"),
    }
}

#[test]
fn every_case_succeeds_with_the_requested_values() -> Result<(), serde_json::Error> {
    let expected: Value = serde_json::from_str(EXPECTED)?;
    let mut cases = 0u64;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reply = dispatch(line);
        let id = &request["case"];
        assert_eq!(reply["unit"], request["unit"], "{id}");
        assert_eq!(reply["case"], *id, "{id}");
        assert!(reply.get("result").is_some(), "{id}: {reply}");
        check(&request["input"], &reply["result"], id);
        cases += 1;
    }
    assert_eq!(Some(cases), expected["cases"].as_u64());
    assert_eq!(Some(cases), expected["compared"].as_u64());
    assert_eq!(expected["deferred"], 0);
    Ok(())
}

#[test]
fn controls_are_refused_with_the_recorded_message() -> Result<(), serde_json::Error> {
    let expected: Value = serde_json::from_str(EXPECTED)?;
    let mut count = 0u64;
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(
            reply["error"], control["error"],
            "{}",
            control["request"]["case"]
        );
        count += 1;
    }
    assert_eq!(Some(count), expected["controls"].as_u64());
    Ok(())
}
