// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.src.geometry.ellipse.application.ui.p001`. It mirrors
//! `port/oracle/shims/kimath_src_ellipse.cpp`: `int` values as JSON integers, `double` values
//! and every angle as exact IEEE bits, the same step sequence on one object, and the same
//! refusal object where the pinned evaluation is undefined.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::eda_angle::{EdaAngle, GeometryError};
use kicad_math::ellipse::{ArcAngles, EllipseCoord, EllipseError, FlipDirection, UNIT};
use kicad_math::geometry_headers::Ellipse;
use kicad_math::Vector2;
use serde_json::{json, Map, Value};

/// Bounds the number of operations applied to one object in a request.
const MAX_STEPS: usize = 8;
const UNDEFINED: &str = "undefined ellipse operation";

enum Error {
    Invalid(String),
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
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

/// The outcome of one evaluated operation: a value, a refusal of an undefined operation, or a
/// deferral that ends the whole case.
enum Outcome {
    Value(Value),
    Undefined,
}
fn ellipse_result<T>(r: Result<T, EllipseError>) -> Result<Option<T>, Error> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(EllipseError::Unported { upstream, unit }) => Err(Error::Unported { upstream, unit }),
        Err(_) => Ok(None),
    }
}
fn geometry_result<T>(r: Result<T, GeometryError>) -> Result<Option<T>, Error> {
    ellipse_result(r.map_err(EllipseError::from))
}

fn keys(v: &Map<String, Value>, fields: &[&str]) -> Result<(), String> {
    if v.len() != fields.len() || fields.iter().any(|f| !v.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn object(v: &Value) -> Result<&Map<String, Value>, String> {
    v.as_object().ok_or_else(|| "expected object".to_string())
}
fn text<'a>(v: &'a Map<String, Value>, name: &str) -> Result<&'a str, String> {
    v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))
}
fn decode_bits(v: &Value) -> Result<f64, String> {
    let s = v.as_str().ok_or("expected hex")?;
    if s.len() != 16 || !s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err("invalid hex".into());
    }
    u64::from_str_radix(s, 16)
        .map(f64::from_bits)
        .map_err(|_| "invalid hex".into())
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

/// The JSON encoding of each instantiation's `NumericType`.
trait Scalar: EllipseCoord {
    fn decode(v: &Value) -> Result<Self, String>;
    fn encode(self) -> Value;
}
impl Scalar for i32 {
    fn decode(v: &Value) -> Result<Self, String> {
        v.as_i64()
            .filter(|_| v.is_i64() || v.is_u64())
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| "integer out of range".into())
    }
    fn encode(self) -> Value {
        json!(self)
    }
}
impl Scalar for f64 {
    fn decode(v: &Value) -> Result<Self, String> {
        decode_bits(v)
    }
    fn encode(self) -> Value {
        bits(self)
    }
}
fn read_scalar<T: Scalar>(v: &Map<String, Value>, name: &str) -> Result<T, String> {
    T::decode(v.get(name).ok_or_else(|| format!("missing: {name}"))?)
}
fn read_point<T: Scalar>(v: &Map<String, Value>, name: &str) -> Result<Vector2<T>, String> {
    let p = v
        .get(name)
        .and_then(Value::as_array)
        .filter(|p| p.len() == 2)
        .ok_or_else(|| format!("expected point: {name}"))?;
    let mut it = p.iter();
    let mut next = || T::decode(it.next().ok_or("expected point")?);
    let x = next()?;
    let y = next()?;
    Ok(Vector2::new(x, y))
}
fn encode_point<T: Scalar>(p: Vector2<T>) -> Value {
    json!([p.x.encode(), p.y.encode()])
}
fn read_angle(v: &Map<String, Value>, name: &str) -> Result<EdaAngle, String> {
    Ok(EdaAngle::degrees(decode_bits(
        v.get(name)
            .ok_or_else(|| format!("expected angle: {name}"))?,
    )?))
}
fn undefined_observation() -> Value {
    json!({"error": {"kind": "invalid_input", "message": UNDEFINED}})
}
fn state<T: Scalar>(e: &Ellipse<T>) -> Value {
    json!({
        "center": encode_point(e.center),
        "radii": [e.major_radius.encode(), e.minor_radius.encode()],
        "angles": [bits(e.rotation.as_degrees()), bits(e.start_angle.as_degrees()),
                   bits(e.end_angle.as_degrees())],
    })
}

