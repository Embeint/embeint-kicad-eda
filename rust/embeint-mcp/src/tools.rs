// SPDX-License-Identifier: GPL-3.0-or-later
//! The tools this server implements, with the Python server's names, schemas and behaviour.
//!
//! `tools.json` holds each tool's name, description (as `inspect.cleandoc` gives it, which does not
//! depend on the Python version) and input/output schema exactly as the Python
//! server advertises them; `rust/parity/mcp_parity.py --write-schemas` regenerates it and the
//! parity check fails when the two differ.  Arguments are validated as FastMCP's pydantic models
//! validate them (including its pre-parsing of JSON passed as a string), and results are encoded
//! as FastMCP encodes them.

use std::sync::{Arc, OnceLock};

use embeint_ipc::client::{self, Client, Error};
use serde_json::{json, Map, Value as Json};
use tokio::sync::Mutex;

use crate::capabilities;

static TOOLS_JSON: &str = include_str!("../tools.json");

/// Every tool definition, in the order tools/list returns them.
pub fn list() -> &'static [Json] {
    static TOOLS: OnceLock<Vec<Json>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        let tools: Vec<Json> = serde_json::from_str(TOOLS_JSON).expect("tools.json");
        for tool in &tools {
            let name = tool["name"].as_str().expect("tool name");
            assert!(
                params(name).is_some(),
                "tools.json lists {name}, which has no implementation"
            );
        }
        tools
    })
}

/// The type of an argument, as the Python signature annotates it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arg {
    /// `dict`
    Dict,
    /// `list[dict]`
    ListOfDict,
    /// `str`
    Str,
    /// `str | None = None`
    OptionalStr,
}

/// Each implemented tool's arguments (name, type, required).
fn params(tool: &str) -> Option<&'static [(&'static str, Arg, bool)]> {
    const DOCUMENT: &[(&str, Arg, bool)] = &[("document_spec", Arg::Dict, true)];
    Some(match tool {
        "live_mcp_capabilities" | "live_schematic_documents" | "live_pcb_documents" => &[],
        "live_schematic_hierarchy" | "live_schematic_items" | "live_document_revision" => DOCUMENT,
        "live_operation_status" => &[("operation_id", Arg::Str, true)],
        "live_schematic_create_items" => &[
            ("document_spec", Arg::Dict, true),
            ("items", Arg::ListOfDict, true),
            ("expected_revision", Arg::OptionalStr, false),
            ("operation_id", Arg::OptionalStr, false),
        ],
        _ => return None,
    })
}

fn type_name(value: &Json) -> &'static str {
    match value {
        Json::Null => "NoneType",
        Json::Bool(_) => "bool",
        Json::Number(n) if n.is_f64() => "float",
        Json::Number(_) => "int",
        Json::String(_) => "str",
        Json::Array(_) => "list",
        Json::Object(_) => "dict",
    }
}

/// FastMCP's `pre_parse_json` and pydantic validation of the arguments model.  Returns the
/// validated arguments (missing optional ones as null) or pydantic-style error text.
fn validate(tool: &str, arguments: &Json) -> Result<Map<String, Json>, String> {
    let spec = params(tool).expect("implemented tool");
    let given = match arguments {
        Json::Object(map) => map.clone(),
        Json::Null => Map::new(),
        other => {
            return Err(format!(
                "1 validation error for {tool}Arguments\n  Input should be a valid dictionary [input_type={}]",
                type_name(other)
            ))
        }
    };
    let mut values = Map::new();
    let mut errors = Vec::new();
    for (name, kind, required) in spec {
        let Some(mut value) = given.get(*name).cloned() else {
            if *required {
                errors.push(format!("{name}\n  Field required [type=missing]"));
            } else {
                values.insert((*name).into(), Json::Null);
            }
            continue;
        };
        // Clients often send objects and lists as JSON text; FastMCP decodes it unless the
        // parameter is a plain str, and keeps the text when it decodes to a str or a number.
        if let (Json::String(text), true) = (&value, *kind != Arg::Str) {
            if let Ok(parsed) = serde_json::from_str::<Json>(text) {
                if !matches!(parsed, Json::String(_) | Json::Number(_) | Json::Bool(_)) {
                    value = parsed;
                }
            }
        }
        let problem = match (kind, &value) {
            (Arg::Dict, Json::Object(_)) => None,
            (Arg::Dict, _) => Some("Input should be a valid dictionary".to_owned()),
            (Arg::ListOfDict, Json::Array(items)) => items
                .iter()
                .position(|item| !item.is_object())
                .map(|index| format!("{index}: Input should be a valid dictionary")),
            (Arg::ListOfDict, _) => Some("Input should be a valid list".to_owned()),
            (Arg::Str, Json::String(_)) | (Arg::OptionalStr, Json::String(_) | Json::Null) => None,
            (Arg::Str | Arg::OptionalStr, _) => Some("Input should be a valid string".to_owned()),
        };
        match problem {
            Some(problem) => errors.push(format!(
                "{name}\n  {problem} [input_type={}]",
                type_name(&value)
            )),
            None => {
                values.insert((*name).into(), value);
            }
        }
    }
    if errors.is_empty() {
        Ok(values)
    } else {
        let count = errors.len();
        Err(format!(
            "{count} validation error{} for {tool}Arguments\n{}",
            if count == 1 { "" } else { "s" },
            errors.join("\n")
        ))
    }
}

