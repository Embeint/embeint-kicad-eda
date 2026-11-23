// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Native JSONL adapter for unit `libs.kimath.include.math.application.pure.p001`.
//! The operation schema is in this crate's README; `port/oracle/shims/kimath_math.cpp` is the
//! C++ counterpart and validates every request identically.

use kicad_math::stream::{FmtFlags, StreamState};
use kicad_math::wide_int::{self, KiInt128};
use kicad_math::{Matrix3x3, MatrixError, Vector2, Vector2L, Vector3};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

const UNIT: &str = "libs.kimath.include.math.application.pure.p001";

/// Operation, required fields in validation order, and optional modifiers.
type Operation = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

const MATRIX: &[&str] = &["type"];
const SCALAR: &[&str] = &["type", "scalar_type"];

/// Every operation and its exact fields.
const OPERATIONS: &[Operation] = &[
    ("matrix_default", &[], MATRIX),
    ("matrix_from_vectors", &["a1", "a2", "a3"], MATRIX),
    ("matrix_from_elements", &["elements"], MATRIX),
    ("set_identity", &["matrix"], MATRIX),
    ("set_translation", &["matrix", "translation"], MATRIX),
    ("get_translation", &["matrix"], MATRIX),
    ("set_rotation", &["matrix", "angle"], MATRIX),
    ("set_scale", &["matrix", "scale"], MATRIX),
    ("get_scale", &["matrix"], MATRIX),
    ("determinant", &["matrix"], MATRIX),
    ("inverse", &["matrix"], MATRIX),
    ("transpose", &["matrix"], MATRIX),
    ("compare", &["matrix", "other"], MATRIX),
    ("mul_matrix", &["a", "b"], MATRIX),
    ("mul_vector2", &["matrix", "vector"], MATRIX),
    ("mul_vector3", &["matrix", "vector"], MATRIX),
    ("mul_scalar", &["matrix", "scalar"], SCALAR),
    ("scalar_mul", &["scalar", "matrix"], SCALAR),
    ("stream", &["matrix"], &["type", "state"]),
    ("words_to_double", &["hi", "lo"], &[]),
    ("to_double", &["value"], &[]),
    ("cross_wide", &["a", "b"], &[]),
    ("dot_wide", &["a", "b"], &[]),
];

/// Resource bound on stream precision and width, so a request cannot ask for a
/// multi-gigabyte expansion. It is a protocol limit, not a claim about the C++ domain.
const STREAM_BOUND: (i64, i64) = (-64, 512);

const FLAG_NAMES: [&str; 15] = [
    "boolalpha",
    "dec",
    "fixed",
    "hex",
    "internal",
    "left",
    "oct",
    "right",
    "scientific",
    "showbase",
    "showpoint",
    "showpos",
    "skipws",
    "unitbuf",
    "uppercase",
];

fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected object")?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

/// The element (or scalar) type selected by an optional modifier; absent means `double`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Double,
    Int,
}

