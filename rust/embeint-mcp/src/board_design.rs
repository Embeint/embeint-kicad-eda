// SPDX-License-Identifier: GPL-3.0-or-later
//! Board design rules, net classes, stackup, RF widths, checks and obstacle-avoiding routing: a
//! port of `BoardDesignTools` and the module functions of
//! `tools/live_schematic_mcp/board_design.py`.
//!
//! The request order, the fields set, the results and the error texts are Python's;
//! `rust/parity/scenarios/board_design.py` compares them (exactly, except the routes and path
//! checks, which are compared by behaviour: see [`crate::board_router`]).  Values from a caller's
//! JSON objects (net classes, custom rules) are checked as Python checks them, with the TypeError,
//! KeyError and protobuf texts it raises.

use std::collections::HashMap;
use std::sync::Mutex;

use embeint_ipc::client::{item_header, Client, Error};
use embeint_ipc::pbjson::{self, py_repr};
use embeint_ipc::proto::{
    self, build, int_field, list_field, message_field, str_field, with_field,
};
use geo::{
    Coord, Distance, Euclidean, Geometry, Intersects, LineString, MultiPolygon, Polygon, Relate,
};
use prost_reflect::{DynamicMessage, MapKey, Value};
use serde_json::{json, Map, Value as Json};

use crate::board_client::{
    self as board, board_unpacked, item_id, layer, layer_name, messages, mm, nm, nm_of, py_round,
    real, sub, text_at, truthy, RuleArea, ZoneSettings,
};
use crate::board_rf;
use crate::board_router::{
    self as router, arc_points, circle, outline_from_lines, Obstacle, PadInfo, Route, RouteError,
    RouteProblem, RouteRequest, Shape, QUAD_SEGS,
};
use crate::tools::args::type_name;

const BC: &str = "kiapi.board.commands.";
const BT: &str = "kiapi.board.types.";
const C: &str = "kiapi.common.commands.";
const T: &str = "kiapi.common.types.";
const EMPTY: &str = "google.protobuf.Empty";

// ---- Python's formatting ------------------------------------------------------------------------

/// `repr(value)` of a float (shortest round trip; exponent as `e+16`, `e-05`).
pub(crate) fn py_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.into();
    }
    let text = format!("{value:?}");
    match text.split_once('e') {
        Some((mantissa, exponent)) => {
            let (sign, digits) = match exponent.strip_prefix('-') {
                Some(digits) => ('-', digits),
                None => ('+', exponent),
            };
            format!("{mantissa}e{sign}{digits:0>2}")
        }
        None => text,
    }
}

/// `f"{value:.{digits}f}"`.
pub(crate) fn py_fixed(value: f64, digits: usize) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.into();
    }
    format!("{value:.digits$}")
}

/// The repr of a list of strings, e.g. `['F.Cu', 'B.Cu']`.
pub(crate) fn py_list_str(items: &[String]) -> String {
    py_repr(&json!(items))
}

/// `KeyError(key)` as `str()` shows it.
fn key_error(key: &Json) -> Error {
    Error::Value(py_repr(key))
}

/// The name of an enum value as Python's `Enum.Name` (which refuses a number it does not know).
fn enum_name(full_name: &str, number: i64) -> Result<String, Error> {
    let short = full_name.rsplit('.').next().unwrap_or(full_name);
    proto::pool()
        .get_enum_by_name(full_name)
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

/// The number of an enum value as Python's `Enum.Value(name)`.
fn enum_value(full_name: &str, name: &str) -> Result<i32, Error> {
    let short = full_name.rsplit('.').next().unwrap_or(full_name);
    proto::pool()
        .get_enum_by_name(full_name)
        .and_then(|e| e.get_value_by_name(name))
        .map(|v| v.number())
        .ok_or_else(|| {
            Error::Value(format!(
                "Enum {short} has no value defined for name {}",
                py_repr(&json!(name))
            ))
        })
}

/// A value this module names, so it exists.
fn known(full_name: &str, name: &str) -> i32 {
    enum_value(full_name, name).unwrap_or_else(|_| panic!("{name} is not a value of {full_name}"))
}

const BOARD_LAYER: &str = "kiapi.board.types.BoardLayer";

/// `is_copper(value)`, which refuses a layer number the protocol does not know.
fn is_copper(value: i64) -> Result<bool, Error> {
    Ok(enum_name(BOARD_LAYER, value)?.ends_with("_Cu"))
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

fn has(message: &DynamicMessage, name: &str) -> bool {
    message.has_field_by_name(name)
}

/// A nested message field to change (set present, as Python's attribute access then assignment).
fn child<'a>(message: &'a mut DynamicMessage, name: &str) -> &'a mut DynamicMessage {
    match message.get_field_by_name_mut(name) {
        Some(Value::Message(inner)) => inner,
        _ => panic!("{name} is not a message field"),
    }
}

fn set(message: &mut DynamicMessage, name: &str, value: Value) {
    message.set_field_by_name(name, value);
}

/// `field.value_nm = _nm(value)` on a Distance at a dotted path.
fn set_distance(message: &mut DynamicMessage, path: &str, value_nm: i64) {
    let mut target = message;
    for name in path.split('.') {
        target = child(target, name);
    }
    set(target, "value_nm", Value::I64(value_nm));
}

/// A string field assignment from a caller's value (None leaves it unset in a constructor).
fn string_value(value: &Json) -> Result<Option<String>, Error> {
    match value {
        Json::String(text) => Ok(Some(text.clone())),
        Json::Null => Ok(None),
        _ => Err(Error::Value(
            "bad argument type for built-in operation".into(),
        )),
    }
}

/// An int32 field assignment from a caller's value, with protobuf's errors.
fn int32_value(value: &Json, field: &str) -> Result<i32, Error> {
    match value {
        Json::Bool(_) => Err(Error::Value(format!(
            "Field {field}: Expected an int, got a boolean."
        ))),
        Json::Number(n) if !n.to_string().contains(['.', 'e', 'E']) => match n.as_i64() {
            Some(v) if i32::try_from(v).is_ok() => Ok(v as i32),
            _ => Err(Error::Value(format!("Value out of range: {n}"))),
        },
        other => Err(Error::Value(format!(
            "'{}' object cannot be interpreted as an integer",
            type_name(other)
        ))),
    }
}

fn board_request(name: &str, value: Json, doc: &DynamicMessage) -> DynamicMessage {
    with_field(build(&format!("{BC}{name}"), value), "board", doc.clone())
}

/// A Python `for x in value` over a caller's JSON value: a list's items, a string's characters, a
/// dict's keys; TypeError otherwise.
fn py_iter(value: &Json) -> Result<Vec<Json>, Error> {
    match value {
        Json::Array(items) => Ok(items.clone()),
        Json::String(text) => Ok(text.chars().map(|c| json!(c.to_string())).collect()),
        Json::Object(map) => Ok(map.keys().map(|k| json!(k)).collect()),
        other => Err(Error::Value(format!(
            "'{}' object is not iterable",
            type_name(other)
        ))),
    }
}

/// `value.get(key)` on a caller's object, with the AttributeError Python raises for others.
fn get<'a>(value: &'a Json, key: &str) -> Result<Option<&'a Json>, Error> {
    match value {
        Json::Object(map) => Ok(map.get(key)),
        other => Err(Error::Value(format!(
            "'{}' object has no attribute 'get'",
            type_name(other)
        ))),
    }
}

/// `value.upper()` on a caller's value.
fn upper(value: &Json) -> Result<String, Error> {
    match value {
        Json::String(text) => Ok(text.to_uppercase()),
        other => Err(Error::Value(format!(
            "'{}' object has no attribute 'upper'",
            type_name(other)
        ))),
    }
}

// ---- DRC categories -----------------------------------------------------------------------------

/// DRC violation types by what they are about (names as `board_drc` reports them), in Python's
/// order.
const CHECK_CATEGORIES: &[(&str, &[&str])] = &[
    (
        "clearance",
        &[
            "clearance",
            "shorting_items",
            "tracks_crossing",
            "hole_clearance",
            "edge_clearance",
            "creepage",
            "copper_sliver",
            "zones_intersect",
            "drilled_holes_too_close",
            "drilled_holes_colocated",
        ],
    ),
    ("keepout", &["allowed_items"]),
    (
        "silkscreen",
        &[
            "silk_clearance",
            "silk_mask_clearance",
            "silk_edge_clearance",
            "text_height",
            "text_thickness",
            "line_width",
            "mirrored_text_on_front_layer",
            "nonmirrored_text_on_back_layer",
            "text_on_edgecuts",
        ],
    ),
    (
        "courtyard",
        &[
            "overlapping_footprints",
            "missing_courtyard",
            "malformed_courtyard",
            "pth_in_courtyard",
            "npth_in_courtyard",
        ],
    ),
    (
        "connectivity",
        &[
            "unconnected_items",
            "dangling_track",
            "dangling_via",
            "isolated_copper",
            "starved_thermal",
            "net_conflict",
        ],
    ),
    (
        "manufacturing",
        &[
            "track_width",
            "connection_width",
            "annular_width",
            "drill_out_of_range",
            "via_diameter",
            "padstack",
            "padstack_invalid",
            "microvia_drill_out_of_range",
            "soldermask_bridge",
            "track_angle",
            "track_segment_length",
            "track_on_post_machined_layer",
            "track_not_centered_on_via",
            "invalid_outline",
            "disabled_layer_item",
        ],
    ),
    (
        "schematic",
        &[
            "missing_footprint",
            "extra_footprint",
            "duplicate_footprint",
            "schematic_parity",
            "schematic_fields_parity",
            "footprint_filters",
            "lib_footprint_issues",
            "lib_footprint_mismatch",
            "footprint_type_mismatch",
            "footprint",
        ],
    ),
];

fn category_of(kind: &str) -> Option<&'static str> {
    CHECK_CATEGORIES
        .iter()
        .find(|(_, kinds)| kinds.contains(&kind))
        .map(|(name, _)| *name)
}

/// `ROUTE_VIOLATIONS`: clearance, keepout and manufacturing findings.
fn is_route_violation(kind: &str) -> bool {
    matches!(
        category_of(kind),
        Some("clearance" | "keepout" | "manufacturing")
    )
}

/// `RULE_FIELDS`: MinimumConstraints field and tool argument, in Python's order.
const RULE_FIELDS: &[(&str, &str)] = &[
    ("min_clearance", "min_clearance"),
    ("min_track_width", "min_track_width"),
    ("min_connection_width", "min_connection_width"),
    ("min_via_annular_width", "min_annular_ring"),
    ("min_via_size", "min_via_diameter"),
    ("min_through_drill", "min_drill"),
    ("min_microvia_size", "min_microvia_diameter"),
    ("min_microvia_drill", "min_microvia_drill"),
    ("copper_edge_clearance", "copper_edge_clearance"),
    ("hole_clearance", "hole_clearance"),
    ("hole_to_hole_min", "hole_to_hole"),
    ("silk_clearance", "silk_clearance"),
    ("min_silk_text_height", "min_silk_text_height"),
    ("min_silk_text_thickness", "min_silk_text_thickness"),
    ("min_groove_width", "min_groove_width"),
];

