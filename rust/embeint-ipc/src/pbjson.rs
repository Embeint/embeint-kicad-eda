// SPDX-License-Identifier: GPL-3.0-or-later
//! Protobuf JSON as the Python MCP writes and reads it.
//!
//! [`to_json`] matches `json_format.MessageToDict(message, preserving_proto_field_name=True)` and
//! [`from_json`] matches `json_format.ParseDict(value, message)` from the Python protobuf
//! package, so tool results and accepted arguments stay the same when a tool moves to Rust:
//!
//! - Field names are the proto names; set fields only (presence as protobuf defines it), in field
//!   number order.  64-bit integers are strings, enums are value names (an unknown value is its
//!   number), bytes are base64 and `float` fields are the shortest decimal that round-trips as a
//!   32-bit float (`type_checkers.ToShortestFloat`).
//! - `Any` is `{"@type": url, ...fields}`; `FieldMask` is a comma-separated camelCase string.
//! - Parsing accepts JSON and proto names, numbers or numeric strings for integers, enum names or
//!   numbers, and `null` to clear a field.  Error texts follow Python's where it is practical;
//!   they are not a compatibility contract.

use std::fmt::Write as _;

use base64::Engine as _;
use prost::Message as _;
use prost_reflect::{
    DynamicMessage, EnumDescriptor, FieldDescriptor, Kind, MapKey, MessageDescriptor,
    ReflectMessage, Value,
};
use serde_json::{Map, Number, Value as Json};

use crate::proto;

/// Python's `json_format.ParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

const ANY: &str = "google.protobuf.Any";
const FIELD_MASK: &str = "google.protobuf.FieldMask";
/// json_format's default `max_recursion_depth`.
const MAX_DEPTH: usize = 100;
const TYPE_KEY: &str = "@type";

// ---- Message to JSON ----------------------------------------------------------------------------

/// `MessageToDict(message, preserving_proto_field_name=True)`.  Fails like Python for an `Any`
/// whose type is not in the protocol or whose payload does not decode, and for messages nested
/// more than [`MAX_DEPTH`] deep (only possible through `Any`, which is decoded level by level).
pub fn to_json(message: &DynamicMessage) -> Result<Json, ParseError> {
    message_to_json(message, 0)
}

fn message_to_json(message: &DynamicMessage, depth: usize) -> Result<Json, ParseError> {
    if depth > MAX_DEPTH {
        return Err(ParseError(format!(
            "Message too deep to convert to JSON: more than {MAX_DEPTH} nested messages"
        )));
    }
    match message.descriptor().full_name() {
        ANY => any_to_json(message, depth),
        FIELD_MASK => Ok(Json::String(field_mask_to_json(message))),
        _ => regular_to_json(message, Map::new(), depth),
    }
}

fn sorted_fields(descriptor: &MessageDescriptor) -> Vec<FieldDescriptor> {
    let mut fields: Vec<_> = descriptor.fields().collect();
    fields.sort_by_key(FieldDescriptor::number);
    fields
}

