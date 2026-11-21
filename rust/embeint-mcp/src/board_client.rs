// SPDX-License-Identifier: GPL-3.0-or-later
//! PCB editor operations: a port of `BoardTools`, its item builders and `parse_board_items` from
//! `tools/live_schematic_mcp/board_client.py`.
//!
//! Everything works on the board open in the PCB editor, unsaved changes included; nothing reads or
//! writes .kicad_pcb files except [`board_save`].  Units are millimetres and degrees
//! (counter-clockwise); the board's Y axis points down.  KiCad stores nanometres: [`nm`] converts
//! as Python's `round(value * 1_000_000)` does (ties to even) and [`mm`] back as
//! `round(nm / 1e6, 6)`.
//!
//! The request order, the fields set (presence included), the results (keys in order, number
//! formatting) and the error texts are Python's; `rust/parity/scenarios/board.py` compares them.
//! Values that come from a caller's JSON objects (moves, shapes, points) are checked as Python
//! checks them, with the TypeError and KeyError texts it raises.
//!
//! The functions marked "helper" serve the board design tools (rules, stackup, routing) as well.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use embeint_ipc::client::{item_header, reply_json, Client, Error};
use embeint_ipc::pbjson::{self, py_repr};
use embeint_ipc::proto::{
    self, build, enum_name, int_field, list_field, message_field, str_field, with_field,
};
use prost_reflect::{DynamicMessage, MapKey, ReflectMessage, Value};
use serde_json::{json, Map, Value as Json};

use crate::tools::args::type_name;

const BC: &str = "kiapi.board.commands.";
const BT: &str = "kiapi.board.types.";
const BJ: &str = "kiapi.board.jobs.";
const C: &str = "kiapi.common.commands.";
const T: &str = "kiapi.common.types.";
const EMPTY: &str = "google.protobuf.Empty";
const RUN_JOB_RESPONSE: &str = "kiapi.common.types.RunJobResponse";

/// `LIMIT_MM`: the largest coordinate or size accepted, in millimetres.
pub(crate) const LIMIT_MM: f64 = 10000.0;
/// How long `board_drc` waits for KiCad's check (`timeout_s=300.0`).
const DRC_TIMEOUT: Duration = Duration::from_secs(300);

// ---- Python values ------------------------------------------------------------------------------

/// Python's truthiness of a JSON value (`if value:`).
pub(crate) fn truthy(value: &Json) -> bool {
    match value {
        Json::Null => false,
        Json::Bool(flag) => *flag,
        Json::Number(number) => number.as_f64().is_some_and(|v| v != 0.0),
        Json::String(text) => !text.is_empty(),
        Json::Array(items) => !items.is_empty(),
        Json::Object(map) => !map.is_empty(),
    }
}

/// A JSON value where Python needs a real number (`math.isfinite`, a double field): a bool is 0 or
/// 1; anything but a number is the TypeError Python raises.
pub(crate) fn real(value: &Json) -> Result<f64, Error> {
    match value {
        Json::Bool(flag) => Ok(f64::from(u8::from(*flag))),
        // A literal beyond the float range is infinite for Python's json too
        Json::Number(number) => Ok(number.as_f64().unwrap_or(f64::INFINITY)),
        other => Err(Error::Value(format!(
            "must be real number, not {}",
            type_name(other)
        ))),
    }
}

/// `str(value)`: a string as it is, anything else as its repr.
fn py_str(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        other => py_repr(other),
    }
}

/// `round(value, digits)` for a float: correctly rounded to that many decimals, ties to even (the
/// decimal formatting of Rust and Python's `round` both round the exact binary value).
pub(crate) fn py_round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// CPython's sum() of floats (3.12 and later): Neumaier's compensated summation from 0.  An empty
/// sum is the int 0 in Python; the caller decides how to show that.
pub(crate) fn py_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut compensation) = (0.0_f64, 0.0_f64);
    for x in values {
        let t = total + x;
        if total.abs() >= x.abs() {
            compensation += (total - t) + x;
        } else {
            compensation += (x - t) + total;
        }
        total = t;
    }
    if compensation != 0.0 && compensation.is_finite() {
        total += compensation;
    }
    total
}

/// `iter(value)`: a list's items, a string's characters, a dict's keys; None if not iterable.
fn py_iter(value: &Json) -> Option<Vec<Json>> {
    match value {
        Json::Array(items) => Some(items.clone()),
        Json::String(text) => Some(text.chars().map(|c| json!(c.to_string())).collect()),
        Json::Object(map) => Some(map.keys().map(|k| json!(k)).collect()),
        _ => None,
    }
}

/// A Python list repr of strings, e.g. `['thermal', 'solid']`.
fn py_list(items: &[&str]) -> String {
    py_repr(&json!(items))
}

/// `KeyError(key)` as `str()` shows it.
fn key_error(key: &str) -> Error {
    Error::Value(py_repr(&json!(key)))
}

/// `dict[key]` on a caller's object.
fn required<'a>(spec: &'a Map<String, Json>, key: &str) -> Result<&'a Json, Error> {
    spec.get(key).ok_or_else(|| key_error(key))
}

/// The name of an enum value as Python's `Enum.Name`, which refuses a number it does not know.
fn py_enum_name(full_name: &str, number: i64) -> Result<String, Error> {
    let short = full_name.rsplit('.').next().unwrap_or(full_name);
    proto::pool()
        .get_enum_by_name(full_name)
        // The first value declared with the number (file order), as Python names an aliased one
        .and_then(|e| {
            e.enum_descriptor_proto()
                .value
                .iter()
                .find(|v| i64::from(v.number()) == number)
                .map(|v| v.name().to_owned())
        })
        .ok_or_else(|| {
            Error::Value(format!(
                "Enum {short} has no name defined for value {number}"
            ))
        })
}

/// The number of an enum value (a name this module uses, so it exists).
fn enum_number(full_name: &str, name: &str) -> i32 {
    proto::pool()
        .get_enum_by_name(full_name)
        .and_then(|e| e.get_value_by_name(name))
        .map(|v| v.number())
        .unwrap_or_else(|| panic!("{name} is not a value of {full_name}"))
}

// ---- Units --------------------------------------------------------------------------------------

/// `_nm(value)`: millimetres to nanometres, refusing values that are not finite or beyond
/// +/-[`LIMIT_MM`].
pub(crate) fn nm(value: f64) -> Result<i64, Error> {
    if !value.is_finite() || value.abs() > LIMIT_MM {
        return Err(Error::Value(
            "Coordinates and sizes must be finite millimetres within +/-10000".into(),
        ));
    }
    Ok((value * 1_000_000.0).round_ties_even() as i64)
}

/// `_nm(value)` for a caller's JSON value.
pub(crate) fn nm_of(value: &Json) -> Result<i64, Error> {
    nm(real(value)?)
}

/// `_vec(x, y)`: a Vector2 as protobuf JSON.
pub(crate) fn vec2(x: f64, y: f64) -> Result<Json, Error> {
    let x = nm(x)?;
    Ok(json!({"x_nm": x, "y_nm": nm(y)?}))
}

fn vec_of(x: &Json, y: &Json) -> Result<Json, Error> {
    let x = nm_of(x)?;
    Ok(json!({"x_nm": x, "y_nm": nm_of(y)?}))
}

/// `_vec(*point)` for a caller's point (`[x, y]`), with Python's errors for other shapes.
pub(crate) fn vec_point(point: &Json) -> Result<Json, Error> {
    let Some(parts) = py_iter(point) else {
        return Err(Error::Value(format!(
            "board_client._vec() argument after * must be an iterable, not {}",
            type_name(point)
        )));
    };
    match parts.len() {
        2 => vec_of(&parts[0], &parts[1]),
        0 => Err(Error::Value(
            "_vec() missing 2 required positional arguments: 'x' and 'y'".into(),
        )),
        1 => Err(Error::Value(
            "_vec() missing 1 required positional argument: 'y'".into(),
        )),
        n => Err(Error::Value(format!(
            "_vec() takes 2 positional arguments but {n} were given"
        ))),
    }
}

/// `_vec(*point)` for a point of numbers.
pub(crate) fn vec_xy(point: &[f64]) -> Result<Json, Error> {
    vec_point(&json!(point))
}

/// `_mm(vector)`: a Vector2 in millimetres, `round(nm / 1e6, 6)`.
pub(crate) fn mm(vector: &DynamicMessage) -> [f64; 2] {
    [
        py_round(int_field(vector, "x_nm") as f64 / 1e6, 6),
        py_round(int_field(vector, "y_nm") as f64 / 1e6, 6),
    ]
}

/// A uint32/int32 constructor argument, refused out of range as Python does.
fn int_in_range(value: i64, min: i64, max: i64) -> Result<i64, Error> {
    if value < min || value > max {
        return Err(Error::Value(format!("Value out of range: {value}")));
    }
    Ok(value)
}

// ---- Messages -----------------------------------------------------------------------------------

/// A nested message field by a dotted path (the default message where unset).
pub(crate) fn sub(message: &DynamicMessage, path: &str) -> DynamicMessage {
    path.split('.')
        .fold(message.clone(), |m, name| message_field(&m, name))
}

/// A string field at a dotted path, e.g. `reference_field.text.text.text`.
pub(crate) fn text_at(message: &DynamicMessage, path: &str) -> String {
    let (parent, name) = path.rsplit_once('.').unwrap_or(("", path));
    let parent = if parent.is_empty() {
        message.clone()
    } else {
        sub(message, parent)
    };
    str_field(&parent, name)
}

/// A double (or float) field.
pub(crate) fn float_field(message: &DynamicMessage, name: &str) -> f64 {
    match message.get_field_by_name(name).as_deref() {
        Some(Value::F64(v)) => *v,
        Some(Value::F32(v)) => f64::from(*v),
        _ => 0.0,
    }
}

/// A repeated message field's messages.
pub(crate) fn messages(message: &DynamicMessage, name: &str) -> Vec<DynamicMessage> {
    list_field(message, name)
        .into_iter()
        .filter_map(|v| match v {
            Value::Message(m) => Some(m),
            _ => None,
        })
        .collect()
}

/// A repeated enum field's numbers.
fn enum_list(message: &DynamicMessage, name: &str) -> Vec<i64> {
    list_field(message, name)
        .into_iter()
        .filter_map(|v| match v {
            Value::EnumNumber(n) => Some(i64::from(n)),
            _ => None,
        })
        .collect()
}

fn has_field(message: &DynamicMessage, name: &str) -> bool {
    message.has_field_by_name(name)
}

/// A request with its `board` document field set.
fn board_request(name: &str, value: Json, doc: &DynamicMessage) -> DynamicMessage {
    with_field(build(&format!("{BC}{name}"), value), "board", doc.clone())
}

/// A request with its item `header` set.
fn header_request(full_name: &str, value: Json, doc: &DynamicMessage) -> DynamicMessage {
    with_field(build(full_name, value), "header", item_header(doc))
}

/// `KIID(value=...)` list for a request.
fn kiids(ids: &[String]) -> Json {
    Json::Array(ids.iter().map(|id| json!({"value": id})).collect())
}

/// `_id()`: a new random item id.
pub(crate) fn new_id() -> Json {
    json!({"value": uuid::Uuid::new_v4().to_string()})
}

/// An item's `id.value`.
pub(crate) fn item_id(item: &DynamicMessage) -> String {
    str_field(&message_field(item, "id"), "value")
}

fn item_ids(items: &[DynamicMessage]) -> Json {
    Json::Array(items.iter().map(|item| json!(item_id(item))).collect())
}

// ---- Layers -------------------------------------------------------------------------------------

/// `LAYER_ALIASES`: names users write for KiCad's layer names.
const LAYER_ALIASES: &[(&str, &str)] = &[
    ("F.Silkscreen", "F.SilkS"),
    ("B.Silkscreen", "B.SilkS"),
    ("F.Courtyard", "F.CrtYd"),
    ("B.Courtyard", "B.CrtYd"),
    ("User.Drawings", "Dwgs.User"),
    ("User.Comments", "Cmts.User"),
    ("User.Eco1", "Eco1.User"),
    ("User.Eco2", "Eco2.User"),
    ("Edge", "Edge.Cuts"),
    ("Top", "F.Cu"),
    ("Bottom", "B.Cu"),
];

const BOARD_LAYER: &str = "kiapi.board.types.BoardLayer";

/// `layer(name)`: the BoardLayer number of a KiCad layer name (F.Cu, In1.Cu, Edge.Cuts, F.SilkS,
/// User.1 or an alias).
pub(crate) fn layer(name: &str) -> Result<i32, Error> {
    let name = LAYER_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name, |(_, canonical)| canonical);
    let key = format!("BL_{}", name.replacen('.', "_", 1));
    proto::pool()
        .get_enum_by_name(BOARD_LAYER)
        .and_then(|e| e.get_value_by_name(&key))
        .map(|v| v.number())
        .ok_or_else(|| {
            Error::Value(format!(
                "Unknown layer {}; use KiCad names like F.Cu, In1.Cu, B.SilkS, Edge.Cuts",
                py_repr(&json!(name))
            ))
        })
}

/// `layer(value)` for a caller's JSON value, with Python's errors for one that is not a string.
fn layer_of(value: &Json) -> Result<i32, Error> {
    match value {
        Json::String(name) => layer(name),
        Json::Array(_) | Json::Object(_) => Err(Error::Value(format!(
            "unhashable type: '{}'",
            type_name(value)
        ))),
        other => Err(Error::Value(format!(
            "'{}' object has no attribute 'replace'",
            type_name(other)
        ))),
    }
}

/// `layer_name(value)`: BL_F_Cu is F.Cu, BL_User_1 User.1, BL_UNKNOWN UNKNOWN.
pub(crate) fn layer_name(value: i64) -> Result<String, Error> {
    let name = py_enum_name(BOARD_LAYER, value)?;
    let name = name.strip_prefix("BL_").unwrap_or(&name);
    Ok(if name.contains('_') {
        name.replacen('_', ".", 1)
    } else {
        name.to_owned()
    })
}

/// `is_copper(value)`.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) fn is_copper(value: i32) -> bool {
    enum_name(BOARD_LAYER, i64::from(value)).ends_with("_Cu")
}

fn b_cu() -> i64 {
    i64::from(enum_number(BOARD_LAYER, "BL_B_Cu"))
}

// ---- Item construction --------------------------------------------------------------------------

fn board_item(name: &str, value: Json) -> DynamicMessage {
    build(&format!("{BT}{name}"), value)
}

/// `make_track(start, end, width, layer, net)`: one track segment (points `[x, y]` in mm).
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) fn make_track(
    start: &Json,
    end: &Json,
    width: f64,
    layer: i32,
    net: Option<&str>,
) -> Result<DynamicMessage, Error> {
    let id = new_id();
    let start = vec_point(start)?;
    let end = vec_point(end)?;
    let width = nm(width)?;
    Ok(board_item(
        "Track",
        json!({"id": id, "start": start, "end": end, "width": {"value_nm": width}, "layer": layer,
               "net": {"name": net.unwrap_or("")}}),
    ))
}

/// `make_via(position, net, size, drill, start, end)`: a through via (blind/buried unless it
/// spans F.Cu to B.Cu), its pad stack sized on F.Cu.
pub(crate) fn make_via(
    position: &Json,
    net: Option<&str>,
    size: f64,
    drill: f64,
    start: &str,
    end: &str,
) -> Result<DynamicMessage, Error> {
    let id = new_id();
    let position = vec_point(position)?;
    let through = start == "F.Cu" && end == "B.Cu";
    let diameter = vec2(drill, drill)?;
    let start_layer = layer(start)?;
    let end_layer = layer(end)?;
    let size = vec2(size, size)?;
    Ok(board_item(
        "Via",
        json!({
            "id": id, "position": position, "net": {"name": net.unwrap_or("")},
            "type": if through { "VT_THROUGH" } else { "VT_BLIND_BURIED" },
            "pad_stack": {
                "type": "PST_NORMAL",
                "drill": {"diameter": diameter, "shape": "DS_CIRCLE", "start_layer": start_layer,
                          "end_layer": end_layer},
                "copper_layers": [{"layer": "BL_F_Cu", "shape": "PSS_CIRCLE", "size": size}],
            },
        }),
    ))
}

