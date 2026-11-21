// SPDX-License-Identifier: GPL-3.0-or-later
//! MCP resources and resource templates, with the Python server's URIs, names, MIME types and
//! text: the bundled skills (`kicad-skill://`, see [`skills`]) and the capability manifest.

pub mod skills;

use serde_json::{json, Value as Json};

use crate::capabilities;

/// The skill file template, as server.py registers it.
const SKILL_TEMPLATE_PREFIX: &str = "kicad-skill://";

/// resources/list: every fixed resource, in the Python server's order.
pub fn list() -> Vec<Json> {
    vec![
        json!({
            "uri": skills::INDEX_URI,
            "name": "KiCad skills",
            "description": "Offline index of bundled skills, versions and available Markdown files.",
            "mimeType": "application/json",
        }),
        json!({
            "uri": skills::MANIFEST_URI,
            "name": "KiCad skill content manifest",
            "description": "Skill versions and content hashes, independent of the capability schema hash.",
            "mimeType": "application/json",
        }),
        capabilities::resource_entry(),
    ]
}

/// resources/templates/list.
pub fn templates() -> Vec<Json> {
    vec![json!({
        "uriTemplate": "kicad-skill://{skill}/{file}",
        "name": "KiCad skill file",
        "description": "One allowlisted bundled Markdown file; no editor connection is needed.",
        "mimeType": "text/markdown",
    })]
}

fn contents(uri: &str, mime_type: &str, text: String) -> Json {
    json!({"contents": [{"uri": uri, "mimeType": mime_type, "text": text}]})
}

/// The JSON-RPC error the Python SDK sends when a handler raised: code 0 and the message.
fn raised(message: String) -> Json {
    json!({"code": 0, "message": message})
}

/// FastMCP's template match: `kicad-skill://(?P<skill>[^/]+)/(?P<file>[^/]+)` in full.
fn skill_template(uri: &str) -> Option<(&str, &str)> {
    let (skill, file) = uri.strip_prefix(SKILL_TEMPLATE_PREFIX)?.split_once('/')?;
    (!skill.is_empty() && !file.is_empty() && !file.contains('/')).then_some((skill, file))
}

/// resources/read: the ReadResourceResult, or the JSON-RPC error object.
///
/// The SDK validates the request's `uri` as pydantic's `AnyUrl`, which parses it with the `url`
/// crate: an invalid URI is refused as invalid parameters, and a valid one is normalised (scheme
/// lowercased, `.` and `..` segments resolved, characters percent-encoded) before it is matched
/// and echoed.
pub fn read(uri: &str) -> Result<Json, Json> {
    let Ok(parsed) = url::Url::parse(uri) else {
        return Err(json!({"code": -32602, "message": "Invalid request parameters", "data": ""}));
    };
    let uri = parsed.as_str();
    match uri {
        capabilities::CAPABILITIES_URI => Ok(contents(
            uri,
            "application/json",
            capabilities::canonical_json(&capabilities::manifest(), true),
        )),
        // json.dumps(skill_index(), sort_keys=True); a resource function's error is reported as
        // FunctionResource.read words it
        skills::INDEX_URI => skills::skill_index()
            .map(|index| contents(uri, "application/json", capabilities::canonical_json(&Json::Array(index), true)))
            .map_err(|error| raised(format!("Error reading resource {uri}: {error}"))),
        skills::MANIFEST_URI => skills::skill_manifest()
            .map(|manifest| contents(uri, "application/json", capabilities::canonical_json(&manifest, true)))
            .map_err(|error| raised(format!("Error reading resource {uri}: {error}"))),
        _ => match skill_template(uri) {
            // The template's error is wrapped by create_resource and again by get_resource
            Some((skill, file)) => skills::read_skill(skill, file)
                .map(|read| {
                    contents(uri, "text/markdown", read["text"].as_str().unwrap_or_default().to_owned())
                })
                .map_err(|error| {
                    raised(format!(
                        "Error creating resource from template: Error creating resource from template: {error}"
                    ))
                }),
            None => Err(raised(format!("Unknown resource: {uri}"))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_matches_as_fastmcp() {
        assert_eq!(skill_template("kicad-skill://a/b.md"), Some(("a", "b.md")));
        assert_eq!(skill_template("kicad-skill://a/b/c.md"), None);
        assert_eq!(skill_template("kicad-skill://a/"), None);
        assert_eq!(skill_template("kicad-skill://index"), None);
    }

    #[test]
    fn uris_are_normalised_as_anyurl() {
        let normalised = read("KICAD-SKILL://kicad-house-style/x/../SKILL.md").unwrap();
        assert_eq!(
            normalised["contents"][0]["uri"],
            "kicad-skill://kicad-house-style/SKILL.md"
        );
        assert_eq!(read("not a url").unwrap_err()["code"], -32602);
        assert_eq!(
            read("kicad-skill://nope").unwrap_err()["message"],
            "Unknown resource: kicad-skill://nope"
        );
    }
}
