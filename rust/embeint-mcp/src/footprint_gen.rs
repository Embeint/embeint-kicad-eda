// SPDX-License-Identifier: GPL-3.0-or-later
//! A port of `tools/live_schematic_mcp/footprint_gen.py`: KLC-style footprint geometry from
//! datasheet package dimensions.  Pure geometry in millimetres, Y pointing down as the IPC API
//! uses; see the Python module for the conventions it follows.
//!
//! Numbers are computed as Python computes them.  [`Num`] keeps an int an int and a float a float
//! (a pad at `0` stays `0`, which the SolidCraft plan prints as `0`, not `0.0`), true division
//! always gives a float, `round(x, 4)` is Python's correctly rounded half-even rounding, and the
//! errors Python raises for bad package arguments (missing or unexpected keywords, odd pin
//! counts, unknown sizes and variants, float counts) carry Python's text.  Values of the wrong
//! type for arithmetic (a string where a dimension belongs) are refused with a generic text: the
//! exact TypeError wording depends on the Python operation and is not reproduced.

use embeint_ipc::client::Error;
use embeint_ipc::pbjson::py_repr;
use serde_json::{json, Map, Number, Value as Json};

pub const SILK_WIDTH: f64 = 0.12;
pub const FAB_WIDTH: f64 = 0.10;
pub const COURTYARD_WIDTH: f64 = 0.05;
const SILK_PAD_CLEARANCE: f64 = 0.2;
const COURTYARD_GRID: f64 = 0.01;

// Nominal (density level B) solder fillets for gull-wing leads
const TOE: f64 = 0.35;
const HEEL: f64 = 0.35;
const SIDE: f64 = 0.03;
// No-lead (QFN/DFN) terminals: the pad extends past the body edge by the toe
const NOLEAD_TOE: f64 = 0.3;
const NOLEAD_HEEL: f64 = 0.05;
const NOLEAD_SIDE: f64 = 0.0;

/// Stock-library land patterns for common chip sizes: (size, (pad_w, pad_h, x, body_l, body_w),
/// metric name), in `sorted(CHIP_SIZES)` order.
const CHIP_SIZES: [(&str, [f64; 5], &str); 6] = [
    ("0402", [0.54, 0.64, 0.51, 1.0, 0.5], "1005"),
    ("0603", [0.8, 0.95, 0.825, 1.6, 0.8], "1608"),
    ("0805", [1.025, 1.4, 0.9125, 2.0, 1.25], "2012"),
    ("1206", [1.125, 1.75, 1.4625, 3.2, 1.6], "3216"),
    ("1210", [1.125, 2.65, 1.4625, 3.2, 2.5], "3225"),
    ("2512", [1.525, 3.35, 3.1375, 6.3, 3.2], "6332"),
];

/// `sorted(PACKAGES)`.
const PACKAGES: [&str; 10] = [
    "chip",
    "dfn",
    "gullwing",
    "msop",
    "pin_header",
    "qfn",
    "soic",
    "sot",
    "ssop",
    "tssop",
];

// ---- Python numbers ------------------------------------------------------------------------

/// `round(value, digits)` for a float: correctly rounded, ties to even on the exact binary value
/// (Rust's fixed-precision formatting rounds exactly as Python's dtoa does).
pub fn py_round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// `repr(value)` for a float: the shortest text that reads back, in exponent form below 1e-4
/// and from 1e16 (`1e-05`, `1e+16`), always with a fraction or exponent (`2.0`).
pub fn py_float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    // `{:e}` gives the shortest round-tripping digits, as repr does
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if (-4..16).contains(&exponent) {
        let text = if exponent >= 0 {
            let whole = exponent as usize + 1;
            if digits.len() <= whole {
                format!("{digits}{}.0", "0".repeat(whole - digits.len()))
            } else {
                format!("{}.{}", &digits[..whole], &digits[whole..])
            }
        } else {
            format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
        };
        format!("{sign}{text}")
    } else {
        let mantissa = if digits.len() == 1 {
            digits
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        let mark = if exponent < 0 { '-' } else { '+' };
        format!("{sign}{mantissa}e{mark}{:02}", exponent.abs())
    }
}

/// Python's `str(value)` for a JSON value.
pub fn py_str(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        Json::Number(number) if is_float_number(number) => {
            py_float_repr(number.as_f64().unwrap_or(f64::NAN))
        }
        other => py_repr(other),
    }
}

