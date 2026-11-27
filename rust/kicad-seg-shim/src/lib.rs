// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.seg.application.ui.p001`. It mirrors
//! `port/oracle/shims/kimath_src_seg.cpp`: 32-bit integer points and IEEE bit strings in,
//! every observable value out exactly. Key and argument validation follow the C++ adapter's
//! order and messages; an undefined pinned evaluation is the same `invalid_input` refusal.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::seg::{Seg, SegError, UNIT};
use kicad_math::{Vector2, Vector2I};
use serde_json::{json, Value};

enum Error {
    Invalid(String),
    Seg(SegError),
}
impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Invalid(v)
    }
}
impl From<&str> for Error {
    fn from(v: &str) -> Self {
        Self::Invalid(v.into())
    }
}
impl From<SegError> for Error {
    fn from(v: SegError) -> Self {
        Self::Seg(v)
    }
}

const UNDEFINED: &str = "undefined seg operation";

fn keys(v: &Value, fields: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str, String> {
    v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn boolean(v: &Value, name: &str) -> Result<bool, String> {
    v.get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("expected bool: {name}"))
}
fn integer(v: &Value, name: &str) -> Result<i32, String> {
    let n = v
        .get(name)
        .ok_or_else(|| format!("expected integer: {name}"))?;
    n.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| "integer out of range".to_string())
}
fn number(v: &Value, name: &str) -> Result<f64, String> {
    let t = text(v, name)?;
    if t.len() != 16 || !t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(format!("invalid hex: {name}"));
    }
    u64::from_str_radix(t, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("invalid hex: {name}"))
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn read_point(v: &Value) -> Result<Vector2I, String> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    let mut values = a.iter().map(|n| {
        n.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| "point out of range".to_string())
    });
    Ok(Vector2::new(
        values.next().ok_or("expected point")??,
        values.next().ok_or("expected point")??,
    ))
}
fn named_point(v: &Value, name: &str) -> Result<Vector2I, String> {
    read_point(v.get(name).ok_or("expected point")?)
}
fn read_seg(v: &Value, name: &str) -> Result<Seg, String> {
    let a = v
        .get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .ok_or_else(|| format!("expected segment: {name}"))?;
    let mut points = a.iter().map(read_point);
    let start = points.next().ok_or("expected point")??;
    let end = points.next().ok_or("expected point")??;
    Ok(Seg::new(start, end))
}
fn point(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn segment(s: &Seg) -> Value {
    json!({"a": point(s.a), "b": point(s.b), "index": s.index()})
}

fn run(v: &Value) -> Result<Value, Error> {
    let op = text(v, "op")?;
    match op {
        "squared_distance_point"
        | "distance_point"
        | "nearest_point_point"
        | "reflect_point"
        | "line_project"
        | "perpendicular_seg"
        | "parallel_seg" => {
            keys(v, &["op", "seg", "point"])?;
            let seg = read_seg(v, "seg")?;
            let p = named_point(v, "point")?;
            Ok(match op {
                "squared_distance_point" => json!(seg.squared_distance_point(p)?),
                "distance_point" => json!(seg.distance_point(p)?),
                "nearest_point_point" => point(seg.nearest_point_point(p)?),
                "reflect_point" => point(seg.reflect_point(p)?),
                "line_project" => point(seg.line_project(p)?),
                "perpendicular_seg" => segment(&seg.perpendicular_seg(p)?),
                _ => segment(&seg.parallel_seg(p)?),
            })
        }
        "contains_point" => {
            let has = v.get("threshold").is_some();
            if has {
                keys(v, &["op", "seg", "point", "threshold"])?;
            } else {
                keys(v, &["op", "seg", "point"])?;
            }
            let seg = read_seg(v, "seg")?;
            let p = named_point(v, "point")?;
            // The C++ default argument.
            let threshold = if has { integer(v, "threshold")? } else { 3 };
            Ok(json!(seg.contains_point(p, threshold)?))
        }
        "line_distance" => {
            let has = v.get("determine_side").is_some();
            if has {
                keys(v, &["op", "seg", "point", "determine_side"])?;
            } else {
                keys(v, &["op", "seg", "point"])?;
            }
            let seg = read_seg(v, "seg")?;
            let p = named_point(v, "point")?;
            let side = if has {
                boolean(v, "determine_side")?
            } else {
                false
            };
            Ok(json!(seg.line_distance(p, side)?))
        }
        "squared_distance_seg"
        | "distance_seg"
        | "angle"
        | "nearest_point_seg"
        | "nearest_points"
        | "intersects"
        | "approx_perpendicular" => {
            keys(v, &["op", "seg", "other"])?;
            let seg = read_seg(v, "seg")?;
            let other = read_seg(v, "other")?;
            Ok(match op {
                "squared_distance_seg" => json!(seg.squared_distance_seg(&other)?),
                "distance_seg" => json!(seg.distance_seg(&other)?),
                "angle" => bits(seg.angle(&other)?.as_degrees()),
                "nearest_point_seg" => point(seg.nearest_point_seg(&other)?),
                "nearest_points" => {
                    let mut a = Vector2I::default();
                    let mut b = Vector2I::default();
                    let mut dist = 0_i64;
                    let returned = seg.nearest_points(&other, &mut a, &mut b, &mut dist)?;
                    json!({"returned": returned, "a": point(a), "b": point(b), "dist": dist})
                }
                "intersects" => json!(seg.intersects(&other)?),
                _ => json!(seg.approx_perpendicular(&other)?),
            })
        }
        "intersect" => {
            let has_lines = v.get("lines").is_some();
            let has_ignore = v.get("ignore_endpoints").is_some();
            if has_lines {
                keys(v, &["op", "seg", "other", "ignore_endpoints", "lines"])?;
            } else if has_ignore {
                keys(v, &["op", "seg", "other", "ignore_endpoints"])?;
            } else {
                keys(v, &["op", "seg", "other"])?;
            }
            let seg = read_seg(v, "seg")?;
            let other = read_seg(v, "other")?;
            let ignore = if has_ignore {
                boolean(v, "ignore_endpoints")?
            } else {
                false
            };
            let lines = if has_lines {
                boolean(v, "lines")?
            } else {
                false
            };
            Ok(match seg.intersect(&other, ignore, lines)? {
                Some(p) => point(p),
                None => Value::Null,
            })
        }
        "intersects_line" => {
            keys(v, &["op", "seg", "slope", "offset", "initial"])?;
            let seg = read_seg(v, "seg")?;
            let slope = number(v, "slope")?;
            let offset = number(v, "offset")?;
            let mut intersection = named_point(v, "initial")?;
            let returned = seg.intersects_line(slope, offset, &mut intersection)?;
            Ok(json!({"returned": returned, "intersection": point(intersection)}))
        }
        "collide" => {
            let has = v.get("actual").is_some();
            if has {
                keys(v, &["op", "seg", "other", "clearance", "actual"])?;
            } else {
                keys(v, &["op", "seg", "other", "clearance"])?;
            }
            let seg = read_seg(v, "seg")?;
            let other = read_seg(v, "other")?;
            let clearance = integer(v, "clearance")?;
            if has {
                let mut actual = integer(v, "actual")?;
                let returned = seg.collide(&other, clearance, Some(&mut actual))?;
                Ok(json!({"returned": returned, "actual": actual}))
            } else {
                let returned = seg.collide(&other, clearance, None)?;
                Ok(json!({"returned": returned, "actual": null}))
            }
        }
        "approx_collinear" | "approx_parallel" => {
            let has = v.get("threshold").is_some();
            if has {
                keys(v, &["op", "seg", "other", "threshold"])?;
            } else {
                keys(v, &["op", "seg", "other"])?;
            }
            let seg = read_seg(v, "seg")?;
            let other = read_seg(v, "other")?;
            // The C++ default argument.
            let threshold = if has { integer(v, "threshold")? } else { 1 };
            Ok(json!(if op == "approx_collinear" {
                seg.approx_collinear(&other, threshold)?
            } else {
                seg.approx_parallel(&other, threshold)?
            }))
        }
        _ => Err("unknown operation".into()),
    }
}

pub fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case":case,"error":{"kind":kind,"message":message},"unit":unit});
    let request = match kicad_difftest::request::parse(line) {
        Ok(request) => request,
        Err(_) => {
            return failure(
                unit,
                case,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    if let Some(value) = request.get("unit").filter(|v| v.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|v| v.is_string()) {
        case = value.clone();
    }
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
    match run(&request["input"]) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(Error::Seg(SegError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
        Err(Error::Seg(SegError::Undefined)) => failure(unit, case, "invalid_input", UNDEFINED),
    }
}

#[cfg(test)]
mod tests;