fn kind(input: &Value, name: &str) -> Result<Kind, String> {
    match input.get(name) {
        None => Ok(Kind::Double),
        Some(value) => match value.as_str() {
            Some("double") => Ok(Kind::Double),
            Some("int") => Ok(Kind::Int),
            _ => Err(format!("expected {name}: double or int")),
        },
    }
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

fn int32(value: &Value, name: &str) -> Result<i32, String> {
    value
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .ok_or_else(|| format!("integer out of range: {name}"))
}

/// An element of the selected kind.
trait Element: kicad_math::MatrixElement {
    fn parse(value: &Value, name: &str) -> Result<Self, String>;
    fn json(self) -> Value;
}

impl Element for f64 {
    fn parse(value: &Value, name: &str) -> Result<Self, String> {
        double(value, name)
    }

    fn json(self) -> Value {
        bits(self)
    }
}

impl Element for i32 {
    fn parse(value: &Value, name: &str) -> Result<Self, String> {
        int32(value, name)
    }

    fn json(self) -> Value {
        Value::from(self)
    }
}

fn elements<T: Element, const N: usize>(
    value: &Value,
    name: &str,
    shape: &str,
) -> Result<[T; N], String> {
    let array = value
        .as_array()
        .filter(|array| array.len() == N)
        .ok_or_else(|| format!("expected {shape}: {name}"))?;
    let mut result = [T::ZERO; N];
    for (slot, item) in result.iter_mut().zip(array) {
        *slot = T::parse(item, name)?;
    }
    Ok(result)
}

fn vector2<T: Element>(value: &Value, name: &str) -> Result<Vector2<T>, String> {
    let [x, y] = elements::<T, 2>(value, name, "vector")?;
    Ok(Vector2::new(x, y))
}

fn vector3<T: Element>(value: &Value, name: &str) -> Result<Vector3<T>, String> {
    let [x, y, z] = elements::<T, 3>(value, name, "vector")?;
    Ok(Vector3::new(x, y, z))
}

fn matrix<T: Element>(value: &Value, name: &str) -> Result<Matrix3x3<T>, String> {
    let error = || format!("expected matrix: {name}");
    let rows = value
        .as_array()
        .filter(|rows| rows.len() == 3)
        .ok_or_else(error)?;
    let mut data = [[T::ZERO; 3]; 3];
    for (slot, row) in data.iter_mut().zip(rows) {
        if !row.as_array().is_some_and(|row| row.len() == 3) {
            return Err(error());
        }
        *slot = elements::<T, 3>(row, name, "matrix")?;
    }
    let [[a00, a01, a02], [a10, a11, a12], [a20, a21, a22]] = data;
    // MATRIX3x3( T a00, ..., T a22 ), as the oracle constructs it.
    Ok(Matrix3x3::from_elements(
        a00, a01, a02, a10, a11, a12, a20, a21, a22,
    ))
}

fn matrix_json<T: Element>(matrix: &Matrix3x3<T>) -> Value {
    Value::Array(
        matrix
            .data
            .iter()
            .map(|row| Value::Array(row.iter().map(|value| value.json()).collect()))
            .collect(),
    )
}

fn vector2_json<T: Element>(vector: &Vector2<T>) -> Value {
    json!([vector.x.json(), vector.y.json()])
}

fn vector3_json<T: Element>(vector: &Vector3<T>) -> Value {
    json!([vector.x.json(), vector.y.json(), vector.z.json()])
}

fn matrix_reply<T: Element>(matrix: &Matrix3x3<T>) -> Value {
    json!({"matrix": matrix_json(matrix)})
}

fn int_error(error: MatrixError) -> String {
    error.to_string()
}

/// `{"flags": [names], "precision": n, "width": n, "fill": "c"}`.
fn stream_state(value: &Value) -> Result<StreamState, String> {
    let error = || String::from("expected stream state");
    keys(value, &["flags", "precision", "width", "fill"], &[]).map_err(|_| error())?;
    let names = value["flags"].as_array().ok_or_else(error)?;
    let mut flags = FmtFlags::default();
    let mut seen = Vec::new();
    for name in names {
        let name = name.as_str().ok_or_else(error)?;
        if seen.contains(&name) || !FLAG_NAMES.contains(&name) {
            return Err(error());
        }
        seen.push(name);
        let flag = match name {
            "boolalpha" => &mut flags.boolalpha,
            "dec" => &mut flags.dec,
            "fixed" => &mut flags.fixed,
            "hex" => &mut flags.hex,
            "internal" => &mut flags.internal,
            "left" => &mut flags.left,
            "oct" => &mut flags.oct,
            "right" => &mut flags.right,
            "scientific" => &mut flags.scientific,
            "showbase" => &mut flags.showbase,
            "showpoint" => &mut flags.showpoint,
            "showpos" => &mut flags.showpos,
            "skipws" => &mut flags.skipws,
            "unitbuf" => &mut flags.unitbuf,
            _ => &mut flags.uppercase,
        };
        *flag = true;
    }
    let bounded = |name: &str| {
        value[name]
            .as_i64()
            .filter(|number| (STREAM_BOUND.0..=STREAM_BOUND.1).contains(number))
            .ok_or_else(|| format!("integer out of range: {name}"))
    };
    let precision = bounded("precision")?;
    let width = bounded("width")?;
    let fill = value["fill"]
        .as_str()
        .filter(|text| text.len() == 1 && text.bytes().all(|c| (0x20..0x7f).contains(&c)))
        .and_then(|text| text.chars().next())
        .ok_or_else(error)?;
    Ok(StreamState {
        flags,
        precision,
        width,
        fill,
    })
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

fn wide_result(value: Result<KiInt128, wide_int::WideIntError>) -> Result<Value, String> {
    let value = value.map_err(|error| error.to_string())?;
    Ok(json!({"value": value.to_string(), "double": bits(wide_int::to_double(value))}))
}

/// Operations whose body is the same for both element types.
fn shared<T: Element>(op: &str, input: &Value) -> Result<Option<Value>, String> {
    let arg = |name: &str| input.get(name).unwrap_or(&Value::Null);
    Ok(Some(match op {
        "matrix_default" => matrix_reply(&Matrix3x3::<T>::new()),
        "matrix_from_vectors" => {
            let a1 = vector3::<T>(arg("a1"), "a1")?;
            let a2 = vector3::<T>(arg("a2"), "a2")?;
            let a3 = vector3::<T>(arg("a3"), "a3")?;
            matrix_reply(&Matrix3x3::from_vectors(a1, a2, a3))
        }
        "matrix_from_elements" => matrix_reply(&matrix::<T>(arg("elements"), "elements")?),
        "set_identity" => {
            let mut m = matrix::<T>(arg("matrix"), "matrix")?;
            m.set_identity();
            matrix_reply(&m)
        }
        "set_translation" => {
            let mut m = matrix::<T>(arg("matrix"), "matrix")?;
            m.set_translation(vector2::<T>(arg("translation"), "translation")?);
            matrix_reply(&m)
        }
        "get_translation" => {
            let m = matrix::<T>(arg("matrix"), "matrix")?;
            json!({"vector": vector2_json(&m.get_translation())})
        }
        "set_scale" => {
            let mut m = matrix::<T>(arg("matrix"), "matrix")?;
            m.set_scale(vector2::<T>(arg("scale"), "scale")?);
            matrix_reply(&m)
        }
        "get_scale" => {
            let m = matrix::<T>(arg("matrix"), "matrix")?;
            json!({"vector": vector2_json(&m.get_scale())})
        }
        "transpose" => matrix_reply(&matrix::<T>(arg("matrix"), "matrix")?.transpose()),
        "compare" => {
            let m = matrix::<T>(arg("matrix"), "matrix")?;
            let other = matrix::<T>(arg("other"), "other")?;
            json!({"equal": m == other, "not_equal": m != other})
        }
        "stream" => {
            let m = matrix::<T>(arg("matrix"), "matrix")?;
            match input.get("state") {
                None => json!({"text": m.stream_text(&mut StreamState::default())
                    .map_err(|error| error.to_string())?}),
                Some(state) => {
                    let mut state = stream_state(state)?;
                    let text = m
                        .stream_text(&mut state)
                        .map_err(|error| error.to_string())?;
                    json!({"text": text, "width_after": state.width})
                }
            }
        }
        _ => return Ok(None),
    }))
}

fn double_ops(op: &str, input: &Value, scalar: Kind) -> Result<Value, String> {
    if let Some(reply) = shared::<f64>(op, input)? {
        return Ok(reply);
    }
    let arg = |name: &str| input.get(name).unwrap_or(&Value::Null);
    match op {
        "set_rotation" => {
            let mut m = matrix::<f64>(arg("matrix"), "matrix")?;
            m.set_rotation(double(arg("angle"), "angle")?);
            Ok(matrix_reply(&m))
        }
        "determinant" => {
            let m = matrix::<f64>(arg("matrix"), "matrix")?;
            Ok(json!({"value": bits(m.determinant())}))
        }
        "inverse" => Ok(matrix_reply(
            &matrix::<f64>(arg("matrix"), "matrix")?.inverse(),
        )),
        "mul_matrix" => {
            let a = matrix::<f64>(arg("a"), "a")?;
            let b = matrix::<f64>(arg("b"), "b")?;
            Ok(matrix_reply(&(a * b)))
        }
        "mul_vector2" => {
            let m = matrix::<f64>(arg("matrix"), "matrix")?;
            let v = vector2::<f64>(arg("vector"), "vector")?;
            Ok(json!({"vector": vector2_json(&(m * v))}))
        }
        "mul_vector3" => {
            let m = matrix::<f64>(arg("matrix"), "matrix")?;
            let v = vector3::<f64>(arg("vector"), "vector")?;
            Ok(json!({"vector": vector3_json(&(m * v))}))
        }
        "mul_scalar" => {
            let m = matrix::<f64>(arg("matrix"), "matrix")?;
            Ok(matrix_reply(&match scalar {
                Kind::Double => m * double(arg("scalar"), "scalar")?,
                Kind::Int => m.mul_scalar_int(int32(arg("scalar"), "scalar")?),
            }))
        }
        "scalar_mul" => {
            let product = match scalar {
                Kind::Double => {
                    let s = double(arg("scalar"), "scalar")?;
                    s * matrix::<f64>(arg("matrix"), "matrix")?
                }
                Kind::Int => {
                    let s = int32(arg("scalar"), "scalar")?;
                    Matrix3x3::<f64>::scalar_mul_int(s, &matrix::<f64>(arg("matrix"), "matrix")?)
                }
            };
            Ok(matrix_reply(&product))
        }
        _ => Err("unknown operation".into()),
    }
}

fn int_ops(op: &str, input: &Value, scalar: Kind) -> Result<Value, String> {
    if let Some(reply) = shared::<i32>(op, input)? {
        return Ok(reply);
    }
    let arg = |name: &str| input.get(name).unwrap_or(&Value::Null);
    match op {
        "set_rotation" => {
            let mut m = matrix::<i32>(arg("matrix"), "matrix")?;
            m.set_rotation(int32(arg("angle"), "angle")?);
            Ok(matrix_reply(&m))
        }
        "determinant" => {
            let m = matrix::<i32>(arg("matrix"), "matrix")?;
            Ok(json!({"value": m.determinant().map_err(int_error)?}))
        }
        "inverse" => {
            let m = matrix::<i32>(arg("matrix"), "matrix")?;
            Ok(matrix_reply(&m.inverse().map_err(int_error)?))
        }
        "mul_matrix" => {
            let a = matrix::<i32>(arg("a"), "a")?;
            let b = matrix::<i32>(arg("b"), "b")?;
            Ok(matrix_reply(&a.mul_matrix(&b).map_err(int_error)?))
        }
        "mul_vector2" => {
            let m = matrix::<i32>(arg("matrix"), "matrix")?;
            let v = vector2::<i32>(arg("vector"), "vector")?;
            Ok(json!({"vector": vector2_json(&m.mul_vector2(&v).map_err(int_error)?)}))
        }
        "mul_vector3" => {
            let m = matrix::<i32>(arg("matrix"), "matrix")?;
            let v = vector3::<i32>(arg("vector"), "vector")?;
            Ok(json!({"vector": vector3_json(&m.mul_vector3(&v).map_err(int_error)?)}))
        }
        "mul_scalar" => {
            let m = matrix::<i32>(arg("matrix"), "matrix")?;
            let product = match scalar {
                Kind::Double => m.mul_scalar_double(double(arg("scalar"), "scalar")?),
                Kind::Int => m.mul_scalar(int32(arg("scalar"), "scalar")?),
            };
            Ok(matrix_reply(&product.map_err(int_error)?))
        }
        "scalar_mul" => {
            let product = match scalar {
                Kind::Double => {
                    let s = double(arg("scalar"), "scalar")?;
                    Matrix3x3::<i32>::scalar_mul_double(s, &matrix::<i32>(arg("matrix"), "matrix")?)
                }
                Kind::Int => {
                    let s = int32(arg("scalar"), "scalar")?;
                    Matrix3x3::<i32>::scalar_mul(s, &matrix::<i32>(arg("matrix"), "matrix")?)
                }
            };
            Ok(matrix_reply(&product.map_err(int_error)?))
        }
        _ => Err("unknown operation".into()),
    }
}

fn wide_ops(op: &str, input: &Value) -> Result<Value, String> {
    let arg = |name: &str| input.get(name).unwrap_or(&Value::Null);
    match op {
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

fn run(input: &Value) -> Result<Value, String> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or("expected op string")?;
    let (_, fields, optional) = OPERATIONS
        .iter()
        .find(|(name, _, _)| *name == op)
        .ok_or("unknown operation")?;
    let mut required = vec!["op"];
    required.extend_from_slice(fields);
    keys(input, &required, optional)?;
    if optional.is_empty() {
        return wide_ops(op, input);
    }
    let element = kind(input, "type")?;
    let scalar = kind(input, "scalar_type")?;
    match element {
        Kind::Double => double_ops(op, input, scalar),
        Kind::Int => int_ops(op, input, scalar),
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
    if keys(&request, &["unit", "case", "input"], &[]).is_err()
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
