// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens. Every public
//! case and every undefined-domain control runs through the same dispatch as the native shim and
//! must reproduce the native reply exactly. The corpus has no expected deferral, so any Rust
//! deferral or error, and any unsuccessful native reply, fails the probe.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.kimath.src.geometry.distribute.application.ui.p001.jsonl"
);
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/distribute-controls.jsonl");
// The host must supply output from this revision's live native runs.
const REFERENCE: &str = include_str!(env!("KICAD_DISTRIBUTE_WASM_REFERENCE"));
const CONTROL_REFERENCE: &str = include_str!(env!("KICAD_DISTRIBUTE_WASM_CONTROL_REFERENCE"));

fn replay(requests: &[String], reference: &str) -> Result<usize, String> {
    let mut native_lines = reference.lines();
    for request in requests {
        let native: Value =
            serde_json::from_str(native_lines.next().ok_or("missing native reply")?)
                .map_err(|e| e.to_string())?;
        let sent: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let actual = kicad_distribute_shim::dispatch(request);
        if native["unit"] != sent["unit"]
            || native["case"] != sent["case"]
            || native.get("result").is_none()
        {
            return Err(format!("unsuccessful native reply: {}", sent["case"]));
        }
        if actual != native {
            return Err(format!("wasm mismatch: {}", sent["case"]));
        }
    }
    if native_lines.next().is_some() {
        return Err("extra native reply".into());
    }
    Ok(requests.len())
}

fn main() -> Result<(), String> {
    let cases: Vec<String> = CASES.lines().map(str::to_owned).collect();
    let controls = CONTROLS
        .lines()
        .map(|line| {
            let control: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
            Ok(control["request"].to_string())
        })
        .collect::<Result<Vec<String>, String>>()?;
    if replay(&cases, REFERENCE)? == 0 || replay(&controls, CONTROL_REFERENCE)? == 0 {
        return Err("empty corpus".into());
    }
    Ok(())
}
