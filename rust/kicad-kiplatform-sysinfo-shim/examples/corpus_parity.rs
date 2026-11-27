// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim. Every native reply
//! must be a successful result and the Rust reply must equal it exactly; there are
//! no expected deferrals. The case count must equal `expected-replies.json`.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use serde_json::Value;
const CASES: &str =
    include_str!("../../../port/cases/libs.kiplatform.os.unix.application.pure.p001.jsonl");
const EXPECTED: &str = include_str!("../expected-replies.json");
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_KIPLATFORM_SYSINFO_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let expected: Value = serde_json::from_str(EXPECTED).map_err(|e| e.to_string())?;
    if expected["deferred"] != 0 {
        return Err("unexpected deferral specification".into());
    }
    let mut reference = REFERENCE.lines();
    let mut cases = 0usize;
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let actual = kicad_kiplatform_sysinfo_shim::dispatch(request);
        cases += 1;
        if native["unit"] != sent["unit"]
            || native["case"] != sent["case"]
            || native.get("result").is_none()
        {
            return Err(format!(
                "identity mismatch or unsuccessful native reply: {}",
                sent["case"]
            ));
        }
        if actual != native {
            return Err(format!("wasm mismatch: {}", sent["case"]));
        }
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    if Some(cases as u64) != expected["cases"].as_u64()
        || Some(cases as u64) != expected["compared"].as_u64()
    {
        return Err(format!("counts differ: {cases} cases"));
    }
    Ok(())
}