/// `SEVERITIES`.
fn severity(level: &str) -> Option<i32> {
    let name = match level {
        "error" => "RS_ERROR",
        "warning" => "RS_WARNING",
        "ignore" => "RS_IGNORE",
        _ => return None,
    };
    Some(known(&format!("{T}RuleSeverity"), name))
}

/// `SEVERITIES[value]` for a caller's value (KeyError, or TypeError for an unhashable one).
fn severity_of(value: &Json) -> Result<i32, Error> {
    match value {
        Json::Array(_) | Json::Object(_) => Err(Error::Value(format!(
            "unhashable type: '{}'",
            type_name(value)
        ))),
        Json::String(level) => severity(level).ok_or_else(|| key_error(value)),
        other => Err(key_error(other)),
    }
}

// ---- Geometry from the protocol -----------------------------------------------------------------

fn coord(p: [f64; 2]) -> Coord<f64> {
    Coord { x: p[0], y: p[1] }
}

fn coords(points: &[[f64; 2]]) -> Vec<Coord<f64>> {
    points.iter().map(|p| coord(*p)).collect()
}

/// `_poly_line(line)`: a PolyLine's points (arcs as 12 segments).
fn poly_line(line: &DynamicMessage) -> Vec<[f64; 2]> {
    let mut points: Vec<[f64; 2]> = Vec::new();
    for node in messages(line, "nodes") {
        if has(&node, "arc") {
            let arc = message_field(&node, "arc");
            let pts = arc_points(
                mm(&message_field(&arc, "start")),
                mm(&message_field(&arc, "mid")),
                mm(&message_field(&arc, "end")),
                12,
            );
            let skip = usize::from(!points.is_empty());
            points.extend(pts.into_iter().skip(skip));
        } else {
            points.push(mm(&message_field(&node, "point")));
        }
    }
    points
}

/// `polygon_from_proto(polygon)`: a polygon from a kiapi PolygonWithHoles, or None.  An invalid
/// polygon is cleaned (shapely's `buffer(0)`) by an overlay with itself.
fn polygon_from_proto(polygon: &DynamicMessage) -> Option<Shape> {
    use geo::{BooleanOps, Validation};
    let outline = poly_line(&message_field(polygon, "outline"));
    if outline.len() < 3 {
        return None;
    }
    let holes: Vec<LineString<f64>> = messages(polygon, "holes")
        .iter()
        .map(poly_line)
        .filter(|h| h.len() >= 3)
        .map(|h| LineString::new(coords(&h)))
        .collect();
    let shape = Polygon::new(LineString::new(coords(&outline)), holes);
    if shape.is_valid() {
        return Some(Shape::polygon(shape));
    }
    let empty = MultiPolygon::new(vec![]);
    Some(Shape::multi(MultiPolygon::new(vec![shape]).union(&empty)))
}

/// `polyset_geometry(polyset)`: the union of a PolySet's polygons, or None.
fn polyset_geometry(polyset: &DynamicMessage) -> Option<Shape> {
    let shapes: Vec<Shape> = messages(polyset, "polygons")
        .iter()
        .filter_map(polygon_from_proto)
        .filter(|s| !s.is_empty())
        .collect();
    match shapes.len() {
        0 => None,
        1 => shapes.into_iter().next(),
        _ => {
            let parts: Vec<MultiPolygon<f64>> = shapes.iter().map(Shape::area).collect();
            Some(Shape::multi(geo::unary_union(parts.iter())))
        }
    }
}

/// `exterior.buffer(r)` of a ring: both sides, GEOS's polygons for a convex ring.
fn ring_buffer(ring: &LineString<f64>, distance: f64) -> Shape {
    let polygon = Shape::polygon(Polygon::new(ring.clone(), vec![]));
    if polygon.is_convex() {
        let outer = polygon.buffer(distance, QUAD_SEGS);
        let inner = polygon.buffer(-distance, QUAD_SEGS);
        if let (Geometry::Polygon(o), Geometry::Polygon(i)) = (&outer.geometry, &inner.geometry) {
            return Shape::polygon(Polygon::new(
                o.exterior().clone(),
                vec![i.exterior().clone()],
            ));
        }
        return outer;
    }
    Shape::line(ring.0.clone()).buffer(distance, QUAD_SEGS)
}

/// `area.boundary.buffer(r)`.
fn boundary_buffer(area: &Shape, distance: f64) -> Shape {
    let polygons = area.polygons();
    if let [polygon] = polygons.as_slice() {
        if polygon.interiors().is_empty() {
            return ring_buffer(polygon.exterior(), distance);
        }
    }
    let rings: Vec<LineString<f64>> = polygons
        .iter()
        .flat_map(|p| std::iter::once(p.exterior().clone()).chain(p.interiors().iter().cloned()))
        .collect();
    Shape::multi(router::geo_buffer(
        &Geometry::MultiLineString(geo::MultiLineString::new(rings)),
        distance,
        QUAD_SEGS,
    ))
}

/// `shape_geometry(shape)`: the copper area of a BoardGraphicShape (a stroke, or a filled area),
/// or None.
fn shape_geometry(shape: &DynamicMessage) -> Option<Shape> {
    let g = message_field(shape, "shape");
    let attributes = message_field(&g, "attributes");
    let width = (int_field(&sub(&attributes, "stroke.width"), "value_nm") as f64 / 1e6).max(0.0);
    let filled = int_field(&message_field(&attributes, "fill"), "fill_type")
        == i64::from(known(&format!("{T}GraphicFillType"), "GFT_FILLED"));
    let outline_width = (width / 2.0).max(1e-4);
    if has(&g, "segment") {
        let s = message_field(&g, "segment");
        let line = Shape::line(coords(&[
            mm(&message_field(&s, "start")),
            mm(&message_field(&s, "end")),
        ]));
        return Some(if width != 0.0 {
            line.buffer(width / 2.0, QUAD_SEGS)
        } else {
            line
        });
    }
    if has(&g, "arc") {
        let a = message_field(&g, "arc");
        let line = Shape::line(coords(&arc_points(
            mm(&message_field(&a, "start")),
            mm(&message_field(&a, "mid")),
            mm(&message_field(&a, "end")),
            24,
        )));
        return Some(if width != 0.0 {
            line.buffer(width / 2.0, QUAD_SEGS)
        } else {
            line
        });
    }
    if has(&g, "rectangle") {
        let r = message_field(&g, "rectangle");
        let ([x1, y1], [x2, y2]) = (
            mm(&message_field(&r, "top_left")),
            mm(&message_field(&r, "bottom_right")),
        );
        let ring = LineString::new(coords(&[[x1, y1], [x2, y1], [x2, y2], [x1, y2], [x1, y1]]));
        let area = Shape::polygon(Polygon::new(ring.clone(), vec![]));
        return Some(if filled {
            area.buffer(width / 2.0, QUAD_SEGS)
        } else {
            ring_buffer(&ring, outline_width)
        });
    }
    if has(&g, "circle") {
        let ci = message_field(&g, "circle");
        let centre = mm(&message_field(&ci, "center"));
        let radius = router::py_dist(centre, mm(&message_field(&ci, "radius_point")));
        let Some(disc) = circle(coord(centre), radius, QUAD_SEGS) else {
            return Some(Shape::empty());
        };
        return Some(if filled {
            Shape::polygon(disc).buffer(width / 2.0, QUAD_SEGS)
        } else {
            ring_buffer(disc.exterior(), outline_width)
        });
    }
    if has(&g, "polygon") {
        let area = polyset_geometry(&message_field(&g, "polygon"))?;
        return Some(if filled {
            area.buffer(width / 2.0, QUAD_SEGS)
        } else {
            boundary_buffer(&area, outline_width)
        });
    }
    None
}

/// `edge_lines(shape)`: the Edge.Cuts line work of a shape, for the board outline.
fn edge_lines(shape: &DynamicMessage) -> Vec<Vec<[f64; 2]>> {
    let g = message_field(shape, "shape");
    if has(&g, "segment") {
        let s = message_field(&g, "segment");
        return vec![vec![
            mm(&message_field(&s, "start")),
            mm(&message_field(&s, "end")),
        ]];
    }
    if has(&g, "arc") {
        let a = message_field(&g, "arc");
        return vec![arc_points(
            mm(&message_field(&a, "start")),
            mm(&message_field(&a, "mid")),
            mm(&message_field(&a, "end")),
            24,
        )];
    }
    if has(&g, "rectangle") {
        let r = message_field(&g, "rectangle");
        let ([x1, y1], [x2, y2]) = (
            mm(&message_field(&r, "top_left")),
            mm(&message_field(&r, "bottom_right")),
        );
        return vec![vec![[x1, y1], [x2, y1], [x2, y2], [x1, y2], [x1, y1]]];
    }
    if has(&g, "circle") {
        let ci = message_field(&g, "circle");
        let centre = mm(&message_field(&ci, "center"));
        let radius = router::py_dist(centre, mm(&message_field(&ci, "radius_point")));
        return match circle(coord(centre), radius, 32) {
            Some(disc) => vec![disc.exterior().coords().map(|p| [p.x, p.y]).collect()],
            None => vec![],
        };
    }
    if has(&g, "polygon") {
        return match polyset_geometry(&message_field(&g, "polygon")) {
            Some(area) => area
                .polygons()
                .iter()
                .map(|p| p.exterior().coords().map(|c| [c.x, c.y]).collect())
                .collect(),
            None => vec![],
        };
    }
    vec![]
}

fn point_buffer(position: [f64; 2], radius: f64) -> Shape {
    circle(coord(position), radius, QUAD_SEGS).map_or_else(Shape::empty, Shape::polygon)
}

fn line_buffer(points: &[[f64; 2]], radius: f64) -> Shape {
    Shape::line(coords(points)).buffer(radius, QUAD_SEGS)
}

// ---- Reading the board for the router ------------------------------------------------------------

