// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `kicad_clipper2::rectclip`, mirroring
//! `port/oracle/shims/clipper2_rectclip.cpp` request for request. Points are `[x, y, z]`
//! integers, rectangles `[left, top, right, bottom]`, locations the `Location` enumerator
//! values 0..=4.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_clipper2::rectclip::{self, Location, RectClip64, RectClipBudget, RectClipLines64};
use kicad_clipper2::{ClipperError, Path64, Paths64, Point64, Rect64};
use serde_json::{json, Value};
use std::result::Result;

pub use kicad_clipper2::rectclip::UNIT;

/// Request size limits, identical in the C++ adapter.
const MAX_POINTS: usize = 1 << 16;
const MAX_CALLS: usize = 64;
/// The host budget for one `Execute` call: far above what a request within the size limits
/// needs, so it only stops loops that would not end.
pub const BUDGET: RectClipBudget = RectClipBudget {
    ops: 1 << 22,
    steps: 1 << 30,
};

enum E {
    Invalid(String),
    Clip(ClipperError),
}

impl From<ClipperError> for E {
    fn from(error: ClipperError) -> Self {
        Self::Clip(error)
    }
}

type R<T> = Result<T, E>;

fn invalid<T>(what: &str) -> R<T> {
    Err(E::Invalid(format!("invalid input: {what}")))
}

fn keys(v: &Value, fields: &[&str]) -> R<()> {
    match v.as_object() {
        Some(o) if o.len() == fields.len() && fields.iter().all(|f| o.contains_key(*f)) => Ok(()),
        _ => invalid("fields"),
    }
}

fn at<'a>(v: &'a Value, name: &str) -> R<&'a Value> {
    match v.as_object().and_then(|o| o.get(name)) {
        Some(value) => Ok(value),
        None => invalid(name),
    }
}

fn i64_of(v: &Value) -> R<i64> {
    match v.as_i64() {
        Some(i) => Ok(i),
        None => invalid("int64"),
    }
}

fn i32_of(v: &Value) -> R<i32> {
    match i32::try_from(i64_of(v)?) {
        Ok(i) => Ok(i),
        Err(_) => invalid("int"),
    }
}

fn boolean(v: &Value) -> R<bool> {
    match v.as_bool() {
        Some(b) => Ok(b),
        None => invalid("boolean"),
    }
}

fn loc_of(v: &Value) -> R<Location> {
    let value = i64_of(v)?;
    match i32::try_from(value).ok().and_then(Location::from_int) {
        Some(loc) => Ok(loc),
        None => invalid("location"),
    }
}

/// Points read so far in one request.
struct Counter(usize);

impl Counter {
    fn add(&mut self, count: usize) -> R<()> {
        self.0 += count;
        if self.0 > MAX_POINTS {
            return invalid("size");
        }
        Ok(())
    }
}

fn point(v: &Value, count: &mut Counter) -> R<Point64> {
    match v.as_array() {
        Some(a) if a.len() == 3 => {
            count.add(1)?;
            Ok(Point64 {
                x: i64_of(&a[0])?,
                y: i64_of(&a[1])?,
                z: i64_of(&a[2])?,
            })
        }
        _ => invalid("point"),
    }
}

fn path(v: &Value, count: &mut Counter) -> R<Path64> {
    let Some(items) = v.as_array() else {
        return invalid("path");
    };
    let mut out = Path64::new();
    for item in items {
        let p = point(item, count)?;
        out.try_reserve(1)
            .map_err(|_| E::Clip(ClipperError::Allocation))?;
        out.push(p);
    }
    Ok(out)
}

fn paths(v: &Value, count: &mut Counter) -> R<Paths64> {
    let Some(items) = v.as_array() else {
        return invalid("paths");
    };
    let mut out = Paths64::new();
    for item in items {
        let p = path(item, count)?;
        out.try_reserve(1)
            .map_err(|_| E::Clip(ClipperError::Allocation))?;
        out.push(p);
    }
    Ok(out)
}

fn rect(v: &Value) -> R<Rect64> {
    match v.as_array() {
        Some(a) if a.len() == 4 => Ok(Rect64::new(
            i64_of(&a[0])?,
            i64_of(&a[1])?,
            i64_of(&a[2])?,
            i64_of(&a[3])?,
        )),
        _ => invalid("rect"),
    }
}

