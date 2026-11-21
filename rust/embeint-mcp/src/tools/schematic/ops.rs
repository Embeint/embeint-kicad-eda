// SPDX-License-Identifier: GPL-3.0-or-later
//! A port of the schematic methods of `LiveSchematic` in `tools/live_schematic_mcp/client.py`:
//! placing, rendering, the page and title block (both editors), annotation, library updates,
//! field autoplacement, moves, undo (both editors), pins, connections, the netlist, ERC, the
//! selection and net highlighting.  Requests, their order, validation and error texts are
//! Python's; the comments name each deliberate match with a Python quirk.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use embeint_ipc::client::{canonical_uuid, item_header, reply_json, Client, Error};
use embeint_ipc::pbjson::py_repr;
use embeint_ipc::proto::{
    self, any_type_name, build, int_field, list_field, message_field, str_field, unpack, with_field,
};
use prost_reflect::{DynamicMessage, Value};
use serde_json::{json, Map, Value as Json};

const C: &str = "kiapi.common.commands.";
const T: &str = "kiapi.common.types.";
const S: &str = "kiapi.schematic.commands.";
const SCH: &str = "kiapi.schematic.types.";
const EMPTY: &str = "google.protobuf.Empty";

// ---- Small helpers (client.py's module functions) ------------------------------------------

/// The repeated message values of a field.
fn messages(message: &DynamicMessage, name: &str) -> Vec<DynamicMessage> {
    list_field(message, name)
        .into_iter()
        .filter_map(|value| match value {
            Value::Message(item) => Some(item),
            _ => None,
        })
        .collect()
}

fn flag(message: &DynamicMessage, name: &str) -> bool {
    int_field(message, name) != 0
}

/// `Enum.Name(number)`, which raises ValueError for a number the enum does not define.
fn enum_name(full_name: &str, number: i64) -> Result<String, Error> {
    let short = full_name.rsplit('.').next().unwrap_or(full_name);
    proto::pool()
        .get_enum_by_name(full_name)
        .and_then(|e| i32::try_from(number).ok().and_then(|n| e.get_value(n)))
        .map(|value| value.name().to_owned())
        .ok_or_else(|| {
            Error::Value(format!(
                "Enum {short} has no name defined for value {number}"
            ))
        })
}

/// A float as JSON (always finite here: every value comes from integers or checked input).
fn num(value: f64) -> Json {
    json!(value)
}

/// Python's `round(value, digits)`: correctly rounded to the decimal digits, ties to even on the
/// exact binary value.  Rust's fixed-precision formatting rounds exactly the same way.
fn py_round(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// `mm(vector)`: a Vector2 as [x, y] in mm.
fn mm(vector: &DynamicMessage) -> [f64; 2] {
    [
        int_field(vector, "x_nm") as f64 / 1e6,
        int_field(vector, "y_nm") as f64 / 1e6,
    ]
}

fn mm_json(vector: &DynamicMessage) -> Json {
    let [x, y] = mm(vector);
    json!([num(x), num(y)])
}

/// `point(x, y)`: millimetres to a Vector2 in nanometres (Python's `round` is banker's rounding).
fn point(x: f64, y: f64) -> Result<Json, Error> {
    if ![x, y].iter().all(|v| v.is_finite() && v.abs() <= 10000.0) {
        return Err(Error::Value(
            "Coordinates must be finite millimeters within +/-10000".into(),
        ));
    }
    let nm = |v: f64| (v * 1_000_000.0).round_ties_even() as i64;
    Ok(json!({"x_nm": nm(x), "y_nm": nm(y)}))
}

/// `kiid(value)`: a canonical UUID, or a new one.  Python tests `if value`, so an empty string
/// also gets a new UUID.
fn kiid(value: &str) -> Result<Json, Error> {
    let id = if value.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        canonical_uuid(value)?
    };
    Ok(json!({"value": id}))
}

fn kiids(values: &[String]) -> Result<Vec<Json>, Error> {
    values.iter().map(|v| kiid(v)).collect()
}

/// `lib_identifier(lib_id)`: Library:Symbol, split at the first colon.
fn lib_identifier(lib_id: &str) -> Result<Json, Error> {
    match lib_id.split_once(':') {
        Some((library, entry)) if !library.is_empty() && !entry.is_empty() => {
            Ok(json!({"library_nickname": library, "entry_name": entry}))
        }
        _ => Err(Error::Value("lib_id must be Library:Symbol".into())),
    }
}

fn lib_id_text(identifier: &DynamicMessage) -> String {
    format!(
        "{}:{}",
        str_field(identifier, "library_nickname"),
        str_field(identifier, "entry_name")
    )
}

fn id_of(message: &DynamicMessage, name: &str) -> String {
    str_field(&message_field(message, name), "value")
}

/// `sheet_key(path)`: the UUIDs of a sheet path.
fn sheet_key(path: &DynamicMessage) -> Vec<String> {
    messages(path, "path")
        .iter()
        .map(|k| str_field(k, "value"))
        .collect()
}

/// A request whose JSON names the document as `{"document": ...}` and friends; `doc` is put in
/// as protobuf JSON (a DocumentSpecifier round-trips exactly).
fn request(
    name: &str,
    mut value: Json,
    doc: &DynamicMessage,
    field: &str,
) -> Result<DynamicMessage, Error> {
    let doc = reply_json(doc)?;
    match field {
        "header" => value["header"] = json!({"document": doc}),
        _ => value[field] = doc,
    }
    Ok(build(name, value))
}

// ---- Documents -------------------------------------------------------------------------------

/// `open_schematic(path)`: opens the project's schematic, then lists the open documents.
pub async fn open_schematic(client: &mut Client, path: Option<String>) -> Result<Json, Error> {
    let request = build(
        &format!("{C}OpenDocument"),
        json!({"type": "DOCTYPE_SCHEMATIC", "path": path.unwrap_or_default()}),
    );
    client
        .send(&request, &format!("{C}OpenDocumentResponse"))
        .await?;
    client.documents().await
}