/// `board_copper_layers(doc)`: F.Cu, In1.Cu ... B.Cu for the enabled copper layer count.
pub(crate) async fn board_copper_layers(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<Vec<String>, Error> {
    let response = client
        .send(
            &board_request("GetBoardEnabledLayers", json!({}), doc),
            &format!("{BC}BoardEnabledLayersResponse"),
        )
        .await?;
    let count = int_field(&response, "copper_layer_count");
    Ok(board::copper_layer_names(if count == 0 {
        2
    } else {
        count
    }))
}

/// `route_problem(doc)`: everything the router avoids, read from the live board.
pub(crate) async fn route_problem(
    client: &mut Client,
    doc: &DynamicMessage,
) -> Result<RouteProblem, Error> {
    let copper = board_copper_layers(client, doc).await?;
    let rules = client
        .send(
            &board_request("GetBoardDesignRules", json!({}), doc),
            &format!("{BC}BoardDesignRulesResponse"),
        )
        .await?;
    let constraints = sub(&rules, "rules.constraints");
    let distance = |name: &str, default: f64| {
        let value = int_field(&message_field(&constraints, name), "value_nm") as f64 / 1e6;
        if value == 0.0 {
            default
        } else {
            value
        }
    };
    let mut problem = RouteProblem {
        layers: copper.clone(),
        min_clearance: distance("min_clearance", 0.2),
        edge_clearance: distance("copper_edge_clearance", 0.3),
        ..RouteProblem::default()
    };
    let footprints = board_unpacked(client, doc, "footprints").await?;

    fn add(
        problem: &mut RouteProblem,
        name: &str,
        geometry: Option<Shape>,
        net: &str,
        kind: &'static str,
    ) {
        if let Some(geometry) = geometry {
            if !geometry.is_empty() && problem.layers.iter().any(|l| l == name) {
                problem.add_obstacle(
                    name,
                    Obstacle {
                        geometry,
                        net: net.to_owned(),
                        kind,
                        key: String::new(),
                    },
                );
            }
        }
    }

    // Board outline and copper shapes
    let edge = i64::from(known(BOARD_LAYER, "BL_Edge_Cuts"));
    let mut lines = Vec::new();
    for shape in board_unpacked(client, doc, "shapes").await? {
        let value = int_field(&shape, "layer");
        if value == edge {
            lines.extend(edge_lines(&shape));
        } else if is_copper(value)? {
            let name = layer_name(value)?;
            add(
                &mut problem,
                &name,
                shape_geometry(&shape),
                &text_at(&shape, "net.name"),
                "copper",
            );
        }
    }
    problem.outline = outline_from_lines(&lines);

    // Pads (exact shapes per layer)
    let pads = board_unpacked(client, doc, "pads").await?;
    let pth = i64::from(known(&format!("{BT}PadType"), "PT_PTH"));
    let npth = i64::from(known(&format!("{BT}PadType"), "PT_NPTH"));
    let mut by_layer: Vec<(String, Vec<DynamicMessage>)> = Vec::new();
    for pad in &pads {
        let stack = message_field(pad, "pad_stack");
        let stack_layers: Vec<i64> = list_field(&stack, "layers")
            .into_iter()
            .filter_map(|v| match v {
                Value::EnumNumber(n) => Some(i64::from(n)),
                _ => None,
            })
            .collect();
        let mut present = Vec::new();
        for value in &stack_layers {
            if is_copper(*value)? {
                present.push(layer_name(*value)?);
            }
        }
        // `if type in (PTH, NPTH) or UNDEFINED in layers: present = copper if PTH else present`
        let kind = int_field(pad, "type");
        if kind == pth {
            present = copper.clone();
        }
        for name in present {
            if copper.contains(&name) {
                match by_layer.iter_mut().find(|(l, _)| *l == name) {
                    Some((_, list)) => list.push(pad.clone()),
                    None => by_layer.push((name, vec![pad.clone()])),
                }
            }
        }
        // PadType.Name(pad.type) is evaluated (and refuses an unknown type) though unused here
        enum_name(&format!("{BT}PadType"), kind)?;
        problem.set_pad(
            &item_id(pad),
            PadInfo {
                net: text_at(pad, "net.name"),
                position: mm(&message_field(pad, "position")),
                layers: Vec::new(),
                number: str_field(pad, "number"),
                parent: text_at(pad, "parent.value"),
            },
        );
    }
    for (name, layer_pads) in &by_layer {
        let ids: Vec<Json> = layer_pads
            .iter()
            .map(|p| json!({"value": item_id(p)}))
            .collect();
        let request = board_request(
            "GetPadShapeAsPolygon",
            json!({"layer": layer(name)?, "pads": ids}),
            doc,
        );
        let response = client
            .send(&request, &format!("{BC}PadShapeAsPolygonResponse"))
            .await?;
        let polygons = messages(&response, "polygons");
        for (pid, polygon) in messages(&response, "pads").iter().zip(polygons.iter()) {
            let Some(geometry) = polygon_from_proto(polygon) else {
                continue;
            };
            let id = str_field(pid, "value");
            let Some(info) = problem.pad_mut(&id) else {
                continue;
            };
            match info.layers.iter_mut().find(|(l, _)| l == name) {
                Some((_, existing)) => *existing = geometry.clone(),
                None => info.layers.push((name.clone(), geometry.clone())),
            }
            let net = info.net.clone();
            problem.add_obstacle(
                name,
                Obstacle {
                    geometry,
                    net,
                    kind: "pad",
                    key: id,
                },
            );
        }
    }
    // NPTH holes have no copper but still need clearance: use the drill
    for pad in &pads {
        let drill = int_field(&sub(pad, "pad_stack.drill.diameter"), "x_nm");
        if int_field(pad, "type") == npth && drill != 0 {
            let hole = point_buffer(mm(&message_field(pad, "position")), drill as f64 / 2e6);
            for name in &copper {
                add(&mut problem, name, Some(hole.clone()), "", "hole");
            }
        }
    }

    // Tracks, arcs and vias
    for track in board_unpacked(client, doc, "tracks").await? {
        let line = [
            mm(&message_field(&track, "start")),
            mm(&message_field(&track, "end")),
        ];
        let width = int_field(&message_field(&track, "width"), "value_nm") as f64 / 2e6;
        let name = layer_name(int_field(&track, "layer"))?;
        add(
            &mut problem,
            &name,
            Some(line_buffer(&line, width)),
            &text_at(&track, "net.name"),
            "track",
        );
    }
    for arc in board_unpacked(client, doc, "arcs").await? {
        let line = arc_points(
            mm(&message_field(&arc, "start")),
            mm(&message_field(&arc, "mid")),
            mm(&message_field(&arc, "end")),
            24,
        );
        let width = int_field(&message_field(&arc, "width"), "value_nm") as f64 / 2e6;
        let name = layer_name(int_field(&arc, "layer"))?;
        add(
            &mut problem,
            &name,
            Some(line_buffer(&line, width)),
            &text_at(&arc, "net.name"),
            "track",
        );
    }
    let through = i64::from(known(&format!("{BT}ViaType"), "VT_THROUGH"));
    for via in board_unpacked(client, doc, "vias").await? {
        let stack = message_field(&via, "pad_stack");
        let diameter = messages(&stack, "copper_layers")
            .first()
            .map_or(600000, |c| int_field(&message_field(c, "size"), "x_nm"))
            as f64
            / 1e6;
        let mut span = copper.clone();
        let drill = message_field(&stack, "drill");
        let (start, end) = (
            int_field(&drill, "start_layer"),
            int_field(&drill, "end_layer"),
        );
        if int_field(&via, "type") != through && start != 0 && end != 0 {
            let mut ends = Vec::new();
            for value in [start, end] {
                if copper.contains(&layer_name(value)?) {
                    let name = layer_name(value)?;
                    ends.push(copper.iter().position(|c| *c == name).unwrap_or(0));
                }
            }
            ends.sort_unstable();
            if ends.len() == 2 {
                span = copper[ends[0]..=ends[1]].to_vec();
            }
        }
        let disc = point_buffer(mm(&message_field(&via, "position")), diameter / 2.0);
        let net = text_at(&via, "net.name");
        for name in &span {
            add(&mut problem, name, Some(disc.clone()), &net, "via");
        }
    }

    // Keepouts (board level and inside footprints, e.g. antenna modules) and footprint copper
    let mut zones = board_unpacked(client, doc, "zones").await?;
    let zone_type = format!("{BT}Zone");
    let shape_type = format!("{BT}BoardGraphicShape");
    for fp in &footprints {
        for packed in messages(&message_field(fp, "definition"), "items") {
            let type_name = proto::any_type_name(&packed);
            if type_name == zone_type {
                if let Some(zone) = proto::unpack(&packed, &zone_type) {
                    zones.push(zone);
                }
            } else if type_name == shape_type {
                if let Some(shape) = proto::unpack(&packed, &shape_type) {
                    let value = int_field(&shape, "layer");
                    if is_copper(value)? {
                        let name = layer_name(value)?;
                        add(
                            &mut problem,
                            &name,
                            shape_geometry(&shape),
                            &text_at(&shape, "net.name"),
                            "copper",
                        );
                    }
                }
            }
        }
    }
    let rule_area = i64::from(known(&format!("{BT}ZoneType"), "ZT_RULE_AREA"));
    for zone in &zones {
        if int_field(zone, "type") != rule_area {
            continue;
        }
        let Some(area) = polyset_geometry(&message_field(zone, "outline")) else {
            continue;
        };
        let settings = message_field(zone, "rule_area_settings");
        let mut names = Vec::new();
        for value in list_field(zone, "layers") {
            if let Value::EnumNumber(n) = value {
                if is_copper(i64::from(n))? {
                    names.push(layer_name(i64::from(n))?);
                }
            }
        }
        if names.is_empty() {
            names = copper.clone();
        }
        if int_field(&settings, "keepout_tracks") != 0 {
            for name in &names {
                problem.add_track_keepout(name, area.clone());
            }
        }
        if int_field(&settings, "keepout_vias") != 0 {
            problem.via_keepouts.push(area);
        }
    }

    // Net class clearances
    let nets = messages(
        &client
            .send(
                &board_request("GetNets", json!({}), doc),
                &format!("{BC}NetsResponse"),
            )
            .await?,
        "nets",
    );
    if !nets.is_empty() {
        let mut request = build(&format!("{BC}GetNetClassForNets"), json!({}));
        request.set_field_by_name(
            "net",
            Value::List(nets.iter().cloned().map(Value::Message).collect()),
        );
        let response = client
            .send(&request, &format!("{BC}NetClassForNetsResponse"))
            .await?;
        if let Some(Value::Map(classes)) = response.get_field_by_name("classes").as_deref() {
            for (name, netclass) in classes {
                if let (MapKey::String(name), Value::Message(netclass)) = (name, netclass) {
                    let settings = message_field(netclass, "board");
                    if has(netclass, "board") && has(&settings, "clearance") {
                        let value = int_field(&message_field(&settings, "clearance"), "value_nm");
                        problem.set_net_clearance(name, value as f64 / 1e6);
                    }
                }
            }
        }
    }
    Ok(problem)
}

// ---- Routing ------------------------------------------------------------------------------------

fn pad<'a>(problem: &'a RouteProblem, id: &str) -> Result<&'a PadInfo, Error> {
    problem.pad(id).ok_or_else(|| key_error(&json!(id)))
}

/// `_pad_layers(problem, pad_id)`.
fn pad_layers(problem: &RouteProblem, id: &str) -> Result<Vec<String>, Error> {
    let layers: Vec<String> = pad(problem, id)?
        .layers
        .iter()
        .map(|(l, _)| l.clone())
        .collect();
    Ok(if layers.is_empty() {
        problem.layers.iter().take(1).cloned().collect()
    } else {
        layers
    })
}

