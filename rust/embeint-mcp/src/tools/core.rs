// SPDX-License-Identifier: GPL-3.0-or-later
//! The first native tools: capabilities, document reads, the revision contract, schematic item
//! edits, the generic command registry and batches, item schemas, exports and the fab package.

use embeint_ipc::client::{self, Error};
use embeint_ipc::registry;
use serde_json::Value as Json;

use super::args::{opt, req, Arg, Args, Default, Param};
use super::{begin_guard, end_guard, Ctx, Output};
use crate::{capabilities, fab};

const DOCUMENT: &[Param] = sig![req("document_spec", Arg::Dict)];
const GUARD: [Param; 2] = [
    opt("expected_revision", Arg::OptStr, Default::None),
    opt("operation_id", Arg::OptStr, Default::None),
];

pub fn params(tool: &str) -> Option<&'static [Param]> {
    Some(match tool {
        "live_mcp_capabilities"
        | "live_schematic_documents"
        | "live_pcb_documents"
        | "live_schematic_list_commands" => sig![],
        "live_schematic_describe_command" => sig![req("command", Arg::Str)],
        "live_schematic_item_schema" | "live_pcb_item_schema" => sig![req("type_name", Arg::Str)],
        "live_schematic_execute" => sig![
            req("command", Arg::Str),
            opt("payload", Arg::OptDict, Default::None),
            GUARD[0],
            GUARD[1],
        ],
        "live_schematic_batch" => sig![
            req("document_spec", Arg::Dict),
            req("steps", Arg::ListDict),
            opt("description", Arg::Str, Default::Str("MCP: batch")),
            GUARD[0],
            GUARD[1],
        ],
        "live_schematic_hierarchy" | "live_schematic_items" | "live_document_revision" => DOCUMENT,
        "live_operation_status" => sig![req("operation_id", Arg::Str)],
        "live_schematic_create_items" | "live_schematic_update_items" => sig![
            req("document_spec", Arg::Dict),
            req("items", Arg::ListDict),
            GUARD[0],
            GUARD[1],
        ],
        "live_schematic_delete_items" => sig![
            req("document_spec", Arg::Dict),
            req("ids", Arg::ListStr),
            GUARD[0],
            GUARD[1],
        ],
        "live_schematic_export" => sig![
            req("document_spec", Arg::Dict),
            req("kind", Arg::Str),
            req("output_path", Arg::Str),
            opt("options", Arg::OptDict, Default::None),
        ],
        "live_pcb_export" => sig![
            req("board_document", Arg::Dict),
            req("kind", Arg::Str),
            req("output_path", Arg::Str),
            opt("options", Arg::OptDict, Default::None),
        ],
        "live_pcb_fab_package" => sig![
            req("board_document", Arg::Dict),
            opt("output_dir", Arg::OptStr, Default::None),
            opt("schematic_document", Arg::OptDict, Default::None),
            opt("outputs", Arg::OptListStr, Default::None),
            opt("options", Arg::OptDict, Default::None),
            opt("checks", Arg::Bool, Default::Bool(true)),
            opt("allow_unsaved", Arg::Bool, Default::Bool(false)),
            opt("overwrite", Arg::Bool, Default::Bool(false)),
            opt("zip_gerbers", Arg::Bool, Default::Bool(true)),
        ],
        _ => return None,
    })
}

pub async fn run(name: &str, args: &Args, ctx: &Ctx<'_>) -> Option<Result<Output, Error>> {
    params(name)?;
    Some(run_tool(name, args, ctx).await)
}

