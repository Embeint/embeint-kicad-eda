// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.distribute.application.ui.p001`. It mirrors
//! `port/oracle/shims/kimath_src_distribute.cpp`: a request holds one to eight calls of
//! `GetDeltasForDistributeByGaps` (`"gaps"`, `extents`) or `GetDeltasForDistributeByPoints`
//! (`"points"`, `positions`) on 32-bit integers, every call is decoded before any runs, and the
//! reply lists each call's deltas, or the signed overflow that leaves the pinned C++ undefined.
//! A refused call does not stop the independent calls after it.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::distribute::{
    get_deltas_for_distribute_by_gaps, get_deltas_for_distribute_by_points, DistributeError, UNIT,
};
use serde_json::{json, Value};

/// Bounds the calls in one request and the items in one call, as the C++ adapter does.
const MAX_CALLS: usize = 8;
const MAX_ITEMS: usize = 4096;

enum Call {
    Gaps(Vec<(i32, i32)>),
    Points(Vec<i32>),
}

fn keys(v: &Value, fields: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn array<'a>(v: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    v.get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected array: {name}"))
}
fn items<'a>(v: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    let items = array(v, name)?;
    if items.len() > MAX_ITEMS {
        return Err("items out of range".into());
    }
    Ok(items)
}
fn int(v: &Value) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| "coordinate out of range".into())
}
fn read_call(v: &Value) -> Result<Call, String> {
    let function = v
        .as_object()
        .and_then(|o| o.get("fn"))
        .and_then(Value::as_str)
        .ok_or("expected string: fn")?;
    match function {
        "gaps" => {
            keys(v, &["fn", "extents"])?;
            let extents = items(v, "extents")?
                .iter()
                .map(|e| match e.as_array().map(Vec::as_slice) {
                    Some([start, end]) => Ok((int(start)?, int(end)?)),
                    _ => Err("expected extent".to_string()),
                })
                .collect::<Result<_, _>>()?;
            Ok(Call::Gaps(extents))
        }
        "points" => {
            keys(v, &["fn", "positions"])?;
            let positions = items(v, "positions")?
                .iter()
                .map(int)
                .collect::<Result<_, _>>()?;
            Ok(Call::Points(positions))
        }
        _ => Err("unknown function".into()),
    }
}

enum Error {
    Invalid(String),
    Distribute(DistributeError),
}
impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Invalid(v)
    }
}
impl From<&str> for Error {
    fn from(v: &str) -> Self {
        Self::Invalid(v.into())
    }
}

fn run_call(call: &Call) -> Result<Value, Error> {
    let result = match call {
        Call::Gaps(extents) => get_deltas_for_distribute_by_gaps(extents),
        Call::Points(positions) => get_deltas_for_distribute_by_points(positions),
    };
    match result {
        Ok(deltas) => Ok(json!({ "deltas": deltas })),
        Err(DistributeError::SignedOverflow(op)) => Ok(json!({
            "refused": {"kind": "signed_overflow", "message": op.to_string()}
        })),
        Err(e) => Err(Error::Distribute(e)),
    }
}

fn run(v: &Value) -> Result<Value, Error> {
    keys(v, &["calls"])?;
    let input = array(v, "calls")?;
    if input.is_empty() || input.len() > MAX_CALLS {
        return Err("calls out of range".into());
    }
    let calls = input.iter().map(read_call).collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Array(
        calls.iter().map(run_call).collect::<Result<_, _>>()?,
    ))
}

