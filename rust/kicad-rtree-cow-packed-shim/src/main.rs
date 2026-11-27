// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_rtree_cow_packed_shim::operations::{run, UNIT};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
fn keys(v: &Value, ks: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if ks.iter().any(|k| !o.contains_key(*k)) || o.keys().any(|k| !ks.contains(&k.as_str())) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"unit":unit,"case":case,"error":{"kind":kind,"message":message}});
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
    if let Some(value) = request.get("unit").filter(|value| value.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|value| value.is_string()) {
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
        Ok(result) => json!({"unit":unit,"case":case,"result":result}),
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch(&line?))?;
        stdout.flush()?;
    }
    Ok(())
}
