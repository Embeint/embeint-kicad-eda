// SPDX-License-Identifier: GPL-3.0-or-later
//! Batch preflight and response validation. Only the curated transaction contract
//! permits an edit; protobuf declarations alone do not imply rollback support.

use prost_reflect::{DynamicMessage, Kind, ReflectMessage, Value};
use serde_json::Value as Json;

use crate::{client::Error, pbjson, proto, registry};

pub struct Step {
    pub command: &'static registry::Command,
    pub request: DynamicMessage,
}

pub fn prepare(doc: &DynamicMessage, steps: &[Json]) -> Result<Vec<Step>, Error> {
    if steps.is_empty() || steps.len() > 100 {
        return Err(Error::Value("Supply between 1 and 100 steps".into()));
    }
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let name = step
                .get("command")
                .ok_or_else(|| Error::Value("'command'".into()))?;
            if name.is_array() || name.is_object() {
                return Err(Error::Value(format!(
                    "unhashable type: '{}'",
                    if name.is_array() { "list" } else { "dict" }
                )));
            }
            let name = name.as_str().ok_or_else(|| {
                Error::Value(format!(
                    "Unknown command {}; use live_schematic_list_commands",
                    pbjson::py_repr(name)
                ))
            })?;
            let command = registry::resolve(name)?;
            let payload = registry::request_payload(step.get("payload"));
            let mut request = pbjson::from_json(&payload, &proto::descriptor(&command.name))?;
            if !command.batchable {
                return Err(Error::Value(format!(
                    "step {index}: {} ({}) cannot run inside a batch",
                    command.name, command.kind
                )));
            }
            if !bind_document(&mut request, doc)? && command.kind == "edit" {
                return Err(Error::Value(format!(
                    "step {index}: {} has no transactional document target",
                    command.name
                )));
            }
            Ok(Step { command, request })
        })
        .collect()
}

fn bind_document(request: &mut DynamicMessage, doc: &DynamicMessage) -> Result<bool, Error> {
    let mut bound = false;
    for field in request.descriptor().fields() {
        if field.is_list() || field.is_map() {
            continue;
        }
        let Kind::Message(kind) = field.kind() else {
            continue;
        };
        let header = kind.full_name() == "kiapi.common.types.ItemHeader";
        if !header && kind.full_name() != "kiapi.common.types.DocumentSpecifier" {
            continue;
        }
        let mut value = proto::message_field(request, field.name());
        let target = if header {
            proto::message_field(&value, "document")
        } else {
            value.clone()
        };
        if target.fields().next().is_some() && target != *doc {
            return Err(Error::Value(
                "Batch steps must target the batch document exactly".into(),
            ));
        }
        if header {
            value.set_field_by_name("document", Value::Message(doc.clone()));
        } else {
            value = doc.clone();
        }
        request.set_field(&field, Value::Message(value));
        bound = true;
    }
    Ok(bound)
}

pub fn check_result(index: usize, step: &Step, response: &DynamicMessage) -> Result<(), Error> {
    let label = format!("step {index} ({})", step.command.name);
    for field in ["created_items", "updated_items", "items"] {
        for item in proto::list_field(response, field) {
            let Value::Message(item) = item else {
                continue;
            };
            let Some(status) = item.get_field_by_name("status") else {
                continue;
            };
            let Value::Message(status) = status.as_ref() else {
                continue;
            };
            if status.descriptor().get_field_by_name("code").is_some()
                && proto::int_field(status, "code") != 1
            {
                let mut error = proto::str_field(status, "error_message");
                if error.is_empty() {
                    error = proto::enum_name(
                        "kiapi.common.commands.ItemStatusCode",
                        proto::int_field(status, "code"),
                    );
                }
                return Err(Error::Runtime(format!(
                    "{label} rejected an item, batch rolled back: {error}"
                )));
            }
        }
    }
    for item in proto::list_field(response, "deleted_items") {
        if let Value::Message(item) = item {
            if proto::int_field(&item, "status") != 1 {
                let id = proto::str_field(&proto::message_field(&item, "id"), "value");
                return Err(Error::Runtime(format!(
                    "{label} could not delete {id}, batch rolled back"
                )));
            }
        }
    }
    for item in proto::list_field(response, "symbols") {
        if let Value::Message(item) = item {
            if matches!(
                item.get_field_by_name("updated").as_deref(),
                Some(Value::Bool(false))
            ) {
                let error = proto::str_field(&item, "error_message");
                let error = if error.is_empty() {
                    "symbol was not updated"
                } else {
                    &error
                };
                return Err(Error::Runtime(format!(
                    "{label} rejected a symbol, batch rolled back: {error}"
                )));
            }
        }
    }
    if let Some(field) = response.descriptor().get_field_by_name("status") {
        if matches!(field.kind(), Kind::Enum(e) if e.full_name() == "kiapi.common.types.ItemRequestStatus")
            && proto::int_field(response, "status") != 1
        {
            let status = proto::enum_name(
                "kiapi.common.types.ItemRequestStatus",
                proto::int_field(response, "status"),
            );
            return Err(Error::Runtime(format!("{label} returned {status}")));
        }
    }
    let name = step.request.descriptor().name().to_owned();
    let fields = match name.as_str() {
        "CreateItems" => Some(("items", "created_items")),
        "UpdateItems" => Some(("items", "updated_items")),
        "DeleteItems" => Some(("item_ids", "deleted_items")),
        "TransformBoardItems" => Some(("items", "items")),
        "UpdateSymbolsFromLibrary" => Some(("symbols", "symbols")),
        _ => None,
    };
    if let Some((input, output)) = fields {
        let expected = proto::list_field(&step.request, input).len();
        if proto::list_field(response, output).len() != expected
            && (expected != 0 || name != "UpdateSymbolsFromLibrary")
        {
            return Err(Error::Runtime(format!(
                "{label} did not acknowledge every item, batch rolled back"
            )));
        }
    }
    Ok(())
}