/// A BoardGraphicShape with one geometry, a solid stroke and (rectangles, circles, polygons) a
/// fill.  `width` is converted after the geometry, as Python sets them.
fn graphic(
    layer: i32,
    geometry: &str,
    value: Json,
    width: &Json,
    fill: Option<bool>,
) -> Result<DynamicMessage, Error> {
    let mut attributes =
        json!({"stroke": {"width": {"value_nm": nm_of(width)?}, "style": "SLS_SOLID"}});
    if let Some(filled) = fill {
        attributes["fill"] =
            json!({"fill_type": if filled { "GFT_FILLED" } else { "GFT_UNFILLED" }});
    }
    let mut shape = Map::new();
    shape.insert(geometry.into(), value);
    shape.insert("attributes".into(), attributes);
    Ok(board_item(
        "BoardGraphicShape",
        json!({"id": new_id(), "layer": layer, "shape": shape}),
    ))
}

/// `make_segment(start, end, layer, width)`.
pub(crate) fn make_segment(
    start: &Json,
    end: &Json,
    layer: i32,
    width: &Json,
) -> Result<DynamicMessage, Error> {
    let start = vec_point(start)?;
    let end = vec_point(end)?;
    graphic(
        layer,
        "segment",
        json!({"start": start, "end": end}),
        width,
        None,
    )
}

/// `make_arc(start, mid, end, layer, width)`.
pub(crate) fn make_arc(
    start: &Json,
    mid: &Json,
    end: &Json,
    layer: i32,
    width: &Json,
) -> Result<DynamicMessage, Error> {
    let start = vec_point(start)?;
    let mid = vec_point(mid)?;
    let end = vec_point(end)?;
    graphic(
        layer,
        "arc",
        json!({"start": start, "mid": mid, "end": end}),
        width,
        None,
    )
}

/// `make_rectangle(x1, y1, x2, y2, layer, width, filled)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_rectangle(
    x1: &Json,
    y1: &Json,
    x2: &Json,
    y2: &Json,
    layer: i32,
    width: &Json,
    filled: bool,
) -> Result<DynamicMessage, Error> {
    let top_left = vec_of(x1, y1)?;
    let bottom_right = vec_of(x2, y2)?;
    graphic(
        layer,
        "rectangle",
        json!({"top_left": top_left, "bottom_right": bottom_right}),
        width,
        Some(filled),
    )
}

/// `make_circle(center, radius, layer, width, filled)`: the radius point is to the right.
pub(crate) fn make_circle(
    center: &Json,
    radius: &Json,
    layer: i32,
    width: &Json,
    filled: bool,
) -> Result<DynamicMessage, Error> {
    let center_vec = vec_point(center)?;
    // vec_point accepted it, so center is [x, y] of numbers
    let parts = py_iter(center).unwrap_or_default();
    let x = match radius {
        Json::Number(_) | Json::Bool(_) => real(&parts[0])? + real(radius)?,
        other => {
            return Err(Error::Value(format!(
                "unsupported operand type(s) for +: '{}' and '{}'",
                type_name(&parts[0]),
                type_name(other)
            )))
        }
    };
    let radius_point = vec_of(&json!(x), &parts[1])?;
    graphic(
        layer,
        "circle",
        json!({"center": center_vec, "radius_point": radius_point}),
        width,
        Some(filled),
    )
}

/// A closed PolyLine through the points (`_vec(*p)` each).
fn closed_outline(points: &Json) -> Result<Json, Error> {
    let Some(points) = py_iter(points) else {
        return Err(Error::Value(format!(
            "'{}' object is not iterable",
            type_name(points)
        )));
    };
    let nodes = points
        .iter()
        .map(|p| vec_point(p).map(|v| json!({"point": v})))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"outline": {"nodes": nodes, "closed": true}}))
}

/// `make_polygon(points, layer, width, filled)`.
pub(crate) fn make_polygon(
    points: &Json,
    layer: i32,
    width: &Json,
    filled: bool,
) -> Result<DynamicMessage, Error> {
    let polygon = closed_outline(points)?;
    graphic(
        layer,
        "polygon",
        json!({"polygons": [polygon]}),
        width,
        Some(filled),
    )
}

/// `make_text(content, position, layer, size, rotation, thickness, mirrored)`: centred text;
/// thickness defaults to 15% of the size, and text on a back layer is mirrored.
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_text(
    content: &Json,
    position: &Json,
    layer: i32,
    size: &Json,
    rotation: &Json,
    thickness: Option<&Json>,
    mirrored: Option<bool>,
) -> Result<DynamicMessage, Error> {
    let id = new_id();
    let Json::String(content) = content else {
        return Err(Error::Value(
            "bad argument type for built-in operation".into(),
        ));
    };
    let position = vec_point(position)?;
    let size_vec = vec_of(size, size)?;
    let stroke = match thickness {
        Some(thickness) => nm_of(thickness)?,
        None => nm(py_round(real(size)? * 0.15, 3))?,
    };
    let angle = real(rotation)?;
    let mirrored = match mirrored {
        Some(flag) => flag,
        None => layer_name(i64::from(layer))?.starts_with("B."),
    };
    Ok(board_item(
        "BoardText",
        json!({"id": id, "layer": layer, "text": {
            "text": content, "position": position,
            "attributes": {"size": size_vec, "stroke_width": {"value_nm": stroke},
                           "horizontal_alignment": "HA_CENTER", "vertical_alignment": "VA_CENTER",
                           "angle": {"value_degrees": angle}, "visible": true, "mirrored": mirrored},
        }}),
    ))
}

/// `rounded_rectangle(...)`: an Edge.Cuts style outline, four lines and four quarter arcs (a plain
/// rectangle without a radius).
#[allow(clippy::too_many_arguments)]
pub(crate) fn rounded_rectangle(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
    layer: i32,
    line_width: f64,
) -> Result<Vec<DynamicMessage>, Error> {
    let w = json!(line_width);
    if radius <= 0.0 {
        return Ok(vec![make_rectangle(
            &json!(x),
            &json!(y),
            &json!(x + width),
            &json!(y + height),
            layer,
            &w,
            false,
        )?]);
    }
    if radius * 2.0 > width.min(height) {
        return Err(Error::Value(
            "corner_radius is larger than half the board's smaller side".into(),
        ));
    }
    let (r, x2, y2) = (radius, x + width, y + height);
    let s = r * (1.0 - 0.5_f64.sqrt());
    let p = |a: f64, b: f64| json!([a, b]);
    Ok(vec![
        make_segment(&p(x + r, y), &p(x2 - r, y), layer, &w)?,
        make_segment(&p(x2, y + r), &p(x2, y2 - r), layer, &w)?,
        make_segment(&p(x2 - r, y2), &p(x + r, y2), layer, &w)?,
        make_segment(&p(x, y2 - r), &p(x, y + r), layer, &w)?,
        make_arc(&p(x2 - r, y), &p(x2 - s, y + s), &p(x2, y + r), layer, &w)?,
        make_arc(
            &p(x2, y2 - r),
            &p(x2 - s, y2 - s),
            &p(x2 - r, y2),
            layer,
            &w,
        )?,
        make_arc(&p(x + r, y2), &p(x + s, y2 - s), &p(x, y2 - r), layer, &w)?,
        make_arc(&p(x, y + r), &p(x + s, y + s), &p(x + r, y), layer, &w)?,
    ])
}

/// What a rule area (keepout) keeps out (`rule_area` of `make_zone`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RuleArea {
    pub copper: bool,
    pub vias: bool,
    pub tracks: bool,
    pub pads: bool,
    pub footprints: bool,
}

/// The copper settings of `make_zone`, with Python's defaults.
#[derive(Clone, Debug)]
pub(crate) struct ZoneSettings<'a> {
    pub net: Option<&'a str>,
    pub name: &'a str,
    pub priority: i64,
    pub clearance: f64,
    pub min_width: f64,
    pub thermal_gap: f64,
    pub thermal_spoke: f64,
    pub connection: &'a str,
}

impl Default for ZoneSettings<'_> {
    fn default() -> Self {
        ZoneSettings {
            net: None,
            name: "",
            priority: 0,
            clearance: 0.2,
            min_width: 0.25,
            thermal_gap: 0.5,
            thermal_spoke: 0.5,
            connection: "thermal",
        }
    }
}

/// `CONNECTION_MODES` of `make_zone`, in Python's order.
const ZONE_CONNECTIONS: &[(&str, &str)] = &[
    ("thermal", "ZCS_THERMAL"),
    ("solid", "ZCS_FULL"),
    ("none", "ZCS_NONE"),
    ("pth_thermal", "ZCS_PTH_THERMAL"),
];

/// `make_zone(points, layers, ...)`: a copper zone, or with `rule_area` a keepout.  Points are
/// caller values (`_vec(*p)` each).
pub(crate) fn make_zone(
    points: &Json,
    layers: &[String],
    settings: &ZoneSettings<'_>,
    rule_area: Option<RuleArea>,
) -> Result<DynamicMessage, Error> {
    let id = new_id();
    let priority = int_in_range(settings.priority, 0, i64::from(u32::MAX))?;
    let layers = layers
        .iter()
        .map(|n| layer(n))
        .collect::<Result<Vec<_>, _>>()?;
    let outline = closed_outline(points)?;
    let mut zone = json!({"id": id, "name": settings.name, "priority": priority, "layers": layers,
                          "outline": {"polygons": [outline]}});
    if let Some(rule) = rule_area {
        zone["type"] = json!("ZT_RULE_AREA");
        zone["rule_area_settings"] = json!({
            "keepout_copper": rule.copper, "keepout_vias": rule.vias, "keepout_tracks": rule.tracks,
            "keepout_pads": rule.pads, "keepout_footprints": rule.footprints,
        });
    } else {
        zone["type"] = json!("ZT_COPPER");
        let clearance = nm(settings.clearance)?;
        let min_thickness = nm(settings.min_width)?;
        let Some((_, mode)) = ZONE_CONNECTIONS
            .iter()
            .find(|(name, _)| *name == settings.connection)
        else {
            let names: Vec<&str> = ZONE_CONNECTIONS.iter().map(|(name, _)| *name).collect();
            return Err(Error::Value(format!(
                "connection must be one of {}",
                py_list(&names)
            )));
        };
        let spoke = nm(settings.thermal_spoke)?;
        let gap = nm(settings.thermal_gap)?;
        zone["copper_settings"] = json!({
            "net": {"name": settings.net.unwrap_or("")},
            "clearance": {"value_nm": clearance}, "min_thickness": {"value_nm": min_thickness},
            "fill_mode": "ZFM_SOLID", "island_mode": "IRM_ALWAYS",
            "connection": {"zone_connection": mode, "thermal_spokes": {
                "width": {"value_nm": spoke}, "gap": {"value_nm": gap}}},
        });
    }
    zone["border"] = json!({"style": "ZBS_DIAGONAL_EDGE", "pitch": {"value_nm": nm(0.5)?}});
    Ok(board_item("Zone", zone))
}

/// `route_points(start, end, posture)`: a 45-degree dog-leg, straight then diagonal
/// (`horizontal_first`, the default for any other posture), diagonal then straight
/// (`diagonal_first`) or along X then Y (`orthogonal`).
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) fn route_points(start: [f64; 2], end: [f64; 2], posture: &str) -> Vec<[f64; 2]> {
    let ([x1, y1], [x2, y2]) = (start, end);
    let (dx, dy) = (x2 - x1, y2 - y1);
    if dx.abs() < 1e-9 || dy.abs() < 1e-9 || (dx.abs() - dy.abs()).abs() < 1e-9 {
        return vec![start, end];
    }
    let diagonal = dx.abs().min(dy.abs());
    let (sx, sy) = (1.0_f64.copysign(dx), 1.0_f64.copysign(dy));
    let corner = match posture {
        "diagonal_first" => [x1 + sx * diagonal, y1 + sy * diagonal],
        "orthogonal" => [x2, y1],
        _ => {
            let straight = if dx.abs() > dy.abs() {
                (dx.abs() - diagonal, 0.0)
            } else {
                (0.0, dy.abs() - diagonal)
            };
            [x1 + sx * straight.0, y1 + sy * straight.1]
        }
    };
    vec![start, corner, end]
}

/// `ITEM_TYPES`: item kind and KiCadObjectType.
const ITEM_TYPES: &[(&str, &str)] = &[
    ("footprints", "KOT_PCB_FOOTPRINT"),
    ("pads", "KOT_PCB_PAD"),
    ("tracks", "KOT_PCB_TRACE"),
    ("arcs", "KOT_PCB_ARC"),
    ("vias", "KOT_PCB_VIA"),
    ("zones", "KOT_PCB_ZONE"),
    ("shapes", "KOT_PCB_SHAPE"),
    ("texts", "KOT_PCB_TEXT"),
    ("textboxes", "KOT_PCB_TEXTBOX"),
    ("dimensions", "KOT_PCB_DIMENSION"),
    ("groups", "KOT_PCB_GROUP"),
    ("tables", "KOT_PCB_TABLE"),
    ("images", "KOT_PCB_REFERENCE_IMAGE"),
];

/// `ITEM_MESSAGES`: the kinds `board_unpacked` reads, and their message.
const ITEM_MESSAGES: &[(&str, &str)] = &[
    ("footprints", "FootprintInstance"),
    ("pads", "Pad"),
    ("tracks", "Track"),
    ("arcs", "Arc"),
    ("vias", "Via"),
    ("zones", "Zone"),
    ("shapes", "BoardGraphicShape"),
    ("texts", "BoardText"),
];

/// `summarize_footprint_instance(fp, pads)`.
pub(crate) fn summarize_footprint_instance(fp: &DynamicMessage, pads: Option<Vec<Json>>) -> Json {
    let definition = sub(fp, "definition.id");
    let mut entry = Map::new();
    entry.insert("id".into(), json!(item_id(fp)));
    entry.insert(
        "reference".into(),
        json!(text_at(fp, "reference_field.text.text.text")),
    );
    entry.insert(
        "value".into(),
        json!(text_at(fp, "value_field.text.text.text")),
    );
    entry.insert(
        "lib_id".into(),
        json!(format!(
            "{}:{}",
            str_field(&definition, "library_nickname"),
            str_field(&definition, "entry_name")
        )),
    );
    entry.insert(
        "position_mm".into(),
        json!(mm(&message_field(fp, "position"))),
    );
    entry.insert(
        "rotation".into(),
        json!(py_round(
            float_field(&message_field(fp, "orientation"), "value_degrees"),
            4
        )),
    );
    entry.insert(
        "side".into(),
        json!(if int_field(fp, "layer") == b_cu() {
            "back"
        } else {
            "front"
        }),
    );
    if int_field(fp, "locked") == i64::from(enum_number(&format!("{T}LockedState"), "LS_LOCKED")) {
        entry.insert("locked".into(), json!(true));
    }
    if int_field(&message_field(fp, "attributes"), "do_not_populate") != 0 {
        entry.insert("dnp".into(), json!(true));
    }
    if let Some(pads) = pads {
        entry.insert("pads".into(), Json::Array(pads));
    }
    Json::Object(entry)
}

/// `summarize_pad(pad)`.
pub(crate) fn summarize_pad(pad: &DynamicMessage) -> Result<Json, Error> {
    let stack = message_field(pad, "pad_stack");
    let copper = messages(&stack, "copper_layers").into_iter().next();
    let kind = py_enum_name(&format!("{BT}PadType"), int_field(pad, "type"))?;
    let mut entry = Map::new();
    entry.insert("number".into(), json!(str_field(pad, "number")));
    entry.insert("net".into(), json!(text_at(pad, "net.name")));
    entry.insert(
        "position_mm".into(),
        json!(mm(&message_field(pad, "position"))),
    );
    entry.insert(
        "type".into(),
        json!(kind.strip_prefix("PT_").unwrap_or(&kind)),
    );
    if let Some(copper) = copper {
        entry.insert("size_mm".into(), json!(mm(&message_field(&copper, "size"))));
        let shape = py_enum_name(&format!("{BT}PadStackShape"), int_field(&copper, "shape"))?;
        entry.insert(
            "shape".into(),
            json!(shape.strip_prefix("PSS_").unwrap_or(&shape).to_lowercase()),
        );
    }
    let drill = int_field(&sub(&stack, "drill.diameter"), "x_nm");
    if drill != 0 {
        entry.insert("drill_mm".into(), json!(py_round(drill as f64 / 1e6, 4)));
    }
    Ok(Json::Object(entry))
}

