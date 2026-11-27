// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts explicit.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_map_helpers_shim::dispatch;
use serde_json::Value;

const CASES: &str = include_str!(
    "../../../port/cases/libs.core.include.core.application.thin.replacement.p001.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/core-map-helpers-controls.jsonl");
const EXPECTED: &str = include_str!("../expected-replies.json");

#[test]
fn every_case_succeeds_without_deferral() -> Result<(), serde_json::Error> {
    let expected: Value = serde_json::from_str(EXPECTED)?;
    let (mut cases, mut failed) = (0u64, Vec::new());
    for line in CASES.lines() {
        let reply = dispatch(line);
        cases += 1;
        if reply.get("result").is_none() {
            failed.push(reply);
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(Some(cases), expected["cases"].as_u64());
    assert_eq!(expected["deferred"].as_u64(), Some(0));
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