pub fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case":case,"error":{"kind":kind,"message":message},"unit":unit});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                unit,
                case,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    if let Some(value) = request.get("unit").filter(|v| v.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|v| v.is_string()) {
        case = value.clone();
    }
    if keys(&request, &["unit", "case", "input"]).is_err()
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
    if unit != UNIT {
        return failure(unit, case, "unknown_unit", "unit has no registered shim");
    }
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): kimathLogOverflow, reached
        // only through KiROUND's non-quiet overflow branch.
        Err(Error::Distribute(DistributeError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
        // The accepted KiROUND has no other error for a double source; report it as a failure.
        Err(Error::Distribute(e)) => failure(unit, case, "internal", &format!("{e:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CASES: &str = include_str!(
        "../../../port/cases/libs.kimath.src.geometry.distribute.application.ui.p001.jsonl"
    );
    const CONTROLS: &str = include_str!("../../../port/oracle/fixtures/distribute-controls.jsonl");

    #[test]
    fn every_public_case_succeeds_without_deferral_or_refusal() {
        let mut count = 0;
        for line in CASES.lines() {
            let reply = dispatch(line);
            let calls = reply["result"].as_array();
            assert!(
                calls.is_some_and(|c| c.iter().all(|c| c["deltas"].is_array())),
                "{reply}"
            );
            count += 1;
        }
        assert!(count > 0);
    }

    #[test]
    fn every_control_refuses_exactly_the_recorded_calls_and_continues() {
        let mut count = 0;
        for line in CONTROLS.lines() {
            let control: Value = serde_json::from_str(line).unwrap_or(Value::Null);
            let reply = dispatch(&control["request"].to_string());
            let calls = reply["result"].as_array().cloned().unwrap_or_default();
            let refused = control["refused"].as_object().cloned().unwrap_or_default();
            assert!(!refused.is_empty(), "{control}");
            assert!(!calls.is_empty(), "{reply}");
            for (i, call) in calls.iter().enumerate() {
                match refused.get(&i.to_string()) {
                    Some(message) => {
                        assert_eq!(call["refused"]["kind"], "signed_overflow", "{reply}");
                        assert_eq!(&call["refused"]["message"], message, "{reply}");
                    }
                    None => assert!(call["deltas"].is_array(), "{reply}"),
                }
            }
            // A successful call follows the last refusal.
            assert!(
                calls.last().is_some_and(|c| c["deltas"].is_array()),
                "{reply}"
            );
            count += 1;
        }
        assert!(count > 0);
    }

    #[test]
    fn malformed_inputs_are_refused_before_any_call_runs() {
        let ok = json!({"fn": "points", "positions": [0, 1, 2]});
        for (input, message) in [
            (json!({}), "unexpected or missing field"),
            (
                json!({"calls": [ok], "x": 1}),
                "unexpected or missing field",
            ),
            (json!({"calls": 1}), "expected array: calls"),
            (json!({"calls": []}), "calls out of range"),
            (json!({"calls": vec![ok.clone(); 9]}), "calls out of range"),
            (json!({"calls": [1]}), "expected string: fn"),
            (json!({"calls": [{"fn": 1}]}), "expected string: fn"),
            (json!({"calls": [{"fn": "spread"}]}), "unknown function"),
            (
                json!({"calls": [{"fn": "gaps"}]}),
                "unexpected or missing field",
            ),
            (
                json!({"calls": [{"fn": "gaps", "positions": []}]}),
                "unexpected or missing field",
            ),
            (
                json!({"calls": [{"fn": "gaps", "extents": 1}]}),
                "expected array: extents",
            ),
            (
                json!({"calls": [{"fn": "points", "positions": vec![0; 4097]}]}),
                "items out of range",
            ),
            (
                json!({"calls": [{"fn": "gaps", "extents": [[0]]}]}),
                "expected extent",
            ),
            (
                json!({"calls": [{"fn": "gaps", "extents": [[0, 2147483648i64]]}]}),
                "coordinate out of range",
            ),
            (
                json!({"calls": [{"fn": "points", "positions": [1.0]}]}),
                "coordinate out of range",
            ),
            // A malformed later call fails the request even after a refused earlier call.
            (
                json!({"calls": [{"fn": "points", "positions": [-2147483648i64, 0, 1]},
                                  {"fn": "points", "positions": [-2147483649i64]}]}),
                "coordinate out of range",
            ),
        ] {
            let reply = dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string());
            assert_eq!(reply["error"]["kind"], "invalid_input", "{reply}");
            assert_eq!(reply["error"]["message"], message, "{reply}");
        }
        let reply = dispatch(
            &json!({"unit": UNIT, "case": "c", "input": {"calls": vec![
                json!({"fn": "points", "positions": vec![0; 4096]}); 8]}})
            .to_string(),
        );
        assert!(
            reply["result"].as_array().is_some_and(|s| s.len() == 8),
            "{reply}"
        );
    }
}
