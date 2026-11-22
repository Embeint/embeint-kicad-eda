// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Native JSONL adapter for unit `libs.kimath.include.math.application.pure.p001`.
//! The operation schema is in this crate's README; `port/oracle/shims/kimath_math.cpp` is the
//! C++ counterpart and validates every request identically.

use kicad_math::wide_int::{self, KiInt128};
use kicad_math::{Matrix3x3D, Vector2, Vector2D, Vector2L, Vector3, Vector3D};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

const UNIT: &str = "libs.kimath.include.math.application.pure.p001";

/// Each operation and its exact argument fields, in validation order.
const OPERATIONS: &[(&str, &[&str])] = &[
    ("matrix_default", &[]),
    ("matrix_from_vectors", &["a1", "a2", "a3"]),
    ("matrix_from_elements", &["elements"]),
    ("set_identity", &["matrix"]),
    ("set_translation", &["matrix", "translation"]),
    ("get_translation", &["matrix"]),
    ("set_rotation", &["matrix", "angle"]),
    ("set_scale", &["matrix", "scale"]),
    ("get_scale", &["matrix"]),
    ("determinant", &["matrix"]),
    ("inverse", &["matrix"]),
    ("transpose", &["matrix"]),
    ("compare", &["matrix", "other"]),
    ("mul_matrix", &["a", "b"]),
    ("mul_vector2", &["matrix", "vector"]),
    ("mul_vector3", &["matrix", "vector"]),
    ("mul_scalar", &["matrix", "scalar"]),
    ("scalar_mul", &["scalar", "matrix"]),
    ("stream", &["matrix"]),
    ("words_to_double", &["hi", "lo"]),
    ("to_double", &["value"]),
    ("cross_wide", &["a", "b"]),
    ("dot_wide", &["a", "b"]),
];

fn keys(value: &Value, required: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected object")?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object.keys().any(|key| !required.contains(&key.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

/// A binary64 value as exactly 16 lowercase hexadecimal digits of its IEEE-754 bits.
fn double(value: &Value, name: &str) -> Result<f64, String> {
    let error = || format!("expected double bit string: {name}");
    let text = value.as_str().ok_or_else(error)?;
    if text.len() != 16 || !text.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(error());
    }
    u64::from_str_radix(text, 16)
        .map(f64::from_bits)
        .map_err(|_| error())
}

fn bits(value: f64) -> Value {
    Value::String(format!("{:016x}", value.to_bits()))
}

fn doubles<const N: usize>(value: &Value, name: &str, kind: &str) -> Result<[f64; N], String> {
    let array = value
        .as_array()
        .filter(|array| array.len() == N)
        .ok_or_else(|| format!("expected {kind}: {name}"))?;
    let mut result = [0.0; N];
    for (slot, item) in result.iter_mut().zip(array) {
        *slot = double(item, name)?;
    }
    Ok(result)
}

fn vector2(value: &Value, name: &str) -> Result<Vector2D, String> {
    let [x, y] = doubles::<2>(value, name, "vector")?;
    Ok(Vector2::new(x, y))
}

fn vector3(value: &Value, name: &str) -> Result<Vector3D, String> {
    let [x, y, z] = doubles::<3>(value, name, "vector")?;
    Ok(Vector3::new(x, y, z))
}

fn matrix(value: &Value, name: &str) -> Result<Matrix3x3D, String> {
    let error = || format!("expected matrix: {name}");
    let rows = value
        .as_array()
        .filter(|rows| rows.len() == 3)
        .ok_or_else(error)?;
    let mut elements = [[0.0; 3]; 3];
    for (slot, row) in elements.iter_mut().zip(rows) {
        if !row.as_array().is_some_and(|row| row.len() == 3) {
            return Err(error());
        }
        *slot = doubles::<3>(row, name, "matrix")?;
    }
    let [[a00, a01, a02], [a10, a11, a12], [a20, a21, a22]] = elements;
    // MATRIX3x3( T a00, ..., T a22 ), as the oracle constructs it.
    Ok(Matrix3x3D::from_elements(
        a00, a01, a02, a10, a11, a12, a20, a21, a22,
    ))
}

fn matrix_json(matrix: &Matrix3x3D) -> Value {
    Value::Array(
        matrix
            .data
            .iter()
            .map(|row| Value::Array(row.iter().map(|value| bits(*value)).collect()))
            .collect(),
    )
}

fn vector2_json(vector: &Vector2D) -> Value {
    json!([bits(vector.x), bits(vector.y)])
}

fn vector3_json(vector: &Vector3D) -> Value {
    json!([bits(vector.x), bits(vector.y), bits(vector.z)])
}