/// live_schematic_focus.
pub async fn focus(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: &[String],
) -> Result<Json, Error> {
    // The ids go as given, without UUID normalisation, as server.py adds them
    let items: Vec<Json> = ids.iter().map(|v| json!({"value": v})).collect();
    let request = request(
        &format!("{C}FocusOnItems"),
        json!({"items": items}),
        doc,
        "document",
    )?;
    client.send(&request, EMPTY).await?;
    Ok(json!({"success": true}))
}

/// live_schematic_save.
pub async fn save(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let request = request(&format!("{C}SaveDocument"), json!({}), doc, "document")?;
    client.send(&request, EMPTY).await?;
    Ok(json!({"success": true}))
}

// ---- Placing and rendering -------------------------------------------------------------------

/// `place_symbol(doc, lib_id, x, y, reference, rotation)`.
pub async fn place_symbol(
    client: &mut Client,
    doc: &DynamicMessage,
    lib_id: &str,
    x: f64,
    y: f64,
    reference: Option<String>,
    rotation: Option<i64>,
) -> Result<Json, Error> {
    let orientation = match rotation {
        Some(0) => "SSO_0",
        Some(90) => "SSO_90",
        Some(180) => "SSO_180",
        Some(270) => "SSO_270",
        _ => {
            return Err(Error::Value(
                "rotation must be 0, 90, 180, or 270 degrees".into(),
            ))
        }
    };
    let mut value = json!({
        "lib_id": lib_identifier(lib_id)?,
        "position": point(x, y)?,
        "orientation": orientation,
    });
    if let Some(reference) = reference.filter(|r| !r.is_empty()) {
        value["reference"] = json!(reference);
    }
    let request = request(&format!("{S}PlaceSymbolFromLibrary"), value, doc, "header")?;
    let result = client
        .run_edit(
            doc,
            &request,
            &format!("{C}PlaceFromLibraryResponse"),
            "MCP: place schematic symbol",
        )
        .await?;
    // The reply embeds the full library definition; only what an agent needs is returned
    let placed = unpack(
        &message_field(&result, "item"),
        &format!("{SCH}SchematicSymbolInstance"),
    )
    .ok_or_else(|| Error::Runtime("KiCad did not return the placed symbol".into()))?;
    let field_text = |name: &str| {
        str_field(
            &message_field(&message_field(&placed, name), "text"),
            "text",
        )
    };
    Ok(json!({
        "id": id_of(&placed, "id"),
        "lib_id": lib_id,
        "reference": field_text("reference_field"),
        "value": field_text("value_field"),
        "position_mm": mm_json(&message_field(&placed, "position")),
        "unit": int_field(&message_field(&placed, "unit"), "unit"),
    }))
}

