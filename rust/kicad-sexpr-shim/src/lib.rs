// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for the `libs/sexpr` ports. It mirrors `port/oracle/shims/libs_sexpr.cpp`:
//! one operation per request (`input.op`), the encoding of [`codec`], and the same
//! `invalid_input` refusal where the pinned evaluation is undefined.
//!
//! Registered units: `libs.sexpr.include.sexpr.application.pure.p001` ([`header_ops`]) and
//! `libs.sexpr.application.pure.p001` ([`source_ops`]).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

pub mod args;
pub mod codec;
pub mod header_ops;
pub mod source_ops;

use codec::{object, text, Error};
use kicad_sexpr::{HEADER_UNIT, SOURCE_UNIT};
use serde_json::{json, Value};

fn run(unit: &str, input: &Value) -> Result<Value, Error> {
    let input = object(input)?;
    let op = text(input, "op")?;
    match unit {
        HEADER_UNIT => header_ops::run(op, input),
        SOURCE_UNIT => source_ops::run(op, input),
        _ => Err("unknown operation".into()),
    }
}

/// Answer one request line.
pub fn dispatch(line: &str) -> Value {
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case": case, "error": {"kind": kind, "message": message}, "unit": unit});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                Value::Null,
                Value::Null,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    let unit = request
        .get("unit")
        .filter(|v| v.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    let case = request
        .get("case")
        .filter(|v| v.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    let shaped = request.as_object().is_some_and(|o| {
        o.len() == 3 && ["unit", "case", "input"].iter().all(|k| o.contains_key(*k))
    });
    if !shaped
        || unit.as_str().is_none_or(str::is_empty)
        || case.as_str().is_none_or(str::is_empty)
        || !request["input"].is_object()
    {
        return failure(
            unit,
            case,
            "invalid_request",
            "expected unit, case, input object",
        );
    }
    let name = unit.as_str().unwrap_or_default();
    if name != HEADER_UNIT && name != SOURCE_UNIT {
        return failure(unit, case, "unknown_unit", "unit has no registered shim");
    }
    match run(name, &request["input"]) {
        Ok(result) => json!({"case": case, "result": result, "unit": unit}),
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(Error::Unported {
            upstream,
            unit: owner,
        }) => {
            json!({"case": case, "error": {"kind": "unported", "unit": owner, "upstream": upstream}, "unit": unit})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(input: Value) -> Value {
        dispatch(&json!({"unit": HEADER_UNIT, "case": "t", "input": input}).to_string())
    }

    #[test]
    fn decodes_and_reports_trees() {
        let reply = request(json!({"op": "predicates", "tree": [
            ["list", null, 3], ["integer", -5, 7], ["null"], ["list", -1, 1], ["symbol", "61", null]
        ]}));
        assert_eq!(
            reply["result"]["tree"],
            json!([
                ["list", 1, 3],
                ["integer", -5, 7],
                ["null"],
                ["list", 18446744073709551615u64, 1],
                ["symbol", "61", 1]
            ])
        );
        assert_eq!(reply["result"]["predicates"][2], Value::Null);
        assert_eq!(
            reply["result"]["predicates"][1],
            json!([false, false, false, false, true, 7])
        );
    }

    #[test]
    fn malformed_trees_are_refused() {
        for (tree, message) in [
            (json!([]), "empty tree"),
            (json!([["null"]]), "null root"),
            (json!([["list", null, 2], ["null"]]), "truncated tree"),
            (json!([["integer", 1, null], ["null"]]), "trailing nodes"),
            (json!([["integer", 1.5, null]]), "invalid node"),
            (json!([["string", "ABC", null]]), "invalid node"),
            (json!([["list", 2147483648u64, 0]]), "invalid node"),
        ] {
            let reply = request(json!({"op": "predicates", "tree": tree}));
            assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
            assert_eq!(reply["error"]["message"], message, "{reply}");
        }
    }

    #[test]
    fn deep_trees_within_the_bound_round_trip_and_beyond_it_are_refused() {
        let mut tree: Vec<Value> = (0..codec::MAX_DEPTH - 1)
            .map(|_| json!(["list", null, 1]))
            .collect();
        tree.push(json!(["integer", 0, null]));
        let reply = request(json!({"op": "predicates", "tree": tree.clone()}));
        assert_eq!(
            reply["result"]["tree"].as_array().map(Vec::len),
            Some(codec::MAX_DEPTH)
        );
        tree.insert(0, json!(["list", null, 1]));
        let reply = request(json!({"op": "predicates", "tree": tree}));
        assert_eq!(reply["error"]["message"], "tree too deep");
    }

    /// Every public case of a corpus must give a result: no deferral or refusal remains.
    fn all_succeed(corpus: &str) -> usize {
        let mut count = 0;
        for line in corpus.lines() {
            let reply = dispatch(line);
            assert!(reply.get("result").is_some(), "{reply}");
            count += 1;
        }
        count
    }

    #[test]
    fn header_cases_all_succeed() {
        let count = all_succeed(include_str!(
            "../../../port/cases/libs.sexpr.include.sexpr.application.pure.p001.jsonl"
        ));
        assert_eq!(count, 1372);
    }

    #[test]
    fn source_cases_all_succeed() {
        let count = all_succeed(include_str!(
            "../../../port/cases/libs.sexpr.application.pure.p001.jsonl"
        ));
        assert_eq!(count, SOURCE_CASES);
    }

    /// The source corpus size (generate_cases.py).
    const SOURCE_CASES: usize = 5765;
    /// The control count (generate_cases.py).
    const CONTROLS: usize = 97;

    #[test]
    fn controls_refuse_exactly_as_recorded() -> Result<(), serde_json::Error> {
        let mut count = 0;
        for line in include_str!("../../../port/oracle/fixtures/sexpr-controls.jsonl").lines() {
            let control: Value = serde_json::from_str(line)?;
            let request = &control["request"];
            assert!(
                control.get("rust_unported").is_none(),
                "{}",
                request["case"]
            );
            let reply = dispatch(&request.to_string());
            let expected = json!({"case": request["case"], "error": control["error"], "unit": request["unit"]});
            assert_eq!(reply, expected, "{}", request["case"]);
            count += 1;
        }
        assert_eq!(count, CONTROLS);
        Ok(())
    }

    #[test]
    fn unknown_units_are_refused() {
        let reply = dispatch(
            &json!({"unit": "libs.sexpr.application.ui.p001", "case": "t", "input": {"op": "x"}})
                .to_string(),
        );
        assert_eq!(reply["error"]["kind"], "unknown_unit");
        let reply = dispatch(
            &json!({"unit": kicad_sexpr::SOURCE_UNIT, "case": "t", "input": {"op": "x"}})
                .to_string(),
        );
        assert_eq!(reply["error"]["message"], "unknown operation");
    }
}