/// `_route_items(problem, net, start_id, end_id, width, layers, via, grid)`: the route and its
/// tracks and vias.
#[allow(clippy::too_many_arguments)]
fn route_items(
    problem: &RouteProblem,
    net: &str,
    start_id: &str,
    end_id: &str,
    width: f64,
    layers: Option<&[String]>,
    via: Option<(f64, f64)>,
    grid: f64,
) -> Result<(Route, Vec<DynamicMessage>), Error> {
    let start = pad(problem, start_id)?.position;
    let end = pad(problem, end_id)?.position;
    let mut own: Vec<(String, Vec<Shape>)> = Vec::new();
    for id in [start_id, end_id] {
        for (name, geometry) in &pad(problem, id)?.layers {
            match own.iter_mut().find(|(l, _)| l == name) {
                Some((_, list)) => list.push(geometry.clone()),
                None => own.push((name.clone(), vec![geometry.clone()])),
            }
        }
    }
    let mut start_layers = pad_layers(problem, start_id)?;
    let mut end_layers = pad_layers(problem, end_id)?;
    if let Some(allowed) = layers.filter(|l| !l.is_empty()) {
        let keep = |list: &[String]| -> Vec<String> {
            let kept: Vec<String> = list
                .iter()
                .filter(|n| allowed.contains(n))
                .cloned()
                .collect();
            if kept.is_empty() {
                list.to_vec()
            } else {
                kept
            }
        };
        start_layers = keep(&start_layers);
        end_layers = keep(&end_layers);
    }
    let own_keys = [start_id.to_owned(), end_id.to_owned()];
    let request = |grid: f64| RouteRequest {
        net,
        start,
        end,
        start_layers: &start_layers,
        end_layers: &end_layers,
        width,
        layers,
        via,
        grid,
        own_pads: &own,
        own_keys: &own_keys,
    };
    let found = match router::route(problem, &request(grid)) {
        Ok(found) => found,
        Err(RouteError::NoRoute(message)) => {
            if grid <= 0.03 {
                return Err(Error::Runtime(message));
            }
            router::route(problem, &request(grid / 2.0)).map_err(RouteError::into_error)?
        }
        Err(other) => return Err(other.into_error()),
    };
    let mut items = Vec::new();
    for (name, points) in &found.segments {
        let value = layer(name)?;
        for w in points.windows(2) {
            items.push(board::make_track(
                &json!(w[0]),
                &json!(w[1]),
                width,
                value,
                Some(net),
            )?);
        }
    }
    if let Some((diameter, drill)) = via {
        for position in &found.vias {
            items.push(board::make_via(
                &json!(position),
                Some(net),
                diameter,
                drill,
                "F.Cu",
                "B.Cu",
            )?);
        }
    }
    Ok((found, items))
}

/// `_net_sizes(doc, net, width, None, None)`: the track width and via (diameter, drill).
async fn net_sizes(
    client: &mut Client,
    doc: &DynamicMessage,
    net: &str,
    width: Option<f64>,
) -> Result<(f64, (f64, f64)), Error> {
    let sizes = if net.is_empty() {
        board::NetclassWidths::default()
    } else {
        board::netclass_widths(client, doc, net)
            .await?
            .unwrap_or_default()
    };
    let width = board::or_default(width.filter(|w| *w != 0.0).or(sizes.track_width), 0.25);
    let via = (
        board::or_default(sizes.via_diameter, 0.6),
        board::or_default(sizes.via_drill, 0.3),
    );
    Ok((width, via))
}

/// `_verify_route(doc, result)`: KiCad's DRC checks the new items; a route it rejects is removed
/// (only its own items, in a separate undoable step).
async fn verify_route(
    client: &mut Client,
    doc: &DynamicMessage,
    result: &mut Map<String, Json>,
    ids: &[String],
) -> Result<(), Error> {
    let report = match board::board_drc(client, doc, false, false, None).await {
        Ok(report) => report,
        Err(error) => {
            return Err(Error::Runtime(format!(
                "Route remains in the editor; DRC verification failed: {error}. \
                 Inspect route item UUIDs {} before continuing.",
                py_list_str(ids)
            )))
        }
    };
    let empty = Vec::new();
    let bad: Vec<&Json> = report["violations"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|v| {
            is_route_violation(v["type"].as_str().unwrap_or_default())
                && v["items"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|i| ids.iter().any(|id| i == id)))
        })
        .collect();
    if !bad.is_empty() {
        if let Err(cleanup) = client.delete_items(doc, ids).await {
            return Err(Error::Runtime(format!(
                "Route rejected by DRC; cleanup unconfirmed: {cleanup}. \
                 Inspect route item UUIDs {} before continuing.",
                py_list_str(ids)
            )));
        }
        let descriptions: Vec<&str> = bad
            .iter()
            .take(5)
            .map(|v| v["description"].as_str().unwrap_or_default())
            .collect();
        return Err(Error::Runtime(format!(
            "Route removed: KiCad's DRC found {} problem(s): {}",
            bad.len(),
            descriptions.join("; ")
        )));
    }
    result.insert("drc_clean".into(), json!(true));
    result.insert(
        "drc_scope".into(),
        json!("new route items: clearance, keepout and manufacturing"),
    );
    Ok(())
}

/// The options of [`board_route_avoiding`].
pub(crate) struct AvoidingRoute<'a> {
    pub from_ref: &'a str,
    pub from_pad: &'a str,
    pub to_ref: &'a str,
    pub to_pad: &'a str,
    pub layers: Option<&'a [String]>,
    pub width: Option<f64>,
    pub vias: bool,
    pub grid: f64,
    pub verify: bool,
}

/// `board_route_avoiding(...)`: a route between two pads around other nets' copper, keepouts and
/// the board edge, as one undo step; with verify, KiCad's DRC checks the new copper and the route is
/// removed if it breaks a rule.
pub(crate) async fn board_route_avoiding(
    client: &mut Client,
    doc: &DynamicMessage,
    options: &AvoidingRoute<'_>,
) -> Result<Json, Error> {
    let problem = route_problem(client, doc).await?;
    let start_id =
        item_id(&board::find_pad(client, doc, options.from_ref, options.from_pad).await?);
    let end_id = item_id(&board::find_pad(client, doc, options.to_ref, options.to_pad).await?);
    let a = pad(&problem, &start_id)?.net.clone();
    let b = pad(&problem, &end_id)?.net.clone();
    if !a.is_empty() && !b.is_empty() && a != b {
        return Err(Error::Value(format!(
            "{}.{} is on {a} but {}.{} is on {b}",
            options.from_ref, options.from_pad, options.to_ref, options.to_pad
        )));
    }
    let net = if a.is_empty() { b } else { a };
    let (width, via) = net_sizes(client, doc, &net, options.width).await?;
    let allowed: Option<Vec<String>> = match options.layers.filter(|l| !l.is_empty()) {
        Some(layers) => Some(layers.to_vec()),
        None if options.vias => Some(problem.layers.clone()),
        None => None,
    };
    let (found, items) = route_items(
        &problem,
        &net,
        &start_id,
        &end_id,
        width,
        allowed.as_deref(),
        options.vias.then_some(via),
        options.grid,
    )?;
    let description = format!(
        "MCP: route {}.{} to {}.{}",
        options.from_ref, options.from_pad, options.to_ref, options.to_pad
    );
    client
        .mutate(doc, &items, false, Some(&description))
        .await?;
    let ids: Vec<String> = items.iter().map(item_id).collect();
    let mut result = Map::new();
    result.insert("net".into(), json!(net));
    result.insert("width_mm".into(), json!(width));
    result.insert("length_mm".into(), json!(found.length));
    result.insert("vias".into(), json!(found.vias));
    result.insert(
        "segments".into(),
        Json::Array(
            found
                .segments
                .iter()
                .map(|(l, pts)| json!({"layer": l, "path_mm": pts}))
                .collect(),
        ),
    );
    result.insert("items".into(), json!(ids));
    result.insert("geometry_checked".into(), json!(found.checked));
    if options.verify {
        verify_route(client, doc, &mut result, &ids).await?;
    }
    Ok(Json::Object(result))
}

/// The footprint references `_pad_label` keeps between calls (`self._ref_cache`, on the one
/// client of the server).
static REF_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// `_pad_label(doc, problem, pad_id)`: "U1.3"; the reference cache is read again only when it does
/// not know the pad's footprint.
async fn pad_label(
    client: &mut Client,
    doc: &DynamicMessage,
    problem: &RouteProblem,
    id: &str,
) -> Result<String, Error> {
    let info = pad(problem, id)?;
    let known_parent = REF_CACHE
        .lock()
        .map(|cache| cache.as_ref().is_some_and(|c| c.contains_key(&info.parent)))
        .unwrap_or(false);
    if !known_parent {
        let fresh: HashMap<String, String> = board_unpacked(client, doc, "footprints")
            .await?
            .iter()
            .map(|fp| (item_id(fp), text_at(fp, "reference_field.text.text.text")))
            .collect();
        if let Ok(mut cache) = REF_CACHE.lock() {
            *cache = Some(fresh);
        }
    }
    let reference = REF_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.as_ref().and_then(|c| c.get(&info.parent).cloned()))
        .unwrap_or_else(|| "?".into());
    Ok(format!("{reference}.{}", info.number))
}

/// The options of [`board_route_net`].
pub(crate) struct NetRoute<'a> {
    pub net: &'a str,
    pub layers: Option<&'a [String]>,
    pub width: Option<f64>,
    pub vias: bool,
    pub grid: f64,
    pub verify: bool,
}

/// `board_route_net(...)`: connects every pad of a net that is not yet connected (shortest joins
/// first), as one undo step.
pub(crate) async fn board_route_net(
    client: &mut Client,
    doc: &DynamicMessage,
    options: &NetRoute<'_>,
) -> Result<Json, Error> {
    let net = options.net;
    let problem = route_problem(client, doc).await?;
    let net_pads: Vec<String> = problem
        .pads
        .iter()
        .filter(|(_, info)| info.net == net)
        .map(|(id, _)| id.clone())
        .collect();
    if net_pads.len() < 2 {
        return Err(Error::Value(format!(
            "net {} has {} pad(s) on the board",
            py_repr(&json!(net)),
            net_pads.len()
        )));
    }
    let mut clusters: Vec<Vec<router::ClusterPoint>> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for id in &net_pads {
        if seen.contains(id) {
            continue;
        }
        let request = with_field(
            build(
                &format!("{BC}GetConnectedItems"),
                json!({"types": ["KOT_PCB_PAD"], "items": [{"value": id}]}),
            ),
            "header",
            item_header(doc),
        );
        let response = client
            .send(&request, &format!("{C}GetItemsResponse"))
            .await?;
        let mut connected = vec![id.clone()];
        for packed in messages(&response, "items") {
            if let Some(found) = proto::unpack(&packed, &format!("{BT}Pad")) {
                let found = item_id(&found);
                if problem.pad(&found).is_some() && !connected.contains(&found) {
                    connected.push(found);
                }
            }
        }
        connected.retain(|p| net_pads.contains(p));
        connected.sort();
        for p in &connected {
            if !seen.contains(p) {
                seen.push(p.clone());
            }
        }
        clusters.push(
            connected
                .iter()
                .map(|p| {
                    let position = problem.pad(p).map_or([0.0, 0.0], |i| i.position);
                    (position[0], position[1], p.clone())
                })
                .collect(),
        );
    }
    if clusters.len() < 2 {
        return Ok(json!({"net": net, "already_connected": true, "items": []}));
    }
    let (width, via) = net_sizes(client, doc, net, options.width).await?;
    let allowed: Option<Vec<String>> = match options.layers.filter(|l| !l.is_empty()) {
        Some(layers) => Some(layers.to_vec()),
        None if options.vias => Some(problem.layers.clone()),
        None => None,
    };
    let mut legs = Vec::new();
    let mut items = Vec::new();
    for (p, q) in router::spanning_order(&clusters) {
        let (found, leg_items) = route_items(
            &problem,
            net,
            &p.2,
            &q.2,
            width,
            allowed.as_deref(),
            options.vias.then_some(via),
            options.grid,
        )?;
        items.extend(leg_items);
        let from = pad_label(client, doc, &problem, &p.2).await?;
        let to = pad_label(client, doc, &problem, &q.2).await?;
        legs.push(json!({"from": from, "to": to, "length_mm": found.length,
                         "vias": found.vias.len()}));
    }
    if items.len() > 200 {
        return Err(Error::Value(
            "this net needs more than 200 items; route it in parts".into(),
        ));
    }
    client
        .mutate(doc, &items, false, Some(&format!("MCP: route net {net}")))
        .await?;
    let ids: Vec<String> = items.iter().map(item_id).collect();
    let mut result = Map::new();
    result.insert("net".into(), json!(net));
    result.insert("width_mm".into(), json!(width));
    result.insert("legs".into(), Json::Array(legs));
    result.insert("items".into(), json!(ids));
    if options.verify {
        verify_route(client, doc, &mut result, &ids).await?;
    }
    Ok(Json::Object(result))
}