/// `tempfile.gettempdir()`: the first directory among TMPDIR, TEMP and TMP, then the platform's
/// candidates (Windows: ~\AppData\Local\Temp, %SYSTEMROOT%\Temp, c:\temp, c:\tmp, \temp, \tmp;
/// elsewhere /tmp, /var/tmp, /usr/tmp), then the working directory, each made absolute first as
/// Python does (it also checks the directory is writable; a directory that is not makes the
/// preview fail when KiCad writes there).
fn temp_root() -> PathBuf {
    let mut candidates: Vec<PathBuf> = ["TMPDIR", "TEMP", "TMP"]
        .iter()
        .filter_map(|name| std::env::var_os(name).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .collect();
    if cfg!(windows) {
        if let Some(home) = std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
            candidates.push(PathBuf::from(home).join(r"AppData\Local\Temp"));
        }
        if let Some(root) = std::env::var_os("SYSTEMROOT").filter(|v| !v.is_empty()) {
            candidates.push(PathBuf::from(root).join("Temp"));
        }
        candidates.extend([r"c:\temp", r"c:\tmp", r"\temp", r"\tmp"].map(PathBuf::from));
    } else {
        candidates.extend(["/tmp", "/var/tmp", "/usr/tmp"].map(PathBuf::from));
    }
    candidates
        .into_iter()
        .map(|path| std::path::absolute(&path).unwrap_or(path))
        .find(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(std::env::temp_dir)
}

/// `render(doc, dpi, frame)`: KiCad plots the live sheet to a PNG in a private temporary
/// directory, which is removed afterwards.
pub async fn render(
    client: &mut Client,
    doc: &DynamicMessage,
    dpi: Option<i64>,
    frame: bool,
) -> Result<Vec<u8>, Error> {
    let dpi = dpi
        .filter(|d| (72..=200).contains(d))
        .ok_or_else(|| Error::Value("Preview DPI must be between 72 and 200".into()))?;
    let directory = tempfile::Builder::new()
        .prefix("kicad-live-preview-")
        .tempdir_in(temp_root())
        .map_err(|e| Error::Runtime(e.to_string()))?;
    let target = directory.path().join("preview.png");
    let value = json!({
        "job_settings": {"document": reply_json(doc)?, "output_path": path_text(&target)},
        "plot_settings": {"sheet_mode": "SJSM_SINGLE_SHEET", "plot_drawing_sheet": frame},
        "dpi": dpi,
        "plot_background_color": true,
    });
    let request = build("kiapi.schematic.jobs.RunSchematicJobExportPng", value);
    let response = client
        .send(&request, "kiapi.common.types.RunJobResponse")
        .await?;
    if !matches!(int_field(&response, "status"), 1 | 2) || !target.is_file() {
        return Err(Error::Runtime(format!(
            "Live rendering failed: {}",
            py_repr(&reply_json(&response)?)
        )));
    }
    std::fs::read(&target).map_err(|e| Error::Runtime(e.to_string()))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// ---- Page, drawing sheet and title block -----------------------------------------------------

/// `box_mm(box)`: [x1, y1, x2, y2] in mm rounded to 3 places, or None for an empty box.
fn box_mm(value: &DynamicMessage) -> Json {
    if !value.has_field_by_name("size") {
        return Json::Null;
    }
    let size = message_field(value, "size");
    let (w, h) = (int_field(&size, "x_nm"), int_field(&size, "y_nm"));
    if w == 0 && h == 0 {
        return Json::Null;
    }
    let [x, y] = mm(&message_field(value, "position"));
    json!([
        num(py_round(x, 3)),
        num(py_round(y, 3)),
        num(py_round(x + w as f64 / 1e6, 3)),
        num(py_round(y + h as f64 / 1e6, 3)),
    ])
}

const TITLE_FIELDS: [&str; 4] = ["title", "date", "revision", "company"];

fn title_block_dict(block: &DynamicMessage) -> Json {
    let mut fields = Map::new();
    for name in TITLE_FIELDS {
        fields.insert(name.into(), json!(str_field(block, name)));
    }
    let mut comments: Vec<String> = (1..=9)
        .map(|n| str_field(block, &format!("comment{n}")))
        .collect();
    while comments.last().is_some_and(String::is_empty) {
        comments.pop();
    }
    fields.insert("comments".into(), json!(comments));
    Json::Object(fields)
}

fn layout_part(item: &DynamicMessage) -> Json {
    let mut entry = Map::new();
    entry.insert("id".into(), json!(id_of(item, "id")));
    entry.insert("type".into(), json!(str_field(item, "type")));
    entry.insert("name".into(), json!(str_field(item, "name")));
    let part = str_field(item, "part");
    if !part.is_empty() {
        entry.insert("part".into(), json!(part));
    }
    entry.insert("box_mm".into(), box_mm(&message_field(item, "box")));
    Json::Object(entry)
}

/// `page_layout(doc)`: page, drawing area, title block, items outside the frame and overlaps.
pub async fn page_layout(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let layout = client
        .send(
            &request(&format!("{C}GetPageLayout"), json!({}), doc, "document")?,
            &format!("{C}PageLayoutResponse"),
        )
        .await?;
    let settings = client
        .send(
            &request(&format!("{C}GetPageSettings"), json!({}), doc, "document")?,
            &format!("{T}PageSettings"),
        )
        .await?;
    let block = client
        .send(
            &request(&format!("{C}GetTitleBlockInfo"), json!({}), doc, "document")?,
            &format!("{T}TitleBlockInfo"),
        )
        .await?;
    let problems: Vec<Json> = messages(&layout, "problems")
        .iter()
        .map(|p| {
            let mut entry = Map::new();
            entry.insert("id".into(), json!(id_of(p, "id")));
            entry.insert("type".into(), json!(str_field(p, "type")));
            entry.insert("name".into(), json!(str_field(p, "name")));
            entry.insert("box_mm".into(), box_mm(&message_field(p, "box")));
            if flag(p, "outside_drawing_area") {
                entry.insert("outside_drawing_area".into(), json!(true));
            }
            if flag(p, "overlaps_title_block") {
                entry.insert("overlaps_title_block".into(), json!(true));
            }
            Json::Object(entry)
        })
        .collect();
    let overlaps: Vec<Json> = messages(&layout, "overlaps")
        .iter()
        .map(|o| {
            let mut entry = Map::new();
            entry.insert("first".into(), layout_part(&message_field(o, "first")));
            entry.insert("second".into(), layout_part(&message_field(o, "second")));
            entry.insert("overlap_mm".into(), box_mm(&message_field(o, "overlap")));
            let layer = str_field(o, "layer");
            if !layer.is_empty() {
                entry.insert("layer".into(), json!(layer));
            }
            Json::Object(entry)
        })
        .collect();
    let mut report = Map::new();
    if flag(&layout, "overlaps_checked") {
        let count = int_field(&layout, "overlap_count");
        let listed = overlaps.len() as i64;
        report.insert("overlap_free".into(), json!(count == 0));
        report.insert("overlaps".into(), json!(overlaps));
        if count > listed {
            report.insert("overlaps_not_listed".into(), json!(count - listed));
        }
    } else {
        report.insert("overlap_free".into(), Json::Null);
        report.insert("overlaps".into(), json!([]));
        report.insert(
            "overlap_check".into(),
            json!("not available from this KiCad build"),
        );
    }
    let paper = enum_name(&format!("{T}PageSize"), int_field(&settings, "page_size"))?;
    let orientation = enum_name(
        &format!("{T}PageOrientation"),
        int_field(&settings, "orientation"),
    )?;
    let [page_w, page_h] = mm(&message_field(&layout, "page_size"));
    let mut result = Map::new();
    result.insert(
        "paper".into(),
        json!(paper.strip_prefix("PS_").unwrap_or(&paper)),
    );
    result.insert(
        "orientation".into(),
        json!(orientation
            .strip_prefix("PO_")
            .unwrap_or(&orientation)
            .to_lowercase()),
    );
    result.insert(
        "page_size_mm".into(),
        json!([num(py_round(page_w, 1)), num(py_round(page_h, 1))]),
    );
    for (key, field) in [
        ("drawing_area_mm", "drawing_area"),
        ("title_block_mm", "title_block"),
        ("content_mm", "content"),
    ] {
        result.insert(key.into(), box_mm(&message_field(&layout, field)));
    }
    let outline = box_mm(&message_field(&layout, "board_outline"));
    // Python tests the list's truth: a present box is a non-empty list
    if !outline.is_null() {
        result.insert("board_outline_mm".into(), outline);
    }
    result.insert("items".into(), json!(int_field(&layout, "item_count")));
    result.insert("fits".into(), json!(problems.is_empty()));
    result.insert("problems".into(), json!(problems));
    result.extend(report);
    result.insert("title_block".into(), title_block_dict(&block));
    let drawing_sheet = str_field(&settings, "drawing_sheet");
    if !drawing_sheet.is_empty() {
        result.insert("drawing_sheet".into(), json!(drawing_sheet));
    }
    Ok(Json::Object(result))
}

/// PAGE_SIZES: accepted size names and their PageSize values.
const PAGE_SIZES: [(&str, &str); 15] = [
    ("A5", "PS_A5"),
    ("A4", "PS_A4"),
    ("A3", "PS_A3"),
    ("A2", "PS_A2"),
    ("A1", "PS_A1"),
    ("A0", "PS_A0"),
    ("ANSI_A", "PS_ANSI_A"),
    ("ANSI_B", "PS_ANSI_B"),
    ("ANSI_C", "PS_ANSI_C"),
    ("ANSI_D", "PS_ANSI_D"),
    ("ANSI_E", "PS_ANSI_E"),
    ("LETTER", "PS_US_LETTER"),
    ("LEGAL", "PS_US_LEGAL"),
    ("LEDGER", "PS_US_LEDGER"),
    ("USER", "PS_USER"),
];

fn enum_number(full_name: &str, value: &str) -> i32 {
    proto::pool()
        .get_enum_by_name(full_name)
        .and_then(|e| e.get_value_by_name(value))
        .map_or(0, |v| v.number())
}

/// The arguments of `set_page` (server.py passes user_size_mm=None).
pub struct PageArgs {
    pub size: Option<String>,
    pub orientation: Option<String>,
    pub title: Option<String>,
    pub date: Option<String>,
    pub revision: Option<String>,
    pub company: Option<String>,
    pub comments: Option<Vec<String>>,
}

/// `datetime.date.today().isoformat()`: the local date.
fn today() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// `set_page(doc, ...)`: changes the paper and/or title block (each one edit outside a commit),
/// then returns the page layout.
pub async fn set_page(
    client: &mut Client,
    doc: &DynamicMessage,
    page: PageArgs,
) -> Result<Json, Error> {
    if page.size.is_some() || page.orientation.is_some() {
        let mut settings = client
            .send(
                &request(&format!("{C}GetPageSettings"), json!({}), doc, "document")?,
                &format!("{T}PageSettings"),
            )
            .await?;
        if let Some(size) = &page.size {
            let upper = size.to_uppercase().replace(['-', ' '], "_");
            let key = upper.strip_prefix("US_").unwrap_or(&upper);
            let Some((_, value)) = PAGE_SIZES.iter().find(|(name, _)| *name == key) else {
                return Err(Error::Value(
                    "size must be one of ['A0', 'A1', 'A2', 'A3', 'A4', 'A5', 'ANSI_A', 'ANSI_B', \
                     'ANSI_C', 'ANSI_D', 'ANSI_E', 'LEDGER', 'LEGAL', 'LETTER', 'USER']"
                        .into(),
                ));
            };
            settings.set_field_by_name(
                "page_size",
                Value::EnumNumber(enum_number(&format!("{T}PageSize"), value)),
            );
        }
        if let Some(orientation) = &page.orientation {
            let value = match orientation.as_str() {
                "landscape" => "PO_LANDSCAPE",
                "portrait" => "PO_PORTRAIT",
                _ => {
                    return Err(Error::Value(
                        "orientation must be landscape or portrait".into(),
                    ))
                }
            };
            settings.set_field_by_name(
                "orientation",
                Value::EnumNumber(enum_number(&format!("{T}PageOrientation"), value)),
            );
        }
        let request = with_field(
            request(&format!("{C}SetPageSettings"), json!({}), doc, "document")?,
            "page_settings",
            settings,
        );
        client
            .send_mutation(&request, &format!("{T}PageSettings"))
            .await?;
    }
    let mut values = [
        ("title", page.title),
        ("date", page.date),
        ("revision", page.revision),
        ("company", page.company),
    ];
    if values.iter().any(|(_, v)| v.is_some()) || page.comments.is_some() {
        let mut block = client
            .send(
                &request(&format!("{C}GetTitleBlockInfo"), json!({}), doc, "document")?,
                &format!("{T}TitleBlockInfo"),
            )
            .await?;
        if values[1].1.as_deref() == Some("today") {
            values[1].1 = Some(today());
        }
        for (name, value) in values {
            if let Some(value) = value {
                block.set_field_by_name(name, Value::String(value));
            }
        }
        if let Some(comments) = &page.comments {
            if comments.len() > 9 {
                return Err(Error::Value("a title block has at most 9 comments".into()));
            }
            for n in 1..=9 {
                let text = comments.get(n - 1).cloned().unwrap_or_default();
                block.set_field_by_name(&format!("comment{n}"), Value::String(text));
            }
        }
        let request = with_field(
            request(&format!("{C}SetTitleBlockInfo"), json!({}), doc, "document")?,
            "title_block",
            block,
        );
        client.send_mutation(&request, EMPTY).await?;
    }
    page_layout(client, doc).await
}

// ---- Edits as one undo step ------------------------------------------------------------------

/// The arguments of `annotate`.
pub struct AnnotateArgs<'a> {
    pub scope: &'a str,
    pub symbols: Vec<String>,
    pub order: &'a str,
    pub numbering: &'a str,
    pub reset_existing: bool,
    pub recursive: bool,
    /// The argument as given: Python ints are unbounded, and protobuf refuses one beyond int32
    pub start_number: &'a Json,
}

/// `annotate(doc, ...)`.
pub async fn annotate(
    client: &mut Client,
    doc: &DynamicMessage,
    args: AnnotateArgs<'_>,
) -> Result<Json, Error> {
    let scope = match args.scope {
        "all" => Some("ANS_ALL"),
        "sheet" => Some("ANS_SHEET"),
        "symbols" => Some("ANS_SYMBOLS"),
        _ => None,
    };
    let order = match args.order {
        "x" => Some("AO_BY_X_POSITION"),
        "y" => Some("AO_BY_Y_POSITION"),
        "unsorted" => Some("AO_UNSORTED"),
        _ => None,
    };
    let numbering = match args.numbering {
        "first_free" => Some("AN_FIRST_FREE"),
        "sheet_x100" => Some("AN_SHEET_X_100"),
        "sheet_x1000" => Some("AN_SHEET_X_1000"),
        _ => None,
    };
    let (Some(scope), Some(order), Some(numbering)) = (scope, order, numbering) else {
        return Err(Error::Value(
            "scope must be one of ['all', 'sheet', 'symbols'], order one of ['x', 'y', \
             'unsorted'], numbering one of ['first_free', 'sheet_x100', 'sheet_x1000']"
                .into(),
        ));
    };
    if args.scope == "symbols" && args.symbols.is_empty() {
        return Err(Error::Value("scope 'symbols' needs symbol UUIDs".into()));
    }
    let start_number = args
        .start_number
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| Error::Value(format!("Value out of range: {}", args.start_number)))?;
    let value = json!({
        "scope": scope, "order": order, "numbering": numbering,
        "reset_existing": args.reset_existing, "recursive": args.recursive,
        "start_number": start_number, "symbols": kiids(&args.symbols)?,
    });
    let request = request(&format!("{S}AnnotateSchematic"), value, doc, "document")?;
    let response = client
        .run_edit(
            doc,
            &request,
            &format!("{S}AnnotateSchematicResponse"),
            "MCP: annotate",
        )
        .await?;
    let changes: Vec<Json> = messages(&response, "changes")
        .iter()
        .map(|c| {
            json!({"id": id_of(c, "symbol"), "old": str_field(c, "old_reference"),
                   "new": str_field(c, "new_reference")})
        })
        .collect();
    Ok(json!({"changes": changes}))
}

