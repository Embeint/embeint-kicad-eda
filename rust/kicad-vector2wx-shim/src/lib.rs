// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.math.application.thin.replacement.p001`. It
//! mirrors `port/oracle/shims/vector2wx.cpp`: one conversion of one 32-bit integer pair
//! (or of a default-constructed source) per request; integer results as JSON integers,
//! VECTOR2D components as exact IEEE bits. No other output is computed here.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_math::vector::{Vector2D, Vector2I};
use kicad_math::vector2wx::{
    to_vector2d_from_point, to_vector2d_from_size, to_vector2i_from_point, to_vector2i_from_size,
    to_wx_point, to_wx_size, WxPoint, WxSize,
};
use serde_json::{json, Value};

pub const UNIT: &str = "libs.kimath.include.math.application.thin.replacement.p001";

fn keys(v: &Value, fields: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

/// `value`: `null` selects the default-constructed source; otherwise exactly two JSON
/// integers in the 32-bit `int` range.
fn pair(v: &Value) -> Result<Option<(i32, i32)>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    let mut values = a.iter().map(|n| {
        n.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| "component out of range".to_string())
    });
    let x = values.next().ok_or("expected point")??;
    let y = values.next().ok_or("expected point")??;
    Ok(Some((x, y)))
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn integer(x: i32, y: i32) -> Value {
    json!({"x": x, "y": y})
}
fn double(v: Vector2D) -> Value {
    json!({"x": bits(v.x), "y": bits(v.y)})
}
fn vector2i(v: Vector2I) -> Value {
    integer(v.x, v.y)
}

fn run(v: &Value) -> Result<Value, String> {
    let op = v
        .get("op")
        .and_then(Value::as_str)
        .ok_or("expected string: op")?;
    const OPS: [&str; 6] = [
        "to_vector2i_size",
        "to_vector2i_point",
        "to_vector2d_point",
        "to_vector2d_size",
        "to_wx_point",
        "to_wx_size",
    ];
    if !OPS.contains(&op) {
        return Err("unknown operation".into());
    }
    keys(v, &["op", "value"])?;
    let source = pair(&v["value"])?;
    let point = || source.map_or_else(WxPoint::default, |(x, y)| WxPoint::new(x, y));
    let size = || source.map_or_else(WxSize::default, |(x, y)| WxSize::new(x, y));
    let vector = || source.map_or_else(Vector2I::default, |(x, y)| Vector2I::new(x, y));
    Ok(match op {
        "to_vector2i_size" => vector2i(to_vector2i_from_size(&size())),
        "to_vector2i_point" => vector2i(to_vector2i_from_point(&point())),
        "to_vector2d_point" => double(to_vector2d_from_point(&point())),
        "to_vector2d_size" => double(to_vector2d_from_size(&size())),
        "to_wx_point" => {
            let p = to_wx_point(&vector());
            integer(p.x, p.y)
        }
        _ => {
            let s = to_wx_size(&vector());
            integer(s.x, s.y)
        }
    })
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
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}