fn points(v: &Value, n: usize, count: &mut Counter) -> R<Path64> {
    match v.as_array() {
        Some(a) if a.len() == n => path(v, count),
        _ => invalid("points"),
    }
}

fn write_point(p: &Point64) -> Value {
    json!([p.x, p.y, p.z])
}

fn write_paths(paths: &Paths64) -> Value {
    Value::Array(
        paths
            .iter()
            .map(|p| Value::Array(p.iter().map(write_point).collect()))
            .collect(),
    )
}

/// The two-element slice `p` (length checked by [`points`]).
fn pair(p: &Path64) -> R<(&Point64, &Point64)> {
    match p.as_slice() {
        [a, b] => Ok((a, b)),
        _ => invalid("points"),
    }
}

fn quad(p: &Path64) -> R<(&Point64, &Point64, &Point64, &Point64)> {
    match p.as_slice() {
        [a, b, c, d] => Ok((a, b, c, d)),
        _ => invalid("points"),
    }
}

fn run(v: &Value, op: &str) -> R<Value> {
    let mut count = Counter(0);
    match op {
        "rect_clip" | "rect_clip_lines" => {
            keys(v, &["op", "rect", "calls"])?;
            let r = rect(at(v, "rect")?)?;
            let calls = match at(v, "calls")?.as_array() {
                Some(c) if c.len() <= MAX_CALLS => c,
                _ => return invalid("calls"),
            };
            let mut input = Vec::new();
            for call in calls {
                input.push(paths(call, &mut count)?);
            }
            let mut results = Vec::new();
            if op == "rect_clip_lines" {
                let mut clipper = RectClipLines64::new(&r, BUDGET)?;
                for call in &input {
                    results.push(write_paths(&clipper.execute(call)?));
                }
            } else {
                let mut clipper = RectClip64::new(&r, BUDGET)?;
                for call in &input {
                    results.push(write_paths(&clipper.execute(call)?));
                }
            }
            Ok(json!({"results": results}))
        }
        "path1_contains_path2" => {
            keys(v, &["op", "path1", "path2"])?;
            let a = path(at(v, "path1")?, &mut count)?;
            let b = path(at(v, "path2")?, &mut count)?;
            Ok(json!(rectclip::path1_contains_path2(&a, &b)?))
        }
        "get_location" => {
            keys(v, &["op", "rect", "pt", "loc"])?;
            let r = rect(at(v, "rect")?)?;
            let pt = point(at(v, "pt")?, &mut count)?;
            let mut loc = loc_of(at(v, "loc")?)?;
            let result = rectclip::get_location(&r, &pt, &mut loc);
            Ok(json!({"result": result, "loc": loc.as_int()}))
        }
        "is_horizontal" => {
            keys(v, &["op", "points"])?;
            let p = points(at(v, "points")?, 2, &mut count)?;
            let (a, b) = pair(&p)?;
            Ok(json!(rectclip::is_horizontal(a, b)))
        }
        "get_segment_intersection" => {
            keys(v, &["op", "points", "ip"])?;
            let p = points(at(v, "points")?, 4, &mut count)?;
            let mut ip = point(at(v, "ip")?, &mut count)?;
            let (a, b, c, d) = quad(&p)?;
            let result = rectclip::get_segment_intersection(a, b, c, d, &mut ip)?;
            Ok(json!({"result": result, "ip": write_point(&ip)}))
        }
        "get_intersection" => {
            keys(v, &["op", "rect_path", "p", "p2", "loc", "ip"])?;
            let rect_path = path(at(v, "rect_path")?, &mut count)?;
            let p = point(at(v, "p")?, &mut count)?;
            let p2 = point(at(v, "p2")?, &mut count)?;
            let mut loc = loc_of(at(v, "loc")?)?;
            let mut ip = point(at(v, "ip")?, &mut count)?;
            let result = rectclip::get_intersection(&rect_path, &p, &p2, &mut loc, &mut ip)?;
            Ok(json!({"result": result, "loc": loc.as_int(), "ip": write_point(&ip)}))
        }
        "get_adjacent_location" => {
            keys(v, &["op", "loc", "clockwise"])?;
            let loc = loc_of(at(v, "loc")?)?;
            let cw = boolean(at(v, "clockwise")?)?;
            Ok(json!(rectclip::get_adjacent_location(loc, cw).as_int()))
        }
        "heading_clockwise" | "are_opposites" => {
            keys(v, &["op", "prev", "curr"])?;
            let prev = loc_of(at(v, "prev")?)?;
            let curr = loc_of(at(v, "curr")?)?;
            Ok(json!(if op == "heading_clockwise" {
                rectclip::heading_clockwise(prev, curr)
            } else {
                rectclip::are_opposites(prev, curr)
            }))
        }
        "is_clockwise" => {
            keys(v, &["op", "prev", "curr", "prev_pt", "curr_pt", "rect_mp"])?;
            let prev = loc_of(at(v, "prev")?)?;
            let curr = loc_of(at(v, "curr")?)?;
            let a = point(at(v, "prev_pt")?, &mut count)?;
            let b = point(at(v, "curr_pt")?, &mut count)?;
            let m = point(at(v, "rect_mp")?, &mut count)?;
            Ok(json!(rectclip::is_clockwise(prev, curr, &a, &b, &m)?))
        }
        "get_edges_for_pt" => {
            keys(v, &["op", "pt", "rect"])?;
            let pt = point(at(v, "pt")?, &mut count)?;
            let r = rect(at(v, "rect")?)?;
            Ok(json!(rectclip::get_edges_for_pt(&pt, &r)))
        }
        "is_heading_clockwise" => {
            keys(v, &["op", "points", "edge"])?;
            let p = points(at(v, "points")?, 2, &mut count)?;
            let edge = i32_of(at(v, "edge")?)?;
            let (a, b) = pair(&p)?;
            Ok(json!(rectclip::is_heading_clockwise(a, b, edge)))
        }
        "has_horz_overlap" | "has_vert_overlap" => {
            keys(v, &["op", "points"])?;
            let p = points(at(v, "points")?, 4, &mut count)?;
            let (a, b, c, d) = quad(&p)?;
            Ok(json!(if op == "has_horz_overlap" {
                rectclip::has_horz_overlap(a, b, c, d)
            } else {
                rectclip::has_vert_overlap(a, b, c, d)
            }))
        }
        "start_locs_are_clockwise" => {
            keys(v, &["op", "locs"])?;
            let locs = match at(v, "locs")?.as_array() {
                Some(l) if l.len() <= MAX_POINTS => l,
                _ => return invalid("locs"),
            };
            let mut values = Vec::new();
            for loc in locs {
                values.push(loc_of(loc)?);
            }
            Ok(json!(rectclip::start_locs_are_clockwise(&values)))
        }
        _ => invalid("op"),
    }
}

