// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim. Every native reply
//! must be a successful result and the wasm reply must equal it exactly; the unit has
//! no deferrals, so any other reply fails. The case count must equal
//! `expected-replies.json`, which the corpus generator derives from the inputs.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.core.include.core.application.thin.replacement.p001.jsonl"
);
const EXPECTED: &str = include_str!("../expected-replies.json");
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_MAP_HELPERS_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let expected: Value = serde_json::from_str(EXPECTED).map_err(|e| e.to_string())?;
    let mut reference = REFERENCE.lines();
    let mut compared = 0usize;
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        if native["unit"] != sent["unit"]
            || native["case"] != sent["case"]
            || native.get("result").is_none()
        {
            return Err(format!(
                "identity mismatch or unsuccessful native reply: {}",
                sent["case"]
            ));
        }
        if kicad_map_helpers_shim::dispatch(request) != native {
            return Err(format!("wasm mismatch: {}", sent["case"]));
        }
        compared += 1;
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    if Some(compared as u64) != expected["cases"].as_u64()
        || Some(compared as u64) != expected["compared"].as_u64()
        || expected["deferred"].as_u64() != Some(0)
    {
        return Err(format!("counts differ: {compared} compared"));
    }
    Ok(())
}
