// SPDX-License-Identifier: GPL-3.0-or-later
//! Compare the portable runtime with this revision's newly executed native shim: every public
//! case and every refusal control, reply for reply, in import-free WebAssembly.
#![forbid(unsafe_code)]
use kicad_font_shim::dispatch;
use serde_json::Value;
const CASES: &str = include_str!("../../../port/cases/include.font.application.ui.p001.jsonl");
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/font-controls.jsonl");
const NATIVE: &str = include_str!(env!("KICAD_FONT_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = NATIVE.lines();
    let controls = CONTROLS
        .lines()
        .map(|line| -> Result<String, String> {
            let control: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
            Ok(control["request"].to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    for request in CASES.lines().map(str::to_string).chain(controls) {
        let expected: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = dispatch(&request);
        if actual != expected {
            return Err(format!("native/wasm mismatch: {}", actual["case"]));
        }
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    Ok(())
}
