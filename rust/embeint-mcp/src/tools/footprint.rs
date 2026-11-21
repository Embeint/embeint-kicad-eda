// SPDX-License-Identifier: GPL-3.0-or-later
//! The Footprint Editor, footprint generation, embedded files, 3D model plans and checks, and
//! `live_kicad_cli`: a port of the footprint tools of server.py and their methods in
//! `footprint_client.py` (geometry in [`crate::footprint_gen`], models in
//! [`crate::model_handoff`], kicad-cli in [`crate::kicad_cli`]).
//!
//! See [`super`] for the module contract.  None of these tools is wrapped in server.py's
//! `guarded(...)`: their edits run as transactions with generated operation ids.  The library
//! table tools belong to the symbol area.

use std::collections::BTreeMap;
use std::path::Path;

use embeint_ipc::client::{self, item_header, reply_json, Client, Error};
use embeint_ipc::pbjson;
use embeint_ipc::proto::{
    self, enum_name, int_field, list_field, message_field, str_field, with_field,
};
use prost_reflect::{DynamicMessage, MapKey, Value};
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{Ctx, Output};
use crate::footprint_gen::{self, py_round, py_str, Num, Pad, Shape, TextSpec};
use crate::{kicad_cli, model_handoff};

const B: &str = "kiapi.board.commands.";
const BT: &str = "kiapi.board.types.";
const C: &str = "kiapi.common.commands.";
const T: &str = "kiapi.common.types.";
const EMPTY: &str = "google.protobuf.Empty";

pub fn params(tool: &str) -> Option<&'static [Param]> {
    const FOOTPRINT: &[Param] = sig![req("footprint_document", Arg::Dict)];
    Some(match tool {
        "live_kicad_cli" => sig![
            req("args", Arg::ListStr),
            opt("working_directory", Arg::OptStr, Default::None),
            opt("timeout_s", Arg::Float, Default::Int(300)),
        ],
        "live_footprint_search" => sig![
            req("query", Arg::Str),
            opt("limit", Arg::Int, Default::Int(25)),
        ],
        "live_footprint_get" | "live_footprint_edit" => sig![req("lib_id", Arg::Str)],
        "live_footprint_new" => sig![
            req("library", Arg::Str),
            req("name", Arg::Str),
            opt("mounting", Arg::Str, Default::Str("smd")),
            opt("replace_existing", Arg::Bool, Default::Bool(false)),
        ],
        "live_footprint_generate" => sig![
            req("footprint_document", Arg::Dict),
            req("package", Arg::Str),
            req("params", Arg::Dict),
            opt("replace", Arg::Bool, Default::Bool(false)),
        ],
        "live_footprint_set_properties" => sig![
            req("footprint_document", Arg::Dict),
            opt("description", Arg::OptStr, Default::None),
            opt("keywords", Arg::OptStr, Default::None),
            opt("mounting", Arg::OptStr, Default::None),
            opt("value", Arg::OptStr, Default::None),
            opt("datasheet", Arg::OptStr, Default::None),
            opt("exclude_from_bom", Arg::OptBool, Default::None),
            opt("exclude_from_position_files", Arg::OptBool, Default::None),
        ],
        "live_footprint_attach_step" => sig![
            req("footprint_document", Arg::Dict),
            req("step_path", Arg::Str),
            opt("offset_mm", Arg::OptListFloat, Default::None),
            opt("rotation_deg", Arg::OptListFloat, Default::None),
        ],
        "live_embedded_files" => sig![req("document_spec", Arg::Dict)],
        "live_embed_file" => sig![
            req("document_spec", Arg::Dict),
            req("path", Arg::Str),
            opt("overwrite", Arg::Bool, Default::Bool(false)),
        ],
        "live_remove_embedded_file" => sig![req("document_spec", Arg::Dict), req("name", Arg::Str)],
        "live_footprint_view" => sig![
            req("footprint_document", Arg::Dict),
            opt("dpi", Arg::Int, Default::Int(1500)),
        ],
        "live_footprint_summary" | "live_footprint_save" => FOOTPRINT,
        "live_footprint_model_plan" => sig![
            req("package", Arg::Str),
            req("params", Arg::Dict),
            req("body_height", Arg::Float),
            opt("standoff", Arg::Float, Default::Float(0.1)),
            opt("lead_thickness", Arg::Float, Default::Float(0.2)),
            opt("lead_width", Arg::OptFloat, Default::None),
            opt("lead_span", Arg::OptFloat, Default::None),
            opt("foot_length", Arg::OptFloat, Default::None),
            opt("export_path", Arg::OptStr, Default::None),
        ],
        "live_footprint_view_3d" => sig![
            req("lib_id", Arg::Str),
            opt("rotate", Arg::Str, Default::Str("-45,0,45")),
            opt("zoom", Arg::Float, Default::Float(1.0)),
        ],
        "live_footprint_check_model" => sig![
            req("lib_id", Arg::Str),
            opt("source_path", Arg::OptStr, Default::None),
        ],
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(run_tool(name, args, ctx).await)
}

async fn run_tool(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<Output, Error> {
    let footprint = || client::footprint_document(args.get("footprint_document"));
    let any_editor = || client::any_editor_document(args.get("document_spec"));
    let empty = Map::new();
    let params = || args.dict("params").unwrap_or(&empty);
    Ok(match name {
        "live_footprint_search" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(footprint_search(&mut client, args.str("query"), args.get("limit")).await?)
        }
        "live_footprint_get" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(library_footprint(&mut client, args.str("lib_id")).await?)
        }
        "live_footprint_new" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(
                new_footprint(
                    &mut client,
                    args.str("library"),
                    args.str("name"),
                    args.str("mounting"),
                    args.bool("replace_existing"),
                )
                .await?,
            )
        }
        "live_footprint_edit" => {
            let mut client = ctx.client.lock().await;
            Output::Dict(edit_footprint(&mut client, args.str("lib_id")).await?)
        }
        "live_footprint_generate" => {
            let doc = footprint()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(
                footprint_generate(
                    &mut client,
                    &doc,
                    args.str("package"),
                    params(),
                    args.bool("replace"),
                )
                .await?,
            )
        }
        "live_footprint_set_properties" => {
            let doc = footprint()?;
            let settings = Properties {
                description: args.opt_str("description"),
                keywords: args.opt_str("keywords"),
                mounting: args.opt_str("mounting"),
                value: args.opt_str("value"),
                datasheet: args.opt_str("datasheet"),
                exclude_from_bom: args.opt_bool("exclude_from_bom"),
                exclude_from_position_files: args.opt_bool("exclude_from_position_files"),
                models: None,
            };
            let mut client = ctx.client.lock().await;
            Output::Dict(footprint_set_properties(&mut client, &doc, settings).await?)
        }
        "live_footprint_attach_step" => {
            let doc = footprint()?;
            // tuple(offset_mm or (0, 0, 0)): an empty list takes the default too
            let vector = |key: &str| -> Vec<Json> {
                match args.get(key) {
                    Json::Array(items) if !items.is_empty() => items.clone(),
                    _ => vec![json!(0), json!(0), json!(0)],
                }
            };
            let mut client = ctx.client.lock().await;
            Output::Dict(
                footprint_attach_step(
                    &mut client,
                    &doc,
                    args.str("step_path"),
                    vector("offset_mm"),
                    vector("rotation_deg"),
                )
                .await?,
            )
        }
        "live_embedded_files" => {
            let doc = any_editor()?;
            let mut client = ctx.client.lock().await;
            Output::List(embedded_files(&mut client, &doc).await?)
        }
        "live_embed_file" => {
            let doc = any_editor()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(
                embed_file(&mut client, &doc, args.str("path"), args.bool("overwrite")).await?,
            )
        }
        "live_remove_embedded_file" => {
            let doc = any_editor()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(remove_embedded_file(&mut client, &doc, args.str("name")).await?)
        }
        "live_footprint_view" => {
            let doc = footprint()?;
            let mut client = ctx.client.lock().await;
            Output::png(footprint_render(&mut client, &doc, args.get("dpi")).await?)
        }
        "live_footprint_summary" => {
            let doc = footprint()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(footprint_summary(&mut client, &doc).await?)
        }
        "live_footprint_save" => {
            let doc = footprint()?;
            let mut client = ctx.client.lock().await;
            Output::Dict(footprint_save(&mut client, &doc).await?)
        }
        "live_footprint_model_plan" => Output::Dict(footprint_model_plan(args, params()).await?),
        "live_footprint_view_3d" => {
            let lib_id = args.str("lib_id");
            // KiCad is asked first; the render runs without holding the connection
            let ((path, env, _), text) = {
                let mut client = ctx.client.lock().await;
                let launch = kicad_cli_launch(&mut client).await?;
                let text = saved_footprint_text(&mut client, lib_id).await?;
                (launch, text)
            };
            Output::png(
                model_handoff::render_footprint_3d(
                    &path,
                    &text,
                    lib_id,
                    args.str("rotate"),
                    args.get("zoom"),
                    &env,
                )
                .await?,
            )
        }
        "live_footprint_check_model" => Output::Dict(
            footprint_check_model(ctx, args.str("lib_id"), args.opt_str("source_path")).await?,
        ),
        "live_kicad_cli" => {
            let cli_args = args.strs("args");
            // The allow-list is checked before KiCad is asked anything
            kicad_cli::cli_arguments(&cli_args)?;
            let (path, env, description) = {
                let mut client = ctx.client.lock().await;
                kicad_cli_launch(&mut client).await?
            };
            let mut result = kicad_cli::run(
                &path,
                &cli_args,
                &env,
                args.opt_str("working_directory").as_deref(),
                args.get("timeout_s"),
            )
            .await?;
            result.insert("environment".into(), Json::Object(description));
            Output::Dict(Json::Object(result))
        }
        _ => unreachable!("validated tool name"),
    })
}