fn int64(value: &Value, name: &str) -> Result<i64, String> {
    value
        .as_i64()
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn uint64(value: &Value, name: &str) -> Result<u64, String> {
    // nlohmann stores a nonnegative integer token as unsigned, and integer -0 as signed zero.
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn vector2l(value: &Value, name: &str) -> Result<Vector2L, String> {
    let array = value
        .as_array()
        .filter(|array| array.len() == 2)
        .ok_or_else(|| format!("expected vector: {name}"))?;
    match array.as_slice() {
        [x, y] => Ok(Vector2::new(int64(x, name)?, int64(y, name)?)),
        _ => Err(format!("expected vector: {name}")),
    }
}

/// A `KI_INT128` as canonical decimal text: optional `-`, no leading zeros, no `-0`.
fn int128(value: &Value, name: &str) -> Result<KiInt128, String> {
    let error = || format!("expected int128 string: {name}");
    let text = value.as_str().ok_or_else(error)?;
    let digits = text.strip_prefix('-').unwrap_or(text);
    let canonical = !digits.is_empty()
        && digits.bytes().all(|c| c.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && text != "-0";
    if !canonical {
        return Err(error());
    }
    text.parse::<KiInt128>().map_err(|_| error())
}

fn wide_result(value: Result<KiInt128, wide_int::WideIntError>) -> Result<Value, String> {
    let value = value.map_err(|error| error.to_string())?;
    Ok(json!({"value": value.to_string(), "double": bits(wide_int::to_double(value))}))
}

fn run(input: &Value) -> Result<Value, String> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or("expected op string")?;
    let (_, fields) = OPERATIONS
        .iter()
        .find(|(name, _)| *name == op)
        .ok_or("unknown operation")?;
    let mut required = vec!["op"];
    required.extend_from_slice(fields);
    keys(input, &required)?;
    let arg = |name: &str| input.get(name).unwrap_or(&Value::Null);

    let matrix_reply = |matrix: Matrix3x3D| Ok(json!({"matrix": matrix_json(&matrix)}));

    match op {
        "matrix_default" => matrix_reply(Matrix3x3D::new()),
        "matrix_from_vectors" => {
            let a1 = vector3(arg("a1"), "a1")?;
            let a2 = vector3(arg("a2"), "a2")?;
            let a3 = vector3(arg("a3"), "a3")?;
            matrix_reply(Matrix3x3D::from_vectors(a1, a2, a3))
        }
        "matrix_from_elements" => matrix_reply(matrix(arg("elements"), "elements")?),
        "set_identity" => {
            let mut m = matrix(arg("matrix"), "matrix")?;
            m.set_identity();
            matrix_reply(m)
        }
        "set_translation" => {
            let mut m = matrix(arg("matrix"), "matrix")?;
            let translation = vector2(arg("translation"), "translation")?;
            m.set_translation(translation);
            matrix_reply(m)
        }
        "get_translation" => {
            let m = matrix(arg("matrix"), "matrix")?;
            Ok(json!({"vector": vector2_json(&m.get_translation())}))
        }
        "set_rotation" => {
            let mut m = matrix(arg("matrix"), "matrix")?;
            let angle = double(arg("angle"), "angle")?;
            m.set_rotation(angle);
            matrix_reply(m)
        }
        "set_scale" => {
            let mut m = matrix(arg("matrix"), "matrix")?;
            let scale = vector2(arg("scale"), "scale")?;
            m.set_scale(scale);
            matrix_reply(m)
        }
        "get_scale" => {
            let m = matrix(arg("matrix"), "matrix")?;
            Ok(json!({"vector": vector2_json(&m.get_scale())}))
        }
        "determinant" => {
            let m = matrix(arg("matrix"), "matrix")?;
            Ok(json!({"value": bits(m.determinant())}))
        }
        "inverse" => matrix_reply(matrix(arg("matrix"), "matrix")?.inverse()),
        "transpose" => matrix_reply(matrix(arg("matrix"), "matrix")?.transpose()),
        "compare" => {
            let m = matrix(arg("matrix"), "matrix")?;
            let other = matrix(arg("other"), "other")?;
            Ok(json!({"equal": m == other, "not_equal": m != other}))
        }
        "mul_matrix" => {
            let a = matrix(arg("a"), "a")?;
            let b = matrix(arg("b"), "b")?;
            matrix_reply(a * b)
        }
        "mul_vector2" => {
            let m = matrix(arg("matrix"), "matrix")?;
            let v = vector2(arg("vector"), "vector")?;
            Ok(json!({"vector": vector2_json(&(m * v))}))
        }
        "mul_vector3" => {
            let m = matrix(arg("matrix"), "matrix")?;
            let v = vector3(arg("vector"), "vector")?;
            Ok(json!({"vector": vector3_json(&(m * v))}))
        }
        "mul_scalar" => {
            let m = matrix(arg("matrix"), "matrix")?;
            let scalar = double(arg("scalar"), "scalar")?;
            matrix_reply(m * scalar)
        }
        "scalar_mul" => {
            let scalar = double(arg("scalar"), "scalar")?;
            let m = matrix(arg("matrix"), "matrix")?;
            matrix_reply(scalar * m)
        }
        "stream" => {
            let m = matrix(arg("matrix"), "matrix")?;
            Ok(json!({"text": m.stream_text()}))
        }
        "words_to_double" => {
            let hi = int64(arg("hi"), "hi")?;
            let lo = uint64(arg("lo"), "lo")?;
            Ok(json!({"value": bits(wide_int::words_to_double(hi, lo))}))
        }
        "to_double" => {
            let value = int128(arg("value"), "value")?;
            Ok(json!({"value": bits(wide_int::to_double(value)),
                "via_words": bits(wide_int::to_double_via_words(value))}))
        }
        "cross_wide" => {
            let a = vector2l(arg("a"), "a")?;
            let b = vector2l(arg("b"), "b")?;
            wide_result(wide_int::cross_wide(&a, &b))
        }
        "dot_wide" => {
            let a = vector2l(arg("a"), "a")?;
            let b = vector2l(arg("b"), "b")?;
            wide_result(wide_int::dot_wide(&a, &b))
        }
        _ => Err("unknown operation".into()),
    }
}

fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"unit":unit,"case":case,"error":{"kind":kind,"message":message}});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(message) => return failure(unit, case, "invalid_request", &message),
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
        Ok(result) => {
            let mut reply = Map::new();
            reply.insert("unit".into(), unit);
            reply.insert("case".into(), case);
            reply.insert("result".into(), result);
            Value::Object(reply)
        }
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

#[cfg(test)]
mod tests;