/// The constructor form; `None` when the pinned constructor would be undefined.
fn construct<T: Scalar>(v: &Map<String, Value>) -> Result<Option<Ellipse<T>>, Error> {
    let form = text(v, "form")?;
    if form == "default" {
        keys(v, &["form"])?;
        return Ok(Some(Ellipse::default()));
    }
    let has_start = v.contains_key("start");
    let has_end = v.contains_key("end");
    if has_end && !has_start {
        return Err("end angle without start angle".into());
    }
    let with_angles = |base: &[&'static str]| {
        let mut fields = base.to_vec();
        if has_start {
            fields.push("start");
        }
        if has_end {
            fields.push("end");
        }
        fields
    };
    // Field order follows the C++ adapter: keys first, then values in parameter order.
    let read_angles = || -> Result<ArcAngles, String> {
        Ok(if has_end {
            ArcAngles::StartEnd(read_angle(v, "start")?, read_angle(v, "end")?)
        } else if has_start {
            ArcAngles::Start(read_angle(v, "start")?)
        } else {
            ArcAngles::Defaults
        })
    };
    match form {
        "radii" => {
            keys(
                v,
                &with_angles(&["form", "center", "major", "minor", "rotation"]),
            )?;
            let center = read_point::<T>(v, "center")?;
            let major = read_scalar::<T>(v, "major")?;
            let minor = read_scalar::<T>(v, "minor")?;
            let rotation = read_angle(v, "rotation")?;
            let angles = read_angles()?;
            Ok(Some(Ellipse::new(center, major, minor, rotation, angles)))
        }
        "major" => {
            keys(v, &with_angles(&["form", "center", "major", "ratio"]))?;
            let center = read_point::<T>(v, "center")?;
            let major = read_point::<T>(v, "major")?;
            let ratio = decode_bits(v.get("ratio").ok_or("missing: ratio")?)?;
            let angles = read_angles()?;
            ellipse_result(Ellipse::from_major_axis(center, major, ratio, angles))
        }
        _ => Err("unknown constructor form".into()),
    }
}

fn point<T: Scalar>(e: &Ellipse<T>, angle: EdaAngle) -> Result<Outcome, Error> {
    Ok(match geometry_result(e.point_at_angle(angle))? {
        Some(p) => Outcome::Value(encode_point(p)),
        None => Outcome::Undefined,
    })
}

fn step<T: Scalar>(e: &mut Ellipse<T>, s: &Map<String, Value>) -> Result<Outcome, Error> {
    match text(s, "op")? {
        "mirror" => {
            keys(s, &["op", "ref", "direction"])?;
            let reference = read_point::<T>(s, "ref")?;
            let direction = match text(s, "direction")? {
                "left_right" => FlipDirection::LeftRight,
                "top_bottom" => FlipDirection::TopBottom,
                _ => return Err("unknown flip direction".into()),
            };
            Ok(match ellipse_result(e.mirror(reference, direction))? {
                Some(()) => Outcome::Value(state(e)),
                None => Outcome::Undefined,
            })
        }
        "subtended" => {
            keys(s, &["op"])?;
            Ok(Outcome::Value(bits(e.subtended_angle().as_degrees())))
        }
        "point" => {
            keys(s, &["op", "angle"])?;
            point(e, read_angle(s, "angle")?)
        }
        "start_point" => {
            keys(s, &["op"])?;
            Ok(match geometry_result(e.arc_start_point())? {
                Some(p) => Outcome::Value(encode_point(p)),
                None => Outcome::Undefined,
            })
        }
        "end_point" => {
            keys(s, &["op"])?;
            Ok(match geometry_result(e.arc_end_point())? {
                Some(p) => Outcome::Value(encode_point(p)),
                None => Outcome::Undefined,
            })
        }
        "angle_at" => {
            keys(s, &["op", "point"])?;
            let pt = read_point::<T>(s, "point")?;
            Ok(match ellipse_result(e.angle_at_point(pt))? {
                Some(a) => Outcome::Value(bits(a.as_degrees())),
                None => Outcome::Undefined,
            })
        }
        _ => Err("unknown step".into()),
    }
}

fn run_type<T: Scalar>(v: &Map<String, Value>) -> Result<Value, Error> {
    let steps = v
        .get("steps")
        .and_then(Value::as_array)
        .filter(|s| s.len() <= MAX_STEPS)
        .ok_or("steps out of range")?;
    // Every step is an object before any operation, as in the C++ adapter.
    let steps = steps
        .iter()
        .map(|s| s.as_object().ok_or("expected step object"))
        .collect::<Result<Vec<_>, _>>()?;
    let construct_input = v.get("construct").ok_or("missing: construct")?;
    let Some(mut e) = construct::<T>(object(construct_input)?)? else {
        return Ok(json!({"state": undefined_observation(), "steps": []}));
    };
    let initial = state(&e);
    let mut out = Vec::new();
    // Evaluation stops at the first undefined observation.
    for s in steps {
        match step(&mut e, s)? {
            Outcome::Value(value) => out.push(value),
            Outcome::Undefined => {
                out.push(undefined_observation());
                break;
            }
        }
    }
    Ok(json!({"state": initial, "steps": out}))
}