/// Python's `repr(text)` for a string: single quotes unless the text holds one and no double
/// quote; backslashes and that quote escaped, control characters as `\n`, `\r`, `\t` or `\xNN`.
pub fn py_str_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..=0xa0).contains(&(c as u32)) || c == '\u{ad}' => {
                out.push_str(&format!(r"\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// The Python type name of a JSON value.
pub fn py_type(value: &Json) -> &'static str {
    match value {
        Json::Null => "NoneType",
        Json::Bool(_) => "bool",
        Json::Number(n) if is_float_number(n) => "float",
        Json::Number(_) => "int",
        Json::String(_) => "str",
        Json::Array(_) => "list",
        Json::Object(_) => "dict",
    }
}

/// Python's truth value of a JSON value.
pub fn truthy(value: &Json) -> bool {
    match value {
        Json::Null => false,
        Json::Bool(flag) => *flag,
        Json::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        Json::String(text) => !text.is_empty(),
        Json::Array(items) => !items.is_empty(),
        Json::Object(map) => !map.is_empty(),
    }
}

/// A JSON number Python reads as a float (it has a fraction or an exponent).
fn is_float_number(number: &Number) -> bool {
    number.to_string().contains(['.', 'e', 'E'])
}

/// A Python TypeError (or another non-ValueError exception): only its text reaches the client.
fn type_error(message: impl Into<String>) -> Error {
    Error::Runtime(message.into())
}

/// A Python int or float.  Arithmetic between ints stays int (falling back to float beyond i64,
/// where Python would keep an exact big int); anything with a float is a float.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    /// The value as a float (`float(x)`).
    pub fn f(self) -> f64 {
        match self {
            Num::Int(value) => value as f64,
            Num::Float(value) => value,
        }
    }

    fn int_op(self, other: Num, op: fn(i64, i64) -> Option<i64>, fop: fn(f64, f64) -> f64) -> Num {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => {
                op(a, b).map_or(Num::Float(fop(a as f64, b as f64)), Num::Int)
            }
            _ => Num::Float(fop(self.f(), other.f())),
        }
    }

    pub fn add(self, other: impl Into<Num>) -> Num {
        self.int_op(other.into(), i64::checked_add, |a, b| a + b)
    }

    pub fn sub(self, other: impl Into<Num>) -> Num {
        self.int_op(other.into(), i64::checked_sub, |a, b| a - b)
    }

    pub fn mul(self, other: impl Into<Num>) -> Num {
        self.int_op(other.into(), i64::checked_mul, |a, b| a * b)
    }

    /// `self / other`: always a float; dividing by zero raises ZeroDivisionError.
    pub fn div(self, other: impl Into<Num>) -> Result<Num, Error> {
        let other = other.into();
        if other.f() == 0.0 {
            let both_int = matches!((self, other), (Num::Int(_), Num::Int(_)));
            return Err(type_error(if both_int {
                "division by zero"
            } else {
                "float division by zero"
            }));
        }
        Ok(Num::Float(self.f() / other.f()))
    }

    /// `self / 2`, which cannot fail.
    pub fn half(self) -> f64 {
        self.f() / 2.0
    }

    pub fn neg(self) -> Num {
        match self {
            Num::Int(value) => value
                .checked_neg()
                .map_or(Num::Float(-(value as f64)), Num::Int),
            Num::Float(value) => Num::Float(-value),
        }
    }

    pub fn abs(self) -> Num {
        match self {
            Num::Int(value) => value
                .checked_abs()
                .map_or(Num::Float((value as f64).abs()), Num::Int),
            Num::Float(value) => Num::Float(value.abs()),
        }
    }

    pub fn lt(self, other: impl Into<Num>) -> bool {
        match (self, other.into()) {
            (Num::Int(a), Num::Int(b)) => a < b,
            (a, b) => a.f() < b.f(),
        }
    }

    pub fn gt(self, other: impl Into<Num>) -> bool {
        other.into().lt(self)
    }

    /// `round(self, 4)` (`_r`): an int is returned as it is.
    pub fn r(self) -> Num {
        match self {
            Num::Int(_) => self,
            Num::Float(value) => Num::Float(py_round(value, 4)),
        }
    }

    /// `round(self * 1_000_000)` (`_nm`): millimetres to integer nanometres.
    pub fn nm(self) -> i64 {
        match self {
            Num::Int(value) => value.saturating_mul(1_000_000),
            Num::Float(value) => (value * 1_000_000.0).round_ties_even() as i64,
        }
    }

    /// The int a count must be (`range(n)`): a float is refused as Python refuses it.
    pub fn count(self) -> Result<i64, Error> {
        match self {
            Num::Int(value) => Ok(value),
            Num::Float(_) => Err(type_error(
                "'float' object cannot be interpreted as an integer",
            )),
        }
    }

    pub fn to_json(self) -> Json {
        match self {
            Num::Int(value) => Json::from(value),
            Num::Float(value) => Number::from_f64(value).map_or(Json::Null, Json::Number),
        }
    }

    pub fn py_str(self) -> String {
        match self {
            Num::Int(value) => value.to_string(),
            Num::Float(value) => py_float_repr(value),
        }
    }
}

impl From<f64> for Num {
    fn from(value: f64) -> Self {
        Num::Float(value)
    }
}

impl From<i64> for Num {
    fn from(value: i64) -> Self {
        Num::Int(value)
    }
}

impl From<i32> for Num {
    fn from(value: i32) -> Self {
        Num::Int(i64::from(value))
    }
}

/// `min(a, b)`: the first unless the second is smaller.
pub fn py_min(a: Num, b: Num) -> Num {
    if b.lt(a) {
        b
    } else {
        a
    }
}