fn regular_to_json(
    message: &DynamicMessage,
    mut js: Map<String, Json>,
    depth: usize,
) -> Result<Json, ParseError> {
    for field in sorted_fields(&message.descriptor()) {
        if !message.has_field(&field) && !negative_zero(&message.get_field(&field)) {
            continue;
        }
        let value = message.get_field(&field);
        let converted = if field.is_map() {
            let Kind::Message(entry) = field.kind() else {
                unreachable!("map fields are messages")
            };
            let value_kind = entry.map_entry_value_field().kind();
            let Value::Map(map) = value.as_ref() else {
                unreachable!("map field value")
            };
            let mut entries = Vec::with_capacity(map.len());
            for (key, item) in map {
                entries.push((map_key_text(key), value_to_json(&value_kind, item, depth)?));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Object(entries.into_iter().collect())
        } else if field.is_list() {
            let Value::List(items) = value.as_ref() else {
                unreachable!("list field value")
            };
            let kind = field.kind();
            let mut converted = Vec::with_capacity(items.len());
            for item in items {
                converted.push(value_to_json(&kind, item, depth)?);
            }
            Json::Array(converted)
        } else {
            value_to_json(&field.kind(), value.as_ref(), depth)?
        };
        js.insert(field.name().to_owned(), converted);
    }
    Ok(Json::Object(js))
}

/// Python's protobuf treats a float field holding -0.0 as set (it compares bits), so it prints it.
fn negative_zero(value: &Value) -> bool {
    match value {
        Value::F64(v) => *v == 0.0 && v.is_sign_negative(),
        Value::F32(v) => *v == 0.0 && v.is_sign_negative(),
        _ => false,
    }
}

fn map_key_text(key: &MapKey) -> String {
    match key {
        MapKey::Bool(true) => "true".into(),
        MapKey::Bool(false) => "false".into(),
        MapKey::I32(v) => v.to_string(),
        MapKey::I64(v) => v.to_string(),
        MapKey::U32(v) => v.to_string(),
        MapKey::U64(v) => v.to_string(),
        MapKey::String(v) => v.clone(),
    }
}

fn float_json(value: f64) -> Json {
    if value.is_nan() {
        Json::String("NaN".into())
    } else if value.is_infinite() {
        Json::String(if value < 0.0 { "-Infinity" } else { "Infinity" }.into())
    } else {
        Number::from_f64(value).map_or(Json::Null, Json::Number)
    }
}

fn value_to_json(kind: &Kind, value: &Value, depth: usize) -> Result<Json, ParseError> {
    Ok(match value {
        Value::Message(message) => message_to_json(message, depth + 1)?,
        Value::EnumNumber(number) => match kind {
            Kind::Enum(descriptor) => descriptor.get_value(*number).map_or_else(
                || Json::from(*number),
                |v| Json::String(v.name().to_owned()),
            ),
            _ => Json::from(*number),
        },
        Value::String(text) => Json::String(text.clone()),
        Value::Bytes(bytes) => {
            Json::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        Value::Bool(flag) => Json::Bool(*flag),
        Value::I32(v) => Json::from(*v),
        Value::U32(v) => Json::from(*v),
        Value::I64(v) => Json::String(v.to_string()),
        Value::U64(v) => Json::String(v.to_string()),
        Value::F64(v) => float_json(*v),
        Value::F32(v) => {
            if v.is_finite() {
                // Rust prints the shortest decimal that reads back as this f32, which is the
                // value Python's ToShortestFloat finds by widening the precision from 6 digits.
                float_json(format!("{v}").parse::<f64>().unwrap_or(f64::from(*v)))
            } else {
                float_json(f64::from(*v))
            }
        }
        Value::List(_) | Value::Map(_) => unreachable!("nested repeated values"),
    })
}

fn any_to_json(message: &DynamicMessage, depth: usize) -> Result<Json, ParseError> {
    let type_url = string_field(message, "type_url");
    let bytes = bytes_field(message, "value");
    if type_url.is_empty() && bytes.is_empty() {
        return Ok(Json::Object(Map::new()));
    }
    let mut js = Map::new();
    js.insert(TYPE_KEY.into(), Json::String(type_url.clone()));
    let name = type_url.rsplit('/').next().unwrap_or_default();
    // Python raises TypeError and DecodeError here, which the MCP reports as the tool's error
    let Some(descriptor) = proto::pool().get_message_by_name(name) else {
        return Err(ParseError(format!(
            "Can not find message descriptor by type_url: {type_url}"
        )));
    };
    let Ok(sub) = DynamicMessage::decode(descriptor, bytes.as_slice()) else {
        return Err(ParseError("Error parsing message".into()));
    };
    match name {
        ANY | FIELD_MASK => {
            js.insert("value".into(), message_to_json(&sub, depth + 1)?);
            Ok(Json::Object(js))
        }
        _ => regular_to_json(&sub, js, depth + 1),
    }
}

fn string_field(message: &DynamicMessage, name: &str) -> String {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn bytes_field(message: &DynamicMessage, name: &str) -> Vec<u8> {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::Bytes(bytes)) => bytes.to_vec(),
        _ => Vec::new(),
    }
}

fn field_mask_to_json(message: &DynamicMessage) -> String {
    let Some(paths) = message.get_field_by_name("paths") else {
        return String::new();
    };
    let Value::List(items) = paths.as_ref() else {
        return String::new();
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .map(snake_to_camel)
        .collect::<Vec<_>>()
        .join(",")
}

fn snake_to_camel(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut upper = false;
    for c in path.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn camel_to_snake(path: &str) -> Result<String, String> {
    if path.contains('_') {
        return Err(format!(
            "Fail to parse FieldMask: Path name {path} must not contain \"_\"."
        ));
    }
    let mut out = String::with_capacity(path.len() + 4);
    for c in path.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

// ---- JSON to message ----------------------------------------------------------------------------

/// `ParseDict(value, message)`: a new message of this type from protobuf JSON.
pub fn from_json(
    value: &Json,
    descriptor: &MessageDescriptor,
) -> Result<DynamicMessage, ParseError> {
    let mut message = DynamicMessage::new(descriptor.clone());
    let mut parser = Parser { depth: 0 };
    parser.convert_message(value, &mut message, descriptor.name())?;
    Ok(message)
}

/// Python's representation of a JSON value, including dictionaries in error messages.
pub fn py_repr(value: &Json) -> String {
    match value {
        Json::Null => "None".into(),
        Json::Bool(true) => "True".into(),
        Json::Bool(false) => "False".into(),
        Json::Number(n) => n.to_string(),
        Json::String(s) => {
            if s.contains('\'') && !s.contains('"') {
                format!("\"{s}\"")
            } else {
                format!("'{}'", s.replace('\'', "\\'"))
            }
        }
        Json::Array(items) => {
            format!(
                "[{}]",
                items.iter().map(py_repr).collect::<Vec<_>>().join(", ")
            )
        }
        Json::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}: {}", py_repr(&Json::String(k.clone())), py_repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Python's `str(value)`: a string as it is, anything else as its repr.
fn py_str(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        other => py_repr(other),
    }
}

fn py_type(value: &Json) -> &'static str {
    match value {
        Json::Null => "NoneType",
        Json::Bool(_) => "bool",
        Json::Number(n) if is_float_text(&n.to_string()) => "float",
        Json::Number(_) => "int",
        Json::String(_) => "str",
        Json::Array(_) => "list",
        Json::Object(_) => "dict",
    }
}

/// Python's json module reads a number with a fraction or exponent as a float, others as int.
fn is_float_text(text: &str) -> bool {
    text.contains(['.', 'e', 'E'])
}

/// Field-level failures: ParseError keeps its text, ValueError/TypeError are wrapped like it.
enum FieldError {
    Parse(String),
    Value(String),
}

impl From<ParseError> for FieldError {
    fn from(error: ParseError) -> Self {
        FieldError::Parse(error.0)
    }
}

struct Parser {
    depth: usize,
}

impl Parser {
    fn convert_message(
        &mut self,
        value: &Json,
        message: &mut DynamicMessage,
        path: &str,
    ) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ParseError(format!(
                "Message too deep. Max recursion depth is {MAX_DEPTH}"
            )));
        }
        let result = match message.descriptor().full_name() {
            ANY => self.convert_any(value, message, path),
            FIELD_MASK => convert_field_mask(value, message, path),
            _ => self.convert_fields(value, message, path, None),
        };
        self.depth -= 1;
        result
    }

    fn convert_any(
        &mut self,
        value: &Json,
        message: &mut DynamicMessage,
        path: &str,
    ) -> Result<(), ParseError> {
        let Json::Object(map) = value else {
            return Err(ParseError(format!(
                "Failed to parse Any at {path}: expected an object, not {}",
                py_type(value)
            )));
        };
        if map.is_empty() {
            return Ok(());
        }
        let Some(type_url) = map.get(TYPE_KEY) else {
            return Err(ParseError(format!(
                "@type is missing when parsing any message at {path}"
            )));
        };
        let Json::String(type_url) = type_url else {
            return Err(ParseError(format!(
                "Can not find message descriptor by type_url: {} at {path}",
                py_repr(type_url)
            )));
        };
        let name = type_url.rsplit('/').next().unwrap_or_default();
        let Some(descriptor) = proto::pool().get_message_by_name(name) else {
            return Err(ParseError(format!(
                "Can not find message descriptor by type_url: {type_url} at {path}"
            )));
        };
        let mut sub = DynamicMessage::new(descriptor.clone());
        match descriptor.full_name() {
            ANY | FIELD_MASK => {
                let inner = map
                    .get("value")
                    .ok_or_else(|| ParseError("'value'".into()))?;
                let inner_path = format!("{path}.value");
                if descriptor.full_name() == ANY {
                    self.convert_any(inner, &mut sub, &inner_path)?;
                } else {
                    convert_field_mask(inner, &mut sub, &inner_path)?;
                }
            }
            _ => self.convert_fields(value, &mut sub, path, Some(TYPE_KEY))?,
        }
        message.set_field_by_name("type_url", Value::String(type_url.clone()));
        message.set_field_by_name("value", Value::Bytes(sub.encode_to_vec().into()));
        Ok(())
    }

    fn convert_fields(
        &mut self,
        value: &Json,
        message: &mut DynamicMessage,
        path: &str,
        skip: Option<&str>,
    ) -> Result<(), ParseError> {
        let descriptor = message.descriptor();
        // Python iterates the value for field names, so an empty list or string is an empty message
        if matches!(value, Json::Array(items) if items.is_empty())
            || matches!(value, Json::String(text) if text.is_empty())
        {
            return Ok(());
        }
        let Json::Object(map) = value else {
            return Err(ParseError(format!(
                "Failed to parse {} at {path}: expected an object, not {}",
                descriptor.full_name(),
                py_type(value)
            )));
        };
        let mut oneofs: Vec<String> = Vec::new();
        for (name, item) in map {
            if Some(name.as_str()) == skip {
                continue;
            }
            let field = descriptor
                .fields()
                .find(|f| f.json_name() == name)
                .or_else(|| descriptor.get_field_by_name(name));
            let Some(field) = field else {
                let available: Vec<String> = descriptor
                    .fields()
                    .map(|f| py_repr(&Json::String(f.json_name().to_owned())))
                    .collect();
                return Err(ParseError(format!(
                    "Message type \"{}\" has no field named \"{name}\" at \"{path}\".\n Available Fields(except extensions): \"[{}]\"",
                    descriptor.full_name(),
                    available.join(", ")
                )));
            };
            let in_oneof = field.containing_oneof();
            if let Some(oneof) = &in_oneof {
                if !item.is_null() {
                    let oneof_name = oneof.name().to_owned();
                    if oneofs.contains(&oneof_name) {
                        return Err(ParseError(format!(
                            "Message type \"{}\" should not have multiple \"{oneof_name}\" oneof fields at \"{path}\".",
                            descriptor.full_name()
                        )));
                    }
                    oneofs.push(oneof_name);
                }
            }
            match self.convert_field(item, message, &field, name, path) {
                Ok(()) => {}
                Err(FieldError::Parse(text)) if in_oneof.is_some() => return Err(ParseError(text)),
                Err(FieldError::Parse(text) | FieldError::Value(text)) => {
                    return Err(ParseError(format!("Failed to parse {name} field: {text}.")))
                }
            }
        }
        Ok(())
    }

    fn convert_field(
        &mut self,
        value: &Json,
        message: &mut DynamicMessage,
        field: &FieldDescriptor,
        name: &str,
        path: &str,
    ) -> Result<(), FieldError> {
        if value.is_null() {
            message.clear_field(field);
            return Ok(());
        }
        let field_path = format!("{path}.{name}");
        if field.is_map() {
            message.clear_field(field);
            let Json::Object(entries) = value else {
                return Err(FieldError::Parse(format!(
                    "Map field {} must be in a dict which is {} at {field_path}",
                    field.name(),
                    py_str(value)
                )));
            };
            let Kind::Message(entry) = field.kind() else {
                unreachable!("map fields are messages")
            };
            let key_field = entry.map_entry_key_field();
            let value_field = entry.map_entry_value_field();
            let mut map = std::collections::HashMap::new();
            for (key, item) in entries {
                let key_value = scalar(
                    &Json::String(key.clone()),
                    &key_field,
                    true,
                    &format!("{field_path}.key"),
                )?;
                let item_path = format!(
                    "{field_path}[{}]",
                    map_key_text(&map_key(key_value.clone()))
                );
                let key = map_key(key_value);
                let converted = match value_field.kind() {
                    Kind::Message(sub) => {
                        let mut sub_message = DynamicMessage::new(sub);
                        self.convert_message(item, &mut sub_message, &item_path)?;
                        Value::Message(sub_message)
                    }
                    _ => scalar(item, &value_field, false, &item_path)?,
                };
                map.insert(key, converted);
            }
            message.set_field(field, Value::Map(map));
            return Ok(());
        }
        if field.is_list() {
            message.clear_field(field);
            let Json::Array(items) = value else {
                return Err(FieldError::Parse(format!(
                    "repeated field {name} must be in [] which is {} at {path}",
                    py_str(value)
                )));
            };
            let mut converted = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                if item.is_null() {
                    return Err(FieldError::Parse(format!(
                        "null is not allowed to be used as an element in a repeated field at {path}.{name}[{index}]"
                    )));
                }
                match field.kind() {
                    Kind::Message(sub) => {
                        let mut sub_message = DynamicMessage::new(sub);
                        self.convert_message(
                            item,
                            &mut sub_message,
                            &format!("{path}.{name}[{index}]"),
                        )?;
                        converted.push(Value::Message(sub_message));
                    }
                    _ => converted.push(scalar(
                        item,
                        field,
                        false,
                        &format!("{path}.{name}[{index}]"),
                    )?),
                }
            }
            message.set_field(field, Value::List(converted));
            return Ok(());
        }
        if let Kind::Message(_) = field.kind() {
            // SetInParent: the field is present even when the object is empty
            let Value::Message(sub) = message.get_field_mut(field) else {
                unreachable!("message field value")
            };
            self.convert_message(value, sub, &field_path)?;
            return Ok(());
        }
        let converted = scalar(value, field, false, &field_path)?;
        message.set_field(field, converted);
        Ok(())
    }
}

