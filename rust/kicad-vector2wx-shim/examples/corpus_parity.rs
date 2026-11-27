// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each public corpus request runs through the same dispatch as the native shim;
//! every reply must equal the native C++ reply exactly. Malformed controls and their
//! continuations are checked separately in native processes. This wasm probe covers
//! the 396 successful corpus requests, so any non-result reply fails.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.kimath.include.math.application.thin.replacement.p001.jsonl"
);
// The host must supply output from this revision's live native run of the corpus.
const REFERENCE: &str = include_str!(env!("KICAD_VECTOR2WX_WASM_REFERENCE"));
const CASE_COUNT: usize = 396;
fn main() -> Result<(), String> {
    let mut reference = REFERENCE.lines();
    let mut cases = 0usize;
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = kicad_vector2wx_shim::dispatch(request);
        if native.get("result").is_none() || actual != native {
            return Err(format!("wasm mismatch or failure: {}", native["case"]));
        }
        cases += 1;
    }
    if reference.next().is_some() || cases != CASE_COUNT {
        return Err(format!(
            "expected {CASE_COUNT} native replies, compared {cases}"
        ));
    }
    Ok(())
}