fn fmin(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

fn fmax(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

/// `round(value, 4)` (`_r`) for a float.
pub fn r(value: f64) -> f64 {
    py_round(value, 4)
}

/// A JSON value used as a Python number (bool is an int in Python).
pub fn num(value: &Json) -> Result<Num, Error> {
    match value {
        Json::Bool(flag) => Ok(Num::Int(i64::from(*flag))),
        Json::Number(number) if !is_float_number(number) => Ok(number
            .as_i64()
            .map_or_else(|| Num::Float(number.as_f64().unwrap_or(f64::NAN)), Num::Int)),
        Json::Number(number) => Ok(Num::Float(number.as_f64().unwrap_or(f64::NAN))),
        other => Err(type_error(format!(
            "unsupported operand type(s) for arithmetic: '{}'",
            py_type(other)
        ))),
    }
}

// ---- Shapes --------------------------------------------------------------------------------

/// One pad as footprint_gen's pad dict describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    pub number: String,
    /// "smd" or "pth"
    pub kind: &'static str,
    /// "rect", "roundrect", "circle" or "oval"
    pub shape: &'static str,
    pub position: (Num, Num),
    pub size: (Num, Num),
    /// The drill diameter as given (pin headers); converted where the pad is built, as Python
    /// first uses it there
    pub drill: Option<Json>,
}

/// A text placement: position, layer and size.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpec {
    pub position: (f64, f64),
    pub layer: &'static str,
    pub size: f64,
}

pub type Line = [(f64, f64); 2];

/// footprint_gen's shape dict.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// `name or default`: the caller's name as given, or the generated one
    pub name: Json,
    pub mounting: &'static str,
    pub pads: Vec<Pad>,
    pub body: [f64; 4],
    pub courtyard: [f64; 4],
    pub fab_lines: Vec<Line>,
    pub silk_lines: Vec<Line>,
    pub pin1_marker: Option<Vec<(Num, Num)>>,
    pub reference: TextSpec,
    pub value: TextSpec,
    pub fab_reference: TextSpec,
}

#[cfg(test)]
fn pair(x: f64, y: f64) -> Json {
    json!([Num::Float(x).to_json(), Num::Float(y).to_json()])
}

#[cfg(test)]
fn line_json(line: &Line) -> Json {
    json!([pair(line[0].0, line[0].1), pair(line[1].0, line[1].1)])
}

#[cfg(test)]
impl Shape {
    /// The shape as footprint_gen returns it (tuples as lists), for tests against Python.
    pub fn to_json(&self) -> Json {
        let pads: Vec<Json> = self
            .pads
            .iter()
            .map(|pad| {
                let mut entry = json!({
                    "number": pad.number, "type": pad.kind, "shape": pad.shape,
                    "position": [pad.position.0.to_json(), pad.position.1.to_json()],
                    "size": [pad.size.0.to_json(), pad.size.1.to_json()],
                });
                if let Some(drill) = &pad.drill {
                    entry["drill"] = drill.clone();
                }
                entry
            })
            .collect();
        let floats = |values: &[f64]| -> Json {
            Json::Array(values.iter().map(|v| Num::Float(*v).to_json()).collect())
        };
        json!({
            "name": self.name,
            "mounting": self.mounting,
            "pads": pads,
            "body": floats(&self.body),
            "courtyard": floats(&self.courtyard),
            "fab_lines": self.fab_lines.iter().map(line_json).collect::<Vec<_>>(),
            "silk_lines": self.silk_lines.iter().map(line_json).collect::<Vec<_>>(),
            "pin1_marker": self.pin1_marker.as_ref().map(|points| {
                points.iter().map(|(x, y)| json!([x.to_json(), y.to_json()])).collect::<Vec<_>>()
            }),
            "reference": {"position": pair(self.reference.position.0, self.reference.position.1),
                          "layer": self.reference.layer, "size": self.reference.size},
            "value": {"position": pair(self.value.position.0, self.value.position.1),
                      "layer": self.value.layer, "size": self.value.size},
            "fab_reference": {"position": pair(self.fab_reference.position.0, self.fab_reference.position.1),
                              "size": self.fab_reference.size},
        })
    }
}

fn smd_pad(number: i64, x: Num, y: Num, w: Num, h: Num, shape: &'static str) -> Pad {
    Pad {
        number: number.to_string(),
        kind: "smd",
        shape,
        position: (x.r(), y.r()),
        size: (w.r(), h.r()),
        drill: None,
    }
}

fn pad_box(pad: &Pad) -> [f64; 4] {
    let ((x, y), (w, h)) = (pad.position, pad.size);
    [
        x.f() - w.half(),
        y.f() - h.half(),
        x.f() + w.half(),
        y.f() + h.half(),
    ]
}

fn union(boxes: &[[f64; 4]]) -> Result<[f64; 4], Error> {
    let Some(first) = boxes.first() else {
        return Err(Error::Value("min() iterable argument is empty".into()));
    };
    Ok(boxes.iter().skip(1).fold(*first, |acc, b| {
        [
            fmin(acc[0], b[0]),
            fmin(acc[1], b[1]),
            fmax(acc[2], b[2]),
            fmax(acc[3], b[3]),
        ]
    }))
}

/// `_grid_out`: round away from zero to the courtyard grid (courtyards only ever grow).
fn grid_out(value: f64) -> f64 {
    let steps = py_round(value.abs() / COURTYARD_GRID, 6).ceil();
    (steps * COURTYARD_GRID).copysign(value)
}