// ---- Messages ------------------------------------------------------------------------------

/// A request or item built from protobuf JSON this module composes (values from arguments
/// included, so a failure is reported, not a panic).
fn message(full_name: &str, value: Json) -> Result<DynamicMessage, Error> {
    Ok(pbjson::from_json(&value, &proto::descriptor(full_name))?)
}

fn with_document(
    full_name: &str,
    value: Json,
    doc: &DynamicMessage,
) -> Result<DynamicMessage, Error> {
    Ok(with_field(
        message(full_name, value)?,
        "document",
        doc.clone(),
    ))
}

fn with_header(
    full_name: &str,
    value: Json,
    doc: &DynamicMessage,
) -> Result<DynamicMessage, Error> {
    Ok(with_field(
        message(full_name, value)?,
        "header",
        item_header(doc),
    ))
}

fn set_items(request: &mut DynamicMessage, items: &[DynamicMessage]) {
    request.set_field_by_name(
        "items",
        Value::List(
            items
                .iter()
                .map(|i| Value::Message(proto::pack(i)))
                .collect(),
        ),
    );
}

/// A nested message field: `message.a.b.c`.
fn nested(message: &DynamicMessage, path: &[&str]) -> DynamicMessage {
    path.iter().fold(message.clone(), |current, name| {
        message_field(&current, name)
    })
}

fn messages(values: Vec<Value>) -> Vec<DynamicMessage> {
    values
        .into_iter()
        .filter_map(|value| match value {
            Value::Message(message) => Some(message),
            _ => None,
        })
        .collect()
}

/// `_mm(vector)`: a Vector2 in millimetres.
fn mm(vector: &DynamicMessage) -> Json {
    let value = |field: &str| Num::Float(int_field(vector, field) as f64 / 1e6).to_json();
    json!([value("x_nm"), value("y_nm")])
}

/// `_vec(x, y)` as protobuf JSON.
fn vec2(x: Num, y: Num) -> Json {
    json!({"x_nm": x.nm(), "y_nm": y.nm()})
}

fn vec2f(point: (f64, f64)) -> Json {
    vec2(Num::Float(point.0), Num::Float(point.1))
}

fn kiid() -> Json {
    json!({"value": uuid::Uuid::new_v4().to_string()})
}

/// `LAYERS[name]`.
fn layer(name: &str) -> &'static str {
    match name {
        "F.Cu" => "BL_F_Cu",
        "B.Cu" => "BL_B_Cu",
        "F.SilkS" => "BL_F_SilkS",
        "F.Fab" => "BL_F_Fab",
        "F.CrtYd" => "BL_F_CrtYd",
        "F.Mask" => "BL_F_Mask",
        "B.Mask" => "BL_B_Mask",
        "F.Paste" => "BL_F_Paste",
        "B.SilkS" => "BL_B_SilkS",
        "B.Fab" => "BL_B_Fab",
        _ => "BL_B_CrtYd",
    }
}

/// `MOUNTING[mounting]`: an unknown name raises KeyError, whose text is the key's repr.
fn mounting_style(mounting: &str) -> Result<&'static str, Error> {
    match mounting {
        "smd" => Ok("FMS_SMD"),
        "tht" => Ok("FMS_THROUGH_HOLE"),
        "unspecified" => Ok("FMS_UNSPECIFIED"),
        other => Err(Error::Runtime(footprint_gen::py_str_repr(other))),
    }
}

