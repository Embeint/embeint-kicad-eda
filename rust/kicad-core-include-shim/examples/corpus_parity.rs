// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim. Every native reply
//! must be a successful result and the Rust reply must equal it exactly; the unit has
//! no deferred calls, so any other reply fails the probe.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::Value;
const CASES: &str =
    include_str!("../../../port/cases/libs.core.include.core.application.pure.p001.jsonl");
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_CORE_INCLUDE_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = REFERENCE.lines();
    let mut cases = 0usize;
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let actual = kicad_core_include_shim::dispatch(request);
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
    if cases == 0 {
        return Err("empty corpus".into());
    }
    Ok(())
}
