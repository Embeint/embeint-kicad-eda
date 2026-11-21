// SPDX-License-Identifier: GPL-3.0-or-later
//! Symbol libraries, the Symbol Editor and the library tables (symbol and footprint).
//!
//! A port of the library and Symbol Editor methods of `tools/live_schematic_mcp/client.py`
//! (`library_table`, `add_library`, `remove_library`, `reload_library`, `search_symbols`,
//! `library_symbol`, `new_symbol`, `edit_symbol`, `symbol_add_pins`, `symbol_delete`,
//! `symbol_set_properties`, `symbol_render`, `symbol_summary`, `symbol_save`) and of the server.py
//! tools that call them.  Requests, their order, result keys and error texts are Python's.
//!
//! `live_symbol_add_pins` lays the pins out with the `embeint-layout` crate, the Rust port of
//! `symbol_layout.layout_box` (measured identical by `rust/parity/parity.py`).  The Python server's
//! default layout backend is Python with no `layout_backend` key in the result; this server gives
//! that result.  See [`super`] for the module contract.

use embeint_ipc::client::{self, Client, Error};
use embeint_ipc::pbjson;
use embeint_ipc::proto::{
    self, build, int_field, list_field, message_field, str_field, with_field,
};
use prost_reflect::{DynamicMessage, Kind, ReflectMessage, Value};
use serde_json::{json, Map, Number, Value as Json};

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{Ctx, Output};

const C: &str = "kiapi.common.commands.";
const T: &str = "kiapi.common.types.";
const SC: &str = "kiapi.schematic.commands.";
const ST: &str = "kiapi.schematic.types.";
const EMPTY: &str = "google.protobuf.Empty";
const LIBRARY_STATUS: &str = "kiapi.common.types.LibraryCommandStatus";

const MANUFACTURER_FIELD: &str = "Manufacturer";
const MPN_FIELD: &str = "MPN";

const ADD_LIBRARY: &[Param] = sig![
    req("nickname", Arg::Str),
    req("path", Arg::Str),
    opt("scope", Arg::Str, Default::Str("global")),
    opt("description", Arg::Str, Default::Str("")),
];
const REMOVE_LIBRARY: &[Param] = sig![
    req("nickname", Arg::Str),
    opt("scope", Arg::Str, Default::Str("global")),
];
const SYMBOL_DOCUMENT: &[Param] = sig![req("symbol_document", Arg::Dict)];

pub fn params(tool: &str) -> Option<&'static [Param]> {
    Some(match tool {
        "live_symbol_search" => sig![
            req("query", Arg::Str),
            opt("limit", Arg::Int, Default::Int(25)),
        ],
        "live_symbol_get" | "live_symbol_edit" => sig![req("lib_id", Arg::Str)],
        "live_symbol_libraries" | "live_footprint_libraries" => {
            sig![opt("scope", Arg::Str, Default::Str("both"))]
        }
        "live_symbol_create_library"
        | "live_symbol_register_library"
        | "live_footprint_create_library"
        | "live_footprint_register_library" => ADD_LIBRARY,
        "live_symbol_unregister_library" | "live_footprint_unregister_library" => REMOVE_LIBRARY,
        "live_symbol_new" => sig![
            req("library", Arg::Str),
            req("name", Arg::Str),
            opt("reference_prefix", Arg::Str, Default::Str("U")),
            opt("unit_count", Arg::Int, Default::Int(1)),
            opt("inherit_from", Arg::Str, Default::Str("")),
            opt("power_symbol", Arg::Bool, Default::Bool(false)),
            opt("pin_names_outside", Arg::Bool, Default::Bool(false)),
            opt("exclude_from_bom", Arg::Bool, Default::Bool(false)),
            opt("exclude_from_board", Arg::Bool, Default::Bool(false)),
            opt("replace_existing", Arg::Bool, Default::Bool(false)),
            opt("manufacturer", Arg::Str, Default::Str("")),
            opt("mpn", Arg::Str, Default::Str("")),
        ],
        "live_symbol_add_pins" => sig![
            req("symbol_document", Arg::Dict),
            req("pins", Arg::ListDict),
            opt("draw_body", Arg::Bool, Default::Bool(true)),
            opt("replace", Arg::Bool, Default::Bool(false)),
            opt("arrange", Arg::Str, Default::Str("auto")),
            opt("group_gap", Arg::Int, Default::Int(2)),
        ],
        "live_symbol_set_properties" => sig![
            req("symbol_document", Arg::Dict),
            opt("value", Arg::OptStr, Default::None),
            opt("footprint", Arg::OptStr, Default::None),
            opt("datasheet", Arg::OptStr, Default::None),
            opt("description", Arg::OptStr, Default::None),
            opt("keywords", Arg::OptStr, Default::None),
            opt("reference_prefix", Arg::OptStr, Default::None),
            opt("footprint_filters", Arg::OptListStr, Default::None),
            opt("user_fields", Arg::OptListDict, Default::None),
            opt("remove_user_fields", Arg::OptListStr, Default::None),
            opt("show_pin_numbers", Arg::OptBool, Default::None),
            opt("show_pin_names", Arg::OptBool, Default::None),
            opt("exclude_from_bom", Arg::OptBool, Default::None),
            opt("exclude_from_board", Arg::OptBool, Default::None),
            opt("exclude_from_sim", Arg::OptBool, Default::None),
            opt("manufacturer", Arg::OptStr, Default::None),
            opt("mpn", Arg::OptStr, Default::None),
        ],
        "live_symbol_view" => sig![
            req("symbol_document", Arg::Dict),
            opt("dpi", Arg::Int, Default::Int(600)),
        ],
        "live_symbol_summary" | "live_symbol_save" => SYMBOL_DOCUMENT,
        "live_symbol_delete_items" => {
            sig![req("symbol_document", Arg::Dict), req("ids", Arg::ListStr),]
        }
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(run_tool(name, args, ctx).await)
}

async fn run_tool(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<Output, Error> {
    // The library-table tools take kind="footprint" when they are footprint tools
    let kind = if name.starts_with("live_footprint_") {
        "footprint"
    } else {
        "symbol"
    };
    // server.py checks the symbol document before the client method runs
    let document = || client::symbol_document(args.get("symbol_document"));
    Ok(match name {
        "live_symbol_search" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(
                search_symbols(&mut client, args.str("query"), clamped(args, "limit")).await?,
            )
        }
        "live_symbol_get" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(library_symbol(&mut client, args.str("lib_id")).await?)
        }
        "live_symbol_libraries" | "live_footprint_libraries" => {
            let mut client = ctx.client.lock().await;
            Output::List(library_table(&mut client, args.str("scope"), kind).await?)
        }
        "live_symbol_create_library"
        | "live_symbol_register_library"
        | "live_footprint_create_library"
        | "live_footprint_register_library" => {
            let library = NewLibrary {
                nickname: args.str("nickname"),
                path: args.str("path"),
                scope: args.str("scope"),
                description: args.str("description"),
                create: name.ends_with("_create_library"),
                kind,
            };
            let mut client = ctx.client.lock().await;
            Output::Dict(add_library(&mut client, library).await?)
        }
        "live_symbol_unregister_library" | "live_footprint_unregister_library" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(
                remove_library(&mut client, args.str("nickname"), args.str("scope"), kind).await?,
            )
        }
        "live_symbol_new" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(new_symbol(&mut client, args).await?)
        }
        "live_symbol_edit" => {
            let request = with_field(
                build(&format!("{SC}EditLibrarySymbol"), json!({})),
                "id",
                lib_identifier(args.str("lib_id"))?,
            );
            let mut client = ctx.client.lock().await;
            let response = client
                .send(&request, &format!("{SC}LibrarySymbolResponse"))
                .await?;
            Output::Dict(client::reply_json(&message_field(&response, "document"))?)
        }
        "live_symbol_add_pins" => {
            let doc = document()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(add_pins(&mut client, &doc, args).await?)
        }
        "live_symbol_set_properties" => {
            let doc = document()?;
            let properties = Properties {
                value: args.opt_str("value"),
                footprint: args.opt_str("footprint"),
                datasheet: args.opt_str("datasheet"),
                description: args.opt_str("description"),
                keywords: args.opt_str("keywords"),
                reference_prefix: args.opt_str("reference_prefix"),
                footprint_filters: args.opt_strs("footprint_filters"),
                user_fields: args.dicts("user_fields").cloned(),
                remove_user_fields: args.opt_strs("remove_user_fields"),
                show_pin_numbers: args.opt_bool("show_pin_numbers"),
                show_pin_names: args.opt_bool("show_pin_names"),
                exclude_from_bom: args.opt_bool("exclude_from_bom"),
                exclude_from_board: args.opt_bool("exclude_from_board"),
                exclude_from_sim: args.opt_bool("exclude_from_sim"),
                manufacturer: args.opt_str("manufacturer"),
                mpn: args.opt_str("mpn"),
            };
            let mut client = ctx.client.lock().await;
            Output::Dict(set_properties(&mut client, &doc, properties).await?)
        }
        "live_symbol_view" => {
            let doc = document()?;
            let dpi = clamped(args, "dpi");
            if !(150..=1200).contains(&dpi) {
                return Err(Error::Value("dpi must be between 150 and 1200".into()));
            }
            let request = with_field(
                build(
                    &format!("{SC}RenderLibrarySymbol"),
                    json!({"dpi": dpi, "unit": 0}),
                ),
                "document",
                doc,
            );
            let mut client = ctx.client.lock().await;
            let response = client
                .send(&request, &format!("{SC}RenderLibrarySymbolResponse"))
                .await?;
            let image = match response.get_field_by_name("image").as_deref() {
                Some(Value::Bytes(bytes)) => bytes.to_vec(),
                _ => Vec::new(),
            };
            Output::png(image)
        }
        "live_symbol_summary" => {
            let doc = document()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(symbol_summary(&mut client, &doc).await?)
        }
        "live_symbol_delete_items" => {
            let doc = document()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(symbol_delete(&mut client, &doc, &args.strs("ids")).await?)
        }
        "live_symbol_save" => {
            let doc = document()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(symbol_save(&mut client, &doc).await?)
        }
        _ => unreachable!("validated tool name"),
    })
}