/// The arguments of `update_from_library`.
pub struct LibraryUpdateArgs {
    pub symbols: Vec<String>,
    pub new_lib_id: Option<String>,
    pub reset_field_text: bool,
    pub reset_field_visibility: bool,
    pub reset_field_positions: bool,
    pub remove_extra_fields: bool,
}

/// `update_from_library(doc, ...)`.
pub async fn update_from_library(
    client: &mut Client,
    doc: &DynamicMessage,
    args: LibraryUpdateArgs,
) -> Result<Json, Error> {
    let mut value = json!({
        "reset_field_text": args.reset_field_text,
        "reset_field_visibility": args.reset_field_visibility,
        "reset_field_positions": args.reset_field_positions,
        "remove_extra_fields": args.remove_extra_fields,
        "symbols": kiids(&args.symbols)?,
    });
    if let Some(new_lib_id) = args.new_lib_id.filter(|v| !v.is_empty()) {
        value["new_lib_id"] = lib_identifier(&new_lib_id)?;
    }
    let request = request(
        &format!("{S}UpdateSymbolsFromLibrary"),
        value,
        doc,
        "document",
    )?;
    let response = client
        .run_edit(
            doc,
            &request,
            &format!("{S}UpdateSymbolsFromLibraryResponse"),
            "MCP: update symbols from library",
        )
        .await?;
    let symbols: Vec<Json> = messages(&response, "symbols")
        .iter()
        .map(|s| {
            let mut entry = Map::new();
            entry.insert("id".into(), json!(id_of(s, "symbol")));
            entry.insert(
                "lib_id".into(),
                json!(lib_id_text(&message_field(s, "lib_id"))),
            );
            entry.insert("updated".into(), json!(flag(s, "updated")));
            let error = str_field(s, "error_message");
            if !error.is_empty() {
                entry.insert("error".into(), json!(error));
            }
            Json::Object(entry)
        })
        .collect();
    Ok(json!({"symbols": symbols}))
}

