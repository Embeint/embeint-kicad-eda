// SPDX-License-Identifier: GPL-3.0-or-later
//! Native shim request parsing, separate from arbitrary-precision result comparison.

use crate::exact::{self, Exact};
use serde_json::Value;

const INVALID_JSON: &str = "invalid JSON or duplicate key";

/// Parse a native shim request before extracting envelope identities or checking its domain.
///
/// Rejects invalid JSON, duplicate keys, excessive nesting and numbers that overflow the
/// pinned nlohmann parser's `double` fallback. Finite out-of-range integers and underflow
/// remain valid JSON. The returned arbitrary-precision value retains integer `-0` and the
/// full `u64` range; numeric conversion here is validation only.
///
/// On error, adapters should emit `invalid_request` with null unit/case and this message.
/// Envelope shape and unit-specific input validation remain the adapter's responsibility.
pub fn parse(text: &str) -> Result<Value, String> {
    let exact = exact::parse(text).map_err(|_| INVALID_JSON.to_owned())?;
    into_value(exact, 127).map_err(|_| INVALID_JSON.to_owned())
}

fn into_value(value: Exact, containers_left: usize) -> Result<Value, ()> {
    match value {
        // Build user objects directly. Value's generic deserializer interprets
        // serde's private Number/RawValue keys as typed internal markers.
        Exact::Object(values) => {
            let remaining = containers_left.checked_sub(1).ok_or(())?;
            values
                .into_iter()
                .map(|(key, value)| Ok((key, into_value(value, remaining)?)))
                .collect::<Result<_, ()>>()
                .map(Value::Object)
        }
        Exact::Array(values) => {
            let remaining = containers_left.checked_sub(1).ok_or(())?;
            values
                .into_iter()
                .map(|value| into_value(value, remaining))
                .collect::<Result<_, ()>>()
                .map(Value::Array)
        }
        Exact::String(value) => Ok(Value::String(value)),
        Exact::Scalar(token) => match token.as_str() {
            "null" => Ok(Value::Null),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            // exact::parse already validated syntax and bounded recursion. nlohmann falls
            // back to strtod for integers outside i64/u64 and rejects only non-finites.
            _ if token.parse::<f64>().is_ok_and(f64::is_finite) => token
                .parse::<serde_json::Number>()
                .map(Value::Number)
                .map_err(|_| ()),
            _ => Err(()),
        },
    }
}