/// shapely's `LineString(points)` checks: numpy's array shape, the coordinate dimension, and GEOS's
/// refusal of a single point.  Returns the 2-D points.
fn line_points(points: &[Vec<f64>]) -> Result<Vec<[f64; 2]>, Error> {
    if points.is_empty() {
        return Ok(Vec::new());
    }
    let dimension = points[0].len();
    if points.iter().any(|p| p.len() != dimension) {
        return Err(Error::Value(format!(
            "setting an array element with a sequence. The requested array has an inhomogeneous \
             shape after 1 dimensions. The detected shape was ({},) + inhomogeneous part.",
            points.len()
        )));
    }
    if dimension != 2 && dimension != 3 {
        return Err(Error::Value(format!(
            "The ordinate (last) dimension should be 2 or 3, got {dimension}"
        )));
    }
    if points.len() == 1 {
        return Err(Error::Value(
            "IllegalArgumentException: point array must contain 0 or >1 elements\n".into(),
        ));
    }
    Ok(points.iter().map(|p| [p[0], p[1]]).collect())
}

/// `board_clear_path(doc, net, points, layer, width)`: does a proposed track (points in mm) keep
/// clear of other nets, keepouts and the board edge?
pub(crate) async fn board_clear_path(
    client: &mut Client,
    doc: &DynamicMessage,
    net: &str,
    points: &[Vec<f64>],
    layer_value_name: &str,
    width: f64,
) -> Result<Json, Error> {
    let problem = route_problem(client, doc).await?;
    let points = line_points(points)?;
    // An empty line (no points, or a width of 0 or less) is empty: its distances are NaN
    let line = if points.is_empty() || width / 2.0 <= 0.0 || width.is_nan() {
        None
    } else {
        Some(line_buffer(&points, width / 2.0))
    };
    let line = line.filter(|l| !l.is_empty());
    let mut hits: Vec<Json> = Vec::new();
    if let Some(line) = &line {
        for obstacle in problem.obstacles_on(layer_value_name) {
            if !obstacle.net.is_empty() && obstacle.net == net {
                continue;
            }
            if obstacle.geometry.is_empty() {
                continue; // distance to an empty geometry is NaN
            }
            let gap = Euclidean.distance(&line.geometry, &obstacle.geometry.geometry);
            let need = problem.clearance(net, &obstacle.net);
            if gap < need - 1e-6 {
                hits.push(json!({"kind": obstacle.kind, "net": obstacle.net,
                                 "gap_mm": py_round(gap, 4), "needed_mm": need}));
            }
        }
        for keepout in problem.track_keepouts_on(layer_value_name) {
            if line.geometry.intersects(&keepout.geometry) {
                hits.push(json!({"kind": "keepout"}));
            }
        }
    }
    if let Some(outline) = &problem.outline {
        if !outline.single {
            return Err(Error::Value(
                "'MultiPolygon' object has no attribute 'exterior'".into(),
            ));
        }
        let edge = match (&line, outline.area.0.first()) {
            (Some(line), Some(area)) => {
                let exterior = Geometry::LineString(area.exterior().clone());
                Euclidean.distance(&line.geometry, &exterior) < problem.edge_clearance - 1e-6
                    || !Geometry::MultiPolygon(outline.area.clone())
                        .relate(&line.geometry)
                        .is_contains()
            }
            // contains(empty) is False
            _ => true,
        };
        if edge {
            hits.push(json!({"kind": "board edge", "needed_mm": problem.edge_clearance}));
        }
    }
    let clear = hits.is_empty();
    hits.truncate(50);
    Ok(json!({"clear": clear, "conflicts": hits}))
}

// ---- Checks -------------------------------------------------------------------------------------

/// `board_check(doc, categories, parity)`: DRC grouped by category; `categories` limits the report
/// (None reports them all).
pub(crate) async fn board_check(
    client: &mut Client,
    doc: &DynamicMessage,
    categories: Option<&[String]>,
    parity: bool,
) -> Result<Json, Error> {
    let wanted: Vec<String> = match categories.filter(|c| !c.is_empty()) {
        Some(list) => list.to_vec(),
        None => CHECK_CATEGORIES
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .collect(),
    };
    let unknown: Vec<String> = wanted
        .iter()
        .filter(|c| category_of_name(c).is_none())
        .cloned()
        .collect();
    if !unknown.is_empty() {
        let mut names: Vec<String> = CHECK_CATEGORIES
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .collect();
        names.sort();
        return Err(Error::Value(format!(
            "unknown categories {}; use {}",
            py_list_str(&unknown),
            py_list_str(&names)
        )));
    }
    let schematic = categories.is_some_and(|c| c.iter().any(|n| n == "schematic"));
    let report = board::board_drc(client, doc, parity || schematic, true, Some(200)).await?;
    let mut grouped: Vec<(String, Vec<Json>)> = Vec::new();
    for c in &wanted {
        if !grouped.iter().any(|(n, _)| n == c) {
            grouped.push((c.clone(), Vec::new()));
        }
    }
    let mut other = Vec::new();
    for violation in report["violations"].as_array().cloned().unwrap_or_default() {
        match category_of(violation["type"].as_str().unwrap_or_default()) {
            Some(category) => {
                if let Some((_, list)) = grouped.iter_mut().find(|(n, _)| n == category) {
                    list.push(violation);
                }
            }
            None => other.push(violation),
        }
    }
    let mut counts = Map::new();
    let mut violations = Map::new();
    for (name, list) in &grouped {
        counts.insert(name.clone(), json!(list.len()));
        if !list.is_empty() {
            violations.insert(
                name.clone(),
                Json::Array(list.iter().take(50).cloned().collect()),
            );
        }
    }
    let mut result = Map::new();
    result.insert(
        "clean".into(),
        json!(grouped.iter().all(|(_, l)| l.is_empty())),
    );
    result.insert("counts".into(), Json::Object(counts));
    result.insert("violations".into(), Json::Object(violations));
    if !other.is_empty() && categories.is_none() {
        result.insert(
            "other".into(),
            Json::Array(other.into_iter().take(20).collect()),
        );
    }
    Ok(Json::Object(result))
}

fn category_of_name(name: &str) -> Option<&'static str> {
    CHECK_CATEGORIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, _)| *n)
}

// ---- Design rules -------------------------------------------------------------------------------

/// The changes of `board_rules`: minimums by tool argument (mm), predefined sizes and severities.
pub(crate) struct RuleChanges<'a> {
    pub minimums: Vec<(&'static str, Option<f64>)>,
    pub track_widths: Option<Vec<f64>>,
    pub via_sizes: Option<Vec<Vec<f64>>>,
    pub severities: Option<&'a Map<String, Json>>,
}

/// `board_rules(doc, track_widths, via_sizes, severities, **minimums)`: the built-in design rules,
/// changed where asked.
pub(crate) async fn board_rules(
    client: &mut Client,
    doc: &DynamicMessage,
    changes: &RuleChanges<'_>,
) -> Result<Json, Error> {
    let response_type = format!("{BC}BoardDesignRulesResponse");
    let mut response = client
        .send(
            &board_request("GetBoardDesignRules", json!({}), doc),
            &response_type,
        )
        .await?;
    let mut rules = message_field(&response, "rules");
    let mut changed = false;
    for (argument, value) in &changes.minimums {
        let Some(value) = value else { continue };
        let field = RULE_FIELDS
            .iter()
            .find(|(_, a)| a == argument)
            .map(|(f, _)| *f)
            .expect("every argument is a rule field");
        let value_nm = nm(*value)?;
        set_distance(&mut rules, &format!("constraints.{field}"), value_nm);
        changed = true;
    }
    if let Some(widths) = &changes.track_widths {
        let mut tracks = Vec::new();
        for w in widths {
            tracks.push(Value::Message(build(
                "kiapi.board.PresetTrackWidth",
                json!({"width": {"value_nm": nm(*w)?}}),
            )));
        }
        set(
            child(&mut rules, "predefined_sizes"),
            "tracks",
            Value::List(tracks),
        );
        changed = true;
    }
    if let Some(sizes) = &changes.via_sizes {
        // `del vias[:]` happens first; a failure below leaves nothing sent
        let mut vias = Vec::new();
        for size in sizes {
            let (diameter, drill) = match size.as_slice() {
                [d, r] => (*d, *r),
                [_, _, ..] => {
                    return Err(Error::Value(
                        "too many values to unpack (expected 2)".into(),
                    ))
                }
                short => {
                    return Err(Error::Value(format!(
                        "not enough values to unpack (expected 2, got {})",
                        short.len()
                    )))
                }
            };
            if drill >= diameter {
                return Err(Error::Value(
                    "a via's drill must be smaller than its diameter".into(),
                ));
            }
            let (d, r) = (nm(diameter)?, nm(drill)?);
            vias.push(Value::Message(build(
                "kiapi.board.PresetViaDimension",
                json!({"diameter": {"value_nm": d}, "drill": {"value_nm": r}}),
            )));
        }
        set(
            child(&mut rules, "predefined_sizes"),
            "vias",
            Value::List(vias),
        );
        changed = true;
    }
    if let Some(severities) = changes.severities.filter(|s| !s.is_empty()) {
        let current = messages(&rules, "severities");
        let mut updated = current.clone();
        let mut added = Vec::new();
        for (name, level) in severities {
            let key = format!("DRCET_{}", name.to_uppercase());
            let Ok(value) = enum_value("kiapi.board.DrcErrorType", &key) else {
                return Err(Error::Value(format!("unknown DRC check {name}")));
            };
            let level = match level {
                Json::Array(_) | Json::Object(_) => {
                    return Err(Error::Value(format!(
                        "unhashable type: '{}'",
                        type_name(level)
                    )))
                }
                Json::String(text) => severity(text),
                _ => None,
            };
            let Some(level) = level else {
                return Err(Error::Value(
                    "severity must be error, warning or ignore".into(),
                ));
            };
            // `current` is the list as it was: a second name for the same check adds another entry
            // {s.rule_type: s}: the last entry of a type is the one changed
            match current
                .iter()
                .rposition(|s| int_field(s, "rule_type") == i64::from(value))
            {
                Some(index) => set(&mut updated[index], "severity", Value::EnumNumber(level)),
                None => added.push(build(
                    "kiapi.board.DrcSeveritySetting",
                    json!({"rule_type": value, "severity": level}),
                )),
            }
        }
        updated.extend(added);
        set(
            &mut rules,
            "severities",
            Value::List(updated.into_iter().map(Value::Message).collect()),
        );
        changed = true;
    }
    if changed {
        let request = with_field(
            board_request("SetBoardDesignRules", json!({}), doc),
            "rules",
            rules,
        );
        response = client.send(&request, &response_type).await?;
    }
    rules_summary(&message_field(&response, "rules"))
}

