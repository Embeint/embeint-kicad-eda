// SPDX-License-Identifier: GPL-3.0-or-later
//! Symbol pin layout for Embeint KiCad.
//!
//! A port of `layout_box` from `tools/live_schematic_mcp/symbol_layout.py`, the operation the MCP
//! tool `live_symbol_add_pins` uses to place a library symbol's pins and body.  The Python
//! implementation stays authoritative; `rust/parity/parity.py` checks that this crate returns the
//! same JSON for the same request.
//!
//! The crate is portable: no filesystem, sockets, processes or GUI types, so the same code runs
//! natively and in a browser through `embeint-web`.  Coordinates are integer nanometres
//! internally and are written to JSON as exact decimal millimetres (see [`mm_number`]).
//!
//! # JSON boundary
//!
//! Request: `{"pins": [{...}, ...], "arrange": "auto" | "as_given", "group_gap": 0..6}`, with
//! `arrange` and `group_gap` optional (defaults `"auto"` and `2`), matching the MCP tool.
//!
//! Response: the Python result, `{"body": [x1, y1, x2, y2], "pins": [{...}, ...]}` in millimetres,
//! or `{"error": {"type": "...", "message": "..."}}`.  `type` is `RequestError` for a request that
//! does not have the shape above, otherwise the Python exception (`ValueError`, or `KeyError` /
//! `TypeError` for pin fields the Python code does not validate) with Python's message.

mod layout;
mod pyvalue;

use serde_json::{Map, Value};

pub use layout::{layout_box, LayoutError, GRID_NM, PIN_LENGTH_NM};

/// Parameters of one layout request, after the request shape has been checked.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutRequest {
    pub pins: Vec<Map<String, Value>>,
    pub arrange: String,
    pub group_gap: i64,
}

/// Lay out a symbol from a JSON request and return the JSON response (never panics on input).
pub fn layout_box_json(request: &str) -> String {
    let response = match parse_request(request) {
        Ok(request) => match layout_box(request) {
            Ok(layout) => layout,
            Err(error) => error_value(error.kind(), error.message()),
        },
        Err(message) => error_value("RequestError", &message),
    };

    response.to_string()
}

/// Same as [`layout_box_json`] for raw bytes, which need not be UTF-8 (the WebAssembly entry).
pub fn layout_box_json_bytes(request: &[u8]) -> String {
    match std::str::from_utf8(request) {
        Ok(text) => layout_box_json(text),
        Err(_) => error_value("RequestError", "request is not valid JSON").to_string(),
    }
}

fn error_value(kind: &str, message: &str) -> Value {
    let mut error = Map::new();
    error.insert("type".into(), Value::String(kind.into()));
    error.insert("message".into(), Value::String(message.into()));
    let mut response = Map::new();
    response.insert("error".into(), Value::Object(error));
    Value::Object(response)
}

/// Check the request shape.  The Python reference adapter in `rust/parity/parity.py` applies the
/// same checks, in the same order and with the same messages.
pub fn parse_request(text: &str) -> Result<LayoutRequest, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| "request is not valid JSON".to_string())?;
    let Value::Object(mut fields) = value else {
        return Err("request must be a JSON object".into());
    };

    if let Some(unknown) = fields
        .keys()
        .find(|key| !matches!(key.as_str(), "pins" | "arrange" | "group_gap"))
    {
        return Err(format!("unknown request field: {unknown}"));
    }

    // Taken by value (not cloned) once every check has passed.
    let Some(Value::Array(pin_values)) = fields.remove("pins").filter(|pins| match pins {
        Value::Array(items) => items.iter().all(Value::is_object),
        _ => false,
    }) else {
        return Err("pins must be an array of objects".into());
    };

    let arrange = match fields.get("arrange") {
        None => "auto".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(_) => return Err("arrange must be a string".into()),
    };

    let group_gap = match fields.get("group_gap") {
        None => 2,
        Some(Value::Number(number)) if !number.to_string().contains(['.', 'e', 'E']) =>
        // An integer too large for i64 is still an integer; it is out of range either way.
        {
            number.as_i64().unwrap_or(i64::MAX)
        }
        Some(_) => return Err("group_gap must be an integer".into()),
    };

    let pins = pin_values
        .into_iter()
        .filter_map(|item| match item {
            Value::Object(pin) => Some(pin),
            _ => None,
        })
        .collect();

    Ok(LayoutRequest {
        pins,
        arrange,
        group_gap,
    })
}

/// Millimetres as a JSON number, written exactly from integer nanometres ("-7.62", "0.0").
///
/// The decimal has at most six fraction digits, so it parses back to the same nanometre count;
/// it always has a fraction so Python decodes it as a `float`, as the Python layout returns.
pub fn mm_number(nm: i64) -> Value {
    let sign = if nm < 0 { "-" } else { "" };
    let magnitude = nm.unsigned_abs();
    let fraction = format!("{:06}", magnitude % 1_000_000);
    let fraction = fraction.trim_end_matches('0');
    let fraction = if fraction.is_empty() { "0" } else { fraction };
    let text = format!("{sign}{}.{fraction}", magnitude / 1_000_000);

    Value::Number(
        text.parse()
            .unwrap_or_else(|_| unreachable!("a decimal literal is a JSON number")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimetres_are_exact_decimals() {
        assert_eq!(mm_number(-7_620_000).to_string(), "-7.62");
        assert_eq!(mm_number(0).to_string(), "0.0");
        assert_eq!(mm_number(1).to_string(), "0.000001");
        assert_eq!(mm_number(15_240_000).to_string(), "15.24");
    }

    #[test]
    fn request_shape_is_checked_before_layout() {
        let error = |text: &str| parse_request(text).unwrap_err();
        assert_eq!(error("[]"), "request must be a JSON object");
        assert_eq!(error("{\"pins\": [1]}"), "pins must be an array of objects");
        assert_eq!(
            error("{\"pins\": [], \"group_gap\": 2.0}"),
            "group_gap must be an integer"
        );
        assert_eq!(
            error("{\"pins\": [], \"pin_length\": 1}"),
            "unknown request field: pin_length"
        );
        assert_eq!(error("{\"pins\": "), "request is not valid JSON");
        assert_eq!(parse_request("{\"pins\": []}").unwrap().group_gap, 2);
    }
}
