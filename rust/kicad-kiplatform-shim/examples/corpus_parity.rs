// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim. Every native
//! reply must be a successful result, and the wasm reply must equal it exactly;
//! this unit has no deferral.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.kiplatform.os.unix.application.thin.replacement.p001.jsonl"
);
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_KIPLATFORM_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = REFERENCE.lines();
    let mut cases = 0usize;
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let actual = kicad_kiplatform_shim::dispatch(request);
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
        cases += 1;
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    if cases != 36 {
        return Err(format!("expected 36 cases, replayed {cases}"));
    }
    Ok(())
}
