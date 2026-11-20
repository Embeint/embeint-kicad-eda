// SPDX-License-Identifier: GPL-3.0-or-later
//! Manufacturing outputs: a port of `tools/live_schematic_mcp/fab_package.py`.
//!
//! `live_pcb_export` and `live_schematic_export` send one export job built from a default with the
//! caller's options merged over it.  `live_pcb_fab_package` checks the design (saved, DRC with
//! schematic parity clean), exports every fab and assembly output into the Embeint `Outputs/`
//! layout, checks what was written and writes a manifest and a zip of the Gerbers folder.
//!
//! The defaults, the order of requests, the result keys (in order), the error texts and the file
//! checks are Python's; `rust/parity/mcp_parity.py` compares the two.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use embeint_ipc::client::{self, Client, Error};
use embeint_ipc::pbjson::{self, py_repr};
use embeint_ipc::proto::{self, enum_name, int_field, list_field, message_field, str_field};
use prost_reflect::{DynamicMessage, Value};
use serde_json::{json, Map, Value as Json};
use sha2::{Digest, Sha256};

const BJ: &str = "kiapi.board.jobs.";
const SJ: &str = "kiapi.schematic.jobs.";
const RUN_JOB_RESPONSE: &str = "kiapi.common.types.RunJobResponse";
/// Whitespace stripped from cells and lines (`BLANK`)
const BLANK: &[char] = &[' ', '\t', '\r', '\n'];
const MANIFEST: &str = "manifest.json";
const MAX_VIOLATIONS: usize = 50;
const DRC_TIMEOUT: Duration = Duration::from_secs(300);

// ---- Single exports ----------------------------------------------------------------------------

/// `BOARD_EXPORTS`: kind and request message.
const BOARD_EXPORTS: &[(&str, &str)] = &[
    ("gerbers", "RunBoardJobExportGerbers"),
    ("drill", "RunBoardJobExportDrill"),
    ("position", "RunBoardJobExportPosition"),
    ("step", "RunBoardJobExport3D"),
    ("pdf", "RunBoardJobExportPdf"),
    ("svg", "RunBoardJobExportSvg"),
    ("dxf", "RunBoardJobExportDxf"),
    ("ipc2581", "RunBoardJobExportIpc2581"),
    ("ipcd356", "RunBoardJobExportIpcD356"),
    ("odb", "RunBoardJobExportODB"),
    ("stats", "RunBoardJobExportStats"),
    ("dsn", "RunBoardJobExportSpecctraDsn"),
    ("gencad", "RunBoardJobExportGencad"),
    ("idf", "RunBoardJobExportIdf"),
];

const SCHEMATIC_EXPORTS: &[(&str, &str)] = &[
    ("svg", "RunSchematicJobExportSvg"),
    ("pdf", "RunSchematicJobExportPdf"),
    ("dxf", "RunSchematicJobExportDxf"),
    ("ps", "RunSchematicJobExportPs"),
    ("netlist", "RunSchematicJobExportNetlist"),
    ("bom", "RunSchematicJobExportBOM"),
];

fn step_default() -> Json {
    json!({"format": "B3D_STEP", "overwrite": true, "include_unspecified": true,
           "substitute_models": true, "export_board_body": true, "export_components": true})
}

/// `BOARD_EXPORT_DEFAULTS`.
fn board_export_default(kind: &str) -> Json {
    match kind {
        "gerbers" => json!({"use_board_plot_params": true, "create_gerber_job_file": true}),
        "step" => step_default(),
        "pdf" | "svg" | "dxf" => json!({"plot_settings": {
            "layers": ["BL_F_Cu", "BL_B_Cu", "BL_F_SilkS", "BL_Edge_Cuts"], "plot_drawing_sheet": true}}),
        _ => json!({}),
    }
}

/// `BOM_COLUMNS`: name, label, group by.
const BOM_COLUMNS: &[(&str, &str, bool)] = &[
    ("Reference", "Reference", false),
    ("${QUANTITY}", "Qty", false),
    ("Value", "Value", true),
    ("Footprint", "Footprint", true),
    ("Manufacturer", "Manufacturer", true),
    ("MPN", "MPN", true),
    ("Alt Manufacturer", "Alt Manufacturer", false),
    ("Alt MPN", "Alt MPN", false),
    ("Description", "Description", false),
    ("Datasheet", "Datasheet", false),
];

/// `BOM_DEFAULT`: an orderable BOM.
fn bom_default() -> Json {
    let fields: Vec<Json> = BOM_COLUMNS
        .iter()
        .map(|(name, label, group)| json!({"name": name, "label": label, "group_by": group}))
        .collect();
    json!({
        "format": {"field_delimiter": ",", "string_delimiter": "\"", "ref_delimiter": ",",
                   "ref_range_delimiter": ""},
        "fields": {"fields": fields, "sort_field": "Reference", "sort_direction": "BSD_ASCENDING"},
        "exclude_dnp": true, "group_symbols": true,
    })
}

fn schematic_export_default(kind: &str) -> Json {
    match kind {
        "netlist" => json!({"format": "SNF_KICAD_SEXPR"}),
        "bom" => bom_default(),
        _ => json!({}),
    }
}

/// `build_request`: the default, then the caller's options merged over it.
fn build_request(
    full_name: &str,
    default: &Json,
    options: Option<&Json>,
) -> Result<DynamicMessage, Error> {
    let mut request = pbjson::from_json(default, &proto::descriptor(full_name))?;
    if let Some(options) = options {
        let Json::Object(map) = options else {
            return Err(Error::Value(
                "options must be an object of protobuf JSON fields".into(),
            ));
        };
        if map.contains_key("job_settings") || map.contains_key("jobSettings") {
            return Err(Error::Value(
                "options cannot set job_settings; the document and output path come from the tool's arguments"
                    .into(),
            ));
        }
        pbjson::merge_json(options, &mut request)?;
    }
    Ok(request)
}

fn py_list<S: AsRef<str>>(items: &[S]) -> String {
    py_repr(&Json::Array(
        items.iter().map(|s| json!(s.as_ref())).collect(),
    ))
}