/// `parse_board_items(values)`: protobuf-JSON Any objects of top-level kiapi.board.types items.
pub(crate) fn parse_board_items(values: &[Json]) -> Result<Vec<DynamicMessage>, Error> {
    let file = proto::pool()
        .get_file_by_name("board/board_types.proto")
        .expect("board types in the protocol");
    let any = proto::descriptor("google.protobuf.Any");
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let packed = pbjson::from_json(value, &any)?;
        let type_name = proto::any_type_name(&packed);
        let name = type_name.strip_prefix(BT).unwrap_or(&type_name);
        let descriptor = file.messages().find(|m| m.name() == name);
        let Some(descriptor) = descriptor.filter(|d| d.full_name() == type_name) else {
            return Err(Error::Value(
                "Only kiapi.board.types item types are accepted".into(),
            ));
        };
        let message = proto::unpack(&packed, descriptor.full_name())
            .ok_or_else(|| Error::Value("Invalid board item".into()))?;
        result.push(message);
    }
    Ok(result)
}

// ---- Transactions -------------------------------------------------------------------------------

/// The outcome of a transaction's steps, for [`Client::finish_transaction`].
fn outcome<V>(result: &Result<V, Error>) -> Result<(), Error> {
    result.as_ref().map(|_| ()).map_err(Clone::clone)
}

/// The errors of a CreateItems/TransformBoardItems reply's per-item statuses (`error_message`, or
/// the status code's name).
fn item_errors(results: &[DynamicMessage]) -> Vec<String> {
    results
        .iter()
        .filter_map(|result| {
            let status = message_field(result, "status");
            let code = int_field(&status, "code");
            (code != 1).then(|| {
                let message = str_field(&status, "error_message");
                if message.is_empty() {
                    enum_name(&format!("{C}ItemStatusCode"), code)
                } else {
                    message
                }
            })
        })
        .collect()
}

// ---- Documents ----------------------------------------------------------------------------------

/// `open_board(path)`: opens (or creates) the project's board in the PCB editor.
pub(crate) async fn open_board(client: &mut Client, path: Option<&str>) -> Result<Json, Error> {
    let request = build(
        &format!("{C}OpenDocument"),
        json!({"type": "DOCTYPE_PCB", "path": path.unwrap_or("")}),
    );
    match client
        .send(&request, &format!("{C}OpenDocumentResponse"))
        .await
    {
        Ok(response) => reply_json(&message_field(&response, "document")),
        Err(error)
            if error.is_runtime_error()
                && (error.to_string().contains("AS_UNIMPLEMENTED")
                    || error.to_string().contains("AS_UNHANDLED")) =>
        {
            Err(Error::Runtime(
                "This KiCad cannot open the PCB editor over the API. Start KiCad's project \
                 manager with the project (run_kicad_wsl.sh or run_kicad_windows.ps1 with a \
                 .kicad_pro), not the schematic editor on its own."
                    .into(),
            ))
        }
        Err(error) => Err(error),
    }
}

/// `board_save(doc)`.
pub(crate) async fn board_save(client: &mut Client, doc: &DynamicMessage) -> Result<Json, Error> {
    let request = with_field(
        build(&format!("{C}SaveDocument"), json!({})),
        "document",
        doc.clone(),
    );
    client.send(&request, EMPTY).await?;
    Ok(json!({"saved": str_field(doc, "board_filename")}))
}

// ---- Reading ------------------------------------------------------------------------------------

/// `board_items(doc, kinds)`: the board's items of these kinds, as packed `Any` messages.
pub(crate) async fn board_items(
    client: &mut Client,
    doc: &DynamicMessage,
    kinds: &[String],
) -> Result<Vec<DynamicMessage>, Error> {
    let mut types = Vec::new();
    for kind in kinds {
        let Some((_, object_type)) = ITEM_TYPES.iter().find(|(name, _)| name == kind) else {
            let mut names: Vec<&str> = ITEM_TYPES.iter().map(|(name, _)| *name).collect();
            names.sort_unstable();
            return Err(Error::Value(format!(
                "item kind must be one of {}",
                py_list(&names)
            )));
        };
        types.push(json!(object_type));
    }
    let request = header_request(&format!("{C}GetItems"), json!({"types": types}), doc);
    let response = client
        .send(&request, &format!("{C}GetItemsResponse"))
        .await?;
    let status = int_field(&response, "status");
    if status != 1 {
        return Err(Error::Runtime(format!(
            "GetItems failed: {}",
            enum_name(&format!("{T}ItemRequestStatus"), status)
        )));
    }
    Ok(messages(&response, "items"))
}

/// Helper: `board_unpacked(doc, kind)`: the items of one kind (footprints, pads, tracks, arcs,
/// vias, zones, shapes, texts), unpacked; items of another type are skipped.
pub(crate) async fn board_unpacked(
    client: &mut Client,
    doc: &DynamicMessage,
    kind: &str,
) -> Result<Vec<DynamicMessage>, Error> {
    let Some((_, message)) = ITEM_MESSAGES.iter().find(|(name, _)| *name == kind) else {
        return Err(key_error(kind));
    };
    let full_name = format!("{BT}{message}");
    Ok(board_items(client, doc, &[kind.to_owned()])
        .await?
        .iter()
        .filter_map(|packed| proto::unpack(packed, &full_name))
        .collect())
}

/// Helper: `board_footprints(doc, references, with_pads)`: footprint summaries sorted by reference
/// (only the given references when there are any), with their pads sorted by number.
pub(crate) async fn board_footprints(
    client: &mut Client,
    doc: &DynamicMessage,
    references: Option<&[String]>,
    with_pads: bool,
) -> Result<Vec<Json>, Error> {
    let mut footprints = board_unpacked(client, doc, "footprints").await?;
    if let Some(wanted) = references.filter(|r| !r.is_empty()) {
        footprints.retain(|fp| wanted.contains(&text_at(fp, "reference_field.text.text.text")));
    }
    let mut pads_by_parent: Vec<(String, Vec<Json>)> = Vec::new();
    if with_pads {
        for pad in board_unpacked(client, doc, "pads").await? {
            let parent = str_field(&message_field(&pad, "parent"), "value");
            let summary = summarize_pad(&pad)?;
            match pads_by_parent.iter_mut().find(|(id, _)| *id == parent) {
                Some((_, pads)) => pads.push(summary),
                None => pads_by_parent.push((parent, vec![summary])),
            }
        }
    }
    let number = |pad: &Json| pad["number"].as_str().unwrap_or_default().to_owned();
    let mut result: Vec<Json> = footprints
        .iter()
        .map(|fp| {
            let pads = with_pads.then(|| {
                let id = item_id(fp);
                let mut pads = pads_by_parent
                    .iter()
                    .find(|(parent, _)| *parent == id)
                    .map(|(_, pads)| pads.clone())
                    .unwrap_or_default();
                // (len(number), number): code points, as Python counts and compares them
                pads.sort_by_key(|p| {
                    let n = number(p);
                    (n.chars().count(), n)
                });
                pads
            });
            summarize_footprint_instance(fp, pads)
        })
        .collect();
    result.sort_by_key(|f| f["reference"].as_str().unwrap_or_default().to_owned());
    Ok(result)
}

/// Helper: `board_footprint(doc, reference)`: the footprint with this reference.
pub(crate) async fn board_footprint(
    client: &mut Client,
    doc: &DynamicMessage,
    reference: &str,
) -> Result<DynamicMessage, Error> {
    board_unpacked(client, doc, "footprints")
        .await?
        .into_iter()
        .find(|fp| text_at(fp, "reference_field.text.text.text") == reference)
        .ok_or_else(|| Error::Value(format!("No footprint {reference} on the board")))
}

/// Helper: `find_pad(doc, reference, number)`: a footprint's pad.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn find_pad(
    client: &mut Client,
    doc: &DynamicMessage,
    reference: &str,
    number: &str,
) -> Result<DynamicMessage, Error> {
    let fp = board_footprint(client, doc, reference).await?;
    let id = item_id(&fp);
    board_unpacked(client, doc, "pads")
        .await?
        .into_iter()
        .find(|pad| {
            str_field(&message_field(pad, "parent"), "value") == id
                && str_field(pad, "number") == number
        })
        .ok_or_else(|| Error::Value(format!("No pad {reference}.{number}")))
}

