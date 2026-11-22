// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Native JSONL adapter for unit `libs.kimath.include.application.pure.p001`.
//! It mirrors `port/oracle/shims/kimath_header_hash.cpp` operation for operation,
//! including validation order and messages. The schema is in README.md.

use kicad_hash::{
    fmix64, hash_combine, hash_val, mmh3_rotl64, Hash128, Md5Hash, Mmh3Hash, StdHash, Transform,
};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

const UNIT: &str = "libs.kimath.include.application.pure.p001";
const MAX_STEPS: usize = 4096;
const MAX_MMH3_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MD5_BYTES: u64 = 1024 * 1024;

type Result<T> = std::result::Result<T, String>;

/// The bytes of a `std::string` argument, which need not be UTF-8.
#[derive(Clone, Debug)]
struct StdString(Vec<u8>);

impl StdHash for StdString {
    fn std_hash(&self) -> u64 {
        self.0.as_slice().std_hash()
    }
}

/// One typed `std::hash` argument, as the oracle's `VALUE` variant.
#[derive(Clone, Debug)]
enum Typed {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Bytes(StdString),
}

impl Typed {
    fn as_hash(&self) -> &dyn StdHash {
        match self {
            Self::Bool(v) => v,
            Self::I8(v) => v,
            Self::U8(v) => v,
            Self::I16(v) => v,
            Self::U16(v) => v,
            Self::I32(v) => v,
            Self::U32(v) => v,
            Self::I64(v) => v,
            Self::U64(v) => v,
            Self::F32(v) => v,
            Self::F64(v) => v,
            Self::Bytes(v) => v,
        }
    }

    /// The oracle instantiates three-argument calls for bool, int32_t, uint64_t,
    /// double and std::string only.
    fn check_three(&self) -> Result<()> {
        match self {
            Self::Bool(_) | Self::I32(_) | Self::U64(_) | Self::F64(_) | Self::Bytes(_) => Ok(()),
            _ => Err("unsupported value type for three values".into()),
        }
    }
}

fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
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

fn field<'a>(object: &'a Value, name: &str) -> Result<&'a Value> {
    object
        .get(name)
        .ok_or_else(|| "unexpected or missing field".into())
}