/// Answer one request line with the shared envelope.
pub fn dispatch(line: &str) -> Value {
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"unit": unit, "case": case, "error": {"kind": kind, "message": message}});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                Value::Null,
                Value::Null,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    let unit = request
        .get("unit")
        .filter(|u| u.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    let case = request
        .get("case")
        .filter(|c| c.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    if keys(&request, &["unit", "case", "input"]).is_err()
        || unit.as_str().is_none_or(str::is_empty)
        || case.as_str().is_none_or(str::is_empty)
        || !request["input"].is_object()
    {
        return failure(
            unit,
            case,
            "invalid_request",
            "expected unit, case, input object",
        );
    }
    if unit != UNIT {
        return failure(unit, case, "unknown_unit", "unit has no registered shim");
    }
    let input = &request["input"];
    let Some(op) = input.get("op").and_then(Value::as_str) else {
        return failure(unit, case, "invalid_input", "invalid input: op");
    };
    match run(input, op) {
        Ok(result) => json!({"unit": unit, "case": case, "result": result}),
        Err(E::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(E::Clip(ClipperError::Undefined(_))) => failure(
            unit,
            case,
            "invalid_input",
            &format!("undefined behaviour: {op}"),
        ),
        Err(E::Clip(ClipperError::ResourceLimit | ClipperError::Allocation)) => {
            failure(unit, case, "invalid_input", &format!("harness limit: {op}"))
        }
        Err(E::Clip(ClipperError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"unit": unit, "case": case, "error": {"kind": "unported", "upstream": upstream, "unit": owner}})
        }
        Err(E::Clip(other)) => failure(unit, case, "execution_error", &other.to_string()),
    }
}
