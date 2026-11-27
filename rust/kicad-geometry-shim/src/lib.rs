// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use kicad_math::eda_angle::{UNIT, *};
use kicad_math::geometry_headers::{Ellipse, GridGeometry, GridKind};
use kicad_math::poly_ystripes::{PolyYstripesIndex, PolygonContours};
use kicad_math::{StreamInsert, StreamState, Vector2, Vector2I};
use serde_json::{json, Value};
enum Error {
    Invalid(String),
    Arithmetic(GeometryError),
}
impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Invalid(v)
    }
}
impl From<GeometryError> for Error {
    fn from(v: GeometryError) -> Self {
        Self::Arithmetic(v)
    }
}
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
fn number(v: &Value, name: &str) -> Result<f64, String> {
    let s = text(v, name)?;
    if s.len() != 16
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("invalid hex: {name}"));
    }
    u64::from_str_radix(s, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("invalid hex: {name}"))
}
fn integer(v: &Value, name: &str) -> Result<i32, String> {
    v.get(name)
        .and_then(Value::as_i64)
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| format!("integer out of range: {name}"))
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
/// An ELLIPSE end point as the C++ adapter encodes it: int components as numbers, double
/// components as exact IEEE bits.
trait EllipsePointReply {
    fn reply(self) -> Value;
}
impl EllipsePointReply for Vector2<i32> {
    fn reply(self) -> Value {
        json!([self.x, self.y])
    }
}
impl EllipsePointReply for Vector2<f64> {
    fn reply(self) -> Value {
        json!([bits(self.x), bits(self.y)])
    }
}
fn array<'a>(v: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    v.get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected array: {name}"))
}
fn point(v: &Value) -> Result<Vector2I, String> {
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
fn run(v: &Value) -> Result<Value, Error> {
    let op = text(v, "op")?;
    match op {
        "default" => {
            keys(v, &["op"])?;
            Ok(
                json!({"angle":bits(EdaAngle::default().as_degrees()),"orientation":bits(EdaOrientation::default().get_angle().as_degrees()),"constants":([ANGLE_HORIZONTAL,ANGLE_VERTICAL,FULL_CIRCLE,ANGLE_0,ANGLE_45,ANGLE_90,ANGLE_135,ANGLE_180,ANGLE_270,ANGLE_360].map(|a| bits(a.as_degrees()))),"conversion":bits(EdaAngle::DEGREES_TO_RADIANS)}),
            )
        }
        "construct" => {
            keys(v, &["op", "a", "unit"])?;
            let unit = match text(v, "unit")? {
                "degrees" => AngleUnit::Degrees,
                "radians" => AngleUnit::Radians,
                "tenths" => AngleUnit::TenthsOfDegree,
                _ => return Err("unknown angle unit".to_string().into()),
            };
            Ok(bits(EdaAngle::new(number(v, "a")?, unit).as_degrees()))
        }
        "vector" => {
            keys(v, &["op", "x", "y"])?;
            Ok(bits(
                EdaAngle::from_vector(Vector2::new(number(v, "x")?, number(v, "y")?))?.as_degrees(),
            ))
        }
        "normalize" => {
            keys(v, &["op", "a", "mode"])?;
            let mut a = EdaAngle::degrees(number(v, "a")?);
            let result = match text(v, "mode")? {
                "positive" => a.normalize(),
                "negative" => a.normalize_negative(),
                "90" => a.normalize90(),
                "180" => a.normalize180(),
                "720" => a.normalize720(),
                "copy" => a.normalized(),
                _ => return Err("unknown normalization".to_string().into()),
            };
            Ok(json!({"state":bits(a.as_degrees()),"returned":bits(result.as_degrees())}))
        }
        "unary" => {
            keys(v, &["op", "a", "kind"])?;
            let x = number(v, "a")?;
            let a = EdaAngle::degrees(x);
            Ok(match text(v, "kind")? {
                "sin" => bits(a.sin()),
                "cos" => bits(a.cos()),
                "tan" => bits(a.tan()?),
                "acos" => bits(EdaAngle::arccos(x)?.as_degrees()),
                "asin" => bits(EdaAngle::arcsin(x)?.as_degrees()),
                "atan" => bits(EdaAngle::arctan(x)?.as_degrees()),
                "radians" => bits(a.as_radians()),
                "negate" => bits((-a).as_degrees()),
                "invert" => bits(a.invert().as_degrees()),
                "abs" => bits(a.abs().as_degrees()),
                "predicates" => json!([a.is_zero(), a.is_horizontal(), a.is_vertical()]),
                "tenths" => json!(a.as_tenths_of_degree()?),
                _ => return Err("unknown unary operation".to_string().into()),
            })
        }
        "binary" => {
            keys(v, &["op", "a", "b", "kind"])?;
            let a = EdaAngle::degrees(number(v, "a")?);
            let b = EdaAngle::degrees(number(v, "b")?);
            Ok(match text(v, "kind")? {
                "add" => bits((a + b).as_degrees()),
                "sub" => bits((a - b).as_degrees()),
                "mul" => bits((a * b.as_degrees()).as_degrees()),
                "div" => bits((a / b.as_degrees()).as_degrees()),
                "ratio" => bits(a / b),
                "add_assign" => {
                    let mut x = a;
                    let original = core::ptr::from_ref(&x);
                    let returned = core::ptr::from_mut(x.add_assign(b));
                    json!({"value":bits(x.as_degrees()),"same":core::ptr::eq(original,returned)})
                }
                "sub_assign" => {
                    let mut x = a;
                    let original = core::ptr::from_ref(&x);
                    let returned = core::ptr::from_mut(x.sub_assign(b));
                    json!({"value":bits(x.as_degrees()),"same":core::ptr::eq(original,returned)})
                }
                "reflect" => bits(a.reflect(b).as_degrees()),
                "atan2" => bits(EdaAngle::arctan2(a.as_degrees(), b.as_degrees())?.as_degrees()),
                "compare" => json!([a == b, a != b, a < b, a > b, a <= b, a >= b]),
                "parallel" => json!(a.is_parallel_to(b)?),
                "snapped" => bits(a.snapped(b)?.as_degrees()),
                _ => return Err("unknown binary operation".to_string().into()),
            })
        }
        "round" => {
            keys(v, &["op", "a", "digits"])?;
            Ok(bits(
                EdaAngle::degrees(number(v, "a")?)
                    .round(integer(v, "digits")?)?
                    .as_degrees(),
            ))
        }
        "orientation" => {
            keys(v, &["op", "a", "b", "axis"])?;
            let a = EdaAngle::degrees(number(v, "a")?);
            let b = EdaAngle::degrees(number(v, "b")?);
            let axis = EdaAngle::degrees(number(v, "axis")?);
            let o = EdaOrientation::new(a);
            let p = EdaOrientation::new(b);
            let mut x = o;
            x += b;
            let mut y = o;
            y -= b;
            let mut assigned = EdaOrientation::default();
            assigned.set_angle(a);
            Ok(
                json!({"values":([o.get_angle(),(-o).get_angle(),(o+b).get_angle(),(o-b).get_angle(),(a+p).get_angle(),(a-p).get_angle(),o-p,x.get_angle(),y.get_angle(),assigned.get_angle(),o.reflect(axis).get_angle()].map(|a|bits(a.as_degrees()))),"equal":o==p}),
            )
        }
        "stream" => {
            keys(
                v,
                &[
                    "op",
                    "a",
                    "orientation",
                    "precision",
                    "width",
                    "fill",
                    "format",
                    "showpos",
                    "uppercase",
                ],
            )?;
            let mut state = StreamState {
                precision: i64::from(integer(v, "precision")?),
                width: i64::from(integer(v, "width")?),
                fill: text(v, "fill")?
                    .chars()
                    .next()
                    .ok_or("invalid fill".to_string())?,
                ..StreamState::default()
            };
            let format = text(v, "format")?;
            state.flags.fixed = matches!(format, "fixed" | "hex");
            state.flags.scientific = matches!(format, "scientific" | "hex");
            state.flags.showpos = v["showpos"].as_bool().ok_or("expected bool".to_string())?;
            state.flags.uppercase = v["uppercase"]
                .as_bool()
                .ok_or("expected bool".to_string())?;
            let a = EdaAngle::degrees(number(v, "a")?);
            let result = if v["orientation"]
                .as_bool()
                .ok_or("expected bool".to_string())?
            {
                EdaOrientation::new(a).insert(&mut state)
            } else {
                a.insert(&mut state)
            }
            .map_err(|_| "resource limit".to_string())?;
            Ok(json!({"text":result,"width":state.width}))
        }
        "ellipse" => {
            keys(v, &["op", "type", "endpoint"])?;
            let endpoint = text(v, "endpoint")?;
            macro_rules! ellipse { ($t:ty) => {{ let e=Ellipse::<$t>::default(); match endpoint { "default"=>Ok(json!({"center":[e.center.x,e.center.y],"radii":[e.major_radius,e.minor_radius],"angles":([e.rotation,e.start_angle,e.end_angle].map(|a| bits(a.as_degrees())))})),"start"=>Ok(e.arc_start_point()?.reply()),"end"=>Ok(e.arc_end_point()?.reply()),_=>Err("unknown endpoint".to_string().into()) } }}; }
            match text(v, "type")? {
                "i32" => ellipse!(i32),
                "f64" => ellipse!(f64),
                _ => Err("unknown ellipse type".to_string().into()),
            }
        }
        "grid" => {
            keys(v, &["op", "a", "b"])?;
            fn grid(v: &Value) -> Result<GridGeometry, String> {
                keys(v, &["priority", "kind", "x", "y"])?;
                let p = v["priority"]
                    .as_u64()
                    // Arbitrary-precision JSON preserves integer -0. The C++
                    // unsigned reader accepts signed integer zero as well.
                    .or_else(|| v["priority"].as_i64().and_then(|n| u64::try_from(n).ok()))
                    .and_then(|p| u32::try_from(p).ok())
                    .ok_or("priority out of range")?;
                Ok(GridGeometry {
                    priority: p,
                    kind: match text(v, "kind")? {
                        "cartesian" => GridKind::Cartesian,
                        "polar" => GridKind::Polar,
                        _ => return Err("unknown grid kind".into()),
                    },
                    extent: Vector2::new(number(v, "x")?, number(v, "y")?),
                    ..GridGeometry::default()
                })
            }
            Ok(json!(grid(&v["a"])?.takes_precedence_over(&grid(&v["b"])?)?))
        }
        "poly" => {
            keys(v, &["op", "builds"])?;
            let mut index = PolyYstripesIndex::new();
            let mut results = Vec::new();
            for build in array(v, "builds")? {
                keys(build, &["polygons", "point", "accuracy"])?;
                let mut owned = Vec::new();
                for poly in array(build, "polygons")? {
                    keys(poly, &["outline", "holes"])?;
                    let outline = array(poly, "outline")?
                        .iter()
                        .map(point)
                        .collect::<Result<Vec<_>, _>>()?;
                    let holes = array(poly, "holes")?
                        .iter()
                        .map(|h| {
                            h.as_array()
                                .ok_or("expected hole".to_string())?
                                .iter()
                                .map(point)
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    owned.push((outline, holes));
                }
                let views: Vec<_> = owned
                    .iter()
                    .map(|(outline, holes)| PolygonContours { outline, holes })
                    .collect();
                index.build(&views)?;
                let accuracy = if build["accuracy"].is_null() {
                    None
                } else {
                    Some(integer(build, "accuracy")?)
                };
                results.push(index.contains(point(&build["point"])?, accuracy)?);
            }
            Ok(json!(results))
        }
        _ => Err("unknown operation".to_string().into()),
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
        Err(Error::Arithmetic(GeometryError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
        Err(Error::Arithmetic(error)) => failure(
            unit,
            case,
            "invalid_input",
            match error {
                GeometryError::ResourceLimit => "resource limit",
                _ => "intermediate overflow",
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_public_cases_succeed_or_defer_to_explicit_owners() {
        let mut counts = [0; 2];
        for line in include_str!(
            "../../../port/cases/libs.kimath.include.geometry.application.ui.p001.jsonl"
        )
        .lines()
        {
            let reply = dispatch(line);
            if reply.get("result").is_some() {
                counts[0] += 1;
            } else {
                assert_eq!(reply["error"]["kind"], "unported", "{reply}");
                assert!(reply["error"]["unit"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty()));
                counts[1] += 1;
            }
        }
        assert_eq!(counts, [21055, 1144]);
    }
    #[test]
    fn undefined_controls_keep_real_refusals_and_earlier_dependencies_distinct(
    ) -> Result<(), serde_json::Error> {
        let mut count = 0;
        for line in include_str!("../../../port/oracle/fixtures/geometry-controls.jsonl").lines() {
            let control: Value = serde_json::from_str(line)?;
            let reply = dispatch(&control["request"].to_string());
            let expected = control.get("rust_error").unwrap_or(&control["error"]);
            assert_eq!(&reply["error"], expected, "{}", control["request"]);
            count += 1;
        }
        assert_eq!(count, 7);
        Ok(())
    }
}

#[cfg(test)]
mod priority_tests {
    use super::*;
    #[test]
    fn integer_negative_zero_is_unsigned_zero_but_float_spellings_are_refused(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut counts = [0; 2];
        for line in
            include_str!("../../../port/oracle/fixtures/geometry-priority-controls.jsonl").lines()
        {
            let control: Value = serde_json::from_str(line)?;
            let reply = dispatch(
                control["request_text"]
                    .as_str()
                    .ok_or("missing literal request")?,
            );
            if let Some(expected) = control.get("result") {
                assert_eq!(reply.get("result"), Some(expected), "{reply}");
                counts[0] += 1;
            } else {
                assert_eq!(reply["error"], control["error"]);
                counts[1] += 1;
            }
        }
        assert_eq!(counts, [4, 2]);
        Ok(())
    }
}