/// `_clip_segment`: split an axis-aligned segment, dropping the parts inside any keep-out.
fn clip_segment(start: (f64, f64), end: (f64, f64), keepouts: &[[f64; 4]]) -> Vec<Line> {
    let ((x1, y1), (x2, y2)) = (start, end);
    let horizontal = (y1 - y2).abs() < 1e-9;
    let (lo, hi) = if horizontal {
        (fmin(x1, x2), fmax(x1, x2))
    } else {
        (fmin(y1, y2), fmax(y1, y2))
    };
    let fixed = if horizontal { y1 } else { x1 };
    let mut cuts = Vec::new();
    for &[kx1, ky1, kx2, ky2] in keepouts {
        if horizontal && ky1 < fixed && fixed < ky2 {
            cuts.push((kx1, kx2));
        } else if !horizontal && kx1 < fixed && fixed < kx2 {
            cuts.push((ky1, ky2));
        }
    }
    // sorted() of float pairs; a stable sort keeps equal pairs in order, as Python does
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut pieces = Vec::new();
    let mut cursor = lo;
    for (c1, c2) in cuts {
        if c1 > cursor {
            pieces.push((cursor, fmin(c1, hi)));
        }
        cursor = fmax(cursor, c2);
        if cursor >= hi {
            break;
        }
    }
    if cursor < hi {
        pieces.push((cursor, hi));
    }
    pieces
        .into_iter()
        .filter(|(a, b)| b - a >= 0.2)
        .map(|(a, b)| {
            if horizontal {
                [(a, fixed), (b, fixed)]
            } else {
                [(fixed, a), (fixed, b)]
            }
        })
        .collect()
}

/// `_finish`: silkscreen, fabrication, courtyard and text around pads and a body rectangle.
fn finish(
    name: Json,
    pads: Vec<Pad>,
    body: [f64; 4],
    pin1_corner: bool,
    courtyard_clearance: f64,
    mounting: &'static str,
) -> Result<Shape, Error> {
    let [bx1, by1, bx2, by2] = body;
    let boxes: Vec<[f64; 4]> = pads.iter().map(pad_box).collect();
    let pad_union = union(&boxes)?;
    let [cx1, cy1, cx2, cy2] = union(&[pad_union, body])?;
    let courtyard = [
        r(grid_out(cx1 - courtyard_clearance)),
        r(grid_out(cy1 - courtyard_clearance)),
        r(grid_out(cx2 + courtyard_clearance)),
        r(grid_out(cy2 + courtyard_clearance)),
    ];

    // Fabrication outline with the pin-1 corner chamfered (25 % of the short side, at most 1 mm)
    let chamfer = if pin1_corner {
        fmin(1.0, 0.25 * fmin(bx2 - bx1, by2 - by1))
    } else {
        0.0
    };
    let fab = [
        (bx1 + chamfer, by1),
        (bx2, by1),
        (bx2, by2),
        (bx1, by2),
        (bx1, by1 + chamfer),
    ];
    let fab_lines: Vec<Line> = (0..fab.len())
        .map(|i| [fab[i], fab[(i + 1) % fab.len()]])
        .collect();

    // Silkscreen just outside the body, cut back from copper
    let offset = SILK_WIDTH / 2.0 + 0.01;
    let (sx1, sy1, sx2, sy2) = (bx1 - offset, by1 - offset, bx2 + offset, by2 + offset);
    // Evaluated left to right as Python does: x1 - 0.2 - 0.06, not x1 - 0.26
    let half_silk = SILK_WIDTH / 2.0;
    let keepouts: Vec<[f64; 4]> = boxes
        .iter()
        .map(|[x1, y1, x2, y2]| {
            [
                x1 - SILK_PAD_CLEARANCE - half_silk,
                y1 - SILK_PAD_CLEARANCE - half_silk,
                x2 + SILK_PAD_CLEARANCE + half_silk,
                y2 + SILK_PAD_CLEARANCE + half_silk,
            ]
        })
        .collect();
    let mut silk_lines = Vec::new();
    for (a, b) in [
        ((sx1, sy1), (sx2, sy1)),
        ((sx2, sy1), (sx2, sy2)),
        ((sx2, sy2), (sx1, sy2)),
        ((sx1, sy2), (sx1, sy1)),
    ] {
        silk_lines.extend(clip_segment(a, b, &keepouts));
    }

    // Pin-1 marker: a small filled triangle pointing at pad 1 from outside it, clear of copper
    // (str(StopIteration()) is empty, so a footprint without pad 1 fails with no text)
    let first = pads
        .iter()
        .find(|p| p.number == "1")
        .ok_or_else(|| type_error(""))?;
    let [px1, py1, px2, _] = pad_box(first);
    let size = 0.6;
    let (fx, fy) = first.position;
    let marker = if (fx.f() - (pad_union[0] + pad_union[2]) / 2.0).abs()
        > (fy.f() - (pad_union[1] + pad_union[3]) / 2.0).abs()
    {
        // pad 1 on the left or right edge: point sideways from beyond its outer end
        let left = fx.lt(0);
        let tip_x = if left {
            px1 - SILK_PAD_CLEARANCE
        } else {
            px2 + SILK_PAD_CLEARANCE
        };
        let back = if left { -size } else { size };
        let y = fy.f();
        vec![
            (Num::Float(tip_x), fy),
            (Num::Float(tip_x + back), Num::Float(y - size / 2.0)),
            (Num::Float(tip_x + back), Num::Float(y + size / 2.0)),
        ]
    } else {
        let tip_y = py1 - SILK_PAD_CLEARANCE;
        let x = fx.f();
        vec![
            (fx, Num::Float(tip_y)),
            (Num::Float(x - size / 2.0), Num::Float(tip_y - size)),
            (Num::Float(x + size / 2.0), Num::Float(tip_y - size)),
        ]
    };
    // The first point keeps pad 1's own coordinate, an int when it is one
    let pin1_marker = pin1_corner.then(|| marker.iter().map(|(x, y)| (x.r(), y.r())).collect());

    let fab_text = r(fmin(1.0, fmax(0.5, 0.25 * fmin(bx2 - bx1, by2 - by1))));
    let round_line =
        |line: &Line| -> Line { [(r(line[0].0), r(line[0].1)), (r(line[1].0), r(line[1].1))] };
    Ok(Shape {
        name,
        mounting,
        pads,
        body: body.map(r),
        courtyard,
        fab_lines: fab_lines.iter().map(round_line).collect(),
        silk_lines: silk_lines.iter().map(round_line).collect(),
        pin1_marker,
        reference: TextSpec {
            position: (0.0, r(courtyard[1] - 1.0)),
            layer: "F.SilkS",
            size: 1.0,
        },
        value: TextSpec {
            position: (0.0, r(courtyard[3] + 1.0)),
            layer: "F.Fab",
            size: 1.0,
        },
        fab_reference: TextSpec {
            position: (r((bx1 + bx2) / 2.0), r((by1 + by2) / 2.0)),
            layer: "F.Fab",
            size: fab_text,
        },
    })
}

