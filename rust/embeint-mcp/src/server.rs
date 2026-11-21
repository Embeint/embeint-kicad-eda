// SPDX-License-Identifier: GPL-3.0-or-later
//! MCP over stdio: newline-delimited JSON-RPC 2.0, answered as the Python FastMCP server answers.
//!
//! Requests are read in order.  Tool calls run as tasks so pings and listings are answered while
//! a call waits on KiCad; the calls themselves take the client lock in turn, as the Python
//! server runs them one at a time.  stdout carries nothing but protocol messages.

use std::process::ExitCode;
use std::sync::Arc;

use embeint_ipc::client::Client;
use serde_json::{json, Value as Json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};

use crate::{resources, tools};

/// Protocol versions the Python SDK (mcp 1.30) accepts; the last is offered when a client asks
/// for another.
const PROTOCOL_VERSIONS: [&str; 4] = ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

pub const SERVER_NAME: &str = "KiCad Live";

/// The Python server's instructions, word for word: this server implements every live_* tool,
/// resource and command it does (mcp_parity.py compares the two texts).
pub const INSTRUCTIONS: &str = "These tools drive Embeint KiCad over IPC. For schematic capture/review, part selection, symbols, footprints or boards, load the bundled Embeint house style before editing: live_skill_reference {\"skill\":\"kicad-house-style\"} or kicad-skill://kicad-house-style/SKILL.md. Read the linked reference for the current task. Skill discovery, file reads, content manifest and local capabilities work offline. Follow the user's current intent and existing authorization; coordinate shared editor use. Preserve manual edits, verify mutation outcomes, and save explicitly. Local schemas and historical version notes do not prove current live editor support.";

fn response(id: &Json, result: Json) -> Json {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: &Json, code: i64, message: &str) -> Json {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn initialize(params: &Json) -> Json {
    let requested = params
        .get("protocolVersion")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let version = if PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        PROTOCOL_VERSIONS[3]
    };
    json!({
        "protocolVersion": version,
        "capabilities": {
            "experimental": {},
            "prompts": {"listChanged": false},
            "resources": {"subscribe": false, "listChanged": false},
            "tools": {"listChanged": false},
        },
        "serverInfo": {"name": SERVER_NAME, "version": crate::server_version()},
        "instructions": INSTRUCTIONS,
    })
}

/// Answers one request; notifications and responses get no answer.  Tool calls are answered by a
/// spawned task.
async fn dispatch(message: Json, client: &Arc<Mutex<Client>>, out: &mpsc::UnboundedSender<Json>) {
    let (Some(id), Some(method)) = (message.get("id").cloned(), message.get("method")) else {
        // Notifications (initialized, cancelled, ...) and responses to client requests: this
        // server sends no requests, so a response is never expected
        return;
    };
    let method = method.as_str().unwrap_or_default().to_owned();
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let reply = match method.as_str() {
        "initialize" => response(&id, initialize(&params)),
        "ping" => response(&id, json!({})),
        "tools/list" => response(&id, json!({"tools": tools::list()})),
        "tools/call" => {
            let client = Arc::clone(client);
            let out = out.clone();
            tokio::spawn(async move {
                let name = params
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                // The call runs as its own task so that a panic in a tool still gets a reply
                // (and leaves no mutation options behind) instead of leaving the client waiting
                let task = {
                    let (name, client) = (name.clone(), Arc::clone(&client));
                    tokio::spawn(async move { tools::call(&name, arguments, &client).await })
                };
                let result = match task.await {
                    Ok(result) => result,
                    Err(error) => {
                        tools::clear_mutations(&client).await;
                        let reason = if error.is_panic() {
                            "internal error"
                        } else {
                            "cancelled"
                        };
                        tools::error_result(format!("Error executing tool {name}: {reason}"))
                    }
                };
                let _ = out.send(response(&id, result));
            });
            return;
        }
        "resources/list" => response(&id, json!({"resources": resources::list()})),
        "resources/templates/list" => {
            response(&id, json!({"resourceTemplates": resources::templates()}))
        }
        "resources/read" => {
            let uri = params.get("uri").and_then(Json::as_str).unwrap_or_default();
            match resources::read(uri) {
                Ok(contents) => response(&id, contents),
                Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
            }
        }
        "prompts/list" => response(&id, json!({"prompts": []})),
        _ => error_response(&id, -32601, "Method not found"),
    };
    let _ = out.send(reply);
}

pub async fn serve(client: Client) -> ExitCode {
    let client = Arc::new(Mutex::new(client));
    let (out, mut outgoing) = mpsc::unbounded_channel::<Json>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = outgoing.recv().await {
            let mut line = serde_json::to_string(&message).expect("JSON");
            line.push('\n');
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Json>(&line) {
            Ok(message @ Json::Object(_)) => dispatch(message, &client, &out).await,
            Ok(_) => {
                let _ = out.send(error_response(&Json::Null, -32600, "Invalid Request"));
            }
            Err(_) => {
                let _ = out.send(error_response(&Json::Null, -32700, "Parse error"));
            }
        }
    }
    // stdin closed: running calls hold senders, so the writer drains until they have answered
    drop(out);
    let _ = writer.await;
    ExitCode::SUCCESS
}
