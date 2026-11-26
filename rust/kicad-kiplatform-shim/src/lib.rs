// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! JSONL adapter for unit `libs.kiplatform.os.unix.application.thin.replacement.p001`.
//! It mirrors `port/oracle/shims/kiplatform_policy.cpp` field for field, including
//! validation order and refusal messages. Request input:
//!
//! ```json
//! {"op": "get_policy_bool" | "get_policy_enum_uint", "key": [<wchar_t unit>, ...]}
//! ```
//!
//! `key` holds at most [`MAX_KEY_UNITS`] integers in `i32::MIN..=i32::MAX`, the
//! signed 32-bit `wchar_t` units of the `wxString` key, encoded as the accepted
//! map_helpers adapter encodes them; any such value is accepted. The key is built
//! with the accepted replacement [`WxString::from_units`]. The result is
//! `{"key_length": <units>, "value": "NOT_CONFIGURED" | null}`, where `key_length`
//! is an adapter check that the whole key reached the call.

use kicad_kiplatform::policy::{self, PBool};
use kicad_map_helpers::{WChar, WxString};
use serde_json::{json, Value};

pub use kicad_kiplatform::UNIT;

/// Largest accepted key, in wide code units.
pub const MAX_KEY_UNITS: usize = 4096;

fn keys(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    })
}

fn unit(value: &Value) -> Option<WChar> {
    // Arbitrary-precision parsing keeps integer -0, which the C++ JSON reader
    // accepts as the integer 0; as_i64 reads it as 0 and refuses floats, exponents
    // and integers outside i64.
    value.as_i64().and_then(|v| WChar::try_from(v).ok())
}

fn pbool_name(value: PBool) -> &'static str {
    match value {
        PBool::Enabled => "ENABLED",
        PBool::Disabled => "DISABLED",
        PBool::NotConfigured => "NOT_CONFIGURED",
    }
}

/// Run one validated operation, or return the refusal message.
pub fn run(input: &Value) -> Result<Value, &'static str> {
    if !keys(input, &["op", "key"]) {
        return Err("unexpected or missing field");
    }
    let op = input["op"].as_str().ok_or("expected op string")?;
    let array = input["key"].as_array().ok_or("expected key array")?;
    if array.len() > MAX_KEY_UNITS {
        return Err("key longer than 4096 units");
    }
    let units = array
        .iter()
        .map(unit)
        .collect::<Option<Vec<WChar>>>()
        .ok_or("key unit out of range")?;
    // wxString( units.data(), units.size() ): every unit, including NULs. At most
    // MAX_KEY_UNITS units, so only an allocator refusal (std::bad_alloc in the
    // reference) can fail here.
    let key = WxString::from_units(&units).map_err(|_| "key allocation failed")?;
    let value = match op {
        "get_policy_bool" => json!(pbool_name(policy::get_policy_bool(&key))),
        "get_policy_enum_uint" => match policy::get_policy_enum_uint(&key) {
            Some(value) => json!(value),
            None => Value::Null,
        },
        _ => return Err("unknown operation"),
    };
    Ok(json!({"key_length": key.len(), "value": value}))
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
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(message) => failure(unit, case, "invalid_input", message),
    }
}
