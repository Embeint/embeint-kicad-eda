// SPDX-License-Identifier: GPL-3.0-or-later
//! Schematic editor tools (open, place, view, page, annotate, move, connect, netlist, ERC, ...),
//! with the page and undo tools the PCB editor shares (`live_pcb_page`, `live_pcb_set_page`,
//! `live_pcb_undo`): server.py's tool functions over the client code ported in [`ops`].
//!
//! See [`super`] for the module contract.

mod ops;

use embeint_ipc::client::{self, Error};

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{begin_guard, end_guard, Ctx, Output};

const GUARD: [Param; 2] = [
    opt("expected_revision", Arg::OptStr, Default::None),
    opt("operation_id", Arg::OptStr, Default::None),
];

/// The settings of live_schematic_set_page and live_pcb_set_page after the document.
const PAGE: [Param; 7] = [
    opt("size", Arg::OptStr, Default::None),
    opt("orientation", Arg::OptStr, Default::None),
    opt("title", Arg::OptStr, Default::None),
    opt("date", Arg::OptStr, Default::None),
    opt("revision", Arg::OptStr, Default::None),
    opt("company", Arg::OptStr, Default::None),
    opt("comments", Arg::OptListStr, Default::None),
];

const DOC: Param = req("document_spec", Arg::Dict);
const BOARD: Param = req("board_document", Arg::Dict);