/// A NetClassForNetsResponse's class for a net name.
fn class_for(response: &DynamicMessage, net: &str) -> Option<DynamicMessage> {
    match response.get_field_by_name("classes").as_deref() {
        Some(Value::Map(map)) => match map.get(&MapKey::String(net.to_owned())) {
            Some(Value::Message(class)) => Some(class.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// `board_nets(doc)`: named nets with their code and net class.
pub(crate) async fn board_nets(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Vec<Json>, Error> {
    let response = client
        .send(
            &board_request("GetNets", json!({}), doc),
            &format!("{BC}NetsResponse"),
        )
        .await?;
    let nets = messages(&response, "nets");
    let mut classes = None;
    if !nets.is_empty() {
        let mut request = build(&format!("{BC}GetNetClassForNets"), json!({}));
        request.set_field_by_name(
            "net",
            Value::List(nets.iter().cloned().map(Value::Message).collect()),
        );
        classes = Some(
            client
                .send(&request, &format!("{BC}NetClassForNetsResponse"))
                .await?,
        );
    }
    let mut result = Vec::new();
    for net in &nets {
        let name = str_field(net, "name");
        if name.is_empty() {
            continue;
        }
        let mut entry = Map::new();
        entry.insert("name".into(), json!(name));
        entry.insert(
            "code".into(),
            json!(int_field(&message_field(net, "code"), "value")),
        );
        if let Some(class) = classes.as_ref().and_then(|c| class_for(c, &name)) {
            entry.insert("netclass".into(), json!(str_field(&class, "name")));
        }
        result.push(Json::Object(entry));
    }
    Ok(result)
}

/// A net class's board widths (`netclass_widths`), in mm; None where the class does not set one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NetclassWidths {
    pub track_width: Option<f64>,
    pub via_diameter: Option<f64>,
    pub via_drill: Option<f64>,
    pub clearance: Option<f64>,
}

/// Python's `value or default` for an optional width (None and 0.0 are falsy).
pub(crate) fn or_default(value: Option<f64>, default: f64) -> f64 {
    value.filter(|v| *v != 0.0).unwrap_or(default)
}

/// Helper: `netclass_widths(doc, net)`: the track width, via size and drill and clearance of the
/// net's class, or None when KiCad reports no class with board settings for it.
pub(crate) async fn netclass_widths(
    client: &mut Client,
    doc: &DynamicMessage,
    net: &str,
) -> Result<Option<NetclassWidths>, Error> {
    let _ = doc; // as Python, the request names only the net
    let request = build(
        &format!("{BC}GetNetClassForNets"),
        json!({"net": [{"name": net}]}),
    );
    let response = client
        .send(&request, &format!("{BC}NetClassForNetsResponse"))
        .await?;
    let Some(class) = class_for(&response, net).filter(|c| has_field(c, "board")) else {
        return Ok(None);
    };
    let board = message_field(&class, "board");
    let distance = |name: &str| {
        has_field(&board, name)
            .then(|| int_field(&message_field(&board, name), "value_nm") as f64 / 1e6)
    };
    let via = has_field(&board, "via_stack").then(|| message_field(&board, "via_stack"));
    let copper = via
        .as_ref()
        .and_then(|v| messages(v, "copper_layers").into_iter().next());
    let size_x = copper
        .map(|c| int_field(&message_field(&c, "size"), "x_nm"))
        .unwrap_or(0);
    let drill_x = via
        .as_ref()
        .map(|v| int_field(&sub(v, "drill.diameter"), "x_nm"))
        .unwrap_or(0);
    Ok(Some(NetclassWidths {
        track_width: distance("track_width"),
        via_diameter: (size_x != 0).then(|| size_x as f64 / 1e6),
        via_drill: (drill_x != 0).then(|| drill_x as f64 / 1e6),
        clearance: distance("clearance"),
    }))
}

/// Helper: `board_box(doc, mode)`: the bounding box `[x1, y1, x2, y2]` (mm, 4 decimals) of the
/// board edges (`edges`), the physical layers (`physical`) or everything (`all`); None for an empty
/// board.
pub(crate) async fn board_box(
    client: &mut Client,
    doc: &DynamicMessage,
    mode: &str,
) -> Result<Option<[f64; 4]>, Error> {
    let mode_value = match mode {
        "edges" => "BBBM_BOARD_EDGES_ONLY",
        "physical" => "BBBM_PHYSICAL_LAYERS_ONLY",
        "all" => "BBBM_ALL_ITEMS",
        _ => return Err(key_error(mode)),
    };
    let response = client
        .send(
            &board_request("GetBoardBoundingBox", json!({"mode": mode_value}), doc),
            &format!("{BC}BoardBoundingBoxResponse"),
        )
        .await?;
    let bbox = message_field(&response, "box");
    let size = message_field(&bbox, "size");
    let (w, h) = (int_field(&size, "x_nm"), int_field(&size, "y_nm"));
    if w == 0 && h == 0 {
        return Ok(None);
    }
    let position = message_field(&bbox, "position");
    let x = int_field(&position, "x_nm") as f64 / 1e6;
    let y = int_field(&position, "y_nm") as f64 / 1e6;
    Ok(Some([
        py_round(x, 4),
        py_round(y, 4),
        py_round(x + w as f64 / 1e6, 4),
        py_round(y + h as f64 / 1e6, 4),
    ]))
}

/// `board_summary(doc)`.
pub(crate) async fn board_summary(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Json, Error> {
    let mut counts = Map::new();
    for kind in [
        "footprints",
        "tracks",
        "arcs",
        "vias",
        "zones",
        "shapes",
        "texts",
    ] {
        let items = board_items(client, doc, &[kind.to_owned()]).await?;
        counts.insert(kind.into(), json!(items.len()));
    }
    let enabled = client
        .send(
            &board_request("GetBoardEnabledLayers", json!({}), doc),
            &format!("{BC}BoardEnabledLayersResponse"),
        )
        .await?;
    let nets = board_nets(client, doc).await?;
    let outline = board_box(client, doc, "edges").await?;
    let modified = client
        .send(
            &with_field(
                build(&format!("{C}GetDocumentModifiedState"), json!({})),
                "document",
                doc.clone(),
            ),
            &format!("{C}GetDocumentModifiedStateResponse"),
        )
        .await?;
    let mut result = Map::new();
    result.insert("board".into(), json!(str_field(doc, "board_filename")));
    result.insert(
        "copper_layers".into(),
        json!(int_field(&enabled, "copper_layer_count")),
    );
    result.insert("outline_mm".into(), json!(outline));
    if let Some([x1, y1, x2, y2]) = outline {
        result.insert(
            "outline_size_mm".into(),
            json!([py_round(x2 - x1, 4), py_round(y2 - y1, 4)]),
        );
    }
    result.insert("counts".into(), Json::Object(counts));
    result.insert("nets".into(), json!(nets.len()));
    result.insert(
        "unsaved".into(),
        json!(
            enum_name(
                &format!("{C}DocumentModifiedState"),
                int_field(&modified, "state")
            ) == "DMS_MODIFIED"
        ),
    );
    result.insert(
        "footprints".into(),
        Json::Array(board_footprints(client, doc, None, false).await?),
    );
    Ok(Json::Object(result))
}

// ---- Editing ------------------------------------------------------------------------------------

/// Helper: `board_create(doc, items, description)`: creates the items as one undo step.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn board_create(
    client: &mut Client,
    doc: &DynamicMessage,
    items: &[DynamicMessage],
    description: &str,
) -> Result<Json, Error> {
    client.mutate(doc, items, false, Some(description)).await
}

/// Sets a field at a dotted path, creating the messages on the way.
fn set_path(message: &mut DynamicMessage, path: &str, value: Value) {
    let mut names: Vec<&str> = path.split('.').collect();
    let last = names.pop().unwrap_or_default();
    let mut current = message;
    for name in names {
        match current.get_field_by_name_mut(name) {
            Some(Value::Message(next)) => current = next,
            _ => return,
        }
    }
    current.set_field_by_name(last, value);
}

/// What `live_pcb_place_footprint` places.
pub(crate) struct Placement<'a> {
    pub lib_id: &'a str,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub side: &'a str,
    pub reference: Option<&'a str>,
    pub value: Option<&'a str>,
    pub locked: bool,
}

/// `board_place_footprint(...)`: places a library footprint as one undo step, then names it in a
/// second (the new footprint joins the board only when its commit is pushed).
pub(crate) async fn board_place_footprint(
    client: &mut Client,
    doc: &DynamicMessage,
    place: &Placement<'_>,
) -> Result<Json, Error> {
    let (library, entry) = place.lib_id.split_once(':').unwrap_or((place.lib_id, ""));
    if library.is_empty() || entry.is_empty() {
        return Err(Error::Value("lib_id must be Library:Footprint".into()));
    }
    if place.side != "front" && place.side != "back" {
        return Err(Error::Value("side must be front or back".into()));
    }
    let request = header_request(
        &format!("{BC}PlaceFootprintFromLibrary"),
        json!({
            "lib_id": {"library_nickname": library, "entry_name": entry},
            "position": vec2(place.x, place.y)?,
            "orientation": {"value_degrees": place.rotation},
            "layer": if place.side == "back" { "BL_B_Cu" } else { "BL_F_Cu" },
        }),
        doc,
    );
    let header = item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let step = client
        .send(&request, &format!("{C}PlaceFromLibraryResponse"))
        .await;
    client
        .finish_transaction(
            &header,
            &id,
            &operation_id,
            &format!("MCP: place {}", place.lib_id),
            outcome(&step),
        )
        .await?;
    let response = step?;
    let mut placed = proto::unpack(
        &message_field(&response, "item"),
        &format!("{BT}FootprintInstance"),
    )
    .ok_or_else(|| Error::Runtime("KiCad did not return the placed footprint".into()))?;
    let reference = place.reference.filter(|r| !r.is_empty());
    let value = place.value.filter(|v| !v.is_empty());
    if reference.is_some() || value.is_some() || place.locked {
        if let Some(reference) = reference {
            set_path(
                &mut placed,
                "reference_field.text.text.text",
                Value::String(reference.into()),
            );
        }
        if let Some(value) = value {
            set_path(
                &mut placed,
                "value_field.text.text.text",
                Value::String(value.into()),
            );
        }
        if place.locked {
            placed.set_field_by_name(
                "locked",
                Value::EnumNumber(enum_number(&format!("{T}LockedState"), "LS_LOCKED")),
            );
        }
        let description = format!("MCP: name {}", reference.unwrap_or(place.lib_id));
        client
            .mutate(doc, std::slice::from_ref(&placed), true, Some(&description))
            .await?;
    }
    Ok(summarize_footprint_instance(&placed, None))
}

/// Python's `(value + 180) % 360 - 180` for a caller's rotation (floats keep the divisor's sign).
fn normalized_rotation(value: &Json) -> Result<f64, Error> {
    let rotation = match value {
        Json::Number(_) | Json::Bool(_) => real(value)?,
        Json::String(_) | Json::Array(_) => {
            let kind = type_name(value);
            return Err(Error::Value(format!(
                "can only concatenate {kind} (not \"int\") to {kind}"
            )));
        }
        other => {
            return Err(Error::Value(format!(
                "unsupported operand type(s) for +: '{}' and 'int'",
                type_name(other)
            )))
        }
    };
    let shifted = rotation + 180.0;
    let mut remainder = shifted % 360.0;
    if remainder != 0.0 {
        if remainder < 0.0 {
            remainder += 360.0;
        }
    } else {
        remainder = 0.0;
    }
    Ok(remainder - 180.0)
}

/// `board_move_footprints(doc, moves)`: moves, rotates, flips and locks footprints in place
/// (TransformBoardItems keeps their pads and graphics with them) as one undo step.
pub(crate) async fn board_move_footprints(
    client: &mut Client,
    doc: &DynamicMessage,
    moves: &[Json],
) -> Result<Json, Error> {
    if moves.is_empty() || moves.len() > 500 {
        return Err(Error::Value("Supply between 1 and 500 moves".into()));
    }
    // A dict by reference: the last footprint with a reference wins, as in Python
    let mut footprints: Vec<(String, DynamicMessage)> = Vec::new();
    for fp in board_unpacked(client, doc, "footprints").await? {
        let reference = text_at(&fp, "reference_field.text.text.text");
        match footprints.iter_mut().find(|(r, _)| *r == reference) {
            Some(entry) => entry.1 = fp,
            None => footprints.push((reference, fp)),
        }
    }
    let empty = Map::new();
    let (mut requests, mut report, mut flips) = (Vec::new(), Vec::new(), 0);
    for movement in moves {
        let movement = movement.as_object().unwrap_or(&empty);
        let reference = required(movement, "reference")?;
        if matches!(reference, Json::Array(_) | Json::Object(_)) {
            return Err(Error::Value(format!(
                "unhashable type: '{}'",
                type_name(reference)
            )));
        }
        let fp = match reference {
            Json::String(r) => footprints.iter().find(|(name, _)| name == r),
            _ => None,
        };
        let Some((name, fp)) = fp else {
            return Err(Error::Value(format!(
                "No footprint {} on the board",
                py_str(reference)
            )));
        };
        let relative = movement.get("relative").is_some_and(truthy);
        let mut request = json!({"items": [{"value": item_id(fp)}]});
        let side = movement.get("side").unwrap_or(&Json::Null);
        if !matches!(side, Json::Null) && side != "front" && side != "back" {
            return Err(Error::Value("side must be front or back".into()));
        }
        let current = if int_field(fp, "layer") == b_cu() {
            "back"
        } else {
            "front"
        };
        if truthy(side) && side != current {
            request["flip"] = json!("BFD_LEFT_RIGHT");
            flips += 1;
        }
        if movement.contains_key("x_mm") || movement.contains_key("y_mm") {
            let zero = json!(0);
            if relative {
                request["translation"] = vec_of(
                    movement.get("x_mm").unwrap_or(&zero),
                    movement.get("y_mm").unwrap_or(&zero),
                )?;
            } else {
                let [x, y] = mm(&message_field(fp, "position"));
                request["position"] = vec_of(
                    movement.get("x_mm").unwrap_or(&json!(x)),
                    movement.get("y_mm").unwrap_or(&json!(y)),
                )?;
            }
        }
        if let Some(rotation) = movement.get("rotation") {
            if relative {
                request["rotation"] = json!({"value_degrees": real(rotation)?});
            } else {
                request["orientation"] = json!({"value_degrees": normalized_rotation(rotation)?});
            }
        }
        if let Some(locked) = movement.get("locked") {
            request["locked"] = json!(truthy(locked));
        }
        requests.push(header_request(
            &format!("{BC}TransformBoardItems"),
            request,
            doc,
        ));
        report.push(name.clone());
    }
    let header = item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let mut step = Ok(());
    for request in &requests {
        step = match client
            .send(request, &format!("{BC}TransformBoardItemsResponse"))
            .await
        {
            Ok(response) => {
                let errors = item_errors(&messages(&response, "items"));
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(Error::Runtime(errors.join("; ")))
                }
            }
            Err(error) => Err(error),
        };
        if step.is_err() {
            break;
        }
    }
    client
        .finish_transaction(&header, &id, &operation_id, "MCP: move footprints", step)
        .await?;
    let footprints = board_footprints(client, doc, Some(&report), false).await?;
    Ok(json!({"moved": report, "flipped": flips, "footprints": footprints}))
}

/// Helper: `_width_for(doc, net, width)`: the given width, else the net class track width, else
/// 0.25 mm.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn width_for(
    client: &mut Client,
    doc: &DynamicMessage,
    net: Option<&str>,
    width: Option<f64>,
) -> Result<f64, Error> {
    if let Some(width) = width {
        return Ok(width);
    }
    let widths = match net.filter(|n| !n.is_empty()) {
        Some(net) => netclass_widths(client, doc, net).await?,
        None => None,
    };
    Ok(or_default(widths.and_then(|w| w.track_width), 0.25))
}

/// Helper: `board_add_tracks(doc, points, layer, width, net)`: a connected chain of track segments
/// through the points (mm) on one copper layer, as one undo step (repeated points are skipped).
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn board_add_tracks(
    client: &mut Client,
    doc: &DynamicMessage,
    points: &[Vec<f64>],
    layer_name_value: &str,
    width: Option<f64>,
    net: Option<&str>,
) -> Result<Json, Error> {
    if points.len() < 2 || points.len() > 200 {
        return Err(Error::Value("Supply between 2 and 200 points".into()));
    }
    let layer_value = layer(layer_name_value)?;
    if !is_copper(layer_value) {
        return Err(Error::Value(
            "Tracks go on copper layers (F.Cu, B.Cu, In1.Cu, ...)".into(),
        ));
    }
    let width = width_for(client, doc, net, width).await?;
    let mut tracks = Vec::new();
    for pair in points.windows(2) {
        if pair[0] != pair[1] {
            tracks.push(make_track(
                &json!(pair[0]),
                &json!(pair[1]),
                width,
                layer_value,
                net,
            )?);
        }
    }
    client
        .mutate(doc, &tracks, false, Some("MCP: add tracks"))
        .await?;
    Ok(json!({"tracks": item_ids(&tracks), "width_mm": width, "net": net.unwrap_or("")}))
}

/// The pads and options of a plain route (`board_route` with avoid=False).
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) struct PlainRoute<'a> {
    pub from_ref: &'a str,
    pub from_pad: &'a str,
    pub to_ref: &'a str,
    pub to_pad: &'a str,
    pub layer: Option<&'a str>,
    pub width: Option<f64>,
    /// `horizontal_first` (Python's default), `diagonal_first` or `orthogonal`
    pub posture: &'a str,
    pub via_at: Option<[f64; 2]>,
    pub second_layer: Option<&'a str>,
}

/// Helper: `board_route(..., avoid=False)`: a plain 45-degree dog-leg between two pads, optionally
/// through a via at `via_at`, that does not look at anything else on the board; one undo step.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn board_route(
    client: &mut Client,
    doc: &DynamicMessage,
    route: &PlainRoute<'_>,
) -> Result<Json, Error> {
    let a = find_pad(client, doc, route.from_ref, route.from_pad).await?;
    let b = find_pad(client, doc, route.to_ref, route.to_pad).await?;
    let (net_a, net_b) = (text_at(&a, "net.name"), text_at(&b, "net.name"));
    if !net_a.is_empty() && !net_b.is_empty() && net_a != net_b {
        return Err(Error::Value(format!(
            "{}.{} is on {net_a} but {}.{} is on {net_b}",
            route.from_ref, route.from_pad, route.to_ref, route.to_pad
        )));
    }
    let net = if net_a.is_empty() { net_b } else { net_a };
    let start = mm(&message_field(&a, "position"));
    let end = mm(&message_field(&b, "position"));
    let fp_a = board_footprint(client, doc, route.from_ref).await?;
    let default_layer = if int_field(&fp_a, "layer") == b_cu() {
        "B.Cu"
    } else {
        "F.Cu"
    };
    let first = layer(
        route
            .layer
            .filter(|l| !l.is_empty())
            .unwrap_or(default_layer),
    )?;
    let width = width_for(client, doc, Some(&net), route.width).await?;
    let mut items = Vec::new();
    let path;
    if let Some(via_point) = route.via_at {
        let second_name = match route.second_layer.filter(|l| !l.is_empty()) {
            Some(name) => name.to_owned(),
            None if layer_name(i64::from(first))? != "B.Cu" => "B.Cu".into(),
            None => "F.Cu".into(),
        };
        let second = layer(&second_name)?;
        let leg1 = route_points(start, via_point, route.posture);
        let leg2 = route_points(via_point, end, route.posture);
        for pair in leg1.windows(2) {
            items.push(make_track(
                &json!(pair[0]),
                &json!(pair[1]),
                width,
                first,
                Some(&net),
            )?);
        }
        let sizes = netclass_widths(client, doc, &net)
            .await?
            .unwrap_or_default();
        items.push(make_via(
            &json!(via_point),
            Some(&net),
            or_default(sizes.via_diameter, 0.6),
            or_default(sizes.via_drill, 0.3),
            "F.Cu",
            "B.Cu",
        )?);
        for pair in leg2.windows(2) {
            items.push(make_track(
                &json!(pair[0]),
                &json!(pair[1]),
                width,
                second,
                Some(&net),
            )?);
        }
        path = leg1
            .into_iter()
            .chain(leg2.into_iter().skip(1))
            .collect::<Vec<_>>();
    } else {
        path = route_points(start, end, route.posture);
        for pair in path.windows(2) {
            items.push(make_track(
                &json!(pair[0]),
                &json!(pair[1]),
                width,
                first,
                Some(&net),
            )?);
        }
    }
    let description = format!(
        "MCP: route {}.{} to {}.{}",
        route.from_ref, route.from_pad, route.to_ref, route.to_pad
    );
    client
        .mutate(doc, &items, false, Some(&description))
        .await?;
    Ok(
        json!({"net": net, "width_mm": width, "layer": layer_name(i64::from(first))?,
              "path_mm": path, "items": item_ids(&items)}),
    )
}

/// `board_add_via(doc, x, y, net, size, drill)`: a through via (F.Cu to B.Cu) sized by the net
/// class unless given, as one undo step.
pub(crate) async fn board_add_via(
    client: &mut Client,
    doc: &DynamicMessage,
    x: f64,
    y: f64,
    net: Option<&str>,
    size: Option<f64>,
    drill: Option<f64>,
) -> Result<Json, Error> {
    let sizes = match net.filter(|n| !n.is_empty()) {
        Some(net) => netclass_widths(client, doc, net).await?.unwrap_or_default(),
        None => NetclassWidths::default(),
    };
    let size = or_default(size.filter(|s| *s != 0.0).or(sizes.via_diameter), 0.6);
    let drill = or_default(drill.filter(|d| *d != 0.0).or(sizes.via_drill), 0.3);
    if drill >= size {
        return Err(Error::Value(
            "drill must be smaller than the via size".into(),
        ));
    }
    let via = make_via(&json!([x, y]), net, size, drill, "F.Cu", "B.Cu")?;
    client
        .mutate(doc, std::slice::from_ref(&via), false, Some("MCP: add via"))
        .await?;
    Ok(json!({"via": item_id(&via), "size_mm": size, "drill_mm": drill, "net": net.unwrap_or("")}))
}

/// The outline `live_pcb_outline` draws.
pub(crate) struct Outline<'a> {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub corner_radius: f64,
    pub line_width: f64,
    pub replace: bool,
    pub points: Option<&'a [Vec<f64>]>,
}

/// `board_outline(...)`: draws the board outline on Edge.Cuts (rectangle, rounded rectangle or
/// polygon), with `replace` removing the board-level Edge.Cuts shapes first; one undo step.
pub(crate) async fn board_outline(
    client: &mut Client,
    doc: &DynamicMessage,
    outline: &Outline<'_>,
) -> Result<Json, Error> {
    let edge = enum_number(BOARD_LAYER, "BL_Edge_Cuts");
    let line_width = json!(outline.line_width);
    let items = match outline.points.filter(|p| !p.is_empty()) {
        Some(points) => {
            if points.len() < 3 {
                return Err(Error::Value(
                    "A polygon outline needs at least 3 points".into(),
                ));
            }
            let mut closed = points.to_vec();
            closed.push(points[0].clone());
            closed
                .windows(2)
                .map(|pair| make_segment(&json!(pair[0]), &json!(pair[1]), edge, &line_width))
                .collect::<Result<Vec<_>, _>>()?
        }
        None => {
            if outline.width <= 0.0 || outline.height <= 0.0 {
                return Err(Error::Value("width and height must be positive".into()));
            }
            rounded_rectangle(
                outline.x,
                outline.y,
                outline.width,
                outline.height,
                outline.corner_radius,
                edge,
                outline.line_width,
            )?
        }
    };
    let header = item_header(doc);
    let (id, operation_id) = client.begin_transaction(&header).await?;
    let step = outline_steps(client, doc, &items, outline.replace, edge).await;
    client
        .finish_transaction(
            &header,
            &id,
            &operation_id,
            "MCP: board outline",
            outcome(&step),
        )
        .await?;
    let removed = step?;
    let outline_mm = board_box(client, doc, "edges").await?;
    Ok(json!({"replaced": removed, "items": item_ids(&items), "outline_mm": outline_mm}))
}

