// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_math::vector::VectorComponent;
use kicad_math::{box2::*, util, Vector2, VectorDomainError};
use serde_json::{json, Value};
type R<T> = Result<T, Error>;
#[derive(Debug)]
enum Error {
    Input(String),
    Domain(VectorDomainError),
}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Input(s)
    }
}
impl From<VectorDomainError> for Error {
    fn from(e: VectorDomainError) -> Self {
        Self::Domain(e)
    }
}
fn text<'a>(v: &'a Value, key: &str) -> R<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Input(format!("expected string: {key}")))
}
fn shape(v: &Value, required: &[&str], optional: &[&str]) -> R<()> {
    let o = v
        .as_object()
        .ok_or_else(|| Error::Input("expected object".into()))?;
    if required.iter().any(|k| !o.contains_key(*k))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(Error::Input("unexpected or missing field".into()));
    }
    Ok(())
}
trait Scalar: Copy {
    fn read(v: &Value) -> R<Self>;
    fn out(self) -> Value;
}
macro_rules! signed {($($t:ty),*)=>{$(impl Scalar for $t{fn read(v:&Value)->R<Self>{v.as_i64().and_then(|x|Self::try_from(x).ok()).ok_or_else(||Error::Input("integer out of range".into()))}fn out(self)->Value{json!(self)}})*};}
signed!(i8, i16, i32, i64);
macro_rules! unsigned {($($t:ty),*)=>{$(impl Scalar for $t{fn read(v:&Value)->R<Self>{v.as_u64().or_else(||v.as_i64().and_then(|x|u64::try_from(x).ok())).and_then(|x|Self::try_from(x).ok()).ok_or_else(||Error::Input("integer out of range".into()))}fn out(self)->Value{json!(self)}})*};}
unsigned!(u8, u16, u32, u64);
impl Scalar for f64 {
    fn read(v: &Value) -> R<Self> {
        let s = v
            .as_str()
            .ok_or_else(|| Error::Input("expected hex".into()))?;
        if s.len() != 16
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Input("invalid hex".into()));
        }
        u64::from_str_radix(s, 16)
            .map(f64::from_bits)
            .map_err(|_| Error::Input("invalid hex".into()))
    }
    fn out(self) -> Value {
        json!(format!("{:016x}", self.to_bits()))
    }
}
impl Scalar for f32 {
    fn read(v: &Value) -> R<Self> {
        let s = v
            .as_str()
            .ok_or_else(|| Error::Input("expected hex".into()))?;
        if s.len() != 8
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Input("invalid hex".into()));
        }
        u32::from_str_radix(s, 16)
            .map(f32::from_bits)
            .map_err(|_| Error::Input("invalid hex".into()))
    }
    fn out(self) -> Value {
        json!(format!("{:08x}", self.to_bits()))
    }
}
impl Scalar for util::LongLong {
    fn read(v: &Value) -> R<Self> {
        Ok(Self(i64::read(v)?))
    }
    fn out(self) -> Value {
        self.0.out()
    }
}
fn point<T: Scalar>(v: &Value) -> R<Vector2<T>> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| Error::Input("expected point".into()))?;
    let mut a = a.iter();
    Ok(Vector2::new(
        T::read(
            a.next()
                .ok_or_else(|| Error::Input("expected point".into()))?,
        )?,
        T::read(
            a.next()
                .ok_or_else(|| Error::Input("expected point".into()))?,
        )?,
    ))
}
fn vec<T: Scalar>(p: Vector2<T>) -> Value {
    json!([p.x.out(), p.y.out()])
}
fn state<T: BoxCoord + Scalar>(b: Box2<T>) -> Value
where
    T::Extended: Scalar,
{
    json!({"pos":vec(b.origin()),"size":vec(b.size()),"valid":b.is_valid()})
}
fn box_read<T: BoxCoord + Scalar>(v: &Value) -> R<Box2<T>>
where
    T::Extended: Scalar,
{
    let mode = text(v, "mode")?;
    match mode {
        "default" => {
            shape(v, &["mode"], &[])?;
            Ok(Box2::default())
        }
        "maximum" => {
            shape(v, &["mode"], &[])?;
            let mut b = Box2::default();
            b.set_maximum();
            Ok(b)
        }
        "at" => {
            shape(v, &["mode", "pos"], &[])?;
            Ok(Box2::at(point(&v["pos"])?)?)
        }
        "new" | "raw" | "center" => {
            shape(v, &["mode", "pos", "size"], &[])?;
            let p = point(&v["pos"])?;
            let s = point(&v["size"])?;
            if mode == "raw" {
                let mut b = Box2::default();
                b.set_origin(p);
                b.set_size(s);
                Ok(b)
            } else if mode == "center" {
                Ok(Box2::by_center(p, s)?)
            } else {
                Ok(Box2::new(p, s)?)
            }
        }
        "corners" => {
            shape(v, &["mode", "pos", "end"], &[])?;
            Ok(Box2::by_corners(point(&v["pos"])?, point(&v["end"])?)?)
        }
        "compute" => {
            shape(v, &["mode", "points"], &[])?;
            let points = v["points"]
                .as_array()
                .ok_or_else(|| Error::Input("expected points".into()))?
                .iter()
                .map(point)
                .collect::<R<Vec<_>>>()?;
            let mut b = Box2::default();
            b.compute(&points)?;
            Ok(b)
        }
        _ => Err(Error::Input("unknown box mode".into())),
    }
}
macro_rules! mutate {
    ($a:ident,$call:expr) => {{
        let original = core::ptr::from_ref(&$a);
        let returned = core::ptr::from_mut($call?);
        let mut result = state($a);
        result["same_ref"] = json!(core::ptr::eq(original, returned));
        result
    }};
}
fn boxes<T: BoxCoord + Scalar>(v: &Value) -> R<Value>
where
    T::Extended: Scalar,
    <T::Extended as VectorComponent>::Extended: Scalar,
{
    let mut a = box_read::<T>(&v["a"])?;
    let k = text(v, "kind")?;
    let required = &["op", "type", "kind", "a"];
    let extras = match k {
        "merge"
        | "contains_box"
        | "intersects"
        | "intersect"
        | "distance_box"
        | "squared_distance_box"
        | "compare" => vec!["b"],
        "rotated" => vec!["b", "angle"],
        "segment" => vec!["p", "q"],
        "circle" => vec!["p", "radius"],
        "circle_edge" => vec!["p", "radius", "width"],
        "inflate" | "inflated" | "set_size" => vec!["s"],
        "inflate_uniform" | "inflated_uniform" | "set_width" | "set_height" => vec!["n"],
        "set_x" | "set_y" => vec!["n"],
        "rotated_bbox" => vec!["p", "angle"],
        "compute" => vec!["points"],
        "merge_point_alias" => vec!["alias"],
        "contains" | "contains_xy" | "merge_point" | "nearest" | "farthest" | "distance"
        | "squared_distance" | "offset" | "offset_xy" | "move" | "with_offset" | "set_end"
        | "set_end_xy" | "set_origin" | "set_origin_xy" => vec!["p"],
        _ => vec![],
    };
    let mut fields = required.to_vec();
    fields.extend(extras);
    shape(v, &fields, &[])?;
    Ok(match k {
        "state" => state(a),
        "normalize" => {
            mutate!(a, a.normalize())
        }
        "maximum" => {
            a.set_maximum();
            state(a)
        }
        "getters" => {
            json!({"origin":vec(a.origin()),"position":vec(a.position()),"end":vec(a.end()?),"center":vec(a.center()?),"centre":vec(a.centre()?),"left":a.left().out(),"top":a.top().out(),"right":a.right()?.out(),"bottom":a.bottom()?.out(),"x":a.x().out(),"y":a.y().out(),"width":a.width().out(),"height":a.height().out(),"size_max":a.size_max()?})
        }
        "contains" => json!(a.contains(point(&v["p"])?)?),
        "contains_xy" => {
            let p = point(&v["p"])?;
            json!(a.contains_xy(p.x, p.y)?)
        }
        "contains_box" => json!(a.contains_box(&box_read(&v["b"])?)?),
        "intersects" => json!(a.intersects(&box_read(&v["b"])?)?),
        "intersect" => state(a.intersect(&box_read(&v["b"])?)?),
        "merge" => {
            mutate!(a, a.merge(&box_read(&v["b"])?))
        }
        "merge_point" => {
            mutate!(a, a.merge_point(point(&v["p"])?))
        }
        "merge_point_alias" => {
            let p = match text(v, "alias")? {
                "origin" => BoxPointOperand::Origin,
                "size" => BoxPointOperand::Size,
                _ => return Err(Error::Input("unknown point alias".into())),
            };
            mutate!(a, a.merge_point_operand(p))
        }
        "compare" => {
            let b = box_read(&v["b"])?;
            json!([a.equals(&b)?, a.not_equals(&b)?])
        }
        "segment" => json!(a.intersects_segment(point(&v["p"])?, point(&v["q"])?)?),
        "rotated" => json!(a.intersects_rotated(
            &box_read(&v["b"])?,
            kicad_math::eda_angle::EdaAngle::degrees(f64::read(&v["angle"])?)
        )?),
        "rotated_bbox" => state(a.bounding_box_rotated(
            point(&v["p"])?,
            kicad_math::eda_angle::EdaAngle::degrees(f64::read(&v["angle"])?),
        )?),
        "circle" => json!(a.intersects_circle(point(&v["p"])?, i32::read(&v["radius"])?)?),
        "circle_edge" => json!(a.intersects_circle_edge(
            point(&v["p"])?,
            i32::read(&v["radius"])?,
            i32::read(&v["width"])?
        )?),
        "nearest" => vec(a.nearest_point(point(&v["p"])?)?),
        "farthest" => vec(a.farthest_point(point(&v["p"])?)?),
        "distance" => a.distance(point(&v["p"])?)?.out(),
        "squared_distance" => a.squared_distance(point(&v["p"])?)?.out(),
        "distance_box" => a.distance_box(&box_read(&v["b"])?)?.out(),
        "squared_distance_box" => a.squared_distance_box(&box_read(&v["b"])?)?.out(),
        "area" => a.area()?.out(),
        "diagonal" => a.diagonal()?.out(),
        "squared_diagonal" => a.squared_diagonal()?.out(),
        "safe" => json!(a.is_safe()?),
        "format" => json!(a.format()?),
        "inflate" => {
            let s = point(&v["s"])?;
            mutate!(a, a.inflate(s.x, s.y))
        }
        "inflated" => {
            let s = point(&v["s"])?;
            state(a.inflated(s.x, s.y)?)
        }
        "inflate_uniform" => {
            mutate!(a, a.inflate_uniform(T::read(&v["n"])?))
        }
        "inflated_uniform" => state(a.inflated_uniform(T::read(&v["n"])?)?),
        "offset" | "move" | "with_offset" | "offset_xy" => {
            let p = point(&v["p"])?;
            match k {
                "offset" => a.offset(p)?,
                "offset_xy" => a.offset_xy(p.x, p.y)?,
                "move" => a.move_by(p)?,
                _ => a = a.with_offset(p)?,
            };
            state(a)
        }
        "set_origin" | "set_origin_xy" => {
            let p = point(&v["p"])?;
            if k == "set_origin" {
                a.set_origin(p)
            } else {
                a.set_origin_xy(p.x, p.y)
            };
            state(a)
        }
        "set_end" | "set_end_xy" => {
            let p = point(&v["p"])?;
            if k == "set_end" {
                a.set_end(p)?
            } else {
                a.set_end_xy(p.x, p.y)?
            };
            state(a)
        }
        "set_size" => {
            a.set_size(point(&v["s"])?);
            state(a)
        }
        "set_x" => {
            a.set_x(T::read(&v["n"])?);
            state(a)
        }
        "set_y" => {
            a.set_y(T::read(&v["n"])?);
            state(a)
        }
        "set_width" => {
            a.set_width(T::Extended::read(&v["n"])?);
            state(a)
        }
        "set_height" => {
            a.set_height(T::Extended::read(&v["n"])?);
            state(a)
        }
        "compute" => {
            let p = v["points"]
                .as_array()
                .ok_or_else(|| Error::Input("expected points".into()))?
                .iter()
                .map(point)
                .collect::<R<Vec<_>>>()?;
            a.compute(&p)?;
            state(a)
        }
        _ => return Err(Error::Input("unknown box operation".into())),
    })
}
fn round<S: util::RoundSource + Scalar>(v: &Value) -> R<Value> {
    let x = S::read(&v["a"])?;
    let quiet = v["quiet"]
        .as_bool()
        .ok_or_else(|| Error::Input("expected bool".into()))?;
    Ok(match text(v, "target")? {
        "i8" => util::ki_round::<_, i8>(x, quiet)?.out(),
        "u8" => util::ki_round::<_, u8>(x, quiet)?.out(),
        "i16" => util::ki_round::<_, i16>(x, quiet)?.out(),
        "u16" => util::ki_round::<_, u16>(x, quiet)?.out(),
        "i32" => util::ki_round::<_, i32>(x, quiet)?.out(),
        "u32" => util::ki_round::<_, u32>(x, quiet)?.out(),
        "i64" => util::ki_round::<_, i64>(x, quiet)?.out(),
        "ll" => util::ki_round::<_, util::LongLong>(x, quiet)?.out(),
        _ => return Err(Error::Input("unknown target".into())),
    })
}
fn integers<T: Scalar + util::SqrtInteger + util::RoundSource + Default + PartialOrd>(
    v: &Value,
) -> R<Value> {
    let a = T::read(&v["a"])?;
    Ok(match text(v, "op")? {
        "round" => return round::<T>(v),
        "sign" => json!(util::sign(a)),
        "isqrt" => util::isqrt(a)?.out(),
        "ct_sqrt" => util::ct_sqrt(a)?.out(),
        "ct_sqrt_helper" => util::ct_sqrt_helper(a, T::read(&v["lo"])?, T::read(&v["hi"])?)?.out(),
        "clamp" => util::clamp(a, T::read(&v["lo"])?, T::read(&v["hi"])?)?.out(),
        _ => return Err(Error::Input("unknown operation".into())),
    })
}
fn run(v: &Value) -> R<Value> {
    let op = text(v, "op")?;
    if op == "box" {
        return match text(v, "type")? {
            "i32" => boxes::<i32>(v),
            "i64" => boxes::<i64>(v),
            "f64" => boxes::<f64>(v),
            _ => Err(Error::Input("unknown box type".into())),
        };
    }
    if op == "minmax" {
        shape(v, &["op", "kind", "a", "b", "p"], &[])?;
        let arr = v["a"]
            .as_array()
            .filter(|a| a.len() == 4)
            .ok_or_else(|| Error::Input("expected edges".into()))?;
        let mut it = arr.iter().map(i32::read);
        let mut a = Box2IMinMax::new(
            it.next().ok_or_else(|| Error::Input("edges".into()))??,
            it.next().ok_or_else(|| Error::Input("edges".into()))??,
            it.next().ok_or_else(|| Error::Input("edges".into()))??,
            it.next().ok_or_else(|| Error::Input("edges".into()))??,
        );
        let p = point(&v["p"])?;
        return Ok(match text(v, "kind")? {
            "default" => {
                a = Box2IMinMax::default();
                json!([a.left, a.top, a.right, a.bottom])
            }
            "center" => vec(a.center()),
            "diameter" => a.diameter()?.out(),
            "box" => state(a.to_box()?),
            "merge" => {
                a.merge(p);
                json!([a.left, a.top, a.right, a.bottom])
            }
            "point" => {
                a = Box2IMinMax::point(p);
                json!([a.left, a.top, a.right, a.bottom])
            }
            "xy" => {
                a = Box2IMinMax::xy(p.x, p.y);
                json!([a.left, a.top, a.right, a.bottom])
            }
            "corners" | "segment" => {
                let q = point(&v["b"])?;
                a = if text(v, "kind")? == "corners" {
                    Box2IMinMax::corners(p, q)
                } else {
                    Box2IMinMax::segment(p, q)
                };
                json!([a.left, a.top, a.right, a.bottom])
            }
            "from_box" => {
                a = Box2IMinMax::from_box(&box_read(&v["b"])?)?;
                json!([a.left, a.top, a.right, a.bottom])
            }
            "arc" => {
                struct PendingArc {
                    _start: kicad_math::Vector2I,
                    _mid: kicad_math::Vector2I,
                }
                impl ArcBoundingBox for PendingArc {
                    fn bbox(&self) -> util::Result<Box2I> {
                        // UNPORTED(libs.kimath.src.geometry.shape_arc.application.ui.p001): SHAPE_ARC::BBox
                        Err(VectorDomainError::Unported {
                            upstream: "SHAPE_ARC::BBox",
                            unit: "libs.kimath.src.geometry.shape_arc.application.ui.p001",
                        })
                    }
                }
                a = Box2IMinMax::from_arc(&PendingArc {
                    _start: p,
                    _mid: point(&v["b"])?,
                })?;
                json!([a.left, a.top, a.right, a.bottom])
            }
            "intersects" => {
                let b = Box2IMinMax::from_box(&box_read(&v["b"])?)?;
                json!(a.intersects(&b))
            }
            _ => return Err(Error::Input("unknown minmax operation".into())),
        });
    }
    if op == "safe_parts" {
        shape(v, &["op", "pos", "size", "type"], &[])?;
        return Ok(match text(v, "type")? {
            "f64" => state(box2i_safe_parts(point(&v["pos"])?, point(&v["size"])?)?),
            "i32" => state(box2i_safe_integral(
                point(&v["pos"])?,
                point::<i32>(&v["size"])?,
            )?),
            "i64" => state(box2i_safe_integral(
                point(&v["pos"])?,
                point::<i64>(&v["size"])?,
            )?),
            _ => return Err(Error::Input("unknown type".into())),
        });
    }
    if op == "safe_box" || op == "round_box" {
        shape(v, &["op", "a"], &[])?;
        let a = box_read(&v["a"])?;
        return Ok(state(if op == "safe_box" {
            box2i_safe(&a)?
        } else {
            ki_round_box(&a)?
        }));
    }
    if op == "round_default" {
        shape(v, &["op", "type", "a"], &[])?;
        return Ok(match text(v, "type")? {
            "f64" => util::ki_round_default(f64::read(&v["a"])?)?.out(),
            "f32" => util::ki_round_default(f32::read(&v["a"])?)?.out(),
            "i32" => util::ki_round_default(i32::read(&v["a"])?)?.out(),
            "i64" => util::ki_round_default(i64::read(&v["a"])?)?.out(),
            "ll" => util::ki_round_default(util::LongLong::read(&v["a"])?)?.out(),
            _ => return Err(Error::Input("unknown type".into())),
        });
    }
    let extra = match op {
        "round" => vec!["target", "quiet"],
        "equals" => vec!["b", "epsilon"],
        "rescale" => vec!["b", "d"],
        "clamp" | "ct_sqrt_helper" => vec!["lo", "hi"],
        "hypot" => vec!["b"],
        _ => vec![],
    };
    let mut fields = vec!["op", "type", "a"];
    fields.extend(extra);
    shape(v, &fields, &[])?;
    if op == "checked_cast" {
        return Ok(util::checked_cast_long_long_to_int(i64::read(&v["a"])?)?.out());
    }
    if op == "abs" {
        return Ok(util::unsigned_abs(i64::read(&v["a"])?).out());
    }
    if op == "hypot" {
        return Ok(util::rounded_hypot(i64::read(&v["a"])?, i64::read(&v["b"])?)?.out());
    }
    let ty = text(v, "type")?;
    if op == "rescale" {
        return Ok(match ty {
            "i32" => util::rescale(
                i32::read(&v["a"])?,
                i32::read(&v["b"])?,
                i32::read(&v["d"])?,
            )?
            .out(),
            "i64" => util::rescale(
                i64::read(&v["a"])?,
                i64::read(&v["b"])?,
                i64::read(&v["d"])?,
            )?
            .out(),
            "f64" => util::rescale(
                f64::read(&v["a"])?,
                f64::read(&v["b"])?,
                f64::read(&v["d"])?,
            )?
            .out(),
            "f32" => util::rescale(
                f32::read(&v["a"])?,
                f32::read(&v["b"])?,
                f32::read(&v["d"])?,
            )?
            .out(),
            _ => return Err(Error::Input("unknown type".into())),
        });
    }
    macro_rules! float {
        ($t:ty,$equals:path) => {{
            let a = <$t>::read(&v["a"])?;
            Ok(match op {
                "round" => return round::<$t>(v),
                "sign" => json!(util::sign(a)),
                "equals" => json!($equals(
                    a,
                    <$t>::read(&v["b"])?,
                    if v["epsilon"].is_null() {
                        None
                    } else {
                        Some(<$t>::read(&v["epsilon"])?)
                    }
                )),
                "clamp" => util::clamp(a, <$t>::read(&v["lo"])?, <$t>::read(&v["hi"])?)?.out(),
                _ => return Err(Error::Input("unknown operation".into())),
            })
        }};
    }
    match ty {
        "f64" => float!(f64, util::equals_f64),
        "f32" => float!(f32, util::equals_f32),
        "i8" => integers::<i8>(v),
        "u8" => integers::<u8>(v),
        "i16" => integers::<i16>(v),
        "u16" => integers::<u16>(v),
        "i32" => integers::<i32>(v),
        "u32" => integers::<u32>(v),
        "i64" => integers::<i64>(v),
        "ll" if op == "round" => round::<util::LongLong>(v),
        "u64" => integers::<u64>(v),
        _ => Err(Error::Input("unknown type".into())),
    }
}
pub fn dispatch(line: &str) -> Value {
    let failure = |u: Value, c: Value, kind: &str, message: &str| json!({"unit":u,"case":c,"error":{"kind":kind,"message":message}});
    let request = match kicad_difftest::request::parse(line) {
        Ok(v) => v,
        Err(_) => {
            return failure(
                Value::Null,
                Value::Null,
                "invalid_request",
                "invalid JSON or duplicate key",
            )
        }
    };
    let u = request
        .get("unit")
        .filter(|v| v.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    let c = request
        .get("case")
        .filter(|v| v.is_string())
        .cloned()
        .unwrap_or(Value::Null);
    if shape(&request, &["unit", "case", "input"], &[]).is_err()
        || u.as_str().is_none_or(str::is_empty)
        || c.as_str().is_none_or(str::is_empty)
        || !request["input"].is_object()
    {
        return failure(u, c, "invalid_request", "expected unit, case, input object");
    }
    if u != util::UNIT {
        return failure(u, c, "unknown_unit", "unit has no registered shim");
    }
    match run(&request["input"]) {
        Ok(result) => json!({"unit":u,"case":c,"result":result}),
        Err(Error::Input(m)) => failure(u, c, "invalid_input", &m),
        Err(Error::Domain(VectorDomainError::Unported { upstream, unit })) => {
            json!({"unit":u,"case":c,"error":{"kind":"unported","upstream":upstream,"unit":unit}})
        }
        Err(Error::Domain(_)) => failure(u, c, "invalid_input", "undefined box/util operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_public_case_returns_or_explicitly_defers() {
        let mut counts = [0, 0];
        for line in
            include_str!("../../../port/cases/libs.kimath.include.math.application.ui.p001.jsonl")
                .lines()
        {
            let r = dispatch(line);
            if r.get("result").is_some() {
                counts[0] += 1;
            } else {
                assert_eq!(r["error"]["kind"], "unported", "{r}");
                counts[1] += 1;
            }
        }
        assert_eq!(counts, [14298, 1607]);
    }
    #[test]
    fn undefined_controls_refuse_and_next_request_survives() -> Result<(), serde_json::Error> {
        for line in include_str!("../../../port/oracle/fixtures/util-box-controls.jsonl").lines() {
            let c: Value = serde_json::from_str(line)?;
            let r = dispatch(&c["request"].to_string());
            assert_eq!(r["error"], c["error"]);
            assert_eq!(dispatch(&json!({"unit":util::UNIT,"case":"after","input":{"op":"abs","type":"i64","a":-1}}).to_string())["result"],json!(1));
        }
        Ok(())
    }
}
