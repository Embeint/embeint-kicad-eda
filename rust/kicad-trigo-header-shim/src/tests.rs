// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;
const CASES: usize = 4215;
const CONTROLS: usize = 274;
/// Public cases whose forwarded rotation reaches a logging KiROUND.
const DEFERRED: usize = 109;

/// Every public case either succeeds or defers to exactly these owners, these many times.
#[test]
fn public_cases_succeed_or_defer_to_their_actual_owners() {
    let mut deferred: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut ok = 0;
    for line in
        include_str!("../../../port/cases/libs.kimath.include.application.ui.p001.jsonl").lines()
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
    // A saturating KiROUND in the forwarded trigo.cpp rotation logs through the math util
    // unit's kimathLogOverflow, which the Rust port defers.
    let expected: BTreeMap<(String, String), usize> = [(
        "libs.kimath.src.math.util.application.ui.p001",
        "kimathLogOverflow",
        DEFERRED,
    )]
    .into_iter()
    .map(|(u, s, n)| ((u.to_string(), s.to_string()), n))
    .collect();
    assert_eq!(deferred, expected);
    assert_eq!(ok, CASES - DEFERRED);
}
#[test]
fn controls_refuse_or_defer_exactly_as_recorded() -> Result<(), serde_json::Error> {
    let mut count = 0;
    for line in include_str!("../../../port/oracle/fixtures/trigo-header-controls.jsonl").lines() {
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

/// `GetRotated` and the in-place `RotatePoint` wrapper give the same reply for every public
/// request of either form.
#[test]
fn get_rotated_agrees_with_rotate_point() -> Result<(), serde_json::Error> {
    let pairs = [
        ("rotate_i", "get_rotated_i"),
        ("rotate_i_about", "get_rotated_i_about"),
        ("rotate_d", "get_rotated_d"),
    ];
    let mut checked = 0;
    for line in
        include_str!("../../../port/cases/libs.kimath.include.application.ui.p001.jsonl").lines()
    {
        let mut request: Value = serde_json::from_str(line)?;
        let op = request["input"]["op"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let Some((a, b)) = pairs.iter().find(|(a, b)| op == *a || op == *b) else {
            continue;
        };
        let first = dispatch(line);
        request["input"]["op"] = json!(if op == *a { *b } else { *a });
        let second = dispatch(&request.to_string());
        assert_eq!(first.get("result"), second.get("result"), "{line}");
        assert_eq!(first.get("error"), second.get("error"), "{line}");
        checked += 1;
    }
    assert!(checked > 2000, "{checked}");
    Ok(())
}

#[test]
fn malformed_requests_are_refused_before_any_operation() {
    let base =
        |input: Value| dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
    let zero = "0000000000000000";
    for input in [
        json!({"op": "nope"}),
        json!({"op": "rotate_i", "point": [0, 0]}),
        json!({"op": "rotate_i", "point": [0, 0], "angle": zero, "x": 1}),
        json!({"op": "rotate_i_about", "point": [0, 0], "center": [0], "angle": zero}),
        json!({"op": "get_rotated_i", "point": [2147483648_i64, 0], "angle": zero}),
        json!({"op": "rotate_d", "point": [zero, "NOTHEX0000000000"], "angle": zero}),
        json!({"op": "deg2rad", "value": 1.0}),
        json!({"op": "normalize_angle_pos", "type": "int", "angle": 2147483648_i64}),
        json!({"op": "normalize_angle_180", "type": "long long", "angle": LOOP_BOUND + 1}),
        json!({"op": "normalize_angle_180", "type": "unsigned", "angle": 0}),
        json!({"op": "intercepts_positive_x", "start": zero}),
    ] {
        assert_eq!(base(input)["error"]["kind"], "invalid_input");
    }
    assert_eq!(
        base(json!({"op": "normalize_angle_pos", "type": "long long", "angle": LOOP_BOUND}))
            ["result"]["angle"],
        json!(0)
    );
    assert_eq!(dispatch("{")["error"]["kind"], "invalid_request");
    assert_eq!(
        dispatch(&json!({"unit": "other", "case": "c", "input": {}}).to_string())["error"]["kind"],
        "unknown_unit"
    );
}
