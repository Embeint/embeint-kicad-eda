// SPDX-License-Identifier: GPL-3.0-or-later
//! Native checks that need no oracle: every control is refused with its recorded envelope,
//! and every corpus case is answered with a result or a typed deferral to the header unit
//! that owns the remaining unported callees.
use kicad_clipper2_p002_shim::dispatch;
use serde_json::{json, Value};

const CASES: &str = include_str!(
    "../../../port/cases/thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/clipper2-engine-p002-controls.jsonl");

#[test]
fn controls_are_refused() {
    for line in CONTROLS.lines() {
        let control: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(reply.get("error"), control.get("error"), "{line}");
        assert!(reply.get("result").is_none());
    }
}

#[test]
fn corpus_is_answered_with_results_or_owned_deferrals() {
    let (mut results, mut deferred) = (0, 0);
    for line in CASES.lines() {
        let request: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        let reply = dispatch(line);
        assert_eq!(reply["case"], request["case"]);
        if reply.get("result").is_some() {
            results += 1;
            continue;
        }
        let error = &reply["error"];
        assert_eq!(error["kind"], "unported", "{line} -> {reply}");
        assert_eq!(
            error["unit"],
            "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002"
        );
        deferred += 1;
    }
    assert!(results > 500, "{results} results, {deferred} deferred");
}

#[test]
fn transport_errors() {
    assert_eq!(
        dispatch("{"),
        json!({"unit": null, "case": null, "error": {"kind": "invalid_request", "message": "invalid JSON or duplicate key"}})
    );
    let other = dispatch(r#"{"unit":"other","case":"c","input":{}}"#);
    assert_eq!(other["error"]["kind"], "unknown_unit");
}
