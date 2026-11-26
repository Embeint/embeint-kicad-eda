// SPDX-License-Identifier: GPL-3.0-or-later
//! Replay the complete corpus and the refusal controls in import-free WebAssembly and require
//! the replies of this revision's newly executed native shim, byte-identical JSON values case
//! by case.
#![forbid(unsafe_code)]
use kicad_clipper2_p002_shim::dispatch;
use serde_json::Value;

const CASES: &str = include_str!(
    "../../../port/cases/thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/clipper2-engine-p002-controls.jsonl");
const NATIVE: &str = include_str!(env!("KICAD_CLIPPER2_P002_WASM_REFERENCE"));

fn main() -> Result<(), String> {
    let mut reference = NATIVE.lines();
    let mut count = 0usize;
    for request in CASES.lines() {
        let expected: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        if dispatch(request) != expected {
            return Err(format!("native/wasm mismatch at case {count}"));
        }
        count += 1;
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let reply = dispatch(&control["request"].to_string());
        if reply.get("error") != control.get("error") || reply.get("result").is_some() {
            return Err("wasm refusal control mismatch".into());
        }
    }
    Ok(())
}