// ---- Python value helpers ------------------------------------------------------------------------

/// An int argument as an i64.  Python ints are unbounded: one beyond i64 saturates, which every
/// caller treats as Python treats the large value (clamped, or out of a checked range).
fn clamped(args: &Args, name: &str) -> i64 {
    match args.get(name) {
        Json::Number(number) => number.as_i64().unwrap_or_else(|| {
            if number.to_string().starts_with('-') {
                i64::MIN
            } else {
                i64::MAX
            }
        }),
        _ => 0,
    }
}

/// `repr()` of a Python str: single quotes unless it holds one and no double quote, with
/// backslashes and ASCII control characters escaped.
fn repr_str(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `repr()` of a Python float, from Rust's shortest round-trip digits.
fn repr_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.into();
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };
        format!("{sign}{text}")
    } else {
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_owned()
        } else {
            format!("{first}.{rest}")
        };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        format!(
            "{sign}{mantissa}e{exponent_sign}{:02}",
            exponent.unsigned_abs()
        )
    }
}

/// `repr()` of a JSON value as Python holds it.
fn repr(value: &Json) -> String {
    match value {
        Json::String(text) => repr_str(text),
        Json::Number(number) => {
            let text = number.to_string();
            if text.contains(['.', 'e', 'E']) {
                repr_float(number.as_f64().unwrap_or(f64::NAN))
            } else {
                text
            }
        }
        Json::Array(items) => format!(
            "[{}]",
            items.iter().map(repr).collect::<Vec<_>>().join(", ")
        ),
        Json::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(key, item)| format!("{}: {}", repr_str(key), repr(item)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => pbjson::py_repr(other),
    }
}

/// `str()` of a JSON value as Python holds it.
fn py_str(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        other => repr(other),
    }
}

/// Python truthiness of a JSON value.
fn truthy(value: &Json) -> bool {
    match value {
        Json::Null => false,
        Json::Bool(flag) => *flag,
        Json::Number(number) => number.as_f64().is_some_and(|v| v != 0.0),
        Json::String(text) => !text.is_empty(),
        Json::Array(items) => !items.is_empty(),
        Json::Object(map) => !map.is_empty(),
    }
}

/// A Python float as FastMCP writes it.
fn float(value: f64) -> Json {
    Number::from_f64(value).map_or(Json::Null, Json::Number)
}

/// `mm(vector)`: a Vector2 as [x, y] in millimetres.
fn mm(vector: &DynamicMessage) -> Json {
    json!([
        float(int_field(vector, "x_nm") as f64 / 1e6),
        float(int_field(vector, "y_nm") as f64 / 1e6)
    ])
}

/// `round(value * 1_000_000)`: Python rounds half to even.
fn nm(value: f64) -> i64 {
    (value * 1_000_000.0).round_ties_even() as i64
}

/// `point(x, y)`: a Vector2 in nanometres from millimetres within +/-10000.
fn point(x: f64, y: f64) -> Result<Json, Error> {
    if ![x, y].iter().all(|v| v.is_finite() && v.abs() <= 10000.0) {
        return Err(Error::Value(
            "Coordinates must be finite millimeters within +/-10000".into(),
        ));
    }
    Ok(json!({"x_nm": nm(x), "y_nm": nm(y)}))
}