/// `ALL_COPPER`: every copper layer in declaration order.
fn all_copper() -> Vec<Json> {
    proto::pool()
        .get_enum_by_name(&format!("{BT}BoardLayer"))
        .map(|e| {
            e.values()
                .map(|v| v.name().to_owned())
                .filter(|n| n.starts_with("BL_") && n.ends_with("_Cu"))
                .map(Json::String)
                .collect()
        })
        .unwrap_or_default()
}

/// `set_text_style`: KLC text, square glyphs, stroke 15 % of the height, centred.
fn text_attributes(size: f64) -> Json {
    json!({
        "size": vec2f((size, size)),
        "stroke_width": {"value_nm": Num::Float(py_round(size * 0.15, 3)).nm()},
        "horizontal_alignment": "HA_CENTER",
        "vertical_alignment": "VA_CENTER",
        "visible": true,
    })
}

/// `make_pad`: a Pad from footprint_gen's pad.
fn make_pad(pad: &Pad) -> Result<DynamicMessage, Error> {
    let (width, height) = pad.size;
    let mut stack = json!({"type": "PST_NORMAL"});
    let kind = if pad.kind == "smd" {
        stack["layers"] = json!(["BL_F_Cu", "BL_F_Paste", "BL_F_Mask"]);
        "PT_SMD"
    } else {
        let mut layers = all_copper();
        layers.extend([json!("BL_F_Mask"), json!("BL_B_Mask")]);
        stack["layers"] = Json::Array(layers);
        let drill = footprint_gen::num(pad.drill.as_ref().unwrap_or(&Json::Null))?;
        stack["drill"] = json!({"diameter": vec2(drill, drill), "shape": "DS_CIRCLE",
                                "start_layer": "BL_F_Cu", "end_layer": "BL_B_Cu"});
        if pad.kind == "pth" {
            "PT_PTH"
        } else {
            "PT_NPTH"
        }
    };
    let shape = match pad.shape {
        "rect" => "PSS_RECTANGLE",
        "roundrect" => "PSS_ROUNDRECT",
        "circle" => "PSS_CIRCLE",
        _ => "PSS_OVAL",
    };
    let mut copper = json!({"layer": "BL_F_Cu", "shape": shape, "size": vec2(width, height)});
    if pad.shape == "roundrect" {
        // KLC: 25 % corner ratio, radius at most 0.25 mm
        let ratio = Num::Float(0.25).div(footprint_gen::py_min(width, height))?;
        copper["corner_rounding_ratio"] = footprint_gen::py_min(Num::Float(0.25), ratio).to_json();
    }
    stack["copper_layers"] = json!([copper]);
    message(
        &format!("{BT}Pad"),
        json!({"id": kiid(), "number": pad.number, "position": vec2(pad.position.0, pad.position.1),
               "type": kind, "pad_stack": stack}),
    )
}

fn stroke(width: f64) -> Json {
    json!({"width": {"value_nm": Num::Float(width).nm()}, "style": "SLS_SOLID"})
}

fn make_line(
    line: &footprint_gen::Line,
    layer_name: &str,
    width: f64,
) -> Result<DynamicMessage, Error> {
    message(
        &format!("{BT}BoardGraphicShape"),
        json!({"id": kiid(), "layer": layer(layer_name), "shape": {
            "segment": {"start": vec2f(line[0]), "end": vec2f(line[1])},
            "attributes": {"stroke": stroke(width)}}}),
    )
}

fn make_rect(rect: &[f64; 4], layer_name: &str, width: f64) -> Result<DynamicMessage, Error> {
    message(
        &format!("{BT}BoardGraphicShape"),
        json!({"id": kiid(), "layer": layer(layer_name), "shape": {
            "rectangle": {"top_left": vec2f((rect[0], rect[1])), "bottom_right": vec2f((rect[2], rect[3]))},
            "attributes": {"stroke": stroke(width), "fill": {"fill_type": "GFT_UNFILLED"}}}}),
    )
}

fn make_polygon(
    points: &[(Num, Num)],
    layer_name: &str,
    width: f64,
) -> Result<DynamicMessage, Error> {
    let nodes: Vec<Json> = points
        .iter()
        .map(|(x, y)| json!({"point": vec2(*x, *y)}))
        .collect();
    message(
        &format!("{BT}BoardGraphicShape"),
        json!({"id": kiid(), "layer": layer(layer_name), "shape": {
            "polygon": {"polygons": [{"outline": {"nodes": nodes, "closed": true}}]},
            "attributes": {"stroke": stroke(width), "fill": {"fill_type": "GFT_FILLED"}}}}),
    )
}

fn make_text(content: &str, spec: &TextSpec, layer_name: &str) -> Result<DynamicMessage, Error> {
    message(
        &format!("{BT}BoardText"),
        json!({"id": kiid(), "layer": layer(layer_name), "text": {
            "text": content, "position": vec2f(spec.position), "attributes": text_attributes(spec.size)}}),
    )
}

/// `generated_items`: every board item for a shape (fields are positioned separately).
fn generated_items(shape: &Shape, fab_reference: bool) -> Result<Vec<DynamicMessage>, Error> {
    use footprint_gen::{COURTYARD_WIDTH, FAB_WIDTH, SILK_WIDTH};
    let mut items = shape
        .pads
        .iter()
        .map(make_pad)
        .collect::<Result<Vec<_>, _>>()?;
    for line in &shape.fab_lines {
        items.push(make_line(line, "F.Fab", FAB_WIDTH)?);
    }
    for line in &shape.silk_lines {
        items.push(make_line(line, "F.SilkS", SILK_WIDTH)?);
    }
    if let Some(marker) = shape.pin1_marker.as_ref().filter(|m| !m.is_empty()) {
        items.push(make_polygon(marker, "F.SilkS", SILK_WIDTH)?);
    }
    items.push(make_rect(&shape.courtyard, "F.CrtYd", COURTYARD_WIDTH)?);
    if fab_reference {
        items.push(make_text("${REFERENCE}", &shape.fab_reference, "F.Fab")?);
    }
    Ok(items)
}

// ---- Libraries and the Footprint Editor ------------------------------------------------------

/// `open_footprint_editor`: the Footprint Editor lives in pcbnew; the schematic editor opens it.
async fn open_footprint_editor(client: &mut Client) -> Result<(), Error> {
    let request = message("kiapi.schematic.commands.OpenFootprintEditor", json!({}))?;
    client.send(&request, EMPTY).await?;
    Ok(())
}

