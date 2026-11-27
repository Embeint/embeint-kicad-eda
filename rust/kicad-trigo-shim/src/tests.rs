// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;
const CASES: usize = 5987;
const CONTROLS: usize = 260;

/// Every public case either succeeds or defers to exactly these owners, these many times.
#[test]
fn public_cases_succeed_or_defer_to_their_actual_owners() {
    let mut deferred: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut ok = 0;
    for line in
        include_str!("../../../port/cases/libs.kimath.src.trigo.application.ui.p001.jsonl").lines()
    {
        let reply = dispatch(line);
        if reply.get("result").is_some() {
            ok += 1;
            continue;
        }
        assert_eq!(reply["error"]["kind"], "unported", "{reply}");
        let key = (
            reply["error"]["unit"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            reply["error"]["upstream"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        *deferred.entry(key).or_default() += 1;
    }
    let expected: BTreeMap<(String, String), usize> = [
        (
            "libs.kimath.src.math.util.application.ui.p001",
            "kimathLogOverflow",
            56,
        ),
        // Coincidence tests of three-point VECTOR2D centres whose difference has two different
        // NaNs: the vector owner defers that SquaredEuclideanNorm sum.
        (
            "libs.kimath.include.math.application.ui.p002",
            "VECTOR floating arithmetic: distinct NaN operands",
            34,
        ),
    ]
    .into_iter()
    .map(|(u, s, n)| ((u.to_string(), s.to_string()), n))
    .collect();
    assert_eq!(deferred, expected);
    assert_eq!(ok, 5897);
    assert_eq!(ok + deferred.values().sum::<usize>(), CASES);
}

#[test]
fn controls_refuse_or_defer_exactly_as_recorded() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in include_str!("../../../port/oracle/fixtures/trigo-controls.jsonl").lines() {
        let control: Value = serde_json::from_str(line)?;
        let reply = dispatch(&control["request"].to_string());
        let error = match control.get("rust_unported") {
            Some(owner) => json!({"kind": "unported", "unit": owner["unit"],
                                  "upstream": owner["upstream"]}),
            None => control["error"].clone(),
        };
        assert_eq!(reply["error"], error, "{reply}");
        count += 1;
    }
    assert_eq!(count, CONTROLS);
    Ok(())
}

#[test]
fn malformed_requests_are_refused_before_any_operation() {
    let base =
        |input: Value| dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
    let zero = "0000000000000000";
    for input in [
        json!({"op": "nope"}),
        json!({"op": "point_on_segment", "start": [0, 0], "end": [1, 1]}),
        json!({"op": "point_on_segment", "start": [0, 0], "end": [1, 1], "point": [0, 0], "x": 1}),
        json!({"op": "segment_intersects", "a1": [0, 0], "a2": [1, 1], "b1": [0, 1],
               "b2": [1, 0], "point": [0]}),
        json!({"op": "segment_hit", "ref": [0, 0], "start": [0, 0], "end": [1, 1],
               "dist": 2147483648_i64}),
        json!({"op": "arc_mid", "start": [0, 0], "end": [1, 1], "center": [0, 0],
               "min_arc_angle": 1}),
        json!({"op": "rotate_i", "point": [0, 0], "angle": "NOTHEX0000000000"}),
        json!({"op": "rotate_d", "point": [zero], "angle": zero}),
        json!({"op": "arc_center_i", "start": [0, 0], "mid": [0, 0]}),
    ] {
        assert_eq!(base(input)["error"]["kind"], "invalid_input");
    }
    assert_eq!(dispatch("{")["error"]["kind"], "invalid_request");
    assert_eq!(
        dispatch(&json!({"unit": "other", "case": "c", "input": {}}).to_string())["error"]["kind"],
        "unknown_unit"
    );
}