/// A JSON integer in `[min, max]`; `-0` is the integer zero, as in nlohmann.
fn signed(object: &Value, name: &str, min: i64, max: i64) -> Result<i64> {
    let value = field(object, name)?;
    let number = if let Some(number) = value.as_i64() {
        Some(number)
    } else {
        value.as_u64().and_then(|number| i64::try_from(number).ok())
    };
    number
        .filter(|number| (min..=max).contains(number))
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn unsigned(object: &Value, name: &str, max: u64) -> Result<u64> {
    let value = field(object, name)?;
    let number = if let Some(number) = value.as_u64() {
        Some(number)
    } else {
        value.as_i64().and_then(|number| u64::try_from(number).ok())
    };
    number
        .filter(|number| *number <= max)
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn boolean(object: &Value, name: &str) -> Result<bool> {
    field(object, name)?
        .as_bool()
        .ok_or_else(|| format!("expected boolean: {name}"))
}

/// Lower-case hexadecimal of even length, at most `max` bytes (or exactly when `exact`).
fn hex(object: &Value, name: &str, max: usize, exact: bool) -> Result<Vec<u8>> {
    let invalid = || format!("invalid hex: {name}");
    let text = field(object, name)?.as_str().ok_or_else(invalid)?;
    if text.len() % 2 != 0 || text.len() / 2 > max || (exact && text.len() / 2 != max) {
        return Err(invalid());
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(invalid()),
    };
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn typed(value: &Value) -> Result<Typed> {
    let kind = value
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .ok_or("invalid value type")?;
    let integer = |min: i64, max: i64| -> Result<i64> {
        keys(value, &["type", "value"], &[])?;
        signed(value, "value", min, max)
    };
    let narrow = |error: std::num::TryFromIntError| error.to_string();
    Ok(match kind {
        "bool" => {
            keys(value, &["type", "value"], &[])?;
            Typed::Bool(boolean(value, "value")?)
        }
        "f32" | "f64" => {
            keys(value, &["type", "bits"], &[])?;
            let bytes = hex(value, "bits", if kind == "f32" { 4 } else { 8 }, true)?;
            // Most significant nibble first, as the oracle reads it.
            let bits = bytes
                .iter()
                .fold(0_u64, |bits, byte| (bits << 8) | u64::from(*byte));
            if kind == "f32" {
                Typed::F32(f32::from_bits(u32::try_from(bits).map_err(narrow)?))
            } else {
                Typed::F64(f64::from_bits(bits))
            }
        }
        "string" => {
            keys(value, &["type", "hex"], &[])?;
            Typed::Bytes(StdString(hex(value, "hex", 4096, false)?))
        }
        "i8" => Typed::I8(i8::try_from(integer(i8::MIN.into(), i8::MAX.into())?).map_err(narrow)?),
        "u8" => Typed::U8(u8::try_from(integer(0, u8::MAX.into())?).map_err(narrow)?),
        "i16" => {
            Typed::I16(i16::try_from(integer(i16::MIN.into(), i16::MAX.into())?).map_err(narrow)?)
        }
        "u16" => Typed::U16(u16::try_from(integer(0, u16::MAX.into())?).map_err(narrow)?),
        "i32" => {
            Typed::I32(i32::try_from(integer(i32::MIN.into(), i32::MAX.into())?).map_err(narrow)?)
        }
        "u32" => Typed::U32(u32::try_from(integer(0, u32::MAX.into())?).map_err(narrow)?),
        "i64" => Typed::I64(integer(i64::MIN, i64::MAX)?),
        "u64" => {
            keys(value, &["type", "value"], &[])?;
            Typed::U64(unsigned(value, "value", u64::MAX)?)
        }
        _ => return Err("invalid value type".into()),
    })
}

fn values(input: &Value, max: usize) -> Result<Vec<Typed>> {
    let array = field(input, "values")?
        .as_array()
        .ok_or("expected values array")?;
    if array.len() > max {
        return Err("too many values".into());
    }
    array.iter().map(typed).collect()
}

fn three_checked(values: &[Typed]) -> Result<()> {
    if values.len() == 3 {
        for value in values {
            value.check_three()?;
        }
    }
    Ok(())
}

fn refs(values: &[Typed]) -> Vec<&dyn StdHash> {
    values.iter().map(Typed::as_hash).collect()
}

fn std_hash_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "values"], &[])?;
    let hashes: Vec<u64> = values(input, 64)?
        .iter()
        .map(|value| value.as_hash().std_hash())
        .collect();
    Ok(json!({ "hashes": hashes }))
}

fn hash_combine_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "seed", "values"], &[])?;
    let mut seed = unsigned(input, "seed", u64::MAX)?;
    let values = values(input, 3)?;
    three_checked(&values)?;
    hash_combine(&mut seed, &refs(&values));
    Ok(json!({ "seed": seed }))
}

fn hash_val_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "values"], &[])?;
    let values = values(input, 3)?;
    three_checked(&values)?;
    Ok(json!({ "hash": hash_val(&refs(&values)) }))
}

fn hash_combine_each_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "seed", "values"], &[])?;
    let mut seed = unsigned(input, "seed", u64::MAX)?;
    let mut seeds = Vec::new();
    for value in values(input, 256)? {
        hash_combine(&mut seed, &[value.as_hash()]);
        seeds.push(seed);
    }
    Ok(json!({ "seeds": seeds }))
}

fn view(hash: &Hash128) -> Value {
    json!({
        "string": hash.to_hex_string(),
        "value8": to_hex(&hash.value8()),
        "value32": hash.value32(),
        "value64": hash.value64(),
    })
}

fn array16(bytes: &[u8]) -> Result<[u8; 16]> {
    <[u8; 16]>::try_from(bytes).map_err(|error| error.to_string())
}

fn hash128_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "a", "b"], &[])?;
    let fresh = Hash128::new();
    let mut a = Hash128::from_value8(array16(&hex(input, "a", 16, true)?)?);
    let b = Hash128::from_value8(array16(&hex(input, "b", 16, true)?)?);
    let mut result = json!({
        "new": view(&fresh),
        "a": view(&a),
        "b": view(&b),
        "equal": a.equals(&b),
        "reverse_equal": b.equals(&a),
        "self_equal": a.equals(&a),
    });
    a.clear();
    result["cleared"] = json!({
        "view": view(&a),
        "equals_new": a.equals(&fresh),
        "equals_b": a.equals(&b),
    });
    Ok(result)
}