async fn footprint_search(client: &mut Client, query: &str, limit: &Json) -> Result<Json, Error> {
    open_footprint_editor(client).await?;
    let request = message(
        &format!("{C}SearchLibraries"),
        json!({"type": "LT_FOOTPRINT", "query": query}),
    )?;
    let response = client
        .send(&request, &format!("{C}SearchLibrariesResponse"))
        .await?;
    let ids: Vec<String> = messages(list_field(&response, "items"))
        .iter()
        .map(|i| {
            format!(
                "{}:{}",
                str_field(i, "library_nickname"),
                str_field(i, "entry_name")
            )
        })
        .collect();
    // ids[:max(1, min(limit, 200))]; a Python int beyond i64 is clamped by its sign
    let limit = limit
        .as_i64()
        .unwrap_or(if limit.as_f64().unwrap_or(0.0) > 0.0 {
            i64::MAX
        } else {
            i64::MIN
        });
    let count = limit.clamp(1, 200) as usize;
    let mut result = json!({
        "total": ids.len(),
        "results": ids.iter().take(count).map(|i| json!({"lib_id": i})).collect::<Vec<_>>(),
    });
    let loading = int_field(&response, "libraries_loading");
    if loading != 0 {
        result["libraries_still_loading"] = json!(loading);
    }
    Ok(result)
}

/// `lib_id.partition(":")`: (nickname, name), no validation.
fn partition(lib_id: &str) -> (&str, &str) {
    lib_id.split_once(':').unwrap_or((lib_id, ""))
}

async fn library_footprint(client: &mut Client, lib_id: &str) -> Result<Json, Error> {
    open_footprint_editor(client).await?;
    let (nickname, name) = partition(lib_id);
    let request = message(
        &format!("{C}GetItemsFromLibrary"),
        json!({"type": "LT_FOOTPRINT", "item_ids": [{"library_nickname": nickname, "entry_name": name}]}),
    )?;
    let response = client
        .send(&request, &format!("{C}GetItemsResponse"))
        .await?;
    let footprint = messages(list_field(&response, "items"))
        .first()
        .and_then(|any| proto::unpack(any, &format!("{BT}Footprint")))
        .ok_or_else(|| Error::Value(format!("{lib_id} not found")))?;
    Ok(summarize_footprint(&footprint))
}

/// The first copper layer of a pad's stack, if any.
fn first_copper(pad: &DynamicMessage) -> Option<DynamicMessage> {
    messages(list_field(
        &message_field(pad, "pad_stack"),
        "copper_layers",
    ))
    .into_iter()
    .next()
}

fn summarize_footprint(footprint: &DynamicMessage) -> Json {
    let mut pads = Vec::new();
    let mut models = Vec::new();
    for packed in messages(list_field(footprint, "items")) {
        if let Some(pad) = proto::unpack(&packed, &format!("{BT}Pad")) {
            let copper = first_copper(&pad);
            pads.push(json!({
                "number": str_field(&pad, "number"),
                "type": enum_name(&format!("{BT}PadType"), int_field(&pad, "type")),
                "position_mm": mm(&message_field(&pad, "position")),
                "size_mm": copper.map(|c| mm(&message_field(&c, "size"))),
            }));
        } else if let Some(model) = proto::unpack(&packed, &format!("{BT}Footprint3DModel")) {
            models.push(json!(str_field(&model, "filename")));
        }
    }
    // sorted(pads, key=lambda p: (len(p["number"]), p["number"])): characters, then code points
    pads.sort_by(|a, b| {
        let key = |p: &Json| {
            let number = p["number"].as_str().unwrap_or_default().to_owned();
            (number.chars().count(), number)
        };
        key(a).cmp(&key(b))
    });
    let attributes = message_field(footprint, "attributes");
    let id = message_field(footprint, "id");
    let field_text = |name: &str| str_field(&nested(footprint, &[name, "text", "text"]), "text");
    json!({
        "lib_id": format!("{}:{}", str_field(&id, "library_nickname"), str_field(&id, "entry_name")),
        "description": str_field(&attributes, "description"),
        "keywords": str_field(&attributes, "keywords"),
        "mounting": enum_name(&format!("{BT}FootprintMountingStyle"), int_field(&attributes, "mounting_style")),
        "value": field_text("value_field"),
        "datasheet": field_text("datasheet_field"),
        "models": models,
        "pads": pads,
    })
}

async fn new_footprint(
    client: &mut Client,
    library: &str,
    name: &str,
    mounting: &str,
    replace_existing: bool,
) -> Result<Json, Error> {
    open_footprint_editor(client).await?;
    // MOUNTING[mounting] is looked up after the editor was asked to open, as in Python
    let request = message(
        &format!("{B}NewLibraryFootprint"),
        json!({"library_nickname": library, "name": name, "mounting_style": mounting_style(mounting)?,
               "replace_existing": replace_existing}),
    )?;
    let response = client
        .send(&request, &format!("{B}LibraryFootprintResponse"))
        .await?;
    reply_json(&message_field(&response, "document"))
}

async fn edit_footprint(client: &mut Client, lib_id: &str) -> Result<Json, Error> {
    open_footprint_editor(client).await?;
    let (nickname, name) = partition(lib_id);
    let request = message(
        &format!("{B}EditLibraryFootprint"),
        json!({"id": {"library_nickname": nickname, "entry_name": name}}),
    )?;
    let response = client
        .send(&request, &format!("{B}LibraryFootprintResponse"))
        .await?;
    reply_json(&message_field(&response, "document"))
}

