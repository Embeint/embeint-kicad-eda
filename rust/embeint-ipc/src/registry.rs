// SPDX-License-Identifier: GPL-3.0-or-later
//! The curated command contract and descriptor inspection, without a Python runtime.
//! `commands.json` is generated from the reference registry by the parity harness;
//! descriptor details come from the protocol compiled into this binary.

use std::sync::OnceLock;

use prost_reflect::{FieldDescriptor, Kind};
use serde_json::{json, Value as Json};

use crate::{client::Error, pbjson, proto};

pub struct Command {
    pub name: String,
    pub response: String,
    pub kind: String,
    pub batchable: bool,
    pub project_edit: bool,
}

pub fn all() -> &'static [Command] {
    static COMMANDS: OnceLock<Vec<Command>> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        let data: Json =
            serde_json::from_str(include_str!("../commands.json")).expect("command registry");
        data["commands"]
            .as_array()
            .expect("commands")
            .iter()
            .map(|entry| {
                let command = Command {
                    name: entry["command"].as_str().expect("command").to_owned(),
                    response: entry["returns"].as_str().expect("response").to_owned(),
                    kind: entry["kind"].as_str().expect("kind").to_owned(),
                    batchable: entry["batchable"].as_bool().expect("batchable"),
                    project_edit: entry["project_edit"].as_bool().expect("project_edit"),
                };
                proto::descriptor(&command.name);
                proto::descriptor(&command.response);
                command
            })
            .collect()
    })
}

pub fn resolve(name: &str) -> Result<&'static Command, Error> {
    if let Some(command) = all().iter().find(|command| command.name == name) {
        return Ok(command);
    }
    let matches = all()
        .iter()
        .filter(|command| command.name.rsplit('.').next() == Some(name))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [command] => Ok(command),
        [] => Err(Error::Value(format!(
            "Unknown command {}; use live_schematic_list_commands",
            pbjson::py_repr(&json!(name))
        ))),
        _ => Err(Error::Value(format!(
            "{} is ambiguous; use one of {}",
            pbjson::py_repr(&json!(name)),
            pbjson::py_repr(&json!(matches.iter().map(|c| &c.name).collect::<Vec<_>>()))
        ))),
    }
}

/// `payload or {}` in the reference client. Nested batch payloads have not passed
/// the top-level MCP dictionary validator.
pub fn request_payload(payload: Option<&Json>) -> Json {
    let nonempty = payload.filter(|value| match value {
        Json::Null => false,
        Json::Bool(value) => *value,
        Json::Number(value) => value.as_f64() != Some(0.0),
        Json::String(value) => !value.is_empty(),
        Json::Array(value) => !value.is_empty(),
        Json::Object(value) => !value.is_empty(),
    });
    nonempty.cloned().unwrap_or_else(|| json!({}))
}

/// Protobuf field type numbers used by Python's descriptor API.
pub fn type_number(field: &FieldDescriptor) -> i64 {
    match field.kind() {
        Kind::Double => 1,
        Kind::Float => 2,
        Kind::Int64 => 3,
        Kind::Uint64 => 4,
        Kind::Int32 => 5,
        Kind::Fixed64 => 6,
        Kind::Fixed32 => 7,
        Kind::Bool => 8,
        Kind::String => 9,
        Kind::Message(_) if field.is_group() => 10,
        Kind::Message(_) => 11,
        Kind::Bytes => 12,
        Kind::Uint32 => 13,
        Kind::Enum(_) => 14,
        Kind::Sfixed32 => 15,
        Kind::Sfixed64 => 16,
        Kind::Sint32 => 17,
        Kind::Sint64 => 18,
    }
}

pub fn describe(command: &Command) -> Json {
    let fields = proto::descriptor(&command.name).fields().map(|field| {
        let field_type = match field.kind() {
            Kind::Message(message) => json!(message.full_name()),
            Kind::Enum(enumeration) => json!(enumeration.values().map(|v| v.name().to_owned()).collect::<Vec<_>>()),
            _ => json!(type_number(&field)),
        };
        json!({"name": field.name(), "repeated": field.is_list() || field.is_map(), "type": field_type})
    }).collect::<Vec<_>>();
    json!({"command": command.name, "kind": command.kind, "batchable": command.batchable,
           "returns": command.response, "fields": fields})
}

pub fn list() -> Vec<Json> {
    all()
        .iter()
        .map(|command| {
            json!({"command": command.name.rsplit('.').next(), "full_name": command.name,
                                    "kind": command.kind, "returns": command.response})
        })
        .collect()
}

pub fn item_schema(name: &str, board: bool) -> Result<Json, Error> {
    let family = if board { "board" } else { "schematic" };
    let name = if name.starts_with("kiapi.") {
        name.to_owned()
    } else {
        format!("kiapi.{family}.types.{name}")
    };
    let prefix = if board {
        "kiapi.board."
    } else {
        "kiapi.schematic.types."
    };
    if !name.starts_with(prefix) && !name.starts_with("kiapi.common.types.") {
        return Err(Error::Value(format!(
            "Only KiCad {family}/common types are available"
        )));
    }
    let descriptor = proto::pool().get_message_by_name(&name).ok_or_else(|| {
        Error::Value(pbjson::py_repr(&json!(format!(
            "Couldn't find message {name}"
        ))))
    })?;
    let fields = descriptor
        .fields()
        .map(|field| {
            let (message, enumeration) = match field.kind() {
                Kind::Message(message) => (json!(message.full_name()), Json::Null),
                Kind::Enum(enumeration) => (
                    Json::Null,
                    json!(enumeration
                        .values()
                        .map(|v| v.name().to_owned())
                        .collect::<Vec<_>>()),
                ),
                _ => (Json::Null, Json::Null),
            };
            json!({"name": field.name(), "repeated": field.is_list() || field.is_map(),
               "message": message, "enum": enumeration, "scalar_type": type_number(&field)})
        })
        .collect::<Vec<_>>();
    Ok(json!({"type": descriptor.full_name(), "fields": fields}))
}