fn convert_field_mask(
    value: &Json,
    message: &mut DynamicMessage,
    path: &str,
) -> Result<(), ParseError> {
    let Json::String(text) = value else {
        return Err(ParseError(format!("FieldMask at {path} must be a string")));
    };
    let mut paths = Vec::new();
    if !text.is_empty() {
        for part in text.split(',') {
            paths.push(Value::String(camel_to_snake(part).map_err(ParseError)?));
        }
    }
    message.set_field_by_name("paths", Value::List(paths));
    Ok(())
}

fn map_key(value: Value) -> MapKey {
    match value {
        Value::Bool(v) => MapKey::Bool(v),
        Value::I32(v) => MapKey::I32(v),
        Value::I64(v) => MapKey::I64(v),
        Value::U32(v) => MapKey::U32(v),
        Value::U64(v) => MapKey::U64(v),
        Value::String(v) => MapKey::String(v),
        other => unreachable!("invalid map key {other:?}"),
    }
}

/// `_ConvertScalarFieldValue` plus the range checks Python's setters make.  Its conversion
/// errors name the field's path; ValueErrors (from `int()` or a range check) do not.
fn scalar(
    value: &Json,
    field: &FieldDescriptor,
    require_str: bool,
    path: &str,
) -> Result<Value, FieldError> {
    match convert_scalar(value, field, require_str) {
        Err(FieldError::Parse(text)) => Err(FieldError::Parse(format!("{text} at {path}"))),
        other => other,
    }
}

