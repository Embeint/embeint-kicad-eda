// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each request, and each malformed control followed by its continuation, runs through
//! the same dispatch as the native shim; every reply must equal the native C++ reply
//! exactly. The unit has no deferrals, so any non-result reply in the corpus fails.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
