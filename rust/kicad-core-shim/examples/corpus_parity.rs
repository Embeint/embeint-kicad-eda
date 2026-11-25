// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim; a native result
//! must be reproduced exactly, and a Rust deferral must be the expected unported
//! reply for a case the C++ answered.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::Value;
const CASES: &str = include_str!("../../../port/cases/libs.core.application.pure.p001.jsonl");
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_CORE_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = REFERENCE.lines();
    let (mut compared, mut deferred) = (0usize, 0usize);
    for request in CASES.lines() {
        let expected: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = kicad_core_shim::dispatch(request);
        if actual["unit"] != expected["unit"]
            || actual["case"] != expected["case"]
            || expected.get("result").is_none()
        {
            return Err("identity mismatch or unsuccessful native reply".into());
        }
        if actual.get("result").is_some() {
            if actual != expected {
                return Err(format!("wasm mismatch: {}", expected["case"]));
            }
            compared += 1;
        } else if actual["error"]["kind"] == "unported" {
            deferred += 1;
        } else {
            return Err(format!("wasm failure: {}", expected["case"]));
        }
    }
    if reference.next().is_some() || compared == 0 {
        return Err("extra native reply or empty corpus".into());
    }
    let _ = deferred;
    Ok(())
}
