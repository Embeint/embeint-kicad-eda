// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.application.ui.p001` (the inline bodies of trigo.h).
//! It mirrors `port/oracle/shims/trigo_header_p001.cpp`: `int` and `long long` values as JSON
//! integers, `double` values as exact IEEE bits, angles as the bits of their value in degrees,
//! one operation per request, and the same refusals: `invalid_input` "undefined trigo.h
//! operation" where the pinned evaluation is undefined (for the `double` template loops, one
//! that never terminates), and "outside the verified domain" for a template angle whose loops
//! need more than [`PASS_BUDGET`] passes (a harness bound of the oracle, which runs the compiled
//! loops; the Rust port evaluates every such angle).
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::eda_angle::{AngleUnit, EdaAngle};
use kicad_math::trigo::TrigoError;
use kicad_math::trigo_header::{self as header, UNIT};
use kicad_math::{Vector2, Vector2D, Vector2I};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined trigo.h operation";

/// The most passes of the compiled template loops (both loops together) the oracle adapter runs.
pub const PASS_BUDGET: i128 = 1 << 24;

/// Beyond this magnitude a finite `double` angle needs more than [`PASS_BUDGET`] passes: up to
/// `2^57` every pass changes the angle by exactly 3600.
const DOUBLE_COUNTED: f64 = 1_099_511_627_776.0; // 2^40

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
impl From<TrigoError> for Error {
    fn from(e: TrigoError) -> Self {
        match e {
            TrigoError::Undefined => Self::Invalid(UNDEFINED.into()),
            TrigoError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
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
fn get<'a>(v: &'a Map<String, Value>, name: &str) -> Result<&'a Value, String> {
    v.get(name).ok_or_else(|| format!("missing: {name}"))
}
fn read_int(v: &Value) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| "integer out of range".into())
}
fn read_long_long(v: &Value) -> Result<i64, String> {
    v.as_i64().ok_or_else(|| "integer out of range".into())
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
fn pair(v: &Value) -> Result<(&Value, &Value), String> {
    match v.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok((x, y)),
        _ => Err("expected point".into()),
    }
}
fn read_point_i(v: &Value) -> Result<Vector2I, String> {
    let (x, y) = pair(v)?;
    Ok(Vector2::new(read_int(x)?, read_int(y)?))
}
fn read_point_d(v: &Value) -> Result<Vector2D, String> {
    let (x, y) = pair(v)?;
    Ok(Vector2::new(decode_bits(x)?, decode_bits(y)?))
}
/// `EDA_ANGLE( value, DEGREES_T )`.
fn read_angle(v: &Value) -> Result<EdaAngle, String> {
    Ok(EdaAngle::new(decode_bits(v)?, AngleUnit::Degrees))
}
fn point_i(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn point_d(p: Vector2D) -> Value {
    json!([bits(p.x), bits(p.y)])
}
/// The template's type `T`.
enum TemplateType {
    Int,
    LongLong,
    Double,
}
fn read_type(v: &Map<String, Value>) -> Result<TemplateType, String> {
    match text(v, "type")? {
        "int" => Ok(TemplateType::Int),
        "long long" => Ok(TemplateType::LongLong),
        "double" => Ok(TemplateType::Double),
        _ => Err("unknown type".into()),
    }
}

/// The passes of the template loops (both loops together) for the angle `floor + g`, where
/// `floor` is an integer and `g` in `[0, 1)` is nonzero exactly when `fraction`. Each loop runs
/// until the first value past its bound, and every pass before the last changes the angle by
/// exactly 3600 within the counted range, so each count is a quotient:
///
/// * `NormalizeAnglePos`, angle < 0: `ceil( -angle / 3600 )`; angle >= 3600:
///   `floor( angle / 3600 )`. (For a `double` the first loop's last pass can round up to 3600,
///   adding one pass of the second loop; that happens only for an angle above `-2^11`, which
///   takes one pass, so it never decides admission and is not counted.)
/// * `NormalizeAngle180`, angle <= -1800: `floor( ( -1800 - angle ) / 3600 ) + 1`; angle > 1800:
///   `ceil( ( angle - 1800 ) / 3600 )`.
fn template_passes(op: &str, floor: i128, fraction: bool) -> i128 {
    let q = |a: i128| a.div_euclid(3600);
    let f = i128::from(fraction);
    if op == "normalize_angle_180" {
        if floor < -1800 || (floor == -1800 && !fraction) {
            return q(-1800 - floor - f) + 1;
        }
        if floor > 1800 || (floor == 1800 && fraction) {
            return if fraction {
                q(floor - 1800) + 1
            } else {
                -q(1800 - floor)
            };
        }
        return 0;
    }
    if floor < 0 {
        return if fraction {
            q(-floor - 1) + 1
        } else {
            -q(floor)
        };
    }
    if floor >= 3600 {
        return q(floor);
    }
    0
}

/// Whether an integer template angle needs more passes than the adapter runs.
fn over_budget(op: &str, angle: i64) -> bool {
    template_passes(op, i128::from(angle), false) > PASS_BUDGET
}

/// Whether a `double` template angle (one whose loops terminate) needs more passes than the
/// adapter runs. A NaN takes none.
fn over_budget_d(op: &str, angle: f64) -> bool {
    if angle.is_nan() {
        return false;
    }
    if !(-DOUBLE_COUNTED..=DOUBLE_COUNTED).contains(&angle) {
        return true;
    }
    // Exact: |angle| <= 2^40.
    let truncated = angle as i64;
    let floor = truncated - i64::from((truncated as f64) > angle);
    template_passes(op, i128::from(floor), floor as f64 != angle) > PASS_BUDGET
}

const OUTSIDE: &str = "outside the verified domain";

/// One template operation on either instantiation.
fn template<T: header::AngleInt>(op: &str, angle: T) -> T {
    match op {
        "normalize_angle_pos" => header::normalize_angle_pos(angle),
        "normalize_angle_180" => header::normalize_angle_180(angle),
        _ => {
            let mut angle = angle;
            header::normalize_angle_pos_in_place(&mut angle);
            angle
        }
    }
}

/// One template operation on the `double` instantiation.
fn template_d(op: &str, angle: f64) -> Result<f64, TrigoError> {
    match op {
        "normalize_angle_pos" => header::normalize_angle_pos_d(angle),
        "normalize_angle_180" => header::normalize_angle_180_d(angle),
        _ => {
            let mut angle = angle;
            header::normalize_angle_pos_in_place_d(&mut angle)?;
            Ok(angle)
        }
    }
}

fn run(input: &Value) -> Result<Value, Error> {
    let v = object(input)?;
    let op = text(v, "op")?;
    match op {
        "rotate_i" | "get_rotated_i" => {
            keys(v, &["op", "point", "angle"])?;
            let mut p = read_point_i(get(v, "point")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            if op == "get_rotated_i" {
                return Ok(json!({"point": point_i(header::get_rotated(p, angle)?)}));
            }
            header::rotate_point(&mut p, angle)?;
            Ok(json!({"point": point_i(p)}))
        }
        "rotate_i_about" | "get_rotated_i_about" => {
            keys(v, &["op", "point", "center", "angle"])?;
            let mut p = read_point_i(get(v, "point")?)?;
            let c = read_point_i(get(v, "center")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            if op == "get_rotated_i_about" {
                return Ok(json!({"point": point_i(header::get_rotated_about(p, c, angle)?)}));
            }
            header::rotate_point_about(&mut p, c, angle)?;
            Ok(json!({"point": point_i(p)}))
        }
        "rotate_d" | "get_rotated_d" => {
            keys(v, &["op", "point", "angle"])?;
            let mut p = read_point_d(get(v, "point")?)?;
            let angle = read_angle(get(v, "angle")?)?;
            if op == "get_rotated_d" {
                return Ok(json!({"point": point_d(header::get_rotated_d(p, angle))}));
            }
            header::rotate_point_d(&mut p, angle);
            Ok(json!({"point": point_d(p)}))
        }
        "rotate_d_about" => {
            keys(v, &["op", "point", "center", "angle"])?;
            let mut p = read_point_d(get(v, "point")?)?;
            let c = read_point_d(get(v, "center")?)?;
            header::rotate_point_d_about(&mut p, c, read_angle(get(v, "angle")?)?);
            Ok(json!({"point": point_d(p)}))
        }
        "deg2rad" | "rad2deg" | "rad2decideg" => {
            keys(v, &["op", "value"])?;
            let value = decode_bits(get(v, "value")?)?;
            let result = match op {
                "deg2rad" => header::deg2rad(value),
                "rad2deg" => header::rad2deg(value),
                _ => header::rad2decideg(value),
            };
            Ok(json!({"value": bits(result)}))
        }
        "normalize_angle_pos" | "normalize_angle_pos_ref" | "normalize_angle_180" => {
            keys(v, &["op", "type", "angle"])?;
            match read_type(v)? {
                TemplateType::Int => {
                    let angle = read_int(get(v, "angle")?)?;
                    if over_budget(op, i64::from(angle)) {
                        return Err(OUTSIDE.into());
                    }
                    Ok(json!({"angle": template(op, angle)}))
                }
                TemplateType::LongLong => {
                    let angle = read_long_long(get(v, "angle")?)?;
                    if over_budget(op, angle) {
                        return Err(OUTSIDE.into());
                    }
                    Ok(json!({"angle": template(op, angle)}))
                }
                TemplateType::Double => {
                    let angle = decode_bits(get(v, "angle")?)?;
                    // A loop that never terminates is undefined; the port decides that itself.
                    let result = template_d(op, angle)?;
                    if over_budget_d(op, angle) {
                        return Err(OUTSIDE.into());
                    }
                    Ok(json!({"angle": bits(result)}))
                }
            }
        }
        "intercepts_positive_x" | "intercepts_negative_x" => {
            keys(v, &["op", "start", "end"])?;
            let start = decode_bits(get(v, "start")?)?;
            let end = decode_bits(get(v, "end")?)?;
            let value = if op == "intercepts_positive_x" {
                header::intercepts_positive_x(start, end)
            } else {
                header::intercepts_negative_x(start, end)
            };
            Ok(json!({"value": value}))
        }
        _ => Err("unknown operation".into()),
    }
}

/// One request line to one reply value, in the shared shim protocol.
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
#[path = "tests.rs"]
mod tests;
