// SPDX-License-Identifier: GPL-3.0-or-later
//! `embeint-mcp`: the native MCP server for Embeint KiCad (roadmap delivery gate 4).
//!
//! It serves a subset of the Python server's `live_*` tools over stdio with the same names,
//! schemas and results: instance discovery, document reads, schematic item edits, generic
//! command execution/batches and schema inspection. The Python server stays the reference and serves
//! every other tool.  See rust/README.md.
//!
//! Usage:
//!   embeint-mcp              serve MCP over stdio
//!   embeint-mcp --discover   print the offline endpoint inventory (as instance_discovery.py)
//!   embeint-mcp --version    print the server version
//!   embeint-mcp --parity     JSON-lines interface for rust/parity/mcp_parity.py (internal)

mod board_client;
mod board_design;
mod board_rf;
mod board_router;
mod capabilities;
mod fab;
mod footprint_gen;
mod kicad_cli;
mod model_handoff;
mod parity;
mod resources;
mod server;
mod tools;

use std::process::ExitCode;

use embeint_ipc::client::{Client, DEFAULT_TIMEOUT_MS};

/// This server's version, as `version_identity()["server_version"]` names the Python one.  A
/// packaged build (qualification/rust_bundle.py) sets EMBEINT_SOURCE_REVISION at compile time,
/// e.g. `g0123456789` or `g0123456789.dirty`, as the Python package's version records it.
pub fn server_version() -> String {
    match option_env!("EMBEINT_SOURCE_REVISION") {
        Some(revision) if !revision.is_empty() => {
            format!("{}+rust.{revision}", env!("CARGO_PKG_VERSION"))
        }
        _ => format!("{}+rust", env!("CARGO_PKG_VERSION")),
    }
}

/// The reply timeout: KICAD_API_TIMEOUT_MS (1 to 600000), else 15000 ms like the Python client.
fn timeout_ms() -> Result<u64, String> {
    let text = std::env::var("KICAD_API_TIMEOUT_MS").unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        return Ok(DEFAULT_TIMEOUT_MS);
    }
    match text.parse::<u64>() {
        Ok(value) if text.bytes().all(|b| b.is_ascii_digit()) && (1..=600_000).contains(&value) => {
            Ok(value)
        }
        _ => Err(
            "KICAD_API_TIMEOUT_MS must be a whole number of milliseconds from 1 to 600000".into(),
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("embeint-mcp {}", server_version());
            ExitCode::SUCCESS
        }
        Some("--discover") => {
            let (report, status) = embeint_ipc::discovery::inventory_report();
            println!("{report}");
            ExitCode::from(status as u8)
        }
        Some("--parity") => runtime.block_on(parity::run()),
        None => {
            let client =
                match timeout_ms().and_then(|t| Client::new(None, t).map_err(|e| e.to_string())) {
                    Ok(client) => client,
                    Err(error) => {
                        eprintln!("embeint-mcp: {error}");
                        return ExitCode::FAILURE;
                    }
                };
            runtime.block_on(server::serve(client))
        }
        Some(other) => {
            eprintln!(
                "embeint-mcp: unknown argument {other}; use --discover, --version or no argument"
            );
            ExitCode::from(2)
        }
    }
}
