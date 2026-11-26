// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts explicit and
//! check the reported state against the decoded fixtures.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_kiplatform_sysinfo_shim::dispatch;
use serde_json::{json, Value};

const CASES: &str =
    include_str!("../../../port/cases/libs.kiplatform.os.unix.application.pure.p001.jsonl");
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/kiplatform-unix-p001-controls.jsonl");
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

/// The fixture as it must be reported: the request's memory, cpus and gpus.
fn fixture(input: &Value) -> Value {
    canonical(&json!({"memory": input["memory"], "cpus": input["cpus"], "gpus": input["gpus"]}))
}

fn same(reported: &Value, fixture: &Value) -> bool {
    canonical(reported) == *fixture
}

#[test]
fn every_case_succeeds_and_leaves_its_outputs_unchanged() -> Result<(), serde_json::Error> {
    let expected: Value = serde_json::from_str(EXPECTED)?;
    let mut cases = 0u64;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reply = dispatch(line);
        let id = &request["case"];
        assert_eq!(reply["unit"], request["unit"], "{id}");
        assert_eq!(reply["case"], *id, "{id}");
        let result = &reply["result"];
        let input = &request["input"];
        cases += 1;
        if input["op"] == "valid_3dconnexion_driver_version" {
            assert_eq!(*result, json!({"returned": false}), "{id}");
            continue;
        }
        let initial = fixture(input);
        assert!(same(&result["before"], &initial), "{id}");
        let steps = result["steps"].as_array().map_or(0, Vec::len);
        assert_eq!(Some(steps), input["steps"].as_array().map(Vec::len), "{id}");
        for (index, step) in result["steps"].as_array().into_iter().flatten().enumerate() {
            assert_eq!(step["step"], input["steps"][index], "{id}");
            let returned = if step["step"] == "reconstruct" {
                Value::Null
            } else {
                Value::Bool(false)
            };
            assert_eq!(
                step.get("returned").cloned().unwrap_or(Value::Null),
                returned
            );
            assert!(same(&step["state"], &initial), "{id} step {index}");
            assert_eq!(step["storage_unchanged"], true, "{id} step {index}");
        }
        assert!(same(&result["after_destroy"]["state"], &initial), "{id}");
        assert_eq!(result["after_destroy"]["storage_unchanged"], true, "{id}");
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