/// `footprint_items(doc)`: the open footprint's items (no status check, as in Python).
async fn footprint_items(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Vec<DynamicMessage>, Error> {
    let request = with_header(&format!("{C}GetItems"), json!({}), doc)?;
    let response = client
        .send(&request, &format!("{C}GetItemsResponse"))
        .await?;
    Ok(messages(list_field(&response, "items")))
}

/// `_item_id`: the id of a packed pad, graphic or text, or None.
fn item_id(packed: &DynamicMessage) -> Option<String> {
    ["Pad", "BoardGraphicShape", "BoardText"]
        .iter()
        .find_map(|name| proto::unpack(packed, &format!("{BT}{name}")))
        .map(|item| str_field(&message_field(&item, "id"), "value"))
        .filter(|id| !id.is_empty())
}

/// `footprint_generate`: a KLC-style footprint from package dimensions, as one undo step.
async fn footprint_generate(
    client: &mut Client,
    doc: &DynamicMessage,
    package: &str,
    params: &Map<String, Json>,
    replace: bool,
) -> Result<Json, Error> {
    let shape = footprint_gen::generate(package, params)?;
    let header = item_header(doc);
    let description = format!("MCP: generate {}", py_str(&shape.name));
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let outcome = generate_steps(client, doc, &shape, replace).await;
    client
        .finish_transaction(&header, &id, &operation_id, &description, outcome)
        .await?;
    let floats = |values: &[f64; 4]| -> Json {
        Json::Array(values.iter().map(|v| Num::Float(*v).to_json()).collect())
    };
    Ok(json!({
        "name": shape.name,
        "pads": shape.pads.len(),
        "courtyard_mm": floats(&shape.courtyard),
        "body_mm": floats(&shape.body),
    }))
}

async fn generate_steps(
    client: &mut Client,
    doc: &DynamicMessage,
    shape: &Shape,
    replace: bool,
) -> Result<(), Error> {
    if replace {
        let existing: Vec<DynamicMessage> = footprint_items(client, doc)
            .await?
            .into_iter()
            .filter(|item| !proto::any_type_name(item).ends_with(".Field"))
            .collect();
        if !existing.is_empty() {
            let ids: Vec<Json> = existing
                .iter()
                .filter_map(item_id)
                .map(|id| json!({"value": id}))
                .collect();
            let request = with_header(&format!("{C}DeleteItems"), json!({"item_ids": ids}), doc)?;
            client
                .send(&request, &format!("{C}DeleteItemsResponse"))
                .await?;
        }
    }
    // New footprints already carry KiCad's default ${REFERENCE} fab text: move it rather than
    // adding a second one
    let reused = place_fab_reference(client, doc, shape).await?;
    let items = generated_items(shape, !reused)?;
    let mut create = with_header(&format!("{C}CreateItems"), json!({}), doc)?;
    set_items(&mut create, &items);
    let response = client
        .send(&create, &format!("{C}CreateItemsResponse"))
        .await?;
    let rejected: Vec<String> = messages(list_field(&response, "created_items"))
        .iter()
        .map(|result| message_field(result, "status"))
        .filter(|status| int_field(status, "code") != 1)
        .map(|status| {
            let text = str_field(&status, "error_message");
            if text.is_empty() {
                enum_name(&format!("{C}ItemStatusCode"), int_field(&status, "code"))
            } else {
                text
            }
        })
        .collect();
    if !rejected.is_empty() {
        return Err(Error::Runtime(rejected.join("; ")));
    }
    place_fields(client, doc, shape).await?;
    let request = with_document(
        &format!("{B}SetLibraryFootprintProperties"),
        json!({"mounting_style": mounting_style(shape.mounting)?}),
        doc,
    )?;
    client.send(&request, EMPTY).await?;
    Ok(())
}

/// `_place_fab_reference`: moves existing `${REFERENCE}` texts to the body centre; true if there
/// was one.
async fn place_fab_reference(
    client: &mut Client,
    doc: &DynamicMessage,
    shape: &Shape,
) -> Result<bool, Error> {
    let spec = &shape.fab_reference;
    let mut texts = Vec::new();
    for packed in footprint_items(client, doc).await? {
        let Some(mut text) = proto::unpack(&packed, &format!("{BT}BoardText")) else {
            continue;
        };
        if str_field(&message_field(&text, "text"), "text") != "${REFERENCE}" {
            continue;
        }
        let patch = json!({"text": {"position": vec2f(spec.position), "attributes": text_attributes(spec.size)},
                           "layer": "BL_F_Fab"});
        pbjson::merge_json(&patch, &mut text)?;
        texts.push(text);
    }
    if texts.is_empty() {
        return Ok(false);
    }
    let mut update = with_header(&format!("{C}UpdateItems"), json!({}), doc)?;
    set_items(&mut update, &texts);
    client
        .send(&update, &format!("{C}UpdateItemsResponse"))
        .await?;
    Ok(true)
}

/// `_place_fields`: Reference on F.SilkS above the courtyard, Value on F.Fab below it (KLC).
async fn place_fields(
    client: &mut Client,
    doc: &DynamicMessage,
    shape: &Shape,
) -> Result<(), Error> {
    let mut fields = Vec::new();
    for packed in footprint_items(client, doc).await? {
        let Some(mut field) = proto::unpack(&packed, &format!("{BT}Field")) else {
            continue;
        };
        let spec = match str_field(&field, "name").as_str() {
            "Reference" => &shape.reference,
            "Value" => &shape.value,
            _ => continue,
        };
        let patch = json!({
            "text": {"text": {"position": vec2f(spec.position), "attributes": text_attributes(spec.size)},
                     "layer": layer(spec.layer)},
            "visible": true,
        });
        pbjson::merge_json(&patch, &mut field)?;
        fields.push(field);
    }
    if !fields.is_empty() {
        let mut update = with_header(&format!("{C}UpdateItems"), json!({}), doc)?;
        set_items(&mut update, &fields);
        client
            .send(&update, &format!("{C}UpdateItemsResponse"))
            .await?;
    }
    Ok(())
}

/// One 3D model for `footprint_set_properties(models=...)`: offset, rotation and scale as given
/// (three values each).
struct Model {
    filename: String,
    offset: Vec<Json>,
    rotation: Vec<Json>,
    scale: Vec<Json>,
}

/// The settings `footprint_set_properties` takes; None leaves a property as it is.
struct Properties {
    description: Option<String>,
    keywords: Option<String>,
    mounting: Option<String>,
    value: Option<String>,
    datasheet: Option<String>,
    exclude_from_bom: Option<bool>,
    exclude_from_position_files: Option<bool>,
    models: Option<Vec<Model>>,
}

/// `x, y, z = values`: Python 3.12's unpacking errors for any other length.
fn three(values: &[Json]) -> Result<Json, Error> {
    match values.len() {
        3 => Ok(json!({"x_nm": values[0], "y_nm": values[1], "z_nm": values[2]})),
        n if n < 3 => Err(Error::Value(format!(
            "not enough values to unpack (expected 3, got {n})"
        ))),
        _ => Err(Error::Value(
            "too many values to unpack (expected 3)".into(),
        )),
    }
}

async fn footprint_set_properties(
    client: &mut Client,
    doc: &DynamicMessage,
    settings: Properties,
) -> Result<Json, Error> {
    let mut request = Map::new();
    for (name, setting) in [
        ("description", settings.description.map(Json::String)),
        ("keywords", settings.keywords.map(Json::String)),
        ("value", settings.value.map(Json::String)),
        ("datasheet", settings.datasheet.map(Json::String)),
        (
            "exclude_from_bill_of_materials",
            settings.exclude_from_bom.map(Json::Bool),
        ),
        (
            "exclude_from_position_files",
            settings.exclude_from_position_files.map(Json::Bool),
        ),
    ] {
        if let Some(setting) = setting {
            request.insert(name.into(), setting);
        }
    }
    if let Some(mounting) = settings.mounting {
        request.insert("mounting_style".into(), json!(mounting_style(&mounting)?));
    }
    if let Some(models) = settings.models {
        request.insert("set_models".into(), json!(true));
        let mut entries = Vec::new();
        for model in models {
            // Vector3D carries raw values here: mm for offset, degrees, plain factors
            entries.push(json!({
                "filename": model.filename, "visible": true, "opacity": 1.0,
                "offset": three(&model.offset)?, "rotation": three(&model.rotation)?,
                "scale": three(&model.scale)?,
            }));
        }
        request.insert("models".into(), Json::Array(entries));
    }
    let request = with_document(
        &format!("{B}SetLibraryFootprintProperties"),
        Json::Object(request),
        doc,
    )?;
    let header = item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let outcome = client.send(&request, EMPTY).await.map(|_| ());
    client
        .finish_transaction(
            &header,
            &id,
            &operation_id,
            "MCP: footprint properties",
            outcome,
        )
        .await?;
    Ok(json!({"success": true}))
}

async fn footprint_render(
    client: &mut Client,
    doc: &DynamicMessage,
    dpi: &Json,
) -> Result<Vec<u8>, Error> {
    // An int32 field: protobuf refuses a larger value when the request is built
    let dpi = dpi
        .as_i64()
        .and_then(|d| i32::try_from(d).ok())
        .ok_or_else(|| Error::Value(format!("Value out of range: {dpi}")))?;
    let request = with_document(
        &format!("{B}RenderLibraryFootprint"),
        json!({"dpi": dpi}),
        doc,
    )?;
    let response = client
        .send(&request, &format!("{B}RenderLibraryFootprintResponse"))
        .await?;
    Ok(match response.get_field_by_name("image").as_deref() {
        Some(Value::Bytes(bytes)) => bytes.to_vec(),
        _ => Vec::new(),
    })
}

async fn footprint_summary(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let mut pads = Vec::new();
    let mut graphics = Map::new();
    let mut texts = Vec::new();
    let layer_name = |item: &DynamicMessage| {
        let name = enum_name(&format!("{BT}BoardLayer"), int_field(item, "layer"));
        name.strip_prefix("BL_").map(str::to_owned).unwrap_or(name)
    };
    for packed in footprint_items(client, doc).await? {
        if let Some(pad) = proto::unpack(&packed, &format!("{BT}Pad")) {
            let copper = first_copper(&pad);
            pads.push(json!({
                "number": str_field(&pad, "number"),
                "type": enum_name(&format!("{BT}PadType"), int_field(&pad, "type")),
                "position_mm": mm(&message_field(&pad, "position")),
                "size_mm": copper.as_ref().map(|c| mm(&message_field(c, "size"))),
                "shape": copper.as_ref().map(|c| enum_name(&format!("{BT}PadStackShape"), int_field(c, "shape"))),
            }));
        } else if let Some(shape) = proto::unpack(&packed, &format!("{BT}BoardGraphicShape")) {
            let name = layer_name(&shape);
            let count = graphics.get(&name).and_then(Json::as_i64).unwrap_or(0);
            graphics.insert(name, json!(count + 1));
        } else if let Some(text) = proto::unpack(&packed, &format!("{BT}BoardText")) {
            texts.push(json!({"text": str_field(&message_field(&text, "text"), "text"), "layer": layer_name(&text)}));
        }
    }
    let lib_id = message_field(doc, "lib_id");
    Ok(json!({
        "lib_id": format!("{}:{}", str_field(&lib_id, "library_nickname"), str_field(&lib_id, "entry_name")),
        "pads": pads,
        "graphics_per_layer": graphics,
        "texts": texts,
    }))
}

/// One row of `library_table(kind="footprint")`: (nickname, path, scope name).
struct LibraryRow {
    nickname: String,
    path: String,
    scope: String,
}

/// `library_table(kind="footprint")`: opens the Footprint Editor, then reads both tables.
async fn footprint_library_table(client: &mut Client) -> Result<Vec<LibraryRow>, Error> {
    open_footprint_editor(client).await?;
    let request = message(
        &format!("{C}GetLibraryTable"),
        json!({"type": "LT_FOOTPRINT", "scope": "LTS_BOTH", "substituted": true}),
    )?;
    let response = client
        .send(&request, &format!("{C}LibraryTableResponse"))
        .await?;
    Ok(messages(list_field(&response, "rows"))
        .iter()
        .map(|row| LibraryRow {
            nickname: str_field(row, "nickname"),
            path: str_field(row, "uri"),
            scope: match enum_name(&format!("{T}LibraryTableScope"), int_field(row, "scope"))
                .as_str()
            {
                "LTS_GLOBAL" => "global",
                "LTS_PROJECT" => "project",
                _ => "?",
            }
            .to_owned(),
        })
        .collect())
}

/// `check_library_status(status, action)`.
fn check_library_status(status: &DynamicMessage, action: &str) -> Result<Json, Error> {
    let code = int_field(status, "code");
    if code != 1 {
        let name = enum_name(&format!("{T}LibraryCommandStatus.Code"), code);
        let text = format!(
            "Could not {action}: {name} {}",
            str_field(status, "error_message")
        );
        return Err(Error::Runtime(text.trim().to_owned()));
    }
    Ok(json!({"success": true}))
}

/// `reload_library(nickname, kind="footprint")`.
async fn reload_footprint_library(client: &mut Client, nickname: &str) -> Result<Json, Error> {
    for row in footprint_library_table(client).await? {
        if row.nickname != nickname {
            continue;
        }
        // SCOPES[row["scope"]]: a row of an unknown scope ("?") raises KeyError
        let scope = match row.scope.as_str() {
            "global" => "LTS_GLOBAL",
            "project" => "LTS_PROJECT",
            other => return Err(Error::Runtime(footprint_gen::py_str_repr(other))),
        };
        let request = message(
            &format!("{C}ReloadLibrary"),
            json!({"type": "LT_FOOTPRINT", "scope": scope, "nickname": [nickname]}),
        )?;
        let status = client
            .send(&request, &format!("{T}LibraryCommandStatus"))
            .await?;
        return check_library_status(&status, "reload");
    }
    Err(Error::Value(format!(
        "Library {nickname} is not in the footprint library tables"
    )))
}

async fn footprint_save(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let request = with_document("kiapi.common.commands.SaveDocument", json!({}), doc)?;
    client.send(&request, EMPTY).await?;
    let lib_id = message_field(doc, "lib_id");
    let nickname = str_field(&lib_id, "library_nickname");
    reload_footprint_library(client, &nickname).await?;
    Ok(json!({"saved": format!("{nickname}:{}", str_field(&lib_id, "entry_name"))}))
}

// ---- Embedded files (footprints and library symbols) -------------------------------------

/// `embed_file`: one transaction; returns the file's EmbeddedFileInfo as protobuf JSON.
async fn embed_file(
    client: &mut Client,
    doc: &DynamicMessage,
    path: &str,
    overwrite: bool,
) -> Result<Json, Error> {
    let request = with_document(
        &format!("{C}EmbedFileFromPath"),
        json!({"path": path, "overwrite": overwrite}),
        doc,
    )?;
    let header = item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let sent = client
        .send(&request, &format!("{C}EmbedFileFromPathResponse"))
        .await;
    let (outcome, response) = match sent {
        Ok(response) => (Ok(()), Some(response)),
        Err(error) => (Err(error), None),
    };
    client
        .finish_transaction(&header, &id, &operation_id, "MCP: embed file", outcome)
        .await?;
    reply_json(&message_field(
        &response.expect("a committed transaction has a response"),
        "file",
    ))
}

async fn embedded_files(client: &mut Client, doc: &DynamicMessage) -> Result<Vec<Json>, Error> {
    let request = with_document(&format!("{C}GetEmbeddedFileList"), json!({}), doc)?;
    let response = client
        .send(&request, &format!("{C}EmbeddedFileListResponse"))
        .await?;
    messages(list_field(&response, "files"))
        .iter()
        .map(reply_json)
        .collect()
}

async fn remove_embedded_file(
    client: &mut Client,
    doc: &DynamicMessage,
    name: &str,
) -> Result<Json, Error> {
    let request = with_document(
        &format!("{C}RemoveEmbeddedFile"),
        json!({"name": name}),
        doc,
    )?;
    let header = item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let outcome = client.send(&request, EMPTY).await.map(|_| ());
    client
        .finish_transaction(
            &header,
            &id,
            &operation_id,
            "MCP: remove embedded file",
            outcome,
        )
        .await?;
    Ok(json!({"removed": name}))
}

/// `footprint_attach_step`: embeds a STEP/WRL model and makes it the footprint's 3D model.  Paths
/// are translated to the form KiCad reads (see [`model_handoff::kicad_path`]).
async fn footprint_attach_step(
    client: &mut Client,
    doc: &DynamicMessage,
    step_path: &str,
    offset: Vec<Json>,
    rotation: Vec<Json>,
) -> Result<Json, Error> {
    let step_path = model_handoff::kicad_path(step_path);
    let suffix = model_handoff::py_suffix(&step_path).to_lowercase();
    if ![".step", ".stp", ".stpz", ".wrl", ".wrz"].contains(&suffix.as_str()) {
        return Err(Error::Value(
            "3D models must be .step, .stp, .stpz, .wrl or .wrz".into(),
        ));
    }
    let embedded = embed_file(client, doc, &step_path, true).await?;
    // embedded["uri"] and embedded["name"]: MessageToDict leaves out empty fields, and a missing
    // key raises KeyError (its text is the key's repr) where Python first reads it
    let key = |name: &str| -> Result<Json, Error> {
        embedded
            .get(name)
            .cloned()
            .ok_or_else(|| Error::Runtime(format!("'{name}'")))
    };
    let uri = key("uri")?;
    let model = Model {
        filename: uri.as_str().unwrap_or_default().to_owned(),
        offset,
        rotation,
        scale: vec![json!(1), json!(1), json!(1)],
    };
    let settings = Properties {
        description: None,
        keywords: None,
        mounting: None,
        value: None,
        datasheet: None,
        exclude_from_bom: None,
        exclude_from_position_files: None,
        models: Some(vec![model]),
    };
    footprint_set_properties(client, doc, settings).await?;
    Ok(json!({"embedded": key("name")?, "model": uri}))
}

// ---- 3D models: SolidCraft handoff and checks in KiCad ------------------------------------

/// `footprint_model_plan`: the SolidCraft batch and export call for this package.
async fn footprint_model_plan(args: &Args, params: &Map<String, Json>) -> Result<Json, Error> {
    let package = args.str("package");
    let export_path = match args.opt_str("export_path") {
        Some(path) => path,
        None => {
            let folder = model_handoff::default_model_dir().await?;
            let shape = footprint_gen::generate(package, params)?;
            model_handoff::py_join(&folder, &format!("{}.step", py_str(&shape.name)))
        }
    };
    // The folder is made before the package is checked, as in Python
    let target = model_handoff::kicad_path(&export_path);
    if let Some(parent) = Path::new(&target).parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            Error::Runtime(model_handoff::file_error(&error, &parent.to_string_lossy()))
        })?;
    }
    model_handoff::model_plan(&model_handoff::PlanArgs {
        package,
        params,
        body_height: args.float("body_height"),
        standoff: args.float("standoff"),
        lead_thickness: args.float("lead_thickness"),
        lead_width: args.opt_float("lead_width"),
        lead_span: args.opt_float("lead_span"),
        foot_length: args.opt_float("foot_length"),
        export_path: Some(export_path),
    })
}

