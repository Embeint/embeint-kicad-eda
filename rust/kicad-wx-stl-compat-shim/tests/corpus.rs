// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++ comparison
//! is the live difftest; these keep the Rust side's counts, its successful replies and a
//! few structural invariants explicit.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_wx_stl_compat::ostream::SentryFlush;
use kicad_wx_stl_compat_shim::{dispatch, dispatch_with, CORPUS_CASES, UNIT};
use serde_json::{json, Value};

const CASES: &str =
    include_str!("../../../port/cases/libs.core.application.thin.replacement.p001.jsonl");
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/wx-stl-compat-controls.jsonl");

#[test]
fn every_public_case_succeeds_and_insertions_return_the_same_stream(
) -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reply = dispatch(line);
        let result = reply.get("result");
        assert!(result.is_some(), "{reply}");
        if request["input"]["op"]
            .as_str()
            .is_some_and(|op| op.starts_with("insert_"))
        {
            assert_eq!(reply["result"]["same_stream"], json!(true), "{reply}");
            // The operators never change flags, precision, fill or the exception mask.
            let stream = &request["input"]["stream"];
            for field in ["fill", "precision", "exceptions"] {
                assert_eq!(reply["result"][field], stream[field], "{field}: {reply}");
            }
        }
        count += 1;
    }
    assert_eq!(count, CORPUS_CASES);
    Ok(())
}

#[test]
fn runtimes_differ_only_on_unitbuf_streams_that_are_not_good() -> Result<(), serde_json::Error> {
    let mut differing = 0;
    let mut badbit = 0;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reference = dispatch(line);
        let later = dispatch_with(line, SentryFlush::WhenGood);
        if reference != later {
            let stream = &request["input"]["stream"];
            // The older destructor syncs more often, and only a failing sync adds badbit.
            assert!(
                reference["result"]["sync_calls"].as_u64() > later["result"]["sync_calls"].as_u64()
            );
            if reference["result"]["state"] != later["result"]["state"] {
                assert_eq!(stream["sync_fails"], json!(true), "{reference}");
                badbit += 1;
            }
            assert!(
                stream["flags"]
                    .as_array()
                    .is_some_and(|f| f.contains(&json!("unitbuf"))),
                "{reference}"
            );
            differing += 1;
        }
    }
    assert!(differing > badbit && badbit > 0);
    Ok(())
}

#[test]
fn controls_are_refused_with_the_recorded_message_and_continue() -> Result<(), serde_json::Error> {
    let continuation = json!({"unit": UNIT, "case": "next", "input": {
        "op": "to_wx_string", "ctype": "C", "view": "610062"}})
    .to_string();
    let mut count = 0;
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(
            reply["error"], control["error"],
            "{}",
            control["request"]["case"]
        );
        assert_eq!(
            dispatch(&continuation)["result"],
            json!({"units": [0x61, 0, 0x62]})
        );
        count += 1;
    }
    assert_eq!(count, 95);
    Ok(())
}
