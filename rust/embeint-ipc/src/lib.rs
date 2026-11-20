// SPDX-License-Identifier: GPL-3.0-or-later
//! Native adapter to the Embeint KiCad IPC API (roadmap delivery gate 4).
//!
//! A port of the Python MCP's transport layer (`instance_discovery.py` and the `send`,
//! revision and transaction code of `client.py`): endpoint discovery, NNG request/reply over
//! local IPC, protobuf envelopes, and commits with rollback and timeout reconciliation.  The
//! Python implementation remains the reference; `rust/parity/mcp_parity.py` compares the two.

pub mod batch;
pub mod client;
pub mod discovery;
pub mod nng;
pub mod pbjson;
pub mod proto;
pub mod registry;
