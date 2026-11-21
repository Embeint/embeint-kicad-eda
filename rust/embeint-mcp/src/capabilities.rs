// SPDX-License-Identifier: GPL-3.0-or-later
//! `live_mcp_capabilities` and the `embeint://mcp/capabilities` resource: the offline inventory
//! of this server's tools and the IPC commands they send, in the Python manifest's format
//! (support.py `local_capabilities`).  Nothing here contacts KiCad.

use embeint_ipc::registry;
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};

use crate::tools;

pub const CAPABILITIES_URI: &str = "embeint://mcp/capabilities";

/// Python's `json.dumps(value, sort_keys=True)` (spaced) or with `separators=(",", ":")`
/// (compact): sorted keys and non-ASCII characters escaped, so hashes match Python's.
pub fn canonical_json(value: &Json, spaced: bool) -> String {
    let mut out = String::new();
    write_canonical(value, spaced, &mut out);
    out
}

fn write_canonical(value: &Json, spaced: bool, out: &mut String) {
    let (item, pair) = if spaced { (", ", ": ") } else { (",", ":") };
    match value {
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            // Python compares code points; UTF-8 byte order is the same order
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                write_string(key, out);
                out.push_str(pair);
                write_canonical(&map[*key], spaced, out);
            }
            out.push('}');
        }
        Json::Array(items) => {
            out.push('[');
            for (index, element) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                write_canonical(element, spaced, out);
            }
            out.push(']');
        }
        Json::String(text) => write_string(text, out),
        other => out.push_str(&other.to_string()),
    }
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The manifest `live_mcp_capabilities` returns.
pub fn manifest() -> Json {
    let mut tool_schemas: Vec<Json> = tools::list()
        .iter()
        .map(|tool| {
            let mut entry = Map::new();
            entry.insert("name".into(), tool["name"].clone());
            entry.insert("input_schema".into(), tool["inputSchema"].clone());
            if let Some(output) = tool.get("outputSchema") {
                entry.insert("output_schema".into(), output.clone());
            }
            Json::Object(entry)
        })
        .collect();
    tool_schemas.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let mut command_schemas: Vec<Json> = registry::all().iter().map(registry::describe).collect();
    command_schemas.sort_by(|a, b| a["command"].as_str().cmp(&b["command"].as_str()));
    let inventory = json!({"tools": tool_schemas, "commands": command_schemas});
    let fingerprint = Sha256::digest(canonical_json(&inventory, false).as_bytes());
    let fingerprint: String = fingerprint.iter().map(|b| format!("{b:02x}")).collect();
    json!({
        "manifest_version": 1,
        "server": "Embeint KiCad Live",
        "server_version": crate::server_version(),
        "distribution": "source build",
        "application_version": null,
        "source_revision": null,
        "implementation": "rust",
        "transport": "stdio",
        "registration_source": "compiled-in tool registry (tools.json) and protobuf descriptors",
        "editor_connection_attempted": false,
        "live_support_verified": false,
        "limitations": [
            "Local registration is not capability negotiation with a running KiCad instance.",
            "application_version and source_revision are null in the native server; live_schematic_documents reports the running editor's version.",
            "Commands may be unavailable for an editor, document, or application version.",
            "Operation kind does not guarantee undo or rollback support.",
            "The registry is a curated adapter surface, not every declared protobuf message.",
        ],
        "tool_count": tool_schemas.len(),
        "command_count": command_schemas.len(),
        "schema_sha256": fingerprint,
        "tools": inventory["tools"],
        "commands": inventory["commands"],
    })
}

/// The resources/list entry for the manifest.
pub fn resource_entry() -> Json {
    json!({
        "uri": CAPABILITIES_URI,
        "name": "Embeint MCP capabilities",
        "description": "Offline tool/command schema inventory, also available as live_mcp_capabilities.",
        "mimeType": "application/json",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Limitations are single sentences with single spaces (a broken line continuation once left
    /// a run of spaces in one, which no parity check compares).
    #[test]
    fn limitations_are_plain_sentences() {
        for text in manifest()["limitations"].as_array().unwrap() {
            let text = text.as_str().unwrap();
            assert!(!text.contains("  ") && !text.contains('\n'), "{text:?}");
            assert!(text.ends_with('.'), "{text:?}");
        }
    }

    #[test]
    fn canonical_json_matches_python_dumps() {
        let value = json!({"b": [1, true, null], "a": "\u{e9}\u{1F600}\n"});
        // é and the surrogate pair 😀, as json.dumps(ensure_ascii=True) writes them
        let escaped = ["00e9", "d83d", "de00"]
            .map(|hex| format!("{}u{hex}", '\\'))
            .concat();
        let newline = format!("{}n", '\\');
        assert_eq!(
            canonical_json(&value, false),
            format!("{{\"a\":\"{escaped}{newline}\",\"b\":[1,true,null]}}")
        );
        assert_eq!(
            canonical_json(&value, true),
            format!("{{\"a\": \"{escaped}{newline}\", \"b\": [1, true, null]}}")
        );
    }

    #[test]
    fn commands_are_described() {
        let manifest = manifest();
        assert_eq!(manifest["command_count"], registry::all().len());
        let create = manifest["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["command"] == "kiapi.common.commands.CreateItems")
            .unwrap();
        assert_eq!(
            create["fields"][0],
            json!({"name": "header", "repeated": false, "type": "kiapi.common.types.ItemHeader"})
        );
    }
}
