// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim. Every native reply
//! must be a successful result. A Rust result must reproduce it exactly; a Rust
//! deferral must be exactly the expected unported reply (`LATENCY_PROBE::delta_ms`,
//! owned by the header unit) for one of the expected case ids. The compared and
//! deferred counts and the deferred ids must equal `expected-replies.json`, which the
//! corpus generator derives from the inputs.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::{json, Value};
const CASES: &str = include_str!("../../../port/cases/libs.core.application.pure.p001.jsonl");
const EXPECTED: &str = include_str!("../expected-replies.json");
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_CORE_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let expected: Value = serde_json::from_str(EXPECTED).map_err(|e| e.to_string())?;
    let deferral = &expected["deferred"]["error"];
    let deferred_ids: Vec<&str> = expected["deferred"]["cases"]
        .as_array()
        .ok_or("expected deferred case list")?
        .iter()
        .map(|id| id.as_str().ok_or("expected deferred case id"))
        .collect::<Result<_, _>>()?;
    if deferral["kind"] != "unported"
        || deferral["upstream"] != "LATENCY_PROBE::delta_ms"
        || deferral["unit"] != kicad_core::HEADER_UNIT
    {
        return Err("unexpected deferral specification".into());
    }
    let mut reference = REFERENCE.lines();
    let (mut cases, mut compared) = (0usize, 0usize);
    let mut deferred: Vec<String> = Vec::new();
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let actual = kicad_core_shim::dispatch(request);
        cases += 1;
        if actual["unit"] != sent["unit"]
            || actual["case"] != sent["case"]
            || native["unit"] != sent["unit"]
            || native["case"] != sent["case"]
            || native.get("result").is_none()
        {
            return Err(format!(
                "identity mismatch or unsuccessful native reply: {}",
                sent["case"]
            ));
        }
        let case = sent["case"].as_str().ok_or("expected case id")?;
        if actual.get("result").is_some() {
            if actual != native {
                return Err(format!("wasm mismatch: {case}"));
            }
            compared += 1;
        } else if deferred_ids.contains(&case)
            && actual == json!({"unit": sent["unit"], "case": case, "error": deferral})
        {
            deferred.push(case.to_owned());
        } else {
            return Err(format!("wasm failure or unexpected deferral: {case}"));
        }
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    if Some(cases as u64) != expected["cases"].as_u64()
        || Some(compared as u64) != expected["compared"].as_u64()
        || deferred != deferred_ids
    {
        return Err(format!(
            "counts differ: {cases} cases, {compared} compared, {} deferred",
            deferred.len()
        ));
    }
    Ok(())
}
