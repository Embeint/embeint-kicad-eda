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

/// The cases of the first corpus, all of which come first and are unchanged.
const FIRST_CORPUS: usize = 3329;

fn has_flag(flags: &Value, flag: &str) -> bool {
    flags.as_array().is_some_and(|f| f.contains(&json!(flag)))
}

#[test]
fn every_public_case_succeeds_and_insertions_return_the_same_stream(
) -> Result<(), serde_json::Error> {
    let mut count = 0;
    let mut outcomes = std::collections::BTreeMap::<String, usize>::new();
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
            // The trace and tie appear exactly for scripted requests; a setup failure
            // exactly when the state intersects the mask.
            let scripted = stream.get("host").is_some();
            assert_eq!(reply["result"].get("events").is_some(), scripted);
            let state = stream["state"].as_array().cloned().unwrap_or_default();
            let overlap = state
                .iter()
                .any(|bit| has_flag(&stream["exceptions"], bit.as_str().unwrap_or("")));
            assert_eq!(
                reply["result"].get("setup_failure").is_some(),
                overlap,
                "{reply}"
            );
            if count < FIRST_CORPUS {
                assert!(!scripted && !overlap);
                assert!(matches!(
                    reply["result"]["outcome"].as_str(),
                    Some("returned" | "ios_failure")
                ));
            }
            let outcome = reply["result"]["outcome"].as_str().unwrap_or("").to_owned();
            *outcomes.entry(outcome).or_default() += 1;
        }
        count += 1;
    }
    assert_eq!(count, CORPUS_CASES);
    // Every defined outcome is reached on the reference runtime model.
    for outcome in ["returned", "ios_failure", "runtime_error", "terminate"] {
        assert!(
            outcomes.get(outcome).is_some_and(|&n| n > 0),
            "{outcomes:?}"
        );
    }
    Ok(())
}

#[test]
fn runtimes_differ_only_through_a_unitbuf_destructor() -> Result<(), serde_json::Error> {
    let mut differing = 0;
    let mut terminating = 0;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let reference = dispatch(line);
        let later = dispatch_with(line, SentryFlush::WhenGood);
        if reference != later {
            let stream = &request["input"]["stream"];
            // Only a sentry destructor differs, and it acts only on a unitbuf stream: the
            // main stream or the tied one.
            assert!(
                has_flag(&stream["flags"], "unitbuf")
                    || has_flag(&stream["host"]["tie"]["flags"], "unitbuf"),
                "{reference}"
            );
            // The GCC 15 destructor never terminates.
            assert_ne!(later["result"]["outcome"], json!("terminate"), "{later}");
            if reference["result"]["outcome"] == json!("terminate") {
                terminating += 1;
            }
            differing += 1;
        }
    }
    assert!(differing > terminating && terminating > 0);
    Ok(())
}

#[test]
fn a_first_write_that_does_not_fit_unwinds_past_the_terminating_flush(
) -> Result<(), serde_json::Error> {
    // Good, unitbuf, failing sync and badbit in the mask: below max(width, literal) bytes the
    // first write throws ios_base::failure with no sync; from there the reference terminates
    // after one sync.
    let mut checked = 0;
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line)?;
        let case = request["case"].as_str().unwrap_or("");
        if !case.contains("-terminate-") || !case.contains("-cap") {
            continue;
        }
        let stream = &request["input"]["stream"];
        let literal = if case.starts_with("size") { 8 } else { 4 };
        let width = stream["width"].as_i64().unwrap_or(0);
        let capacity = stream["capacity"].as_i64().unwrap_or(0);
        let result = &dispatch(line)["result"];
        if capacity < width.max(literal) {
            assert_eq!(result["outcome"], json!("ios_failure"), "{case}");
            assert_eq!(result["sync_calls"], json!(0), "{case}");
        } else {
            assert_eq!(result["outcome"], json!("terminate"), "{case}");
            assert_eq!(result["sync_calls"], json!(1), "{case}");
        }
        checked += 1;
    }
    assert_eq!(checked, 264);
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
    assert_eq!(count, 140);
    Ok(())
}
