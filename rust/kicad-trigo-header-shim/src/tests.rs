// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;
const CASES: usize = 4693;
const CONTROLS: usize = 397;
/// `3600 * 2^24`: the angle whose loops take exactly [`PASS_BUDGET`] passes.
const BUDGET_ANGLE: i64 = 3600 << 24;
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
        json!({"op": "normalize_angle_180", "type": "long long", "angle": BUDGET_ANGLE + 1801}),
        json!({"op": "normalize_angle_pos", "type": "double", "angle": 0}),
        json!({"op": "normalize_angle_180", "type": "unsigned", "angle": 0}),
        json!({"op": "intercepts_positive_x", "start": zero}),
    ] {
        assert_eq!(base(input)["error"]["kind"], "invalid_input");
    }
    assert_eq!(
        base(json!({"op": "normalize_angle_pos", "type": "long long", "angle": BUDGET_ANGLE}))
            ["result"]["angle"],
        json!(0)
    );
    assert_eq!(dispatch("{")["error"]["kind"], "invalid_request");
    assert_eq!(
        dispatch(&json!({"unit": "other", "case": "c", "input": {}}).to_string())["error"]["kind"],
        "unknown_unit"
    );
}

/// The literal passes of the template loops, for counts small enough to run.
fn literal_passes(op: &str, angle: f64) -> i128 {
    let half = op == "normalize_angle_180";
    let mut a = angle;
    let mut passes = 0;
    while if half { a <= -1800.0 } else { a < 0.0 } {
        a += 3600.0;
        passes += 1;
    }
    while if half { a > 1800.0 } else { a >= 3600.0 } {
        a -= 3600.0;
        passes += 1;
    }
    passes
}

/// The pass count deciding admission is the literal count, except for the one uncounted pass
/// after a `NormalizeAnglePos` first loop that rounds up to 3600 (a single-pass angle).
#[test]
fn pass_counts_equal_the_literal_loops() {
    for op in ["normalize_angle_pos", "normalize_angle_180"] {
        for base in [
            -36_000_i64,
            -7200,
            -3600,
            -1800,
            0,
            1800,
            3600,
            7200,
            36_000,
        ] {
            for d in -3..=3 {
                let a = base + d;
                assert_eq!(
                    template_passes(op, i128::from(a), false),
                    literal_passes(op, a as f64),
                    "{op} {a}"
                );
                for g in [0.25, 0.5, 0.75] {
                    let x = a as f64 + g;
                    let counted = template_passes(op, i128::from(a), true);
                    assert_eq!(counted, literal_passes(op, x), "{op} {x}");
                }
            }
        }
        for x in [-1e-300, -5e-324, -0.1, -2.5e-13] {
            let counted = template_passes(op, -1, true);
            let literal = literal_passes(op, x);
            assert!(
                counted == literal || (counted == 1 && literal == 2),
                "{op} {x}"
            );
        }
    }
    // The budget boundaries.
    let b = i128::from(BUDGET_ANGLE);
    assert_eq!(
        template_passes("normalize_angle_pos", b + 3599, false),
        PASS_BUDGET
    );
    assert_eq!(
        template_passes("normalize_angle_pos", b + 3600, false),
        PASS_BUDGET + 1
    );
    assert_eq!(
        template_passes("normalize_angle_pos", -b, false),
        PASS_BUDGET
    );
    assert_eq!(
        template_passes("normalize_angle_pos", -b - 1, false),
        PASS_BUDGET + 1
    );
    assert_eq!(
        template_passes("normalize_angle_180", b + 1800, false),
        PASS_BUDGET
    );
    assert_eq!(
        template_passes("normalize_angle_180", b + 1801, false),
        PASS_BUDGET + 1
    );
    assert_eq!(
        template_passes("normalize_angle_180", -b - 1799, false),
        PASS_BUDGET
    );
    assert_eq!(
        template_passes("normalize_angle_180", -b - 1800, false),
        PASS_BUDGET + 1
    );
    assert!(over_budget("normalize_angle_180", i64::MIN));
    assert!(over_budget("normalize_angle_pos", i64::MAX));
    assert!(!over_budget_d("normalize_angle_pos", f64::NAN));
    assert!(over_budget_d("normalize_angle_pos", 1e15));
    assert!(!over_budget_d(
        "normalize_angle_pos",
        -(BUDGET_ANGLE as f64)
    ));
    assert!(over_budget_d(
        "normalize_angle_pos",
        (-(BUDGET_ANGLE as f64)).next_down()
    ));
}
