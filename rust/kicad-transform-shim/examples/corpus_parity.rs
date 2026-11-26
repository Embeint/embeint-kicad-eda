// SPDX-License-Identifier: GPL-3.0-or-later
//! Compare portable runtime behaviour with this revision's newly executed native shim: every
//! public case and every undefined-domain control must give the native reply exactly.
#![forbid(unsafe_code)]
use kicad_transform_shim::dispatch;
use serde_json::Value;
const CASES: &str =
    include_str!("../../../port/cases/libs.kimath.src.transform.application.ui.p001.jsonl");
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/transform-controls.jsonl");
const NATIVE: &str = include_str!(env!("KICAD_TRANSFORM_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = NATIVE.lines();
    let mut check = |request: &str| -> Result<(), String> {
        let expected: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = dispatch(request);
        if actual != expected {
            return Err(format!("native/wasm mismatch: {}", actual["case"]));
        }
        Ok(())
    };
    for request in CASES.lines() {
        check(request)?;
    }
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        check(&control["request"].to_string())?;
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    Ok(())
}
