// SPDX-License-Identifier: GPL-3.0-or-later
//! MCP resources and resource templates, with the Python server's URIs, names, MIME types and
//! text.  The capability manifest is served here; the bundled skills (`kicad-skill://`) are added
//! by the shared tools when they are ported.

use serde_json::{json, Value as Json};

use crate::capabilities;

/// resources/list: every fixed resource, in the Python server's order.
pub fn list() -> Vec<Json> {
    vec![capabilities::resource_entry()]
}

/// resources/templates/list.
pub fn templates() -> Vec<Json> {
    Vec::new()
}

/// resources/read: the contents of one resource, or None for an unknown URI.  An error is the
/// message the Python server reports for a resource whose function raised.
pub fn read(uri: &str) -> Option<Result<Json, String>> {
    if uri == capabilities::CAPABILITIES_URI {
        return Some(Ok(json!({"contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": capabilities::canonical_json(&capabilities::manifest(), true),
        }]})));
    }
    None
}