pub fn params(tool: &str) -> Option<&'static [Param]> {
    Some(match tool {
        "live_schematic_open" => sig![opt("path", Arg::OptStr, Default::None)],
        "live_schematic_place_symbol" => sig![
            DOC,
            req("lib_id", Arg::Str),
            req("x_mm", Arg::Float),
            req("y_mm", Arg::Float),
            opt("reference", Arg::OptStr, Default::None),
            opt("rotation", Arg::Int, Default::Int(0)),
        ],
        "live_schematic_view" => sig![
            DOC,
            opt("dpi", Arg::Int, Default::Int(100)),
            opt("frame", Arg::Bool, Default::Bool(true)),
        ],
        "live_schematic_page" | "live_schematic_save" | "live_schematic_erc" => sig![DOC],
        "live_pcb_page" => sig![BOARD],
        "live_schematic_set_page" => {
            sig![DOC, PAGE[0], PAGE[1], PAGE[2], PAGE[3], PAGE[4], PAGE[5], PAGE[6]]
        }
        "live_pcb_set_page" => {
            sig![BOARD, PAGE[0], PAGE[1], PAGE[2], PAGE[3], PAGE[4], PAGE[5], PAGE[6]]
        }
        "live_schematic_focus" => sig![DOC, req("ids", Arg::ListStr)],
        "live_schematic_annotate" => sig![
            DOC,
            opt("scope", Arg::Str, Default::Str("all")),
            opt("symbols", Arg::OptListStr, Default::None),
            opt("order", Arg::Str, Default::Str("x")),
            opt("numbering", Arg::Str, Default::Str("first_free")),
            opt("reset_existing", Arg::Bool, Default::Bool(false)),
            opt("recursive", Arg::Bool, Default::Bool(true)),
            opt("start_number", Arg::Int, Default::Int(0)),
        ],
        "live_schematic_update_from_library" => sig![
            DOC,
            opt("symbols", Arg::OptListStr, Default::None),
            opt("new_lib_id", Arg::OptStr, Default::None),
            opt("reset_field_text", Arg::Bool, Default::Bool(false)),
            opt("reset_field_visibility", Arg::Bool, Default::Bool(false)),
            opt("reset_field_positions", Arg::Bool, Default::Bool(false)),
            opt("remove_extra_fields", Arg::Bool, Default::Bool(false)),
        ],
        "live_schematic_autoplace_fields" => {
            sig![DOC, opt("ids", Arg::OptListStr, Default::None)]
        }
        "live_schematic_move" => sig![
            DOC,
            req("ids", Arg::ListStr),
            // `float = 0`: FastMCP passes the default unconverted
            opt("dx_mm", Arg::Float, Default::Int(0)),
            opt("dy_mm", Arg::Float, Default::Int(0)),
            opt("rotation", Arg::Int, Default::Int(0)),
            opt("mirror_horizontally", Arg::Bool, Default::Bool(false)),
            opt("mirror_vertically", Arg::Bool, Default::Bool(false)),
            opt("drag_wires", Arg::Bool, Default::Bool(true)),
        ],
        "live_schematic_undo" => sig![
            DOC,
            opt("redo", Arg::Bool, Default::Bool(false)),
            GUARD[0],
            GUARD[1],
        ],
        "live_pcb_undo" => sig![
            BOARD,
            opt("redo", Arg::Bool, Default::Bool(false)),
            GUARD[0],
            GUARD[1],
        ],
        "live_schematic_symbol_pins" => {
            sig![DOC, opt("symbols", Arg::OptListStr, Default::None)]
        }
        "live_schematic_connect_pins" => sig![
            DOC,
            req("from_reference", Arg::Str),
            req("from_pin", Arg::Str),
            req("to_reference", Arg::Str),
            req("to_pin", Arg::Str),
        ],
        "live_schematic_connect_pin_to_net" => sig![
            DOC,
            req("reference", Arg::Str),
            req("pin", Arg::Str),
            req("net", Arg::Str),
            opt("label", Arg::Str, Default::Str("local")),
            opt("stub_mm", Arg::Float, Default::Float(2.54)),
        ],
        "live_schematic_netlist" => {
            sig![DOC, opt("scope", Arg::Str, Default::Str("hierarchy"))]
        }
        "live_schematic_select" => sig![
            DOC,
            req("ids", Arg::ListStr),
            opt("add", Arg::Bool, Default::Bool(false)),
        ],
        "live_schematic_highlight_nets" => sig![DOC, req("nets", Arg::ListStr)],
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(run_tool(name, args, ctx).await)
}

/// The page settings of a set_page call.
fn page_args(args: &Args) -> ops::PageArgs {
    ops::PageArgs {
        size: args.opt_str("size"),
        orientation: args.opt_str("orientation"),
        title: args.opt_str("title"),
        date: args.opt_str("date"),
        revision: args.opt_str("revision"),
        company: args.opt_str("company"),
        comments: args.opt_strs("comments"),
    }
}

async fn run_tool(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<Output, Error> {
    // Every tool but open and the PCB ones checks its schematic document before anything else,
    // as server.py's document(document_spec) argument does
    let doc = match name {
        "live_schematic_open" => None,
        "live_pcb_page" | "live_pcb_set_page" | "live_pcb_undo" => {
            Some(client::board_document(args.get("board_document"))?)
        }
        _ => Some(client::schematic_document(args.get("document_spec"))?),
    };
    let doc = doc.as_ref();
    let spec = || doc.expect("validated document");
    let mut client = ctx.client.lock().await;
    let client = &mut *client;
    // `symbols or []` and friends in server.py
    let list = |name: &str| args.strs(name);
    Ok(match name {
        "live_schematic_open" => {
            Output::Dict(ops::open_schematic(client, args.opt_str("path")).await?)
        }
        "live_schematic_place_symbol" => Output::Dict(
            ops::place_symbol(
                client,
                spec(),
                args.str("lib_id"),
                args.float("x_mm"),
                args.float("y_mm"),
                args.opt_str("reference"),
                args.opt_int("rotation"),
            )
            .await?,
        ),
        "live_schematic_view" => {
            Output::png(ops::render(client, spec(), args.opt_int("dpi"), args.bool("frame")).await?)
        }
        "live_schematic_page" | "live_pcb_page" => {
            Output::Dict(ops::page_layout(client, spec()).await?)
        }
        "live_schematic_set_page" | "live_pcb_set_page" => {
            Output::Dict(ops::set_page(client, spec(), page_args(args)).await?)
        }
        "live_schematic_focus" => Output::Dict(ops::focus(client, spec(), &list("ids")).await?),
        "live_schematic_save" => Output::Dict(ops::save(client, spec()).await?),
        "live_schematic_annotate" => {
            let annotate = ops::AnnotateArgs {
                scope: args.str("scope"),
                symbols: list("symbols"),
                order: args.str("order"),
                numbering: args.str("numbering"),
                reset_existing: args.bool("reset_existing"),
                recursive: args.bool("recursive"),
                start_number: args.get("start_number"),
            };
            Output::Dict(ops::annotate(client, spec(), annotate).await?)
        }
        "live_schematic_update_from_library" => {
            let update = ops::LibraryUpdateArgs {
                symbols: list("symbols"),
                new_lib_id: args.opt_str("new_lib_id"),
                reset_field_text: args.bool("reset_field_text"),
                reset_field_visibility: args.bool("reset_field_visibility"),
                reset_field_positions: args.bool("reset_field_positions"),
                remove_extra_fields: args.bool("remove_extra_fields"),
            };
            Output::Dict(ops::update_from_library(client, spec(), update).await?)
        }
        "live_schematic_autoplace_fields" => {
            Output::Dict(ops::autoplace(client, spec(), &list("ids")).await?)
        }
        "live_schematic_move" => {
            let ids = list("ids");
            let transform = ops::TransformArgs {
                ids: &ids,
                dx: args.float("dx_mm"),
                dy: args.float("dy_mm"),
                rotation: args.opt_int("rotation"),
                mirror_horizontally: args.bool("mirror_horizontally"),
                mirror_vertically: args.bool("mirror_vertically"),
                drag_wires: args.bool("drag_wires"),
            };
            Output::Dict(ops::transform(client, spec(), transform).await?)
        }
        "live_schematic_undo" | "live_pcb_undo" => {
            begin_guard(client, args);
            let result = ops::undo(client, spec(), args.bool("redo")).await;
            Output::Dict(end_guard(client, result)?)
        }
        "live_schematic_symbol_pins" => {
            Output::List(ops::symbol_pins(client, spec(), &list("symbols")).await?)
        }
        "live_schematic_connect_pins" => Output::Dict(
            ops::connect_pins(
                client,
                spec(),
                (args.str("from_reference"), args.str("from_pin")),
                (args.str("to_reference"), args.str("to_pin")),
            )
            .await?,
        ),
        "live_schematic_connect_pin_to_net" => Output::Dict(
            ops::connect_pin_to_net(
                client,
                spec(),
                (args.str("reference"), args.str("pin")),
                args.str("net"),
                args.str("label"),
                args.float("stub_mm"),
            )
            .await?,
        ),
        "live_schematic_netlist" => {
            Output::Dict(ops::netlist_pins(client, spec(), args.str("scope")).await?)
        }
        "live_schematic_erc" => Output::Dict(ops::erc(client, spec()).await?),
        "live_schematic_select" => {
            Output::Dict(ops::select(client, spec(), &list("ids"), args.bool("add")).await?)
        }
        "live_schematic_highlight_nets" => {
            Output::Dict(ops::highlight_nets(client, &list("nets")).await?)
        }
        _ => unreachable!("validated tool name"),
    })
}