/// `rules_summary(rules)`.
fn rules_summary(rules: &DynamicMessage) -> Result<Json, Error> {
    let constraints = message_field(rules, "constraints");
    let mut out = Map::new();
    for (field, argument) in RULE_FIELDS {
        let value = int_field(&message_field(&constraints, field), "value_nm") as f64 / 1e6;
        out.insert((*argument).into(), json!(py_round(value, 4)));
    }
    let sizes = message_field(rules, "predefined_sizes");
    let distance = |m: &DynamicMessage, name: &str| {
        int_field(&message_field(m, name), "value_nm") as f64 / 1e6
    };
    out.insert(
        "track_widths_mm".into(),
        Json::Array(
            messages(&sizes, "tracks")
                .iter()
                .map(|t| json!(py_round(distance(t, "width"), 4)))
                .collect(),
        ),
    );
    out.insert(
        "via_sizes_mm".into(),
        Json::Array(
            messages(&sizes, "vias")
                .iter()
                .map(|v| {
                    json!([
                        py_round(distance(v, "diameter"), 4),
                        py_round(distance(v, "drill"), 4)
                    ])
                })
                .collect(),
        ),
    );
    let mut severities = Map::new();
    for s in messages(rules, "severities") {
        let kind = enum_name("kiapi.board.DrcErrorType", int_field(&s, "rule_type"))?;
        let level = enum_name(&format!("{T}RuleSeverity"), int_field(&s, "severity"))?;
        severities.insert(
            kind.strip_prefix("DRCET_").unwrap_or(&kind).to_lowercase(),
            json!(level.strip_prefix("RS_").unwrap_or(&level).to_lowercase()),
        );
    }
    out.insert("severities".into(), Json::Object(severities));
    let mask = message_field(rules, "solder_mask_paste");
    out.insert(
        "soldermask".into(),
        json!({"expansion_mm": distance(&mask, "mask_expansion"),
               "min_web_mm": distance(&mask, "mask_min_width")}),
    );
    Ok(Json::Object(out))
}

/// `custom_rule(spec)`: a CustomRule from `{name, condition, constraints: [{type, min, opt, max
/// (mm) | items | expression | style}], layer, severity, comments}`, or protobuf JSON (with
/// constraints whose type starts with CRCT_).
fn custom_rule(spec: &Json) -> Result<DynamicMessage, Error> {
    let descriptor = "kiapi.board.CustomRule";
    let constraints = match get(spec, "constraints")? {
        Some(value) => py_iter(value)?,
        None => Vec::new(),
    };
    // any(str(c.get("type", "")).startswith("CRCT_") for c in constraints) stops at the first
    for c in &constraints {
        if get(c, "type")?
            .and_then(Json::as_str)
            .is_some_and(|t| t.starts_with("CRCT_"))
        {
            return Ok(pbjson::from_json(spec, &proto::descriptor(descriptor))?);
        }
    }
    let name = spec.get("name").ok_or_else(|| key_error(&json!("name")))?;
    let name = string_value(name)?;
    let condition = string_value(spec.get("condition").unwrap_or(&json!("")))?;
    let mut rule = build(descriptor, json!({}));
    if let Some(name) = name {
        set(&mut rule, "name", Value::String(name));
    }
    if let Some(condition) = condition {
        set(&mut rule, "condition", Value::String(condition));
    }
    if let Some(comments) = spec.get("comments").filter(|c| truthy(c)) {
        match string_value(comments)? {
            Some(text) => set(&mut rule, "comments", Value::String(text)),
            None => unreachable!("a truthy value is not None"),
        }
    }
    match spec.get("layer") {
        Some(Json::String(mode)) if mode == "outer" || mode == "inner" => {
            let name = if mode == "outer" {
                "CRLM_OUTER"
            } else {
                "CRLM_INNER"
            };
            set(
                &mut rule,
                "layer_mode",
                Value::EnumNumber(known("kiapi.board.CustomRuleLayerMode", name)),
            );
        }
        Some(value) if truthy(value) => {
            set(
                &mut rule,
                "single_layer",
                Value::EnumNumber(layer_of(value)?),
            );
        }
        _ => {}
    }
    if let Some(level) = spec.get("severity").filter(|s| truthy(s)) {
        set(
            &mut rule,
            "severity",
            Value::EnumNumber(severity_of(level)?),
        );
    }
    let mut built = Vec::new();
    for c in &constraints {
        let kind_value = c.get("type").ok_or_else(|| key_error(&json!("type")))?;
        let key = format!("CRCT_{}", upper(kind_value)?);
        let Ok(kind) = enum_value("kiapi.board.CustomRuleConstraintType", &key) else {
            return Err(Error::Value(format!(
                "unknown constraint {}",
                kind_value.as_str().unwrap_or_default()
            )));
        };
        let mut constraint = build("kiapi.board.CustomRuleConstraint", json!({"type": kind}));
        let kind_text = kind_value.as_str().unwrap_or_default();
        if kind_text == "disallow" {
            let items = match c.get("items") {
                Some(items) => py_iter(items)?,
                None => Vec::new(),
            };
            let mut types = Vec::new();
            for item in &items {
                let name = format!("CRDT_{}", upper(item)?);
                types.push(Value::EnumNumber(enum_value(
                    "kiapi.board.CustomRuleDisallowType",
                    &name,
                )?));
            }
            if !types.is_empty() {
                set(
                    child(&mut constraint, "disallow"),
                    "types",
                    Value::List(types),
                );
            }
        } else if kind_text == "assertion" {
            let expression = c
                .get("expression")
                .ok_or_else(|| key_error(&json!("expression")))?;
            match expression {
                Json::String(text) => set(
                    &mut constraint,
                    "assertion_expression",
                    Value::String(text.clone()),
                ),
                _ => {
                    return Err(Error::Value(
                        "bad argument type for built-in operation".into(),
                    ))
                }
            }
        } else if kind_text == "zone_connection" {
            let style = c.get("style").ok_or_else(|| key_error(&json!("style")))?;
            let name = match style {
                Json::String(s) if s == "solid" => "ZCS_FULL",
                Json::String(s) if s == "thermal" => "ZCS_THERMAL",
                Json::String(s) if s == "none" => "ZCS_NONE",
                Json::Array(_) | Json::Object(_) => {
                    return Err(Error::Value(format!(
                        "unhashable type: '{}'",
                        type_name(style)
                    )))
                }
                other => return Err(key_error(other)),
            };
            set(
                &mut constraint,
                "zone_connection",
                Value::EnumNumber(known(&format!("{BT}ZoneConnectionStyle"), name)),
            );
        } else {
            for bound in ["min", "opt", "max"] {
                if let Some(value) = c.get(bound).filter(|v| !v.is_null()) {
                    let value_nm = nm_of(value)?;
                    let value_nm = i32::try_from(value_nm)
                        .map_err(|_| Error::Value(format!("Value out of range: {value_nm}")))?;
                    set(
                        child(&mut constraint, "numeric"),
                        bound,
                        Value::I32(value_nm),
                    );
                }
            }
        }
        built.push(Value::Message(constraint));
    }
    if !built.is_empty() {
        set(&mut rule, "constraints", Value::List(built));
    }
    Ok(rule)
}

/// `custom_rules_summary(response)`.
fn custom_rules_summary(response: &DynamicMessage) -> Result<Json, Error> {
    let mut rules = Vec::new();
    for rule in messages(response, "rules") {
        let mut entry = Map::new();
        entry.insert("name".into(), json!(str_field(&rule, "name")));
        entry.insert("condition".into(), json!(str_field(&rule, "condition")));
        let mut constraints = Vec::new();
        for c in messages(&rule, "constraints") {
            let kind = enum_name(
                "kiapi.board.CustomRuleConstraintType",
                int_field(&c, "type"),
            )?;
            let mut item = Map::new();
            item.insert(
                "type".into(),
                json!(kind.strip_prefix("CRCT_").unwrap_or(&kind).to_lowercase()),
            );
            if has(&c, "numeric") {
                let numeric = message_field(&c, "numeric");
                for bound in ["min", "opt", "max"] {
                    if has(&numeric, bound) {
                        item.insert(bound.into(), json!(int_field(&numeric, bound) as f64 / 1e6));
                    }
                }
            } else if has(&c, "disallow") {
                let mut names = Vec::new();
                for value in list_field(&message_field(&c, "disallow"), "types") {
                    if let Value::EnumNumber(n) = value {
                        let name = enum_name("kiapi.board.CustomRuleDisallowType", i64::from(n))?;
                        names.push(json!(name
                            .strip_prefix("CRDT_")
                            .unwrap_or(&name)
                            .to_lowercase()));
                    }
                }
                item.insert("items".into(), Json::Array(names));
            } else {
                let expression = str_field(&c, "assertion_expression");
                if !expression.is_empty() {
                    item.insert("expression".into(), json!(expression));
                }
            }
            constraints.push(Json::Object(item));
        }
        entry.insert("constraints".into(), Json::Array(constraints));
        if has(&rule, "layer_mode") {
            let mode = enum_name(
                "kiapi.board.CustomRuleLayerMode",
                int_field(&rule, "layer_mode"),
            )?;
            entry.insert(
                "layer".into(),
                json!(mode.strip_prefix("CRLM_").unwrap_or(&mode).to_lowercase()),
            );
        } else if has(&rule, "single_layer") {
            entry.insert(
                "layer".into(),
                json!(layer_name(int_field(&rule, "single_layer"))?),
            );
        }
        let severity = int_field(&rule, "severity");
        if severity != 0 {
            let name = enum_name(&format!("{T}RuleSeverity"), severity)?;
            entry.insert(
                "severity".into(),
                json!(name.strip_prefix("RS_").unwrap_or(&name).to_lowercase()),
            );
        }
        rules.push(Json::Object(entry));
    }
    let status = enum_name(
        "kiapi.board.commands.CustomRulesStatus",
        int_field(response, "status"),
    )?;
    let mut out = Map::new();
    out.insert(
        "status".into(),
        json!(status
            .strip_prefix("CRS_")
            .unwrap_or(&status)
            .to_lowercase()),
    );
    out.insert("rules".into(), Json::Array(rules));
    let error = str_field(response, "error_text");
    if !error.is_empty() {
        out.insert("error".into(), json!(error));
    }
    Ok(Json::Object(out))
}