/// `Enum.Name(number)`: Python raises for a number the enum does not define.
fn enum_name(enum_full_name: &str, number: i64) -> Result<String, Error> {
    let descriptor = proto::pool()
        .get_enum_by_name(enum_full_name)
        .unwrap_or_else(|| panic!("{enum_full_name} is not in the KiCad protocol"));
    i32::try_from(number)
        .ok()
        .and_then(|n| descriptor.get_value(n))
        .map(|v| v.name().to_owned())
        .ok_or_else(|| {
            Error::Value(format!(
                "Enum {} has no name defined for value {number}",
                descriptor.name()
            ))
        })
}

/// `lib_identifier(lib_id)`: a LibraryIdentifier from `Library:Entry`.
fn lib_identifier(lib_id: &str) -> Result<DynamicMessage, Error> {
    match lib_id.split_once(':') {
        Some((library, entry)) if !library.is_empty() && !entry.is_empty() => Ok(build(
            &format!("{T}LibraryIdentifier"),
            json!({"library_nickname": library, "entry_name": entry}),
        )),
        _ => Err(Error::Value("lib_id must be Library:Symbol".into())),
    }
}

/// `lib_id_text(identifier)`.
fn lib_id_text(identifier: &DynamicMessage) -> String {
    format!(
        "{}:{}",
        str_field(identifier, "library_nickname"),
        str_field(identifier, "entry_name")
    )
}

/// `kiid(value)`: the UUID as Python's uuid.UUID writes it, or a new one for an empty value.
fn kiid(value: &str) -> Result<Json, Error> {
    let value = if value.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        client::canonical_uuid(value)?
    };
    Ok(json!({"value": value}))
}

/// `item_id(packed)`: the UUID of a packed pin, graphic shape or text, or None.
fn item_id(packed: &DynamicMessage) -> Option<String> {
    ["SchematicPin", "SchematicGraphicShape", "SchematicText"]
        .iter()
        .find_map(|name| proto::unpack(packed, &format!("{ST}{name}")))
        .map(|message| str_field(&message_field(&message, "id"), "value"))
        .filter(|id| !id.is_empty())
}

fn messages(message: &DynamicMessage, name: &str) -> Vec<DynamicMessage> {
    list_field(message, name)
        .into_iter()
        .filter_map(|value| match value {
            Value::Message(message) => Some(message),
            _ => None,
        })
        .collect()
}

// ---- Library tables ------------------------------------------------------------------------------

/// `library_kind(kind)`.
fn library_kind(kind: &str) -> &'static str {
    if kind == "footprint" {
        "LT_FOOTPRINT"
    } else {
        "LT_SYMBOL"
    }
}

/// `SCOPES[scope]`: a KeyError (whose text is the key's repr) for any other scope.
fn scope_value(scope: &str) -> Result<&'static str, Error> {
    match scope {
        "global" => Ok("LTS_GLOBAL"),
        "project" => Ok("LTS_PROJECT"),
        "both" => Ok("LTS_BOTH"),
        other => Err(Error::Value(repr_str(other))),
    }
}

/// `open_footprint_editor()`: the Footprint Editor lives in pcbnew; the schematic editor opens it.
async fn open_footprint_editor(client: &mut Client) -> Result<(), Error> {
    client
        .send(
            &build(&format!("{SC}OpenFootprintEditor"), json!({})),
            EMPTY,
        )
        .await?;
    Ok(())
}

/// `check_library_status(status, action)`.
fn check_library_status(status: &DynamicMessage, action: &str) -> Result<Json, Error> {
    let code = int_field(status, "code");
    if code != 1 {
        let name = enum_name(&format!("{LIBRARY_STATUS}.Code"), code)?;
        let message = format!(
            "Could not {action}: {name} {}",
            str_field(status, "error_message")
        );
        return Err(Error::Runtime(message.trim().to_owned()));
    }
    Ok(json!({"success": true}))
}

/// `library_table(scope, kind)`: the rows of the global and/or project table.
async fn library_table(client: &mut Client, scope: &str, kind: &str) -> Result<Vec<Json>, Error> {
    if kind == "footprint" {
        open_footprint_editor(client).await?;
    }
    let request = build(
        &format!("{C}GetLibraryTable"),
        json!({"type": library_kind(kind), "scope": scope_value(scope)?, "substituted": true}),
    );
    let response = client
        .send(&request, &format!("{C}LibraryTableResponse"))
        .await?;
    Ok(messages(&response, "rows")
        .iter()
        .map(|row| {
            let scope = match int_field(row, "scope") {
                1 => "global",
                2 => "project",
                _ => "?",
            };
            let mut entry = json!({
                "nickname": str_field(row, "nickname"),
                "path": str_field(row, "uri"),
                "scope": scope,
                "description": str_field(row, "description"),
                "writable": int_field(row, "writable") != 0,
            });
            if int_field(row, "disabled") != 0 {
                entry["disabled"] = json!(true);
            }
            entry
        })
        .collect())
}

/// `add_library(...)`'s arguments.
struct NewLibrary<'a> {
    nickname: &'a str,
    path: &'a str,
    scope: &'a str,
    description: &'a str,
    create: bool,
    kind: &'a str,
}

/// `add_library(...)`: adds a table row, creating an empty library first when asked.
async fn add_library(client: &mut Client, library: NewLibrary<'_>) -> Result<Json, Error> {
    if !matches!(library.scope, "global" | "project") {
        return Err(Error::Value("scope must be global or project".into()));
    }
    let suffix = if library.kind == "symbol" {
        ".kicad_sym"
    } else {
        ".pretty"
    };
    let path = library.path.trim_end_matches('/');
    if library.create && !path.ends_with(suffix) {
        return Err(Error::Value(format!(
            "A new {} library must end in {suffix}",
            library.kind
        )));
    }
    if library.kind == "footprint" {
        open_footprint_editor(client).await?;
    }
    let request = build(
        &format!("{C}AddLibraryTableEntry"),
        json!({
            "entry": {"nickname": library.nickname, "uri": path, "type": library_kind(library.kind),
                      "description": library.description},
            "scope": scope_value(library.scope)?,
            "create_library": library.create,
        }),
    );
    let status = client.send(&request, LIBRARY_STATUS).await?;
    check_library_status(&status, &format!("add library {}", library.nickname))
}

/// `remove_library(nickname, scope, kind)`: the library file is left on disk.
async fn remove_library(
    client: &mut Client,
    nickname: &str,
    scope: &str,
    kind: &str,
) -> Result<Json, Error> {
    if kind == "footprint" {
        open_footprint_editor(client).await?;
    }
    let request = build(
        &format!("{C}DeleteLibraryTableEntry"),
        json!({"type": library_kind(kind), "nickname": nickname, "scope": scope_value(scope)?}),
    );
    let status = client.send(&request, LIBRARY_STATUS).await?;
    check_library_status(&status, &format!("remove {nickname}"))
}

