// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime probe against freshly collected native C++ replies, not stored goldens.
//! Each public corpus request runs through the same dispatch as the native shim, with the
//! reference runtime's sentry; every reply must equal the native C++ reply exactly. The
//! corpus holds only successful requests, so any non-result reply fails. Malformed controls
//! and their continuations are checked separately in native processes.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_wx_stl_compat::ostream::SentryFlush;
use serde_json::Value;
const CASES: &str =
    include_str!("../../../port/cases/libs.core.application.thin.replacement.p001.jsonl");
// The host must supply output from this revision's live native run of the corpus.
const REFERENCE: &str = include_str!(env!("KICAD_WX_STL_COMPAT_WASM_REFERENCE"));
const CASE_COUNT: usize = kicad_wx_stl_compat_shim::CORPUS_CASES;
// The libstdc++ runtime of the native replies decides ~sentry; the hosted reference is the
// default. KICAD_WX_STL_COMPAT_WASM_SENTRY_FLUSH=when-good selects a GCC 15 or later runtime.
const SENTRY_FLUSH: Option<&str> = option_env!("KICAD_WX_STL_COMPAT_WASM_SENTRY_FLUSH");
fn main() -> Result<(), String> {
    let flush = match SENTRY_FLUSH {
        None | Some("unconditional") => SentryFlush::Unconditional,
        Some("when-good") => SentryFlush::WhenGood,
        Some(other) => return Err(format!("unknown sentry flush {other}")),
    };
    let mut reference = REFERENCE.lines();
    let mut cases = 0usize;
    for request in CASES.lines() {
        let native: Value = serde_json::from_str(reference.next().ok_or("missing native reply")?)
            .map_err(|e| e.to_string())?;
        let actual = kicad_wx_stl_compat_shim::dispatch_with(request, flush);
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