/// The steps of `board_outline`'s transaction: delete the old outline, create the new one.
/// Returns how many shapes were removed.
async fn outline_steps(
    client: &mut Client,
    doc: &DynamicMessage,
    items: &[DynamicMessage],
    replace: bool,
    edge: i32,
) -> Result<usize, Error> {
    let mut removed = Vec::new();
    if replace {
        // Board-level shapes carry the board as parent; a footprint's carry the footprint
        let footprint_ids: Vec<String> = board_unpacked(client, doc, "footprints")
            .await?
            .iter()
            .map(item_id)
            .collect();
        for shape in board_unpacked(client, doc, "shapes").await? {
            let parent = str_field(&message_field(&shape, "parent"), "value");
            if int_field(&shape, "layer") == i64::from(edge) && !footprint_ids.contains(&parent) {
                removed.push(item_id(&shape));
            }
        }
        if !removed.is_empty() {
            let request = header_request(
                &format!("{C}DeleteItems"),
                json!({"item_ids": kiids(&removed)}),
                doc,
            );
            client
                .send(&request, &format!("{C}DeleteItemsResponse"))
                .await?;
        }
    }
    let mut request = header_request(&format!("{C}CreateItems"), json!({}), doc);
    request.set_field_by_name(
        "items",
        Value::List(
            items
                .iter()
                .map(|i| Value::Message(proto::pack(i)))
                .collect(),
        ),
    );
    let response = client
        .send(&request, &format!("{C}CreateItemsResponse"))
        .await?;
    let errors = item_errors(&messages(&response, "created_items"));
    if !errors.is_empty() {
        return Err(Error::Runtime(errors.join("; ")));
    }
    Ok(removed.len())
}

/// `board_add_zone(...)`: a copper pour for a net following the board outline (inset by `inset`
/// mm) unless points are given, as one undo step, then (with `fill`) every zone refilled.
pub(crate) async fn board_add_zone(
    client: &mut Client,
    doc: &DynamicMessage,
    layers: &[String],
    points: Option<&Json>,
    settings: &ZoneSettings<'_>,
    inset: f64,
    fill: bool,
) -> Result<Json, Error> {
    let points = match points {
        Some(points) => points.clone(),
        None => {
            let Some([x1, y1, x2, y2]) = board_box(client, doc, "edges").await? else {
                return Err(Error::Value(
                    "The board has no outline yet; draw one (live_pcb_outline) or give points"
                        .into(),
                ));
            };
            json!([
                [x1 + inset, y1 + inset],
                [x2 - inset, y1 + inset],
                [x2 - inset, y2 - inset],
                [x1 + inset, y2 - inset]
            ])
        }
    };
    let zone = make_zone(&points, layers, settings, None)?;
    let net = settings.net.unwrap_or("");
    client
        .mutate(
            doc,
            std::slice::from_ref(&zone),
            false,
            Some(&format!("MCP: add {net} zone")),
        )
        .await?;
    if fill {
        board_refill(client, doc, &[]).await?;
    }
    Ok(json!({"zone": item_id(&zone), "net": net, "layers": layers, "filled": fill}))
}

/// Helper: `board_add_keepout(doc, points, layers, name, ...)`: a rule area keeping out what the
/// flags say, as one undo step.  Returns `{"rule_area": id}`.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn board_add_keepout(
    client: &mut Client,
    doc: &DynamicMessage,
    points: &Json,
    layers: &[String],
    name: &str,
    rule: RuleArea,
) -> Result<Json, Error> {
    let settings = ZoneSettings {
        name,
        ..ZoneSettings::default()
    };
    let zone = make_zone(points, layers, &settings, Some(rule))?;
    client
        .mutate(
            doc,
            std::slice::from_ref(&zone),
            false,
            Some("MCP: add keepout"),
        )
        .await?;
    Ok(json!({"rule_area": item_id(&zone)}))
}

/// One shape of `board_add_graphics`.
fn graphic_item(spec: &Map<String, Json>) -> Result<DynamicMessage, Error> {
    let kind = spec.get("kind").unwrap_or(&Json::Null);
    let layer_value = layer_of(spec.get("layer").unwrap_or(&json!("F.SilkS")))?;
    let default_width = if layer_name(i64::from(layer_value))?.contains("Silk") {
        0.12
    } else {
        0.1
    };
    let default_width = json!(default_width);
    let width = spec.get("width").unwrap_or(&default_width);
    let filled = |default: bool| spec.get("filled").map_or(default, truthy);
    match kind.as_str().unwrap_or_default() {
        "line" => make_segment(
            required(spec, "start")?,
            required(spec, "end")?,
            layer_value,
            width,
        ),
        "arc" => make_arc(
            required(spec, "start")?,
            required(spec, "mid")?,
            required(spec, "end")?,
            layer_value,
            width,
        ),
        "rect" => {
            let corners = unpack4(required(spec, "box")?)?;
            make_rectangle(
                &corners[0],
                &corners[1],
                &corners[2],
                &corners[3],
                layer_value,
                width,
                filled(false),
            )
        }
        "circle" => make_circle(
            required(spec, "center")?,
            required(spec, "radius")?,
            layer_value,
            width,
            filled(false),
        ),
        "polygon" => make_polygon(required(spec, "points")?, layer_value, width, filled(true)),
        "text" => make_text(
            required(spec, "text")?,
            required(spec, "position")?,
            layer_value,
            spec.get("size").unwrap_or(&json!(1.0)),
            spec.get("rotation").unwrap_or(&json!(0.0)),
            spec.get("thickness").filter(|t| !t.is_null()),
            None,
        ),
        _ => Err(Error::Value(
            "kind must be line, arc, rect, circle, polygon or text".into(),
        )),
    }
}

/// `x1, y1, x2, y2 = value`, with Python's unpacking errors.
fn unpack4(value: &Json) -> Result<Vec<Json>, Error> {
    let Some(parts) = py_iter(value) else {
        return Err(Error::Value(format!(
            "cannot unpack non-iterable {} object",
            type_name(value)
        )));
    };
    match parts.len() {
        4 => Ok(parts),
        n if n < 4 => Err(Error::Value(format!(
            "not enough values to unpack (expected 4, got {n})"
        ))),
        _ => Err(Error::Value(
            "too many values to unpack (expected 4)".into(),
        )),
    }
}

/// `board_add_graphics(doc, shapes)`: lines, arcs, rectangles, circles, polygons and text (mm) on
/// any layer, as one undo step.
pub(crate) async fn board_add_graphics(
    client: &mut Client,
    doc: &DynamicMessage,
    shapes: &[Json],
) -> Result<Json, Error> {
    let empty = Map::new();
    let items = shapes
        .iter()
        .map(|spec| graphic_item(spec.as_object().unwrap_or(&empty)))
        .collect::<Result<Vec<_>, _>>()?;
    if items.is_empty() || items.len() > 200 {
        return Err(Error::Value("Supply between 1 and 200 shapes".into()));
    }
    client
        .mutate(doc, &items, false, Some("MCP: add graphics"))
        .await?;
    Ok(json!({"items": item_ids(&items)}))
}

/// `board_delete_where(doc, kinds, net, layer)`: deletes tracks, arcs, vias or zones by net and/or
/// layer, one undo step per 200 items.
pub(crate) async fn board_delete_where(
    client: &mut Client,
    doc: &DynamicMessage,
    kinds: &[String],
    net: Option<&str>,
    layer_filter: Option<&str>,
) -> Result<Json, Error> {
    let mut ids = Vec::new();
    for kind in kinds {
        for item in board_unpacked(client, doc, kind).await? {
            let descriptor = item.descriptor();
            let item_net = if kind == "zones" {
                text_at(&item, "copper_settings.net.name")
            } else if descriptor.get_field_by_name("net").is_some() {
                text_at(&item, "net.name")
            } else {
                String::new()
            };
            if net.is_some_and(|n| n != item_net) {
                continue;
            }
            if let Some(filter) = layer_filter {
                // Python looks the layer up for every item, so an unknown name fails only then
                let wanted = i64::from(layer(filter)?);
                if kind == "zones" {
                    if !enum_list(&item, "layers").contains(&wanted) {
                        continue;
                    }
                } else if kind != "vias" {
                    if descriptor.get_field_by_name("layer").is_none() {
                        return Err(Error::Value("layer".into()));
                    }
                    if int_field(&item, "layer") != wanted {
                        continue;
                    }
                }
            }
            ids.push(item_id(&item));
        }
    }
    if ids.is_empty() {
        return Ok(json!({"deleted": 0}));
    }
    for chunk in ids.chunks(200) {
        client.delete_items(doc, chunk).await?;
    }
    Ok(json!({"deleted": ids.len()}))
}

/// `board_refill(doc, zones)`: refills the given zones, or every zone.
pub(crate) async fn board_refill(
    client: &mut Client,
    doc: &DynamicMessage,
    zones: &[String],
) -> Result<Json, Error> {
    let request = board_request("RefillZones", json!({"zones": kiids(zones)}), doc);
    client.send(&request, EMPTY).await?;
    Ok(json!({"refilled": true}))
}

/// `board_drc(doc, parity, refill, limit)`: runs KiCad's DRC task, waits for it (300 s) and
/// reports the violations by type, with positions in mm.  `limit` None reports all of them.
pub(crate) async fn board_drc(
    client: &mut Client,
    doc: &DynamicMessage,
    parity: bool,
    refill: bool,
    limit: Option<usize>,
) -> Result<Json, Error> {
    let start = board_request_in(
        "kiapi.board.tasks.StartDrcTask",
        json!({"test_schematic_parity": parity, "require_zones_filled": refill}),
        doc,
    );
    let started = client
        .send(&start, "kiapi.common.tasks.StartTaskResponse")
        .await?;
    let token = message_field(&started, "token");
    let status_request = with_field(
        build("kiapi.common.tasks.GetTaskStatus", json!({})),
        "token",
        token.clone(),
    );
    let deadline = Instant::now() + DRC_TIMEOUT;
    let timeout =
        || Error::Runtime("DRC did not finish in time; it may still be running in KiCad".into());
    loop {
        // Read-only, so a reply held up while KiCad (re)loads footprint libraries for the
        // schematic parity check is simply waited for again
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
    let result_request = with_field(
        build("kiapi.board.tasks.GetDrcTaskResult", json!({})),
        "token",
        token,
    );
    let result = client
        .send(&result_request, "kiapi.board.tasks.DrcTaskResult")
        .await?;
    let mut by_type = Map::new();
    let mut violations = Vec::new();
    for marker in messages(&result, "violations") {
        let kind = py_enum_name("kiapi.board.DrcErrorType", int_field(&marker, "error_type"))?;
        let kind = kind.strip_prefix("DRCET_").unwrap_or(&kind);
        let kind = kind.strip_prefix("DRCE_").unwrap_or(kind).to_lowercase();
        let count = by_type.get(&kind).and_then(Json::as_i64).unwrap_or(0);
        by_type.insert(kind.clone(), json!(count + 1));
        let severity = py_enum_name(
            "kiapi.common.types.RuleSeverity",
            int_field(&marker, "severity"),
        )?;
        let layer_value = int_field(&marker, "layer");
        let mut entry = Map::new();
        entry.insert("type".into(), json!(kind));
        entry.insert(
            "severity".into(),
            json!(severity.strip_prefix("RS_").unwrap_or(&severity)),
        );
        entry.insert(
            "description".into(),
            json!(str_field(&marker, "description")),
        );
        entry.insert(
            "position_mm".into(),
            json!(mm(&message_field(&marker, "position"))),
        );
        entry.insert(
            "layer".into(),
            if layer_value != 0 {
                json!(layer_name(layer_value)?)
            } else {
                Json::Null
            },
        );
        entry.insert(
            "items".into(),
            Json::Array(
                messages(&marker, "items")
                    .iter()
                    .map(|i| json!(str_field(i, "value")))
                    .collect(),
            ),
        );
        let rule = str_field(&marker, "violating_rule");
        if !rule.is_empty() {
            entry.insert("rule".into(), json!(rule));
        }
        violations.push(Json::Object(entry));
    }
    let truncated = limit.is_some_and(|l| violations.len() > l);
    let count = violations.len();
    if let Some(limit) = limit {
        violations.truncate(limit);
    }
    Ok(json!({
        "violation_count": count, "by_type": by_type,
        "ignored": int_field(&result, "ignored_count"),
        "excluded": int_field(&result, "excluded_count"),
        "violations_truncated": truncated, "violations": violations,
    }))
}

/// A request in any package with its `board` document field set.
fn board_request_in(full_name: &str, value: Json, doc: &DynamicMessage) -> DynamicMessage {
    with_field(build(full_name, value), "board", doc.clone())
}

// ---- Temporary files ----------------------------------------------------------------------------

/// Python's `tempfile.gettempdir()`: the first of TMPDIR, TEMP and TMP naming a directory (made
/// absolute), else the system's temporary directory.  Renders and the netlist go there.
fn python_temp_dir() -> PathBuf {
    for name in ["TMPDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name).filter(|v| !v.is_empty()) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return std::path::absolute(&path).unwrap_or(path);
            }
        }
    }
    std::env::temp_dir()
}