fn export_request(
    exports: &[(&str, &str)],
    prefix: &str,
    default: fn(&str) -> Json,
    kind: &str,
    options: Option<&Json>,
) -> Result<DynamicMessage, Error> {
    let Some((_, message)) = exports.iter().find(|(name, _)| *name == kind) else {
        let mut kinds: Vec<&str> = exports.iter().map(|(name, _)| *name).collect();
        kinds.sort_unstable();
        return Err(Error::Value(format!(
            "kind must be one of {}",
            py_list(&kinds)
        )));
    };
    build_request(&format!("{prefix}{message}"), &default(kind), options)
}

/// Sets `job_settings.document` and `job_settings.output_path`.
fn set_job(request: &mut DynamicMessage, doc: &DynamicMessage, output_path: &str) {
    let Some(Value::Message(settings)) = request.get_field_by_name_mut("job_settings") else {
        unreachable!("export requests have job settings")
    };
    settings.set_field_by_name("document", Value::Message(doc.clone()));
    settings.set_field_by_name("output_path", Value::String(output_path.to_owned()));
}

fn option_object(value: Option<&Json>) -> Option<&Json> {
    value.filter(|v| !v.is_null())
}

/// `board_export`: one output from the live board.
pub async fn board_export(
    client: &mut Client,
    doc: &DynamicMessage,
    kind: &str,
    output_path: &str,
    options: Option<&Json>,
) -> Result<Json, Error> {
    let mut request = export_request(
        BOARD_EXPORTS,
        BJ,
        board_export_default,
        kind,
        option_object(options),
    )?;
    set_job(&mut request, doc, output_path);
    let response = client.send(&request, RUN_JOB_RESPONSE).await?;
    if !job_ok(&response) {
        return Err(Error::Runtime(format!(
            "Export failed: {}",
            str_field(&response, "message")
        )));
    }
    Ok(json!({"outputs": output_paths(&response), "message": str_field(&response, "message")}))
}

/// `export`: one output from the live schematic.
pub async fn schematic_export(
    client: &mut Client,
    doc: &DynamicMessage,
    kind: &str,
    output_path: &str,
    options: Option<&Json>,
) -> Result<Json, Error> {
    let mut request = export_request(
        SCHEMATIC_EXPORTS,
        SJ,
        schematic_export_default,
        kind,
        option_object(options),
    )?;
    set_job(&mut request, doc, output_path);
    let response = client.send(&request, RUN_JOB_RESPONSE).await?;
    let message = str_field(&response, "message");
    if !job_ok(&response) {
        return Err(Error::Runtime(format!("Export failed: {message}")));
    }
    // Warnings KiCad logged while exporting (JS_WARNING)
    let mut result = Map::new();
    result.insert("outputs".into(), json!(output_paths(&response)));
    if !message.is_empty() {
        result.insert("message".into(), json!(message));
    }
    Ok(Json::Object(result))
}

/// `JOB_OK`: a job that wrote its outputs (JS_SUCCESS, or JS_WARNING with KiCad's warnings).
fn job_ok(response: &DynamicMessage) -> bool {
    matches!(int_field(response, "status"), 1 | 2)
}

fn output_paths(response: &DynamicMessage) -> Vec<Json> {
    list_field(response, "output_path")
        .into_iter()
        .map(|v| match v {
            Value::String(text) => json!(text),
            _ => json!(""),
        })
        .collect()
}

// ---- The fabrication package -------------------------------------------------------------------

/// One output of the package (`PLAN`).
struct Output {
    name: &'static str,
    folder: &'static str,
    message: String,
    default: Json,
    /// File name ({name} is the project name), or None for a folder output
    file: Option<&'static str>,
}

/// `PLAN`, in export order.
fn plan() -> Vec<Output> {
    let board = |kind: &str| {
        format!(
            "{BJ}{}",
            BOARD_EXPORTS.iter().find(|(k, _)| *k == kind).unwrap().1
        )
    };
    let schematic = |kind: &str| {
        format!(
            "{SJ}{}",
            SCHEMATIC_EXPORTS
                .iter()
                .find(|(k, _)| *k == kind)
                .unwrap()
                .1
        )
    };
    vec![
        Output {
            name: "gerbers",
            folder: "Gerbers",
            message: board("gerbers"),
            default: json!({"use_board_plot_params": false, "create_gerber_job_file": true,
                            "include_netlist_attributes": true, "use_x2_format": true, "precision": "GP_6",
                            "plot_settings": {"scale": 1.0, "drill_marks": "PDM_NONE",
                                              "plot_reference_designators": true,
                                              "plot_footprint_values": true}}),
            file: None,
        },
        Output {
            name: "drill",
            folder: "Gerbers",
            message: board("drill"),
            default: json!({"format": "DF_EXCELLON", "units": "U_MM", "origin": "DO_ABSOLUTE",
                            "zeros_format": "DZF_DECIMAL",
                            "excellon": {"mirror_y": false, "minimal_header": false, "combine_pth_npth": false},
                            "map_format": "DMF_PDF", "report_format": "DRF_STANDARD"}),
            file: None,
        },
        Output {
            name: "position",
            folder: "PnP",
            message: board("position"),
            // naked_filename: KiCad writes the given name, not <name>-pos-all-pos.csv
            default: json!({"use_drill_place_file_origin": false, "exclude_dnp": true, "single_file": true,
                            "naked_filename": true, "side": "PS_BOTH", "units": "U_MM", "format": "PF_CSV"}),
            file: Some("{name}-pos.csv"),
        },
        Output {
            name: "step",
            folder: "STEP",
            message: board("step"),
            default: step_default(),
            file: Some("{name}.step"),
        },
        Output {
            name: "assembly_pdf",
            folder: "Assembly",
            message: board("pdf"),
            default: json!({"plot_settings": {"layers": ["BL_F_Fab", "BL_B_Fab"], "common_layers": ["BL_Edge_Cuts"],
                                              "sketch_pads_on_fab_layers": true,
                                              "crossout_dnp_footprints_on_fab_layers": true,
                                              "plot_reference_designators": true, "plot_drawing_sheet": true},
                            "single_document": true, "page_mode": "BJPM_EACH_LAYER_OWN_PAGE"}),
            file: Some("{name}-assembly.pdf"),
        },
        Output {
            name: "ipc2581",
            folder: "IPC-2581",
            message: board("ipc2581"),
            default: json!({"units": "U_MM", "version": "IPC2581V_C", "manufacturer_part_number_column": "MPN",
                            "manufacturer_column": "Manufacturer"}),
            file: Some("{name}-ipc2581.xml"),
        },
        Output {
            name: "odb",
            folder: "ODB",
            message: board("odb"),
            default: json!({"units": "U_MM", "compression": "ODBC_ZIP", "manufacturer_part_number_column": "MPN"}),
            file: Some("{name}-odb.zip"),
        },
        Output {
            name: "ipcd356",
            folder: "Netlist",
            message: board("ipcd356"),
            default: json!({}),
            file: Some("{name}.d356"),
        },
        Output {
            name: "bom",
            folder: "BOM",
            message: schematic("bom"),
            default: bom_default(),
            file: Some("{name}-bom.csv"),
        },
        Output {
            name: "schematic_pdf",
            folder: "Schematic",
            message: schematic("pdf"),
            default: json!({}),
            file: Some("{name}-schematic.pdf"),
        },
    ]
}

