// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++
//! comparison is the live difftest; these keep the Rust side's counts explicit.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_core_include_shim::{adversary, adversary_json, dispatch};
use serde_json::Value;

const CASES: &str =
    include_str!("../../../port/cases/libs.core.include.core.application.pure.p001.jsonl");
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/core-include-p001-controls.jsonl");
const ADVERSARY: &str = include_str!("../adversary-weights.json");

/// Every corpus case is inside the defined domain and succeeds; none defers.
#[test]
fn every_case_succeeds() {
    let mut cases = 0;
    let mut refused = Vec::new();
    for line in CASES.lines() {
        let reply = dispatch(line);
        cases += 1;
        if reply.get("result").is_none() {
            refused.push(reply);
        }
    }
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(cases, 669);
}

#[test]
fn controls_are_refused_with_the_recorded_message() -> Result<(), serde_json::Error> {
    let mut count = 0;
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
    assert_eq!(count, 49);
    Ok(())
}

/// The committed adversarial weights are what the adversary produces against the
/// ported algorithms.
#[test]
fn adversary_weights_are_reproducible() {
    let weights = adversary::weights().map_err(|e| format!("{e:?}"));
    // A Windows checkout may convert the committed file's line endings.
    assert_eq!(
        weights.map(|w| adversary_json(&w)),
        Ok(ADVERSARY.replace("\r\n", "\n"))
    );
}