/// `tempfile.TemporaryDirectory(prefix=...)`: removed when dropped.
fn temporary_directory(prefix: &str) -> Result<tempfile::TempDir, Error> {
    tempfile::Builder::new()
        .prefix(prefix)
        .rand_bytes(8)
        .tempdir_in(python_temp_dir())
        .map_err(|error| Error::Runtime(error.to_string()))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `JOB_OK`: a job that wrote its outputs (JS_SUCCESS, or JS_WARNING with KiCad's warnings).
fn job_ok(response: &DynamicMessage) -> bool {
    matches!(int_field(response, "status"), 1 | 2)
}

/// Sets a job request's `job_settings.document` and `output_path`.
fn with_job(request: DynamicMessage, doc: &DynamicMessage, output: &Path) -> DynamicMessage {
    let settings = with_field(
        build(
            &format!("{T}RunJobSettings"),
            json!({"output_path": path_text(output)}),
        ),
        "document",
        doc.clone(),
    );
    with_field(request, "job_settings", settings)
}

/// `board_update_from_schematic(doc, schematic_doc, ...)`: Update PCB from Schematic with the
/// schematic's live netlist (exported to a temporary file).
pub(crate) async fn board_update_from_schematic(
    client: &mut Client,
    doc: &DynamicMessage,
    schematic: &DynamicMessage,
    dry_run: bool,
    delete_extra: bool,
    update_footprints: bool,
    match_mode: &str,
) -> Result<Json, Error> {
    let directory = temporary_directory("kicad-live-netlist-")?;
    let path = directory.path().join("update.net");
    let request = with_job(
        build(
            "kiapi.schematic.jobs.RunSchematicJobExportNetlist",
            json!({"format": "SNF_KICAD_SEXPR"}),
        ),
        schematic,
        &path,
    );
    let response = client.send(&request, RUN_JOB_RESPONSE).await?;
    if !job_ok(&response) || !path.is_file() {
        return Err(Error::Runtime(format!(
            "Could not export the schematic netlist: {}",
            str_field(&response, "message")
        )));
    }
    let import = board_request(
        "ImportNetlist",
        json!({
            "netlist_path": path_text(&path), "dry_run": dry_run,
            "match_mode": if match_mode == "reference" { "NMM_REFERENCE" } else { "NMM_UUID" },
            "delete_extra_footprints": delete_extra, "update_footprints": update_footprints,
        }),
        doc,
    );
    let imported = client
        .send(&import, &format!("{BC}ImportNetlistResponse"))
        .await?;
    drop(directory);
    let report = str_field(&imported, "report");
    // report[-6000:], counted in code points as Python slices
    let skip = report.chars().count().saturating_sub(6000);
    let tail: String = report.chars().skip(skip).collect();
    Ok(json!({
        "dry_run": dry_run, "errors": int_field(&imported, "error_count"),
        "warnings": int_field(&imported, "warning_count"),
        "new_footprints": int_field(&imported, "new_footprint_count"), "report": tail,
    }))
}

// ---- Settings: origins, layers, rules, stackup ---------------------------------------------------

/// `board_origins(doc, grid, drill)`: sets the given origins (mm), then reads both.
pub(crate) async fn board_origins(
    client: &mut Client,
    doc: &DynamicMessage,
    grid: Option<&[f64]>,
    drill: Option<&[f64]>,
) -> Result<Json, Error> {
    for (kind, value) in [("BOT_GRID", grid), ("BOT_DRILL", drill)] {
        if let Some(value) = value {
            let request = board_request("SetBoardOrigin", json!({"type": kind}), doc);
            let request = with_field(
                request,
                "origin",
                build(&format!("{T}Vector2"), vec_xy(value)?),
            );
            client.send(&request, EMPTY).await?;
        }
    }
    let mut result = Map::new();
    for (name, kind) in [
        ("grid_origin_mm", "BOT_GRID"),
        ("drill_origin_mm", "BOT_DRILL"),
    ] {
        let origin = client
            .send(
                &board_request("GetBoardOrigin", json!({"type": kind}), doc),
                &format!("{T}Vector2"),
            )
            .await?;
        result.insert(name.into(), json!(mm(&origin)));
    }
    Ok(Json::Object(result))
}

/// Layer names of a repeated BoardLayer field.
fn layer_names(message: &DynamicMessage, name: &str) -> Result<Vec<String>, Error> {
    enum_list(message, name)
        .into_iter()
        .map(layer_name)
        .collect()
}

/// `board_layers(doc, copper_layers, None, visible, active)`: sets the copper layer count
/// (through the stackup, which refuses to drop layers that still carry items), the visible layers
/// and the active layer, then reads them.  (Python's `enable` argument is not ported: no tool
/// passes it, and its order comes from a Python set.)
pub(crate) async fn board_layers(
    client: &mut Client,
    doc: &DynamicMessage,
    copper_layers: Option<i64>,
    visible: Option<&[String]>,
    active: Option<&str>,
) -> Result<Json, Error> {
    if let Some(count) = copper_layers {
        let changes = StackupChanges {
            copper_layers: Some(count),
            ..StackupChanges::default()
        };
        board_set_stackup(client, doc, &changes).await?;
    }
    if let Some(visible) = visible {
        let layers = visible
            .iter()
            .map(|n| layer(n))
            .collect::<Result<Vec<_>, _>>()?;
        client
            .send(
                &board_request("SetVisibleLayers", json!({"layers": layers}), doc),
                EMPTY,
            )
            .await?;
    }
    if let Some(active) = active {
        let layer_value = layer(active)?;
        client
            .send(
                &board_request("SetActiveLayer", json!({"layer": layer_value}), doc),
                EMPTY,
            )
            .await?;
    }
    let enabled = client
        .send(
            &board_request("GetBoardEnabledLayers", json!({}), doc),
            &format!("{BC}BoardEnabledLayersResponse"),
        )
        .await?;
    let shown = client
        .send(
            &board_request("GetVisibleLayers", json!({}), doc),
            &format!("{BC}BoardLayers"),
        )
        .await?;
    let current = client
        .send(
            &board_request("GetActiveLayer", json!({}), doc),
            &format!("{BC}BoardLayerResponse"),
        )
        .await?;
    Ok(json!({
        "copper_layers": int_field(&enabled, "copper_layer_count"),
        "enabled": layer_names(&enabled, "layers")?,
        "visible": layer_names(&shown, "layers")?,
        "active": layer_name(int_field(&current, "layer"))?,
    }))
}

/// Helper: `board_design_rules(doc, changes)`: the board's design rules as protobuf JSON; with
/// `changes` (protobuf JSON merged over the current rules) they are set first.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn board_design_rules(
    client: &mut Client,
    doc: &DynamicMessage,
    changes: Option<&Json>,
) -> Result<Json, Error> {
    let response_type = format!("{BC}BoardDesignRulesResponse");
    let mut current = client
        .send(
            &board_request("GetBoardDesignRules", json!({}), doc),
            &response_type,
        )
        .await?;
    if let Some(changes) = changes.filter(|c| truthy(c)) {
        let mut rules = message_field(&current, "rules");
        pbjson::merge_json(changes, &mut rules)?;
        let request = with_field(
            board_request("SetBoardDesignRules", json!({}), doc),
            "rules",
            rules,
        );
        current = client.send(&request, &response_type).await?;
    }
    reply_json(&current)
}

/// Helper: `board_stackup(doc)`: the stackup as protobuf JSON.
#[allow(dead_code)] // the board design tools call it (board_design.py)
pub(crate) async fn board_stackup(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Json, Error> {
    let response = client
        .send(
            &board_request("GetBoardStackup", json!({}), doc),
            &format!("{BC}BoardStackupResponse"),
        )
        .await?;
    reply_json(&message_field(&response, "stackup"))
}

/// `"#%02X%02X%02X"`-style hex of one colour channel (`%02X` of a negative number keeps its sign).
fn hex_channel(value: f64) -> String {
    let n = (value * 255.0).round_ties_even() as i64;
    if n < 0 {
        format!("-{:X}", -n)
    } else {
        format!("{n:02X}")
    }
}

/// Helper: `board_stackup_layers(doc)`: the stackup top to bottom in mm, as
/// `{"layers": [{type, thickness, layer, material, color, dielectric, sublayers, epsilon_r}],
/// "board_thickness_mm", "finish", "impedance_controlled"}`.
pub(crate) async fn board_stackup_layers(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Json, Error> {
    let response = client
        .send(
            &board_request("GetBoardStackup", json!({}), doc),
            &format!("{BC}BoardStackupResponse"),
        )
        .await?;
    let stackup = message_field(&response, "stackup");
    let mut layers = Vec::new();
    let mut summed = Vec::new();
    for entry in messages(&stackup, "layers") {
        let kind = py_enum_name(
            "kiapi.board.BoardStackupLayerType",
            int_field(&entry, "type"),
        )?;
        let kind = kind.strip_prefix("BSLT_").unwrap_or(&kind).to_lowercase();
        let thickness = py_round(
            int_field(&message_field(&entry, "thickness"), "value_nm") as f64 / 1e6,
            4,
        );
        if kind == "copper" || kind == "dielectric" {
            summed.push(thickness);
        }
        let mut item = Map::new();
        item.insert("type".into(), json!(kind));
        item.insert("thickness".into(), json!(thickness));
        if kind != "dielectric" {
            item.insert(
                "layer".into(),
                json!(layer_name(int_field(&entry, "layer"))?),
            );
        }
        let material = str_field(&entry, "material_name");
        if !material.is_empty() {
            item.insert("material".into(), json!(material));
        }
        if has_field(&entry, "color") {
            let c = message_field(&entry, "color");
            item.insert(
                "color".into(),
                json!(format!(
                    "#{}{}{}",
                    hex_channel(float_field(&c, "r")),
                    hex_channel(float_field(&c, "g")),
                    hex_channel(float_field(&c, "b"))
                )),
            );
        }
        if has_field(&entry, "dielectric") {
            let dielectric = message_field(&entry, "dielectric");
            let name = py_enum_name(
                "kiapi.board.BoardStackupDielectricType",
                int_field(&dielectric, "type"),
            )?;
            item.insert(
                "dielectric".into(),
                json!(name.strip_prefix("BSDT_").unwrap_or(&name).to_lowercase()),
            );
            let sublayers: Vec<Json> = messages(&dielectric, "layer")
                .iter()
                .map(|p| {
                    json!({
                        "thickness": py_round(
                            int_field(&message_field(p, "thickness"), "value_nm") as f64 / 1e6, 4),
                        "epsilon_r": float_field(p, "epsilon_r"),
                        "loss_tangent": float_field(p, "loss_tangent"),
                        "material": str_field(p, "material_name"),
                    })
                })
                .collect();
            item.insert("sublayers".into(), Json::Array(sublayers));
        }
        if has_field(&entry, "soldermask") {
            let mask = message_field(&entry, "soldermask");
            item.insert("epsilon_r".into(), json!(float_field(&mask, "epsilon_r")));
            item.insert("material".into(), json!(str_field(&mask, "material_name")));
        }
        layers.push(Json::Object(item));
    }
    // sum() of no layers is the int 0 in Python
    let thickness = if summed.is_empty() {
        json!(0)
    } else {
        json!(py_round(py_sum(summed), 4))
    };
    Ok(json!({
        "layers": layers, "board_thickness_mm": thickness,
        "finish": str_field(&message_field(&stackup, "finish"), "type_name"),
        "impedance_controlled": int_field(&message_field(&stackup, "impedance"), "is_controlled") != 0,
    }))
}

/// `copper_layer_names(count)`: F.Cu, In1.Cu ... In<count-2>.Cu, B.Cu.
pub(crate) fn copper_layer_names(count: i64) -> Vec<String> {
    let mut names = vec!["F.Cu".to_owned()];
    // KiCad has In1.Cu to In30.Cu: every caller looks the names up in order and fails at In31.Cu,
    // so names past it are never used.  Stopping there keeps an absurd count (a client's
    // copper_layers) from allocating without bound; Python builds them all first.
    names.extend((1..count - 1).take(31).map(|i| format!("In{i}.Cu")));
    names.push("B.Cu".into());
    names
}

/// The arguments of `board_set_stackup`; None keeps the current setting.
#[derive(Clone, Debug, Default)]
pub(crate) struct StackupChanges<'a> {
    pub copper_layers: Option<i64>,
    /// mm for every copper layer, or `{layer: mm}`
    pub copper_thickness: Option<&'a Json>,
    /// `[{type: core|prepreg, thickness, epsilon_r, loss_tangent, material}]` top to bottom
    pub dielectrics: Option<&'a [Json]>,
    pub board_thickness: Option<f64>,
    pub soldermask_color: Option<&'a str>,
    pub silkscreen_color: Option<&'a str>,
    pub finish: Option<&'a str>,
    pub impedance_controlled: Option<bool>,
    pub soldermask_thickness: Option<f64>,
}

/// `int(text, 16)`, with Python's error.
fn hex_byte(text: &str) -> Result<f64, Error> {
    let trimmed = text.trim();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    match i64::from_str_radix(digits, 16) {
        Ok(n) if !digits.starts_with(['+', '-']) => Ok((if negative { -n } else { n }) as f64),
        _ => Err(Error::Value(format!(
            "invalid literal for int() with base 16: {}",
            py_repr(&json!(text))
        ))),
    }
}

/// `colour(entry, value)` of `board_set_stackup`: `#RRGGBB` as channels from 0 to 1.
fn stackup_colour(value: Option<&str>) -> Result<Option<Json>, Error> {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let value: Vec<char> = value.trim_start_matches('#').chars().collect();
    let slice = |i: usize| -> String { value.iter().skip(i).take(2).collect() };
    let r = hex_byte(&slice(0))? / 255.0;
    let g = hex_byte(&slice(2))? / 255.0;
    let b = hex_byte(&slice(4))? / 255.0;
    Ok(Some(json!({"r": r, "g": g, "b": b, "a": 1.0})))
}

/// Helper: `board_set_stackup(doc, ...)`: changes the stackup (copper layer count, thicknesses,
/// dielectrics, colours, finish, impedance control); `board_thickness` then scales the
/// dielectrics to reach it.  Returns `board_stackup_layers`.  Not an undo step.
pub(crate) async fn board_set_stackup(
    client: &mut Client,
    doc: &DynamicMessage,
    changes: &StackupChanges<'_>,
) -> Result<Json, Error> {
    let current = board_stackup_layers(client, doc).await?;
    let count = match changes.copper_layers.filter(|c| *c != 0) {
        Some(count) => count,
        None => current["layers"]
            .as_array()
            .map_or(0, |l| l.iter().filter(|e| e["type"] == "copper").count())
            as i64,
    };
    if count < 2 || count % 2 != 0 {
        return Err(Error::Value(
            "copper_layers must be an even number, at least 2".into(),
        ));
    }
    if let Some(dielectrics) = changes.dielectrics {
        if dielectrics.len() as i64 != count - 1 {
            return Err(Error::Value(format!(
                "{count} copper layers need {} dielectrics, top to bottom",
                count - 1
            )));
        }
    }
    let names = copper_layer_names(count);
    let mut layers = Vec::new();
    let silk_colour = stackup_colour(changes.silkscreen_color)?;
    let mask_colour = stackup_colour(changes.soldermask_color)?;
    for side in ["F", "B"] {
        let mut silk =
            json!({"type": "BSLT_SILKSCREEN", "layer": layer(&format!("{side}.SilkS"))?});
        if let Some(colour) = &silk_colour {
            silk["color"] = colour.clone();
        }
        layers.push(silk);
        let mut mask = json!({"type": "BSLT_SOLDERMASK", "layer": layer(&format!("{side}.Mask"))?});
        if let Some(colour) = &mask_colour {
            mask["color"] = colour.clone();
        }
        if let Some(thickness) = changes.soldermask_thickness.filter(|t| *t != 0.0) {
            mask["soldermask"] = json!({"thickness": {"value_nm": nm(thickness)?}});
        }
        layers.push(mask);
    }
    for (index, name) in names.iter().enumerate() {
        let mut entry = json!({"type": "BSLT_COPPER", "layer": layer(name)?});
        let thickness = match changes.copper_thickness {
            Some(Json::Object(map)) => map.get(name).cloned().unwrap_or(Json::Null),
            Some(value) => value.clone(),
            None => Json::Null,
        };
        if truthy(&thickness) {
            entry["thickness"] = json!({"value_nm": nm_of(&thickness)?});
        }
        layers.push(entry);
        if (index as i64) < count - 1 {
            let mut dielectric = json!({"type": "BSLT_DIELECTRIC"});
            let spec = changes
                .dielectrics
                .and_then(|d| d.get(index))
                .filter(|s| truthy(s));
            if let Some(Json::Object(spec)) = spec {
                dielectric["dielectric"] = dielectric_layer(spec)?;
            }
            layers.push(dielectric);
        }
    }
    let mut stackup = json!({"layers": layers});
    if let Some(finish) = changes.finish.filter(|f| !f.is_empty()) {
        stackup["finish"] = json!({"type_name": finish});
    }
    if let Some(controlled) = changes.impedance_controlled {
        stackup["impedance"] = json!({"is_controlled": controlled});
    }
    update_stackup(client, doc, stackup).await?;
    let mut result = board_stackup_layers(client, doc).await?;
    if let Some(board_thickness) = changes.board_thickness.filter(|t| *t != 0.0) {
        let entries = result["layers"].as_array().cloned().unwrap_or_default();
        let thickness_of = |kind: &str| {
            py_sum(
                entries
                    .iter()
                    .filter(|e| e["type"] == kind)
                    .map(|e| e["thickness"].as_f64().unwrap_or(0.0)),
            )
        };
        let fixed = thickness_of("copper");
        let dielectric_total = thickness_of("dielectric");
        let target = board_thickness - fixed;
        if target <= 0.0 || dielectric_total <= 0.0 {
            return Err(Error::Value(
                "board_thickness is thinner than the copper alone".into(),
            ));
        }
        let scale = target / dielectric_total;
        let mut scaled = Vec::new();
        for entry in entries.iter().filter(|e| e["type"] == "dielectric") {
            let kind = entry
                .get("dielectric")
                .and_then(Json::as_str)
                .filter(|k| !k.is_empty())
                .unwrap_or("core")
                .to_uppercase();
            let kind = format!("BSDT_{kind}");
            if proto::pool()
                .get_enum_by_name("kiapi.board.BoardStackupDielectricType")
                .and_then(|e| e.get_value_by_name(&kind))
                .is_none()
            {
                return Err(Error::Value(format!(
                    "Enum BoardStackupDielectricType has no value defined for name {}",
                    py_repr(&json!(kind))
                )));
            }
            let fallback = vec![json!({"thickness": entry["thickness"]})];
            let sublayers = entry
                .get("sublayers")
                .and_then(Json::as_array)
                .filter(|s| !s.is_empty())
                .unwrap_or(&fallback);
            let mut props = Vec::new();
            for sub in sublayers {
                let value = sub["thickness"].as_f64().unwrap_or(0.0) * scale;
                props.push(json!({"thickness": {"value_nm": nm(value)?}}));
            }
            scaled.push(json!({"type": "BSLT_DIELECTRIC",
                               "dielectric": {"type": kind, "layer": props}}));
        }
        for name in &names {
            scaled.push(json!({"type": "BSLT_COPPER", "layer": layer(name)?}));
        }
        update_stackup(client, doc, json!({"layers": scaled})).await?;
        result = board_stackup_layers(client, doc).await?;
    }
    Ok(result)
}

/// One dielectric of `board_set_stackup` from its caller spec.
fn dielectric_layer(spec: &Map<String, Json>) -> Result<Json, Error> {
    let kind = match spec.get("type").unwrap_or(&json!("core")) {
        Json::String(kind) if kind == "prepreg" => "BSDT_PREPREG",
        value @ (Json::Array(_) | Json::Object(_)) => {
            return Err(Error::Value(format!(
                "unhashable type: '{}'",
                type_name(value)
            )))
        }
        _ => "BSDT_CORE",
    };
    let mut props = Map::new();
    if let Some(thickness) = spec.get("thickness").filter(|t| truthy(t)) {
        props.insert("thickness".into(), json!({"value_nm": nm_of(thickness)?}));
    }
    // `spec.get(key, 0) or 0`: a falsy value is 0
    let number = |key: &str| -> Result<f64, Error> {
        match spec.get(key).filter(|v| truthy(v)) {
            Some(value) => real(value),
            None => Ok(0.0),
        }
    };
    props.insert("epsilon_r".into(), json!(number("epsilon_r")?));
    props.insert("loss_tangent".into(), json!(number("loss_tangent")?));
    let material = match spec.get("material").filter(|v| truthy(v)) {
        Some(Json::String(text)) => text.clone(),
        Some(_) => {
            return Err(Error::Value(
                "bad argument type for built-in operation".into(),
            ))
        }
        None => String::new(),
    };
    props.insert("material_name".into(), json!(material));
    Ok(json!({"type": kind, "layer": [props]}))
}

async fn update_stackup(
    client: &mut Client,
    doc: &DynamicMessage,
    stackup: Json,
) -> Result<(), Error> {
    let request = with_field(
        board_request("UpdateBoardStackup", json!({}), doc),
        "stackup",
        build("kiapi.board.BoardStackup", stackup),
    );
    client
        .send(&request, &format!("{BC}BoardStackupResponse"))
        .await?;
    Ok(())
}

// ---- Seeing the board ---------------------------------------------------------------------------

/// The layers `board_render` shows by default.
const DEFAULT_VIEW_LAYERS: &[&str] = &["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts", "F.Fab"];

/// `board_render(doc, layers, dpi, frame)`: a PNG of the live board on its page (with the drawing
/// sheet when `frame`).
pub(crate) async fn board_render(
    client: &mut Client,
    doc: &DynamicMessage,
    layers: Option<&[String]>,
    dpi: i64,
    frame: bool,
) -> Result<Vec<u8>, Error> {
    if !(50..=600).contains(&dpi) {
        return Err(Error::Value("dpi must be between 50 and 600".into()));
    }
    let names: Vec<String> = match layers.filter(|l| !l.is_empty()) {
        Some(layers) => layers.to_vec(),
        None => DEFAULT_VIEW_LAYERS
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    };
    let directory = temporary_directory("kicad-live-board-")?;
    let target = directory.path().join("board.png");
    let layers = names
        .iter()
        .map(|n| layer(n))
        .collect::<Result<Vec<_>, _>>()?;
    let request = with_job(
        build(
            &format!("{BJ}RunBoardJobExportPng"),
            json!({
                "dpi": dpi, "plot_background_color": true, "page_mode": "BJPM_ALL_LAYERS_ONE_PAGE",
                "plot_settings": {"layers": layers, "plot_drawing_sheet": frame,
                                  "plot_reference_designators": true, "plot_footprint_values": true,
                                  "scale": 1.0},
            }),
        ),
        doc,
        &target,
    );
    let response = client.send(&request, RUN_JOB_RESPONSE).await?;
    let mut outputs: Vec<PathBuf> = list_field(&response, "output_path")
        .into_iter()
        .map(|v| match v {
            Value::String(text) => PathBuf::from(text),
            _ => PathBuf::new(),
        })
        .collect();
    if outputs.is_empty() {
        outputs.push(target);
    }
    let produced = outputs.into_iter().find(|p| p.is_file());
    match produced {
        Some(path) if job_ok(&response) => {
            std::fs::read(&path).map_err(|error| Error::Runtime(error.to_string()))
        }
        _ => Err(Error::Runtime(format!(
            "Board render failed: {}",
            py_repr(&reply_json(&response)?)
        ))),
    }
}

/// `board_render_3d(doc, side, width, height, zoom, quality, rotation)`: KiCad's 3D render of the
/// live board as a PNG.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn board_render_3d(
    client: &mut Client,
    doc: &DynamicMessage,
    side: &str,
    width: i64,
    height: i64,
    zoom: f64,
    quality: &str,
    rotation: Option<[f64; 3]>,
) -> Result<Vec<u8>, Error> {
    let sides = [
        ("top", "RS_TOP"),
        ("bottom", "RS_BOTTOM"),
        ("front", "RS_FRONT"),
        ("back", "RS_BACK"),
        ("left", "RS_LEFT"),
        ("right", "RS_RIGHT"),
    ];
    let qualities = [("basic", "RQ_BASIC"), ("high", "RQ_HIGH")];
    let side_value = sides.iter().find(|(name, _)| *name == side);
    let quality_value = qualities.iter().find(|(name, _)| *name == quality);
    let (Some((_, side_value)), Some((_, quality_value))) = (side_value, quality_value) else {
        let names: Vec<&str> = sides.iter().map(|(name, _)| *name).collect();
        return Err(Error::Value(format!(
            "side must be one of {}; quality basic or high",
            py_list(&names)
        )));
    };
    let directory = temporary_directory("kicad-live-3d-")?;
    let target = directory.path().join("board3d.png");
    let width = int_in_range(width, i64::from(i32::MIN), i64::from(i32::MAX))?;
    let height = int_in_range(height, i64::from(i32::MIN), i64::from(i32::MAX))?;
    let mut request = json!({
        "format": "RF_PNG", "quality": quality_value, "side": side_value, "width": width,
        "height": height, "zoom": zoom, "background_style": "RBS_OPAQUE", "floor": false,
        "use_board_stackup_colors": false, "anti_alias": true,
    });
    if let Some([x, y, z]) = rotation {
        request["rotation"] = json!({"x_nm": x, "y_nm": y, "z_nm": z});
    }
    let request = with_job(
        build(&format!("{BJ}RunBoardJobExportRender"), request),
        doc,
        &target,
    );
    let response = client.send(&request, RUN_JOB_RESPONSE).await?;
    if !job_ok(&response) || !target.is_file() {
        return Err(Error::Runtime(format!(
            "3D render failed: {}",
            py_repr(&reply_json(&response)?)
        )));
    }
    std::fs::read(&target).map_err(|error| Error::Runtime(error.to_string()))
}

/// The ids of the given items and of the footprints with the given references (one GetItems per
/// reference, as in Python).
async fn ids_and_references(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: Option<&[String]>,
    references: Option<&[String]>,
) -> Result<Vec<String>, Error> {
    let mut ids = ids.map(<[String]>::to_vec).unwrap_or_default();
    for reference in references.unwrap_or_default() {
        ids.push(item_id(&board_footprint(client, doc, reference).await?));
    }
    Ok(ids)
}

/// `board_select(doc, ids, references, add)`: selects items in the PCB editor (replacing the
/// selection unless `add`).
pub(crate) async fn board_select(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: Option<&[String]>,
    references: Option<&[String]>,
    add: bool,
) -> Result<Json, Error> {
    let ids = ids_and_references(client, doc, ids, references).await?;
    if !add {
        client
            .send(
                &header_request(&format!("{C}ClearSelection"), json!({}), doc),
                EMPTY,
            )
            .await?;
    }
    if ids.is_empty() {
        return Ok(json!({"selected": 0}));
    }
    let request = header_request(
        &format!("{C}AddToSelection"),
        json!({"items": kiids(&ids)}),
        doc,
    );
    let response = client
        .send(&request, &format!("{C}SelectionResponse"))
        .await?;
    Ok(json!({"selected": list_field(&response, "items").len()}))
}

/// `board_focus(doc, ids, references)`: zooms the PCB editor to the items.
pub(crate) async fn board_focus(
    client: &mut Client,
    doc: &DynamicMessage,
    ids: Option<&[String]>,
    references: Option<&[String]>,
) -> Result<Json, Error> {
    let ids = ids_and_references(client, doc, ids, references).await?;
    let request = with_field(
        build(&format!("{C}FocusOnItems"), json!({"items": kiids(&ids)})),
        "document",
        doc.clone(),
    );
    client.send(&request, EMPTY).await?;
    Ok(json!({"focused": ids.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_follow_python() {
        assert_eq!(nm(1.0), Ok(1_000_000));
        assert_eq!(nm(-10000.0), Ok(-10_000_000_000));
        // round() ties to even: 0.0000005 mm is 0.5 nm
        assert_eq!(nm(0.0000005), Ok(0));
        assert_eq!(nm(0.0000015), Ok(2));
        assert!(nm(10000.5).is_err());
        assert!(nm(f64::NAN).is_err());
        assert_eq!(
            nm_of(&json!("1")).unwrap_err().to_string(),
            "must be real number, not str"
        );
        assert_eq!(nm_of(&json!(true)), Ok(1_000_000));
        assert_eq!(py_round(2.675, 2), 2.67); // the binary value is below the tie
        assert_eq!(py_round(0.125, 2), 0.12); // an exact tie rounds to even
        assert_eq!(py_round(-0.0000001, 6), -0.0);
        assert_eq!(
            vec_point(&json!([1, 2, 3])).unwrap_err().to_string(),
            "_vec() takes 2 positional arguments but 3 were given"
        );
        assert_eq!(
            vec_point(&json!(5)).unwrap_err().to_string(),
            "board_client._vec() argument after * must be an iterable, not int"
        );
        assert_eq!(
            vec_point(&json!("ab")).unwrap_err().to_string(),
            "must be real number, not str"
        );
        // Python 3.12's compensated sum
        assert_eq!(py_sum([0.1, 0.2, 0.3]), 0.6);
        assert_eq!(py_sum([1e100, 1.0, -1e100]), 1.0);
    }

    #[test]
    fn layers_follow_python() {
        assert_eq!(
            layer_name(i64::from(layer("F.Cu").unwrap())).unwrap(),
            "F.Cu"
        );
        assert_eq!(
            layer_name(i64::from(layer("Edge").unwrap())).unwrap(),
            "Edge.Cuts"
        );
        assert_eq!(
            layer_name(i64::from(layer("In2.Cu").unwrap())).unwrap(),
            "In2.Cu"
        );
        assert_eq!(
            layer_name(i64::from(layer("User.1").unwrap())).unwrap(),
            "User.1"
        );
        assert_eq!(layer_name(0).unwrap(), "UNKNOWN");
        assert_eq!(
            layer_name(9999).unwrap_err().to_string(),
            "Enum BoardLayer has no name defined for value 9999"
        );
        assert_eq!(
            layer("Nope").unwrap_err().to_string(),
            "Unknown layer 'Nope'; use KiCad names like F.Cu, In1.Cu, B.SilkS, Edge.Cuts"
        );
        assert!(is_copper(layer("B.Cu").unwrap()));
        assert!(!is_copper(layer("F.SilkS").unwrap()));
        assert_eq!(
            layer_of(&json!(5)).unwrap_err().to_string(),
            "'int' object has no attribute 'replace'"
        );
        assert_eq!(copper_layer_names(4), ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]);
    }

    #[test]
    fn dog_legs_follow_python() {
        assert_eq!(
            route_points([0.0, 0.0], [10.0, 4.0], "horizontal_first"),
            [[0.0, 0.0], [6.0, 0.0], [10.0, 4.0]]
        );
        assert_eq!(
            route_points([0.0, 0.0], [10.0, -4.0], "diagonal_first"),
            [[0.0, 0.0], [4.0, -4.0], [10.0, -4.0]]
        );
        assert_eq!(
            route_points([0.0, 0.0], [3.0, 4.0], "orthogonal"),
            [[0.0, 0.0], [3.0, 0.0], [3.0, 4.0]]
        );
        assert_eq!(
            route_points([0.0, 0.0], [5.0, 5.0], "x"),
            [[0.0, 0.0], [5.0, 5.0]]
        );
    }

    #[test]
    fn items_are_built_like_python() {
        let track = make_track(&json!([1, 2]), &json!([3.5, 2]), 0.25, 3, Some("GND")).unwrap();
        let value = pbjson::to_json(&track).unwrap();
        assert_eq!(
            value["start"],
            json!({"x_nm": "1000000", "y_nm": "2000000"})
        );
        assert_eq!(value["width"], json!({"value_nm": "250000"}));
        assert_eq!(value["net"], json!({"name": "GND"}));
        let via = make_via(&json!([0, 0]), None, 0.6, 0.3, "F.Cu", "B.Cu").unwrap();
        let value = pbjson::to_json(&via).unwrap();
        assert_eq!(value["type"], "VT_THROUGH");
        assert_eq!(value["net"], json!({}));
        let zone = make_zone(
            &json!([[0, 0], [1, 0], [1, 1]]),
            &["F.Cu".into()],
            &ZoneSettings {
                connection: "bogus",
                ..ZoneSettings::default()
            },
            None,
        );
        assert_eq!(
            zone.unwrap_err().to_string(),
            "connection must be one of ['thermal', 'solid', 'none', 'pth_thermal']"
        );
        let rule = RuleArea {
            copper: true,
            vias: true,
            tracks: true,
            pads: false,
            footprints: false,
        };
        let keepout = make_zone(
            &json!([[0, 0], [1, 0], [1, 1]]),
            &[],
            &ZoneSettings::default(),
            Some(rule),
        );
        let value = pbjson::to_json(&keepout.unwrap()).unwrap();
        assert_eq!(value["type"], "ZT_RULE_AREA");
        assert_eq!(
            value["rule_area_settings"],
            json!({"keepout_copper": true, "keepout_vias": true, "keepout_tracks": true})
        );
        let text = make_text(
            &json!("B"),
            &json!([0, 0]),
            layer("B.SilkS").unwrap(),
            &json!(1),
            &json!(0.0),
            None,
            None,
        )
        .unwrap();
        let value = pbjson::to_json(&text).unwrap();
        assert_eq!(value["text"]["attributes"]["mirrored"], true);
        assert_eq!(
            value["text"]["attributes"]["stroke_width"],
            json!({"value_nm": "150000"})
        );
        assert_eq!(
            rounded_rectangle(0.0, 0.0, 10.0, 10.0, 2.0, 44, 0.05)
                .unwrap()
                .len(),
            8
        );
        assert!(rounded_rectangle(0.0, 0.0, 10.0, 3.0, 2.0, 44, 0.05).is_err());
        assert_eq!(
            unpack4(&json!([1, 2])).unwrap_err().to_string(),
            "not enough values to unpack (expected 4, got 2)"
        );
        assert_eq!(normalized_rotation(&json!(190)).unwrap(), -170.0);
        assert_eq!(normalized_rotation(&json!(-180.0)).unwrap(), -180.0);
        assert_eq!(normalized_rotation(&json!(540)).unwrap(), -180.0);
    }

    #[test]
    fn only_board_items_are_accepted() {
        let track = json!({"@type": "type.googleapis.com/kiapi.board.types.Track"});
        assert_eq!(parse_board_items(&[track]).unwrap().len(), 1);
        for other in [
            json!({"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine"}),
            json!({}),
            json!({"@type": "type.googleapis.com/kiapi.common.types.KIID"}),
        ] {
            assert_eq!(
                parse_board_items(&[other]).unwrap_err().to_string(),
                "Only kiapi.board.types item types are accepted"
            );
        }
    }

    /// A scripted KiCad for the helpers no tool of this area calls (the board design tools do):
    /// an NNG REP0 endpoint that answers each request in order and records it.
    mod peer {
        use super::*;
        use prost::Message as _;
        use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

        /// The reply to one request: its command message in, the reply message out.
        pub type Respond = Box<dyn Fn(&DynamicMessage) -> DynamicMessage + Send + Sync>;

        pub fn respond(
            reply: impl Fn(&DynamicMessage) -> DynamicMessage + Send + Sync + 'static,
        ) -> Respond {
            Box::new(reply)
        }

        /// A fixed reply of `full_name` from protobuf JSON.
        pub fn fixed(full_name: &'static str, value: Json) -> Respond {
            Box::new(move |_| build(full_name, value.clone()))
        }

        /// Answers one connection; None when it closes before sending a request (the client's
        /// reachability probe connects and closes on Unix sockets).
        async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
            stream: &mut S,
            kind: &str,
            respond: &Respond,
        ) -> Option<DynamicMessage> {
            stream
                .write_all(&[0, b'S', b'P', 0, 0, 0x31, 0, 0])
                .await
                .ok()?;
            let mut head = [0u8; 8];
            stream.read_exact(&mut head).await.ok()?;
            let mut frame = [0u8; 9];
            stream.read_exact(&mut frame).await.ok()?;
            let length = u64::from_be_bytes(frame[1..].try_into().unwrap()) as usize;
            let mut message = vec![0u8; length];
            stream.read_exact(&mut message).await.unwrap();
            let request =
                DynamicMessage::decode(proto::descriptor("kiapi.common.ApiRequest"), &message[4..])
                    .unwrap();
            let packed = message_field(&request, "message");
            let type_name = proto::any_type_name(&packed);
            assert!(
                type_name.ends_with(&format!(".{kind}")),
                "{type_name} for {kind}"
            );
            let command = proto::unpack(&packed, &type_name).unwrap();
            let reply = with_field(
                build(
                    "kiapi.common.ApiResponse",
                    json!({"header": {"kicad_token": "t"}, "status": {"status": "AS_OK"}}),
                ),
                "message",
                proto::pack(&respond(&command)),
            )
            .encode_to_vec();
            let mut out = vec![1];
            out.extend_from_slice(&(4 + reply.len() as u64).to_be_bytes());
            out.extend_from_slice(&message[..4]);
            out.extend_from_slice(&reply);
            stream.write_all(&out).await.unwrap();
            Some(command)
        }

        /// Starts the endpoint; returns a client connected to it and the task that ends with the
        /// commands it received.
        pub async fn start(
            script: Vec<(&'static str, Respond)>,
        ) -> (Client, tokio::task::JoinHandle<Vec<DynamicMessage>>) {
            let directory =
                std::env::temp_dir().join(format!("emb-board-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("api.sock").to_string_lossy().into_owned();
            let url = format!("ipc://{path}");
            #[cfg(windows)]
            let handle = {
                use tokio::net::windows::named_pipe::ServerOptions;
                let name = format!(r"\\.\pipe\{path}");
                let mut server = ServerOptions::new()
                    .first_pipe_instance(true)
                    .create(&name)
                    .unwrap();
                tokio::spawn(async move {
                    let mut commands = Vec::new();
                    for (kind, respond) in script {
                        server.connect().await.unwrap();
                        let mut connected = server;
                        // The next instance is listed before this reply, as KiCad's endpoint is
                        server = ServerOptions::new().create(&name).unwrap();
                        commands.push(serve(&mut connected, kind, &respond).await.unwrap());
                    }
                    commands
                })
            };
            #[cfg(unix)]
            let handle = {
                let listener = tokio::net::UnixListener::bind(&path).unwrap();
                tokio::spawn(async move {
                    let mut commands = Vec::new();
                    for (kind, respond) in script {
                        // Connections that close unanswered are reachability probes: skip them
                        loop {
                            let (mut stream, _) = listener.accept().await.unwrap();
                            if let Some(command) = serve(&mut stream, kind, &respond).await {
                                commands.push(command);
                                break;
                            }
                        }
                    }
                    commands
                })
            };
            (Client::new(Some(url), 5000).unwrap(), handle)
        }
    }

    use peer::{fixed, respond};

    fn board_doc() -> DynamicMessage {
        build(
            &format!("{T}DocumentSpecifier"),
            json!({"type": "DOCTYPE_PCB", "board_filename": "t.kicad_pcb"}),
        )
    }

    const FP_ID: &str = "f0000000-0000-0000-0000-000000000001";

    fn footprints_reply() -> peer::Respond {
        fixed(
            "kiapi.common.commands.GetItemsResponse",
            json!({"status": "IRS_OK", "items": [
                {"@type": "type.googleapis.com/kiapi.board.types.FootprintInstance",
                 "id": {"value": FP_ID}, "layer": "BL_F_Cu",
                 "reference_field": {"text": {"text": {"text": "U1"}}}}]}),
        )
    }

    fn pads_reply() -> peer::Respond {
        fixed(
            "kiapi.common.commands.GetItemsResponse",
            json!({"status": "IRS_OK", "items": [
                {"@type": "type.googleapis.com/kiapi.board.types.Pad", "number": "1",
                 "parent": {"value": FP_ID}, "net": {"name": "SIG"},
                 "position": {"x_nm": "1000000", "y_nm": "1000000"}},
                {"@type": "type.googleapis.com/kiapi.board.types.Pad", "number": "2",
                 "parent": {"value": FP_ID}, "net": {"name": "SIG"},
                 "position": {"x_nm": "11000000", "y_nm": "5000000"}}]}),
        )
    }

    fn netclass_reply(track_width_nm: &'static str) -> peer::Respond {
        respond(move |request| {
            let net = text_at(&messages(request, "net")[0], "name");
            let mut classes = Map::new();
            classes.insert(
                net,
                json!({"name": "Default", "board": {
                "track_width": {"value_nm": track_width_nm},
                "via_stack": {"copper_layers": [{"size": {"x_nm": "800000", "y_nm": "800000"}}],
                              "drill": {"diameter": {"x_nm": "400000", "y_nm": "400000"}}}}}),
            );
            build(
                &format!("{BC}NetClassForNetsResponse"),
                json!({"classes": classes}),
            )
        })
    }

    /// BeginCommit, the item step (every item acknowledged), EndCommit.
    fn commit(command: &'static str) -> Vec<(&'static str, peer::Respond)> {
        vec![
            (
                "BeginCommit",
                fixed(
                    "kiapi.common.commands.BeginCommitResponse",
                    json!({"id": {"value": "c"}}),
                ),
            ),
            (
                command,
                respond(move |request| {
                    let count = list_field(request, "items").len();
                    let field = if command == "UpdateItems" {
                        "updated_items"
                    } else {
                        "created_items"
                    };
                    let mut value = Map::new();
                    value.insert("status".into(), json!("IRS_OK"));
                    value.insert(
                        field.into(),
                        json!(vec![json!({"status": {"code": "ISC_OK"}}); count]),
                    );
                    build(&format!("{C}{command}Response"), Json::Object(value))
                }),
            ),
            (
                "EndCommit",
                fixed("kiapi.common.commands.EndCommitResponse", json!({})),
            ),
        ]
    }

    fn created_items(command: &DynamicMessage) -> Vec<Json> {
        list_field(command, "items")
            .into_iter()
            .map(|v| match v {
                Value::Message(any) => pbjson::to_json(&any).unwrap(),
                _ => Json::Null,
            })
            .collect()
    }

    #[tokio::test]
    async fn find_pad_and_widths_follow_python() {
        let script = vec![
            ("GetItems", footprints_reply()),
            ("GetItems", pads_reply()),
            ("GetItems", footprints_reply()),
            ("GetItems", pads_reply()),
            ("GetNetClassForNets", netclass_reply("300000")),
        ];
        let (mut client, peer) = peer::start(script).await;
        let doc = board_doc();
        let pad = find_pad(&mut client, &doc, "U1", "2").await.unwrap();
        assert_eq!(mm(&message_field(&pad, "position")), [11.0, 5.0]);
        let missing = find_pad(&mut client, &doc, "U1", "9").await.unwrap_err();
        assert_eq!(missing.to_string(), "No pad U1.9");
        // No net: the default width, nothing asked
        assert_eq!(
            width_for(&mut client, &doc, Some(""), None).await.unwrap(),
            0.25
        );
        assert_eq!(
            width_for(&mut client, &doc, None, Some(0.4)).await.unwrap(),
            0.4
        );
        assert_eq!(
            width_for(&mut client, &doc, Some("SIG"), None)
                .await
                .unwrap(),
            0.3
        );
        let commands = peer.await.unwrap();
        assert_eq!(commands.len(), 5);
        assert_eq!(
            pbjson::to_json(&commands[4]).unwrap(),
            json!({"net": [{"name": "SIG"}]})
        );
    }

    #[tokio::test]
    async fn plain_route_draws_a_dog_leg_through_a_via() {
        let mut script = vec![
            ("GetItems", footprints_reply()),
            ("GetItems", pads_reply()),
            ("GetItems", footprints_reply()),
            ("GetItems", pads_reply()),
            ("GetItems", footprints_reply()),
            ("GetNetClassForNets", netclass_reply("200000")),
            ("GetNetClassForNets", netclass_reply("200000")),
        ];
        script.extend(commit("CreateItems"));
        let (mut client, peer) = peer::start(script).await;
        let route = PlainRoute {
            from_ref: "U1",
            from_pad: "1",
            to_ref: "U1",
            to_pad: "2",
            layer: None,
            width: None,
            posture: "horizontal_first",
            via_at: Some([5.0, 1.0]),
            second_layer: None,
        };
        let result = board_route(&mut client, &board_doc(), &route)
            .await
            .unwrap();
        assert_eq!(result["net"], "SIG");
        assert_eq!(result["width_mm"], json!(0.2));
        assert_eq!(result["layer"], "F.Cu");
        assert_eq!(
            result["path_mm"],
            json!([[1.0, 1.0], [5.0, 1.0], [7.0, 1.0], [11.0, 5.0]])
        );
        let commands = peer.await.unwrap();
        let items = created_items(&commands[8]);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0]["layer"], "BL_F_Cu");
        assert_eq!(
            items[1]["@type"],
            "type.googleapis.com/kiapi.board.types.Via"
        );
        assert_eq!(
            items[1]["pad_stack"]["copper_layers"][0]["size"],
            json!({"x_nm": "800000", "y_nm": "800000"})
        );
        assert_eq!(items[3]["layer"], "BL_B_Cu");
        assert_eq!(
            pbjson::to_json(&commands[9]).unwrap()["message"],
            "MCP: route U1.1 to U1.2"
        );
    }

    #[tokio::test]
    async fn tracks_and_keepouts_are_one_commit_each() {
        let mut script = vec![("GetNetClassForNets", netclass_reply("0"))];
        script.extend(commit("CreateItems"));
        script.extend(commit("CreateItems"));
        script.extend(commit("CreateItems"));
        let (mut client, peer) = peer::start(script).await;
        let doc = board_doc();
        let one = [vec![0.0, 0.0]];
        let two = [vec![0.0, 0.0], vec![1.0, 0.0]];
        let refused = board_add_tracks(&mut client, &doc, &one, "F.Cu", None, None);
        assert_eq!(
            refused.await.unwrap_err().to_string(),
            "Supply between 2 and 200 points"
        );
        let silk = board_add_tracks(&mut client, &doc, &two, "F.SilkS", None, None);
        assert_eq!(
            silk.await.unwrap_err().to_string(),
            "Tracks go on copper layers (F.Cu, B.Cu, In1.Cu, ...)"
        );
        // A class width of 0 is falsy: 0.25 mm; the repeated point draws nothing
        let points = [vec![0.0, 0.0], vec![0.0, 0.0], vec![2.0, 0.0]];
        let result = board_add_tracks(&mut client, &doc, &points, "B.Cu", None, Some("SIG"))
            .await
            .unwrap();
        assert_eq!(result["width_mm"], json!(0.25));
        assert_eq!(result["tracks"].as_array().unwrap().len(), 1);
        let rule = RuleArea {
            copper: true,
            vias: true,
            tracks: true,
            pads: false,
            footprints: false,
        };
        let keepout = board_add_keepout(
            &mut client,
            &doc,
            &json!([[0, 0], [1, 0], [1, 1]]),
            &["F.Cu".into(), "B.Cu".into()],
            "antenna",
            rule,
        )
        .await
        .unwrap();
        assert!(keepout["rule_area"].as_str().is_some());
        let track = make_track(&json!([0, 0]), &json!([1, 1]), 0.2, 3, None).unwrap();
        board_create(&mut client, &doc, &[track], "MCP: test")
            .await
            .unwrap();
        let commands = peer.await.unwrap();
        assert_eq!(
            pbjson::to_json(&commands[3]).unwrap()["message"],
            "MCP: add tracks"
        );
        let zone = &created_items(&commands[5])[0];
        assert_eq!(zone["name"], "antenna");
        assert_eq!(zone["layers"], json!(["BL_F_Cu", "BL_B_Cu"]));
        assert_eq!(
            pbjson::to_json(&commands[6]).unwrap()["message"],
            "MCP: add keepout"
        );
        assert_eq!(
            pbjson::to_json(&commands[9]).unwrap()["message"],
            "MCP: test"
        );
    }

    #[tokio::test]
    async fn rules_and_stackup_are_read_and_merged() {
        let rules = json!({"rules": {"constraints": {"min_clearance": {"value_nm": "200000"}}}});
        let stackup = json!({"stackup": {"finish": {"type_name": "ENIG"}, "layers": [
            {"type": "BSLT_COPPER", "layer": "BL_F_Cu", "thickness": {"value_nm": "35000"}},
            {"type": "BSLT_DIELECTRIC", "thickness": {"value_nm": "1510000"},
             "dielectric": {"type": "BSDT_CORE", "layer": [
                 {"thickness": {"value_nm": "1510000"}, "epsilon_r": 4.5}]}},
            {"type": "BSLT_COPPER", "layer": "BL_B_Cu", "thickness": {"value_nm": "35000"},
             "color": {"r": 1.0, "g": 0.5, "b": 0.0, "a": 1.0}}]}});
        let script = vec![
            (
                "GetBoardDesignRules",
                fixed(
                    "kiapi.board.commands.BoardDesignRulesResponse",
                    rules.clone(),
                ),
            ),
            (
                "GetBoardDesignRules",
                fixed(
                    "kiapi.board.commands.BoardDesignRulesResponse",
                    rules.clone(),
                ),
            ),
            (
                "SetBoardDesignRules",
                respond(|request| {
                    with_field(
                        build("kiapi.board.commands.BoardDesignRulesResponse", json!({})),
                        "rules",
                        message_field(request, "rules"),
                    )
                }),
            ),
            (
                "GetBoardStackup",
                fixed("kiapi.board.commands.BoardStackupResponse", stackup.clone()),
            ),
            (
                "GetBoardStackup",
                fixed("kiapi.board.commands.BoardStackupResponse", stackup),
            ),
        ];
        let (mut client, peer) = peer::start(script).await;
        let doc = board_doc();
        assert_eq!(
            board_design_rules(&mut client, &doc, None).await.unwrap(),
            rules
        );
        let changes = json!({"constraints": {"min_track_width": {"value_nm": 150000}}});
        let changed = board_design_rules(&mut client, &doc, Some(&changes))
            .await
            .unwrap();
        assert_eq!(
            changed["rules"]["constraints"],
            json!({"min_clearance": {"value_nm": "200000"}, "min_track_width": {"value_nm": "150000"}})
        );
        let raw = board_stackup(&mut client, &doc).await.unwrap();
        assert_eq!(raw["finish"], json!({"type_name": "ENIG"}));
        let layers = board_stackup_layers(&mut client, &doc).await.unwrap();
        assert_eq!(layers["board_thickness_mm"], json!(1.58));
        assert_eq!(layers["layers"][1]["sublayers"][0]["epsilon_r"], json!(4.5));
        assert_eq!(layers["layers"][2]["color"], "#FF8000");
        assert_eq!(layers["finish"], "ENIG");
        assert_eq!(peer.await.unwrap().len(), 5);
    }

    #[test]
    fn stackup_colours_follow_python() {
        assert_eq!(hex_channel(1.0), "FF");
        assert_eq!(hex_channel(0.0), "00");
        assert_eq!(hex_channel(-0.004), "-1");
        assert_eq!(
            stackup_colour(Some("#00ff80")).unwrap(),
            Some(json!({"r": 0.0, "g": 1.0, "b": 128.0 / 255.0, "a": 1.0}))
        );
        assert_eq!(
            stackup_colour(Some("#zz")).unwrap_err().to_string(),
            "invalid literal for int() with base 16: 'zz'"
        );
        assert_eq!(
            stackup_colour(Some("#ff")).unwrap_err().to_string(),
            "invalid literal for int() with base 16: ''"
        );
    }
}
