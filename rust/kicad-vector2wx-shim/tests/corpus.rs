// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts and the
//! component copy explicit.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_vector2wx_shim::{dispatch, UNIT};
use serde_json::{json, Value};

const CASES: &str = include_str!(
    "../../../port/cases/libs.kimath.include.math.application.thin.replacement.p001.jsonl"
);
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/vector2wx-controls.jsonl");

/// The value the conversion must produce: the source components (or 0, 0 for a
/// default source), as integers or as the exact bits of the converted doubles.
fn expected(input: &Value) -> Option<Value> {
    let (x, y) = match &input["value"] {
        Value::Null => (0, 0),
        v => (v[0].as_i64()?, v[1].as_i64()?),
    };
    let bits = |n: i64| -> Option<Value> {
        let n = i32::try_from(n).ok()?;
        Some(json!(format!("{:016x}", f64::from(n).to_bits())))
    };
    Some(if input["op"].as_str()?.starts_with("to_vector2d") {
        json!({"x": bits(x)?, "y": bits(y)?})
    } else {
        json!({"x": x, "y": y})
    })
}

#[test]
fn every_public_case_copies_its_components() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reply = dispatch(line);
        assert_eq!(
            reply.get("result"),
            expected(&request["input"]).as_ref(),
            "{reply}"
        );
        count += 1;
    }
    assert_eq!(count, 396);
    Ok(())
}

#[test]
fn controls_are_refused_with_the_recorded_message_and_continue() -> Result<(), serde_json::Error> {
    let continuation =
        json!({"unit": UNIT, "case": "next", "input": {"op": "to_wx_size", "value": [-7, 9]}})
            .to_string();
    let mut count = 0;
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(
            reply["error"], control["error"],
            "{}",
            control["request"]["case"]
        );
        assert_eq!(dispatch(&continuation)["result"], json!({"x": -7, "y": 9}));
        count += 1;
    }
    assert_eq!(count, 45);
    Ok(())
}
