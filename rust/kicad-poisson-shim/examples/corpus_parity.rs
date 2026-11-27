// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens. Each public
//! request, then each control request, runs through the same dispatch as the native shim and must
//! reproduce the native reply exactly: every public reply and every continuation is a result, and
//! every refusal control is the same `invalid_input` refusal. The unit has no deferrals.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use serde_json::Value;
const CASES: &str = include_str!(
    "../../../port/cases/libs.kimath.src.geometry.poisson_disk.application.pure.p001.jsonl"
);
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/poisson-disk-controls.jsonl");
// The host must supply output from this revision's live native runs of CASES then the control
// requests, in that order.
const REFERENCE: &str = include_str!(env!("KICAD_POISSON_WASM_REFERENCE"));
fn main() -> Result<(), String> {
    let mut requests: Vec<String> = CASES.lines().map(str::to_owned).collect();
    let public = requests.len();
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        requests.push(control["request"].to_string());
    }
    let mut reference = REFERENCE.lines();
    let (mut results, mut refusals) = (0usize, 0usize);
    for (i, request) in requests.iter().enumerate() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = kicad_poisson_shim::dispatch(request);
        if actual != native {
            return Err(format!("wasm mismatch at request {i}"));
        }
        if native.get("result").is_some() {
            results += 1;
        } else if i >= public && native["error"]["kind"] == "invalid_input" {
            refusals += 1;
        } else {
            return Err(format!("unexpected native failure at request {i}"));
        }
    }
    if reference.next().is_some() {
        return Err("extra native reply".into());
    }
    if results < public || refusals == 0 {
        return Err(format!(
            "counts differ: {results} results, {refusals} refusals"
        ));
    }
    Ok(())
}