/// `reload_library(nickname)` for a symbol library: finds its table, then reloads it.
async fn reload_library(client: &mut Client, nickname: &str) -> Result<Json, Error> {
    for row in library_table(client, "both", "symbol").await? {
        if row["nickname"] == nickname {
            let scope = row["scope"].as_str().unwrap_or("?");
            let request = build(
                &format!("{C}ReloadLibrary"),
                json!({"type": "LT_SYMBOL", "scope": scope_value(scope)?, "nickname": [nickname]}),
            );
            let status = client.send(&request, LIBRARY_STATUS).await?;
            return check_library_status(&status, "reload");
        }
    }
    Err(Error::Value(format!(
        "Library {nickname} is not in the symbol library tables"
    )))
}

// ---- Library symbols -----------------------------------------------------------------------------

/// `library_symbols(lib_ids)`: the SchematicSymbols KiCad returns for these ids.
async fn library_symbols(
    client: &mut Client,
    lib_ids: &[String],
) -> Result<Vec<DynamicMessage>, Error> {
    let ids = lib_ids
        .iter()
        .map(|id| lib_identifier(id).map(Value::Message))
        .collect::<Result<Vec<_>, _>>()?;
    let mut request = build(
        &format!("{C}GetItemsFromLibrary"),
        json!({"type": "LT_SYMBOL"}),
    );
    request.set_field_by_name("item_ids", Value::List(ids));
    let response = client
        .send(&request, &format!("{C}GetItemsResponse"))
        .await?;
    Ok(messages(&response, "items")
        .iter()
        .filter_map(|packed| proto::unpack(packed, &format!("{ST}SchematicSymbol")))
        .collect())
}

fn field_text(symbol: &DynamicMessage, name: &str) -> String {
    let field = message_field(symbol, &format!("{name}_field"));
    str_field(&message_field(&field, "text"), "text")
}

/// The symbol's unit count, or 1 when unset (`symbol.unit_count or 1`).
fn units(symbol: &DynamicMessage) -> i64 {
    match int_field(symbol, "unit_count") {
        0 => 1,
        count => count,
    }
}

/// `search_symbols(query, limit)`.
async fn search_symbols(client: &mut Client, query: &str, limit: i64) -> Result<Json, Error> {
    let request = build(
        &format!("{C}SearchLibraries"),
        json!({"type": "LT_SYMBOL", "query": query}),
    );
    let response = client
        .send(&request, &format!("{C}SearchLibrariesResponse"))
        .await?;
    let ids: Vec<String> = messages(&response, "items")
        .iter()
        .map(lib_id_text)
        .collect();
    let count = usize::try_from(limit.clamp(1, 200)).unwrap_or(1);
    let top: Vec<String> = ids.iter().take(count).cloned().collect();
    let mut result = Map::new();
    result.insert("total".into(), json!(ids.len()));
    let loading = int_field(&response, "libraries_loading");
    if loading != 0 {
        result.insert("libraries_still_loading".into(), json!(loading));
    }
    if top.is_empty() {
        result.insert("results".into(), json!([]));
        return Ok(Json::Object(result));
    }
    let symbols = library_symbols(client, &top).await?;
    // A dict keyed by lib_id: a later symbol with the same id replaces an earlier one
    let found = |lib_id: &str| {
        symbols
            .iter()
            .rev()
            .find(|s| lib_id_text(&message_field(s, "id")) == lib_id)
    };
    let results = top
        .iter()
        .map(|lib_id| {
            let mut entry = json!({"lib_id": lib_id});
            if let Some(symbol) = found(lib_id) {
                let pins = messages(symbol, "items")
                    .iter()
                    .filter(|child| {
                        proto::any_type_name(&message_field(child, "item"))
                            .ends_with(".SchematicPin")
                    })
                    .count();
                entry["description"] = json!(field_text(symbol, "description"));
                entry["keywords"] = json!(str_field(symbol, "keywords"));
                entry["pins"] = json!(pins);
                entry["units"] = json!(units(symbol));
                let footprint = field_text(symbol, "footprint");
                if !footprint.is_empty() {
                    entry["footprint"] = json!(footprint);
                }
            }
            entry
        })
        .collect::<Vec<_>>();
    result.insert("results".into(), Json::Array(results));
    Ok(Json::Object(result))
}

/// `library_symbol(lib_id)`: one library symbol summarised.
async fn library_symbol(client: &mut Client, lib_id: &str) -> Result<Json, Error> {
    let symbols = library_symbols(client, &[lib_id.to_owned()]).await?;
    match symbols.first() {
        Some(symbol) => summarize_symbol(symbol),
        None => Err(Error::Value(format!("{lib_id} not found"))),
    }
}

/// `summarize_symbol(symbol)`: the parts of a library symbol an agent needs, without graphics.
fn summarize_symbol(symbol: &DynamicMessage) -> Result<Json, Error> {
    let mut pins = Vec::new();
    let mut user_fields = Map::new();
    for child in messages(symbol, "items") {
        let item = message_field(&child, "item");
        if let Some(field) = proto::unpack(&item, &format!("{ST}SchematicField")) {
            let name = str_field(&field, "name");
            if !name.is_empty() {
                user_fields.insert(
                    name,
                    json!(str_field(&message_field(&field, "text"), "text")),
                );
                continue;
            }
        }
        if let Some(pin) = proto::unpack(&item, &format!("{ST}SchematicPin")) {
            let mut entry = json!({
                "number": str_field(&pin, "number"),
                "name": str_field(&pin, "name"),
                "type": enum_name(&format!("{T}ElectricalPinType"), int_field(&pin, "electrical_type"))?,
                "position_mm": mm(&message_field(&pin, "position")),
                "orientation": enum_name(&format!("{ST}SchematicPinOrientation"), int_field(&pin, "orientation"))?,
            });
            if int_field(&pin, "visible") == 0 {
                entry["hidden"] = json!(true);
            }
            if child.has_field_by_name("unit") {
                entry["unit"] = json!(int_field(&message_field(&child, "unit"), "unit"));
            }
            pins.push(entry);
        }
    }
    // sorted(key=(len(number), number)): length in code points, then code point order
    pins.sort_by(|a, b| {
        let key = |pin: &Json| {
            let number = pin["number"].as_str().unwrap_or_default().to_owned();
            (number.chars().count(), number)
        };
        key(a).cmp(&key(b))
    });
    let mut result = Map::new();
    result.insert(
        "lib_id".into(),
        json!(lib_id_text(&message_field(symbol, "id"))),
    );
    for name in [
        "reference",
        "value",
        "footprint",
        "datasheet",
        "description",
    ] {
        result.insert(name.into(), json!(field_text(symbol, name)));
    }
    result.insert("fields".into(), Json::Object(user_fields));
    let mut placement = Map::new();
    for name in ["reference", "value"] {
        let text = message_field(&message_field(symbol, &format!("{name}_field")), "text");
        let align = client::reply_json(&text)?
            .get("attributes")
            .and_then(|attributes| attributes.get("horizontal_alignment"))
            .cloned()
            .unwrap_or(Json::Null);
        placement.insert(
            name.into(),
            json!({"position_mm": mm(&message_field(&text, "position")), "align": align}),
        );
    }
    result.insert("field_placement".into(), Json::Object(placement));
    result.insert("keywords".into(), json!(str_field(symbol, "keywords")));
    let filters: Vec<Json> = list_field(symbol, "footprint_filters")
        .into_iter()
        .filter_map(|value| match value {
            Value::String(text) => Some(Json::String(text)),
            _ => None,
        })
        .collect();
    result.insert("footprint_filters".into(), Json::Array(filters));
    result.insert("units".into(), json!(units(symbol)));
    result.insert(
        "type".into(),
        json!(enum_name(
            &format!("{ST}SchematicSymbolType"),
            int_field(symbol, "type")
        )?),
    );
    result.insert("pins".into(), Json::Array(pins));
    Ok(Json::Object(result))
}

