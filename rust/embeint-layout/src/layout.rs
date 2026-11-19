// SPDX-License-Identifier: GPL-3.0-or-later
//! `layout_box` from `symbol_layout.py`, step for step, with integer nanometre coordinates.
//!
//! Comments name the Python behaviour each step reproduces; keep the two in step.  Where Python
//! recomputes a pin's classification, this computes it once per pin from the same inputs.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::pyvalue::{py_repr, py_str, py_strip, truthy, unhashable_type};
use crate::{mm_number, LayoutRequest};

/// KiCad's 100 mil schematic grid.
pub const GRID_NM: i64 = 2_540_000;
/// Length of every visible pin (`PIN_LENGTH`).
pub const PIN_LENGTH_NM: i64 = 2_540_000;
/// Approximate width of one character of 1.27 mm pin-name text (`CHAR_WIDTH`).
const CHAR_WIDTH_NM: i64 = 1_000_000;
const HALF_GRID_NM: i64 = GRID_NM / 2;

const SIDES: [&str; 4] = ["left", "right", "top", "bottom"];
const LEFT: usize = 0;
const RIGHT: usize = 1;
const TOP: usize = 2;
const BOTTOM: usize = 3;
/// `ORIENTATION`, indexed like `SIDES`: the direction from the connection point to the body.
const ORIENTATION: [&str; 4] = ["SPO_RIGHT", "SPO_LEFT", "SPO_DOWN", "SPO_UP"];

const ELECTRICAL_TYPES: [(&str, &str); 12] = [
    ("input", "EPT_INPUT"),
    ("output", "EPT_OUTPUT"),
    ("bidirectional", "EPT_BIDIRECTIONAL"),
    ("tri_state", "EPT_TRISTATE"),
    ("passive", "EPT_PASSIVE"),
    ("free", "EPT_FREE"),
    ("unspecified", "EPT_UNSPECIFIED"),
    ("power_in", "EPT_POWER_INPUT"),
    ("power_out", "EPT_POWER_OUTPUT"),
    ("open_collector", "EPT_OPEN_COLLECTOR"),
    ("open_emitter", "EPT_OPEN_EMITTER"),
    ("no_connect", "EPT_NO_CONNECT"),
];

const GROUND_NAMES: [&str; 12] = [
    "GND", "VSS", "AGND", "DGND", "PGND", "SGND", "GNDA", "GNDD", "EP", "PAD", "EPAD", "0V",
];
const FEEDBACK_NAMES: [&str; 12] = [
    "FB", "ADJ", "SENSE", "SNS", "VSENSE", "VFB", "BYP", "BYPASS", "NR", "PG", "PGOOD", "FLAG",
];
const NO_CONNECT_NAMES: [&str; 5] = ["NC", "DNC", "N.C.", "N/C", "NIC"];

/// Python's message for an unknown electrical type: `sorted(ELECTRICAL_TYPES)`.
const TYPE_CHOICES: &str = "['bidirectional', 'free', 'input', 'no_connect', 'open_collector', \
                            'open_emitter', 'output', 'passive', 'power_in', 'power_out', \
                            'tri_state', 'unspecified']";

/// The exception `layout_box` raises, with Python's type name and `str(exception)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    Value(String),
    Key(String),
    Type(String),
}

impl LayoutError {
    pub fn kind(&self) -> &'static str {
        match self {
            LayoutError::Value(_) => "ValueError",
            LayoutError::Key(_) => "KeyError",
            LayoutError::Type(_) => "TypeError",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            LayoutError::Value(m) | LayoutError::Key(m) | LayoutError::Type(m) => m,
        }
    }
}

type Pin = Map<String, Value>;

/// A visible pin on a side, with its `group_rank` computed once.
struct Row {
    rank: u8,
    pin: Pin,
}

fn text(value: &str) -> Value {
    Value::String(value.to_string())
}

/// `_base_name`: `str(name).upper()` without the inversion marks `~{`, `}`, `/`, `~`, `#`, `!`.
///
/// Python removes them with successive `replace` calls.  `~` goes anyway, so removing `~{` first
/// only matters for a `{` straight after a `~` in the upper-cased name; one pass does the same.
fn base_name(pin: &Pin) -> String {
    let upper = match pin.get("name") {
        None => return String::new(),
        Some(Value::String(name)) => name.to_uppercase(),
        Some(other) => py_str(other).to_uppercase(),
    };
    let mut name = String::with_capacity(upper.len());
    let mut previous = None;

    for ch in upper.chars() {
        let after_tilde = previous == Some('~');
        previous = Some(ch);

        if !(matches!(ch, '}' | '/' | '~' | '#' | '!') || (ch == '{' && after_tilde)) {
            name.push(ch);
        }
    }

    name
}