/// `_dual_rows`: pads in two columns, left top-to-bottom, then right bottom-to-top.
fn dual_rows(
    left: Num,
    right: Num,
    pitch: Num,
    row_center: Num,
    pad_len: Num,
    pad_w: Num,
) -> Result<Vec<Pad>, Error> {
    let mut positions = Vec::new();
    for (is_left, count) in [(true, left), (false, right)] {
        let x = if is_left {
            row_center.neg()
        } else {
            row_center
        };
        let n = count.count()?;
        let middle = count.sub(1).half();
        let mut ys: Vec<Num> = (0..n.max(0))
            .map(|i| Num::Float(i as f64 - middle).mul(pitch))
            .collect();
        if !is_left {
            ys.reverse();
        }
        positions.extend(ys.into_iter().map(|y| (x, y)));
    }
    Ok(positions
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| smd_pad(i as i64 + 1, x, y, pad_len, pad_w, "roundrect"))
        .collect())
}

/// The exposed pad `[w, h]`: `exposed_pad[0]`, `exposed_pad[1]` as Python indexes it.
fn exposed_size(value: &Json) -> Result<(Num, Num), Error> {
    let index = |i: usize| -> Result<Num, Error> {
        match value {
            Json::Array(items) => items
                .get(i)
                .ok_or_else(|| type_error("list index out of range"))
                .and_then(num),
            Json::Bool(_) | Json::Number(_) => Err(type_error(format!(
                "'{}' object is not subscriptable",
                py_type(value)
            ))),
            other => Err(type_error(format!(
                "exposed_pad must be a list [w, h], not '{}'",
                py_type(other)
            ))),
        }
    };
    Ok((index(0)?, index(1)?))
}

/// One package function's bound arguments (`f(**params)`).
struct Bound(Map<String, Json>);

impl Bound {
    fn get(&self, name: &str) -> &Json {
        self.0.get(name).unwrap_or(&Json::Null)
    }

    fn num(&self, name: &str) -> Result<Num, Error> {
        num(self.get(name))
    }

    /// `value or default()`: the argument when it is truthy, else the default.
    fn or_num(
        &self,
        name: &str,
        default: impl FnOnce() -> Result<Num, Error>,
    ) -> Result<Num, Error> {
        let value = self.get(name);
        if truthy(value) {
            num(value)
        } else {
            default()
        }
    }

    /// `name or default`.
    fn name_or(&self, default: impl FnOnce() -> String) -> Json {
        let name = self.get("name");
        if truthy(name) {
            name.clone()
        } else {
            Json::String(default())
        }
    }
}

/// Python's argument binding for `function(**params)`: an unexpected keyword or missing required
/// arguments raise TypeError with CPython's text.  `signature` lists (name, default) in order,
/// None for a required argument.
fn bind(
    function: &str,
    signature: &[(&str, Option<Json>)],
    params: &Map<String, Json>,
) -> Result<Bound, Error> {
    if let Some(key) = params
        .keys()
        .find(|key| !signature.iter().any(|(name, _)| name == key))
    {
        return Err(type_error(format!(
            "{function}() got an unexpected keyword argument '{key}'"
        )));
    }
    let missing: Vec<String> = signature
        .iter()
        .filter(|(name, default)| default.is_none() && !params.contains_key(*name))
        .map(|(name, _)| format!("'{name}'"))
        .collect();
    if !missing.is_empty() {
        let names = match missing.len() {
            1 => missing[0].clone(),
            2 => format!("{} and {}", missing[0], missing[1]),
            n => format!("{}, and {}", missing[..n - 1].join(", "), missing[n - 1]),
        };
        let plural = if missing.len() == 1 { "" } else { "s" };
        return Err(type_error(format!(
            "{function}() missing {} required positional argument{plural}: {names}",
            missing.len()
        )));
    }
    let mut values = Map::new();
    for (name, default) in signature {
        let value = params
            .get(*name)
            .cloned()
            .or_else(|| default.clone())
            .unwrap_or(Json::Null);
        values.insert((*name).to_owned(), value);
    }
    Ok(Bound(values))
}

fn none() -> Option<Json> {
    Some(Json::Null)
}

