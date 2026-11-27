// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts explicit.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_kiplatform_shim::{dispatch, UNIT};
use serde_json::Value;

const CASES: &str = include_str!(
    "../../../port/cases/libs.kiplatform.os.unix.application.thin.replacement.p001.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/kiplatform-policy-controls.jsonl");

#[test]
fn every_case_is_a_genuine_call_with_the_whole_key() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reply = dispatch(line);
        assert_eq!(reply["unit"], UNIT);
        assert_eq!(reply["case"], request["case"]);
        let result = &reply["result"];
        let length = request["input"]["key"].as_array().map(Vec::len);
        assert_eq!(result["key_length"].as_u64(), length.map(|n| n as u64));
        let op = request["input"]["op"].as_str();
        let expected = match op {
            Some("get_policy_bool") => Some(Value::from("NOT_CONFIGURED")),
            Some("get_policy_enum_uint") => Some(Value::Null),
            _ => None,
        };
        assert!(expected.is_some(), "unexpected op {op:?}");
        assert_eq!(
            Some(&result["value"]),
            expected.as_ref(),
            "{}",
            request["case"]
        );
        count += 1;
    }
    assert_eq!(count, 36);
    Ok(())
}

#[test]
fn controls_are_refused_with_the_recorded_envelope() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line)?;
        let raw = control["line"].as_str().unwrap_or_default();
        assert_eq!(dispatch(raw), control["reply"], "{raw}");
        count += 1;
    }
    assert_eq!(count, 25);
    Ok(())
}