/// `autoplace(doc, ids)`.
pub async fn autoplace(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: &[String],
) -> Result<Json, Error> {
    let request = request(
        &format!("{S}AutoplaceFields"),
        json!({"items": kiids(ids)?}),
        doc,
        "document",
    )?;
    client
        .run_edit(doc, &request, EMPTY, "MCP: autoplace fields")
        .await?;
    Ok(json!({"success": true}))
}

/// The arguments of `transform`.
pub struct TransformArgs<'a> {
    pub ids: &'a [String],
    pub dx: f64,
    pub dy: f64,
    pub rotation: Option<i64>,
    pub mirror_horizontally: bool,
    pub mirror_vertically: bool,
    pub drag_wires: bool,
}

/// `transform(doc, ids, ...)`: live_schematic_move.
pub async fn transform(
    client: &mut Client,
    doc: &DynamicMessage,
    args: TransformArgs<'_>,
) -> Result<Json, Error> {
    let rotation = match args.rotation {
        Some(0) => "IR_NONE",
        Some(90) => "IR_CCW_90",
        Some(180) => "IR_180",
        Some(270 | -90) => "IR_CW_90",
        _ => {
            return Err(Error::Value(
                "rotation must be 0, 90, 180 or 270 degrees counter-clockwise".into(),
            ))
        }
    };
    if args.ids.is_empty() || args.ids.len() > 200 {
        return Err(Error::Value("Supply between 1 and 200 UUIDs".into()));
    }
    let value = json!({
        "translation": point(args.dx, args.dy)?,
        "rotation": rotation,
        "mirror_horizontally": args.mirror_horizontally,
        "mirror_vertically": args.mirror_vertically,
        "drag_wires": args.drag_wires,
        "items": kiids(args.ids)?,
    });
    let request = request(&format!("{S}TransformItems"), value, doc, "document")?;
    let response = client
        .run_edit(
            doc,
            &request,
            &format!("{S}TransformItemsResponse"),
            "MCP: move items",
        )
        .await?;
    let dragged: Vec<Json> = messages(&response, "dragged_items")
        .iter()
        .map(|i| json!(str_field(i, "value")))
        .collect();
    Ok(json!({"moved": args.ids.len(), "dragged": dragged}))
}

/// `undo(doc, redo)`: one edit outside a commit (refused while any commit is open).
pub async fn undo(client: &mut Client, doc: &DynamicMessage, redo: bool) -> Result<Json, Error> {
    let command = if redo {
        "RedoLastChange"
    } else {
        "UndoLastChange"
    };
    let request = request(&format!("{C}{command}"), json!({}), doc, "document")?;
    let response = client
        .send_mutation(&request, &format!("{C}UndoRedoResponse"))
        .await?;
    reply_json(&response)
}

// ---- Pins, connectivity and connections ------------------------------------------------------

/// `netlist(doc)`: nets as {name: [(sheet, id)]} (sorted by name, as every caller iterates them)
/// and a pin lookup {(sheet, pin id): net}, where a later net wins as in the Python dict.
type Members = Vec<(Vec<String>, String)>;

