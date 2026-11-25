// SPDX-License-Identifier: GPL-3.0-or-later
//! Compare portable runtime behavior with this revision's newly executed native shim.
#![forbid(unsafe_code)]
use kicad_geometry_shim::dispatch;
use serde_json::Value;
const CASES: &str =
    include_str!("../../../port/cases/libs.kimath.include.geometry.application.ui.p001.jsonl");
const NATIVE: &str = include_str!(env!("KICAD_GEOMETRY_WASM_REFERENCE"));
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
    Ok(())
}
