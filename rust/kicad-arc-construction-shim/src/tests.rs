// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const CORPUS: &str = include_str!(
    "../../../port/cases/libs.kimath.src.geometry.arc_construction.application.ui.p001.jsonl"
);
const CASES: usize = 2912;
/// Full hypot now covers finite and non-finite tangent norms; every public case returns a result.
const DEFERRED: usize = 0;
const VECTOR_UNIT: &str = "libs.kimath.include.math.application.ui.p002";

#[test]
fn every_public_case_succeeds_or_defers_to_the_vector_owner() {
    let mut count = 0;
    let mut deferred = 0;
    for line in CORPUS.lines() {
        let reply = dispatch(line);
        if let Some(error) = reply.get("error") {
            assert_eq!(error["kind"], "unported", "{reply}");
            assert_eq!(error["unit"], VECTOR_UNIT, "{reply}");
            assert_eq!(
                error["upstream"], "VECTOR2<double>::EuclideanNorm std::hypot",
                "{reply}"
            );
            assert!(
                reply["case"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("tangent-special-")),
                "{reply}"
            );
            deferred += 1;
        } else {
            assert!(reply.get("result").is_some(), "{reply}");
        }
        count += 1;
    }
    assert_eq!(count, CASES);
    assert_eq!(deferred, DEFERRED);
}

fn request(input: Value) -> Value {
    dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string())
}

#[test]
fn upstream_qa_expectations_hold() {
    // ProjectLandsOnBisector
    let reply = request(
        json!({"op": "project", "start": [0, 0], "end": [1000000, 0], "cursor": [700000, 300000]}),
    );
    assert_eq!(reply["result"], json!([500000, 300000]));
    let reply =
        request(json!({"op": "project", "start": [0, 0], "end": [0, 0], "cursor": [12, 34]}));
    assert_eq!(reply["result"], json!([12, 34]));
    // MidDragProjectsOntoBisector
    let reply = request(
        json!({"op": "mid_drag", "start": [0, 0], "end": [1000000, 0], "cursor": [700000, 300000]}),
    );
    assert_eq!(reply["result"]["valid"], true);
    assert_eq!(reply["result"]["mid"], json!([500000, 300000]));
    // ThroughPointsVeryShallowArc keeps the given apex.
    let reply = request(
        json!({"op": "through", "start": [0, 0], "on_arc": [1000000000, 1000], "end": [2000000000, 0]}),
    );
    assert_eq!(reply["result"]["mid"], json!([1000000000, 1000]));
    // CenterDragDegenerateAndOverflowAreInvalid
    for (major, valid) in [(true, false), (false, true)] {
        let reply = request(json!({"op": "center_drag", "start": [-2000000000, 0],
            "end": [2000000000, 0], "cursor": [0, 2000000000], "major": major}));
        assert_eq!(reply["result"]["valid"], valid, "{reply}");
    }
    // A degenerate ArcThroughPoints is the default solution; buildOnChord keeps start and end.
    let reply =
        request(json!({"op": "through", "start": [2, 0], "on_arc": [500, 0], "end": [1000, 0]}));
    assert_eq!(reply["result"]["start"], json!([0, 0]));
    assert_eq!(reply["result"]["end"], json!([0, 0]));
}

#[test]
fn refused_constructions_keep_their_partial_fields() {
    // The apex rounds onto the chord: invalid, but start, end and the rounded mid are kept.
    let reply = request(
        json!({"op": "center_drag", "start": [0, 0], "end": [1000, 0],
        "cursor": [500, 2000000000], "major": false}),
    );
    let r = &reply["result"];
    assert_eq!(r["valid"], false, "{reply}");
    assert_eq!(r["start"], json!([0, 0]));
    assert_eq!(r["end"], json!([1000, 0]));
    assert_eq!(r["mid"], json!([500, 0]));
    assert_eq!(r["radius"], "0000000000000000");
    // The apex leaves the coordinate range: mid stays at its default.
    let reply = request(
        json!({"op": "center_drag", "start": [0, 0], "end": [1000, 0],
        "cursor": [500, 2000000000], "major": true}),
    );
    assert_eq!(reply["result"]["mid"], json!([0, 0]), "{reply}");
    assert_eq!(reply["result"]["end"], json!([1000, 0]), "{reply}");
}

#[test]
fn harness_controls_match_their_recorded_refusals() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in
        include_str!("../../../port/oracle/fixtures/arc-construction-controls.jsonl").lines()
    {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(
            reply,
            json!({"unit": UNIT, "case": control["request"]["case"], "error": control["error"]})
        );
        count += 1;
    }
    assert_eq!(count, 20);
    Ok(())
}

#[test]
fn malformed_inputs_are_refused_before_any_operation() {
    let p = json!([0, 0]);
    for (input, message) in [
        (json!({"start": p}), "expected string: op"),
        (json!({"op": 3}), "expected string: op"),
        (json!({"op": "rotate"}), "unknown operation"),
        (
            json!({"op": "project", "start": p, "end": p}),
            "unexpected or missing field",
        ),
        (
            json!({"op": "project", "start": p, "end": p, "cursor": p, "x": 1}),
            "unexpected or missing field",
        ),
        (
            json!({"op": "project", "start": [0], "end": p, "cursor": p}),
            "expected point",
        ),
        (
            json!({"op": "through", "start": p, "on_arc": [2147483648i64, 0], "end": p}),
            "point out of range",
        ),
        (
            json!({"op": "through", "start": p, "on_arc": [0, -2147483649i64], "end": p}),
            "point out of range",
        ),
        (
            json!({"op": "mid_drag", "start": p, "end": [0.0, 1], "cursor": p}),
            "point out of range",
        ),
        (
            json!({"op": "center_drag", "start": p, "end": p, "cursor": p, "major": 1}),
            "expected boolean: major",
        ),
        (
            json!({"op": "tangent", "start": p, "tangent": [0, 0], "end": p}),
            "expected double bits",
        ),
        (
            json!({"op": "tangent", "start": p, "tangent": ["3FF0000000000000", "0000000000000000"], "end": p}),
            "expected double bits",
        ),
        (
            json!({"op": "tangent", "start": p, "tangent": ["3ff000000000000", "0000000000000000"], "end": p}),
            "expected double bits",
        ),
        (
            json!({"op": "tangent", "start": p, "tangent": ["3ff0000000000000"], "end": p}),
            "expected vector",
        ),
    ] {
        let reply = request(input);
        assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
        assert_eq!(reply["error"]["message"], message, "{reply}");
    }
}
