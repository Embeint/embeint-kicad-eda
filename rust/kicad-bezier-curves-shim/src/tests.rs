// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const CASES: &str =
    include_str!("../../../port/cases/libs.kimath.src.bezier_curves.application.ui.p001.jsonl");
const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/bezier-curves-controls.jsonl");

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn request(input: Value) -> String {
    json!({"unit": UNIT, "case": "t", "input": input}).to_string()
}

#[test]
fn every_public_case_succeeds_or_defers_at_the_assertion_host() {
    let mut deferred = 0;
    let mut succeeded = 0;
    for line in CASES.lines() {
        let reply = dispatch(line);
        let case = parse(line)["case"].clone();
        assert_eq!(reply["case"], case);
        if let Some(error) = reply.get("error") {
            assert_eq!(error["kind"], "unported", "{case}: {error}");
            assert_eq!(error["upstream"], bc::WX_ON_ASSERT, "{case}");
            assert_eq!(error["unit"], UNIT, "{case}");
            deferred += 1;
        } else {
            assert_eq!(
                reply["result"]["assertions"].as_array().map_or(0, Vec::len),
                0
            );
            succeeded += 1;
        }
    }
    assert_eq!((succeeded, deferred), (2142, 38));
}

#[test]
fn controls_give_the_recorded_refusal() {
    let mut count = 0;
    for line in CONTROLS.lines() {
        let control = parse(line);
        assert!(control.get("rust_unported").is_none());
        let reply = dispatch(&control["request"].to_string());
        assert_eq!(
            reply["error"], control["error"],
            "{}",
            control["request"]["case"]
        );
        count += 1;
    }
    assert_eq!(count, 138);
}

#[test]
fn defaulted_errors_equal_the_explicit_defaults() {
    let points = json!([[0, 0], [1000, 2000], [3000, 2000], [4000, 0]]);
    let default_i = dispatch(&request(
        json!({"op": "get_poly_i", "form": "points4", "points": points, "output": []}),
    ));
    let explicit_i = dispatch(&request(json!({"op": "get_poly_i", "form": "points4",
        "points": points, "output": [], "max_error": 10})));
    assert_eq!(default_i, explicit_i);
    assert!(default_i["result"]["output"]
        .as_array()
        .is_some_and(|o| o.len() > 2));
    let d = |v: f64| bits(v);
    let points_d = json!([[d(0.0), d(0.0)], [d(1.0), d(2.0)], [d(3.0), d(0.0)]]);
    let default_d = dispatch(&request(
        json!({"op": "get_poly_d", "form": "vector_d", "points": points_d, "output": []}),
    ));
    let explicit_d = dispatch(&request(json!({"op": "get_poly_d", "form": "vector_d",
        "points": points_d, "output": [], "max_error": d(10.0)})));
    assert_eq!(default_d, explicit_d);
}

#[test]
fn malformed_requests_are_refused() {
    assert_eq!(
        dispatch("{")["error"]["message"],
        "invalid JSON or duplicate key"
    );
    assert_eq!(
        dispatch(r#"{"unit":"x","case":"c","input":{}}"#)["error"]["kind"],
        "unknown_unit"
    );
    assert_eq!(
        dispatch(&request(json!({"op": "construct"})))["error"]["message"],
        "unexpected or missing field"
    );
    assert_eq!(
        dispatch(&request(
            json!({"op": "eval", "points": [[1]], "t": bits(0.5)})
        ))["error"]["message"],
        "expected point"
    );
}

#[test]
fn shim_budgets_match_the_adapter() {
    let adapter = include_str!("../../../port/oracle/shims/kimath_src_bezier_curves.cpp");
    assert!(adapter.contains("constexpr std::size_t MAX_POINTS = 4096;"));
    assert!(adapter.contains("constexpr uint64_t STEP_BUDGET = uint64_t( 1 ) << 20;"));
    assert!(adapter.contains("constexpr int MAX_BEZIERS = 4096;"));
    assert_eq!(
        (MAX_POINTS, STEP_BUDGET, MAX_BEZIERS),
        (4096, 1 << 20, 4096)
    );
}
