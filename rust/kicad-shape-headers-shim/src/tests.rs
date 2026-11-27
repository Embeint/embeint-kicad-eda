// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;

const CASES: usize = 4101;
const CONTROLS: usize = 446;
const ARC: &str = "libs.kimath.src.geometry.shape_arc.application.ui.p001";
const COMPOUND: &str = "libs.kimath.src.geometry.shape_compound.application.ui.p001";
const ELLIPSE: &str = "libs.kimath.src.geometry.shape_ellipse.application.ui.p001";

/// Every public case either succeeds or defers to exactly these owners, these many times.
#[test]
fn public_cases_succeed_or_defer_to_their_actual_owners() {
    let mut deferred: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut ok = 0;
    for line in
        include_str!("../../../port/cases/libs.kimath.include.geometry.application.ui.p003.jsonl")
            .lines()
    {
        let reply = dispatch(line);
        if reply.get("result").is_some() {
            ok += 1;
            continue;
        }
        assert_eq!(reply["error"]["kind"], "unported", "{line} -> {reply}");
        let field = |k: &str| reply["error"][k].as_str().unwrap_or_default().to_string();
        *deferred
            .entry((field("unit"), field("upstream")))
            .or_default() += 1;
    }
    let expected: BTreeMap<(String, String), usize> = [
        (ARC, "SHAPE_ARC::SHAPE_ARC(const SHAPE_ARC&)", 71),
        (
            ARC,
            "SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const VECTOR2I&, int)",
            30,
        ),
        (ARC, "SHAPE_ARC::BBox", 2),
        (
            "libs.kimath.include.application.ui.p001",
            "RotatePoint(VECTOR2I&, const VECTOR2I&, EDA_ANGLE)",
            34,
        ),
        (
            "libs.kimath.src.geometry.shape_collisions.application.ui.p001",
            "SHAPE::Collide(const SHAPE*, int, int*, VECTOR2I*)",
            20,
        ),
        (COMPOUND, "SHAPE_COMPOUND::BBox", 10),
        (
            COMPOUND,
            "SHAPE_COMPOUND::Collide(const SEG&, int, int*, VECTOR2I*)",
            10,
        ),
        (COMPOUND, "SHAPE_COMPOUND::IsSolid", 5),
        (COMPOUND, "SHAPE_COMPOUND::Move", 5),
        (COMPOUND, "SHAPE_COMPOUND::Rotate", 5),
        (COMPOUND, "SHAPE_COMPOUND::Clone", 5),
        (ELLIPSE, "SHAPE_ELLIPSE::SHAPE_ELLIPSE()", 8),
        (
            ELLIPSE,
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, int, int, const EDA_ANGLE&)",
            8,
        ),
        (
            ELLIPSE,
            "SHAPE_ELLIPSE::SHAPE_ELLIPSE(const VECTOR2I&, int, int, const EDA_ANGLE&, const EDA_ANGLE&, const EDA_ANGLE&)",
            8,
        ),
    ]
    .into_iter()
    .map(|(u, s, n)| ((u.to_string(), s.to_string()), n))
    .collect();
    assert_eq!(deferred, expected);
    assert_eq!(ok, 3880);
    assert_eq!(ok + deferred.values().sum::<usize>(), CASES);
}

#[test]
fn controls_refuse_or_defer_exactly_as_recorded() -> core::result::Result<(), serde_json::Error> {
    let mut count = 0;
    for line in include_str!("../../../port/oracle/fixtures/shape-headers-controls.jsonl").lines() {
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

fn request(input: Value) -> Value {
    dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string())
}

#[test]
fn malformed_requests_are_refused_before_any_operation() {
    let s = json!([[0, 0], [10, 0]]);
    for (input, message) in [
        (json!({"seg": s}), "expected string: op"),
        (json!({"op": "nope"}), "unknown operation"),
        (json!({"op": "seg_length"}), "unexpected or missing field"),
        (
            json!({"op": "seg_length", "seg": s, "x": 1}),
            "unexpected or missing field",
        ),
        (
            json!({"op": "seg_length", "seg": [[0, 0]]}),
            "expected segment: seg",
        ),
        (
            json!({"op": "seg_side", "seg": s, "point": [2147483648_i64, 0]}),
            "integer out of range",
        ),
        (
            json!({"op": "seg_new", "ctor": "coords", "coords": [1, 2, 3]}),
            "expected array: coords",
        ),
        (
            json!({"op": "shape_type_as_string", "value": 11}),
            "integer out of range",
        ),
        (
            json!({"op": "shape", "shape": {"kind": "square", "ctor": "default"}, "calls": []}),
            "unknown shape",
        ),
        (
            json!({"op": "shape", "shape": {"kind": "circle", "ctor": "nope", "center": [0, 0],
                                            "radius": 1}, "calls": []}),
            "unknown constructor",
        ),
        (
            json!({"op": "shape", "shape": {"kind": "circle", "ctor": "default"},
                   "calls": [{"call": "get_p0"}]}),
            "unknown call",
        ),
        (
            json!({"op": "shape", "shape": {"kind": "circle", "ctor": "default"},
                   "calls": [{"call": "rotate", "angle": "xyz"}]}),
            "invalid hex: angle",
        ),
        (
            json!({"op": "shape", "shape": {"kind": "circle", "ctor": "default"},
                   "calls": [{"call": "indexable_subshapes", "prefill": 9}]}),
            "integer out of range",
        ),
        (
            json!({"op": "shape", "shape": {"kind": "circle", "ctor": "default"},
                   "calls": [7]}),
            "expected object",
        ),
    ] {
        let reply = request(input);
        assert_eq!(
            reply["error"],
            json!({"kind": "invalid_input", "message": message}),
            "{reply}"
        );
    }
}

#[test]
fn out_of_domain_calls_are_refused_before_the_call() {
    // An arc's out-of-line Collide is never in the verified domain.
    let reply = request(
        json!({"op": "shape", "shape": {"kind": "arc", "ctor": "default"},
                               "calls": [{"call": "collide_point", "point": [0, 0]}]}),
    );
    assert_eq!(reply["error"]["message"], OUTSIDE, "{reply}");
    // A compound holding an arc.
    let reply = request(json!({"op": "shape", "shape": {"kind": "compound", "add": [
        {"shape": {"kind": "arc", "ctor": "default"}, "shared": false}]},
        "calls": [{"call": "is_solid"}]}));
    assert_eq!(reply["error"]["message"], OUTSIDE, "{reply}");
}