/// `kicad_cli()`: the kicad-cli binary KiCad reports.
async fn kicad_cli_path(client: &mut Client) -> Result<String, Error> {
    let request = message(
        &format!("{C}GetKiCadBinaryPath"),
        json!({"binary_name": "kicad-cli"}),
    )?;
    let path = str_field(
        &client.send(&request, &format!("{C}PathResponse")).await?,
        "path",
    );
    if path.is_empty() || !Path::new(&path).exists() {
        return Err(Error::Runtime(
            "KiCad did not report a kicad-cli binary".into(),
        ));
    }
    Ok(path)
}

/// `kicad_environment()`: the variables KiCad was launched with, or None if it cannot say.
async fn kicad_environment(client: &mut Client) -> Result<Option<BTreeMap<String, String>>, Error> {
    let request = message(&format!("{C}GetKiCadEnvironment"), json!({}))?;
    match client
        .send(&request, &format!("{C}KiCadEnvironmentResponse"))
        .await
    {
        Ok(response) => {
            let mut variables = BTreeMap::new();
            if let Some(Value::Map(map)) = response.get_field_by_name("variables").as_deref() {
                for (key, value) in map {
                    if let (MapKey::String(key), Value::String(value)) = (key, value) {
                        variables.insert(key.clone(), value.clone());
                    }
                }
            }
            Ok(Some(variables))
        }
        Err(error) if error.is_runtime_error() && error.to_string().starts_with("AS_UNHANDLED") => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Errors Python's `kicad_cli_launch` treats as "no KiCad answers": DiscoveryError, pynng errors
/// (a lost reply is a pynng.Timeout) and OSError.
fn unreachable_kicad(error: &Error) -> bool {
    match error {
        Error::Discovery(_) | Error::ReplyTimeout(_) => true,
        Error::Runtime(text) => text.starts_with("Cannot talk to KiCad at the selected endpoint"),
        _ => false,
    }
}

type Launch = (String, BTreeMap<String, String>, Map<String, Json>);

/// `kicad_cli_launch`: (kicad-cli path, environment, description): KiCad's own binary and launch
/// environment, or, when no KiCad answers, the installed or development kicad-cli.  Any other
/// error from KiCad is raised.
async fn kicad_cli_launch(client: &mut Client) -> Result<Launch, Error> {
    let asked = async {
        let path = kicad_cli_path(client).await?;
        let variables = kicad_environment(client).await?;
        Ok::<_, Error>((path, variables))
    }
    .await;
    let (path, variables, note) = match asked {
        Ok((path, variables)) => (path, variables, None),
        Err(error) if unreachable_kicad(&error) => {
            let Some(path) = kicad_cli::default_cli() else {
                return Err(Error::Runtime(format!(
                    "No running KiCad reported kicad-cli ({error}) and none was found; \
                     set KICAD_CLI to the kicad-cli of this KiCad build"
                )));
            };
            (path, None, Some(format!("KiCad not reachable: {error}")))
        }
        Err(error) => return Err(error),
    };
    let (env, mut description) = kicad_cli::cli_environment(&path, variables.as_ref());
    if let Some(note) = note {
        description.insert("note".into(), json!(note));
    }
    Ok((path, env, description))
}

/// `saved_footprint_text(lib_id)`: the saved .kicad_mod of a footprint, from its library's path.
async fn saved_footprint_text(client: &mut Client, lib_id: &str) -> Result<String, Error> {
    let (nickname, name) = partition(lib_id);
    for row in footprint_library_table(client).await? {
        if row.nickname == nickname {
            let path = model_handoff::py_join(&row.path, &format!("{name}.kicad_mod"));
            if !Path::new(&path).exists() {
                return Err(Error::Value(format!(
                    "{lib_id} is not saved yet (no {path})"
                )));
            }
            return model_handoff::read_text(&path);
        }
    }
    Err(Error::Value(format!(
        "footprint library {nickname} is not in the tables"
    )))
}

/// `bytes.lstrip()`: ASCII whitespace, vertical tab included.
fn lstrip(data: &[u8]) -> &[u8] {
    let start = data
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c))
        .unwrap_or(data.len());
    &data[start..]
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `footprint_check_model`: a saved footprint's embedded models (stored bytes, links) and the
/// solids KiCad exports when it loads the footprint in a separate process.
async fn footprint_check_model(
    ctx: &Ctx<'_>,
    lib_id: &str,
    source_path: Option<String>,
) -> Result<Json, Error> {
    let mut client = ctx.client.lock().await;
    let text = saved_footprint_text(&mut client, lib_id).await?;
    let model_pattern = regex::Regex::new(r#"\(model\s+"([^"]+)""#).expect("a valid pattern");
    let file_pattern =
        regex::Regex::new(r#"\(file\s+\(name\s+"([^"]+)"\)"#).expect("a valid pattern");
    let links: Vec<String> = model_pattern
        .captures_iter(&text)
        .map(|c| c[1].to_owned())
        .collect();
    let mut embedded = Vec::new();
    for captures in file_pattern.captures_iter(&text) {
        let name = &captures[1];
        let data = model_handoff::embedded_file_bytes(&text, name)?;
        let digest = sha256_hex(&data);
        let mut entry = json!({
            "name": name, "bytes": data.len(), "sha256": digest,
            "is_step": lstrip(&data).starts_with(b"ISO-10303-21;"),
            "linked": links.iter().any(|l| *l == format!("kicad-embed://{name}")),
        });
        if let Some(source) = source_path.as_deref().filter(|s| !s.is_empty()) {
            let source = model_handoff::kicad_path(source);
            let bytes = std::fs::read(&source)
                .map_err(|error| Error::Runtime(model_handoff::file_error(&error, &source)))?;
            entry["matches_source"] = json!(sha256_hex(&bytes) == digest);
        }
        embedded.push(entry);
    }
    let (path, env, _) = kicad_cli_launch(&mut client).await?;
    drop(client);
    let step = model_handoff::export_board_step(&path, &text, lib_id, &env).await?;
    Ok(json!({
        "lib_id": lib_id,
        "models": links,
        "embedded": embedded,
        "kicad_export_step_bytes": step.chars().count(),
        "kicad_export_solids": step.matches("MANIFOLD_SOLID_BREP").count(),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::args::validate;
    use super::*;

    #[test]
    fn signatures_and_items() {
        let spec = params("live_kicad_cli").unwrap();
        let args = validate("live_kicad_cli", spec, &json!({"args": ["version"]})).unwrap();
        assert_eq!(args.get("timeout_s"), &json!(300));
        let args = validate(
            "live_kicad_cli",
            spec,
            &json!({"args": ["version"], "timeout_s": 2}),
        )
        .unwrap();
        assert_eq!(py_str(args.get("timeout_s")), "2.0");
        assert_eq!(mounting_style("tht").unwrap(), "FMS_THROUGH_HOLE");
        assert_eq!(mounting_style("bga").unwrap_err().to_string(), "'bga'");
        assert_eq!(
            three(&[json!(1)]).unwrap_err().to_string(),
            "not enough values to unpack (expected 3, got 1)"
        );
        assert_eq!(all_copper().len(), 32);
        let shape =
            footprint_gen::generate("sot", json!({"variant": "23"}).as_object().unwrap()).unwrap();
        let items = generated_items(&shape, true).unwrap();
        // 3 pads, 5 fab lines, the silk lines, a marker, the courtyard and the fab text
        assert_eq!(items.len(), 3 + 5 + shape.silk_lines.len() + 1 + 1 + 1);
        assert_eq!(lstrip(b" \x0b\nISO"), b"ISO");
    }
}
