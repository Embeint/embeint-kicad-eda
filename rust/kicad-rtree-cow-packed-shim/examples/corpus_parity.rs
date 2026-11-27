// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native replies, not stored goldens.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_rtree_cow_packed_shim::operations::run;
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.kimath.include.geometry.rtree.application.pure.p002.jsonl"
);
// The host must supply output from this revision's live native comparison.
const REFERENCE: &str = include_str!(env!("KICAD_COW_PACKED_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut reference = REFERENCE.lines();
    let mut count = 0;
    for request in CASES.lines() {
        let r: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let expected: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        if r["unit"] != expected["unit"] || r["case"] != expected["case"] {
            return Err("identity mismatch".into());
        }
        let actual = run(&r["input"])?;
        if expected.get("result") != Some(&actual) {
            return Err(format!("wasm mismatch: {}", r["case"]));
        }
        count += 1;
    }
    if reference.next().is_some() || count == 0 {
        return Err("extra native reply or empty corpus".into());
    }
    Ok(())
}
