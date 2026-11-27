// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-only checks of the committed corpus and refusal controls. The exact C++ comparison
//! is the live difftest; these keep the Rust side's counts, its successful replies and a
//! few structural invariants explicit.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_wx_stl_compat::ostream::SentryFlush;
use kicad_wx_stl_compat_shim::{dispatch, dispatch_with, CORPUS_CASES, UNIT};
use serde_json::{json, Value};

const CASES: &str =
    include_str!("../../../port/cases/libs.core.application.thin.replacement.p001.jsonl");
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/wx-stl-compat-controls.jsonl");

/// The cases of the first corpus, all of which come first and are unchanged.
const FIRST_CORPUS: usize = 3329;
/// The cases before the call context, facet changes, forced unwinding and null buffers,
/// which follow them unchanged.
const SECOND_CORPUS: usize = 4719;

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
            // The operators never change the fill or the exception mask, nor the precision
            // unless a facet sets it.
            let stream = &request["input"]["stream"];
            let put = &stream["host"]["num_put"];
            let sets_precision = ["set_before", "set_after"]
                .iter()
                .any(|set| !put[set]["precision"].is_null());
            for field in ["fill", "precision", "exceptions"] {
                if field != "precision" || !sets_precision {
                    assert_eq!(reply["result"][field], stream[field], "{field}: {reply}");
                }
            }
            // The trace and tie appear exactly for scripted requests; a setup failure
            // exactly when the state (with badbit for a null buffer) intersects the mask.
            let scripted = stream.get("host").is_some();
            assert_eq!(reply["result"].get("events").is_some(), scripted);
            let mut state = stream["state"].as_array().cloned().unwrap_or_default();
            let null_rdbuf = stream["host"]["null_rdbuf"] == json!(true);
            if null_rdbuf {
                state.push(json!("bad"));
            }
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
            let unwinding = request["input"]["unwinding"] == json!(true);
            if count < SECOND_CORPUS {
                assert!(request["input"].get("unwinding").is_none());
                assert_ne!(outcome, "forced_unwind");
            }
            // While another exception propagates no destructor syncs, so none terminates,
            // and the outer exception is untouched.
            if unwinding {
                assert_ne!(outcome, "terminate", "{reply}");
                assert_eq!(reply["result"]["outer_preserved"], json!(true));
            }
            // A stream without a buffer is never written or synced and stays bad.
            if null_rdbuf {
                assert_eq!(reply["result"]["bytes"], json!(""));
                assert_eq!(reply["result"]["sync_calls"], json!(0));
                assert!(has_flag(&reply["result"]["state"], "bad"));
            }
            // A forced unwind always leaves badbit set on the stream it passed: the main
            // stream, or the tied stream whose flush was cancelled.
            if outcome == "forced_unwind" {
                assert!(
                    has_flag(&reply["result"]["state"], "bad")
                        || has_flag(&reply["result"]["tie"]["state"], "bad"),
                    "{reply}"
                );
            }
            *outcomes.entry(outcome).or_default() += 1;
        }
        count += 1;
    }
    assert_eq!(count, CORPUS_CASES);
    // Every defined outcome is reached on the reference runtime model.
    for outcome in [
        "returned",
        "ios_failure",
        "runtime_error",
        "terminate",
        "forced_unwind",
    ] {
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
            // main stream (unitbuf from the start or set by its facet) or the tied one.
            assert!(
                has_flag(&stream["flags"], "unitbuf")
                    || has_flag(&reference["result"]["flags"], "unitbuf")
                    || has_flag(&stream["host"]["tie"]["flags"], "unitbuf"),
                "{reference}"
            );
            // The GCC 15 destructor never terminates; it aborts only on a forced unwind it
            // swallowed, which the reference destructor terminates on.
            assert_ne!(later["result"]["outcome"], json!("terminate"), "{later}");
            if later["result"]["outcome"] == json!("abort") {
                assert_eq!(
                    reference["result"]["outcome"],
                    json!("terminate"),
                    "{later}"
                );
            }
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
    assert_eq!(count, 217);
    Ok(())
}
