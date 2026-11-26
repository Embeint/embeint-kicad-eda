// SPDX-License-Identifier: GPL-3.0-or-later
//! Compare portable runtime behavior with this revision's newly executed native shim, for every
//! public case (deferrals included) and every harness control.
#![forbid(unsafe_code)]
use kicad_arc_construction_shim::dispatch;
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.kimath.src.geometry.arc_construction.application.ui.p001.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/arc-construction-controls.jsonl");
const NATIVE: &str = include_str!(env!("KICAD_ARC_CONSTRUCTION_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = NATIVE.lines();
    for request in CASES.lines() {
        let expected: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = dispatch(request);
        if actual != expected {
            return Err(format!("native/wasm mismatch: {}", actual["case"]));
        }
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let actual = dispatch(&control["request"].to_string());
        if actual.get("error") != control.get("error") {
            return Err(format!("wasm control mismatch: {}", actual["case"]));
        }
    }
    Ok(())
}