/// What a tool returns: a dict (one text block) or a list (one block per item, plus
/// structuredContent, as FastMCP does for a `list[dict]` return type).
enum Output {
    Dict(Json),
    List(Vec<Json>),
}

fn text(value: &Json) -> Json {
    json!({"type": "text", "text": serde_json::to_string_pretty(value).expect("JSON")})
}

fn error_result(message: String) -> Json {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

/// Runs one tools/call and returns its CallToolResult.
pub async fn call(name: &str, arguments: Json, client: &Arc<Mutex<Client>>) -> Json {
    if params(name).is_none() {
        return error_result(format!("Unknown tool: {name}"));
    }
    let arguments = match validate(name, &arguments) {
        Ok(arguments) => arguments,
        Err(message) => return error_result(format!("Error executing tool {name}: {message}")),
    };
    match run(name, &arguments, client).await {
        Ok(Output::Dict(value)) => json!({"content": [text(&value)], "isError": false}),
        Ok(Output::List(items)) => json!({
            "content": items.iter().map(text).collect::<Vec<_>>(),
            "structuredContent": {"result": items},
            "isError": false,
        }),
        Err(error) => error_result(format!("Error executing tool {name}: {error}")),
    }
}

fn str_arg(arguments: &Map<String, Json>, name: &str) -> Option<String> {
    arguments
        .get(name)
        .and_then(Json::as_str)
        .map(str::to_owned)
}

async fn run(
    name: &str,
    args: &Map<String, Json>,
    client: &Arc<Mutex<Client>>,
) -> Result<Output, Error> {
    let document = || args.get("document_spec").cloned().unwrap_or(Json::Null);
    Ok(match name {
        "live_mcp_capabilities" => Output::Dict(capabilities::manifest()),
        "live_schematic_documents" => Output::Dict(client.lock().await.documents().await?),
        "live_pcb_documents" => match client.lock().await.boards().await? {
            Json::Array(items) => Output::List(items),
            other => Output::List(vec![other]),
        },
        "live_schematic_hierarchy" => {
            let spec = client::schematic_document(&document())?;
            Output::Dict(client.lock().await.hierarchy(&spec).await?)
        }
        "live_schematic_items" => {
            let spec = client::schematic_document(&document())?;
            Output::Dict(client.lock().await.items(&spec).await?)
        }
        "live_document_revision" => {
            let spec = client::editor_document(&document())?;
            Output::Dict(client.lock().await.document_revision(&spec).await?)
        }
        "live_operation_status" => {
            let operation_id = str_arg(args, "operation_id").unwrap_or_default();
            Output::Dict(client.lock().await.operation_status(&operation_id).await?)
        }
        "live_schematic_create_items" => {
            // Arguments are checked before the editor is contacted, as in server.py
            let spec = client::schematic_document(&document())?;
            let values = match args.get("items") {
                Some(Json::Array(values)) => values.clone(),
                _ => Vec::new(),
            };
            let items = client::parse_schematic_items(&values)?;
            let mut client = client.lock().await;
            // guarded(): the call's expected revision and operation id, and its operations
            client.begin_mutations(
                str_arg(args, "expected_revision"),
                str_arg(args, "operation_id"),
            );
            let result = client.create_items(&spec, &items).await;
            let operations = client.end_mutations();
            let mut result = result?;
            if let (Json::Object(map), Some(last)) = (&mut result, operations.last()) {
                map.insert("revision".into(), last["revision"].clone());
                map.insert("operations".into(), Json::Array(operations.clone()));
            }
            Output::Dict(result)
        }
        _ => unreachable!("validated tool name"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_tool_is_implemented() {
        assert!(list().len() >= 8);
    }

    #[test]
    fn validation_follows_fastmcp() {
        let ok = validate(
            "live_schematic_create_items",
            &json!({
                "document_spec": "{\"type\": \"DOCTYPE_SCHEMATIC\"}",
                "items": "[{\"@type\": \"x\"}]",
                "extra": 1,
            }),
        )
        .unwrap();
        assert_eq!(ok["document_spec"], json!({"type": "DOCTYPE_SCHEMATIC"}));
        assert_eq!(ok["items"], json!([{"@type": "x"}]));
        assert_eq!(ok["expected_revision"], Json::Null);
        // "null" decodes to None for an optional string; JSON numbers stay text and are refused
        let ok = validate(
            "live_schematic_create_items",
            &json!({
                "document_spec": {}, "items": [], "operation_id": "null"
            }),
        )
        .unwrap();
        assert_eq!(ok["operation_id"], Json::Null);
        let error = validate("live_schematic_create_items", &json!({"items": [1]})).unwrap_err();
        assert!(
            error.starts_with("2 validation errors for live_schematic_create_itemsArguments"),
            "{error}"
        );
        // A plain str parameter is never decoded
        let ok = validate("live_operation_status", &json!({"operation_id": "[1]"})).unwrap();
        assert_eq!(ok["operation_id"], json!("[1]"));
        assert!(validate("live_operation_status", &json!({"operation_id": 5})).is_err());
    }
}
