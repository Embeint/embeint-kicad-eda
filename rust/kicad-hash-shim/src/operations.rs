// SPDX-License-Identifier: GPL-3.0-or-later
//! The pure request operations of the hash/transform/Bezier adapter: validation, calls into
//! `kicad_hash` and JSON results, without I/O. `main.rs` adds the JSONL transport; the portable
//! `wasm_parity` example of `kicad-hash` reuses this file to run the corpus in a wasm runtime.

use kicad_hash::{
    fmix64, hash_combine, hash_val, mmh3_rotl64, Bezier, BezierCoordinate, BezierPoly,
    CxxNumericGrouping, Hash128, Md5Hash, Mmh3Hash, StdHash, Transform,
};
use kicad_math::{Vector2, Vector2D};
use serde_json::{json, Map, Value};

pub const UNIT: &str = "libs.kimath.include.application.pure.p001";
const MAX_STEPS: usize = 4096;
const MAX_MMH3_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MD5_BYTES: u64 = 1024 * 1024;
const MAX_GROUPING_BYTES: usize = 4096;
const MAX_POLY_POINTS: usize = 4096;

pub type Result<T> = std::result::Result<T, String>;

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

pub fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
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

/// The optional caller locale: `thousands_sep` (a `char` as 0..255) and the
/// `grouping` string bytes of the global locale's `numpunct<char>`. The 4096-byte
/// limit is the transport bound shared with every other hex field, not a property
/// of the reference.
fn grouping(input: &Value) -> Result<Option<CxxNumericGrouping>> {
    let Some(locale) = input.get("locale") else {
        return Ok(None);
    };
    keys(locale, &["thousands_sep", "grouping"], &[])?;
    let sep =
        u8::try_from(unsigned(locale, "thousands_sep", 255)?).map_err(|error| error.to_string())?;
    let bytes = hex(locale, "grouping", MAX_GROUPING_BYTES, false)?;
    Ok(Some(CxxNumericGrouping::new(sep, &bytes)))
}

fn hash128_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "a", "b"], &["locale"])?;
    let fresh = Hash128::new();
    let mut a = Hash128::from_value8(array16(&hex(input, "a", 16, true)?)?);
    let b = Hash128::from_value8(array16(&hex(input, "b", 16, true)?)?);
    let locale = grouping(input)?;
    let mut grouped = locale.as_ref().map(|locale| {
        json!({
            "new": to_hex(&fresh.to_string_in(locale)),
            "a": to_hex(&a.to_string_in(locale)),
            "b": to_hex(&b.to_string_in(locale)),
        })
    });
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
    if let (Some(grouped), Some(locale)) = (grouped.as_mut(), locale.as_ref()) {
        grouped["cleared"] = json!(to_hex(&a.to_string_in(locale)));
    }
    if let Some(grouped) = grouped {
        result["grouped"] = grouped;
    }
    Ok(result)
}

fn snapshot(hash: &Mmh3Hash, op: &str) -> Map<String, Value> {
    let mut event = Map::new();
    event.insert("op".into(), json!(op));
    event.insert("h1".into(), json!(hash.h1()));
    event.insert("h2".into(), json!(hash.h2()));
    event.insert("len".into(), json!(hash.len()));
    // A byte the reference never wrote has no value; it is shown as "??".
    let blocks: String = hash
        .blocks()
        .iter()
        .map(|byte| byte.map_or_else(|| "??".to_owned(), |byte| format!("{byte:02x}")))
        .collect();
    event.insert("blocks".into(), json!(blocks));
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
            // A new hash has no written block bytes, as the reference constructors leave
            // `blocks` uninitialized; neither program gives those bytes a value.
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
                            "data_v1" => {
                                state.add_data_v1(&data);
                                Ok(())
                            }
                            "string" => state.add_string(&data),
                            _ => state.add_chars(&data),
                        }
                        .map_err(|error| error.to_string())?;
                    }
                }
                "int" => {
                    keys(step, &["op", "value"], &[])?;
                    total += 4;
                    if total > MAX_MMH3_BYTES {
                        return Err("byte budget exceeded".into());
                    }
                    let value = signed(step, "value", i32::MIN.into(), i32::MAX.into())?;
                    state
                        .add_i32(i32::try_from(value).map_err(|error| error.to_string())?)
                        .map_err(|error| error.to_string())?;
                }
                "digest" => {
                    keys(step, &["op"], &[])?;
                    let digest = state.digest().map_err(|error| error.to_string())?;
                    extra.insert("digest".into(), view(&digest));
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
                        state.hash_block()
                    } else {
                        state.hash_tail()
                    }
                    .map_err(|error| error.to_string())?;
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

/// A `double` as 16 lower-case hex digits of its bits, most significant first.
fn bits(value: &Value, name: &str) -> Result<f64> {
    let invalid = || format!("invalid bits: {name}");
    let text = value.as_str().ok_or_else(invalid)?;
    if text.len() != 16 || !text.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(invalid());
    }
    u64::from_str_radix(text, 16)
        .map(f64::from_bits)
        .map_err(|_| invalid())
}