/// `board_custom_rules(doc, action, rules, names)`: get, add (replaces rules of the same name),
/// remove (names) or set (all) the custom (.kicad_dru) rules.
pub(crate) async fn board_custom_rules(
    client: &mut Client,
    doc: &DynamicMessage,
    action: &str,
    rules: Option<&[Json]>,
    names: Option<&[String]>,
) -> Result<Json, Error> {
    let response_type = format!("{BC}CustomRulesResponse");
    let current = client
        .send(
            &board_request("GetCustomDesignRules", json!({}), doc),
            &response_type,
        )
        .await?;
    if action == "get" {
        return custom_rules_summary(&current);
    }
    let existing = messages(&current, "rules");
    let final_rules: Vec<DynamicMessage> = match action {
        "remove" => {
            let wanted: Vec<String> = names.unwrap_or_default().to_vec();
            let mut missing: Vec<String> = wanted
                .iter()
                .filter(|n| !existing.iter().any(|r| str_field(r, "name") == **n))
                .cloned()
                .collect();
            missing.sort();
            missing.dedup();
            if !missing.is_empty() {
                return Err(Error::Value(format!(
                    "no custom rules named {}",
                    py_list_str(&missing)
                )));
            }
            existing
                .into_iter()
                .filter(|r| !wanted.contains(&str_field(r, "name")))
                .collect()
        }
        "add" | "set" => {
            let new = rules
                .unwrap_or_default()
                .iter()
                .map(custom_rule)
                .collect::<Result<Vec<_>, _>>()?;
            if action == "add" {
                let replaced: Vec<String> = new.iter().map(|r| str_field(r, "name")).collect();
                existing
                    .into_iter()
                    .filter(|r| !replaced.contains(&str_field(r, "name")))
                    .chain(new)
                    .collect()
            } else {
                new
            }
        }
        _ => {
            return Err(Error::Value(
                "action must be get, add, remove or set".into(),
            ))
        }
    };
    let mut request = board_request("SetCustomDesignRules", json!({}), doc);
    request.set_field_by_name(
        "rules",
        Value::List(final_rules.into_iter().map(Value::Message).collect()),
    );
    let response = client.send(&request, &response_type).await?;
    if int_field(&response, "status")
        == i64::from(known(
            "kiapi.board.commands.CustomRulesStatus",
            "CRS_INVALID",
        ))
    {
        return Err(Error::Runtime(format!(
            "KiCad rejected the custom rules: {}",
            str_field(&response, "error_text")
        )));
    }
    custom_rules_summary(&response)
}

// ---- Net classes --------------------------------------------------------------------------------

/// `netclass_summary(netclass)`.
fn netclass_summary(netclass: &DynamicMessage) -> Json {
    let b = message_field(netclass, "board");
    let mut out = Map::new();
    out.insert("name".into(), json!(str_field(netclass, "name")));
    for (key, attr) in [
        ("clearance_mm", "clearance"),
        ("track_width_mm", "track_width"),
        ("diff_pair_width_mm", "diff_pair_track_width"),
        ("diff_pair_gap_mm", "diff_pair_gap"),
    ] {
        if has(&b, attr) {
            let value = int_field(&message_field(&b, attr), "value_nm") as f64 / 1e6;
            out.insert(key.into(), json!(py_round(value, 4)));
        }
    }
    if has(&b, "via_stack") {
        let stack = message_field(&b, "via_stack");
        if let Some(copper) = messages(&stack, "copper_layers").first() {
            let size = int_field(&message_field(copper, "size"), "x_nm") as f64 / 1e6;
            out.insert("via_diameter_mm".into(), json!(py_round(size, 4)));
        }
        let drill = int_field(&sub(&stack, "drill.diameter"), "x_nm");
        if drill != 0 {
            out.insert(
                "via_drill_mm".into(),
                json!(py_round(drill as f64 / 1e6, 4)),
            );
        }
    }
    if has(netclass, "priority") {
        out.insert("priority".into(), json!(int_field(netclass, "priority")));
    }
    Json::Object(out)
}

/// `board_netclasses(doc, classes, assign, patterns)`: creates or changes net classes, assigns
/// nets, then returns every class and assignment.
pub(crate) async fn board_netclasses(
    client: &mut Client,
    doc: &DynamicMessage,
    classes: Option<&[Json]>,
    assign: Option<&Map<String, Json>>,
    patterns: Option<&Map<String, Json>>,
) -> Result<Json, Error> {
    // The net class commands match the project path with its trailing separator; board listings
    // omit it.  KiCad uses the native separator: "\" for a Windows path, "/" otherwise
    let mut project = message_field(doc, "project");
    let path = str_field(&project, "path");
    if !path.is_empty() && !path.ends_with(['/', '\\']) {
        let windows = path.contains('\\') && !path.contains('/');
        set(
            &mut project,
            "path",
            Value::String(format!("{path}{}", if windows { "\\" } else { "/" })),
        );
    }
    let with_project = |name: &str, value: Json| {
        with_field(
            build(&format!("{C}{name}"), value),
            "project",
            project.clone(),
        )
    };
    let classes_type = format!("{C}NetClassesResponse");
    if let Some(classes) = classes.filter(|c| !c.is_empty()) {
        let listing = client
            .send(&with_project("GetNetClasses", json!({})), &classes_type)
            .await?;
        let existing = messages(&listing, "net_classes");
        let mut changed = Vec::new();
        for spec in classes {
            let name = spec.get("name").ok_or_else(|| key_error(&json!("name")))?;
            // NetClass(name=name) is built first, then the name is looked up
            let name_text = string_value(name)?;
            let lookup = name_text.clone().unwrap_or_default();
            let mut netclass = match existing
                .iter()
                .rev()
                .find(|c| str_field(c, "name") == lookup)
            {
                Some(found) if name_text.is_some() => found.clone(),
                _ => {
                    let mut fresh = build("kiapi.common.project.NetClass", json!({}));
                    if let Some(text) = &name_text {
                        set(&mut fresh, "name", Value::String(text.clone()));
                    }
                    fresh
                }
            };
            for (key, attr) in [
                ("clearance", "clearance"),
                ("track_width", "track_width"),
                ("diff_pair_width", "diff_pair_track_width"),
                ("diff_pair_gap", "diff_pair_gap"),
                ("diff_pair_via_gap", "diff_pair_via_gap"),
            ] {
                if let Some(value) = spec.get(key).filter(|v| !v.is_null()) {
                    let value_nm = nm_of(value)?;
                    set_distance(&mut netclass, &format!("board.{attr}"), value_nm);
                }
            }
            let diameter = spec.get("via_diameter").filter(|v| !v.is_null());
            let drill = spec.get("via_drill").filter(|v| !v.is_null());
            if diameter.is_some() || drill.is_some() {
                let stack = child(child(&mut netclass, "board"), "via_stack");
                if let Some(d) = diameter {
                    let size = board::vec2(real(d)?, real(d)?)?;
                    let mut copper = messages(stack, "copper_layers");
                    if copper.is_empty() {
                        copper.push(build(
                            &format!("{BT}PadStackLayer"),
                            json!({"layer": "BL_F_Cu", "shape": "PSS_CIRCLE"}),
                        ));
                    }
                    set(
                        &mut copper[0],
                        "size",
                        Value::Message(build(&format!("{T}Vector2"), size)),
                    );
                    set(
                        stack,
                        "copper_layers",
                        Value::List(copper.into_iter().map(Value::Message).collect()),
                    );
                }
                if let Some(d) = drill {
                    let size = board::vec2(real(d)?, real(d)?)?;
                    set(
                        child(stack, "drill"),
                        "diameter",
                        Value::Message(build(&format!("{T}Vector2"), size)),
                    );
                }
            }
            if let Some(priority) = spec.get("priority").filter(|v| !v.is_null()) {
                let value = int32_value(priority, "kiapi.common.project.NetClass.priority")?;
                set(&mut netclass, "priority", Value::I32(value));
            }
            changed.push(Value::Message(netclass));
        }
        let mut request = with_project("SetNetClasses", json!({"merge_mode": "MMM_MERGE"}));
        request.set_field_by_name("net_classes", Value::List(changed));
        client.send(&request, EMPTY).await?;
    }
    let assign = assign.filter(|a| !a.is_empty());
    let patterns = patterns.filter(|p| !p.is_empty());
    if assign.is_some() || patterns.is_some() {
        let mut assignments = Vec::new();
        for (net, name) in assign.into_iter().flatten() {
            match name {
                Json::String(name) => assignments.push(json!({"net": net, "netclasses": [name]})),
                _ => {
                    return Err(Error::Value(
                        "bad argument type for built-in operation".into(),
                    ))
                }
            }
        }
        let mut pattern_assignments = Vec::new();
        for (pattern, name) in patterns.into_iter().flatten() {
            let mut entry = json!({"pattern": pattern});
            if let Some(name) = string_value(name)? {
                entry["netclass"] = json!(name);
            }
            pattern_assignments.push(entry);
        }
        let request = with_project(
            "SetNetClassAssignments",
            json!({"merge_mode": "MMM_MERGE", "assignments": assignments,
                   "pattern_assignments": pattern_assignments}),
        );
        client.send(&request, EMPTY).await?;
    }
    let listing = client
        .send(&with_project("GetNetClasses", json!({})), &classes_type)
        .await?;
    let assignments = client
        .send(
            &with_project("GetNetClassAssignments", json!({})),
            &format!("{C}NetClassAssignmentsResponse"),
        )
        .await?;
    let mut by_net = Map::new();
    for a in messages(&assignments, "assignments") {
        let names: Vec<Json> = list_field(&a, "netclasses")
            .into_iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(json!(s)),
                _ => None,
            })
            .collect();
        by_net.insert(str_field(&a, "net"), Json::Array(names));
    }
    let mut by_pattern = Map::new();
    for p in messages(&assignments, "pattern_assignments") {
        by_pattern.insert(str_field(&p, "pattern"), json!(str_field(&p, "netclass")));
    }
    Ok(json!({
        "classes": messages(&listing, "net_classes").iter().map(netclass_summary).collect::<Vec<_>>(),
        "assignments": by_net, "patterns": by_pattern,
    }))
}

// ---- RF and controlled impedance ----------------------------------------------------------------

/// `board_impedance_width(doc, impedance, signal_layer, structure, gap, reference_layer)`: the track
/// width for an impedance on this board's stackup.
pub(crate) async fn board_impedance_width(
    client: &mut Client,
    doc: &DynamicMessage,
    impedance: f64,
    signal_layer: &str,
    structure: &str,
    gap: Option<f64>,
    reference_layer: Option<&str>,
) -> Result<Json, Error> {
    if structure == "microstrip" && signal_layer != "F.Cu" && signal_layer != "B.Cu" {
        return Err(Error::Value(
            "microstrip is an outer-layer structure; use F.Cu or B.Cu".into(),
        ));
    }
    let stack = board::board_stackup_layers(client, doc).await?;
    let layers = stack["layers"].as_array().cloned().unwrap_or_default();
    let d = board_rf::dielectric_under(&layers, signal_layer, reference_layer)?;
    let width = board_rf::width_for(
        impedance,
        d.height,
        d.epsilon_r,
        d.copper_thickness,
        structure,
        gap,
    )?;
    let z = if structure == "microstrip" {
        board_rf::microstrip(width, d.height, d.epsilon_r, d.copper_thickness)?
    } else {
        board_rf::cpwg(width, gap.unwrap_or(0.0), d.height, d.epsilon_r)?
    };
    let mut out = Map::new();
    out.insert("width_mm".into(), json!(py_round(width, 3)));
    out.insert("impedance_ohm".into(), json!(py_round(z.0, 2)));
    out.insert("effective_er".into(), json!(py_round(z.1, 3)));
    out.insert("structure".into(), json!(structure));
    if let Some(gap) = gap.filter(|g| *g != 0.0) {
        out.insert("gap_mm".into(), json!(gap));
    }
    out.insert("signal_layer".into(), json!(signal_layer));
    out.insert("reference_layer".into(), json!(d.reference_layer));
    out.insert("dielectric_height_mm".into(), json!(py_round(d.height, 4)));
    out.insert("epsilon_r".into(), json!(py_round(d.epsilon_r, 3)));
    out.insert("copper_thickness_mm".into(), json!(d.copper_thickness));
    out.insert(
        "note".into(),
        json!("Soldermask lowers a surface trace by about 1-3 ohm; have the board house confirm."),
    );
    Ok(Json::Object(out))
}

