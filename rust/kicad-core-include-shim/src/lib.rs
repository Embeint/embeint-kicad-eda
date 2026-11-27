// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! JSONL adapter for unit `libs.core.include.core.application.pure.p001`. The operation
//! schema is in README.md and mirrors `port/oracle/shims/core_include_p001.cpp` field
//! for field, including validation order and refusal messages.

pub mod adversary;
pub mod mt19937;
pub mod operations;
pub mod profile_script;

use serde_json::{json, Value};

pub use kicad_core::HEADER_UNIT as UNIT;

fn keys(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    })
}

/// `adversary-weights.json`: one line per list, `{"<name>": [weights...]}` overall.
pub fn adversary_json(weights: &[(String, Vec<u64>)]) -> String {
    let lines: Vec<String> = weights
        .iter()
        .map(|(name, values)| format!(" {}: {}", json!(name), json!(values)))
        .collect();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

/// Answer one protocol line.
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
    if !keys(&request, &["unit", "case", "input"])
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
    match operations::run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(kicad_core_shim::operations::Error::Invalid(message)) => {
            failure(unit, case, "invalid_input", &message)
        }
        Err(kicad_core_shim::operations::Error::Unported {
            upstream,
            unit: owner,
        }) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
    }
}