// ---- The Symbol Editor ---------------------------------------------------------------------------

/// One request inside a symbol transaction, with `check_item_statuses` applied when `check`
/// names the command.
struct Step {
    request: DynamicMessage,
    response: String,
    check: Option<&'static str>,
}

/// `with self.transaction(doc, description): ...` around `steps`: one undo step, dropped (rolled
/// back) if a step fails, and reconciled rather than resent when a reply is lost.
async fn transaction(
    client: &mut Client,
    doc: &DynamicMessage,
    description: &str,
    steps: &[Step],
) -> Result<(), Error> {
    let header = client::item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let outcome = run_steps(client, steps).await;
    client
        .finish_transaction(&header, &id, &operation_id, description, outcome)
        .await
}

async fn run_steps(client: &mut Client, steps: &[Step]) -> Result<(), Error> {
    for step in steps {
        let response = client.send(&step.request, &step.response).await?;
        if let Some(command) = step.check {
            check_item_statuses(command, &response)?;
        }
    }
    Ok(())
}

/// `check_item_statuses(0, command, response)`: a per-item rejection inside an OK reply.
fn check_item_statuses(command: &str, response: &DynamicMessage) -> Result<(), Error> {
    let label = format!("step 0 ({command})");
    for field in ["created_items", "updated_items", "items"] {
        let Some(descriptor) = response.descriptor().get_field_by_name(field) else {
            continue;
        };
        if !matches!(descriptor.kind(), Kind::Message(m) if m.get_field_by_name("status").is_some())
        {
            continue;
        }
        for result in messages(response, field) {
            let status = message_field(&result, "status");
            let code = int_field(&status, "code");
            if code != 1 {
                let mut error = str_field(&status, "error_message");
                if error.is_empty() {
                    error = proto::enum_name(&format!("{C}ItemStatusCode"), code);
                }
                return Err(Error::Runtime(format!(
                    "{label} rejected an item, batch rolled back: {error}"
                )));
            }
        }
    }
    for result in messages(response, "deleted_items") {
        if int_field(&result, "status") != 1 {
            let id = str_field(&message_field(&result, "id"), "value");
            return Err(Error::Runtime(format!(
                "{label} could not delete {id}, batch rolled back"
            )));
        }
    }
    let status = int_field(response, "status");
    if status != 1 {
        let name = proto::enum_name(&format!("{T}ItemRequestStatus"), status);
        return Err(Error::Runtime(format!("{label} returned {name}")));
    }
    Ok(())
}

/// `new_symbol(...)`: a new symbol in the Symbol Editor with hidden sourcing fields.
async fn new_symbol(client: &mut Client, args: &Args) -> Result<Json, Error> {
    let unit_count = match args.get("unit_count") {
        Json::Number(number) => number
            .as_i64()
            .filter(|count| i32::try_from(*count).is_ok())
            .ok_or_else(|| Error::Value(format!("Value out of range: {number}")))?,
        _ => 1,
    };
    let request = build(
        &format!("{SC}NewLibrarySymbol"),
        json!({
            "library_nickname": args.str("library"),
            "name": args.str("name"),
            "reference_prefix": args.str("reference_prefix"),
            "unit_count": unit_count,
            "inherit_from": args.str("inherit_from"),
            "power_symbol": args.bool("power_symbol"),
            "pin_names_outside": args.bool("pin_names_outside"),
            "hide_pin_numbers": false,
            "hide_pin_names": false,
            "exclude_from_bom": args.bool("exclude_from_bom"),
            "exclude_from_board": args.bool("exclude_from_board"),
            "replace_existing": args.bool("replace_existing"),
        }),
    );
    let response = client
        .send(&request, &format!("{SC}LibrarySymbolResponse"))
        .await?;
    let doc = message_field(&response, "document");
    // Every new symbol carries sourcing fields, hidden, ready to fill in (`manufacturer or ""`)
    let properties = Properties {
        manufacturer: Some(args.str("manufacturer").to_owned()),
        mpn: Some(args.str("mpn").to_owned()),
        ..Properties::default()
    };
    set_properties(client, &doc, properties).await?;
    client::reply_json(&doc)
}

/// `symbol_set_properties(...)`'s settings; None leaves one unchanged.
#[derive(Default)]
struct Properties {
    value: Option<String>,
    footprint: Option<String>,
    datasheet: Option<String>,
    description: Option<String>,
    keywords: Option<String>,
    reference_prefix: Option<String>,
    footprint_filters: Option<Vec<String>>,
    user_fields: Option<Vec<Json>>,
    remove_user_fields: Option<Vec<String>>,
    show_pin_numbers: Option<bool>,
    show_pin_names: Option<bool>,
    exclude_from_bom: Option<bool>,
    exclude_from_board: Option<bool>,
    exclude_from_sim: Option<bool>,
    manufacturer: Option<String>,
    mpn: Option<String>,
}

