// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;

const CASES: usize = 7545;
const CONTROLS: usize = 805;
/// Public cases whose DIRECTION_45 is built from a three-point SHAPE_ARC.
const DEFERRED: usize = 81;

fn cases() -> impl Iterator<Item = &'static str> {
    include_str!("../../../port/cases/libs.kimath.include.geometry.application.pure.p001.jsonl")
        .lines()
}

fn controls() -> Vec<Value> {
    include_str!("../../../port/oracle/fixtures/geometry-pure-p001-controls.jsonl")
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Every public case either succeeds or defers to exactly this owner, this many times.
#[test]
fn public_cases_succeed_or_defer_to_their_actual_owners() {
    let mut deferred: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut ok = 0;
    for line in cases() {
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
    let expected: BTreeMap<(String, String), usize> = [(
        "libs.kimath.src.geometry.shape_arc.application.ui.p001".to_string(),
        "SHAPE_ARC::SHAPE_ARC(const VECTOR2I&, const VECTOR2I&, const VECTOR2I&, int)".to_string(),
        DEFERRED,
    )]
    .into_iter()
    .map(|(u, s, n)| ((u, s), n))
    .collect();
    assert_eq!(deferred, expected);
    assert_eq!(ok + DEFERRED, CASES);
}

/// Every control gives its recorded refusal, or the recorded deferral first.
#[test]
fn controls_refuse_or_defer_as_recorded() {
    let controls = controls();
    assert_eq!(controls.len(), CONTROLS);
    for control in controls {
        let reply = dispatch(&control["request"].to_string());
        let expected = match control.get("rust_unported") {
            Some(owner) => {
                json!({"kind": "unported", "unit": owner["unit"], "upstream": owner["upstream"]})
            }
            None => control["error"].clone(),
        };
        assert_eq!(reply["error"], expected, "{}", control["request"]);
    }
}

fn request(input: Value) -> Value {
    dispatch(&json!({"unit": UNIT, "case": "t", "input": input}).to_string())
}

/// The default arguments: omitting `only45`, `a90` and `aPadding` equals passing their defaults.
#[test]
fn default_arguments_equal_their_values() {
    for vec in [[10, 3], [3, 10], [10, 6], [-6, 10]] {
        let omitted = request(json!({"op": "snapped45", "type": "int", "vec": vec}));
        let given = request(json!({"op": "snapped45", "type": "int", "vec": vec, "only45": false}));
        assert_eq!(omitted["result"], given["result"]);
    }
    for vec in [[10, -3], [0, 0], [-7, 7]] {
        for member in ["state", "right", "left"] {
            let omitted = request(json!({"op": "direction45", "member": member,
                                         "dir": {"ctor": "vector", "vec": vec}}));
            let given = request(json!({"op": "direction45", "member": member,
                                       "dir": {"ctor": "vector", "vec": vec, "a90": false}}));
            assert_eq!(omitted["result"], given["result"]);
        }
    }
    let omitted = request(
        json!({"op": "clamped_coords", "in": "int", "ret": "int", "coords": [i32::MIN, i32::MAX]}),
    );
    let given = request(json!({"op": "clamped_coords", "in": "int", "ret": "int",
                               "coords": [i32::MIN, i32::MAX], "padding": 1}));
    assert_eq!(omitted["result"], given["result"]);
    assert_eq!(
        omitted["result"]["coords"],
        json!([-(i32::MAX - 1), i32::MAX - 1])
    );
}

/// DIRECTION_45 on the eight compass vectors in screen coordinates (y down).
#[test]
fn compass_directions() {
    let names = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    let vectors = [
        [0, -1],
        [1, -1],
        [1, 0],
        [1, 1],
        [0, 1],
        [-1, 1],
        [-1, 0],
        [-1, -1],
    ];
    for (k, vec) in vectors.iter().enumerate() {
        let reply = request(json!({"op": "direction45", "member": "format",
                                   "dir": {"ctor": "vector", "vec": vec}}));
        assert_eq!(reply["result"]["value"], names[k]);
        let back = request(json!({"op": "direction45", "member": "to_vector",
                                  "dir": {"ctor": "directions", "value": k}}));
        assert_eq!(back["result"]["value"], json!(vec));
    }
}