fn snapshot(hash: &Mmh3Hash, op: &str) -> Map<String, Value> {
    let mut event = Map::new();
    event.insert("op".into(), json!(op));
    event.insert("h1".into(), json!(hash.h1()));
    event.insert("h2".into(), json!(hash.h2()));
    event.insert("len".into(), json!(hash.len()));
    event.insert("blocks".into(), json!(to_hex(&hash.blocks())));
    event
}

fn steps(input: &Value) -> Result<&Vec<Value>> {
    keys(input, &["op", "steps"], &[])?;
    let steps = field(input, "steps")?
        .as_array()
        .ok_or("expected steps array")?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err("steps outside 1..4096".into());
    }
    Ok(steps)
}

fn step_op(step: &Value) -> Result<&str> {
    step.as_object()
        .and_then(|object| object.get("op"))
        .and_then(Value::as_str)
        .ok_or_else(|| "expected op string".into())
}

fn seed(step: &Value) -> Result<u32> {
    u32::try_from(unsigned(step, "seed", u32::MAX.into())?).map_err(|error| error.to_string())
}

fn mmh3_op(input: &Value) -> Result<Value> {
    let steps = steps(input)?;
    let mut hash: Option<Mmh3Hash> = None;
    let mut events = Vec::new();
    let mut total = 0_u64;

    for step in steps {
        let op = step_op(step)?;
        if op != "new" && hash.is_none() {
            return Err("first step must be new".into());
        }
        let mut extra = Map::new();

        if op == "new" {
            keys(step, &["op"], &["seed"])?;
            // Mmh3Hash starts with zero block bytes; the oracle defines the
            // reference's indeterminate bytes the same way.
            hash = Some(if step.get("seed").is_some() {
                Mmh3Hash::with_seed(seed(step)?)
            } else {
                Mmh3Hash::new()
            });
        } else {
            let state = hash.as_mut().ok_or("first step must be new")?;
            match op {
                "reset" => {
                    keys(step, &["op"], &["seed"])?;
                    if step.get("seed").is_some() {
                        state.reset(seed(step)?);
                    } else {
                        state.reset_default();
                    }
                }
                "data" | "data_v1" | "string" | "chars" => {
                    keys(step, &["op", "hex"], &["repeat"])?;
                    let data = hex(step, "hex", 65536, false)?;
                    let repeat = if step.get("repeat").is_some() {
                        unsigned(step, "repeat", 4096)?
                    } else {
                        1
                    };
                    total += data.len() as u64 * repeat;
                    if total > MAX_MMH3_BYTES {
                        return Err("byte budget exceeded".into());
                    }
                    for _ in 0..repeat {
                        match op {
                            "data" => state.add_data(&data),
                            "data_v1" => state.add_data_v1(&data),
                            "string" => state.add_string(&data),
                            _ => state.add_chars(&data),
                        }
                    }
                }
                "int" => {
                    keys(step, &["op", "value"], &[])?;
                    total += 4;
                    if total > MAX_MMH3_BYTES {
                        return Err("byte budget exceeded".into());
                    }
                    let value = signed(step, "value", i32::MIN.into(), i32::MAX.into())?;
                    state.add_i32(i32::try_from(value).map_err(|error| error.to_string())?);
                }
                "digest" => {
                    keys(step, &["op"], &[])?;
                    extra.insert("digest".into(), view(&state.digest()));
                }
                "state" => {
                    keys(step, &["op", "h1", "h2", "blocks", "len"], &[])?;
                    let h1 = unsigned(step, "h1", u64::MAX)?;
                    let h2 = unsigned(step, "h2", u64::MAX)?;
                    let blocks = array16(&hex(step, "blocks", 16, true)?)?;
                    let len = u32::try_from(unsigned(step, "len", u32::MAX.into())?)
                        .map_err(|error| error.to_string())?;
                    *state = Mmh3Hash::from_state(h1, h2, blocks, len);
                }
                "hash_block" | "hash_tail" => {
                    keys(step, &["op"], &[])?;
                    if op == "hash_block" {
                        state.hash_block();
                    } else {
                        state.hash_tail();
                    }
                }
                "hash_final" => {
                    keys(step, &["op"], &[])?;
                    let mut out = Hash128::default();
                    state.hash_final(&mut out);
                    extra.insert("out".into(), view(&out));
                }
                "getblock64" => {
                    keys(step, &["op", "index"], &[])?;
                    let index = i32::try_from(signed(step, "index", 0, 1)?)
                        .map_err(|error| error.to_string())?;
                    let block = state
                        .get_block64(index)
                        .map_err(|error| error.to_string())?;
                    extra.insert("block".into(), json!(block));
                }
                _ => return Err("unknown operation".into()),
            }
        }

        let state = hash.as_ref().ok_or("first step must be new")?;
        let mut event = snapshot(state, op);
        event.extend(extra);
        events.push(Value::Object(event));
    }

    Ok(json!({ "events": events }))
}