async fn run_tool(name: &str, args: &Args, ctx: &Ctx<'_>) -> Result<Output, Error> {
    let client = ctx.client;
    let document = || args.get("document_spec").clone();
    Ok(match name {
        "live_mcp_capabilities" => Output::Dict(capabilities::manifest()),
        "live_schematic_list_commands" => Output::List(registry::list()),
        "live_schematic_describe_command" => {
            Output::Dict(registry::describe(registry::resolve(args.str("command"))?))
        }
        "live_schematic_item_schema" | "live_pcb_item_schema" => Output::Dict(
            registry::item_schema(args.str("type_name"), name == "live_pcb_item_schema")?,
        ),
        "live_schematic_execute" => {
            let mut client = client.lock().await;
            begin_guard(&mut client, args);
            let result = client
                .execute(args.str("command"), args.get("payload"))
                .await;
            Output::Dict(end_guard(&mut client, result)?)
        }
        "live_schematic_batch" => {
            let doc = client::editor_document(&document())?;
            let steps = args.dicts("steps").cloned().unwrap_or_default();
            let mut client = client.lock().await;
            begin_guard(&mut client, args);
            let result = client.batch(&doc, &steps, args.str("description")).await;
            Output::Dict(end_guard(&mut client, result)?)
        }
        "live_schematic_documents" => Output::Dict(client.lock().await.documents().await?),
        "live_pcb_documents" => match client.lock().await.boards().await? {
            Json::Array(items) => Output::List(items),
            other => Output::List(vec![other]),
        },
        "live_schematic_hierarchy" => {
            let spec = client::schematic_document(&document())?;
            Output::Dict(client.lock().await.hierarchy(&spec).await?)
        }
        "live_schematic_items" => {
            let spec = client::schematic_document(&document())?;
            Output::Dict(client.lock().await.items(&spec).await?)
        }
        "live_document_revision" => {
            let spec = client::editor_document(&document())?;
            Output::Dict(client.lock().await.document_revision(&spec).await?)
        }
        "live_operation_status" => {
            let operation_id = args.opt_str("operation_id").unwrap_or_default();
            Output::Dict(client.lock().await.operation_status(&operation_id).await?)
        }
        "live_schematic_create_items"
        | "live_schematic_update_items"
        | "live_schematic_delete_items" => {
            // Arguments are checked before the editor is contacted, as in server.py
            let spec = client::schematic_document(&document())?;
            let values = args.dicts("items").cloned().unwrap_or_default();
            let items = client::parse_schematic_items(&values)?;
            let mut client = client.lock().await;
            begin_guard(&mut client, args);
            let result = match name {
                "live_schematic_update_items" => client.update_items(&spec, &items).await,
                "live_schematic_delete_items" => {
                    client.delete_items(&spec, &args.strs("ids")).await
                }
                _ => client.create_items(&spec, &items).await,
            };
            Output::Dict(end_guard(&mut client, result)?)
        }
        "live_pcb_export" => {
            let doc = client::board_document(args.get("board_document"))?;
            let mut client = client.lock().await;
            Output::Dict(
                fab::board_export(
                    &mut client,
                    &doc,
                    args.str("kind"),
                    args.str("output_path"),
                    args.0.get("options"),
                )
                .await?,
            )
        }
        "live_schematic_export" => {
            let doc = client::schematic_document(&document())?;
            let mut client = client.lock().await;
            Output::Dict(
                fab::schematic_export(
                    &mut client,
                    &doc,
                    args.str("kind"),
                    args.str("output_path"),
                    args.0.get("options"),
                )
                .await?,
            )
        }
        "live_pcb_fab_package" => {
            let board = client::board_document(args.get("board_document"))?;
            let schematic = match args.get("schematic_document") {
                Json::Null => None,
                value => Some(client::schematic_document(value)?),
            };
            let package = fab::PackageArgs {
                board,
                output_dir: args.opt_str("output_dir"),
                schematic,
                outputs: args.opt_strs("outputs"),
                options: args.0.get("options"),
                checks: args.bool("checks"),
                allow_unsaved: args.bool("allow_unsaved"),
                overwrite: args.bool("overwrite"),
                zip_gerbers: args.bool("zip_gerbers"),
            };
            let mut client = client.lock().await;
            Output::Dict(fab::fab_package(&mut client, package).await?)
        }
        _ => unreachable!("validated tool name"),
    })
}

#[cfg(test)]
mod tests {
    use super::super::args::validate;
    use super::*;
    use serde_json::json;

    #[test]
    fn validation_follows_fastmcp() {
        let spec = params("live_schematic_create_items").unwrap();
        let ok = validate(
            "live_schematic_create_items",
            spec,
            &json!({
                "document_spec": "{\"type\": \"DOCTYPE_SCHEMATIC\"}",
                "items": "[{\"@type\": \"x\"}]",
                "extra": 1,
            }),
        )
        .unwrap();
        assert_eq!(
            ok.get("document_spec"),
            &json!({"type": "DOCTYPE_SCHEMATIC"})
        );
        assert_eq!(ok.get("items"), &json!([{"@type": "x"}]));
        assert_eq!(ok.get("expected_revision"), &Json::Null);
        // "null" decodes to None for an optional string; JSON numbers stay text and are refused
        let ok = validate(
            "live_schematic_create_items",
            spec,
            &json!({"document_spec": {}, "items": [], "operation_id": "null"}),
        )
        .unwrap();
        assert_eq!(ok.get("operation_id"), &Json::Null);
        let error =
            validate("live_schematic_create_items", spec, &json!({"items": [1]})).unwrap_err();
        assert!(
            error.starts_with("2 validation errors for live_schematic_create_itemsArguments"),
            "{error}"
        );
        // A plain str parameter is never decoded
        let status = params("live_operation_status").unwrap();
        let ok = validate(
            "live_operation_status",
            status,
            &json!({"operation_id": "[1]"}),
        )
        .unwrap();
        assert_eq!(ok.get("operation_id"), &json!("[1]"));
        assert!(validate("live_operation_status", status, &json!({"operation_id": 5})).is_err());
    }
}