/// `symbol_set_properties(...)` as one undo step.
async fn set_properties(
    client: &mut Client,
    doc: &DynamicMessage,
    properties: Properties,
) -> Result<Json, Error> {
    // Designator/value alignment depends on their text, so re-place them when it changes
    let place = properties.value.is_some() || properties.reference_prefix.is_some();
    let mut request = with_field(
        build(
            &format!("{SC}SetLibrarySymbolProperties"),
            json!({"place_fields_around_body": place}),
        ),
        "document",
        doc.clone(),
    );
    // Optional fields: setting one, even to "" or false, marks it present, as setattr does
    for (name, setting) in [
        ("value", &properties.value),
        ("footprint", &properties.footprint),
        ("datasheet", &properties.datasheet),
        ("description", &properties.description),
        ("keywords", &properties.keywords),
        ("reference_prefix", &properties.reference_prefix),
    ] {
        if let Some(text) = setting {
            request.set_field_by_name(name, Value::String(text.clone()));
        }
    }
    for (name, setting) in [
        ("show_pin_numbers", properties.show_pin_numbers),
        ("show_pin_names", properties.show_pin_names),
        ("exclude_from_bom", properties.exclude_from_bom),
        ("exclude_from_board", properties.exclude_from_board),
        ("exclude_from_sim", properties.exclude_from_sim),
    ] {
        if let Some(flag) = setting {
            request.set_field_by_name(name, Value::Bool(flag));
        }
    }
    if let Some(filters) = &properties.footprint_filters {
        request.set_field_by_name("set_footprint_filters", Value::Bool(true));
        request.set_field_by_name(
            "footprint_filters",
            Value::List(filters.iter().cloned().map(Value::String).collect()),
        );
    }
    let mut fields: Vec<Json> = [
        (MANUFACTURER_FIELD, &properties.manufacturer),
        (MPN_FIELD, &properties.mpn),
    ]
    .into_iter()
    .filter_map(|(name, setting)| {
        setting
            .as_ref()
            .map(|value| json!({"name": name, "value": value, "visible": false}))
    })
    .collect();
    fields.extend(properties.user_fields.unwrap_or_default());
    let mut user_fields = Vec::with_capacity(fields.len());
    for field in &fields {
        // field["name"]: a KeyError whose text is the key's repr
        let name = field
            .get("name")
            .ok_or_else(|| Error::Value("'name'".into()))?;
        let value = py_str(field.get("value").unwrap_or(&json!("")));
        let visible = truthy(field.get("visible").unwrap_or(&json!(false)));
        let mut entry = json!({"value": value, "visible": visible});
        match name {
            Json::String(text) => entry["name"] = json!(text),
            // name=None leaves the field unset
            Json::Null => {}
            // What protobuf's add() raises for a value of the wrong type
            _ => {
                return Err(Error::Value(
                    "bad argument type for built-in operation".into(),
                ))
            }
        }
        user_fields.push(Value::Message(build(
            &format!("{SC}SymbolUserField"),
            entry,
        )));
    }
    request.set_field_by_name("user_fields", Value::List(user_fields));
    request.set_field_by_name(
        "remove_user_fields",
        Value::List(
            properties
                .remove_user_fields
                .unwrap_or_default()
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    let step = Step {
        request,
        response: EMPTY.into(),
        check: None,
    };
    transaction(client, doc, "MCP: symbol properties", &[step]).await?;
    Ok(json!({"success": true}))
}

/// `symbol_delete(doc, ids)`: deletes pins or graphics as one undo step.
async fn symbol_delete(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: &[String],
) -> Result<Json, Error> {
    let item_ids = ids
        .iter()
        .map(|id| kiid(id))
        .collect::<Result<Vec<_>, _>>()?;
    let request = with_field(
        build(&format!("{C}DeleteItems"), json!({"item_ids": item_ids})),
        "header",
        client::item_header(doc),
    );
    let step = Step {
        request,
        response: format!("{C}DeleteItemsResponse"),
        check: Some("DeleteItems"),
    };
    transaction(client, doc, "MCP: delete symbol items", &[step]).await?;
    Ok(json!({"deleted": ids.len()}))
}

/// `symbol_items(doc)`: the GetItemsResponse for the open symbol (its status is not checked).
async fn symbol_items(client: &mut Client, doc: &DynamicMessage) -> Result<DynamicMessage, Error> {
    let request = with_field(
        build(&format!("{C}GetItems"), json!({})),
        "header",
        client::item_header(doc),
    );
    client.send(&request, &format!("{C}GetItemsResponse")).await
}

/// `symbol_summary(doc)`: the open symbol's pins and graphic shape ids.
async fn symbol_summary(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let response = symbol_items(client, doc).await?;
    let mut pins = Vec::new();
    let mut shapes = Vec::new();
    for packed in messages(&response, "items") {
        if let Some(pin) = proto::unpack(&packed, &format!("{ST}SchematicPin")) {
            let mut entry = json!({
                "number": str_field(&pin, "number"),
                "name": str_field(&pin, "name"),
                "position_mm": mm(&message_field(&pin, "position")),
                "type": enum_name(&format!("{T}ElectricalPinType"), int_field(&pin, "electrical_type"))?,
                "orientation": enum_name(&format!("{ST}SchematicPinOrientation"), int_field(&pin, "orientation"))?,
                "id": str_field(&message_field(&pin, "id"), "value"),
            });
            if int_field(&pin, "visible") == 0 {
                entry["hidden"] = json!(true);
            }
            pins.push(entry);
        } else if proto::any_type_name(&packed).ends_with(".SchematicGraphicShape") {
            shapes.push(item_id(&packed).map_or(Json::Null, Json::String));
        }
    }
    Ok(json!({
        "lib_id": lib_id_text(&message_field(doc, "lib_id")),
        "pins": pins,
        "graphic_shape_ids": shapes,
    }))
}

/// `symbol_save(doc)`: writes the symbol to its library, then reloads the library.
async fn symbol_save(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let request = with_field(
        build(&format!("{C}SaveDocument"), json!({})),
        "document",
        doc.clone(),
    );
    client.send(&request, EMPTY).await?;
    let lib_id = message_field(doc, "lib_id");
    reload_library(client, &str_field(&lib_id, "library_nickname")).await?;
    Ok(json!({"saved": lib_id_text(&lib_id)}))
}

/// `<Enum>.Value(value)` for a pin key: Python raises ValueError for an unknown name (including
/// numbers and None, which are not names) and TypeError for an unhashable value; a missing key
/// (no `default`) is the KeyError `pin["key"]` raises.
fn pin_enum(
    enum_full_name: &str,
    pin: &Json,
    key: &str,
    default: Option<&str>,
) -> Result<i32, Error> {
    let descriptor = proto::pool()
        .get_enum_by_name(enum_full_name)
        .unwrap_or_else(|| panic!("{enum_full_name} is not in the KiCad protocol"));
    let value = match (pin.get(key), default) {
        (Some(value), _) => value.clone(),
        (None, Some(default)) => json!(default),
        (None, None) => return Err(Error::Value(format!("'{key}'"))),
    };
    match &value {
        Json::String(name) => {
            if let Some(found) = descriptor.get_value_by_name(name) {
                return Ok(found.number());
            }
        }
        Json::Array(_) => return Err(Error::Value("unhashable type: 'list'".into())),
        Json::Object(_) => return Err(Error::Value("unhashable type: 'dict'".into())),
        _ => {}
    }
    Err(Error::Value(format!(
        "Enum {} has no value defined for name {}",
        descriptor.name(),
        repr(&value)
    )))
}

/// The decimal digits of an integral f64, as Python prints int(value) (exact, however large).
fn exact_integer(value: f64) -> String {
    let negative = value < 0.0;
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let mut mantissa = bits & ((1 << 52) - 1);
    if exponent > 0 {
        mantissa |= 1 << 52;
    }
    let shift = exponent.max(1) - 1075;
    // Little-endian base-1e9 digits of the mantissa, then doubled `shift` times
    let mut limbs: Vec<u64> = vec![
        mantissa % 1_000_000_000,
        mantissa / 1_000_000_000 % 1_000_000_000,
        mantissa / 1_000_000_000_000_000_000,
    ];
    if shift < 0 {
        // Integral values below 2**53 fit the mantissa: divide exactly
        let whole = mantissa >> (-shift).min(63);
        limbs = vec![
            whole % 1_000_000_000,
            whole / 1_000_000_000 % 1_000_000_000,
            whole / 1_000_000_000_000_000_000,
        ];
    } else {
        for _ in 0..shift {
            let mut carry = 0;
            for limb in &mut limbs {
                let doubled = *limb * 2 + carry;
                *limb = doubled % 1_000_000_000;
                carry = doubled / 1_000_000_000;
            }
            if carry > 0 {
                limbs.push(carry);
            }
        }
    }
    while limbs.len() > 1 && limbs.last() == Some(&0) {
        limbs.pop();
    }
    let mut text = limbs.last().map(u64::to_string).unwrap_or_default();
    for limb in limbs.iter().rev().skip(1) {
        text.push_str(&format!("{limb:09}"));
    }
    if negative && text != "0" {
        text.insert(0, '-');
    }
    text
}

/// `message.length.value_nm = round(pin["length"] * 1_000_000)`, with Python's results for every
/// JSON value: a bool counts as 0 or 1, an int is exact, a float rounds half to even, a str or
/// list repeats and then cannot be rounded, None and dict cannot be multiplied, and protobuf
/// refuses a result outside int64.
fn pin_length_nm(pin: &Json) -> Result<i64, Error> {
    let out_of_range = |digits: String| Error::Value(format!("Value out of range: {digits}"));
    match pin.get("length") {
        None => Err(Error::Value("'length'".into())),
        Some(Json::Bool(flag)) => Ok(if *flag { 1_000_000 } else { 0 }),
        Some(Json::Number(number)) => {
            let text = number.to_string();
            if !text.contains(['.', 'e', 'E']) {
                // An int: exact, so the product is the digits with six zeros appended
                let digits = text.trim_start_matches('-');
                if digits.trim_start_matches('0').is_empty() {
                    return Ok(0);
                }
                let product = format!("{text}000000");
                return product.parse::<i64>().map_err(|_| out_of_range(product));
            }
            let rounded = (number.as_f64().unwrap_or(f64::NAN) * 1_000_000.0).round_ties_even();
            if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&rounded) {
                Ok(rounded as i64)
            } else {
                Err(out_of_range(exact_integer(rounded)))
            }
        }
        Some(Json::String(_)) => Err(Error::Value(
            "type str doesn't define __round__ method".into(),
        )),
        Some(Json::Array(_)) => Err(Error::Value(
            "type list doesn't define __round__ method".into(),
        )),
        Some(Json::Null) => Err(Error::Value(
            "unsupported operand type(s) for *: 'NoneType' and 'int'".into(),
        )),
        Some(Json::Object(_)) => Err(Error::Value(
            "unsupported operand type(s) for *: 'dict' and 'int'".into(),
        )),
    }
}

fn number_list(value: &Json) -> Vec<f64> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(Json::as_f64).collect())
        .unwrap_or_default()
}