/// SOIC / SOP / SSOP / TSSOP / MSOP: two rows of gull-wing leads.
fn gullwing(params: &Map<String, Json>) -> Result<Shape, Error> {
    let a = bind(
        "gullwing",
        &[
            ("pins", None),
            ("pitch", None),
            ("body_width", None),
            ("body_length", None),
            ("lead_span", none()),
            ("lead_length", Some(json!(0.6))),
            ("lead_width", none()),
            ("pad_length", none()),
            ("pad_width", none()),
            ("row_pitch", none()),
            ("exposed_pad", none()),
            ("name", none()),
            ("model", none()),
        ],
        params,
    )?;
    let pins = a.num("pins")?;
    let odd = match pins {
        Num::Int(value) => value.rem_euclid(2) != 0,
        Num::Float(value) => value.rem_euclid(2.0) != 0.0,
    };
    if odd || pins.lt(4) {
        return Err(Error::Value(
            "gull-wing packages need an even pin count of at least 4".into(),
        ));
    }
    let (pad_length, row_pitch) = if a.get("pad_length").is_null() || a.get("row_pitch").is_null() {
        if a.get("lead_span").is_null() {
            return Err(Error::Value(
                "give lead_span (tip to tip), or pad_length and row_pitch from the land pattern"
                    .into(),
            ));
        }
        let length = a.or_num("pad_length", || {
            Ok(a.num("lead_length")?.add(TOE).add(HEEL))
        })?;
        let pitch = a.or_num("row_pitch", || {
            Ok(a.num("lead_span")?
                .sub(a.num("lead_length")?)
                .add(TOE - HEEL))
        })?;
        (length, pitch)
    } else {
        (a.num("pad_length")?, a.num("row_pitch")?)
    };
    let pitch = a.num("pitch")?;
    let pad_width = if a.get("pad_width").is_null() {
        let lead = a.or_num("lead_width", || Ok(pitch.mul(0.5)))?;
        py_min(lead.add(2.0 * SIDE), pitch.sub(0.2))
    } else {
        a.num("pad_width")?
    };
    // pins // 2 of a float is a float, which range() then refuses
    let half = match pins {
        Num::Int(n) => Num::Int(n.div_euclid(2)),
        Num::Float(v) => Num::Float((v / 2.0).floor()),
    };
    let mut pads = dual_rows(
        half,
        half,
        pitch,
        Num::Float(row_pitch.half()),
        pad_length,
        pad_width,
    )?;
    let exposed = a.get("exposed_pad");
    if truthy(exposed) {
        let (w, h) = exposed_size(exposed)?;
        let number = pins.add(1).count()?;
        pads.push(smd_pad(number, Num::Int(0), Num::Int(0), w, h, "rect"));
    }
    let width = a.num("body_width")?;
    let length = a.num("body_length")?;
    // -body_width / 2 negates first: an int 0 gives 0.0, a float 0.0 gives -0.0
    let body = [
        width.neg().half(),
        length.neg().half(),
        width.half(),
        length.half(),
    ];
    let name = a.name_or(|| format!("SOIC-{}", py_str(a.get("pins"))));
    finish(name, pads, body, true, 0.25, "smd")
}

/// QFN (pins on four sides) or DFN (pins_y only: two columns).
fn no_lead(params: &Map<String, Json>) -> Result<Shape, Error> {
    let a = bind(
        "no_lead",
        &[
            ("pins_x", None),
            ("pins_y", None),
            ("pitch", None),
            ("body_x", None),
            ("body_y", None),
            ("terminal_length", Some(json!(0.4))),
            ("terminal_width", none()),
            ("pad_length", none()),
            ("pad_width", none()),
            ("exposed_pad", none()),
            ("name", none()),
            ("model", none()),
        ],
        params,
    )?;
    let pad_length = a.or_num("pad_length", || {
        Ok(a.num("terminal_length")?.add(NOLEAD_TOE).add(NOLEAD_HEEL))
    })?;
    let pad_width = a.or_num("pad_width", || {
        let pitch = a.num("pitch")?;
        let terminal = a.or_num("terminal_width", || Ok(pitch.mul(0.5)))?;
        Ok(py_min(terminal.add(2.0 * NOLEAD_SIDE), pitch.sub(0.2)))
    })?;
    let outer_x = a.num("body_x")?.half() + NOLEAD_TOE;
    let outer_y = a.num("body_y")?.half() + NOLEAD_TOE;
    let pitch = a.num("pitch")?;
    let span = |count: Num| -> Result<Vec<Num>, Error> {
        let n = count.count()?;
        let middle = count.sub(1).half();
        Ok((0..n.max(0))
            .map(|i| Num::Float(i as f64 - middle).mul(pitch))
            .collect())
    };
    let pins_x = a.num("pins_x")?;
    let pins_y = a.num("pins_y")?;
    let half_length = pad_length.half();
    let mut raw: Vec<(Num, Num, Num, Num)> = Vec::new();
    // left, top to bottom
    for y in span(pins_y)? {
        raw.push((Num::Float(-outer_x + half_length), y, pad_length, pad_width));
    }
    // bottom, left to right
    for x in span(pins_x)? {
        raw.push((x, Num::Float(outer_y - half_length), pad_width, pad_length));
    }
    // right, bottom to top
    for y in span(pins_y)?.into_iter().rev() {
        raw.push((Num::Float(outer_x - half_length), y, pad_length, pad_width));
    }
    // top, right to left
    for x in span(pins_x)?.into_iter().rev() {
        raw.push((x, Num::Float(-outer_y + half_length), pad_width, pad_length));
    }
    let mut pads: Vec<Pad> = raw
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, w, h))| smd_pad(i as i64 + 1, x, y, w, h, "roundrect"))
        .collect();
    let exposed = a.get("exposed_pad");
    if truthy(exposed) {
        let (w, h) = exposed_size(exposed)?;
        let number = pads.len() as i64 + 1;
        pads.push(smd_pad(number, Num::Int(0), Num::Int(0), w, h, "rect"));
    }
    let body_x = a.num("body_x")?;
    let body_y = a.num("body_y")?;
    let body = [
        body_x.neg().half(),
        body_y.neg().half(),
        body_x.half(),
        body_y.half(),
    ];
    let total = pins_x.add(pins_y).mul(2);
    let kind = if truthy(a.get("pins_x")) {
        "QFN"
    } else {
        "DFN"
    };
    let name = a.name_or(|| format!("{kind}-{}", total.py_str()));
    finish(name, pads, body, true, 0.25, "smd")
}