fn bits_json(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

/// A coordinate of one of the two instantiated component types.
trait Coordinate: BezierCoordinate + Default {
    fn parse(value: &Value, name: &str) -> Result<Self>;
    fn to_json(self) -> Value;
}

impl Coordinate for i32 {
    fn parse(value: &Value, name: &str) -> Result<i32> {
        let wrapper = json!({ name: value });
        let number = signed(&wrapper, name, i32::MIN.into(), i32::MAX.into())?;
        i32::try_from(number).map_err(|error| error.to_string())
    }

    fn to_json(self) -> Value {
        json!(self)
    }
}

impl Coordinate for f64 {
    fn parse(value: &Value, name: &str) -> Result<f64> {
        bits(value, name)
    }

    fn to_json(self) -> Value {
        bits_json(self)
    }
}

/// `[x, y]`.
fn point<T: Coordinate>(value: &Value, name: &str) -> Result<Vector2<T>> {
    let pair = value
        .as_array()
        .filter(|pair| pair.len() == 2)
        .ok_or_else(|| format!("expected point: {name}"))?;
    Ok(Vector2::new(
        T::parse(&pair[0], name)?,
        T::parse(&pair[1], name)?,
    ))
}

fn point_json<T: Coordinate>(point: Vector2<T>) -> Value {
    json!([point.x.to_json(), point.y.to_json()])
}

fn curve_json<T: Coordinate>(curve: &Bezier<T>) -> Value {
    json!([
        point_json(curve.start),
        point_json(curve.c1),
        point_json(curve.c2),
        point_json(curve.end)
    ])
}

/// One `BEZIER<T>` constructed from `points` and one owned member call on it.
fn bezier_call<T: Coordinate>(input: &Value, call: &str) -> Result<Value> {
    let points = field(input, "points")?
        .as_array()
        .filter(|points| points.len() == 4)
        .ok_or("expected four points: points")?;
    let mut parsed = [Vector2::<T>::default(); 4];
    for (slot, value) in parsed.iter_mut().zip(points) {
        *slot = point::<T>(value, "points")?;
    }
    match call {
        "new" => keys(input, &["op", "type", "points", "call"], &[])?,
        "point_at" | "split" => keys(input, &["op", "type", "points", "call", "t"], &[])?,
        "sub_curve" => keys(input, &["op", "type", "points", "call", "t0", "t1"], &[])?,
        _ => return Err("unknown bezier call".into()),
    }
    let [start, c1, c2, end] = parsed;
    let curve = Bezier::new(start, c1, c2, end);
    let mut result = json!({ "curve": curve_json(&curve) });
    let error = |error: kicad_math::VectorDomainError| error.to_string();
    match call {
        "point_at" => {
            let t = bits(field(input, "t")?, "t")?;
            result["point"] = point_json(curve.point_at(t).map_err(error)?);
        }
        "split" => {
            let t = bits(field(input, "t")?, "t")?;
            let (left, right) = curve.split(t).map_err(error)?;
            result["left"] = curve_json(&left);
            result["right"] = curve_json(&right);
        }
        "sub_curve" => {
            let t0 = bits(field(input, "t0")?, "t0")?;
            let t1 = bits(field(input, "t1")?, "t1")?;
            result["sub_curve"] = curve_json(&curve.sub_curve(t0, t1).map_err(error)?);
        }
        _ => {}
    }
    Ok(result)
}

fn bezier_op(input: &Value) -> Result<Value> {
    // Validation order (shared with the oracle): keys, type, points, call, the call's
    // exact keys, then its parameters.
    keys(input, &["op", "type", "points", "call"], &["t", "t0", "t1"])?;
    let kind = field(input, "type")?.as_str().unwrap_or_default();
    let call = field(input, "call")?.as_str().unwrap_or_default();
    match kind {
        "i32" => bezier_call::<i32>(input, call),
        "f64" => bezier_call::<f64>(input, call),
        _ => Err("invalid bezier type".into()),
    }
}

fn bezier_poly_op(input: &Value) -> Result<Value> {
    keys(input, &["op", "points"], &[])?;
    let points = field(input, "points")?
        .as_array()
        .ok_or("expected points array")?;
    if points.len() > MAX_POLY_POINTS {
        return Err("too many points".into());
    }
    let control: Vec<Vector2D> = points
        .iter()
        .map(|value| point::<f64>(value, "points"))
        .collect::<Result<_>>()?;
    let poly = BezierPoly::from_vector2d(&control);
    Ok(json!({
        "min_seg_len": bits_json(poly.min_seg_len()),
        "ctrl_pts": poly.ctrl_pts().iter().map(|p| point_json(*p)).collect::<Vec<_>>(),
    }))
}

pub fn run(input: &Value) -> Result<Value> {
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
        "bezier" => bezier_op(input),
        "bezier_poly" => bezier_poly_op(input),
        _ => Err("unknown operation".into()),
    }
}