fn convert_scalar(
    value: &Json,
    field: &FieldDescriptor,
    require_str: bool,
) -> Result<Value, FieldError> {
    match field.kind() {
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
            let v = integer(value)?;
            i32::try_from(v)
                .map(Value::I32)
                .map_err(|_| out_of_range(v))
        }
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => {
            let v = integer(value)?;
            i64::try_from(v)
                .map(Value::I64)
                .map_err(|_| out_of_range(v))
        }
        Kind::Uint32 | Kind::Fixed32 => {
            let v = integer(value)?;
            u32::try_from(v)
                .map(Value::U32)
                .map_err(|_| out_of_range(v))
        }
        Kind::Uint64 | Kind::Fixed64 => {
            let v = integer(value)?;
            u64::try_from(v)
                .map(Value::U64)
                .map_err(|_| out_of_range(v))
        }
        Kind::Double => Ok(Value::F64(float(value, false)?)),
        Kind::Float => Ok(Value::F32(float(value, true)? as f32)),
        Kind::Bool => boolean(value, require_str).map(Value::Bool),
        Kind::String => match value {
            Json::String(text) => Ok(Value::String(text.clone())),
            other => Err(FieldError::Value(format!(
                "expected string or bytes-like object, got '{}'",
                py_type(other)
            ))),
        },
        Kind::Bytes => match value {
            Json::String(text) => Ok(Value::Bytes(decode_base64(text)?.into())),
            other => Err(FieldError::Value(format!(
                "argument should be a bytes-like object or ASCII string, not '{}'",
                py_type(other)
            ))),
        },
        Kind::Enum(descriptor) => enumeration(value, &descriptor).map(Value::EnumNumber),
        Kind::Message(_) => unreachable!("messages are not scalars"),
    }
}