/// The options of [`board_rf_net`].
pub(crate) struct RfNet<'a> {
    pub nets: &'a [String],
    pub netclass: &'a str,
    pub impedance: f64,
    pub width: Option<f64>,
    pub structure: &'a str,
    pub gap: Option<f64>,
    pub clearance: Option<f64>,
    pub signal_layer: &'a str,
    pub keep_others_away: Option<f64>,
}

/// `board_rf_net(...)`: a net class with the impedance-controlled width and clearance for the RF
/// nets, optionally with a custom rule keeping other nets away.
pub(crate) async fn board_rf_net(
    client: &mut Client,
    doc: &DynamicMessage,
    options: &RfNet<'_>,
) -> Result<Json, Error> {
    let mut calc = None;
    let width = match options.width {
        Some(width) => width,
        None => {
            let found = board_impedance_width(
                client,
                doc,
                options.impedance,
                options.signal_layer,
                options.structure,
                options.gap,
                None,
            )
            .await?;
            let width = found["width_mm"].as_f64().unwrap_or(0.0);
            calc = Some(found);
            width
        }
    };
    let clearance = match options.clearance {
        Some(clearance) => clearance,
        None => match options
            .gap
            .filter(|g| options.structure == "cpwg" && *g != 0.0)
        {
            Some(gap) => gap,
            None => py_round(3.0 * width, 3),
        },
    };
    let spec = json!({"name": options.netclass, "track_width": width, "clearance": clearance});
    let mut assign = Map::new();
    for net in options.nets {
        assign.insert(net.clone(), json!(options.netclass));
    }
    let classes = board_netclasses(client, doc, Some(&[spec]), Some(&assign), None).await?;
    let mut result = Map::new();
    result.insert("netclass".into(), json!(options.netclass));
    result.insert("nets".into(), json!(options.nets));
    result.insert("track_width_mm".into(), json!(width));
    result.insert("clearance_mm".into(), json!(clearance));
    if let Some(calc) = calc.filter(truthy) {
        result.insert("impedance".into(), calc);
    }
    let class = classes["classes"]
        .as_array()
        .and_then(|list| list.iter().find(|c| c["name"] == options.netclass).cloned())
        .unwrap_or(Json::Null);
    result.insert("class".into(), class);
    if let Some(away) = options.keep_others_away.filter(|a| *a != 0.0) {
        let netclass = options.netclass;
        let rule = json!({"name": format!("{netclass} isolation"),
                          "condition": format!("A.NetClass == '{netclass}' && B.NetClass != '{netclass}'"),
                          "constraints": [{"type": "clearance", "min": away}]});
        let rules = board_custom_rules(client, doc, "add", Some(&[rule]), None).await?;
        result.insert("custom_rules".into(), rules["rules"].clone());
    }
    Ok(Json::Object(result))
}

// ---- Keepouts -----------------------------------------------------------------------------------

/// What a keepout keeps out.
pub(crate) fn rule_area(
    tracks: bool,
    vias: bool,
    pads: bool,
    copper: bool,
    footprints: bool,
) -> RuleArea {
    RuleArea {
        copper,
        vias,
        tracks,
        pads,
        footprints,
    }
}

/// `board_footprint_keepout(doc, reference, margin, layers, ...)`: a keepout rule area around a
/// footprint (its courtyard, or its bounding box, plus margin) on every copper layer unless layers
/// are given.
pub(crate) async fn board_footprint_keepout(
    client: &mut Client,
    doc: &DynamicMessage,
    reference: &str,
    margin: f64,
    layers: Option<&[String]>,
    rule: RuleArea,
) -> Result<Json, Error> {
    let fp = board::board_footprint(client, doc, reference).await?;
    let back = int_field(&fp, "layer") == i64::from(known(BOARD_LAYER, "BL_B_Cu"));
    let courtyard = i64::from(known(
        BOARD_LAYER,
        if back { "BL_B_CrtYd" } else { "BL_F_CrtYd" },
    ));
    // Only the bounds of the union are used: (present, bounds); an empty part has no bounds
    let mut area: Option<Option<[f64; 4]>> = None;
    let shape_type = format!("{BT}BoardGraphicShape");
    for packed in messages(&message_field(&fp, "definition"), "items") {
        if proto::any_type_name(&packed) != shape_type {
            continue;
        }
        let Some(shape) = proto::unpack(&packed, &shape_type) else {
            continue;
        };
        if int_field(&shape, "layer") != courtyard {
            continue;
        }
        if let Some(g) = shape_geometry(&shape) {
            let bounds = g
                .bounds()
                .map(|r| [r.min().x, r.min().y, r.max().x, r.max().y]);
            area = Some(match (area, bounds) {
                (Some(Some(a)), Some(b)) => Some([
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]),
                (Some(Some(a)), None) => Some(a),
                (_, b) => b,
            });
        }
    }
    let bounds = match area {
        Some(bounds) => bounds.unwrap_or([f64::NAN; 4]),
        None => {
            let request = with_field(
                build(
                    &format!("{C}GetBoundingBox"),
                    json!({"items": [{"value": item_id(&fp)}]}),
                ),
                "header",
                item_header(doc),
            );
            let response = client
                .send(&request, &format!("{C}GetBoundingBoxResponse"))
                .await?;
            let Some(b) = messages(&response, "boxes").into_iter().next() else {
                return Err(Error::Value(format!(
                    "{reference} has no courtyard and no size"
                )));
            };
            let [x, y] = mm(&message_field(&b, "position"));
            let [w, h] = mm(&message_field(&b, "size"));
            [x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h)]
        }
    };
    let [x1, y1, x2, y2] = bounds;
    let points = [
        [x1 - margin, y1 - margin],
        [x2 + margin, y1 - margin],
        [x2 + margin, y2 + margin],
        [x1 - margin, y2 + margin],
    ];
    let names = match layers {
        None => board_copper_layers(client, doc).await?,
        Some(layers) => layers.to_vec(),
    };
    let name = format!("{reference} keepout");
    let settings = ZoneSettings {
        name: &name,
        ..ZoneSettings::default()
    };
    let zone = board::make_zone(&json!(points), &names, &settings, Some(rule))?;
    client
        .mutate(
            doc,
            std::slice::from_ref(&zone),
            false,
            Some(&format!("MCP: keepout around {reference}")),
        )
        .await?;
    let box_mm: Vec<f64> = [points[0][0], points[0][1], points[2][0], points[2][1]]
        .iter()
        .map(|v| py_round(*v, 4))
        .collect();
    Ok(json!({"rule_area": item_id(&zone), "around": reference, "box_mm": box_mm, "layers": names}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_formatting() {
        assert_eq!(py_float(50.0), "50.0");
        assert_eq!(py_float(1e16), "1e+16");
        assert_eq!(py_float(1e15), "1000000000000000.0");
        assert_eq!(py_float(0.0001), "0.0001");
        assert_eq!(py_float(0.00001), "1e-05");
        assert_eq!(py_float(1.5e-7), "1.5e-07");
        assert_eq!(py_float(-2.5), "-2.5");
        assert_eq!(py_fixed(49.95, 1), "50.0");
        assert_eq!(py_fixed(f64::NAN, 1), "nan");
        assert_eq!(py_list_str(&["F.Cu".into()]), "['F.Cu']");
    }

    #[test]
    fn custom_rules_follow_python() {
        let rule = custom_rule(&json!({"name": "RF", "condition": "A.NetClass == 'RF'",
            "constraints": [{"type": "clearance", "min": 0.5}, {"type": "disallow", "items": ["tracks"]},
                            {"type": "assertion", "expression": "x"}],
            "layer": "outer", "severity": "warning"}))
        .unwrap();
        let value = pbjson::to_json(&rule).unwrap();
        assert_eq!(value["layer_mode"], "CRLM_OUTER");
        assert_eq!(value["constraints"][0]["numeric"], json!({"min": 500000}));
        assert_eq!(
            value["constraints"][1]["disallow"],
            json!({"types": ["CRDT_TRACKS"]})
        );
        let error = |spec: Json| custom_rule(&spec).unwrap_err().to_string();
        assert_eq!(error(json!({})), "'name'");
        assert_eq!(
            error(json!({"name": 5})),
            "bad argument type for built-in operation"
        );
        assert_eq!(
            error(json!({"name": "x", "constraints": ["a"]})),
            "'str' object has no attribute 'get'"
        );
        assert_eq!(
            error(json!({"name": "x", "constraints": 5})),
            "'int' object is not iterable"
        );
        assert_eq!(
            error(json!({"name": "x", "constraints": [{"type": "bogus"}]})),
            "unknown constraint bogus"
        );
        assert_eq!(
            error(json!({"name": "x", "constraints": [{"type": 3}]})),
            "'int' object has no attribute 'upper'"
        );
        assert_eq!(
            error(json!({"name": "x", "layer": 3})),
            "'int' object has no attribute 'replace'"
        );
        assert_eq!(error(json!({"name": "x", "severity": "loud"})), "'loud'");
        assert_eq!(
            error(json!({"name": "x", "constraints": [{"type": "disallow", "items": "ab"}]})),
            "Enum CustomRuleDisallowType has no value defined for name 'CRDT_A'"
        );
        assert_eq!(
            error(json!({"name": "x", "constraints": [{"type": "clearance", "max": 3000}]})),
            "Value out of range: 3000000000"
        );
    }

    #[test]
    fn line_points_follow_shapely() {
        assert_eq!(line_points(&[]).unwrap(), Vec::<[f64; 2]>::new());
        assert!(line_points(&[vec![0.0, 0.0]])
            .unwrap_err()
            .to_string()
            .starts_with("IllegalArgumentException"));
        assert_eq!(
            line_points(&[vec![0.0], vec![1.0]])
                .unwrap_err()
                .to_string(),
            "The ordinate (last) dimension should be 2 or 3, got 1"
        );
        assert!(line_points(&[vec![0.0, 0.0], vec![1.0]])
            .unwrap_err()
            .to_string()
            .contains("(2,)"));
        assert_eq!(
            line_points(&[vec![0.0, 0.0, 9.0], vec![1.0, 1.0, 9.0]]).unwrap(),
            [[0.0, 0.0], [1.0, 1.0]]
        );
    }
}
