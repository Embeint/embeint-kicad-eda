// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! JSONL adapter for unit `libs.core.include.core.application.thin.replacement.p001`.
//! The operation schema is in README.md and mirrors
//! `port/oracle/shims/core_map_helpers.cpp` field for field, including validation
//! order and refusal messages. Only validation, conversion and calls into
//! `kicad-map-helpers` live here.

use std::collections::BTreeMap;
use std::ffi::CStr;

use kicad_map_helpers::{self as mh, WxError, WxString};
use serde_json::{json, Map, Value};

pub use kicad_map_helpers::UNIT;

/// Largest number of map entries in one request.
pub const MAX_ENTRIES: usize = 256;
/// Largest wide string, in units.
pub const MAX_WIDE: usize = 4096;
/// Largest byte string (before its implicit terminator).
pub const MAX_BYTES: usize = 4096;

/// A refusal of the request, or an allocation the port refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    Alloc,
}

impl From<WxError> for Error {
    fn from(_: WxError) -> Self {
        Self::Alloc
    }
}

type Result<T> = core::result::Result<T, Error>;

/// Require exactly the required keys plus any subset of the optional ones.
fn fields<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a Map<String, Value>> {
    let object = value
        .as_object()
        .ok_or(Error::Invalid("unexpected or missing field"))?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err(Error::Invalid("unexpected or missing field"));
    }
    Ok(object)
}

/// A JSON integer token within `[min, max]` (the oracle's `Integer` contract).
fn integer(value: &Value, min: i64, max: i64) -> Option<i64> {
    let number = value.as_number()?;
    let parsed = number
        .as_i64()
        .or_else(|| number.as_u64().and_then(|u| i64::try_from(u).ok()));
    parsed.filter(|n| (min..=max).contains(n))
}

/// A map key or value type of the adapter: `int` or `wxString`.
trait Item: Sized + Ord + mh::TryClone {
    fn parse(value: &Value) -> Result<Self>;
    fn emit(&self) -> Value;
}

impl Item for i32 {
    fn parse(value: &Value) -> Result<Self> {
        integer(value, i64::from(i32::MIN), i64::from(i32::MAX))
            .and_then(|n| i32::try_from(n).ok())
            .ok_or(Error::Invalid("expected int"))
    }

    fn emit(&self) -> Value {
        json!(self)
    }
}

impl Item for WxString {
    fn parse(value: &Value) -> Result<Self> {
        let array = value
            .as_array()
            .filter(|a| a.len() <= MAX_WIDE)
            .ok_or(Error::Invalid("expected wide string"))?;
        let mut units = Vec::new();
        units
            .try_reserve_exact(array.len())
            .map_err(|_| Error::Alloc)?;
        for unit in array {
            let unit = integer(unit, i64::from(i32::MIN), i64::from(i32::MAX))
                .and_then(|n| i32::try_from(n).ok())
                .ok_or(Error::Invalid("expected wide string"))?;
            units.push(unit);
        }
        Ok(WxString::from_units(&units)?)
    }

    fn emit(&self) -> Value {
        Value::Array(self.units().iter().map(|u| json!(u)).collect())
    }
}

/// A `const char*` argument: `None` for a null pointer, else the bytes up to the
/// first NUL of the given bytes followed by an implicit terminator.
fn bytes(value: &Value) -> Result<Option<Vec<u8>>> {
    if value.is_null() {
        return Ok(None);
    }
    let array = value
        .as_array()
        .filter(|a| a.len() <= MAX_BYTES)
        .ok_or(Error::Invalid("expected bytes"))?;
    let mut data = Vec::new();
    data.try_reserve_exact(array.len() + 1)
        .map_err(|_| Error::Alloc)?;
    for byte in array {
        let byte = integer(byte, 0, 255)
            .and_then(|n| u8::try_from(n).ok())
            .ok_or(Error::Invalid("expected bytes"))?;
        data.push(byte);
    }
    data.push(0);
    Ok(Some(data))
}

/// View adapter bytes (always NUL-terminated) as the C string a `const char*` reads.
fn c_str(data: &Option<Vec<u8>>) -> Result<Option<&CStr>> {
    match data {
        None => Ok(None),
        Some(data) => CStr::from_bytes_until_nul(data)
            .map(Some)
            .map_err(|_| Error::Invalid("expected bytes")),
    }
}

/// Insert in request order with `std::map::emplace` semantics: the first entry for a
/// key is kept.
fn parse_map<K: Item, V: Item>(value: &Value) -> Result<BTreeMap<K, V>> {
    let entries = value
        .as_array()
        .filter(|a| a.len() <= MAX_ENTRIES)
        .ok_or(Error::Invalid("expected map"))?;
    let mut map = BTreeMap::new();
    for entry in entries {
        let pair = entry
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or(Error::Invalid("expected map entry"))?;
        let (Some(key), Some(item)) = (pair.first(), pair.get(1)) else {
            return Err(Error::Invalid("expected map entry"));
        };
        let key = K::parse(key)?;
        let item = V::parse(item)?;
        map.entry(key).or_insert(item);
    }
    Ok(map)
}

fn optional<V: Item>(found: Option<V>) -> Value {
    match found {
        None => json!({"found": false, "value": null}),
        Some(value) => json!({"found": true, "value": value.emit()}),
    }
}

#[derive(Clone, Copy)]
enum Type {
    Int,
    Wx,
}

fn item_type(input: &Value, name: &'static str, message: &'static str) -> Result<Type> {
    match input.get(name).and_then(Value::as_str) {
        Some("int") => Ok(Type::Int),
        Some("wx") => Ok(Type::Wx),
        _ => Err(Error::Invalid(message)),
    }
}

