// SPDX-License-Identifier: GPL-3.0-or-later
//! Board design: rules, custom rules, net classes, stackup, RF and impedance, path checks, tracks,
//! keepouts and routing (the `live_pcb_*` tools of server.py that call `BoardDesignTools`).
//!
//! See [`super`] for the module contract.  The operations are in [`crate::board_design`] (and the
//! stackup helpers in [`crate::board_client`]); this module checks the arguments as server.py does
//! (the board document first) and maps each tool to its operation.

use embeint_ipc::client::{self, Error};

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{Ctx, Output};
use crate::board_client::{self as board, PlainRoute, StackupChanges};
use crate::board_design::{self as design, AvoidingRoute, NetRoute, RfNet, RuleChanges};

/// The minimum rule arguments of live_pcb_design_rules, in the order Python passes them.
const MINIMUMS: [&str; 14] = [
    "min_clearance",
    "min_track_width",
    "min_via_diameter",
    "min_drill",
    "min_annular_ring",
    "copper_edge_clearance",
    "hole_clearance",
    "hole_to_hole",
    "silk_clearance",
    "min_silk_text_height",
    "min_silk_text_thickness",
    "min_connection_width",
    "min_microvia_diameter",
    "min_microvia_drill",
];

pub fn params(tool: &str) -> Option<&'static [Param]> {
    Some(match tool {
        "live_pcb_add_tracks" => sig![
            req("board_document", Arg::Dict),
            req("points", Arg::ListListFloat),
            opt("layer", Arg::Str, Default::Str("F.Cu")),
            opt("width_mm", Arg::OptFloat, Default::None),
            opt("net", Arg::OptStr, Default::None),
            opt("check", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_route" => sig![
            req("board_document", Arg::Dict),
            req("from_reference", Arg::Str),
            req("from_pad", Arg::Str),
            req("to_reference", Arg::Str),
            req("to_pad", Arg::Str),
            opt("layers", Arg::OptListStr, Default::None),
            opt("width_mm", Arg::OptFloat, Default::None),
            opt("vias", Arg::Bool, Default::Bool(true)),
            opt("grid_mm", Arg::Float, Default::Float(0.1)),
            opt("verify", Arg::Bool, Default::Bool(true)),
            opt("avoid", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_route_net" => sig![
            req("board_document", Arg::Dict),
            req("net", Arg::Str),
            opt("layers", Arg::OptListStr, Default::None),
            opt("width_mm", Arg::OptFloat, Default::None),
            opt("vias", Arg::Bool, Default::Bool(true)),
            opt("grid_mm", Arg::Float, Default::Float(0.1)),
            opt("verify", Arg::Bool, Default::Bool(true)),
        ],
        "live_pcb_check_path" => sig![
            req("board_document", Arg::Dict),
            req("points", Arg::ListListFloat),
            opt("layer", Arg::Str, Default::Str("F.Cu")),
            opt("width_mm", Arg::Float, Default::Float(0.25)),
            opt("net", Arg::Str, Default::Str("")),
        ],
        "live_pcb_add_keepout" => sig![
            req("board_document", Arg::Dict),
            opt("points", Arg::OptListListFloat, Default::None),
            opt("around_reference", Arg::OptStr, Default::None),
            opt("margin_mm", Arg::Float, Default::Float(1.0)),
            opt("layers", Arg::OptListStr, Default::None),
            opt("name", Arg::Str, Default::Str("")),
            opt("tracks", Arg::Bool, Default::Bool(true)),
            opt("vias", Arg::Bool, Default::Bool(true)),
            opt("pads", Arg::Bool, Default::Bool(false)),
            opt("copper", Arg::Bool, Default::Bool(true)),
            opt("footprints", Arg::Bool, Default::Bool(false)),
        ],
        "live_pcb_design_rules" => sig![
            req("board_document", Arg::Dict),
            opt("min_clearance", Arg::OptFloat, Default::None),
            opt("min_track_width", Arg::OptFloat, Default::None),
            opt("min_via_diameter", Arg::OptFloat, Default::None),
            opt("min_drill", Arg::OptFloat, Default::None),
            opt("min_annular_ring", Arg::OptFloat, Default::None),
            opt("copper_edge_clearance", Arg::OptFloat, Default::None),
            opt("hole_clearance", Arg::OptFloat, Default::None),
            opt("hole_to_hole", Arg::OptFloat, Default::None),
            opt("silk_clearance", Arg::OptFloat, Default::None),
            opt("min_silk_text_height", Arg::OptFloat, Default::None),
            opt("min_silk_text_thickness", Arg::OptFloat, Default::None),
            opt("min_connection_width", Arg::OptFloat, Default::None),
            opt("min_microvia_diameter", Arg::OptFloat, Default::None),
            opt("min_microvia_drill", Arg::OptFloat, Default::None),
            opt("track_widths_mm", Arg::OptListFloat, Default::None),
            opt("via_sizes_mm", Arg::OptListListFloat, Default::None),
            opt("severities", Arg::OptDict, Default::None),
        ],
        "live_pcb_custom_rules" => sig![
            req("board_document", Arg::Dict),
            opt("action", Arg::Str, Default::Str("get")),
            opt("rules", Arg::OptListDict, Default::None),
            opt("names", Arg::OptListStr, Default::None),
        ],
        "live_pcb_netclasses" => sig![
            req("board_document", Arg::Dict),
            opt("classes", Arg::OptListDict, Default::None),
            opt("assign", Arg::OptDict, Default::None),
            opt("patterns", Arg::OptDict, Default::None),
        ],
        "live_pcb_stackup" => sig![req("board_document", Arg::Dict)],
        "live_pcb_set_stackup" => sig![
            req("board_document", Arg::Dict),
            opt("copper_layers", Arg::OptInt, Default::None),
            opt("copper_thickness_mm", Arg::OptFloatOrDict, Default::None),
            opt("dielectrics", Arg::OptListDict, Default::None),
            opt("board_thickness_mm", Arg::OptFloat, Default::None),
            opt("soldermask_color", Arg::OptStr, Default::None),
            opt("silkscreen_color", Arg::OptStr, Default::None),
            opt("finish", Arg::OptStr, Default::None),
            opt("impedance_controlled", Arg::OptBool, Default::None),
        ],
        "live_pcb_impedance_width" => sig![
            req("board_document", Arg::Dict),
            opt("impedance_ohm", Arg::Float, Default::Float(50.0)),
            opt("layer", Arg::Str, Default::Str("F.Cu")),
            opt("structure", Arg::Str, Default::Str("microstrip")),
            opt("gap_mm", Arg::OptFloat, Default::None),
            opt("reference_layer", Arg::OptStr, Default::None),
        ],
        "live_pcb_rf_net" => sig![
            req("board_document", Arg::Dict),
            req("nets", Arg::ListStr),
            opt("netclass", Arg::Str, Default::Str("RF")),
            opt("impedance_ohm", Arg::Float, Default::Float(50.0)),
            opt("width_mm", Arg::OptFloat, Default::None),
            opt("structure", Arg::Str, Default::Str("microstrip")),
            opt("gap_mm", Arg::OptFloat, Default::None),
            opt("clearance_mm", Arg::OptFloat, Default::None),
            opt("layer", Arg::Str, Default::Str("F.Cu")),
            opt("keep_others_away_mm", Arg::OptFloat, Default::None),
        ],
        "live_pcb_check" => sig![
            req("board_document", Arg::Dict),
            opt("categories", Arg::OptListStr, Default::None),
            opt("schematic_parity", Arg::Bool, Default::Bool(false)),
        ],
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(run_tool(name, args, ctx).await.map(Output::Dict))
}

async fn run_tool(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<serde_json::Value, Error> {
    // server.py's board(board_document) comes first in every tool
    let doc = client::board_document(args.get("board_document"))?;
    let layers = args.opt_strs("layers");
    let mut client = ctx.client.lock().await;
    let client = &mut *client;
    match name {
        "live_pcb_add_tracks" => {
            let points = args.points("points").unwrap_or_default();
            let layer = args.str("layer");
            let width = args.opt_float("width_mm");
            let net = args.opt_str("net");
            if args.bool("check") {
                let width = board::width_for(client, &doc, net.as_deref(), width).await?;
                let found = design::board_clear_path(
                    client,
                    &doc,
                    net.as_deref().unwrap_or(""),
                    &points,
                    layer,
                    width,
                )
                .await?;
                if found["clear"] != true {
                    let conflicts: Vec<serde_json::Value> = found["conflicts"]
                        .as_array()
                        .map(|c| c.iter().take(5).cloned().collect())
                        .unwrap_or_default();
                    return Err(Error::Value(format!(
                        "the path conflicts with other copper: {} (use live_pcb_route to find a \
                         clear way, or check=false to draw it anyway)",
                        embeint_ipc::pbjson::py_repr(&serde_json::Value::Array(conflicts))
                    )));
                }
            }
            board::board_add_tracks(client, &doc, &points, layer, width, net.as_deref()).await
        }
        "live_pcb_route" => {
            let width = args.opt_float("width_mm");
            if !args.bool("avoid") {
                let route = PlainRoute {
                    from_ref: args.str("from_reference"),
                    from_pad: args.str("from_pad"),
                    to_ref: args.str("to_reference"),
                    to_pad: args.str("to_pad"),
                    // layers[0] if layers else None
                    layer: layers
                        .as_deref()
                        .and_then(|l| l.first())
                        .map(String::as_str),
                    width,
                    posture: "horizontal_first",
                    via_at: None,
                    second_layer: None,
                };
                return board::board_route(client, &doc, &route).await;
            }
            let options = AvoidingRoute {
                from_ref: args.str("from_reference"),
                from_pad: args.str("from_pad"),
                to_ref: args.str("to_reference"),
                to_pad: args.str("to_pad"),
                layers: layers.as_deref(),
                width,
                vias: args.bool("vias"),
                grid: args.float("grid_mm"),
                verify: args.bool("verify"),
            };
            design::board_route_avoiding(client, &doc, &options).await
        }
        "live_pcb_route_net" => {
            let options = NetRoute {
                net: args.str("net"),
                layers: layers.as_deref(),
                width: args.opt_float("width_mm"),
                vias: args.bool("vias"),
                grid: args.float("grid_mm"),
                verify: args.bool("verify"),
            };
            design::board_route_net(client, &doc, &options).await
        }
        "live_pcb_check_path" => {
            let points = args.points("points").unwrap_or_default();
            design::board_clear_path(
                client,
                &doc,
                args.str("net"),
                &points,
                args.str("layer"),
                args.float("width_mm"),
            )
            .await
        }
        "live_pcb_add_keepout" => {
            let rule = design::rule_area(
                args.bool("tracks"),
                args.bool("vias"),
                args.bool("pads"),
                args.bool("copper"),
                args.bool("footprints"),
            );
            if let Some(reference) = args.opt_str("around_reference").filter(|r| !r.is_empty()) {
                return design::board_footprint_keepout(
                    client,
                    &doc,
                    &reference,
                    args.float("margin_mm"),
                    layers.as_deref(),
                    rule,
                )
                .await;
            }
            let points = args.get("points");
            if points.as_array().is_none_or(|p| p.is_empty()) {
                return Err(Error::Value("give points or around_reference".into()));
            }
            // layers or board_copper_layers(spec)
            let names = match layers.filter(|l| !l.is_empty()) {
                Some(names) => names,
                None => design::board_copper_layers(client, &doc).await?,
            };
            board::board_add_keepout(client, &doc, points, &names, args.str("name"), rule).await
        }
        "live_pcb_design_rules" => {
            let changes = RuleChanges {
                minimums: MINIMUMS.iter().map(|m| (*m, args.opt_float(m))).collect(),
                track_widths: args.floats("track_widths_mm"),
                via_sizes: args.points("via_sizes_mm"),
                severities: args.dict("severities"),
            };
            design::board_rules(client, &doc, &changes).await
        }
        "live_pcb_custom_rules" => {
            let rules = args.dicts("rules").cloned();
            let names = args.opt_strs("names");
            design::board_custom_rules(
                client,
                &doc,
                args.str("action"),
                rules.as_deref(),
                names.as_deref(),
            )
            .await
        }
        "live_pcb_netclasses" => {
            let classes = args.dicts("classes").cloned();
            design::board_netclasses(
                client,
                &doc,
                classes.as_deref(),
                args.dict("assign"),
                args.dict("patterns"),
            )
            .await
        }
        "live_pcb_stackup" => board::board_stackup_layers(client, &doc).await,
        "live_pcb_set_stackup" => {
            let thickness = Some(args.get("copper_thickness_mm")).filter(|v| !v.is_null());
            let dielectrics = args.dicts("dielectrics").cloned();
            let soldermask = args.opt_str("soldermask_color");
            let silkscreen = args.opt_str("silkscreen_color");
            let finish = args.opt_str("finish");
            let changes = StackupChanges {
                copper_layers: args.opt_int("copper_layers"),
                copper_thickness: thickness,
                dielectrics: dielectrics.as_deref(),
                board_thickness: args.opt_float("board_thickness_mm"),
                soldermask_color: soldermask.as_deref(),
                silkscreen_color: silkscreen.as_deref(),
                finish: finish.as_deref(),
                impedance_controlled: args.opt_bool("impedance_controlled"),
                soldermask_thickness: None,
            };
            board::board_set_stackup(client, &doc, &changes).await
        }
        "live_pcb_impedance_width" => {
            let reference = args.opt_str("reference_layer");
            design::board_impedance_width(
                client,
                &doc,
                args.float("impedance_ohm"),
                args.str("layer"),
                args.str("structure"),
                args.opt_float("gap_mm"),
                reference.as_deref(),
            )
            .await
        }
        "live_pcb_rf_net" => {
            let nets = args.strs("nets");
            let options = RfNet {
                nets: &nets,
                netclass: args.str("netclass"),
                impedance: args.float("impedance_ohm"),
                width: args.opt_float("width_mm"),
                structure: args.str("structure"),
                gap: args.opt_float("gap_mm"),
                clearance: args.opt_float("clearance_mm"),
                signal_layer: args.str("layer"),
                keep_others_away: args.opt_float("keep_others_away_mm"),
            };
            design::board_rf_net(client, &doc, &options).await
        }
        "live_pcb_check" => {
            let categories = args.opt_strs("categories");
            design::board_check(
                client,
                &doc,
                categories.as_deref(),
                args.bool("schematic_parity"),
            )
            .await
        }
        _ => unreachable!("validated tool name"),
    }
}
