// SPDX-License-Identifier: GPL-3.0-or-later
//! Execute every corpus operation against actual C++ replies, without host I/O.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

#[path = "../../kicad-resistors-shim/src/operations.rs"]
mod operations;
use serde_json::Value;

const CASES: &str = include_str!("../../../port/cases/pcb_calculator.application.pure.p001.jsonl");
const GOLDEN: &str =
    include_str!("../../../port/goldens/pcb_calculator.application.pure.p001.jsonl");

// Every protocol result number is an integer (including explicit f64 bits).
// Reject fixture floats before Value equality could coerce numeric types.
fn integer_numbers(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.as_u64().is_some() || number.as_i64().is_some(),
        Value::Array(items) => items.iter().all(integer_numbers),
        Value::Object(items) => items.values().all(integer_numbers),
        _ => true,
    }
}

fn main() -> Result<(), String> {
    let cases = CASES.lines().collect::<Vec<_>>();
    let golden = GOLDEN.lines().collect::<Vec<_>>();
    if cases.len() != 1050 || golden.len() != cases.len() {
        return Err("complete 1050-case golden required".into());
    }
    for (request, reply) in cases.iter().zip(golden) {
        let request: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let reply: Value = serde_json::from_str(reply).map_err(|e| e.to_string())?;
        if request["unit"] != reply["unit"] || request["case"] != reply["case"] {
            return Err("fixture identity mismatch".into());
        }
        let context = request["case"].as_str().ok_or("fixture case")?;
        let actual = operations::run(&request["input"])?;
        let expected = reply.get("result").ok_or("fixture result")?;
        if !integer_numbers(&actual) || !integer_numbers(expected) || actual != *expected {
            return Err(format!("full wasm result mismatch: {context}"));
        }
    }
    Ok(())
}
