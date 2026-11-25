// SPDX-License-Identifier: GPL-3.0-or-later
//! Native checks that need no oracle: every undefined-behaviour control is refused with its
//! recorded envelope, every corpus case is answered in order with a result or a typed
//! deferral naming a Clipper2 engine source unit, and transport errors keep their shape.
use kicad_clipper2_shim::dispatch;
use serde_json::{json, Value};

const CASES: &str = include_str!(
    "../../../port/cases/thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001.jsonl"
);
const CONTROLS: &str =
    include_str!("../../../port/oracle/fixtures/clipper2-core-engine-controls.jsonl");

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
        assert!(matches!(
            error["unit"].as_str(),
            Some("thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001")
                | Some("thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002")
        ));
        deferred += 1;
    }
    assert!(
        results > 1000 && deferred > 0,
        "{results} results, {deferred} deferred"
    );
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