/// `symbol_add_pins(doc, pins, draw_body, replace, arrange, group_gap)`.
async fn add_pins(client: &mut Client, doc: &DynamicMessage, args: &Args) -> Result<Json, Error> {
    let pins = args
        .dicts("pins")
        .map(|pins| {
            pins.iter()
                .filter_map(|pin| pin.as_object().cloned())
                .collect()
        })
        .unwrap_or_default();
    // layout_backend.layout with its default (Python) backend: layout_box's result, no info
    let layout = embeint_layout::layout_box(embeint_layout::LayoutRequest {
        pins,
        arrange: args.str("arrange").to_owned(),
        group_gap: clamped(args, "group_gap"),
    })
    .map_err(|error| Error::Value(error.message().to_owned()))?;
    let draw_body = args.bool("draw_body");
    if args.bool("replace") {
        let existing: Vec<DynamicMessage> = messages(&symbol_items(client, doc).await?, "items")
            .into_iter()
            .filter(|item| {
                let name = proto::any_type_name(item);
                name.ends_with(".SchematicPin") || name.ends_with(".SchematicGraphicShape")
            })
            .collect();
        if !existing.is_empty() {
            let ids: Vec<String> = existing.iter().filter_map(item_id).collect();
            symbol_delete(client, doc, &ids).await?;
        }
    }
    let body = number_list(&layout["body"]);
    let laid_out = layout["pins"].as_array().cloned().unwrap_or_default();
    let mut items = Vec::new();
    if draw_body {
        let [x1, y1, x2, y2] = <[f64; 4]>::try_from(body.clone())
            .map_err(|_| Error::Runtime("layout without a body".into()))?;
        // Stock KiCad symbol body: 0.254 mm default-style outline, filled with the body color
        let shape = json!({
            "shape": {
                "attributes": {
                    "stroke": {"width": {"value_nm": 254000}, "style": "SLS_DEFAULT"},
                    "fill": {"fill_type": "GFT_FILLED_WITH_BACKGROUND_BODY_COLOR"},
                },
                "rectangle": {"top_left": point(x1, y1)?, "bottom_right": point(x2, y2)?},
            },
        });
        items.push(build(&format!("{ST}SchematicGraphicShape"), shape));
    }
    for pin in &laid_out {
        let id = kiid("")?;
        let name = py_str(pin.get("name").unwrap_or(&json!("")));
        let position = number_list(&pin["position"]);
        let position = point(
            position.first().copied().unwrap_or(f64::NAN),
            position.get(1).copied().unwrap_or(f64::NAN),
        )?;
        // In Python's order: orientation, electrical type, shape, then the length
        let orientation = pin_enum(
            &format!("{ST}SchematicPinOrientation"),
            pin,
            "orientation",
            None,
        )?;
        let electrical_type = pin_enum(
            &format!("{T}ElectricalPinType"),
            pin,
            "electrical_type",
            None,
        )?;
        let shape = pin_enum(
            &format!("{ST}SchematicPinShape"),
            pin,
            "shape",
            Some("SPS_LINE"),
        )?;
        let length = pin_length_nm(pin)?;
        let message = json!({
            "id": id,
            "name": name,
            "number": pin["number"],
            "position": position,
            "length": {"value_nm": length},
            "orientation": orientation,
            "electrical_type": electrical_type,
            "shape": shape,
            "visible": !truthy(pin.get("hidden").unwrap_or(&Json::Null)),
            // 1.27 mm (50 mil) is KiCad's standard pin text; zero would hide names and numbers
            "name_text_size": {"value_nm": 1270000},
            "number_text_size": {"value_nm": 1270000},
        });
        items.push(pbjson::from_json(
            &message,
            &proto::descriptor(&format!("{ST}SchematicPin")),
        )?);
    }
    let mut request = with_field(
        build(&format!("{C}CreateItems"), json!({})),
        "header",
        client::item_header(doc),
    );
    request.set_field_by_name(
        "items",
        Value::List(
            items
                .iter()
                .map(|item| Value::Message(proto::pack(item)))
                .collect(),
        ),
    );
    let mut steps = vec![Step {
        request,
        response: format!("{C}CreateItemsResponse"),
        check: Some("CreateItems"),
    }];
    if draw_body {
        steps.push(Step {
            request: with_field(
                build(
                    &format!("{SC}SetLibrarySymbolProperties"),
                    json!({"place_fields_around_body": true}),
                ),
                "document",
                doc.clone(),
            ),
            response: EMPTY.into(),
            check: None,
        });
    }
    transaction(client, doc, "MCP: add symbol pins", &steps).await?;
    let pins: Vec<Json> = laid_out
        .iter()
        .map(|pin| {
            json!({
                "number": pin["number"],
                "name": pin.get("name").cloned().unwrap_or_else(|| json!("")),
                "side": pin["side"],
                "position_mm": number_list(&pin["position"]).into_iter().map(float).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({
        "body_mm": body.into_iter().map(float).collect::<Vec<_>>(),
        "pins": pins,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_reprs() {
        assert_eq!(repr_str("a'b"), "\"a'b\"");
        assert_eq!(repr_str("a\\b\n"), "'a\\\\b\\n'");
        assert_eq!(repr_str("x"), "'x'");
        assert_eq!(repr_float(1e-6), "1e-06");
        assert_eq!(repr_float(1e16), "1e+16");
        assert_eq!(repr_float(1.5e16), "1.5e+16");
        assert_eq!(repr_float(123.0), "123.0");
        assert_eq!(repr_float(0.0001), "0.0001");
        assert_eq!(repr_float(-2.54), "-2.54");
        assert_eq!(repr_float(1e15), "1000000000000000.0");
        assert_eq!(
            repr(&json!({"a": [1, 2.5, null, true]})),
            "{'a': [1, 2.5, None, True]}"
        );
        assert_eq!(py_str(&json!(1.0)), "1.0");
    }

    #[test]
    fn helpers_follow_python() {
        assert_eq!(nm(2.5e-6), 2);
        assert_eq!(nm(3.5e-6), 4);
        assert_eq!(nm(-7.62), -7_620_000);
        assert!(point(10000.1, 0.0).is_err());
        assert!(lib_identifier("Lib").is_err());
        assert!(lib_identifier(":R").is_err());
        assert!(lib_identifier("Lib:R:x").is_ok());
        let shape = |pin: Json| {
            pin_enum(
                &format!("{ST}SchematicPinShape"),
                &pin,
                "shape",
                Some("SPS_LINE"),
            )
            .map_err(|e| e.to_string())
        };
        assert_eq!(shape(json!({})), Ok(1));
        assert_eq!(
            shape(json!({"shape": "X"})),
            Err("Enum SchematicPinShape has no value defined for name 'X'".into())
        );
        assert_eq!(
            shape(json!({"shape": [1]})),
            Err("unhashable type: 'list'".into())
        );
        // Python's SchematicPinOrientation.Value() takes names only, and pin["orientation"] is
        // a KeyError when missing (texts from the MCP interpreter, protobuf 6)
        let orientation = |pin: Json| {
            pin_enum(
                &format!("{ST}SchematicPinOrientation"),
                &pin,
                "orientation",
                None,
            )
            .map_err(|e| e.to_string())
        };
        assert_eq!(orientation(json!({"orientation": "SPO_LEFT"})), Ok(2));
        for (value, text) in [
            (json!(2), "2"),
            (Json::Null, "None"),
            (json!("bogus"), "'bogus'"),
        ] {
            assert_eq!(
                orientation(json!({"orientation": value})),
                Err(format!(
                    "Enum SchematicPinOrientation has no value defined for name {text}"
                ))
            );
        }
        assert_eq!(orientation(json!({})), Err("'orientation'".into()));
        let length =
            |value: Json| pin_length_nm(&json!({"length": value})).map_err(|e| e.to_string());
        assert_eq!(length(json!(3.81)), Ok(3_810_000));
        assert_eq!(length(json!(true)), Ok(1_000_000));
        assert_eq!(length(json!(2.5e-7)), Ok(0));
        assert_eq!(length(json!(0)), Ok(0));
        assert_eq!(
            length(json!(9_223_372_036_854_i64)),
            Ok(9_223_372_036_854_000_000)
        );
        assert_eq!(
            length(json!(9_223_372_036_855_i64)),
            Err("Value out of range: 9223372036855000000".into())
        );
        assert_eq!(
            length(json!(10_000_000_000_000_i64)),
            Err("Value out of range: 10000000000000000000".into())
        );
        assert_eq!(
            length(json!(1e300)),
            Err(format!(
                "Value out of range: {}",
                "1000000000000000017216064596736454828831087825013238982328892017892380671244575047987920451875459594568606138861698291060311049225532948520696938805711440650122628514669428460356992624968028329550689224175284346730060716088829214255439694630119794546505512415617982143262670862918816362862119154749127262208"
            ))
        );
        assert_eq!(
            length(json!("3.81")),
            Err("type str doesn't define __round__ method".into())
        );
        assert_eq!(
            length(json!([1])),
            Err("type list doesn't define __round__ method".into())
        );
        assert_eq!(
            length(Json::Null),
            Err("unsupported operand type(s) for *: 'NoneType' and 'int'".into())
        );
        assert_eq!(
            length(json!({"a": 1})),
            Err("unsupported operand type(s) for *: 'dict' and 'int'".into())
        );
        assert_eq!(exact_integer(9_007_199_254_740_993.0), "9007199254740992");
        assert_eq!(exact_integer(-1e20), "-100000000000000000000");
        assert_eq!(exact_integer(123.0), "123");
        assert_eq!(
            enum_name("kiapi.common.types.ElectricalPinType", 99)
                .unwrap_err()
                .to_string(),
            "Enum ElectricalPinType has no name defined for value 99"
        );
        assert_eq!(scope_value("x").unwrap_err().to_string(), "'x'");
        assert!(!truthy(&json!(0.0)) && truthy(&json!("x")) && !truthy(&json!({})));
    }
}
