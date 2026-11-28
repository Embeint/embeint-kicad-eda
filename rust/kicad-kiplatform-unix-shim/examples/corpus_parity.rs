// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request runs through the same dispatch as the native shim. Every native reply must be
//! a successful result; the Rust reply must equal it exactly, or be the expected deferral
//! (an `unported` reply). `TerminateWithCode` ends a process, which wasm32 has no child for,
//! so those cases are counted and skipped. The counts must equal `expected-replies.json`.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use serde_json::Value;
const CASES: &str =
    include_str!("../../../port/cases/libs.kiplatform.os.unix.application.ui.p001.jsonl");
const EXPECTED: &str = include_str!("../expected-replies.json");
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_KIPLATFORM_UNIX_UI_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let expected: Value = serde_json::from_str(EXPECTED).map_err(|e| e.to_string())?;
    let mut reference = REFERENCE.lines();
    let (mut cases, mut compared, mut deferred, mut native_only) = (0u64, 0u64, 0u64, 0u64);
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
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
        if sent["input"]["op"] == "app_terminate_with_code" {
            native_only += 1;
            continue;
        }
        let actual = kicad_kiplatform_unix_shim::dispatch(request);
        if actual["error"]["kind"] == "unported" {
            deferred += 1;
            continue;
        }
        if actual != native {
            return Err(format!("wasm mismatch: {}", sent["case"]));
        }
        compared += 1;
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    if Some(cases) != expected["cases"].as_u64()
        || Some(compared + native_only) != expected["compared"].as_u64()
        || Some(deferred) != expected["deferred"].as_u64()
        || Some(native_only) != expected["native_only"].as_u64()
    {
        return Err(format!(
            "counts differ: {cases} cases, {compared} compared, {deferred} deferred, {native_only} native only"
        ));
    }
    Ok(())
}