fn rotl_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "x", "r"], &[])?;
    let x = unsigned(input, "x", u64::MAX)?;
    let r = i8::try_from(signed(input, "r", 1, 63)?).map_err(|error| error.to_string())?;
    let value = mmh3_rotl64(x, r).map_err(|error| error.to_string())?;
    Ok(json!({ "value": value }))
}

fn fmix64_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "k"], &[])?;
    Ok(json!({ "value": fmix64(unsigned(input, "k", u64::MAX)?) }))
}

fn md5_snapshot(hash: &Md5Hash, op: &str) -> Value {
    json!({ "op": op, "valid": hash.is_valid(), "compact": hash.format(true) })
}

fn md5_valid_op(input: &Value) -> Result<Value> {
    let steps = steps(input)?;
    let mut hash: Option<Md5Hash> = None;
    let mut events = Vec::new();
    let mut total = 0_u64;

    for step in steps {
        let op = step_op(step)?;
        if op != "new" && hash.is_none() {
            return Err("first step must be new".into());
        }
        if op == "new" {
            keys(step, &["op"], &[])?;
            hash = Some(Md5Hash::new());
        } else {
            let state = hash.as_mut().ok_or("first step must be new")?;
            match op {
                "set_valid" => {
                    keys(step, &["op", "value"], &[])?;
                    state.set_valid(boolean(step, "value")?);
                }
                "init" | "finalize" | "copy" | "assign" => {
                    keys(step, &["op"], &[])?;
                    match op {
                        "init" => state.init(),
                        "finalize" => state.finalize(),
                        "copy" => *state = state.clone(),
                        _ => {
                            let mut fresh = Md5Hash::new();
                            fresh.clone_from(state);
                            *state = fresh;
                        }
                    }
                }
                "bytes" => {
                    keys(step, &["op", "hex"], &[])?;
                    let data = hex(step, "hex", 4096, false)?;
                    total += data.len() as u64;
                    if total > MAX_MD5_BYTES {
                        return Err("byte budget exceeded".into());
                    }
                    state.hash(&data).map_err(|_| "byte count exceeds uint32")?;
                }
                "int" => {
                    keys(step, &["op", "value"], &[])?;
                    total += 4;
                    if total > MAX_MD5_BYTES {
                        return Err("byte budget exceeded".into());
                    }
                    let value = signed(step, "value", i32::MIN.into(), i32::MAX.into())?;
                    state.hash_int(i32::try_from(value).map_err(|error| error.to_string())?);
                }
                "inspect" => keys(step, &["op"], &[])?,
                _ => return Err("unknown operation".into()),
            }
        }
        let state = hash.as_ref().ok_or("first step must be new")?;
        events.push(md5_snapshot(state, op));
    }

    Ok(json!({ "events": events }))
}

fn transform(input: &Value, name: &str) -> Result<Transform> {
    let value = field(input, name)?;
    if value.is_null() {
        return Ok(Transform::default());
    }
    let members = value
        .as_array()
        .filter(|array| array.len() == 4)
        .ok_or_else(|| format!("expected transform: {name}"))?;
    let mut parsed = [0_i32; 4];
    for (slot, member) in parsed.iter_mut().zip(members) {
        let wrapper = json!({ name: member });
        let number = signed(&wrapper, name, i32::MIN.into(), i32::MAX.into())?;
        *slot = i32::try_from(number).map_err(|error| error.to_string())?;
    }
    let [x1, y1, x2, y2] = parsed;
    Ok(Transform::new(x1, y1, x2, y2))
}

fn transform_json(transform: &Transform) -> Value {
    json!([transform.x1, transform.y1, transform.x2, transform.y2])
}

