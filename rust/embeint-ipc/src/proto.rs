// SPDX-License-Identifier: GPL-3.0-or-later
//! The KiCad IPC protocol as runtime descriptors, compiled from `api/proto` by build.rs.

use std::sync::OnceLock;

use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use serde_json::Value as Json;

use crate::pbjson;

static DESCRIPTORS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kiapi_descriptors.bin"));

/// Every message, enum and service in `api/proto` and the well-known types it imports.
pub fn pool() -> &'static DescriptorPool {
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();
    POOL.get_or_init(|| {
        DescriptorPool::decode(DESCRIPTORS).expect("descriptors compiled by build.rs")
    })
}

/// The descriptor of a message type in the protocol.  Panics for a name that is not in it, which
/// is a programming error caught by the unit tests.
pub fn descriptor(full_name: &str) -> MessageDescriptor {
    pool()
        .get_message_by_name(full_name)
        .unwrap_or_else(|| panic!("{full_name} is not in the KiCad protocol"))
}

/// A message built from protobuf JSON.  For requests the crate composes itself, whose JSON is
/// fixed in the code; tool arguments go through [`pbjson::from_json`] and report errors.
pub fn build(full_name: &str, value: Json) -> DynamicMessage {
    pbjson::from_json(&value, &descriptor(full_name))
        .unwrap_or_else(|e| panic!("internal {full_name} request: {e}"))
}

/// Sets a message-typed field, e.g. the `document` of a request built with [`build`].
pub fn with_field(
    mut message: DynamicMessage,
    name: &str,
    value: DynamicMessage,
) -> DynamicMessage {
    message.set_field_by_name(name, Value::Message(value));
    message
}

/// The type URL `Any.Pack` writes.
pub fn type_url(full_name: &str) -> String {
    format!("type.googleapis.com/{full_name}")
}

/// `Any.Pack(message)`.
pub fn pack(message: &DynamicMessage) -> DynamicMessage {
    let mut any = DynamicMessage::new(descriptor("google.protobuf.Any"));
    any.set_field_by_name(
        "type_url",
        Value::String(type_url(message.descriptor().full_name())),
    );
    any.set_field_by_name("value", Value::Bytes(message.encode_to_vec().into()));
    any
}

/// `Any.TypeName()`: the full message name after the last `/` of the type URL.
pub fn any_type_name(any: &DynamicMessage) -> String {
    match any.get_field_by_name("type_url").as_deref() {
        Some(Value::String(url)) => url.rsplit('/').next().unwrap_or_default().to_owned(),
        _ => String::new(),
    }
}

/// `Any.Unpack`: the message if the Any holds this type and decodes, else None.  As `Any.Is`, the
/// type URL must contain a `/`.
pub fn unpack(any: &DynamicMessage, full_name: &str) -> Option<DynamicMessage> {
    let has_slash = matches!(any.get_field_by_name("type_url").as_deref(), Some(Value::String(url)) if url.contains('/'));
    if !has_slash || any_type_name(any) != full_name {
        return None;
    }
    let bytes = match any.get_field_by_name("value").as_deref() {
        Some(Value::Bytes(bytes)) => bytes.clone(),
        _ => Default::default(),
    };
    DynamicMessage::decode(descriptor(full_name), bytes).ok()
}

/// A field's value as a string ("" when unset or not a string).
pub fn str_field(message: &DynamicMessage, name: &str) -> String {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

/// An enum or integer field's number (0 when unset).
pub fn int_field(message: &DynamicMessage, name: &str) -> i64 {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::EnumNumber(v) | Value::I32(v)) => i64::from(*v),
        Some(Value::U32(v)) => i64::from(*v),
        Some(Value::I64(v)) => *v,
        Some(Value::Bool(v)) => i64::from(*v),
        _ => 0,
    }
}

/// A message field (its default when unset).
pub fn message_field(message: &DynamicMessage, name: &str) -> DynamicMessage {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::Message(sub)) => sub.clone(),
        _ => panic!(
            "{name} is not a message field of {}",
            message.descriptor().full_name()
        ),
    }
}

/// A repeated field's values.
pub fn list_field(message: &DynamicMessage, name: &str) -> Vec<Value> {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::List(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// The name of an enum value, as Python's `Enum.Name` (the number if the value is unknown).
pub fn enum_name(enum_full_name: &str, number: i64) -> String {
    pool()
        .get_enum_by_name(enum_full_name)
        .and_then(|e| i32::try_from(number).ok().and_then(|n| e.get_value(n)))
        .map_or_else(|| number.to_string(), |v| v.name().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_protocol_compiles_and_packs() {
        let request = build(
            "kiapi.common.commands.GetOpenDocuments",
            json!({"type": "DOCTYPE_PCB"}),
        );
        let any = pack(&request);
        assert_eq!(
            any_type_name(&any),
            "kiapi.common.commands.GetOpenDocuments"
        );
        let back = unpack(&any, "kiapi.common.commands.GetOpenDocuments").unwrap();
        assert_eq!(
            pbjson::to_json(&back).unwrap(),
            json!({"type": "DOCTYPE_PCB"})
        );
        assert!(unpack(&any, "kiapi.common.commands.GetItems").is_none());
        assert_eq!(
            enum_name("kiapi.common.ApiStatusCode", 100),
            "AS_REVISION_MISMATCH"
        );
        assert_eq!(enum_name("kiapi.common.ApiStatusCode", 77), "77");
    }
}