async fn netlist(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<
    (
        BTreeMap<String, Members>,
        HashMap<(Vec<String>, String), String>,
    ),
    Error,
> {
    let response = client
        .send(
            &request(
                &format!("{S}GetSchematicNetlist"),
                json!({}),
                doc,
                "document",
            )?,
            &format!("{S}SchematicNetlistResponse"),
        )
        .await?;
    let mut nets: BTreeMap<String, Members> = BTreeMap::new();
    let mut pin_nets = HashMap::new();
    for net in messages(&response, "nets") {
        let name = str_field(&net, "name");
        let members = nets.entry(name.clone()).or_default();
        for sheet in messages(&net, "sheets") {
            let key = sheet_key(&message_field(&sheet, "path"));
            for item in messages(&sheet, "items") {
                let id = str_field(&item, "value");
                members.push((key.clone(), id.clone()));
                pin_nets.insert((key.clone(), id), name.clone());
            }
        }
    }
    Ok((nets, pin_nets))
}

/// One placed pin as `symbol_pins` reports it.
struct Pin {
    number: String,
    name: String,
    position: [f64; 2],
    orientation: String,
    kind: String,
    id: String,
}

struct PlacedSymbol {
    id: String,
    reference: String,
    lib_id: String,
    unit: i64,
    pins: Vec<Pin>,
}

/// GetSymbolPins for one sheet.
async fn pins_reply(
    client: &mut Client,
    doc: &DynamicMessage,
    symbols: &[String],
) -> Result<DynamicMessage, Error> {
    let request = request(
        &format!("{S}GetSymbolPins"),
        json!({"symbols": kiids(symbols)?}),
        doc,
        "document",
    )?;
    client
        .send(&request, &format!("{S}SymbolPinsResponse"))
        .await
}

/// The placed symbols of a SymbolPinsResponse.  Python names the enums while it builds the
/// whole list, so an unknown value fails the call whichever pin it is on.
fn placed(response: &DynamicMessage) -> Result<Vec<PlacedSymbol>, Error> {
    let mut result = Vec::new();
    for symbol in messages(response, "symbols") {
        let mut pins = Vec::new();
        for pin in messages(&symbol, "pins") {
            pins.push(Pin {
                number: str_field(&pin, "number"),
                name: str_field(&pin, "name"),
                position: mm(&message_field(&pin, "position")),
                orientation: enum_name(
                    &format!("{SCH}SchematicPinOrientation"),
                    int_field(&pin, "orientation"),
                )?,
                kind: enum_name(
                    &format!("{T}ElectricalPinType"),
                    int_field(&pin, "electrical_type"),
                )?,
                id: id_of(&pin, "id"),
            });
        }
        result.push(PlacedSymbol {
            id: id_of(&symbol, "symbol"),
            reference: str_field(&symbol, "reference"),
            lib_id: lib_id_text(&message_field(&symbol, "lib_id")),
            unit: int_field(&symbol, "unit"),
            pins,
        });
    }
    Ok(result)
}

/// `symbol_pins(doc, with_nets=False)`: every placed symbol on one sheet.
async fn placed_symbols(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Vec<PlacedSymbol>, Error> {
    placed(&pins_reply(client, doc, &[]).await?)
}

/// `symbol_pins(doc, symbols)`: placed pins with their connection points and nets.
pub async fn symbol_pins(
    client: &mut Client,
    doc: &DynamicMessage,
    symbols: &[String],
) -> Result<Vec<Json>, Error> {
    let reply = pins_reply(client, doc, symbols).await?;
    let pin_nets = netlist(client, doc).await?.1;
    let placed = placed(&reply)?;
    let key = sheet_key(&message_field(doc, "sheet_path"));
    Ok(placed
        .into_iter()
        .map(|symbol| {
            let pins: Vec<Json> = symbol
                .pins
                .iter()
                .map(|pin| {
                    json!({
                        "number": pin.number, "name": pin.name,
                        "position_mm": [num(pin.position[0]), num(pin.position[1])],
                        "orientation": pin.orientation, "type": pin.kind, "id": pin.id,
                        "net": pin_nets.get(&(key.clone(), pin.id.clone())),
                    })
                })
                .collect();
            json!({"id": symbol.id, "reference": symbol.reference, "lib_id": symbol.lib_id,
                   "unit": symbol.unit, "pins": pins})
        })
        .collect())
}

/// Every sheet instance, depth first: (document for that sheet, its readable path).
async fn sheet_instances(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Vec<(DynamicMessage, String)>, Error> {
    let response = client
        .send(
            &request(
                &format!("{S}GetSchematicHierarchy"),
                json!({}),
                doc,
                "document",
            )?,
            &format!("{S}SchematicHierarchyResponse"),
        )
        .await?;
    fn visit(
        doc: &DynamicMessage,
        nodes: Vec<DynamicMessage>,
        result: &mut Vec<(DynamicMessage, String)>,
    ) {
        for node in nodes {
            let path = message_field(&node, "path");
            let readable = str_field(&path, "path_human_readable");
            let spec = with_field(doc.clone(), "sheet_path", path);
            result.push((
                spec,
                if readable.is_empty() {
                    "/".into()
                } else {
                    readable
                },
            ));
            visit(doc, messages(&node, "children"), result);
        }
    }
    let mut result = Vec::new();
    visit(doc, messages(&response, "top_level_sheets"), &mut result);
    Ok(result)
}

/// `netlist_pins(doc, scope)`: nets with their pins as REF.PIN and their item counts.
pub async fn netlist_pins(
    client: &mut Client,
    doc: &DynamicMessage,
    scope: &str,
) -> Result<Json, Error> {
    if scope != "hierarchy" && scope != "sheet" {
        return Err(Error::Value(
            "scope must be \"hierarchy\" or \"sheet\"".into(),
        ));
    }
    let nets = netlist(client, doc).await?.0;
    let mut result = Map::new();
    if scope == "sheet" {
        let mut labels = HashMap::new();
        for symbol in placed_symbols(client, doc).await? {
            for pin in &symbol.pins {
                labels.insert(
                    pin.id.clone(),
                    format!("{}.{}", symbol.reference, pin.number),
                );
            }
        }
        for (name, members) in nets {
            let pins: BTreeSet<&String> =
                members.iter().filter_map(|(_, i)| labels.get(i)).collect();
            result.insert(name, json!({"pins": pins, "items": members.len()}));
        }
        return Ok(Json::Object(result));
    }
    let mut labels = HashMap::new();
    let mut paths = HashMap::new();
    for (spec, path) in sheet_instances(client, doc).await? {
        let key = sheet_key(&message_field(&spec, "sheet_path"));
        paths.insert(key.clone(), path);
        for symbol in placed_symbols(client, &spec).await? {
            for pin in &symbol.pins {
                labels.insert(
                    (key.clone(), pin.id.clone()),
                    format!("{}.{}", symbol.reference, pin.number),
                );
            }
        }
    }
    for (name, members) in nets {
        let mut sheets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (key, item) in &members {
            if let Some(label) = labels.get(&(key.clone(), item.clone())) {
                let path = paths.get(key).cloned().unwrap_or_else(|| key.join("/"));
                sheets.entry(path).or_default().insert(label.clone());
            }
        }
        let all: BTreeSet<&String> = sheets.values().flatten().collect();
        let sheets: Vec<Json> = sheets
            .iter()
            .map(|(path, pins)| json!({"path": path, "pins": pins}))
            .collect();
        result.insert(
            name,
            json!({"pins": all, "items": members.len(), "sheets": sheets}),
        );
    }
    Ok(Json::Object(result))
}

/// `find_pin(doc, reference, pin)`: one placed pin by reference and pin number or name.
async fn find_pin(
    client: &mut Client,
    doc: &DynamicMessage,
    reference: &str,
    pin: &str,
) -> Result<Pin, Error> {
    for symbol in placed_symbols(client, doc).await? {
        if symbol.reference != reference {
            continue;
        }
        if let Some(found) = symbol
            .pins
            .into_iter()
            .find(|c| c.number == pin || c.name == pin)
        {
            return Ok(found);
        }
    }
    Err(Error::Value(format!(
        "No pin {reference}.{pin} on this sheet"
    )))
}

/// OUTWARD: the direction a wire leaves a pin of each drawn orientation.  Python indexes a dict
/// by the enum value, so an orientation without an entry raises KeyError, whose text is the key.
fn outward(orientation: &str) -> Result<(i32, i32), Error> {
    Ok(match orientation {
        "SPO_RIGHT" => (-1, 0),
        "SPO_LEFT" => (1, 0),
        "SPO_UP" => (0, 1),
        "SPO_DOWN" => (0, -1),
        other => {
            let number = enum_number(&format!("{SCH}SchematicPinOrientation"), other);
            return Err(Error::Runtime(number.to_string()));
        }
    })
}

fn wire(start: [f64; 2], end: [f64; 2]) -> Result<(String, Json), Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let item = json!({
        "@type": format!("type.googleapis.com/{SCH}SchematicLine"),
        "id": {"value": id}, "start": point(start[0], start[1])?, "end": point(end[0], end[1])?,
        "type": "SLT_WIRE",
    });
    Ok((id, item))
}

fn items(values: &[Json]) -> Result<Vec<DynamicMessage>, Error> {
    embeint_ipc::client::parse_schematic_items(values)
}

/// `connect_pins(doc, ...)`: an orthogonal wire of one or two segments between two pins.
pub async fn connect_pins(
    client: &mut Client,
    doc: &DynamicMessage,
    from: (&str, &str),
    to: (&str, &str),
) -> Result<Json, Error> {
    let a = find_pin(client, doc, from.0, from.1).await?;
    let b = find_pin(client, doc, to.0, to.1).await?;
    let ([x1, y1], [x2, y2]) = (a.position, b.position);
    if (x1, y1) == (x2, y2) {
        return Err(Error::Value("Pins already share a connection point".into()));
    }
    // Leave the first pin along its own axis, then turn toward the second
    let horizontal = outward(&a.orientation)?.1 == 0;
    let corner = if horizontal { [x2, y1] } else { [x1, y2] };
    let mut points = vec![[x1, y1]];
    for p in [corner, [x2, y2]] {
        if points.last() != Some(&p) {
            points.push(p);
        }
    }
    let mut ids = Vec::new();
    let mut wires = Vec::new();
    for pair in points.windows(2) {
        let (id, item) = wire(pair[0], pair[1])?;
        ids.push(id);
        wires.push(item);
    }
    client.create_items(doc, &items(&wires)?).await?;
    let path: Vec<Json> = points
        .iter()
        .map(|[x, y]| json!([num(*x), num(*y)]))
        .collect();
    Ok(json!({"wires": ids, "path_mm": path}))
}

/// `connect_pin_to_net(doc, ...)`: a short wire leaving the pin and a label at its end.
pub async fn connect_pin_to_net(
    client: &mut Client,
    doc: &DynamicMessage,
    pin: (&str, &str),
    net: &str,
    label: &str,
    stub_mm: f64,
) -> Result<Json, Error> {
    let kind = match label {
        "local" => "LocalLabel",
        "global" => "GlobalLabel",
        "hierarchical" => "HierarchicalLabel",
        _ => {
            return Err(Error::Value(
                "label must be local, global or hierarchical".into(),
            ))
        }
    };
    if !(0.0 < stub_mm && stub_mm <= 50.0) {
        return Err(Error::Value("stub_mm must be between 0 and 50".into()));
    }
    let found = find_pin(client, doc, pin.0, pin.1).await?;
    let [x, y] = found.position;
    let direction = outward(&found.orientation)?;
    // Python multiplies the int direction by the float stub, then adds
    let end = [
        x + f64::from(direction.0) * stub_mm,
        y + f64::from(direction.1) * stub_mm,
    ];
    let (wire_id, wire_item) = wire([x, y], end)?;
    let spin = match direction {
        (-1, 0) => "SLSS_LEFT",
        (1, 0) => "SLSS_RIGHT",
        (0, 1) => "SLSS_BOTTOM",
        _ => "SLSS_UP",
    };
    let label_id = uuid::Uuid::new_v4().to_string();
    let mut tag = json!({
        "@type": format!("type.googleapis.com/{SCH}{kind}"),
        "id": {"value": label_id}, "position": point(end[0], end[1])?,
        "text": {"text": net}, "spin_style": spin,
    });
    if label != "local" {
        tag["shape"] = json!(if label == "hierarchical" {
            "SLSH_BIDI"
        } else {
            "SLSH_PASSIVE"
        });
    }
    client.create_items(doc, &items(&[wire_item, tag])?).await?;
    Ok(json!({"wire": wire_id, "label": label_id,
              "label_position_mm": [num(end[0]), num(end[1])]}))
}

// ---- ERC, selection and highlighting ---------------------------------------------------------

/// `erc(doc)`: runs the check as a KiCad task, polls it every 0.25 s for up to 60 s.
pub async fn erc(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let started = client
        .send(
            &request(
                "kiapi.schematic.tasks.StartErcTask",
                json!({}),
                doc,
                "document",
            )?,
            "kiapi.common.tasks.StartTaskResponse",
        )
        .await?;
    let token = message_field(&started, "token");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let status_request = with_field(
            build("kiapi.common.tasks.GetTaskStatus", json!({})),
            "token",
            token.clone(),
        );
        let status = client
            .send(&status_request, "kiapi.common.tasks.TaskStatusResponse")
            .await?;
        let state = int_field(&status, "state");
        if state == 2 {
            break;
        }
        if state == 3 || state == 4 {
            return Err(Error::Runtime(format!(
                "ERC {}: {}",
                enum_name("kiapi.common.tasks.TaskState", state)?,
                str_field(&status, "error_message")
            )));
        }
        if tokio::time::Instant::now() > deadline {
            // Python raises TimeoutError; only its text reaches the tool result
            return Err(Error::Runtime(
                "ERC did not finish in time; it may still be running in KiCad".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let result_request = with_field(
        build("kiapi.schematic.tasks.GetErcTaskResult", json!({})),
        "token",
        token,
    );
    let result = client
        .send(&result_request, "kiapi.schematic.tasks.ErcTaskResult")
        .await?;
    let mut violations = Vec::new();
    for marker in messages(&result, "violations") {
        let mut entry = match reply_json(&marker)? {
            Json::Object(map) => map,
            _ => Map::new(),
        };
        entry.insert(
            "position_mm".into(),
            mm_json(&message_field(&marker, "position")),
        );
        // dict.pop keeps the order of the other keys
        entry.shift_remove("position");
        violations.push(Json::Object(entry));
    }
    Ok(json!({
        "violation_count": violations.len(),
        "ignored_checks": int_field(&result, "ignored_count"),
        "excluded": int_field(&result, "excluded_count"),
        "violations": violations,
    }))
}

/// `select(doc, ids, add)`: the selection is cleared first unless add; the ids are checked only
/// after that, as in Python.
pub async fn select(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: &[String],
    add: bool,
) -> Result<Json, Error> {
    if !add {
        let clear = with_field(
            build(&format!("{C}ClearSelection"), json!({})),
            "header",
            item_header(doc),
        );
        client.send(&clear, EMPTY).await?;
    }
    if ids.is_empty() {
        return Ok(json!({"selected": []}));
    }
    let request = request(
        &format!("{C}AddToSelection"),
        json!({"items": kiids(ids)?}),
        doc,
        "header",
    )?;
    let response = client
        .send(&request, &format!("{C}SelectionResponse"))
        .await?;
    let selected: Vec<Json> = messages(&response, "items")
        .iter()
        .map(|i| {
            let name = any_type_name(i);
            json!(name.rsplit('.').next().unwrap_or_default())
        })
        .collect();
    Ok(json!({"selected": selected}))
}

/// `highlight_nets(doc, nets)`: the document is not part of the request.
pub async fn highlight_nets(client: &mut Client, nets: &[String]) -> Result<Json, Error> {
    let request = build(&format!("{C}HighlightNets"), json!({"net_name": nets}));
    let response = client
        .send(&request, &format!("{C}HighlightNetsResponse"))
        .await?;
    reply_json(&response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_follows_python() {
        // round(0.125, 2) == 0.12, round(2.675, 2) == 2.67, round(-0.0004, 3) == -0.0
        assert_eq!(py_round(0.125, 2), 0.12);
        assert_eq!(py_round(2.675, 2), 2.67);
        assert_eq!(py_round(-0.0004, 3).to_bits(), (-0.0f64).to_bits());
        assert_eq!(py_round(296.99999999, 1), 297.0);
        assert_eq!(
            point(0.0000025, -0.0000015).unwrap(),
            json!({"x_nm": 2, "y_nm": -2})
        );
        assert!(point(10000.5, 0.0).is_err());
    }

    #[test]
    fn requests_use_protocol_names() {
        for name in [
            "OpenDocument",
            "OpenDocumentResponse",
            "FocusOnItems",
            "SaveDocument",
            "PlaceFromLibraryResponse",
            "GetPageLayout",
            "PageLayoutResponse",
            "GetPageSettings",
            "GetTitleBlockInfo",
            "SetPageSettings",
            "SetTitleBlockInfo",
            "UndoLastChange",
            "RedoLastChange",
            "UndoRedoResponse",
            "ClearSelection",
            "AddToSelection",
            "SelectionResponse",
            "HighlightNets",
            "HighlightNetsResponse",
        ] {
            proto::descriptor(&format!("{C}{name}"));
        }
        for name in [
            "PlaceSymbolFromLibrary",
            "AnnotateSchematic",
            "AnnotateSchematicResponse",
            "UpdateSymbolsFromLibrary",
            "UpdateSymbolsFromLibraryResponse",
            "AutoplaceFields",
            "TransformItems",
            "TransformItemsResponse",
            "GetSchematicNetlist",
            "SchematicNetlistResponse",
            "GetSymbolPins",
            "SymbolPinsResponse",
            "GetSchematicHierarchy",
            "SchematicHierarchyResponse",
        ] {
            proto::descriptor(&format!("{S}{name}"));
        }
        for name in [
            "kiapi.schematic.tasks.StartErcTask",
            "kiapi.schematic.tasks.GetErcTaskResult",
            "kiapi.schematic.tasks.ErcTaskResult",
            "kiapi.common.tasks.StartTaskResponse",
            "kiapi.common.tasks.GetTaskStatus",
            "kiapi.common.tasks.TaskStatusResponse",
            "kiapi.schematic.jobs.RunSchematicJobExportPng",
            "kiapi.common.types.RunJobResponse",
            "kiapi.common.types.PageSettings",
            "kiapi.common.types.TitleBlockInfo",
            EMPTY,
        ] {
            proto::descriptor(name);
        }
        assert_eq!(enum_number(&format!("{T}PageSize"), "PS_US_LEDGER"), 15);
        assert_eq!(
            enum_name(&format!("{T}PageSize"), 99)
                .unwrap_err()
                .to_string(),
            "Enum PageSize has no name defined for value 99"
        );
    }
}