const DEFAULT_OUTPUTS: &[&str] = &[
    "gerbers",
    "drill",
    "position",
    "step",
    "assembly_pdf",
    "bom",
    "schematic_pdf",
];
const SCHEMATIC_OUTPUTS: &[&str] = &["bom", "schematic_pdf"];
const TECHNICAL_LAYERS: &[&str] = &[
    "BL_F_Mask",
    "BL_B_Mask",
    "BL_F_Paste",
    "BL_B_Paste",
    "BL_F_SilkS",
    "BL_B_SilkS",
    "BL_Edge_Cuts",
];

/// `gerber_layers`: every copper layer of a board with this many, then the technical layers.
fn gerber_layers(copper_layers: i64) -> Vec<String> {
    let mut layers = vec!["BL_F_Cu".to_owned()];
    layers.extend((1..copper_layers - 1).map(|index| format!("BL_In{index}_Cu")));
    if copper_layers > 1 {
        layers.push("BL_B_Cu".into());
    }
    layers.extend(TECHNICAL_LAYERS.iter().map(|s| (*s).to_owned()));
    layers
}

/// `select_outputs`: the outputs to export, in export order.
fn select_outputs(plan: &[Output], outputs: Option<&[String]>) -> Result<Vec<&'static str>, Error> {
    let Some(outputs) = outputs else {
        return Ok(plan
            .iter()
            .map(|o| o.name)
            .filter(|n| DEFAULT_OUTPUTS.contains(n))
            .collect());
    };
    let unknown: Vec<&String> = outputs
        .iter()
        .filter(|o| !plan.iter().any(|p| p.name == o.as_str()))
        .collect();
    if !unknown.is_empty() {
        let names: Vec<&str> = plan.iter().map(|o| o.name).collect();
        return Err(Error::Value(format!(
            "Unknown outputs {}; choose from {}",
            py_list(&unknown),
            py_list(&names)
        )));
    }
    if outputs.is_empty() {
        return Err(Error::Value("outputs must name at least one output".into()));
    }
    Ok(plan
        .iter()
        .map(|o| o.name)
        .filter(|n| outputs.iter().any(|o| o == n))
        .collect())
}

/// `package_requests`: every selected output's request, so bad options fail before anything is sent.
fn package_requests(
    plan: &[Output],
    selected: &[&str],
    options: Option<&Json>,
) -> Result<Vec<DynamicMessage>, Error> {
    let empty = Map::new();
    let options = match option_object(options) {
        Some(Json::Object(map)) => map,
        _ => &empty,
    };
    let stray: Vec<&String> = options
        .keys()
        .filter(|k| !selected.contains(&k.as_str()))
        .collect();
    if !stray.is_empty() {
        return Err(Error::Value(format!(
            "options name outputs that are not exported: {}",
            py_list(&stray)
        )));
    }
    let mut requests = Vec::new();
    for name in selected {
        let output = output(plan, name);
        let value = option_object(options.get(*name));
        if let Some(value) = value {
            if !value.is_object() {
                return Err(Error::Value(format!(
                    "options[{}] must be an object of protobuf JSON fields",
                    py_repr(&json!(name))
                )));
            }
        }
        requests.push(build_request(&output.message, &output.default, value)?);
    }
    Ok(requests)
}

fn output<'a>(plan: &'a [Output], name: &str) -> &'a Output {
    plan.iter()
        .find(|o| o.name == name)
        .expect("planned output")
}

/// `name_stem`: the file name without folders and extension.
pub fn name_stem(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or_default();
    match base.rfind('.') {
        Some(dot) if dot > 0 => base[..dot].to_owned(),
        _ => base.to_owned(),
    }
}

fn join(directory: &str, parts: &[&str]) -> String {
    let mut all = vec![directory.trim_end_matches(['/', '\\'])];
    all.extend_from_slice(parts);
    all.join("/")
}

fn zip_name(name: &str) -> String {
    format!("{name}-gerbers.zip")
}

fn sorted_entries(path: &Path) -> Result<Vec<(String, PathBuf)>, Error> {
    let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(path)
        .map_err(io_error)?
        .filter_map(Result::ok)
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn io_error(error: std::io::Error) -> Error {
    Error::Runtime(error.to_string())
}

/// `check_output_dir`: (folders, existing files).
fn check_output_dir(
    output_dir: &str,
    plan: &[Output],
    selected: &[&str],
    name: &str,
    zip_gerbers: bool,
    overwrite: bool,
) -> Result<(Vec<&'static str>, Vec<String>), Error> {
    let root = Path::new(output_dir);
    if root.exists() && !root.is_dir() {
        return Err(Error::Value(format!("output_dir {output_dir} is a file")));
    }
    let folders: Vec<&'static str> = selected
        .iter()
        .map(|n| output(plan, n).folder)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut existing = Vec::new();
    for folder in &folders {
        let target = root.join(folder);
        if target.is_file() {
            return Err(Error::Value(format!(
                "{} is a file",
                join(output_dir, &[folder])
            )));
        }
        if target.is_dir() {
            let entries = sorted_entries(&target)?;
            if entries.iter().any(|(_, path)| path.is_dir()) {
                return Err(Error::Value(format!(
                    "{} contains folders; choose an empty output_dir",
                    join(output_dir, &[folder])
                )));
            }
            existing.extend(
                entries
                    .into_iter()
                    .map(|(entry, _)| format!("{folder}/{entry}")),
            );
        }
    }
    let mut singles = vec![MANIFEST.to_owned()];
    if zip_gerbers && folders.contains(&"Gerbers") {
        singles.push(zip_name(name));
    }
    for single in singles {
        if root.join(&single).is_dir() {
            return Err(Error::Value(format!(
                "{} is a folder",
                join(output_dir, &[&single])
            )));
        }
        if root.join(&single).is_file() {
            existing.push(single);
        }
    }
    if !existing.is_empty() && !overwrite {
        let mut shown = existing
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if existing.len() > 10 {
            shown.push_str(" ...");
        }
        return Err(Error::Value(format!(
            "{output_dir} already has outputs ({shown}); pass overwrite=true to replace them"
        )));
    }
    Ok((folders, existing))
}

/// Arguments of `live_pcb_fab_package`.
pub struct PackageArgs<'a> {
    pub board: DynamicMessage,
    pub output_dir: Option<String>,
    pub schematic: Option<DynamicMessage>,
    pub outputs: Option<Vec<String>>,
    pub options: Option<&'a Json>,
    pub checks: bool,
    pub allow_unsaved: bool,
    pub overwrite: bool,
    pub zip_gerbers: bool,
}

