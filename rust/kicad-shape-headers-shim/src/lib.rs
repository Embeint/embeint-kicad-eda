// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.geometry.application.ui.p003` (the seg.h and shape
//! header inline bodies). It mirrors `port/oracle/shims/kimath_include_geometry_p003.cpp`:
//! the same requests, key validation order and messages, the same "outside the verified domain"
//! refusals before an out-of-line call, and the same `invalid_input` refusal where the pinned
//! evaluation is undefined. Rust deferrals are reported as `unported` with their owner.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use std::rc::Rc;

use kicad_math::circle::Circle;
use kicad_math::eda_angle::EdaAngle;
use kicad_math::seg::{Seg, SegError};
use kicad_math::seg_header::UNIT;
use kicad_math::shape::{
    shape_type_as_string, Shape, ShapeError, ShapeTypeValue, MIN_PRECISION_IU,
};
use kicad_math::shape_arc::{ShapeArc, ARC_HIGH_DEF};
use kicad_math::shape_circle::ShapeCircle;
use kicad_math::shape_compound::ShapeCompound;
use kicad_math::shape_ellipse::ShapeEllipse;
use kicad_math::{Box2I, FmtFlags, StreamInsert, StreamState, Vector2, Vector2I};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined p003 operation";
const OUTSIDE: &str = "outside the verified domain";
/// The verified domain of the out-of-line calls (see the C++ adapter).
const DOMAIN: i64 = 1 << 20;

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
impl From<ShapeError> for Error {
    fn from(e: ShapeError) -> Self {
        match e {
            ShapeError::Undefined => Self::Invalid(UNDEFINED.into()),
            ShapeError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}
impl From<SegError> for Error {
    fn from(e: SegError) -> Self {
        match e {
            SegError::Undefined => Self::Invalid(UNDEFINED.into()),
            SegError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}
type Result<T> = core::result::Result<T, Error>;

fn outside<T>() -> Result<T> {
    Err(OUTSIDE.into())
}

// Decoding and encoding ------------------------------------------------------------------------

fn object(v: &Value) -> Result<&Map<String, Value>> {
    Ok(v.as_object().ok_or("expected object")?)
}
fn keys(v: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let o = object(v)?;
    if required.iter().any(|f| !o.contains_key(*f))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn text<'a>(v: &'a Value, name: &str) -> Result<&'a str> {
    Ok(v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected string: {name}"))?)
}
fn boolean(v: &Value, name: &str) -> Result<bool> {
    Ok(v.get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("expected bool: {name}"))?)
}
fn int_value(v: &Value) -> Result<i32> {
    Ok(v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or("integer out of range")?)
}
fn int(v: &Value, name: &str) -> Result<i32> {
    int_value(v.get(name).ok_or("unexpected or missing field")?)
}
fn angle(v: &Value, name: &str) -> Result<EdaAngle> {
    let t = text(v, name)?;
    if t.len() != 16 || !t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(format!("invalid hex: {name}").into());
    }
    let value = u64::from_str_radix(t, 16)
        .map(f64::from_bits)
        .map_err(|_| format!("invalid hex: {name}"))?;
    Ok(EdaAngle::degrees(value))
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn point_value(v: &Value) -> Result<Vector2I> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    Ok(Vector2::new(int_value(&a[0])?, int_value(&a[1])?))
}
fn point(v: &Value, name: &str) -> Result<Vector2I> {
    point_value(v.get(name).ok_or("unexpected or missing field")?)
}
fn seg(v: &Value, name: &str) -> Result<Seg> {
    let a = v
        .get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .ok_or_else(|| format!("expected segment: {name}"))?;
    Ok(Seg::new(point_value(&a[0])?, point_value(&a[1])?))
}
/// `{"flags": [...], "precision": n, "width": n, "fill": "c"}` as the C++ adapter's
/// `applyStreamState` reads it: each flag named once, precision and width within [-64, 512], a
/// printable ASCII fill.
fn stream_state(v: &Value) -> Result<StreamState> {
    const ERROR: &str = "expected stream state";
    let o = v.as_object().ok_or(ERROR)?;
    if o.len() != 4
        || ["flags", "precision", "width", "fill"]
            .iter()
            .any(|k| !o.contains_key(*k))
    {
        return Err(ERROR.into());
    }
    let names = v["flags"].as_array().ok_or(ERROR)?;
    let mut flags = FmtFlags::default();
    let mut seen: Vec<&str> = Vec::new();
    for name in names {
        let name = name.as_str().ok_or(ERROR)?;
        if seen.contains(&name) {
            return Err(ERROR.into());
        }
        seen.push(name);
        let flag = match name {
            "boolalpha" => &mut flags.boolalpha,
            "dec" => &mut flags.dec,
            "fixed" => &mut flags.fixed,
            "hex" => &mut flags.hex,
            "internal" => &mut flags.internal,
            "left" => &mut flags.left,
            "oct" => &mut flags.oct,
            "right" => &mut flags.right,
            "scientific" => &mut flags.scientific,
            "showbase" => &mut flags.showbase,
            "showpoint" => &mut flags.showpoint,
            "showpos" => &mut flags.showpos,
            "skipws" => &mut flags.skipws,
            "unitbuf" => &mut flags.unitbuf,
            "uppercase" => &mut flags.uppercase,
            _ => return Err(ERROR.into()),
        };
        *flag = true;
    }
    let bounded = |name: &str| -> Result<i64> {
        Ok(v[name]
            .as_i64()
            .filter(|n| (-64..=512).contains(n))
            .ok_or(ERROR)?)
    };
    let precision = bounded("precision")?;
    let width = bounded("width")?;
    let fill = v["fill"]
        .as_str()
        .filter(|t| t.len() == 1 && t.bytes().all(|c| (0x20..0x7f).contains(&c)))
        .and_then(|t| t.chars().next())
        .ok_or(ERROR)?;
    Ok(StreamState {
        flags,
        precision,
        width,
        fill,
    })
}
fn array<'a>(v: &'a Value, name: &str) -> Result<&'a Vec<Value>> {
    Ok(v.get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected array: {name}"))?)
}
fn pt(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn segment(s: &Seg) -> Value {
    json!({"a": pt(s.a), "b": pt(s.b), "index": s.index()})
}

// Verified domains -------------------------------------------------------------------------------

fn within(v: i64) -> bool {
    (-DOMAIN..=DOMAIN).contains(&v)
}
fn within_point(p: Vector2I) -> bool {
    within(i64::from(p.x)) && within(i64::from(p.y))
}
fn circle_domain(shape: &dyn Shape) -> Result<()> {
    let Some(c) = shape.as_any().downcast_ref::<ShapeCircle>() else {
        return outside();
    };
    if !within_point(c.get_center()) || !(0..=DOMAIN).contains(&i64::from(c.get_radius())) {
        return outside();
    }
    Ok(())
}
fn compound_domain(compound: &ShapeCompound) -> Result<()> {
    for shape in compound.shapes() {
        match shape {
            Some(shape) => circle_domain(shape.as_ref())?,
            None => return outside(),
        }
    }
    Ok(())
}
fn arc_domain(start: Vector2I, mid: Vector2I, end: Vector2I, width: i32) -> Result<()> {
    if !within_point(start) || !within_point(mid) || !within_point(end) || !within(width.into()) {
        return outside();
    }
    let d = |a: Vector2I, b: Vector2I| {
        (
            i128::from(b.x) - i128::from(a.x),
            i128::from(b.y) - i128::from(a.y),
        )
    };
    let (d1x, d1y) = d(start, mid);
    let (d2x, d2y) = d(mid, end);
    let n1 = d1x * d1x + d1y * d1y;
    let n2 = d2x * d2x + d2y * d2y;
    let cross = d1x * d2y - d1y * d2x;
    if start == end || n1 == 0 || n2 == 0 || cross * cross * 64 < n1 * n2 {
        return outside();
    }
    Ok(())
}
/// The domain of an out-of-line call on a shape that is not a circle: a compound of circles.
fn out_of_line_domain(shape: &Built) -> Result<()> {
    match shape {
        Built::Compound(c) => compound_domain(c),
        _ => outside(),
    }
}

// Shapes ------------------------------------------------------------------------------------------

/// A built shape, by concrete class.
enum Built {
    Circle(ShapeCircle),
    Arc(ShapeArc),
    Compound(ShapeCompound),
    Ellipse(ShapeEllipse),
}

impl Built {
    fn shape(&self) -> &dyn Shape {
        match self {
            Built::Circle(s) => s,
            Built::Arc(s) => s,
            Built::Compound(s) => s,
            Built::Ellipse(s) => s,
        }
    }
    fn shape_mut(&mut self) -> &mut dyn Shape {
        match self {
            Built::Circle(s) => s,
            Built::Arc(s) => s,
            Built::Compound(s) => s,
            Built::Ellipse(s) => s,
        }
    }
    fn boxed(self) -> Box<dyn Shape> {
        match self {
            Built::Circle(s) => Box::new(s),
            Built::Arc(s) => Box::new(s),
            Built::Compound(s) => Box::new(s),
            Built::Ellipse(s) => Box::new(s),
        }
    }
    fn shared(self) -> Rc<dyn Shape> {
        match self {
            Built::Circle(s) => Rc::new(s),
            Built::Arc(s) => Rc::new(s),
            Built::Compound(s) => Rc::new(s),
            Built::Ellipse(s) => Rc::new(s),
        }
    }
}

fn state(shape: &dyn Shape) -> Result<Value> {
    let any = shape.as_any();
    if let Some(c) = any.downcast_ref::<ShapeCircle>() {
        return Ok(
            json!({"type": "SH_CIRCLE", "center": pt(c.get_center()), "radius": c.get_radius()}),
        );
    }
    if let Some(a) = any.downcast_ref::<ShapeArc>() {
        return Ok(
            json!({"type": "SH_ARC", "start": pt(a.get_p0()), "mid": pt(a.get_arc_mid()),
                         "end": pt(a.get_p1()), "width": a.get_width()}),
        );
    }
    if let Some(c) = any.downcast_ref::<ShapeCompound>() {
        let shapes = c
            .shapes()
            .iter()
            .map(|s| optional_state(s.as_deref()))
            .collect::<Result<Vec<_>>>()?;
        return Ok(json!({"type": "SH_COMPOUND", "shapes": shapes}));
    }
    if let Some(e) = any.downcast_ref::<ShapeEllipse>() {
        return Ok(json!({"type": "SH_ELLIPSE", "center": pt(e.get_center()),
                         "major": e.get_major_radius(), "minor": e.get_minor_radius(),
                         "rotation": bits(e.get_rotation().as_degrees()),
                         "start": bits(e.get_start_angle().as_degrees()),
                         "end": bits(e.get_end_angle().as_degrees()), "is_arc": e.is_arc()}));
    }
    Err("unknown shape".into())
}

fn optional_state(shape: Option<&dyn Shape>) -> Result<Value> {
    shape.map_or(Ok(Value::Null), state)
}

fn add_shape(compound: &mut ShapeCompound, item: &Value, is_call: bool) -> Result<()> {
    if is_call {
        keys(item, &["call", "shape", "shared"], &[])?;
    } else {
        keys(item, &["shape", "shared"], &[])?;
    }
    let spec = item.get("shape").ok_or("unexpected or missing field")?;
    // A null "shape" is a null SHAPE*: stored by the raw overload, refused as undefined by the
    // shared one (which dereferences it).
    if spec.is_null() {
        if boolean(item, "shared")? {
            compound.add_shape_shared(None)?;
        } else {
            compound.add_shape(None)?;
        }
        return Ok(());
    }
    let shape = build(spec)?;
    if boolean(item, "shared")? {
        compound.add_shape_shared(Some(shape.shared()))?;
    } else {
        compound.add_shape(Some(shape.boxed()))?;
    }
    Ok(())
}

fn build(spec: &Value) -> Result<Built> {
    object(spec)?;
    let kind = text(spec, "kind")?;
    if kind == "compound" {
        keys(spec, &["kind", "add"], &[])?;
        let mut compound = ShapeCompound::new();
        for item in array(spec, "add")? {
            add_shape(&mut compound, item, false)?;
        }
        return Ok(Built::Compound(compound));
    }
    let ctor = text(spec, "ctor")?;
    match (kind, ctor) {
        ("circle", "default") => {
            keys(spec, &["kind", "ctor"], &[])?;
            Ok(Built::Circle(ShapeCircle::new()))
        }
        ("circle", _) => {
            keys(spec, &["kind", "ctor", "center", "radius"], &[])?;
            let center = point(spec, "center")?;
            let radius = int(spec, "radius")?;
            match ctor {
                "center_radius" => Ok(Built::Circle(ShapeCircle::with_center_radius(
                    center, radius,
                ))),
                "circle" => Ok(Built::Circle(ShapeCircle::from_circle(
                    &Circle::with_center_radius(center, radius),
                ))),
                "copy" => Ok(Built::Circle(ShapeCircle::copy_of(
                    &ShapeCircle::with_center_radius(center, radius),
                ))),
                _ => Err("unknown constructor".into()),
            }
        }
        ("arc", "default") => {
            keys(spec, &["kind", "ctor"], &[])?;
            Ok(Built::Arc(ShapeArc::new()))
        }
        ("arc", "three_point") => {
            keys(spec, &["kind", "ctor", "start", "mid", "end", "width"], &[])?;
            let start = point(spec, "start")?;
            let mid = point(spec, "mid")?;
            let end = point(spec, "end")?;
            let width = int(spec, "width")?;
            arc_domain(start, mid, end, width)?;
            Ok(Built::Arc(ShapeArc::from_three_points(
                start, mid, end, width,
            )?))
        }
        ("ellipse", "default") => {
            keys(spec, &["kind", "ctor"], &[])?;
            Ok(Built::Ellipse(ShapeEllipse::new()?))
        }
        ("ellipse", "radii" | "arc_radii") => {
            if ctor == "radii" {
                keys(
                    spec,
                    &["kind", "ctor", "center", "major", "minor", "rotation"],
                    &[],
                )?;
            } else {
                keys(
                    spec,
                    &[
                        "kind", "ctor", "center", "major", "minor", "rotation", "start", "end",
                    ],
                    &[],
                )?;
            }
            let center = point(spec, "center")?;
            let major = int(spec, "major")?;
            let minor = int(spec, "minor")?;
            let rotation = angle(spec, "rotation")?;
            if ctor == "radii" {
                return Ok(Built::Ellipse(ShapeEllipse::from_radii(
                    center, major, minor, rotation,
                )?));
            }
            let start = angle(spec, "start")?;
            let end = angle(spec, "end")?;
            Ok(Built::Ellipse(ShapeEllipse::arc_from_radii(
                center, major, minor, rotation, start, end,
            )?))
        }
        ("arc" | "ellipse", _) => Err("unknown constructor".into()),
        _ => Err("unknown shape".into()),
    }
}

fn box_json(b: &Box2I) -> Value {
    let size = b.size();
    json!({"pos": pt(b.position()), "size": [size.x, size.y]})
}

fn is_default_arc(arc: &ShapeArc) -> bool {
    let zero = Vector2::new(0, 0);
    arc.get_p0() == zero && arc.get_arc_mid() == zero && arc.get_p1() == zero
}

/// The optional `clearance`, `actual` and `location` arguments of a Collide call.
struct CollideArgs {
    clearance: i32,
    actual: Option<i32>,
    location: Option<Vector2I>,
}

fn collide_args(call: &Value) -> Result<CollideArgs> {
    // An omitted clearance is the C++ default of 0.
    let clearance = match call.get("clearance") {
        Some(_) => int(call, "clearance")?,
        None => 0,
    };
    let actual = match call.get("actual") {
        Some(_) => Some(int(call, "actual")?),
        None => None,
    };
    let location = match call.get("location") {
        Some(_) => Some(point(call, "location")?),
        None => None,
    };
    Ok(CollideArgs {
        clearance,
        actual,
        location,
    })
}

fn collide_reply(returned: bool, args: &CollideArgs) -> Value {
    json!({"returned": returned, "actual": args.actual, "location": args.location.map(pt)})
}

fn collide(shape: &Built, call: &Value, is_point: bool) -> Result<Value> {
    if is_point {
        keys(
            call,
            &["call", "point"],
            &["clearance", "actual", "location"],
        )?;
    } else {
        keys(call, &["call", "seg"], &["clearance", "actual", "location"])?;
    }
    let s = if is_point {
        let p = point(call, "point")?;
        Seg::new(p, p)
    } else {
        seg(call, "seg")?
    };
    let mut args = collide_args(call)?;
    if !matches!(shape, Built::Circle(_)) {
        out_of_line_domain(shape)?;
        if !within(args.clearance.into()) || !within_point(s.a) || !within_point(s.b) {
            return outside();
        }
    }
    // Omitted pointers are the C++ default nullptr.
    let target = shape.shape();
    let returned = if is_point {
        target.collide_point(
            s.a,
            args.clearance,
            args.actual.as_mut(),
            args.location.as_mut(),
        )?
    } else {
        target.collide_seg(
            &s,
            args.clearance,
            args.actual.as_mut(),
            args.location.as_mut(),
        )?
    };
    Ok(collide_reply(returned, &args))
}

fn apply_call(built: &mut Built, call: &Value) -> Result<Value> {
    let name = text(call, "call")?;
    let plain = || keys(call, &["call"], &[]);

    // SHAPE_BASE and SHAPE members, through the trait.
    match name {
        "type" => {
            plain()?;
            return Ok(json!(built.shape().shape_type().value()));
        }
        "type_name" => {
            plain()?;
            return Ok(json!(built.shape().type_name()));
        }
        "is_null" => {
            plain()?;
            return Ok(json!(built.shape().is_null()));
        }
        "has_indexable_subshapes" => {
            plain()?;
            return Ok(json!(built.shape().has_indexable_subshapes()));
        }
        "indexable_subshape_count" => {
            plain()?;
            return Ok(json!(built.shape().get_indexable_subshape_count()));
        }
        "indexable_subshapes" => {
            keys(call, &["call", "prefill"], &[])?;
            let prefill = usize::try_from(int(call, "prefill")?)
                .ok()
                .filter(|n| *n <= 8)
                .ok_or("integer out of range")?;
            let mut subshapes: Vec<Option<&dyn Shape>> = vec![None; prefill];
            built.shape().get_indexable_subshapes(&mut subshapes);
            let out = subshapes
                .into_iter()
                .map(optional_state)
                .collect::<Result<Vec<_>>>()?;
            return Ok(Value::Array(out));
        }
        "get_start" => {
            plain()?;
            return Ok(pt(built.shape().get_start()));
        }
        "get_end" => {
            plain()?;
            return Ok(pt(built.shape().get_end()));
        }
        "get_width" => {
            plain()?;
            return Ok(json!(built.shape().get_width()));
        }
        "set_width" => {
            keys(call, &["call", "width"], &[])?;
            built.shape_mut().set_width(int(call, "width")?);
            return Ok(Value::Null);
        }
        "is_solid" => {
            plain()?;
            if let Built::Compound(c) = built {
                compound_domain(c)?;
            }
            return Ok(json!(built.shape().is_solid()?));
        }
        "clone" => {
            plain()?;
            let copy = built.shape().clone_shape()?;
            return optional_state(copy.as_deref());
        }
        "bbox" | "centre" => {
            if name == "bbox" {
                keys(call, &["call"], &["clearance"])?;
            } else {
                plain()?;
            }
            let clearance = match call.get("clearance") {
                Some(_) => int(call, "clearance")?,
                None => 0,
            };
            match built {
                Built::Circle(_) => {}
                Built::Arc(a) => {
                    if !is_default_arc(a)
                        || !within(a.get_width().into())
                        || !within(clearance.into())
                    {
                        return outside();
                    }
                }
                _ => {
                    out_of_line_domain(built)?;
                    if !within(clearance.into()) {
                        return outside();
                    }
                }
            }
            if name == "centre" {
                return Ok(pt(built.shape().centre()?));
            }
            return Ok(box_json(&built.shape().bbox(clearance)?));
        }
        "collide_seg" | "collide_point" => return collide(built, call, name == "collide_point"),
        "collide_shape" => {
            keys(
                call,
                &["call", "other"],
                &["clearance", "actual", "location"],
            )?;
            let other = build(call.get("other").ok_or("unexpected or missing field")?)?;
            let mut args = collide_args(call)?;
            match built {
                Built::Circle(c) => circle_domain(c)?,
                _ => out_of_line_domain(built)?,
            }
            if !matches!(other, Built::Circle(_))
                || !(0..=DOMAIN).contains(&i64::from(args.clearance))
            {
                return outside();
            }
            circle_domain(other.shape())?;
            let returned = built.shape().collide_shape(
                other.shape(),
                args.clearance,
                args.actual.as_mut(),
                args.location.as_mut(),
            )?;
            return Ok(collide_reply(returned, &args));
        }
        "move" => {
            keys(call, &["call", "vector"], &[])?;
            let vector = point(call, "vector")?;
            if !matches!(built, Built::Circle(_)) {
                out_of_line_domain(built)?;
                if !within_point(vector) {
                    return outside();
                }
            }
            built.shape_mut().move_by(vector)?;
            return Ok(Value::Null);
        }
        "rotate" => {
            keys(call, &["call", "angle"], &["center"])?;
            let a = angle(call, "angle")?;
            let center = match call.get("center") {
                Some(_) => point(call, "center")?,
                None => Vector2::new(0, 0),
            };
            if !matches!(built, Built::Circle(_)) {
                out_of_line_domain(built)?;
                if !within_point(center) {
                    return outside();
                }
            }
            built.shape_mut().rotate(a, center)?;
            return Ok(Value::Null);
        }
        _ => {}
    }

    match built {
        Built::Circle(c) => match name {
            "get_radius" => {
                plain()?;
                Ok(json!(c.get_radius()))
            }
            "get_center" => {
                plain()?;
                Ok(pt(c.get_center()))
            }
            "get_circle" => {
                plain()?;
                let circle = c.get_circle();
                Ok(json!({"center": pt(circle.center), "radius": circle.radius}))
            }
            "set_radius" => {
                keys(call, &["call", "radius"], &[])?;
                c.set_radius(int(call, "radius")?);
                Ok(Value::Null)
            }
            "set_center" => {
                keys(call, &["call", "center"], &[])?;
                c.set_center(point(call, "center")?);
                Ok(Value::Null)
            }
            "assign" => {
                keys(call, &["call", "from"], &[])?;
                let from = build(call.get("from").ok_or("unexpected or missing field")?)?;
                let Built::Circle(from) = from else {
                    return Err("unknown shape".into());
                };
                let target: *const ShapeCircle = c;
                let result: *const ShapeCircle = c.assign_from(&from);
                Ok(json!(core::ptr::eq(result, target)))
            }
            _ => Err("unknown call".into()),
        },
        Built::Arc(a) => match name {
            "get_p0" => {
                plain()?;
                Ok(pt(a.get_p0()))
            }
            "get_p1" => {
                plain()?;
                Ok(pt(a.get_p1()))
            }
            "get_arc_mid" => {
                plain()?;
                Ok(pt(a.get_arc_mid()))
            }
            "get_chord" => {
                plain()?;
                Ok(segment(&a.get_chord()))
            }
            "is_ccw" => {
                plain()?;
                Ok(json!(a.is_ccw()?))
            }
            "is_clockwise" => {
                plain()?;
                Ok(json!(a.is_clockwise()?))
            }
            "equals" => {
                keys(call, &["call", "other"], &["other_width"])?;
                let other = build(call.get("other").ok_or("unexpected or missing field")?)?;
                let Built::Arc(mut other) = other else {
                    return Err("unknown shape".into());
                };
                if call.get("other_width").is_some() {
                    other.set_width(int(call, "other_width")?);
                }
                Ok(json!({"this": *a == other, "other": other == *a}))
            }
            _ => Err("unknown call".into()),
        },
        Built::Compound(c) => match name {
            "add_shape" => {
                add_shape(c, call, true)?;
                Ok(Value::Null)
            }
            "empty" => {
                plain()?;
                Ok(json!(c.empty()))
            }
            "size" => {
                plain()?;
                Ok(json!(c.size()))
            }
            "unique_subshape" => {
                plain()?;
                optional_state(c.unique_subshape())
            }
            "shapes" => {
                plain()?;
                let out = c
                    .shapes()
                    .iter()
                    .map(|s| optional_state(s.as_deref()))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::Array(out))
            }
            _ => Err("unknown call".into()),
        },
        Built::Ellipse(e) => match name {
            "get_center" => {
                plain()?;
                Ok(pt(e.get_center()))
            }
            "get_major_radius" => {
                plain()?;
                Ok(json!(e.get_major_radius()))
            }
            "get_minor_radius" => {
                plain()?;
                Ok(json!(e.get_minor_radius()))
            }
            "get_rotation" => {
                plain()?;
                Ok(bits(e.get_rotation().as_degrees()))
            }
            "get_start_angle" => {
                plain()?;
                Ok(bits(e.get_start_angle().as_degrees()))
            }
            "get_end_angle" => {
                plain()?;
                Ok(bits(e.get_end_angle().as_degrees()))
            }
            "is_arc" => {
                plain()?;
                Ok(json!(e.is_arc()))
            }
            "equals" => {
                keys(call, &["call", "other"], &[])?;
                let other = build(call.get("other").ok_or("unexpected or missing field")?)?;
                let Built::Ellipse(other) = other else {
                    return Err("unknown shape".into());
                };
                Ok(json!({"this": *e == other, "other": other == *e}))
            }
            "assign" => {
                keys(call, &["call", "from"], &[])?;
                let from = build(call.get("from").ok_or("unexpected or missing field")?)?;
                let Built::Ellipse(from) = from else {
                    return Err("unknown shape".into());
                };
                let target: *const ShapeEllipse = e;
                let result: *const ShapeEllipse = e.assign_from(&from);
                Ok(json!(core::ptr::eq(result, target)))
            }
            _ => Err("unknown call".into()),
        },
    }
}

fn run(v: &Value) -> Result<Value> {
    let op = text(v, "op")?;
    match op {
        "shape" => {
            keys(v, &["op", "shape", "calls"], &[])?;
            let mut built = build(v.get("shape").ok_or("unexpected or missing field")?)?;
            let mut replies = Vec::new();
            for call in array(v, "calls")? {
                if !call.is_object() {
                    return Err("expected object".into());
                }
                replies.push(apply_call(&mut built, call)?);
            }
            Ok(json!({"calls": replies, "state": state(built.shape())?}))
        }
        "constants" => {
            keys(v, &["op"], &[])?;
            Ok(
                json!({"min_precision_iu": MIN_PRECISION_IU, "arc_high_def": ARC_HIGH_DEF,
                      "default_accuracy_for_pcb": ShapeArc::default_accuracy_for_pcb()}),
            )
        }
        "shape_type_as_string" => {
            keys(v, &["op", "value"], &[])?;
            let value = int(v, "value")?;
            // Outside the enumeration's value range 0 to 15 the conversion is undefined.
            let t = ShapeTypeValue::new(value).ok_or(UNDEFINED)?;
            Ok(json!(shape_type_as_string(t)))
        }
        "seg_new" => {
            let ctor = text(v, "ctor")?;
            match ctor {
                "default" => {
                    keys(v, &["op", "ctor"], &[])?;
                    Ok(segment(&Seg::default()))
                }
                "coords" => {
                    keys(v, &["op", "ctor", "coords"], &[])?;
                    let c = array(v, "coords")?;
                    if c.len() != 4 {
                        return Err("expected array: coords".into());
                    }
                    Ok(segment(&Seg::from_coords(
                        int_value(&c[0])?,
                        int_value(&c[1])?,
                        int_value(&c[2])?,
                        int_value(&c[3])?,
                    )))
                }
                "points" => {
                    keys(v, &["op", "ctor", "seg"], &[])?;
                    let s = seg(v, "seg")?;
                    Ok(segment(&Seg::new(s.a, s.b)))
                }
                "indexed" => {
                    keys(v, &["op", "ctor", "seg", "index"], &[])?;
                    let s = seg(v, "seg")?;
                    Ok(segment(&Seg::with_index(s.a, s.b, int(v, "index")?)))
                }
                _ => Err("unknown constructor".into()),
            }
        }
        "seg_copy" => {
            keys(v, &["op", "seg", "index"], &["assign_to"])?;
            let s0 = seg(v, "seg")?;
            let source = Seg::with_index(s0.a, s0.b, int(v, "index")?);
            let Some(target) = v.get("assign_to") else {
                // The copy constructor copies A, B and m_index.
                let copy = source;
                return Ok(segment(&copy));
            };
            keys(target, &["seg", "index"], &[])?;
            let t0 = seg(target, "seg")?;
            let mut assigned = Seg::with_index(t0.a, t0.b, int(target, "index")?);
            // operator= copies all three members.
            assigned.clone_from(&source);
            Ok(segment(&assigned))
        }
        "seg_reverse" => {
            keys(v, &["op", "seg", "index"], &[])?;
            let s0 = seg(v, "seg")?;
            let mut s = Seg::with_index(s0.a, s0.b, int(v, "index")?);
            s.reverse();
            Ok(segment(&s))
        }
        "seg_square" => {
            keys(v, &["op", "value"], &[])?;
            Ok(json!(Seg::square(int(v, "value")?)))
        }
        "seg_canonical" | "seg_length" | "seg_squared_length" | "seg_center" | "seg_stream"
        | "seg_reversed" => {
            if op == "seg_stream" {
                keys(v, &["op", "seg"], &["state"])?;
            } else {
                keys(v, &["op", "seg"], &[])?;
            }
            let s = seg(v, "seg")?;
            Ok(match op {
                "seg_canonical" => {
                    let (a, b, c) = s.canonical_coefs()?;
                    json!([a, b, c])
                }
                "seg_length" => json!(s.length()?),
                "seg_squared_length" => json!(s.squared_length()?),
                "seg_center" => pt(s.center()?),
                "seg_stream" => {
                    // Without "state", a fresh stream and the text alone; with it, the caller's
                    // stream state, the text and the width left after the insertion.
                    let Some(state) = v.get("state") else {
                        let text = s
                            .insert(&mut StreamState::default())
                            .map_err(|e| e.to_string())?;
                        return Ok(json!(text));
                    };
                    let mut state = stream_state(state)?;
                    let text = s.insert(&mut state).map_err(|e| e.to_string())?;
                    json!({"text": text, "width_after": state.width})
                }
                _ => segment(&s.reversed()),
            })
        }
        "seg_side" | "seg_tcoef" => {
            keys(v, &["op", "seg", "point"], &[])?;
            let s = seg(v, "seg")?;
            let p = point(v, "point")?;
            Ok(if op == "seg_side" {
                json!(s.side(p)?)
            } else {
                json!(s.tcoef(p)?)
            })
        }
        "seg_compare"
        | "seg_collinear"
        | "seg_overlaps"
        | "seg_contains_seg"
        | "seg_intersect_lines"
        | "seg_approx_overlaps" => {
            if op == "seg_approx_overlaps" {
                keys(v, &["op", "seg", "other"], &["threshold"])?;
            } else {
                keys(v, &["op", "seg", "other"], &[])?;
            }
            let s = seg(v, "seg")?;
            let o = seg(v, "other")?;
            Ok(match op {
                "seg_compare" => {
                    json!({"eq": s == o, "ne": s != o, "lt": s.less(&o), "gt": o.less(&s)})
                }
                "seg_collinear" => json!(s.collinear(&o)?),
                "seg_overlaps" => json!(s.overlaps(&o)?),
                "seg_contains_seg" => json!(s.contains_seg(&o)?),
                "seg_intersect_lines" => s.intersect_lines(&o)?.map_or(Value::Null, pt),
                _ => {
                    // The C++ default threshold is 3.
                    let threshold = match v.get("threshold") {
                        Some(_) => int(v, "threshold")?,
                        None => 3,
                    };
                    json!(s.approx_overlaps(&o, threshold)?)
                }
            })
        }
        _ => Err("unknown operation".into()),
    }
}

/// One protocol line in, one reply out.
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
    if keys(&request, &["unit", "case", "input"], &[]).is_err()
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
mod tests;
