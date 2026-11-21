// SPDX-License-Identifier: GPL-3.0-or-later
//! The tools this server implements, with the Python server's names, schemas and behaviour.
//!
//! `tools.json` holds every Python tool's name, description (as `inspect.cleandoc` gives it, which
//! does not depend on the Python version) and input/output schema exactly as the Python server
//! advertises them, in its tools/list order; `rust/parity/mcp_parity.py --write-schemas`
//! regenerates it and the parity check fails when the two differ.  This server advertises the
//! entries it implements.
//!
//! Each area of tools is a module with two functions:
//!
//! - `params(name) -> Option<&'static [Param]>`: the Python signature of each tool it implements
//!   (see [`args`]), or None for a tool it does not know;
//! - `run(name, &Args, &Ctx) -> Option<Result<Output, Error>>` (async): runs one of its tools
//!   with validated arguments, or None for a tool it does not know.
//!
//! Adding a tool touches only its area module (and the parity scenarios for that area).

/// A tool's signature as a `&'static [Param]`: `sig![req("document_spec", Arg::Dict), opt(...)]`.
/// (A `const fn` call in a plain slice literal is not promoted to a static.)
macro_rules! sig {
    ($($param:expr),* $(,)?) => {{
        const SIGNATURE: &[$crate::tools::args::Param] = &[$($param),*];
        SIGNATURE
    }};
}

pub mod args;
mod board;
mod board_design;
mod core;
mod footprint;
mod schematic;
mod shared;
mod symbol;

use std::sync::{Arc, OnceLock};

use base64::Engine as _;
use embeint_ipc::client::{Client, Error};
use serde_json::{json, Value as Json};
use tokio::sync::Mutex;

pub use args::{Args, Param};

static TOOLS_JSON: &str = include_str!("../../tools.json");

/// What a tool's run gets besides its arguments.
pub struct Ctx<'a> {
    /// The one connection to KiCad; a tool takes the lock for as long as its requests must not
    /// interleave with another call's (a whole transaction).
    pub client: &'a Arc<Mutex<Client>>,
}

/// What a tool returns: a dict (one text block), a list (one block per item, plus
/// structuredContent, as FastMCP does for a `list[dict]` return type), or an image (FastMCP's
/// `Image(data=..., format="png")`).
// Image and png() serve the view tools as their areas are ported
#[allow(dead_code)]
pub enum Output {
    Dict(Json),
    List(Vec<Json>),
    Image {
        data: Vec<u8>,
        mime_type: &'static str,
    },
}

#[allow(dead_code)]
impl Output {
    /// A PNG image result.
    pub fn png(data: Vec<u8>) -> Self {
        Output::Image {
            data,
            mime_type: "image/png",
        }
    }
}

/// Every area module's `params`, in lookup order.
fn params(name: &str) -> Option<&'static [Param]> {
    core::params(name)
        .or_else(|| shared::params(name))
        .or_else(|| schematic::params(name))
        .or_else(|| symbol::params(name))
        .or_else(|| footprint::params(name))
        .or_else(|| board::params(name))
        .or_else(|| board_design::params(name))
}

/// Runs a validated call in the area module that implements it.
async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<Output, Error> {
    if let Some(result) = core::run(name, args, ctx).await {
        return result;
    }
    if let Some(result) = shared::run(name, args, ctx).await {
        return result;
    }
    if let Some(result) = schematic::run(name, args, ctx).await {
        return result;
    }
    if let Some(result) = symbol::run(name, args, ctx).await {
        return result;
    }
    if let Some(result) = footprint::run(name, args, ctx).await {
        return result;
    }
    if let Some(result) = board::run(name, args, ctx).await {
        return result;
    }
    if let Some(result) = board_design::run(name, args, ctx).await {
        return result;
    }
    Err(Error::Runtime(format!(
        "{name} is declared but not implemented by this server"
    )))
}

/// Every Python tool definition (tools.json), in the Python server's order.
fn python_tools() -> &'static [Json] {
    static TOOLS: OnceLock<Vec<Json>> = OnceLock::new();
    TOOLS.get_or_init(|| serde_json::from_str(TOOLS_JSON).expect("tools.json"))
}

/// The definitions of the tools this server implements, in the order tools/list returns them.
pub fn list() -> &'static [Json] {
    static TOOLS: OnceLock<Vec<Json>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        python_tools()
            .iter()
            .filter(|tool| params(tool["name"].as_str().unwrap_or_default()).is_some())
            .cloned()
            .collect()
    })
}

/// The start of server.py's `guarded(...)`: this call's mutations run under the revision contract
/// with its `expected_revision` and `operation_id` arguments.  Hold the client lock from here to
/// [`end_guard`], and check arguments before it: a `?` in between returns without
/// [`end_guard`] (the options are then dropped by [`call`] after the tool returns, so they never
/// apply to another call, but the result carries no operations).
pub fn begin_guard(client: &mut Client, args: &Args) {
    client.begin_mutations(
        args.opt_str("expected_revision"),
        args.opt_str("operation_id"),
    );
}

/// The end of `guarded(...)`: a dict result gains `revision` (the last operation's) and
/// `operations`; an error drops the operations and is returned as it is.
pub fn end_guard(client: &mut Client, result: Result<Json, Error>) -> Result<Json, Error> {
    let operations = client.end_mutations();
    Ok(with_operations(result?, operations))
}

/// Adds `revision` (the last operation's) and `operations` to a dict result, as `guarded` does.
pub fn with_operations(mut result: Json, operations: Vec<Json>) -> Json {
    if let (Json::Object(map), Some(last)) = (&mut result, operations.last()) {
        map.insert("revision".into(), last["revision"].clone());
        map.insert("operations".into(), Json::Array(operations));
    }
    result
}