fn layer_number(name: &str) -> i32 {
    proto::pool()
        .get_enum_by_name("kiapi.board.types.BoardLayer")
        .and_then(|e| e.get_value_by_name(name))
        .map(|v| v.number())
        .unwrap_or_else(|| panic!("{name} is not a board layer"))
}

/// `board_drc(board, parity=True, refill=True, limit=None)`: (type, severity, description) of each
/// violation, and the ignored and excluded counts.
async fn drc(
    client: &mut Client,
    board: &DynamicMessage,
) -> Result<(Vec<(String, String, String)>, i64, i64), Error> {
    let start = proto::with_field(
        proto::build(
            "kiapi.board.tasks.StartDrcTask",
            json!({"test_schematic_parity": true, "require_zones_filled": true}),
        ),
        "board",
        board.clone(),
    );
    let started = client
        .send(&start, "kiapi.common.tasks.StartTaskResponse")
        .await?;
    let token = message_field(&started, "token");
    let status_request = proto::with_field(
        proto::build("kiapi.common.tasks.GetTaskStatus", json!({})),
        "token",
        token.clone(),
    );
    let deadline = Instant::now() + DRC_TIMEOUT;
    let timeout =
        || Error::Runtime("DRC did not finish in time; it may still be running in KiCad".into());
    loop {
        // Read-only, so a reply held up while KiCad loads libraries for the parity check is waited for again
        let status = match client
            .send(&status_request, "kiapi.common.tasks.TaskStatusResponse")
            .await
        {
            Ok(status) => status,
            Err(Error::ReplyTimeout(_)) => {
                if Instant::now() > deadline {
                    return Err(timeout());
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let state = enum_name("kiapi.common.tasks.TaskState", int_field(&status, "state"));
        if state == "TS_COMPLETED" {
            break;
        }
        if state == "TS_FAILED" || state == "TS_CANCELLED" {
            return Err(Error::Runtime(format!(
                "DRC {state}: {}",
                str_field(&status, "error_message")
            )));
        }
        if Instant::now() > deadline {
            return Err(timeout());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let result_request = proto::with_field(
        proto::build("kiapi.board.tasks.GetDrcTaskResult", json!({})),
        "token",
        token,
    );
    let result = client
        .send(&result_request, "kiapi.board.tasks.DrcTaskResult")
        .await?;
    let violations = list_field(&result, "violations")
        .into_iter()
        .filter_map(|v| match v {
            Value::Message(marker) => Some(marker),
            _ => None,
        })
        .map(|marker| {
            let kind = enum_name("kiapi.board.DrcErrorType", int_field(&marker, "error_type"));
            let kind = kind.strip_prefix("DRCET_").unwrap_or(&kind);
            let kind = kind.strip_prefix("DRCE_").unwrap_or(kind).to_lowercase();
            let severity = enum_name(
                "kiapi.common.types.RuleSeverity",
                int_field(&marker, "severity"),
            );
            let severity = severity.strip_prefix("RS_").unwrap_or(&severity).to_owned();
            (kind, severity, str_field(&marker, "description"))
        })
        .collect();
    Ok((
        violations,
        int_field(&result, "ignored_count"),
        int_field(&result, "excluded_count"),
    ))
}

/// `drc_summary`.
fn drc_summary(violations: &[(String, String, String)], ignored: i64, excluded: i64) -> Json {
    let mut by_type = Map::new();
    for (kind, _, _) in violations {
        let count = by_type.get(kind).and_then(Json::as_i64).unwrap_or(0);
        by_type.insert(kind.clone(), json!(count + 1));
    }
    let shown: Vec<Json> = violations
        .iter()
        .take(MAX_VIOLATIONS)
        .map(|(t, s, d)| json!({"type": t, "severity": s, "description": d}))
        .collect();
    let mut summary = Map::new();
    summary.insert(
        "errors".into(),
        json!(violations.iter().filter(|v| v.1 == "ERROR").count()),
    );
    summary.insert(
        "warnings".into(),
        json!(violations.iter().filter(|v| v.1 == "WARNING").count()),
    );
    summary.insert("by_type".into(), Json::Object(by_type));
    summary.insert("ignored".into(), json!(ignored));
    summary.insert("excluded".into(), json!(excluded));
    summary.insert("violations".into(), Json::Array(shown));
    summary.insert(
        "violations_truncated".into(),
        json!(violations.len() > MAX_VIOLATIONS),
    );
    Json::Object(summary)
}

/// `match_schematic`: the open schematic of the board's project.
fn match_schematic(documents: &[Json], board: &DynamicMessage) -> Result<Json, Error> {
    // KiCad reports the schematic's project path with a trailing separator and the board's without
    let board_path = str_field(&message_field(board, "project"), "path");
    let path = board_path.trim_end_matches(['/', '\\']);
    documents
        .iter()
        .find(|d| {
            let other = d.get("project").and_then(|p| p.get("path")).and_then(Json::as_str).unwrap_or("");
            path.is_empty() || other.trim_end_matches(['/', '\\']) == path
        })
        .cloned()
        .ok_or_else(|| {
            Error::Value(
                "No schematic of this project is open; open it with live_schematic_open or pass schematic_document"
                    .into(),
            )
        })
}

/// `fab_package`.
pub async fn fab_package(client: &mut Client, args: PackageArgs<'_>) -> Result<Json, Error> {
    let plan = plan();
    let board = args.board;
    let selected = select_outputs(&plan, args.outputs.as_deref())?;
    let mut requests = package_requests(&plan, &selected, args.options)?;
    let project = message_field(&board, "project");
    let board_filename = str_field(&board, "board_filename");
    let mut name = str_field(&project, "name");
    if name.is_empty() {
        name = name_stem(&board_filename);
    }
    if name.is_empty() {
        name = "board".into();
    }
    let output_dir = match args.output_dir.filter(|d| !d.is_empty()) {
        Some(dir) => dir,
        None => {
            let path = str_field(&project, "path");
            if path.is_empty() {
                return Err(Error::Value(
                    "output_dir is required: the board document names no project path".into(),
                ));
            }
            join(&path, &["Outputs"])
        }
    };
    let zip_gerbers = args.zip_gerbers
        && selected
            .iter()
            .any(|n| output(&plan, n).folder == "Gerbers");
    let (folders, existing) = check_output_dir(
        &output_dir,
        &plan,
        &selected,
        &name,
        zip_gerbers,
        args.overwrite,
    )?;

    // The design must be what is saved, and clean
    let board_state = client.document_revision(&board).await?;
    let mut unsaved: Vec<String> = Vec::new();
    if board_state["modified"] == json!(true) {
        unsaved.push(board_filename.clone());
    }
    let mut schematic = args.schematic;
    let mut schematic_state = None;
    if selected.iter().any(|n| SCHEMATIC_OUTPUTS.contains(n)) {
        if schematic.is_none() {
            let request = proto::build(
                "kiapi.common.commands.GetOpenDocuments",
                json!({"type": "DOCTYPE_SCHEMATIC"}),
            );
            let found = match client
                .send(&request, "kiapi.common.commands.GetOpenDocumentsResponse")
                .await
            {
                Ok(opened) => pbjson::to_json(&opened).map_err(|e| Error::Decode(e.0))?,
                Err(error)
                    if error.is_runtime_error() && error.to_string().contains("AS_UNHANDLED") =>
                {
                    json!({})
                }
                Err(error) => return Err(error),
            };
            let documents = found
                .get("documents")
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            schematic = Some(client::schematic_document(&match_schematic(
                &documents, &board,
            )?)?);
        }
        let sheet = schematic.as_ref().expect("schematic");
        let state = client.document_revision(sheet).await?;
        if state["modified"] == json!(true) {
            unsaved.push(format!(
                "{}.kicad_sch",
                str_field(&message_field(sheet, "project"), "name")
            ));
        }
        schematic_state = Some(state);
    }
    let mut result = Map::new();
    result.insert("exported".into(), json!(false));
    result.insert("output_dir".into(), json!(output_dir));
    result.insert("project".into(), json!(name));
    result.insert("board_revision".into(), board_state["revision"].clone());
    result.insert(
        "schematic_revision".into(),
        schematic_state
            .as_ref()
            .map_or(Json::Null, |s| s["revision"].clone()),
    );
    result.insert("unsaved".into(), json!(unsaved));
    result.insert("checks".into(), Json::Null);
    if !unsaved.is_empty() && !args.allow_unsaved {
        result.insert("blocked_by".into(), json!("unsaved"));
        result.insert(
            "message".into(),
            json!("Save the design first (live_pcb_save / live_schematic_save) so the outputs match the files, or pass allow_unsaved=true"),
        );
        return Ok(Json::Object(result));
    }
    if args.checks {
        let (violations, ignored, excluded) = drc(client, &board).await?;
        let summary = drc_summary(&violations, ignored, excluded);
        let errors = summary["errors"].as_u64().unwrap_or(0);
        result.insert("checks".into(), json!({"drc": summary}));
        if errors > 0 {
            result.insert("blocked_by".into(), json!("drc"));
            result.insert(
                "message".into(),
                json!("DRC (with schematic parity) reports errors; fix them (live_pcb_drc lists where) or pass checks=false"),
            );
            return Ok(Json::Object(result));
        }
    }
    let mut copper_layers = None;
    if let Some(index) = selected.iter().position(|n| *n == "gerbers") {
        let request = proto::with_field(
            proto::build("kiapi.board.commands.GetBoardEnabledLayers", json!({})),
            "board",
            board.clone(),
        );
        let enabled = client
            .send(&request, "kiapi.board.commands.BoardEnabledLayersResponse")
            .await?;
        let count = int_field(&enabled, "copper_layer_count");
        copper_layers = Some(count);
        let Some(Value::Message(settings)) = requests[index].get_field_by_name_mut("plot_settings")
        else {
            unreachable!("gerber requests have plot settings")
        };
        if list_field(settings, "layers").is_empty() {
            let layers = gerber_layers(count)
                .iter()
                .map(|n| Value::EnumNumber(layer_number(n)))
                .collect();
            settings.set_field_by_name("layers", Value::List(layers));
        }
    }

    // Export, each output into its folder
    let root = Path::new(&output_dir);
    for item in &existing {
        std::fs::remove_file(root.join(item)).map_err(io_error)?;
    }
    for folder in &folders {
        std::fs::create_dir_all(root.join(folder)).map_err(io_error)?;
    }
    let mut produced: Vec<(&str, Vec<String>)> = Vec::new();
    let mut kicad_warnings: Vec<String> = Vec::new();
    for (index, name_of_output) in selected.iter().enumerate() {
        let planned = output(&plan, name_of_output);
        let target = match planned.file {
            Some(file) => join(
                &output_dir,
                &[planned.folder, &file.replace("{name}", &name)],
            ),
            None => join(&output_dir, &[planned.folder]) + "/",
        };
        let doc = if SCHEMATIC_OUTPUTS.contains(name_of_output) {
            schematic.as_ref().expect("schematic")
        } else {
            &board
        };
        let request = &mut requests[index];
        set_job(request, doc, &target);
        let folder = root.join(planned.folder);
        let before: BTreeSet<String> = sorted_entries(&folder)?
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        let response = client.send(request, RUN_JOB_RESPONSE).await?;
        let message = str_field(&response, "message");
        if !job_ok(&response) {
            return Err(Error::Runtime(format!(
                "{name_of_output} export failed: {message}"
            )));
        }
        let new: Vec<String> = sorted_entries(&folder)?
            .into_iter()
            .filter(|(n, path)| !before.contains(n) && path.is_file())
            .map(|(n, _)| n)
            .collect();
        if new.is_empty() {
            let kicad = if message.is_empty() {
                String::new()
            } else {
                format!("; KiCad: {message}")
            };
            return Err(Error::Runtime(format!(
                "{name_of_output} export reported success but wrote nothing in {}{kicad}",
                join(&output_dir, &[planned.folder])
            )));
        }
        produced.push((
            name_of_output,
            new.into_iter()
                .map(|n| format!("{}/{n}", planned.folder))
                .collect(),
        ));
        if !message.is_empty() {
            kicad_warnings.push(format!("{name_of_output}: KiCad: {message}"));
        }
    }

    let (verification, verify_warnings) = verify(root, &plan, &produced, copper_layers, &name)?;
    let mut warnings = kicad_warnings;
    warnings.extend(verify_warnings);
    let mut files: Vec<String> = produced
        .iter()
        .flat_map(|(_, paths)| paths.iter().cloned())
        .collect();
    files.sort();
    let mut archive = Json::Null;
    if zip_gerbers {
        let zip = zip_name(&name);
        let mut entries = Vec::new();
        for path in files.iter().filter(|p| p.starts_with("Gerbers/")) {
            let member = path
                .split_once('/')
                .map_or(path.as_str(), |(_, rest)| rest)
                .to_owned();
            entries.push((member, std::fs::read(root.join(path)).map_err(io_error)?));
        }
        std::fs::write(root.join(&zip), stored_zip(&entries)?).map_err(io_error)?;
        files.push(zip.clone());
        archive = json!(zip);
    }
    let outputs_json = Json::Object(
        produced
            .iter()
            .map(|(n, paths)| ((*n).to_owned(), json!(paths)))
            .collect(),
    );
    let mut file_entries = Vec::new();
    for path in &files {
        let data = std::fs::read(root.join(path)).map_err(io_error)?;
        file_entries.push(json!({"path": path, "bytes": data.len(), "sha256": hex_sha256(&data)}));
    }
    let manifest = json!({
        "generator": "live_pcb_fab_package", "project": name, "board": board_filename,
        "board_revision": result["board_revision"], "schematic_revision": result["schematic_revision"],
        "unsaved": result["unsaved"], "outputs": outputs_json, "checks": result["checks"],
        "verification": verification, "warnings": warnings, "files": file_entries,
    });
    std::fs::write(root.join(MANIFEST), manifest_text(&manifest)).map_err(io_error)?;
    result.insert("exported".into(), json!(true));
    result.insert("outputs".into(), outputs_json);
    result.insert("verification".into(), verification);
    result.insert("warnings".into(), json!(warnings));
    result.insert("zip".into(), archive);
    result.insert("manifest".into(), json!(MANIFEST));
    result.insert("files".into(), json!(files.len() + 1));
    Ok(Json::Object(result))
}

// ---- Reading what was written --------------------------------------------------------------------

fn read_text(path: &Path) -> Result<String, Error> {
    Ok(String::from_utf8_lossy(&std::fs::read(path).map_err(io_error)?).into_owned())
}

/// `parse_csv`: rows of comma-separated values with "quoted" cells; blank rows are dropped.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .chars()
        .collect();
    let (mut rows, mut row, mut cell, mut quoted, mut index) =
        (Vec::new(), Vec::new(), String::new(), false, 0);
    while index < chars.len() {
        let c = chars[index];
        if quoted {
            if c == '"' {
                if index + 1 < chars.len() && chars[index + 1] == '"' {
                    cell.push('"');
                    index += 1;
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == ',' {
            row.push(std::mem::take(&mut cell));
        } else if c == '\r' || c == '\n' {
            if c == '\r' && index + 1 < chars.len() && chars[index + 1] == '\n' {
                index += 1;
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else {
            cell.push(c);
        }
        index += 1;
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.into_iter()
        .filter(|r| r.iter().any(|c| !c.trim_matches(BLANK).is_empty()))
        .collect()
}

fn column(header: &[String], label: &str) -> Option<usize> {
    header.iter().position(|c| c.trim_matches(BLANK) == label)
}

fn cell(row: &[String], index: Option<usize>) -> String {
    match index {
        Some(i) if i < row.len() => row[i].trim_matches(BLANK).to_owned(),
        _ => String::new(),
    }
}

fn split_refs(text: &str) -> Vec<String> {
    text.split(',')
        .map(|r| r.trim_matches(BLANK))
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
        .collect()
}

fn shorten(items: &[String]) -> String {
    let mut text = items
        .iter()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > 20 {
        text.push_str(&format!(" and {} more", items.len() - 20));
    }
    text
}

fn check_gerbers(
    directory: &Path,
    files: &[String],
    copper_layers: Option<i64>,
    warnings: &mut Vec<String>,
) -> Result<Json, Error> {
    let jobs: Vec<&String> = files.iter().filter(|f| f.ends_with(".gbrjob")).collect();
    let mut result = Map::new();
    result.insert(
        "files".into(),
        json!(files.iter().filter(|f| !f.ends_with(".gbrjob")).count()),
    );
    result.insert(
        "job_file".into(),
        jobs.first().map_or(Json::Null, |j| json!(j)),
    );
    result.insert("copper_layers".into(), Json::Null);
    result.insert("functions".into(), json!([]));
    let Some(job_file) = jobs.first() else {
        warnings.push("gerbers: no Gerber job file (.gbrjob) was written".into());
        return Ok(Json::Object(result));
    };
    let Ok(job) = serde_json::from_str::<Json>(&read_text(&directory.join(job_file))?) else {
        warnings.push("gerbers: the Gerber job file is not valid JSON".into());
        return Ok(Json::Object(result));
    };
    let functions: Vec<String> = match job.get("FilesAttributes") {
        Some(Json::Array(attributes)) => attributes
            .iter()
            .filter(|a| a.is_object())
            .filter_map(|a| match a.get("FileFunction") {
                None => Some(String::new()),
                Some(Json::String(text)) => Some(text.clone()),
                Some(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let copper = functions.iter().filter(|f| f.starts_with("Copper")).count();
    result.insert("copper_layers".into(), json!(copper));
    result.insert("functions".into(), json!(functions));
    if let Some(expected) = copper_layers {
        if copper as i64 != expected {
            warnings.push(format!(
                "gerbers: {copper} copper layers plotted, the board has {expected}"
            ));
        }
    }
    if !functions.iter().any(|f| f.starts_with("Profile")) {
        warnings.push("gerbers: no board outline (Edge.Cuts profile) was plotted".into());
    }
    Ok(Json::Object(result))
}

/// `parse_excellon`: (tools, holes) in an Excellon drill file.
pub fn parse_excellon(text: &str) -> (usize, usize) {
    let (mut tools, mut holes, mut header) = (0, 0, false);
    for raw in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let line = raw.trim_matches(BLANK);
        if line == "M48" {
            header = true;
        } else if header {
            if line == "%" || line == "M95" {
                header = false;
            } else if line.starts_with('T') && line.contains('C') {
                tools += 1;
            }
        } else if line.starts_with('X') || line.starts_with('Y') || line.starts_with("G00") {
            holes += 1;
        }
    }
    (tools, holes)
}

fn check_drill(
    directory: &Path,
    files: &[String],
    warnings: &mut Vec<String>,
) -> Result<Json, Error> {
    let (mut per_file, mut total, mut other) = (Map::new(), 0, Vec::new());
    for name in files {
        if name.to_lowercase().ends_with(".drl") {
            let (tools, holes) = parse_excellon(&read_text(&directory.join(name))?);
            per_file.insert(name.clone(), json!({"tools": tools, "holes": holes}));
            total += holes;
        } else {
            other.push(name.clone());
        }
    }
    if per_file.is_empty() {
        warnings.push("drill: no Excellon drill file (.drl) was written".into());
    } else if total == 0 {
        warnings.push("drill: the drill files have no holes".into());
    }
    Ok(json!({"files": per_file, "holes": total, "other_files": other}))
}

fn check_position(path: &Path, warnings: &mut Vec<String>) -> Result<(Json, Vec<String>), Error> {
    let rows = parse_csv(&read_text(path)?);
    let Some((header, body)) = rows.split_first() else {
        warnings.push("position: the placement file is empty".into());
        return Ok((json!({"parts": 0, "top": 0, "bottom": 0}), Vec::new()));
    };
    let (reference, side) = (column(header, "Ref"), column(header, "Side"));
    let sides: Vec<String> = body
        .iter()
        .map(|row| cell(row, side).to_lowercase())
        .collect();
    let refs = body
        .iter()
        .map(|row| cell(row, reference))
        .filter(|r| !r.is_empty())
        .collect();
    Ok((
        json!({"parts": body.len(), "top": sides.iter().filter(|s| *s == "top").count(),
               "bottom": sides.iter().filter(|s| *s == "bottom").count()}),
        refs,
    ))
}

fn check_bom(path: &Path, warnings: &mut Vec<String>) -> Result<(Json, Vec<String>), Error> {
    let rows = parse_csv(&read_text(path)?);
    let Some((header, body)) = rows.split_first() else {
        warnings.push("bom: the BOM is empty".into());
        return Ok((
            json!({"lines": 0, "parts": 0, "lines_without_mpn": 0}),
            Vec::new(),
        ));
    };
    let (reference, qty, mpn) = (
        column(header, "Reference"),
        column(header, "Qty"),
        column(header, "MPN"),
    );
    let (mut refs, mut parts, mut missing) = (Vec::new(), 0u64, Vec::new());
    for row in body {
        let line_refs = split_refs(&cell(row, reference));
        let count = cell(row, qty);
        parts +=
            if !count.is_empty() && count.len() <= 9 && count.bytes().all(|b| b.is_ascii_digit()) {
                count.parse::<u64>().unwrap_or(0)
            } else {
                line_refs.len() as u64
            };
        if mpn.is_some() && cell(row, mpn).is_empty() {
            if line_refs.is_empty() {
                missing.push("?".to_owned());
            } else {
                missing.extend(line_refs.iter().cloned());
            }
        }
        refs.extend(line_refs);
    }
    if mpn.is_none() {
        warnings.push("bom: no MPN column; order codes cannot be checked".into());
    } else if !missing.is_empty() {
        warnings.push(format!(
            "bom: {} part(s) without an MPN: {}",
            missing.len(),
            shorten(&missing)
        ));
    }
    Ok((
        json!({"lines": body.len(), "parts": parts, "lines_without_mpn": missing.len()}),
        refs,
    ))
}

fn check_magic(
    path: &Path,
    magic: &str,
    label: &str,
    warnings: &mut Vec<String>,
) -> Result<(), Error> {
    use std::io::Read;
    let mut start = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(magic.len() as u64).read_to_end(&mut start))
        .map_err(io_error)?;
    if start != magic.as_bytes() {
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        warnings.push(format!("{label}: {file} is not a {magic} file"));
    }
    Ok(())
}

/// `csv_file`: the expected name, else the only CSV written, else None.
fn csv_file<'a>(names: &'a [String], expected: &str) -> Option<&'a str> {
    if let Some(name) = names.iter().find(|n| *n == expected) {
        return Some(name);
    }
    let tables: Vec<&String> = names
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".csv"))
        .collect();
    match tables.as_slice() {
        [only] => Some(only.as_str()),
        _ => None,
    }
}

/// `verify`: what each output contains, and warnings.
fn verify(
    root: &Path,
    plan: &[Output],
    outputs: &[(&str, Vec<String>)],
    copper_layers: Option<i64>,
    name: &str,
) -> Result<(Json, Vec<String>), Error> {
    let mut warnings = Vec::new();
    let mut result = Map::new();
    for (output, paths) in outputs {
        for path in paths {
            if std::fs::metadata(root.join(path)).map_err(io_error)?.len() == 0 {
                warnings.push(format!("{output}: {path} is empty"));
            }
        }
    }
    let (mut pnp_refs, mut bom_refs) = (None, None);
    for (output_name, paths) in outputs {
        let planned = output(plan, output_name);
        let folder = root.join(planned.folder);
        let names: Vec<String> = paths
            .iter()
            .map(|p| {
                p.split_once('/')
                    .map_or(p.clone(), |(_, rest)| rest.to_owned())
            })
            .collect();
        let target = planned
            .file
            .map(|f| f.replace("{name}", name))
            .unwrap_or_default();
        match *output_name {
            "gerbers" => {
                result.insert(
                    "gerbers".into(),
                    check_gerbers(&folder, &names, copper_layers, &mut warnings)?,
                );
            }
            "drill" => {
                result.insert("drill".into(), check_drill(&folder, &names, &mut warnings)?);
            }
            "position" | "bom" => match csv_file(&names, &target) {
                None => warnings.push(format!(
                    "{output_name}: no single CSV file to check ({})",
                    names.join(", ")
                )),
                Some(file) if *output_name == "position" => {
                    let (summary, refs) = check_position(&folder.join(file), &mut warnings)?;
                    result.insert("position".into(), summary);
                    pnp_refs = Some(refs);
                }
                Some(file) => {
                    let (summary, refs) = check_bom(&folder.join(file), &mut warnings)?;
                    result.insert("bom".into(), summary);
                    bom_refs = Some(refs);
                }
            },
            "assembly_pdf" | "schematic_pdf" => {
                for item in &names {
                    check_magic(&folder.join(item), "%PDF", output_name, &mut warnings)?;
                }
            }
            "step" => {
                for item in &names {
                    check_magic(
                        &folder.join(item),
                        "ISO-10303-21",
                        output_name,
                        &mut warnings,
                    )?;
                }
            }
            _ => {}
        }
    }
    if let (Some(pnp), Some(bom)) = (pnp_refs, bom_refs) {
        let pnp: BTreeSet<String> = pnp.into_iter().collect();
        let bom: BTreeSet<String> = bom.into_iter().collect();
        let bom_only: Vec<String> = bom.difference(&pnp).cloned().collect();
        let pnp_only: Vec<String> = pnp.difference(&bom).cloned().collect();
        if !bom_only.is_empty() {
            warnings.push(format!(
                "bom: {} part(s) in the BOM have no placement: {}",
                bom_only.len(),
                shorten(&bom_only)
            ));
        }
        result.insert(
            "bom_vs_position".into(),
            json!({"bom_only": bom_only, "position_only": pnp_only}),
        );
    }
    Ok((Json::Object(result), warnings))
}

fn hex_sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// zlib's CRC-32.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// `stored_zip`: a deterministic ZIP archive (stored, dated 1980-01-01).
pub fn stored_zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in entries {
        if data.len() > 0xFFFF_FFFF || out.len() > 0xFFFF_FFFF {
            return Err(Error::Value(
                "the Gerber archive would need ZIP64; leave zip_gerbers off".into(),
            ));
        }
        let encoded = name.as_bytes();
        let flags: u16 = if name.is_ascii() { 0 } else { 0x0800 };
        let crc = crc32(data);
        let (size, name_len, offset) = (data.len() as u32, encoded.len() as u16, out.len() as u32);
        out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        for value in [20u16, flags, 0, 0, 0x21] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name_len, 0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(encoded);
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        for value in [20u16, 20, flags, 0, 0, 0x21] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0u32, offset] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(encoded);
    }
    let (central_size, central_offset, count) =
        (central.len() as u32, out.len() as u32, entries.len() as u16);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
    for value in [0u16, 0, count, count] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [central_size, central_offset] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

fn sort_keys(value: &Json) -> Json {
    match value {
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Json::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sort_keys(&map[k])))
                    .collect(),
            )
        }
        Json::Array(items) => Json::Array(items.iter().map(sort_keys).collect()),
        other => other.clone(),
    }
}

/// `manifest_text`: `json.dumps(indent=2, sort_keys=True, ensure_ascii=False)` and a newline.
fn manifest_text(manifest: &Json) -> String {
    serde_json::to_string_pretty(&sort_keys(manifest)).expect("JSON") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_follow_python() {
        assert_eq!(
            gerber_layers(4)[..5],
            ["BL_F_Cu", "BL_In1_Cu", "BL_In2_Cu", "BL_B_Cu", "BL_F_Mask"]
        );
        assert_eq!(gerber_layers(1)[..2], ["BL_F_Cu", "BL_F_Mask"]);
        assert_eq!(name_stem("C:\\x\\demo.kicad_pcb"), "demo");
        assert_eq!(name_stem(".hidden"), ".hidden");
        assert_eq!(
            parse_csv("\"a,b\",c\r\n\r\n\"d\"\"e\",\n"),
            vec![
                vec!["a,b".to_owned(), "c".into()],
                vec!["d\"e".into(), String::new()]
            ]
        );
        assert_eq!(
            parse_excellon("M48\r\nT1C0.3\r\n%\r\nT1\r\nX1Y1G85X2Y1\r\nM30\r\n"),
            (1, 1)
        );
        assert_eq!(crc32(b"hello"), 0x3610_A686);
        assert_eq!(
            join("C:\\x\\", &["Outputs", "Gerbers"]),
            "C:\\x/Outputs/Gerbers"
        );
        let manifest = manifest_text(&json!({"b": [], "a": {"z": 1, "y": "é"}}));
        assert_eq!(
            manifest,
            "{\n  \"a\": {\n    \"y\": \"é\",\n    \"z\": 1\n  },\n  \"b\": []\n}\n"
        );
    }

    #[test]
    fn defaults_build_and_options_merge() {
        for (kind, _) in BOARD_EXPORTS {
            export_request(BOARD_EXPORTS, BJ, board_export_default, kind, None).unwrap();
        }
        let bom = export_request(
            SCHEMATIC_EXPORTS,
            SJ,
            schematic_export_default,
            "bom",
            Some(&json!({"exclude_dnp": false})),
        )
        .unwrap();
        let value = pbjson::to_json(&bom).unwrap();
        assert_eq!(value["exclude_dnp"], Json::Null);
        assert_eq!(value["group_symbols"], json!(true));
        let error =
            export_request(BOARD_EXPORTS, BJ, board_export_default, "brd", None).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("kind must be one of ['drill', 'dsn', "),
            "{error}"
        );
        assert!(build_request(
            &format!("{BJ}RunBoardJobExportDrill"),
            &json!({}),
            Some(&json!({"jobSettings": {}}))
        )
        .is_err());
        for output in plan() {
            build_request(&output.message, &output.default, None).unwrap();
        }
    }
}