fn literal_pad(number: i64, x: f64, y: f64, w: f64, h: f64) -> Pad {
    // A literal 0 in the Python source is an int
    let coordinate = |v: f64| if v == 0.0 { Num::Int(0) } else { Num::Float(v) };
    smd_pad(
        number,
        coordinate(x),
        coordinate(y),
        Num::Float(w),
        Num::Float(h),
        "roundrect",
    )
}

/// SOT-23 (-3, -5, -6) and SOT-223 with stock-library land patterns.
fn sot(params: &Map<String, Json>) -> Result<Shape, Error> {
    let a = bind(
        "sot",
        &[("variant", None), ("name", none()), ("model", none())],
        params,
    )?;
    let variant = match a.get("variant") {
        Json::String(text) => text.to_uppercase().replace("SOT-", "").replace("SOT", ""),
        other => {
            return Err(type_error(format!(
                "'{}' object has no attribute 'upper'",
                py_type(other)
            )))
        }
    };
    let (pads, body, default) = match variant.as_str() {
        "23" | "23-3" => (
            vec![
                literal_pad(1, -0.9375, -0.95, 1.475, 0.6),
                literal_pad(2, -0.9375, 0.95, 1.475, 0.6),
                literal_pad(3, 0.9375, 0.0, 1.475, 0.6),
            ],
            [-0.65, -1.45, 0.65, 1.45],
            "SOT-23".to_owned(),
        ),
        "23-5" | "23-6" => {
            let count: i64 = if variant.ends_with('5') { 5 } else { 6 };
            let mut pads = dual_rows(
                Num::Int(3),
                Num::Int(count - 3),
                Num::Float(0.95),
                Num::Float(1.1375),
                Num::Float(1.325),
                Num::Float(0.6),
            )?;
            if count == 5 {
                // pins 4 and 5 sit at the outer positions on the right
                pads[3].position = (Num::Float(1.1375), Num::Float(0.95));
                pads[4].position = (Num::Float(1.1375), Num::Float(-0.95));
            }
            (pads, [-0.8, -1.45, 0.8, 1.45], format!("SOT-23-{count}"))
        }
        "223" => {
            let mut pads: Vec<Pad> = [(1, -2.3), (2, 0.0), (3, 2.3)]
                .into_iter()
                .map(|(n, y)| literal_pad(n, -3.15, y, 2.0, 1.5))
                .collect();
            pads.push(literal_pad(4, 3.15, 0.0, 2.0, 3.8));
            (
                pads,
                [-1.75, -3.25, 1.75, 3.25],
                "SOT-223-3_TabPin2".to_owned(),
            )
        }
        _ => return Err(Error::Value("variant must be 23, 23-5, 23-6 or 223".into())),
    };
    finish(a.name_or(|| default), pads, body, true, 0.25, "smd")
}

/// Two-terminal chip parts (R, C, L, D) in imperial sizes 0402-2512.
fn chip(params: &Map<String, Json>) -> Result<Shape, Error> {
    let a = bind(
        "chip",
        &[
            ("size", None),
            ("prefix", Some(json!("R"))),
            ("name", none()),
            ("model", none()),
        ],
        params,
    )?;
    let size = a.get("size");
    if matches!(size, Json::Array(_) | Json::Object(_)) {
        return Err(type_error(format!("unhashable type: '{}'", py_type(size))));
    }
    let Some((key, [pad_w, pad_h, x, body_l, body_w], metric)) = CHIP_SIZES
        .iter()
        .find(|(key, _, _)| size.as_str() == Some(*key))
    else {
        let sizes: Vec<Json> = CHIP_SIZES.iter().map(|(key, _, _)| json!(key)).collect();
        return Err(Error::Value(format!(
            "size must be one of {}",
            py_repr(&Json::Array(sizes))
        )));
    };
    let pads = vec![
        literal_pad(1, -x, 0.0, *pad_w, *pad_h),
        literal_pad(2, *x, 0.0, *pad_w, *pad_h),
    ];
    let body = [-body_l / 2.0, -body_w / 2.0, body_l / 2.0, body_w / 2.0];
    let prefix = py_str(a.get("prefix"));
    let name = a.name_or(|| format!("{prefix}_{key}_{metric}Metric"));
    finish(name, pads, body, false, 0.25, "smd")
}