fn text(value: &Json) -> Json {
    json!({"type": "text", "text": serde_json::to_string_pretty(value).expect("JSON")})
}

pub fn error_result(message: String) -> Json {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

/// Drops mutation options a tool left on the client (it returned or panicked between
/// [`begin_guard`] and [`end_guard`]), so they never apply to the next call.  A call holds the
/// lock for its whole guarded section, so this cannot interrupt another call's.
pub async fn clear_mutations(client: &Arc<Mutex<Client>>) {
    client.lock().await.end_mutations();
}

/// Runs one tools/call and returns its CallToolResult.
pub async fn call(name: &str, arguments: Json, client: &Arc<Mutex<Client>>) -> Json {
    let Some(spec) = params(name).filter(|_| list().iter().any(|tool| tool["name"] == name)) else {
        return error_result(format!("Unknown tool: {name}"));
    };
    let arguments = match args::validate(name, spec, &arguments) {
        Ok(arguments) => arguments,
        Err(message) => return error_result(format!("Error executing tool {name}: {message}")),
    };
    let result = run(name, &arguments, &Ctx { client }).await;
    clear_mutations(client).await;
    match result {
        Ok(Output::Dict(value)) => json!({"content": [text(&value)], "isError": false}),
        Ok(Output::List(items)) => json!({
            "content": items.iter().map(text).collect::<Vec<_>>(),
            "structuredContent": {"result": items},
            "isError": false,
        }),
        Ok(Output::Image { data, mime_type }) => json!({
            "content": [{
                "type": "image",
                "data": base64::engine::general_purpose::STANDARD.encode(data),
                "mimeType": mime_type,
            }],
            "isError": false,
        }),
        Err(error) => error_result(format!("Error executing tool {name}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::args::Arg;
    use super::*;

    /// The annotation a parameter's JSON schema stands for, as FastMCP writes it.
    fn schema_kind(property: &Json) -> Option<Arg> {
        fn shape(schema: &Json) -> String {
            match schema["type"].as_str() {
                Some("array") => format!("list[{}]", shape(&schema["items"])),
                Some("object") => "dict".into(),
                Some(other) => other.into(),
                None => "?".into(),
            }
        }
        let mut parts: Vec<String> = match property["anyOf"].as_array() {
            Some(options) => options.iter().map(shape).collect(),
            None => vec![shape(property)],
        };
        let optional = parts.iter().any(|part| part == "null");
        parts.retain(|part| part != "null");
        Some(match (parts.join("|").as_str(), optional) {
            ("string", false) => Arg::Str,
            ("string", true) => Arg::OptStr,
            ("integer", false) => Arg::Int,
            ("integer", true) => Arg::OptInt,
            ("number", false) => Arg::Float,
            ("number", true) => Arg::OptFloat,
            ("boolean", false) => Arg::Bool,
            ("boolean", true) => Arg::OptBool,
            ("dict", false) => Arg::Dict,
            ("dict", true) => Arg::OptDict,
            ("list[dict]", false) => Arg::ListDict,
            ("list[dict]", true) => Arg::OptListDict,
            ("list[string]", false) => Arg::ListStr,
            ("list[string]", true) => Arg::OptListStr,
            ("list[number]", true) => Arg::OptListFloat,
            ("list[list[number]]", false) => Arg::ListListFloat,
            ("list[list[number]]", true) => Arg::OptListListFloat,
            ("number|list[number]", true) => Arg::OptFloatOrListFloat,
            ("number|dict", true) => Arg::OptFloatOrDict,
            _ => return None,
        })
    }

    /// Every Python parameter has an Arg kind, so any Python tool can be declared.
    #[test]
    fn every_python_parameter_has_a_kind() {
        for tool in python_tools() {
            let properties = tool["inputSchema"]["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for (name, property) in properties {
                assert!(
                    schema_kind(&property).is_some(),
                    "{}.{name}: no Arg kind for {property}",
                    tool["name"]
                );
            }
        }
    }

    /// Every implemented tool is a Python tool with a matching signature in tools.json: the same
    /// parameters, each with the type, required flag and default the Python signature gives.
    #[test]
    fn implemented_tools_match_python_signatures() {
        assert!(list().len() >= 19);
        for tool in list() {
            let name = tool["name"].as_str().unwrap();
            let schema = &tool["inputSchema"];
            let properties = schema["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            let required: Vec<&str> = schema["required"]
                .as_array()
                .map(|r| r.iter().filter_map(Json::as_str).collect())
                .unwrap_or_default();
            let spec = params(name).unwrap();
            let mut declared: Vec<&str> = spec.iter().map(|p| p.name).collect();
            let mut advertised: Vec<&str> = properties.keys().map(String::as_str).collect();
            declared.sort_unstable();
            advertised.sort_unstable();
            assert_eq!(
                declared, advertised,
                "{name}: parameters differ from the Python tool"
            );
            for param in spec {
                let property = &properties[param.name];
                assert_eq!(
                    Some(param.kind),
                    schema_kind(property),
                    "{name}.{}: type differs from the Python tool ({property})",
                    param.name
                );
                let is_required = param.default == args::Default::Required;
                assert_eq!(
                    is_required,
                    required.contains(&param.name),
                    "{name}.{}: required differs from the Python tool",
                    param.name
                );
                if let Some(default) = property.get("default") {
                    let ours = match param.default {
                        args::Default::Required | args::Default::None => Json::Null,
                        args::Default::Bool(flag) => json!(flag),
                        args::Default::Int(number) => json!(number),
                        args::Default::Float(number) => json!(number),
                        args::Default::Str(text) => json!(text),
                    };
                    assert_eq!(&ours, default, "{name}.{}: default differs", param.name);
                }
            }
        }
    }
}
