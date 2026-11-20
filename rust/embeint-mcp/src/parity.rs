// SPDX-License-Identifier: GPL-3.0-or-later
//! `embeint-mcp --parity`: the pure functions behind the tools, one JSON request per stdin line,
//! so rust/parity/mcp_parity.py can compare them with the Python functions they port on many
//! inputs.  Internal; not part of the MCP interface.
//!
//! Requests and replies:
//!   {"op": "parse", "type": T, "value": V}            protobuf JSON through T and back
//!   {"op": "items", "values": [...]}                  parse_items, re-packed as Any JSON
//!   {"op": "document", "kind": K, "value": V}         K: schematic, board or editor
//!   {"op": "url", "platform": "unix"|"windows", "url": U}
//!   {"op": "candidates", "root": R, "pipes": [...]}  Windows candidates, directories compared as given
//!   {"op": "temp", "env": {...}}                      windows_temp_dir
//!   {"op": "manifest"}                                the capabilities manifest
//! Each reply is {"ok": value} or {"error": {"class": ..., "message": ...}}.

use std::process::ExitCode;

use embeint_ipc::client::{self, Error};
use embeint_ipc::discovery::{self, Platform};
use embeint_ipc::{pbjson, proto};
use serde_json::{json, Value as Json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::capabilities;

fn error(class: &str, message: impl Into<String>) -> Json {
    json!({"error": {"class": class, "message": message.into()}})
}

fn printed(result: Result<Json, pbjson::ParseError>) -> Json {
    match result {
        Ok(value) => json!({"ok": value}),
        Err(e) => error("ParseError", e.0),
    }
}

fn client_error(e: Error) -> Json {
    error(e.class(), e.to_string())
}

fn handle(request: &Json) -> Json {
    let op = request["op"].as_str().unwrap_or_default();
    match op {
        "parse" => {
            let name = request["type"].as_str().unwrap_or_default();
            let Some(descriptor) = proto::pool().get_message_by_name(name) else {
                return error("KeyError", name);
            };
            match pbjson::from_json(&request["value"], &descriptor) {
                Ok(message) => printed(pbjson::to_json(&message)),
                Err(e) => error("ParseError", e.0),
            }
        }
        "items" => {
            let values = request["values"].as_array().cloned().unwrap_or_default();
            match client::parse_schematic_items(&values) {
                Ok(items) => {
                    let printed: Result<Vec<_>, _> = items
                        .iter()
                        .map(|m| pbjson::to_json(&proto::pack(m)))
                        .collect();
                    match printed {
                        Ok(values) => json!({"ok": values}),
                        Err(e) => error("ParseError", e.0),
                    }
                }
                Err(e) => client_error(e),
            }
        }
        "document" => {
            let value = &request["value"];
            let result = match request["kind"].as_str() {
                Some("schematic") => client::schematic_document(value),
                Some("board") => client::board_document(value),
                _ => client::editor_document(value),
            };
            match result {
                Ok(message) => printed(pbjson::to_json(&message)),
                Err(e) => client_error(e),
            }
        }
        "url" => {
            let platform = if request["platform"] == "windows" {
                Platform::Windows
            } else {
                Platform::Unix
            };
            match discovery::local_socket_path(
                request["url"].as_str().unwrap_or_default(),
                platform,
            ) {
                Ok(path) => json!({"ok": path}),
                Err(e) => {
                    json!({"error": {"class": "DiscoveryError", "code": e.code, "message": e.message}})
                }
            }
        }
        "candidates" => {
            let pipes: Vec<String> = request["pipes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let root = discovery::pure_windows_path(request["root"].as_str().unwrap_or_default());
            let found = discovery::windows_candidates(&root, &pipes, str::to_owned);
            json!({"ok": found.iter().map(|e| json!({"socket_url": e.socket_url, "kind": e.kind})).collect::<Vec<_>>()})
        }
        "temp" => {
            let env = request["env"].clone();
            let lookup = move |name: &str| env.get(name).and_then(Json::as_str).map(str::to_owned);
            json!({"ok": discovery::windows_temp_dir(lookup)})
        }
        "manifest" => json!({"ok": capabilities::manifest()}),
        _ => error("ValueError", format!("unknown op {op}")),
    }
}

pub async fn run() -> ExitCode {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        let reply = match serde_json::from_str::<Json>(&line) {
            Ok(request) => handle(&request),
            Err(e) => error("JSONDecodeError", e.to_string()),
        };
        let mut text = serde_json::to_string(&reply).expect("JSON");
        text.push('\n');
        if stdout.write_all(text.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}