#[allow(clippy::eq_op)] // a != a deliberately exercises the owned operator on one object.
fn transform_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "a", "b"], &[])?;
    let a = transform(input, "a")?;
    let b = transform(input, "b")?;
    Ok(json!({
        "a": transform_json(&a),
        "b": transform_json(&b),
        "equal": a == b,
        "not_equal": a != b,
        "reverse_not_equal": b != a,
        "self_not_equal": a != a,
    }))
}

fn run(input: &Value) -> Result<Value> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or("expected op string")?;
    match op {
        "std_hash" => std_hash_op(input),
        "hash_combine" => hash_combine_op(input),
        "hash_val" => hash_val_op(input),
        "hash_combine_each" => hash_combine_each_op(input),
        "hash128" => hash128_op(input),
        "mmh3" => mmh3_op(input),
        "rotl" => rotl_op(input),
        "fmix64" => fmix64_op(input),
        "md5_valid" => md5_valid_op(input),
        "transform" => transform_op(input),
        _ => Err("unknown operation".into()),
    }
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
        Ok(result) => json!({"unit":unit,"case":case,"result":result}),
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch(&line?))?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{dispatch, UNIT};
    use serde_json::{json, Value};

    fn reply(input: Value) -> Value {
        dispatch(&json!({"unit": UNIT, "case": "t", "input": input}).to_string())
    }

    fn message(input: Value) -> Option<String> {
        reply(input)["error"]["message"].as_str().map(str::to_owned)
    }

    #[test]
    fn mmh3_digest_of_foo() {
        let value = reply(json!({"op":"mmh3","steps":[
            {"op":"new"},{"op":"string","hex":"666f6f"},{"op":"digest"}]}));
        assert_eq!(
            value["result"]["events"][2]["digest"]["string"],
            "E271865701F545617EAF87E42BBA7D87"
        );
    }

    #[test]
    fn malformed_inputs_are_refused_with_the_oracle_messages() {
        assert_eq!(
            message(json!({"op":"rotl","x":1,"r":0})).as_deref(),
            Some("integer out of range: r")
        );
        assert_eq!(
            message(json!({"op":"rotl","x":-1,"r":1})).as_deref(),
            Some("integer out of range: x")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"digest"}]})).as_deref(),
            Some("first step must be new")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[]})).as_deref(),
            Some("steps outside 1..4096")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"new"},{"op":"data","hex":"ABCD"}]}))
                .as_deref(),
            Some("invalid hex: hex")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"new","seed":4294967296_u64}]})).as_deref(),
            Some("integer out of range: seed")
        );
        assert_eq!(
            message(json!({"op":"hash_val","values":[
                {"type":"i8","value":1},{"type":"i32","value":1},{"type":"i32","value":1}]}))
            .as_deref(),
            Some("unsupported value type for three values")
        );
        assert_eq!(
            message(json!({"op":"hash_val","values":[{"type":"i8","value":128}]})).as_deref(),
            Some("integer out of range: value")
        );
        assert_eq!(
            message(json!({"op":"hash_val","values":[{"type":"f64","bits":"00"}]})).as_deref(),
            Some("invalid hex: bits")
        );
        assert_eq!(
            message(json!({"op":"transform","a":[1,2,3],"b":null})).as_deref(),
            Some("expected transform: a")
        );
        assert_eq!(
            message(json!({"op":"transform","a":[1,2,3,1.0],"b":null})).as_deref(),
            Some("integer out of range: a")
        );
        assert_eq!(
            message(json!({"op":"md5_valid","steps":[{"op":"new","x":1}]})).as_deref(),
            Some("unexpected or missing field")
        );
        assert_eq!(
            message(json!({"op":"nope"})).as_deref(),
            Some("unknown operation")
        );
        assert_eq!(
            message(json!({"op":"mmh3","steps":[{"op":"new"},
                {"op":"data","hex":"00","repeat":4097}]}))
            .as_deref(),
            Some("integer out of range: repeat")
        );
    }

    #[test]
    fn transform_inequality_uses_co_ported_equality() {
        let value = reply(json!({"op":"transform","a":null,"b":[1,0,0,1]}));
        assert_eq!(value["result"]["not_equal"], false);
        assert_eq!(value["result"]["a"], json!([1, 0, 0, 1]));
    }
}