/// `is_ground` for a pin of this (validated) type and base name.
fn is_ground(kind: &str, base: &str) -> bool {
    matches!(kind, "power_in" | "passive")
        && (GROUND_NAMES.contains(&base) || base.starts_with("GND"))
}

/// `default_side`: supplies and control inputs left, outputs and feedback right, grounds bottom.
fn default_side(kind: &str, base: &str) -> usize {
    if is_ground(kind, base) {
        BOTTOM
    } else if FEEDBACK_NAMES.contains(&base)
        || matches!(
            kind,
            "output" | "power_out" | "open_collector" | "open_emitter" | "tri_state"
        )
    {
        RIGHT
    } else {
        LEFT
    }
}

/// `group_rank`: supplies first on each side, then signals, then no-connects.
fn group_rank(kind: &str, base: &str) -> u8 {
    if matches!(kind, "power_in" | "power_out") && !is_ground(kind, base) {
        0
    } else if kind == "no_connect" || NO_CONNECT_NAMES.contains(&base) {
        2
    } else {
        1
    }
}

/// `{**pin, key: value, ...}` on an owned pin: existing keys keep their place, new ones append.
fn update(pin: &mut Pin, fields: [(&str, Value); 3]) {
    for (key, value) in fields {
        pin.insert(key.to_string(), value);
    }
}

/// `pin.setdefault(key, value)`
fn set_default(pin: &mut Pin, key: &str, value: Value) {
    if !pin.contains_key(key) {
        pin.insert(key.to_string(), value);
    }
}

/// `len(str(name))`, without copying a string name.
fn name_length(pin: &Pin) -> usize {
    match pin.get("name") {
        None => 0,
        Some(Value::String(name)) => name.chars().count(),
        Some(other) => py_str(other).chars().count(),
    }
}

/// `centred`: grid x positions for top/bottom pins, symmetric about the centre line.
fn centred(count: usize) -> Vec<i64> {
    let count = count as i64;
    let half = count / 2;
    // An even count skips the centre position.
    let skip = |i: i64| if count % 2 == 0 && i >= half { 1 } else { 0 };
    (0..count).map(|i| (i - half + skip(i)) * GRID_NM).collect()
}

/// `_round_up(value, step)` for positive whole-nanometre values.  Python rounds the quotient to
/// six decimals before taking the ceiling; every width here is a multiple of 10 um, so a
/// remainder is never small enough for that rounding to matter.
fn round_up(value: i64, step: i64) -> i64 {
    (value + step - 1).div_euclid(step) * step
}

fn point(x: i64, y: i64) -> Value {
    Value::Array(vec![mm_number(x), mm_number(y)])
}

