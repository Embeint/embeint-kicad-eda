// SPDX-License-Identifier: GPL-3.0-or-later
//! PCB editor: open, reads, item edits, placement, outline, zones, graphics, views, layers,
//! origins, selection and DRC (the `live_pcb_*` tools of server.py that call `BoardTools`).
//!
//! See [`super`] for the module contract.  The operations are in [`crate::board_client`]; this
//! module checks the arguments as server.py does (the board document first) and maps each tool to
//! its operation.

use embeint_ipc::client::{self, Error};
use serde_json::Value as Json;

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{begin_guard, end_guard, Ctx, Output};
use crate::board_client::{self as board, Outline, Placement, ZoneSettings};

const GUARD: [Param; 2] = [
    opt("expected_revision", Arg::OptStr, Default::None),
    opt("operation_id", Arg::OptStr, Default::None),
];

pub fn params(tool: &str) -> Option<&'static [Param]> {
    Some(match tool {
        "live_pcb_open" => sig![opt("path", Arg::OptStr, Default::None)],
        "live_pcb_summary" | "live_pcb_nets" | "live_pcb_refill_zones" | "live_pcb_save" => {
            sig![req("board_document", Arg::Dict)]
        }
        "live_pcb_footprints" => sig![
            req("board_document", Arg::Dict),
            opt("references", Arg::OptListStr, Default::None),
            opt("pads", Arg::Bool, Default::Bool(false)),
        ],
        "live_pcb_items" => sig![req("board_document", Arg::Dict), req("kinds", Arg::ListStr),],
        "live_pcb_create_items" | "live_pcb_update_items" => sig![
            req("board_document", Arg::Dict),
            req("items", Arg::ListDict),
            GUARD[0],
            GUARD[1],
        ],
        "live_pcb_delete_items" => sig![
            req("board_document", Arg::Dict),
            req("ids", Arg::ListStr),
            GUARD[0],
            GUARD[1],
        ],
        "live_pcb_delete_routing" => sig![
            req("board_document", Arg::Dict),
            opt("net", Arg::OptStr, Default::None),
            opt("layer", Arg::OptStr, Default::None),
            opt("kinds", Arg::OptListStr, Default::None),
        ],
        "live_pcb_update_from_schematic" => sig![
            req("board_document", Arg::Dict),
            req("schematic_document", Arg::Dict),
            opt("dry_run", Arg::Bool, Default::Bool(false)),
            opt("delete_extra_footprints", Arg::Bool, Default::Bool(false)),
            opt("update_footprints", Arg::Bool, Default::Bool(true)),
            opt("match", Arg::Str, Default::Str("uuid")),
        ],
        "live_pcb_place_footprint" => sig![
            req("board_document", Arg::Dict),
            req("lib_id", Arg::Str),
            req("x_mm", Arg::Float),
            req("y_mm", Arg::Float),
            // `rotation: float = 0`: FastMCP passes the int default unconverted
            opt("rotation", Arg::Float, Default::Int(0)),
            opt("side", Arg::Str, Default::Str("front")),
            opt("reference", Arg::OptStr, Default::None),
            opt("value", Arg::OptStr, Default::None),
            opt("locked", Arg::Bool, Default::Bool(false)),
        ],
        "live_pcb_move" => sig![
            req("board_document", Arg::Dict),
            req("moves", Arg::ListDict),
        ],
        "live_pcb_add_via" => sig![
            req("board_document", Arg::Dict),
            req("x_mm", Arg::Float),
            req("y_mm", Arg::Float),
            opt("net", Arg::OptStr, Default::None),
            opt("size_mm", Arg::OptFloat, Default::None),
            opt("drill_mm", Arg::OptFloat, Default::None),
        ],
        "live_pcb_outline" => sig![
            req("board_document", Arg::Dict),
            opt("width_mm", Arg::Float, Default::Int(0)),
            opt("height_mm", Arg::Float, Default::Int(0)),
            opt("x_mm", Arg::Float, Default::Int(0)),
            opt("y_mm", Arg::Float, Default::Int(0)),
            opt("corner_radius_mm", Arg::Float, Default::Int(0)),
            opt("points", Arg::OptListListFloat, Default::None),
            opt("replace", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_add_zone" => sig![
            req("board_document", Arg::Dict),
            req("net", Arg::Str),
            opt("layers", Arg::OptListStr, Default::None),
            opt("points", Arg::OptListListFloat, Default::None),
            opt("name", Arg::Str, Default::Str("")),
            opt("priority", Arg::Int, Default::Int(0)),
            opt("clearance_mm", Arg::Float, Default::Float(0.2)),
            opt("min_width_mm", Arg::Float, Default::Float(0.25)),
            opt("connection", Arg::Str, Default::Str("thermal")),
            opt("inset_mm", Arg::Float, Default::Float(0.0)),
            opt("fill", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_add_graphics" => sig![
            req("board_document", Arg::Dict),
            req("shapes", Arg::ListDict),
        ],
        "live_pcb_drc" => sig![
            req("board_document", Arg::Dict),
            opt("schematic_parity", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_view" => sig![
            req("board_document", Arg::Dict),
            opt("layers", Arg::OptListStr, Default::None),
            opt("dpi", Arg::Int, Default::Int(100)),
            opt("frame", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_view_3d" => sig![
            req("board_document", Arg::Dict),
            opt("side", Arg::Str, Default::Str("top")),
            opt("zoom", Arg::Float, Default::Float(1.0)),
            opt("quality", Arg::Str, Default::Str("basic")),
            opt("width", Arg::Int, Default::Int(1200)),
            opt("height", Arg::Int, Default::Int(900)),
        ],
        "live_pcb_origins" => sig![
            req("board_document", Arg::Dict),
            opt("grid_origin_mm", Arg::OptListFloat, Default::None),
            opt("drill_origin_mm", Arg::OptListFloat, Default::None),
        ],
        "live_pcb_layers" => sig![
            req("board_document", Arg::Dict),
            opt("copper_layers", Arg::OptInt, Default::None),
            opt("visible", Arg::OptListStr, Default::None),
            opt("active", Arg::OptStr, Default::None),
        ],
        "live_pcb_select" => sig![
            req("board_document", Arg::Dict),
            opt("references", Arg::OptListStr, Default::None),
            opt("ids", Arg::OptListStr, Default::None),
            opt("add", Arg::Bool, Default::Bool(false)),
        ],
        "live_pcb_focus" => sig![
            req("board_document", Arg::Dict),
            opt("references", Arg::OptListStr, Default::None),
            opt("ids", Arg::OptListStr, Default::None),
        ],
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(run_tool(name, args, ctx).await)
}

/// A Python int argument that does not fit an i64 (Python ints are unbounded); KiCad's int32 and
/// uint32 fields refuse it as protobuf does.
fn int_arg(args: &Args, name: &str) -> Result<i64, Error> {
    args.opt_int(name)
        .ok_or_else(|| Error::Value(format!("Value out of range: {}", args.get(name))))
}

async fn run_tool(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<Output, Error> {
    if name == "live_pcb_open" {
        let path = args.opt_str("path");
        let mut client = ctx.client.lock().await;
        return Ok(Output::Dict(
            board::open_board(&mut client, path.as_deref()).await?,
        ));
    }
    // server.py's board(board_document) comes first in every other tool
    let doc = client::board_document(args.get("board_document"))?;
    let references = args.opt_strs("references");
    let ids = args.opt_strs("ids");
    let mut client = ctx.client.lock().await;
    let client = &mut *client;
    Ok(match name {
        "live_pcb_summary" => Output::Dict(board::board_summary(client, &doc).await?),
        "live_pcb_footprints" => Output::List(
            board::board_footprints(client, &doc, references.as_deref(), args.bool("pads")).await?,
        ),
        "live_pcb_nets" => Output::List(board::board_nets(client, &doc).await?),
        "live_pcb_items" => {
            let items = board::board_items(client, &doc, &args.strs("kinds")).await?;
            Output::List(
                items
                    .iter()
                    .map(client::reply_json)
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
        "live_pcb_create_items" | "live_pcb_update_items" => {
            let values = args.dicts("items").cloned().unwrap_or_default();
            let items = board::parse_board_items(&values)?;
            let (update, description) = if name == "live_pcb_update_items" {
                (true, "MCP: update board items")
            } else {
                (false, "MCP: create board items")
            };
            begin_guard(client, args);
            let result = client.mutate(&doc, &items, update, Some(description)).await;
            Output::Dict(end_guard(client, result)?)
        }
        "live_pcb_delete_items" => {
            begin_guard(client, args);
            let result = client.delete_items(&doc, &args.strs("ids")).await;
            Output::Dict(end_guard(client, result)?)
        }
        "live_pcb_delete_routing" => {
            // kinds or ["tracks", "arcs", "vias"]: an empty list takes the default too
            let kinds = args
                .opt_strs("kinds")
                .filter(|k| !k.is_empty())
                .unwrap_or_else(|| vec!["tracks".into(), "arcs".into(), "vias".into()]);
            Output::Dict(
                board::board_delete_where(
                    client,
                    &doc,
                    &kinds,
                    args.opt_str("net").as_deref(),
                    args.opt_str("layer").as_deref(),
                )
                .await?,
            )
        }
        "live_pcb_update_from_schematic" => {
            let schematic = client::schematic_document(args.get("schematic_document"))?;
            Output::Dict(
                board::board_update_from_schematic(
                    client,
                    &doc,
                    &schematic,
                    args.bool("dry_run"),
                    args.bool("delete_extra_footprints"),
                    args.bool("update_footprints"),
                    args.str("match"),
                )
                .await?,
            )
        }
        "live_pcb_place_footprint" => {
            let reference = args.opt_str("reference");
            let value = args.opt_str("value");
            let place = Placement {
                lib_id: args.str("lib_id"),
                x: args.float("x_mm"),
                y: args.float("y_mm"),
                rotation: args.float("rotation"),
                side: args.str("side"),
                reference: reference.as_deref(),
                value: value.as_deref(),
                locked: args.bool("locked"),
            };
            Output::Dict(board::board_place_footprint(client, &doc, &place).await?)
        }
        "live_pcb_move" => {
            let moves = args.dicts("moves").cloned().unwrap_or_default();
            Output::Dict(board::board_move_footprints(client, &doc, &moves).await?)
        }
        "live_pcb_add_via" => Output::Dict(
            board::board_add_via(
                client,
                &doc,
                args.float("x_mm"),
                args.float("y_mm"),
                args.opt_str("net").as_deref(),
                args.opt_float("size_mm"),
                args.opt_float("drill_mm"),
            )
            .await?,
        ),
        "live_pcb_outline" => {
            let points = args.points("points");
            let outline = Outline {
                width: args.float("width_mm"),
                height: args.float("height_mm"),
                x: args.float("x_mm"),
                y: args.float("y_mm"),
                corner_radius: args.float("corner_radius_mm"),
                line_width: 0.05,
                replace: args.bool("replace"),
                points: points.as_deref(),
            };
            Output::Dict(board::board_outline(client, &doc, &outline).await?)
        }
        "live_pcb_add_zone" => {
            // layers or ["F.Cu"]
            let layers = args
                .opt_strs("layers")
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| vec!["F.Cu".into()]);
            let net = args.str("net");
            let settings = ZoneSettings {
                net: Some(net),
                name: args.str("name"),
                priority: int_arg(args, "priority")?,
                clearance: args.float("clearance_mm"),
                min_width: args.float("min_width_mm"),
                connection: args.str("connection"),
                ..ZoneSettings::default()
            };
            let points = Some(args.get("points")).filter(|p| !p.is_null());
            Output::Dict(
                board::board_add_zone(
                    client,
                    &doc,
                    &layers,
                    points,
                    &settings,
                    args.float("inset_mm"),
                    args.bool("fill"),
                )
                .await?,
            )
        }
        "live_pcb_add_graphics" => {
            let shapes = args.dicts("shapes").cloned().unwrap_or_default();
            Output::Dict(board::board_add_graphics(client, &doc, &shapes).await?)
        }
        "live_pcb_refill_zones" => Output::Dict(board::board_refill(client, &doc, &[]).await?),
        "live_pcb_drc" => Output::Dict(
            board::board_drc(client, &doc, args.bool("schematic_parity"), true, Some(200)).await?,
        ),
        "live_pcb_save" => Output::Dict(board::board_save(client, &doc).await?),
        "live_pcb_view" => {
            let layers = args.opt_strs("layers");
            Output::png(
                board::board_render(
                    client,
                    &doc,
                    layers.as_deref(),
                    // beyond i64 it is outside 50..600 all the same
                    args.opt_int("dpi").unwrap_or(i64::MAX),
                    args.bool("frame"),
                )
                .await?,
            )
        }
        "live_pcb_view_3d" => Output::png(
            board::board_render_3d(
                client,
                &doc,
                args.str("side"),
                int_arg(args, "width")?,
                int_arg(args, "height")?,
                args.float("zoom"),
                args.str("quality"),
                None,
            )
            .await?,
        ),
        "live_pcb_origins" => {
            let grid = args.floats("grid_origin_mm");
            let drill = args.floats("drill_origin_mm");
            Output::Dict(
                board::board_origins(client, &doc, grid.as_deref(), drill.as_deref()).await?,
            )
        }
        "live_pcb_layers" => {
            let copper_layers = match args.get("copper_layers") {
                Json::Null => None,
                _ => Some(int_arg(args, "copper_layers")?),
            };
            let visible = args.opt_strs("visible");
            Output::Dict(
                board::board_layers(
                    client,
                    &doc,
                    copper_layers,
                    visible.as_deref(),
                    args.opt_str("active").as_deref(),
                )
                .await?,
            )
        }
        "live_pcb_select" => Output::Dict(
            board::board_select(
                client,
                &doc,
                ids.as_deref(),
                references.as_deref(),
                args.bool("add"),
            )
            .await?,
        ),
        "live_pcb_focus" => Output::Dict(
            board::board_focus(client, &doc, ids.as_deref(), references.as_deref()).await?,
        ),
        _ => unreachable!("validated tool name"),
    })
}
