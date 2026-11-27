// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const CASES: usize = 9503;
const CONTROLS: usize = 1887;

#[test]
fn every_public_case_succeeds() {
    // The seg.h SEG::Square and SEG::SquaredLength are ported by the header unit
    // (libs.kimath.include.geometry.application.ui.p003), so no public case defers.
    let mut results = 0;
    for line in
        include_str!("../../../port/cases/libs.kimath.src.geometry.seg.application.ui.p001.jsonl")
            .lines()
    {
        let reply = dispatch(line);
        assert!(reply.get("result").is_some(), "{line} -> {reply}");
        results += 1;
    }
    assert_eq!(results, CASES);
}

#[test]
fn controls_refuse_exactly_the_undefined_evaluations() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in include_str!("../../../port/oracle/fixtures/seg-controls.jsonl").lines() {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(reply["case"], control["request"]["case"], "{reply}");
        assert_eq!(reply["error"], control["error"], "{line} -> {reply}");
        count += 1;
    }
    assert_eq!(count, CONTROLS);
    Ok(())
}

#[test]
fn malformed_inputs_are_refused_before_any_operation() {
    let s = json!([[0, 0], [10, 0]]);
    for (input, message) in [
        (json!({"seg": s, "point": [1, 1]}), "expected string: op"),
        (
            json!({"op": "distance_point", "seg": s}),
            "unexpected or missing field",
        ),
        (
            json!({"op": "distance_point", "seg": s, "point": [1, 1], "x": 1}),
            "unexpected or missing field",
        ),
        (
            json!({"op": "distance_point", "seg": [[0, 0]], "point": [1, 1]}),
            "expected segment: seg",
        ),
        (
            json!({"op": "distance_point", "seg": [[0, 0], [1]], "point": [1, 1]}),
            "expected point",
        ),
        (
            json!({"op": "distance_point", "seg": s, "point": [2147483648i64, 1]}),
            "point out of range",
        ),
        (
            json!({"op": "distance_point", "seg": s, "point": [1.0, 1]}),
            "point out of range",
        ),
        (
            json!({"op": "contains_point", "seg": s, "point": [1, 1], "threshold": 1.5}),
            "integer out of range",
        ),
        (
            json!({"op": "line_distance", "seg": s, "point": [1, 1], "determine_side": 1}),
            "expected bool: determine_side",
        ),
        (
            json!({"op": "intersect", "seg": s, "other": s, "lines": true}),
            "unexpected or missing field",
        ),
        (
            json!({"op": "intersects_line", "seg": s, "slope": "0", "offset": "0", "initial": [0, 0]}),
            "invalid hex: slope",
        ),
        (
            json!({"op": "intersects_line", "seg": s, "slope": "3FF0000000000000",
                   "offset": "0000000000000000", "initial": [0, 0]}),
            "invalid hex: slope",
        ),
        (
            json!({"op": "collide", "seg": s, "other": s, "clearance": 2147483648i64}),
            "integer out of range",
        ),
        (json!({"op": "square"}), "unknown operation"),
    ] {
        let reply = dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
        assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
        assert_eq!(reply["error"]["message"], message, "{reply}");
    }
}

#[test]
fn defaults_match_the_explicit_arguments() {
    let run =
        |input: Value| dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
    let s = json!([[0, 0], [10, 0]]);
    // Contains defaults to a squared threshold of 3: (9, 1) is inside, (5, 2) is not.
    for (p, inside) in [([9, 1], true), ([5, 2], false)] {
        let default = run(json!({"op": "contains_point", "seg": s, "point": p}));
        let explicit = run(json!({"op": "contains_point", "seg": s, "point": p, "threshold": 3}));
        assert_eq!(default["result"], json!(inside));
        assert_eq!(default["result"], explicit["result"]);
    }
    let default = run(json!({"op": "line_distance", "seg": s, "point": [3, -4]}));
    let explicit =
        run(json!({"op": "line_distance", "seg": s, "point": [3, -4], "determine_side": false}));
    let side =
        run(json!({"op": "line_distance", "seg": s, "point": [3, -4], "determine_side": true}));
    assert_eq!(default["result"], json!(4));
    assert_eq!(default["result"], explicit["result"]);
    assert_eq!(side["result"], json!(-4));
    let collide =
        run(json!({"op": "collide", "seg": s, "other": [[0, 3], [10, 3]], "clearance": 4}));
    assert_eq!(collide["result"], json!({"returned": true, "actual": null}));
}