/// Lay out the request's pins around a centred rectangle, exactly as `layout_box` does.
pub fn layout_box(request: LayoutRequest) -> Result<Value, LayoutError> {
    if !matches!(request.arrange.as_str(), "auto" | "as_given") {
        return Err(LayoutError::Value(
            "arrange must be auto or as_given".into(),
        ));
    }

    if !(0..=6).contains(&request.group_gap) {
        return Err(LayoutError::Value(
            "group_gap must be between 0 and 6 rows".into(),
        ));
    }

    if request.pins.is_empty() {
        return Err(LayoutError::Value("A symbol needs at least one pin".into()));
    }

    let pin_count = request.pins.len();
    // Each side is a list of rows; None is an empty row (an explicit or inserted gap).
    let mut by_side: [Vec<Option<Row>>; 4] = Default::default();
    let mut hidden: Vec<Pin> = Vec::new();
    let mut numbers: HashSet<String> = HashSet::new();

    for mut pin in request.pins {
        if pin.get("gap").is_some_and(truthy) {
            // by_side[pin.get("side", "left")].append(None)
            match pin.get("side") {
                None => by_side[LEFT].push(None),
                Some(Value::String(side)) if SIDES.contains(&side.as_str()) => {
                    by_side[SIDES.iter().position(|s| s == side).unwrap_or(LEFT)].push(None)
                }
                Some(side) => {
                    return Err(match unhashable_type(side) {
                        Some(kind) => LayoutError::Type(format!("unhashable type: '{kind}'")),
                        None => LayoutError::Key(py_repr(side)),
                    })
                }
            }

            continue;
        }

        let number = match pin.get("number") {
            None => String::new(),
            Some(Value::String(number)) => py_strip(number).to_string(),
            Some(other) => py_strip(&py_str(other)).to_string(),
        };

        if number.is_empty() {
            return Err(LayoutError::Value(format!(
                "Pin {} needs a number",
                py_repr(&Value::Object(pin))
            )));
        }

        if !numbers.insert(number.clone()) {
            return Err(LayoutError::Value(format!(
                "Pin number {number} is used twice"
            )));
        }

        // kind = pin.get("type", "passive"), which must be a key of ELECTRICAL_TYPES
        let declared = match pin.get("type") {
            None => "passive",
            Some(Value::String(kind)) => {
                match ELECTRICAL_TYPES.iter().find(|(name, _)| name == kind) {
                    Some((name, _)) => name,
                    None => return Err(unknown_type(&number)),
                }
            }
            Some(other) => {
                return Err(match unhashable_type(other) {
                    Some(name) => LayoutError::Type(format!("unhashable type: '{name}'")),
                    None => unknown_type(&number),
                })
            }
        };

        let base = base_name(&pin);
        let mut kind = declared;

        // is_no_connect({**pin, "type": kind})
        if kind == "no_connect" || NO_CONNECT_NAMES.contains(&base.as_str()) {
            // An NC pin is a hidden no-connect pin that takes no row, unless "hidden" is false.
            kind = "no_connect";

            if pin.get("hidden").is_none_or(truthy) {
                update(
                    &mut pin,
                    [
                        ("number", text(&number)),
                        ("type", text(kind)),
                        ("side", text("hidden")),
                    ],
                );
                pin.insert("hidden".into(), Value::Bool(true));
                hidden.push(pin);
                continue;
            }
        }

        // side = pin.get("side") or default_side(pin), with the pin's own (declared) type
        let side = match pin.get("side") {
            Some(value) if truthy(value) => match value {
                Value::String(side) => SIDES.iter().position(|s| s == side),
                _ => None,
            },
            _ => Some(default_side(declared, &base)),
        };

        let Some(side) = side else {
            return Err(LayoutError::Value(format!(
                "Pin {number}: side must be one of ('left', 'right', 'top', 'bottom')"
            )));
        };

        let rank = group_rank(kind, &base);
        update(
            &mut pin,
            [
                ("number", text(&number)),
                ("type", text(kind)),
                ("side", text(SIDES[side])),
            ],
        );
        by_side[side].push(Some(Row { rank, pin }));
    }

    if request.arrange == "auto" {
        for side in [LEFT, RIGHT] {
            if by_side[side].iter().any(Option::is_none) {
                continue; // explicit gaps: the caller is arranging this side
            }

            // sorted(entries, key=group_rank) is stable, and so is sort_by_key
            let mut ordered: Vec<Row> = by_side[side].drain(..).flatten().collect();
            ordered.sort_by_key(|row| row.rank);
            let mut arranged: Vec<Option<Row>> = Vec::with_capacity(ordered.len());

            for row in ordered {
                if arranged.last().is_some_and(|last: &Option<Row>| {
                    last.as_ref().is_some_and(|l| l.rank != row.rank)
                }) {
                    arranged.extend((0..request.group_gap).map(|_| None));
                }

                arranged.push(Some(row));
            }

            by_side[side] = arranged;
        }
    }

    let label_width = |side: usize| -> i64 {
        let longest = by_side[side]
            .iter()
            .flatten()
            .map(|row| name_length(&row.pin))
            .max();
        longest.unwrap_or(0) as i64 * CHAR_WIDTH_NM
    };
    let [left_width, right_width, top_width, bottom_width] =
        [LEFT, RIGHT, TOP, BOTTOM].map(label_width);
    let rows = by_side[LEFT].len().max(by_side[RIGHT].len()).max(1) as i64;
    let top_x = centred(by_side[TOP].len());
    let bottom_x = centred(by_side[BOTTOM].len());
    let column_extent = top_x
        .iter()
        .chain(&bottom_x)
        .map(|x| x.abs())
        .max()
        .unwrap_or(-HALF_GRID_NM)
        + HALF_GRID_NM;

    // Size the body so side-pin names and top/bottom names cannot cross; edges on the grid.
    let side_names = left_width.max(right_width) + HALF_GRID_NM;
    let width = (2 * 5_080_000)
        .max(left_width + right_width + 2 * GRID_NM)
        .max(2 * (column_extent + side_names));
    let height = 5_080_000
        .max((rows + 1) * GRID_NM)
        .max(top_width + bottom_width + 2 * GRID_NM);
    let width = round_up(width, 2 * GRID_NM);
    let height = round_up(height, 2 * GRID_NM);
    let (x1, y1, x2, y2) = (-width / 2, -height / 2, width / 2, height / 2);

    // Hidden pins sit inside the body in rows no visible side pin uses (the gaps between groups
    // first), just inside an edge and pointing inward.  Right side first, then left.
    let total_rows = (height / GRID_NM - 1).max(1);
    let mut slots: Vec<(i64, i64, &str)> = Vec::new();

    for (side, edge_x, orientation) in [
        (RIGHT, x2 - GRID_NM, "SPO_LEFT"),
        (LEFT, x1 + GRID_NM, "SPO_RIGHT"),
    ] {
        let used = |row: i64| by_side[side].get(row as usize).is_some_and(Option::is_some);
        slots.extend(
            (0..total_rows)
                .filter(|row| !used(*row))
                .map(|row| (edge_x, y1 + GRID_NM * (row + 1), orientation)),
        );
    }

    let [left, right, top, bottom] = by_side;
    let side_rows = |rows: Vec<Option<Row>>, x: i64| {
        rows.into_iter()
            .enumerate()
            .filter_map(move |(index, row)| Some((row?.pin, x, y1 + GRID_NM * (index as i64 + 1))))
    };
    let column = |rows: Vec<Option<Row>>, xs: Vec<i64>, y: i64| {
        xs.into_iter()
            .zip(rows)
            .filter_map(move |(x, row)| Some((row?.pin, x, y)))
    };
    let placed = side_rows(left, x1 - PIN_LENGTH_NM)
        .chain(side_rows(right, x2 + PIN_LENGTH_NM))
        .chain(column(top, top_x, y1 - PIN_LENGTH_NM))
        .chain(column(bottom, bottom_x, y2 + PIN_LENGTH_NM));
    let mut output: Vec<Value> = Vec::with_capacity(pin_count);

    for (mut pin, x, y) in placed {
        pin.insert("position".into(), point(x, y));
        let side = pin
            .get("side")
            .and_then(Value::as_str)
            .and_then(|s| SIDES.iter().position(|t| *t == s));
        set_default(
            &mut pin,
            "orientation",
            side.map_or(Value::Null, |side| text(ORIENTATION[side])),
        );
        finish(&mut pin, mm_number(PIN_LENGTH_NM));
        output.push(Value::Object(pin));
    }

    for (index, mut pin) in hidden.into_iter().enumerate() {
        let (x, y, orientation) = match slots.get(index) {
            Some(slot) => *slot,
            None => {
                // More hidden pins than free rows: stack them in further columns.
                let extra = (index - slots.len()) as i64;
                let (column, row) = (extra / total_rows, extra % total_rows);
                (
                    x2 - GRID_NM * (column + 2),
                    y1 + GRID_NM * (row + 1),
                    "SPO_LEFT",
                )
            }
        };

        // {**pin, "position": ..., "orientation": ..., "length": 0}
        update(
            &mut pin,
            [
                ("position", point(x, y)),
                ("orientation", text(orientation)),
                ("length", Value::from(0)),
            ],
        );
        finish(&mut pin, Value::from(0));
        output.push(Value::Object(pin));
    }

    let mut layout = Map::new();
    layout.insert(
        "body".into(),
        Value::Array(vec![
            mm_number(x1),
            mm_number(y1),
            mm_number(x2),
            mm_number(y2),
        ]),
    );
    layout.insert("pins".into(), Value::Array(output));
    Ok(Value::Object(layout))
}

fn unknown_type(number: &str) -> LayoutError {
    LayoutError::Value(format!("Pin {number}: type must be one of {TYPE_CHOICES}"))
}

/// The common tail of every placed pin: its electrical type and, if not given, its length.
fn finish(pin: &mut Pin, length: Value) {
    let kind = pin.get("type").and_then(Value::as_str).unwrap_or("passive");
    let electrical = ELECTRICAL_TYPES
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or("EPT_PASSIVE", |(_, e)| e);
    pin.insert("electrical_type".into(), text(electrical));
    set_default(pin, "length", length);
}