/// `format(value, "02d")`: an int (or bool) zero-padded to two characters.
fn two_digits(value: &Json) -> Result<String, Error> {
    match num(value)? {
        Num::Int(n) => Ok(format!("{n:02}")),
        Num::Float(_) => Err(Error::Value(
            "Unknown format code 'd' for object of type 'float'".into(),
        )),
    }
}

/// Through-hole pin header, pin 1 at the origin (square), odd pins in the first column.
fn pin_header(params: &Map<String, Json>) -> Result<Shape, Error> {
    let a = bind(
        "pin_header",
        &[
            ("pins_per_row", None),
            ("rows", Some(json!(1))),
            ("pitch", Some(json!(2.54))),
            ("drill", Some(json!(1.0))),
            ("pad", Some(json!(1.7))),
            ("name", none()),
            ("model", none()),
        ],
        params,
    )?;
    // rows not in (1, 2): numbers equal to 1 or 2 (True == 1) pass
    let rows_value = a.get("rows");
    let rows = match rows_value {
        Json::Bool(_) | Json::Number(_) => num(rows_value)?,
        _ => Num::Float(f64::NAN),
    };
    if rows.f() != 1.0 && rows.f() != 2.0 {
        return Err(Error::Value("rows must be 1 or 2".into()));
    }
    let total = a.num("pins_per_row")?.mul(rows).count()?;
    let rows_int = rows.count()?;
    let pitch = a.num("pitch")?;
    let mut pads = Vec::new();
    for index in 0..total.max(0) {
        let (row, column) = (index.div_euclid(rows_int), index.rem_euclid(rows_int));
        let pad = a.num("pad")?;
        pads.push(Pad {
            number: (index + 1).to_string(),
            kind: "pth",
            shape: if index == 0 { "rect" } else { "circle" },
            position: (
                Num::Int(column).mul(pitch).r(),
                Num::Int(row).mul(pitch).r(),
            ),
            size: (pad, pad),
            drill: Some(a.get("drill").clone()),
        });
    }
    let half = pitch.half();
    let pins_per_row = a.num("pins_per_row")?;
    let body = [
        -half,
        -half,
        rows.sub(1).mul(pitch).f() + half,
        pins_per_row.sub(1).mul(pitch).f() + half,
    ];
    let default = format!(
        "PinHeader_{}x{}_P{:.2}mm_Vertical",
        py_str(rows_value),
        two_digits(a.get("pins_per_row"))?,
        pitch.f()
    );
    finish(a.name_or(|| default), pads, body, true, 0.5, "tht")
}

/// `generate(package, **params)`.
pub fn generate(package: &str, params: &Map<String, Json>) -> Result<Shape, Error> {
    if params.contains_key("package") {
        return Err(type_error(
            "generate() got multiple values for argument 'package'",
        ));
    }
    let key = package.to_lowercase();
    match key.as_str() {
        "gullwing" | "soic" | "tssop" | "msop" | "ssop" => gullwing(params),
        "qfn" => no_lead(params),
        "dfn" => {
            let mut params = params.clone();
            params.entry("pins_x").or_insert(json!(0));
            no_lead(&params)
        }
        "sot" => sot(params),
        "chip" => chip(params),
        "pin_header" => pin_header(params),
        _ => {
            let names: Vec<Json> = PACKAGES.iter().map(|name| json!(name)).collect();
            Err(Error::Value(format!(
                "package must be one of {}",
                py_repr(&Json::Array(names))
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected shapes and errors, generated once with the Python module (see the file's
    /// generated_by entry) for every package kind.
    static EXPECTED: &str = include_str!("../testdata/footprint_gen.json");

    #[test]
    fn shapes_match_python() {
        let data: Json = serde_json::from_str(EXPECTED).unwrap();
        let cases = data["cases"].as_array().unwrap();
        assert!(cases.len() >= 20);
        for case in cases {
            let package = case["package"].as_str().unwrap();
            let params = case["params"].as_object().unwrap();
            let result = generate(package, params);
            match (&case["shape"], &case["error"]) {
                (Json::Null, Json::String(error)) => {
                    let ours = result.expect_err(&format!("{package} {params:?} should fail"));
                    assert_eq!(&ours.to_string(), error, "{package} {params:?}");
                }
                (shape, _) => {
                    let ours = result.unwrap_or_else(|e| panic!("{package} {params:?}: {e}"));
                    assert_eq!(&ours.to_json(), shape, "{package} {params:?}");
                }
            }
        }
    }

    #[test]
    fn python_number_formatting() {
        assert_eq!(py_float_repr(1.0), "1.0");
        assert_eq!(py_float_repr(0.5), "0.5");
        assert_eq!(py_float_repr(1e-5), "1e-05");
        assert_eq!(py_float_repr(0.0001), "0.0001");
        assert_eq!(py_float_repr(1e16), "1e+16");
        assert_eq!(py_float_repr(123456789012345.6), "123456789012345.6");
        assert_eq!(py_float_repr(-2.5e-7), "-2.5e-07");
        assert_eq!(py_float_repr(-0.0), "-0.0");
        assert_eq!(py_round(0.125, 2), 0.12);
        assert_eq!(py_round(2.675, 2), 2.67);
        assert_eq!(py_round(1.00005, 4), 1.0001);
        assert_eq!(py_str(&json!(true)), "True");
        assert_eq!(py_str(&json!(8)), "8");
    }
}