fn out_of_range(value: i128) -> FieldError {
    FieldError::Value(format!("Value out of range: {value}"))
}

fn python_float_text(value: f64) -> String {
    let mut text = String::new();
    let _ = write!(text, "{value:?}");
    text
}

/// Python's `int(text)` for a decimal string: surrounding whitespace, a sign, and digits that may
/// be grouped with single underscores.
fn python_int(text: &str) -> Option<i128> {
    let trimmed = text.trim();
    let (negative, digits) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    let plain: String = digits.chars().filter(|c| *c != '_').collect();
    if !plain.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Values beyond i128 are out of every protobuf range; saturate so the range check reports them.
    let magnitude = plain.parse::<i128>().unwrap_or(i128::MAX);
    Some(if negative { -magnitude } else { magnitude })
}

/// Python's `float(text)`: surrounding whitespace, a sign, `inf`/`infinity`/`nan` in any case, or
/// a decimal with an optional exponent whose digits may be grouped with single underscores.
fn python_float(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let (negative, body) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let sign = if negative { -1.0 } else { 1.0 };
    match body.to_ascii_lowercase().as_str() {
        "inf" | "infinity" => return Some(sign * f64::INFINITY),
        "nan" => return Some(f64::NAN),
        _ => {}
    }
    // An underscore must sit between two digits
    let bytes = body.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'_'
            && !(index > 0
                && bytes[index - 1].is_ascii_digit()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    let plain = body.replace('_', "");
    let (mantissa, exponent) = match plain.find(['e', 'E']) {
        Some(at) => (&plain[..at], Some(&plain[at + 1..])),
        None => (plain.as_str(), None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return None;
    }
    if let Some(exponent) = exponent {
        let unsigned = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if unsigned.is_empty() || !digits(unsigned) {
            return None;
        }
    }
    plain.parse::<f64>().ok().map(|v| sign * v)
}

/// `_ConvertInteger`.
fn integer(value: &Json) -> Result<i128, FieldError> {
    match value {
        Json::Number(number) => {
            let text = number.to_string();
            if is_float_text(&text) {
                let v: f64 = text.parse().unwrap_or(f64::INFINITY);
                if !v.is_finite() {
                    return Err(FieldError::Value(
                        "cannot convert float infinity to integer".into(),
                    ));
                }
                if v.fract() != 0.0 {
                    return Err(FieldError::Parse(format!(
                        "Couldn't parse integer: {}",
                        python_float_text(v)
                    )));
                }
                Ok(v as i128)
            } else {
                Ok(python_int(&text).unwrap_or(i128::MAX))
            }
        }
        Json::String(text) => {
            if text.contains(' ') {
                return Err(FieldError::Parse(format!(
                    "Couldn't parse integer: \"{text}\""
                )));
            }
            if let Some(number) = python_int(text) {
                return Ok(number);
            }
            // Not an integer literal: Python then accepts a float literal with a whole value
            match python_float(text) {
                None => Err(FieldError::Value(format!(
                    "invalid literal for int() with base 10: {}",
                    py_repr(value)
                ))),
                Some(v) if !v.is_finite() || v.fract() != 0.0 => Err(FieldError::Parse(format!(
                    "Couldn't parse non-integer string: \"{text}\""
                ))),
                Some(v) => Ok(v as i128),
            }
        }
        Json::Bool(flag) => Err(FieldError::Parse(format!(
            "Bool value {} is not acceptable for integer field",
            if *flag { "True" } else { "False" }
        ))),
        other => Err(FieldError::Value(format!(
            "int() argument must be a string, a bytes-like object or a real number, not '{}'",
            py_type(other)
        ))),
    }
}

/// `_ConvertFloat`, with the 32-bit range check for `float` fields.
fn float(value: &Json, single: bool) -> Result<f64, FieldError> {
    const FLOAT_MAX: f64 = 3.4028234663852886e38;
    match value {
        Json::Number(number) => {
            let text = number.to_string();
            let v: f64 = text.parse().unwrap_or(f64::INFINITY);
            if is_float_text(&text) {
                if v.is_infinite() {
                    return Err(FieldError::Parse(if v > 0.0 {
                        "Couldn't parse Infinity or value too large, use quoted \"Infinity\" instead".into()
                    } else {
                        "Couldn't parse -Infinity or value too small, use quoted \"-Infinity\" instead".into()
                    }));
                }
                if single && v > FLOAT_MAX {
                    return Err(FieldError::Parse("Float value too large".into()));
                }
                if single && v < -FLOAT_MAX {
                    return Err(FieldError::Parse("Float value too small".into()));
                }
            }
            Ok(v)
        }
        Json::Bool(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
        Json::String(text) => {
            if text == "nan" {
                return Err(FieldError::Parse(
                    "Couldn't parse float \"nan\", use \"NaN\" instead".into(),
                ));
            }
            python_float(text)
                .ok_or_else(|| FieldError::Parse(format!("Couldn't parse float: {text}")))
        }
        other => Err(FieldError::Value(format!(
            "float() argument must be a string or a real number, not '{}'",
            py_type(other)
        ))),
    }
}

/// `_ConvertBool`.
fn boolean(value: &Json, require_str: bool) -> Result<bool, FieldError> {
    if require_str {
        return match value {
            Json::String(text) if text == "true" => Ok(true),
            Json::String(text) if text == "false" => Ok(false),
            other => Err(FieldError::Parse(format!(
                "Expected \"true\" or \"false\", not {}",
                match other {
                    Json::String(text) => text.clone(),
                    _ => py_repr(other),
                }
            ))),
        };
    }
    match value {
        Json::Bool(flag) => Ok(*flag),
        _ => Err(FieldError::Parse(
            "Expected true or false without quotes".into(),
        )),
    }
}

/// An enum value name, or a number (Python's `int(value)`); open enums accept unknown numbers.
fn enumeration(value: &Json, descriptor: &EnumDescriptor) -> Result<i32, FieldError> {
    if let Json::String(name) = value {
        if let Some(v) = descriptor.get_value_by_name(name) {
            return Ok(v.number());
        }
    }
    let number: Option<i128> = match value {
        Json::String(text) => python_int(text),
        Json::Bool(flag) => Some(i128::from(*flag)),
        Json::Number(number) => {
            let text = number.to_string();
            if is_float_text(&text) {
                text.parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| v.trunc() as i128)
            } else {
                python_int(&text)
            }
        }
        other => {
            return Err(FieldError::Value(format!(
                "int() argument must be a string, a bytes-like object or a real number, not '{}'",
                py_type(other)
            )))
        }
    };
    let Some(number) = number else {
        return Err(FieldError::Parse(format!(
            "Invalid enum value {} for enum type {}",
            match value {
                Json::String(text) => text.clone(),
                other => py_repr(other),
            },
            descriptor.full_name()
        )));
    };
    i32::try_from(number).map_err(|_| out_of_range(number))
}

/// Python's `base64.urlsafe_b64decode(text + padding)`: both alphabets, characters outside them
/// discarded, padding optional.
fn decode_base64(text: &str) -> Result<Vec<u8>, FieldError> {
    let cleaned: String = text
        .chars()
        .filter_map(|c| match c {
            '-' => Some('+'),
            '_' => Some('/'),
            'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/' => Some(c),
            _ => None,
        })
        .collect();
    let engine = base64::engine::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    engine
        .decode(cleaned)
        .map_err(|_| FieldError::Value("Incorrect padding".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn descriptor(name: &str) -> MessageDescriptor {
        proto::pool().get_message_by_name(name).expect(name)
    }

    #[test]
    fn round_trips_a_document_specifier() {
        let spec = json!({
            "type": "DOCTYPE_SCHEMATIC",
            "project": {"name": "live-demo", "path": "/tmp/live-demo"},
            "sheet_path": {"path": [{"value": "a3f0c7d2-0000-4000-8000-000000000001"}]}
        });
        let message =
            from_json(&spec, &descriptor("kiapi.common.types.DocumentSpecifier")).unwrap();
        assert_eq!(to_json(&message).unwrap(), spec);
    }

    #[test]
    fn accepts_json_names_and_clears_with_null() {
        let spec = json!({"type": 1, "sheetPath": {"path": []}, "project": null});
        let message =
            from_json(&spec, &descriptor("kiapi.common.types.DocumentSpecifier")).unwrap();
        assert_eq!(
            to_json(&message).unwrap(),
            json!({"type": "DOCTYPE_SCHEMATIC", "sheet_path": {}})
        );
    }

    #[test]
    fn rejects_unknown_fields_like_python() {
        let error =
            from_json(&json!({"typo": 1}), &descriptor("kiapi.common.types.KIID")).unwrap_err();
        assert_eq!(
            error.0,
            "Message type \"kiapi.common.types.KIID\" has no field named \"typo\" at \"KIID\".\n Available Fields(except extensions): \"['value']\""
        );
    }

    #[test]
    fn packs_and_prints_any() {
        let item = json!({
            "@type": "type.googleapis.com/kiapi.common.types.Vector2",
            "x_nm": "1000000",
            "y_nm": "-2540000"
        });
        let message = from_json(&item, &descriptor(ANY)).unwrap();
        assert_eq!(to_json(&message).unwrap(), item);
        let missing = from_json(&json!({"x_nm": 1}), &descriptor(ANY)).unwrap_err();
        assert_eq!(
            missing.0,
            "@type is missing when parsing any message at Any"
        );
        assert_eq!(
            to_json(&from_json(&json!({}), &descriptor(ANY)).unwrap()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn any_printing_refuses_unknown_types_and_excessive_nesting() {
        let mut unknown = DynamicMessage::new(descriptor(ANY));
        unknown.set_field_by_name(
            "type_url",
            Value::String("type.googleapis.com/unknown.Type".into()),
        );
        assert!(to_json(&unknown)
            .unwrap_err()
            .0
            .contains("Can not find message descriptor"));

        let mut nested = DynamicMessage::new(descriptor("kiapi.common.types.Vector2"));
        for _ in 0..MAX_DEPTH + 2 {
            nested = proto::pack(&nested);
        }
        assert!(to_json(&nested).unwrap_err().0.contains("Message too deep"));
    }

    #[test]
    fn integers_follow_python_conversion() {
        let vector = descriptor("kiapi.common.types.Vector2");
        let parsed = from_json(&json!({"x_nm": 5.0, "y_nm": " 7 "}), &vector);
        assert!(parsed.is_err(), "a string with a space is refused");
        let parsed = from_json(&json!({"x_nm": 5.0, "y_nm": "1_000"}), &vector).unwrap();
        assert_eq!(
            to_json(&parsed).unwrap(),
            json!({"x_nm": "5", "y_nm": "1000"})
        );
        let parsed = from_json(&json!({"x_nm": "2540000.0", "y_nm": "1e3"}), &vector).unwrap();
        assert_eq!(
            to_json(&parsed).unwrap(),
            json!({"x_nm": "2540000", "y_nm": "1000"})
        );
        let error = from_json(&json!({"x_nm": "2.5"}), &vector).unwrap_err();
        assert_eq!(
            error.0,
            "Failed to parse x_nm field: Couldn't parse non-integer string: \"2.5\" at Vector2.x_nm."
        );
        let error = from_json(&json!({"x_nm": 1.5}), &vector).unwrap_err();
        assert_eq!(
            error.0,
            "Failed to parse x_nm field: Couldn't parse integer: 1.5 at Vector2.x_nm."
        );
        let error = from_json(&json!({"x_nm": true}), &vector).unwrap_err();
        assert_eq!(error.0, "Failed to parse x_nm field: Bool value True is not acceptable for integer field at Vector2.x_nm.");
    }

    #[test]
    fn doubles_accept_numbers_strings_and_booleans() {
        let descriptor = descriptor("kiapi.common.types.Color");
        let message = from_json(
            &json!({"r": 0.1, "g": 1, "b": "0.5", "a": true}),
            &descriptor,
        )
        .unwrap();
        assert_eq!(
            to_json(&message).unwrap(),
            json!({"r": 0.1, "g": 1.0, "b": 0.5, "a": 1.0})
        );
        let message = from_json(&json!({"r": "-Infinity", "g": "NaN"}), &descriptor).unwrap();
        assert_eq!(
            to_json(&message).unwrap(),
            json!({"r": "-Infinity", "g": "NaN"})
        );
    }

    #[test]
    fn single_precision_prints_the_shortest_round_trip_decimal() {
        let kind = Kind::Float;
        assert_eq!(
            value_to_json(&kind, &Value::F32(0.1), 0).unwrap(),
            json!(0.1)
        );
        assert_eq!(
            value_to_json(&kind, &Value::F32(1.0 / 3.0), 0).unwrap(),
            json!(0.33333334)
        );
        assert_eq!(
            value_to_json(&kind, &Value::F32(2.0), 0).unwrap(),
            json!(2.0)
        );
    }

    #[test]
    fn maps_and_oneofs() {
        let spec = descriptor("kiapi.common.types.DocumentSpecifier");
        let error = from_json(
            &json!({"board_filename": "a.kicad_pcb", "sheet_path": {}}),
            &spec,
        )
        .unwrap_err();
        assert_eq!(
            error.0,
            "Message type \"kiapi.common.types.DocumentSpecifier\" should not have multiple \"identifier\" oneof fields at \"DocumentSpecifier\"."
        );
        let text = descriptor("kiapi.common.commands.SetTextVariables");
        let message = from_json(
            &json!({"variables": {"variables": {"B": "2", "A": "1"}}}),
            &text,
        )
        .unwrap();
        assert_eq!(
            to_json(&message).unwrap(),
            json!({"variables": {"variables": {"A": "1", "B": "2"}}})
        );
    }
}
