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
    if !finite_numbers(&exact) {
        return Err(INVALID_JSON.to_owned());
    }
    serde_json::from_str(text).map_err(|_| INVALID_JSON.to_owned())
}

fn finite_numbers(value: &Exact) -> bool {
    match value {
        Exact::Object(values) => values.values().all(finite_numbers),
        Exact::Array(values) => values.iter().all(finite_numbers),
        Exact::String(_) => true,
        Exact::Scalar(token) => match token.as_str() {
            "null" | "true" | "false" => true,
            // exact::parse already validated syntax and bounded recursion. nlohmann falls
            // back to strtod for integers outside i64/u64 and rejects only non-finites.
            _ => token.parse::<f64>().is_ok_and(f64::is_finite),
        },
    }
}