fn key_type(input: &Value) -> Result<Type> {
    item_type(input, "key_type", "expected type: key_type")
}

fn value_type(input: &Value) -> Result<Type> {
    item_type(input, "value_type", "expected type: value_type")
}

fn map_order<K: Item, V: Item>(input: &Value) -> Result<Value> {
    let map = parse_map::<K, V>(&input["map"])?;
    Ok(json!({
        "keys": map.keys().map(Item::emit).collect::<Vec<_>>(),
        "values": map.values().map(Item::emit).collect::<Vec<_>>(),
    }))
}

fn get_opt_wx<V: Item>(input: &Value) -> Result<Value> {
    let map = parse_map::<WxString, V>(&input["map"])?;
    let key = WxString::parse(&input["key"])?;
    Ok(optional(mh::get_opt_wx(&map, &key)?))
}

fn get_opt_cstr<V: Item>(input: &Value) -> Result<Value> {
    let map = parse_map::<WxString, V>(&input["map"])?;
    let key = bytes(&input["key"])?;
    Ok(optional(mh::get_opt_cstr(&map, c_str(&key)?)?))
}

fn get_opt<K: Item, V: Item>(input: &Value) -> Result<Value> {
    let map = parse_map::<K, V>(&input["map"])?;
    let key = K::parse(&input["key"])?;
    Ok(optional(mh::get_opt(&map, &key)?))
}

fn get_def<K: Item, V: Item + Default>(input: &Value) -> Result<Value> {
    let map = parse_map::<K, V>(&input["map"])?;
    let key = K::parse(&input["key"])?;
    let value = match input.get("defval") {
        None => mh::get_def_or_value_init(&map, &key)?,
        Some(defval) => mh::get_def(&map, &key, &V::parse(defval)?)?,
    };
    Ok(json!({"value": value.emit()}))
}

/// Run one operation.
pub fn run(input: &Value) -> Result<Value> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or(Error::Invalid("expected string: op"))?;
    match op {
        "from_utf8" => {
            fields(input, &["op", "text"], &[])?;
            let text = bytes(&input["text"])?;
            Ok(json!({"value": WxString::from_utf8(c_str(&text)?)?.emit()}))
        }
        "map_order" => {
            fields(input, &["op", "key_type", "value_type", "map"], &[])?;
            match (key_type(input)?, value_type(input)?) {
                (Type::Int, Type::Int) => map_order::<i32, i32>(input),
                (Type::Int, Type::Wx) => map_order::<i32, WxString>(input),
                (Type::Wx, Type::Int) => map_order::<WxString, i32>(input),
                (Type::Wx, Type::Wx) => map_order::<WxString, WxString>(input),
            }
        }
        "get_opt_wx" => {
            fields(input, &["op", "value_type", "map", "key"], &[])?;
            match value_type(input)? {
                Type::Int => get_opt_wx::<i32>(input),
                Type::Wx => get_opt_wx::<WxString>(input),
            }
        }
        "get_opt_cstr" => {
            fields(input, &["op", "value_type", "map", "key"], &[])?;
            match value_type(input)? {
                Type::Int => get_opt_cstr::<i32>(input),
                Type::Wx => get_opt_cstr::<WxString>(input),
            }
        }
        "get_opt" => {
            fields(input, &["op", "key_type", "value_type", "map", "key"], &[])?;
            match (key_type(input)?, value_type(input)?) {
                (Type::Int, Type::Int) => get_opt::<i32, i32>(input),
                (Type::Int, Type::Wx) => get_opt::<i32, WxString>(input),
                (Type::Wx, Type::Int) => get_opt::<WxString, i32>(input),
                (Type::Wx, Type::Wx) => get_opt::<WxString, WxString>(input),
            }
        }
        "get_def_cstr" => {
            fields(input, &["op", "map", "key", "defval"], &[])?;
            let map = parse_map::<WxString, WxString>(&input["map"])?;
            let key = bytes(&input["key"])?;
            let defval = bytes(&input["defval"])?;
            let value = mh::get_def_cstr(&map, c_str(&key)?, c_str(&defval)?)?;
            Ok(json!({"value": value.emit()}))
        }
        "get_def_cstr_wx" => {
            fields(input, &["op", "map", "key", "defval"], &[])?;
            let map = parse_map::<WxString, WxString>(&input["map"])?;
            let key = bytes(&input["key"])?;
            let defval = WxString::parse(&input["defval"])?;
            let value = mh::get_def_cstr_wx(&map, c_str(&key)?, &defval)?;
            Ok(json!({"value": value.emit()}))
        }
        "get_def_wx" => {
            fields(input, &["op", "map", "key"], &["defval"])?;
            let map = parse_map::<WxString, WxString>(&input["map"])?;
            let key = WxString::parse(&input["key"])?;
            let value = match input.get("defval") {
                None => mh::get_def_wx_or_empty(&map, &key)?,
                Some(defval) => mh::get_def_wx(&map, &key, &WxString::parse(defval)?)?,
            };
            Ok(json!({"value": value.emit()}))
        }
        "get_def" => {
            fields(
                input,
                &["op", "key_type", "value_type", "map", "key"],
                &["defval"],
            )?;
            match (key_type(input)?, value_type(input)?) {
                (Type::Int, Type::Int) => get_def::<i32, i32>(input),
                (Type::Int, Type::Wx) => get_def::<i32, WxString>(input),
                (Type::Wx, Type::Int) => get_def::<WxString, i32>(input),
                (Type::Wx, Type::Wx) => get_def::<WxString, WxString>(input),
            }
        }
        _ => Err(Error::Invalid("unknown op")),
    }
}

fn keys(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    })
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
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", message),
        Err(Error::Alloc) => failure(unit, case, "execution_error", "allocation refused"),
    }
}