fn run(v: &Value) -> Result<Value, Error> {
    let v = object(v)?;
    keys(v, &["op", "type", "construct", "steps"])?;
    if text(v, "op")? != "ellipse" {
        return Err("unknown operation".into());
    }
    if !v.get("construct").is_some_and(Value::is_object) {
        return Err("expected construct object".into());
    }
    match text(v, "type")? {
        "i32" => run_type::<i32>(v),
        "f64" => run_type::<f64>(v),
        _ => Err("unknown ellipse type".into()),
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
    let shaped = request
        .as_object()
        .is_some_and(|o| keys(o, &["unit", "case", "input"]).is_ok());
    if !shaped
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
        Err(Error::Unported {
            upstream,
            unit: owner,
        }) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CASES: usize = 2224;
    const DEFERRED: usize = 100;
    const CONTROLS: usize = 249;
    fn has_refusal(result: &Value) -> bool {
        result["state"].get("error").is_some()
            || result["steps"]
                .as_array()
                .is_some_and(|s| s.iter().any(|v| v.get("error").is_some()))
    }
    #[test]
    fn public_cases_succeed_or_defer_to_their_actual_owners() {
        let (mut ok, mut deferred) = (0, 0);
        for line in include_str!(
            "../../../port/cases/libs.kimath.src.geometry.ellipse.application.ui.p001.jsonl"
        )
        .lines()
        {
            let reply = dispatch(line);
            if let Some(result) = reply.get("result") {
                assert!(!has_refusal(result), "{reply}");
                ok += 1;
            } else {
                let owner = reply["error"]["unit"].as_str();
                assert_eq!(reply["error"]["kind"], "unported", "{reply}");
                assert_eq!(
                    owner,
                    Some("libs.kimath.src.math.util.application.ui.p001"),
                    "{reply}"
                );
                deferred += 1;
            }
        }
        assert_eq!((ok + deferred, deferred), (CASES, DEFERRED));
    }
    #[test]
    fn controls_refuse_exactly_the_recorded_observation() -> Result<(), serde_json::Error> {
        let mut count = 0;
        for line in include_str!("../../../port/oracle/fixtures/ellipse-controls.jsonl").lines() {
            let control: Value = serde_json::from_str(line)?;
            let reply = dispatch(&control["request"].to_string());
            let refusal = json!({"error": control["error"]});
            let result = &reply["result"];
            if control["undefined"] == "state" {
                assert_eq!(result, &json!({"state": refusal, "steps": []}), "{reply}");
            } else {
                let steps = result["steps"].as_array().cloned().unwrap_or_default();
                let index = control["undefined"].as_u64().map(|i| i as usize);
                assert_eq!(Some(steps.len()), index.map(|i| i + 1), "{reply}");
                assert_eq!(steps.last(), Some(&refusal), "{reply}");
            }
            count += 1;
        }
        assert_eq!(count, CONTROLS);
        Ok(())
    }
    #[test]
    fn malformed_requests_are_refused_before_any_operation() {
        let base = |input: Value| {
            dispatch(&json!({"unit": UNIT, "case": "c", "input": input}).to_string())
        };
        for input in [
            json!({"op": "ellipse", "type": "i32", "construct": {"form": "default"}}),
            json!({"op": "ellipse", "type": "u32", "construct": {"form": "default"}, "steps": []}),
            json!({"op": "ellipse", "type": "i32", "construct": {"form": "radii", "center": [0, 0],
                   "major": 1, "minor": 1, "rotation": "0000000000000000",
                   "end": "0000000000000000"}, "steps": []}),
            json!({"op": "ellipse", "type": "i32", "construct": {"form": "default"},
                   "steps": [1]}),
            json!({"op": "ellipse", "type": "i32", "construct": {"form": "default"},
                   "steps": [{"op": "point", "angle": "nothex"}]}),
            json!({"op": "ellipse", "type": "i32", "construct": {"form": "radii",
                   "center": [0, 2147483648_i64], "major": 1, "minor": 1,
                   "rotation": "0000000000000000"}, "steps": []}),
            json!({"op": "ellipse", "type": "i32", "construct": {"form": "default"},
                   "steps": vec![json!({"op": "subtended"}); 9]}),
        ] {
            assert_eq!(base(input)["error"]["kind"], "invalid_input");
        }
    }
}
