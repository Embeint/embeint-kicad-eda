// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.geometry.application.ui.p004` (the shape_line_chain.h,
//! shape_index.h and shape_index_list.h inline bodies). It mirrors
//! `port/oracle/shims/kimath_include_geometry_p004.cpp`: the same requests, key validation order
//! and messages, and the same "outside the verified domain" refusals before a call into a body
//! whose undefined domain the adapter does not replay. Everything else is the port's own result:
//! undefined evaluations are its `invalid_input` refusals and deferrals are reported as
//! `unported` with their owner.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
use std::cell::RefCell;
use std::rc::Rc;

use kicad_math::box2::Box2I;
use kicad_math::seg::Seg;
use kicad_math::shape::{Shape, ShapeError, ShapeLineChainBase, ShapeType};
use kicad_math::shape_circle::ShapeCircle;
use kicad_math::shape_index::{
    bounding_box, collide, query_callback, shape_functor, IndexForest, ShapeIndex, ShapeIndexError,
    WxCheck,
};
use kicad_math::shape_index_list::{ListIterator, QueryIterator, ShapeIndexList, ShapeItem};
use kicad_math::shape_line_chain::{
    ClipperZValue, CompareOriginDistance, Intersection, ReserveError, ShapeLineChain,
    POINTS_MAX_SIZE, UNIT,
};
use kicad_math::{Vector2, Vector2I};
use kicad_rtree_cow_packed::{Budget, NodeData};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined p004 operation";
const OUTSIDE: &str = "outside the verified domain";
const DOMAIN: i64 = 1 << 20;
/// `ReservePoints` sizes above this (and up to `max_size()`) are defined allocations whose
/// success depends on the host: a harness limit, refused as [`BUDGET`].
const RESERVE_BUDGET: u64 = 1 << 22;
const BUDGET: &str = "harness allocation budget exceeded";
const LISTS: i64 = 2;
const CHAINS: i64 = 4;
const INDICES: i64 = 3;
const ITEMS: usize = 64;
const STEPS: usize = 256;

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
impl From<ShapeIndexError> for Error {
    fn from(e: ShapeIndexError) -> Self {
        match e {
            ShapeIndexError::Undefined => Self::Invalid(UNDEFINED.into()),
            ShapeIndexError::Unported { upstream, unit } => Self::Unported { upstream, unit },
            ShapeIndexError::Tree(e) => Self::Invalid(e.to_string()),
        }
    }
}
impl From<kicad_math::VectorDomainError> for Error {
    fn from(e: kicad_math::VectorDomainError) -> Self {
        ShapeError::from(e).into()
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
fn boolean_or(v: &Value, name: &str, default: bool) -> Result<bool> {
    if v.get(name).is_some() {
        boolean(v, name)
    } else {
        Ok(default)
    }
}
fn i64_value(v: &Value) -> Result<i64> {
    Ok(v.as_i64().ok_or("integer out of range")?)
}
fn int_value(v: &Value) -> Result<i32> {
    Ok(i32::try_from(i64_value(v)?).map_err(|_| "integer out of range")?)
}
fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value> {
    Ok(v.get(name).ok_or("unexpected or missing field")?)
}
fn int(v: &Value, name: &str) -> Result<i32> {
    int_value(field(v, name)?)
}
fn i64_field(v: &Value, name: &str) -> Result<i64> {
    i64_value(field(v, name)?)
}
fn u64_field(v: &Value, name: &str) -> Result<u64> {
    Ok(field(v, name)?.as_u64().ok_or("integer out of range")?)
}
fn slot(v: &Value, name: &str, count: i64) -> Result<usize> {
    let s = int(v, name)?;
    if s < 0 || i64::from(s) >= count {
        return Err("invalid slot".into());
    }
    Ok(s as usize)
}
fn point_value(v: &Value) -> Result<Vector2I> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    Ok(Vector2::new(int_value(&a[0])?, int_value(&a[1])?))
}
fn point(v: &Value, name: &str) -> Result<Vector2I> {
    point_value(field(v, name)?)
}
fn array<'a>(v: &'a Value, name: &str, limit: usize) -> Result<&'a Vec<Value>> {
    Ok(v.get(name)
        .and_then(Value::as_array)
        .filter(|a| a.len() <= limit)
        .ok_or_else(|| format!("expected array: {name}"))?)
}
fn pt(p: Vector2I) -> Value {
    json!([p.x, p.y])
}
fn box_json(b: &Box2I) -> Value {
    let size = b.size();
    json!({"pos": pt(b.position()), "size": [size.x, size.y], "valid": b.is_valid()})
}
fn seg_json(s: &Seg) -> Value {
    json!({"a": pt(s.a), "b": pt(s.b), "index": s.index()})
}

// Verified domains ------------------------------------------------------------------------------

fn within(v: i64, limit: i64) -> bool {
    (-limit..=limit).contains(&v)
}
fn within_point(p: Vector2I, limit: i64) -> bool {
    within(p.x.into(), limit) && within(p.y.into(), limit)
}
/// A circle or an arc-free chain within 2^20 (the C++ `shapeDomain`).
fn shape_domain(shape: &dyn Shape) -> Result<()> {
    if let Some(c) = shape.as_any().downcast_ref::<ShapeCircle>() {
        if !within_point(c.get_center(), DOMAIN) || !(0..=DOMAIN).contains(&c.get_radius().into()) {
            return outside();
        }
        return Ok(());
    }
    let Some(chain) = shape.as_any().downcast_ref::<ShapeLineChain>() else {
        return outside();
    };
    if chain.arc_count() != 0 || !within(chain.width().into(), DOMAIN) {
        return outside();
    }
    if chain.cpoints().iter().any(|p| !within_point(*p, DOMAIN)) {
        return outside();
    }
    Ok(())
}
/// `SHAPE::Collide( const SHAPE*, int )`: circles within 2^20, clearance in [0, 2^20].
fn collide_shape_domain(
    shape: &dyn Shape,
    other: Option<&dyn Shape>,
    clearance: i32,
) -> Result<()> {
    let Some(other) = other else {
        return outside();
    };
    if shape.shape_type() != ShapeType::Circle
        || other.shape_type() != ShapeType::Circle
        || !(0..=DOMAIN).contains(&clearance.into())
    {
        return outside();
    }
    shape_domain(shape)?;
    shape_domain(other)
}

// Line chains -------------------------------------------------------------------------------------

fn chain_state(c: &ShapeLineChain) -> Value {
    let points: Vec<_> = c.cpoints().iter().map(|p| pt(*p)).collect();
    let shapes: Vec<_> = c.cshapes().iter().map(|(a, b)| json!([a, b])).collect();
    let cached = c.get_cached_bbox().map_or(Value::Null, box_json);
    json!({"points": points, "shapes": shapes, "arcs": c.arc_count(), "closed": c.is_closed(),
           "width": c.width(), "cached_bbox": cached})
}

/// Two distinct chains, the first mutable.
fn pair(
    chains: &mut [ShapeLineChain],
    a: usize,
    b: usize,
) -> Result<(&mut ShapeLineChain, &mut ShapeLineChain)> {
    if a < b {
        let (lo, hi) = chains.split_at_mut(b);
        Ok((&mut lo[a], &mut hi[0]))
    } else {
        let (lo, hi) = chains.split_at_mut(a);
        Ok((&mut hi[0], &mut lo[b]))
    }
}

fn chain_call(chains: &mut [ShapeLineChain], step: &Value) -> Result<Value> {
    let name = text(step, "call")?;
    let a = slot(step, "a", CHAINS)?;
    let plain = || keys(step, &["call", "a"], &[]);

    match name {
        "new" => {
            plain()?;
            chains[a] = ShapeLineChain::new();
            return Ok(Value::Null);
        }
        "copy" | "assign" | "move_assign" | "ne" => {
            keys(step, &["call", "a", "src"], &[])?;
            let src = slot(step, "src", CHAINS)?;
            if chains[src].arc_count() != 0 {
                return outside();
            }
            match name {
                "copy" => chains[a] = ShapeLineChain::copy_of(&chains[src])?,
                "ne" => return Ok(json!(chains[a].ne(&chains[src])?)),
                // Self-assignment: the copy assigns every member to itself and the move
                // assignment's `this != &aOther` check skips it; neither changes the chain.
                _ if a == src => {}
                "assign" => {
                    let (dst, source) = pair(chains, a, src)?;
                    dst.assign_from(source)?;
                }
                _ => {
                    let (dst, source) = pair(chains, a, src)?;
                    dst.move_assign_from(source);
                }
            }
            return Ok(Value::Null);
        }
        _ => {}
    }

    let c = &mut chains[a];
    match name {
        "clear" => {
            plain()?;
            c.clear();
            Ok(Value::Null)
        }
        "set_closed" => {
            keys(step, &["call", "a", "closed"], &[])?;
            if c.arc_count() != 0 {
                return outside();
            }
            c.set_closed(boolean(step, "closed")?)?;
            Ok(Value::Null)
        }
        "set_width" => {
            keys(step, &["call", "a", "width"], &[])?;
            c.set_width(int(step, "width")?);
            Ok(Value::Null)
        }
        "is_closed"
        | "width"
        | "segment_count"
        | "point_count"
        | "get_point_count"
        | "get_segment_count"
        | "is_solid"
        | "arc_count"
        | "cpoints"
        | "cshapes"
        | "carcs"
        | "cached_bbox"
        | "clast_point"
        | "generate_bbox_cache"
        | "type" => {
            plain()?;
            Ok(match name {
                "is_closed" => json!(c.is_closed()),
                "width" => json!(c.width()),
                "segment_count" => json!(c.segment_count()),
                "point_count" => json!(c.point_count()),
                "get_point_count" => json!(c.get_point_count() as u64),
                "get_segment_count" => json!(c.get_segment_count() as u64),
                "is_solid" => json!(c.is_solid()?),
                "arc_count" => json!(c.arc_count()),
                "carcs" => json!(c.carcs().len()),
                "cached_bbox" => c.get_cached_bbox().map_or(Value::Null, box_json),
                "type" => json!({"type": c.shape_type().value(), "width": c.get_width()}),
                "clast_point" => pt(c.clast_point()?),
                "generate_bbox_cache" => {
                    c.generate_bbox_cache()?;
                    Value::Null
                }
                "cpoints" => chain_state(c)["points"].clone(),
                _ => chain_state(c)["shapes"].clone(),
            })
        }
        "cpoint" | "get_point" => {
            keys(step, &["call", "a", "index"], &[])?;
            let index = int(step, "index")?;
            Ok(pt(if name == "cpoint" {
                c.cpoint(index)?
            } else {
                c.get_point(index)?
            }))
        }
        "csegment" | "get_segment" => {
            keys(step, &["call", "a", "index"], &[])?;
            let index = int(step, "index")?;
            // Segment's wxCHECK accepts an adjusted index in [0, SegmentCount()).
            let count = c.segment_count();
            let adjusted = if index < 0 {
                index.wrapping_add(count)
            } else {
                index
            };
            if adjusted < 0 || adjusted >= count {
                return outside();
            }
            Ok(seg_json(&if name == "csegment" {
                c.csegment(index)?
            } else {
                c.get_segment(index)?
            }))
        }
        "bbox" => {
            keys(step, &["call", "a"], &["clearance"])?;
            let clearance = if step.get("clearance").is_some() {
                int(step, "clearance")?
            } else {
                0
            };
            Ok(box_json(&c.bbox(clearance)?))
        }
        "reserve_points" => {
            keys(step, &["call", "a", "size"], &[])?;
            let size = u64_field(step, "size")?;
            if size > RESERVE_BUDGET && size <= POINTS_MAX_SIZE {
                return Err(BUDGET.into());
            }
            match c.reserve_points(size) {
                Ok(()) => Ok(Value::Null),
                Err(ReserveError::LengthError) => Ok(json!({"threw": "length_error"})),
                Err(ReserveError::BadAlloc) => Err("std::bad_alloc".into()),
            }
        }
        "append" | "append_xy" => {
            if name == "append" {
                keys(step, &["call", "a", "point"], &["allow_duplication"])?;
            } else {
                keys(step, &["call", "a", "x", "y"], &["allow_duplication"])?;
            }
            let dup = boolean_or(step, "allow_duplication", false)?;
            if name == "append" {
                c.append(point(step, "point")?, dup)?;
            } else {
                let x = int(step, "x")?;
                let y = int(step, "y")?;
                c.append_xy(x, y, dup)?;
            }
            Ok(Value::Null)
        }
        "remove" => {
            keys(step, &["call", "a", "index"], &[])?;
            let index = int(step, "index")?;
            if c.arc_count() != 0 || i64::from(index) < -i64::from(c.point_count()) {
                return outside();
            }
            c.remove(index)?;
            Ok(Value::Null)
        }
        "move" => {
            keys(step, &["call", "a", "vector"], &[])?;
            c.move_by(point(step, "vector")?)?;
            Ok(Value::Null)
        }
        "arc_index" | "reversed_arc_index" => {
            keys(step, &["call", "a", "segment"], &[])?;
            let segment = u64_field(step, "segment")?;
            Ok(json!(if name == "arc_index" {
                c.arc_index(segment)?
            } else {
                c.reversed_arc_index(segment)?
            }))
        }
        "arc" | "amend_arc_start" | "amend_arc_end" => {
            if name == "arc" {
                keys(step, &["call", "a", "index"], &[])?;
            } else {
                keys(step, &["call", "a", "index", "point"], &[])?;
            }
            let index = u64_field(step, "index")?;
            match name {
                "arc" => {
                    c.arc(index)?;
                }
                "amend_arc_start" => c.amend_arc_start(index, point(step, "point")?)?,
                _ => c.amend_arc_end(index, point(step, "point")?)?,
            }
            // Only reached with an arc, which no chain built here holds.
            outside()
        }
        "distance" => {
            keys(step, &["call", "a", "point", "outline_only"], &[])?;
            let p = point(step, "point")?;
            let outline_only = boolean(step, "outline_only")?;
            shape_domain(c)?;
            if !within_point(p, DOMAIN) {
                return outside();
            }
            Ok(json!(c.distance(p, outline_only)?))
        }
        _ => Err("unknown call".into()),
    }
}

fn run_chain(v: &Value) -> Result<Value> {
    keys(v, &["op", "steps"], &[])?;
    let mut chains: Vec<ShapeLineChain> = (0..CHAINS).map(|_| ShapeLineChain::new()).collect();
    let mut replies = Vec::new();
    for step in array(v, "steps", STEPS)? {
        replies.push(chain_call(&mut chains, step)?);
    }
    let states: Vec<_> = chains.iter().map(chain_state).collect();
    Ok(json!({"replies": replies, "chains": states}))
}

// Items, shapes and the assertion recorder -------------------------------------------------------

fn build_shape(spec: &Value) -> Result<Box<dyn Shape>> {
    let kind = text(spec, "kind")?;
    match kind {
        "circle" => {
            keys(spec, &["kind", "center", "radius"], &[])?;
            let center = point(spec, "center")?;
            let radius = int(spec, "radius")?;
            Ok(Box::new(ShapeCircle::with_center_radius(center, radius)))
        }
        "chain" => {
            keys(spec, &["kind", "points"], &["width"])?;
            let mut chain = ShapeLineChain::new();
            for p in array(spec, "points", ITEMS)? {
                chain.append(point_value(p)?, false)?;
            }
            if spec.get("width").is_some() {
                chain.set_width(int(spec, "width")?);
            }
            Ok(Box::new(chain))
        }
        _ => Err("unknown shape".into()),
    }
}

fn shape_state(shape: Option<&dyn Shape>) -> Value {
    let Some(shape) = shape else {
        return Value::Null;
    };
    if let Some(c) = shape.as_any().downcast_ref::<ShapeCircle>() {
        return json!({"type": "SH_CIRCLE", "center": pt(c.get_center()), "radius": c.get_radius()});
    }
    match shape.as_any().downcast_ref::<ShapeLineChain>() {
        Some(c) => {
            let mut state = chain_state(c);
            state["type"] = json!("SH_LINE_CHAIN");
            state
        }
        None => Value::Null,
    }
}

/// A test item: a shape (or none) that `Shape( aLayer )` returns on its layers.
#[derive(Debug)]
struct Item {
    tag: usize,
    shape: Option<Box<dyn Shape>>,
    layers: Vec<i32>,
}

/// An `ITEM*`: identity is the address, as in C++.
#[derive(Clone, Copy, Debug)]
struct ItemRef<'a>(&'a Item);

impl PartialEq for ItemRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}
impl NodeData for ItemRef<'_> {}
impl<'a> ShapeItem<'a> for ItemRef<'a> {
    fn shape(&self, layer: i32) -> Option<&'a dyn Shape> {
        if self.0.layers.contains(&layer) {
            self.0.shape.as_deref()
        } else {
            None
        }
    }
}

fn read_items(v: &Value) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    for spec in array(v, "items", ITEMS)? {
        keys(spec, &["shape", "layers"], &[])?;
        let shape = match spec.get("shape") {
            Some(Value::Null) | None => None,
            Some(s) => Some(build_shape(s)?),
        };
        let mut layers = Vec::new();
        for layer in array(spec, "layers", 8)? {
            layers.push(int_value(layer)?);
        }
        items.push(Item {
            tag: items.len(),
            shape,
            layers,
        });
    }
    Ok(items)
}

fn read_item<'a>(v: &Value, name: &str, items: &'a [Item]) -> Result<Option<ItemRef<'a>>> {
    let value = field(v, name)?;
    if value.is_null() {
        return Ok(None);
    }
    let n = i64_value(value)?;
    let item = usize::try_from(n)
        .ok()
        .and_then(|n| items.get(n))
        .ok_or("invalid item")?;
    Ok(Some(ItemRef(item)))
}

fn wire(item: Option<ItemRef<'_>>) -> Value {
    item.map_or(Value::Null, |i| json!(i.0.tag))
}

type Records = Rc<RefCell<Vec<Value>>>;

/// The returning host assertion handler: records each call, as the oracle's recorder does
/// around wx's default handler (which logs and returns).
fn recorder(
    records: &Records,
) -> impl FnMut(&WxCheck) -> core::result::Result<(), ShapeIndexError> + '_ {
    move |c: &WxCheck| {
        records
            .borrow_mut()
            .push(json!({"line": c.line, "function": c.function,
                                         "condition": c.condition, "message": ""}));
        Ok(())
    }
}

/// The C++ `VISITOR` by value: records `[calls, item]` with its own call count.
fn accept_visitor_fn<'a>(out: &Records) -> impl FnMut(Option<ItemRef<'a>>) + Clone {
    let out = out.clone();
    let mut calls: i32 = 0;
    move |item| {
        out.borrow_mut().push(json!([calls, wire(item)]));
        calls = calls.wrapping_add(1);
    }
}

/// The item's box as `boundingBox` computes it, without its assertions (the C++ guard).
fn item_box(item: Option<ItemRef<'_>>, layer: i32) -> Result<Box2I> {
    let Some(item) = item else {
        return Ok(Box2I::default());
    };
    match item.shape(layer) {
        Some(shape) => Ok(shape.bbox(0)?),
        None => Ok(Box2I::default()),
    }
}

fn run_collide(
    item: Option<ItemRef<'_>>,
    layer: i32,
    min_distance: i32,
    target: &Value,
    records: &Records,
) -> Result<Value> {
    let shape = item.and_then(|i| i.shape(layer));
    keys(target, &[], &["point", "seg", "shape"])?;
    if object(target)?.len() != 1 {
        return Err("expected one target".into());
    }
    let mut on_assert = recorder(records);

    if let Some(spec) = target.get("shape") {
        let other = build_shape(spec)?;
        if let Some(shape) = shape {
            collide_shape_domain(shape, Some(other.as_ref()), min_distance)?;
        }
        let other: &dyn Shape = other.as_ref();
        return Ok(json!(collide(
            item,
            other,
            layer,
            min_distance,
            &mut on_assert
        )?));
    }

    let is_point = target.get("point").is_some();
    let (p, s) = if is_point {
        let p = point(target, "point")?;
        (p, Seg::new(p, p))
    } else {
        let a = target
            .get("seg")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 2)
            .ok_or("expected segment: seg")?;
        (
            Vector2::default(),
            Seg::new(point_value(&a[0])?, point_value(&a[1])?),
        )
    };
    if let Some(shape) = shape {
        shape_domain(shape)?;
        let target_within = if is_point {
            within_point(p, DOMAIN)
        } else {
            within_point(s.a, DOMAIN) && within_point(s.b, DOMAIN)
        };
        if !within(min_distance.into(), DOMAIN) || !target_within {
            return outside();
        }
    }
    Ok(json!(if is_point {
        collide(item, p, layer, min_distance, &mut on_assert)?
    } else {
        collide(item, s, layer, min_distance, &mut on_assert)?
    }))
}

// SHAPE_INDEX -------------------------------------------------------------------------------------

type Index<'a> = ShapeIndex<ItemRef<'a>>;

struct Indices<'a> {
    forest: IndexForest<ItemRef<'a>>,
    index: Vec<Option<(Index<'a>, i32)>>,
}

impl<'a> Indices<'a> {
    fn replace(&mut self, a: usize, new: (Index<'a>, i32)) -> Result<()> {
        if let Some((old, _)) = self.index[a].replace(new) {
            old.destroy(&mut self.forest)?;
        }
        Ok(())
    }
}

fn index_call<'a>(
    state: &mut Indices<'a>,
    items: &'a [Item],
    step: &Value,
    records: &Records,
) -> Result<Value> {
    let name = text(step, "call")?;

    match name {
        "shape_functor" | "bounding_box" => {
            keys(step, &["call", "item", "layer"], &[])?;
            let item = read_item(step, "item", items)?;
            let layer = int(step, "layer")?;
            let mut on_assert = recorder(records);
            return Ok(if name == "shape_functor" {
                shape_state(shape_functor(item, layer, &mut on_assert)?)
            } else {
                box_json(&bounding_box(item, layer, &mut on_assert)?)
            });
        }
        "collide" => {
            keys(
                step,
                &["call", "item", "layer", "min_distance", "target"],
                &[],
            )?;
            let item = read_item(step, "item", items)?;
            let layer = int(step, "layer")?;
            let min_distance = int(step, "min_distance")?;
            return run_collide(item, layer, min_distance, field(step, "target")?, records);
        }
        "query_callback" => {
            keys(step, &["call", "item"], &[])?;
            let item = read_item(step, "item", items)?;
            let out: Records = Rc::new(RefCell::new(Vec::new()));
            let visitor = accept_visitor_fn(&out);
            let returned = query_callback(item, &visitor);
            let visits = out.borrow().clone();
            return Ok(json!({"returned": returned, "visits": visits}));
        }
        _ => {}
    }

    let a = slot(step, "a", INDICES)?;

    if name == "new" {
        keys(step, &["call", "a", "layer"], &[])?;
        let layer = int(step, "layer")?;
        let index = Index::new(&mut state.forest, layer)?;
        state.replace(a, (index, layer))?;
        return Ok(Value::Null);
    }
    match name {
        "clone" => {
            keys(step, &["call", "a", "src"], &[])?;
            let src = slot(step, "src", INDICES)?;
            let (clone, layer) = match &state.index[src] {
                Some((index, layer)) => (index.clone_index(&mut state.forest)?, *layer),
                None => return Err("missing index".into()),
            };
            state.replace(a, (clone, layer))?;
            return Ok(Value::Null);
        }
        _ if state.index[a].is_none() => return Err("missing index".into()),
        "destroy" => {
            keys(step, &["call", "a"], &[])?;
            if let Some((old, _)) = state.index[a].take() {
                old.destroy(&mut state.forest)?;
            }
            return Ok(Value::Null);
        }
        _ => {}
    }

    let Indices { forest, index } = state;
    let Some((index, layer)) = index[a].as_mut() else {
        return Err("missing index".into());
    };
    let layer = *layer;
    let mut on_assert = recorder(records);

    match name {
        "add" | "remove" => {
            keys(step, &["call", "a", "item"], &[])?;
            let item = read_item(step, "item", items)?;
            item_box(item, layer)?;
            if name == "add" {
                index.add(forest, item, &mut on_assert)?;
            } else {
                index.remove(forest, item, &mut on_assert)?;
            }
            Ok(Value::Null)
        }
        "add_bbox" => {
            keys(step, &["call", "a", "item", "pos", "size"], &[])?;
            let item = read_item(step, "item", items)?;
            let b = tree_box(step)?;
            index.add_with_bbox(forest, item, &b)?;
            Ok(Value::Null)
        }
        "remove_all" | "size" | "range" | "accept" | "reindex" => {
            keys(step, &["call", "a"], &[])?;
            match name {
                "remove_all" => {
                    index.remove_all(forest)?;
                    Ok(Value::Null)
                }
                "size" => Ok(json!(index.size(forest)? as u64)),
                "range" => {
                    let mut out = Vec::new();
                    let mut it = index.begin(forest)?;
                    let end = index.end(forest);
                    while it != end {
                        out.push(wire(*it.current()));
                        it.advance().map_err(ShapeIndexError::from)?;
                    }
                    Ok(json!(out))
                }
                "accept" => {
                    let out: Records = Rc::new(RefCell::new(Vec::new()));
                    index.accept(forest, accept_visitor_fn(&out))?;
                    let visits = out.borrow().clone();
                    Ok(json!(visits))
                }
                _ => {
                    // The tree reports an undefined insertion itself.
                    index.reindex(forest, &mut on_assert)?;
                    Ok(Value::Null)
                }
            }
        }
        "bulk_load" => {
            keys(step, &["call", "a", "entries"], &[])?;
            let mut entries = Vec::new();
            for e in array(step, "entries", ITEMS)? {
                keys(e, &["item", "pos", "size"], &[])?;
                let item = read_item(e, "item", items)?;
                entries.push((item, tree_box(e)?));
            }
            index.bulk_load(forest, &entries)?;
            Ok(Value::Null)
        }
        "query" => {
            keys(step, &["call", "a", "shape", "min_distance", "stop"], &[])?;
            let shape = build_shape(field(step, "shape")?)?;
            let min_distance = int(step, "min_distance")?;
            let stop = int(step, "stop")?;
            let mut out = Vec::new();
            let mut calls: i32 = 0;
            let mut visitor = |item: &Option<ItemRef<'a>>| {
                out.push(json!([calls, wire(*item)]));
                calls = calls.wrapping_add(1);
                calls < stop
            };
            let found = index.query(forest, shape.as_ref(), min_distance, &mut visitor)?;
            Ok(json!({"found": found, "visits": out}))
        }
        "iterate" => {
            keys(step, &["call", "a", "ops"], &[])?;
            let ops = array(step, "ops", STEPS)?;
            let mut it = index.begin_iterator(forest)?;
            let mut out = Vec::new();
            for op in ops {
                let op = op.as_str().ok_or("expected op")?;
                out.push(match op {
                    "deref" => wire(it.deref()),
                    "inc" | "inc_post" => json!(it.increment()?),
                    "is_null" => json!(it.is_null()),
                    "is_not_null" => json!(it.is_not_null()),
                    "next" => wire(it.next_item()?),
                    _ => return Err("unknown op".into()),
                });
            }
            Ok(json!(out))
        }
        _ => Err("unknown call".into()),
    }
}

/// `BOX2I( pos, VECTOR2L( size ) )` for a tree; the tree reports an undefined insertion or bulk
/// load itself.
fn tree_box(v: &Value) -> Result<Box2I> {
    let pos = point(v, "pos")?;
    let size = point(v, "size")?;
    Ok(Box2I::new(
        pos,
        Vector2::new(i64::from(size.x), i64::from(size.y)),
    )?)
}

fn run_index(v: &Value) -> Result<Value> {
    keys(v, &["op", "items", "steps"], &[])?;
    let items = read_items(v)?;
    let forest = IndexForest::new(Budget {
        entries: 4096,
        trees: 1024,
        nodes: 1 << 16,
    })
    .map_err(ShapeIndexError::from)?;
    let mut state = Indices {
        forest,
        index: (0..INDICES).map(|_| None).collect(),
    };
    let records: Records = Rc::new(RefCell::new(Vec::new()));
    let mut replies = Vec::new();
    for step in array(v, "steps", STEPS)? {
        replies.push(index_call(&mut state, &items, step, &records)?);
    }
    let assertions = records.borrow().clone();
    Ok(json!({"replies": replies, "assertions": assertions}))
}

// SHAPE_INDEX_LIST ----------------------------------------------------------------------------------

type List<'a> = ShapeIndexList<
    'a,
    Option<ItemRef<'a>>,
    kicad_math::shape_index_list::DefaultFunctor<'a, ItemRef<'a>>,
>;

/// The C++ guard of `query_iterator::next()` from `pos`: `Ok( Some( pos ) )` is where it stops
/// without an exact test; `Ok( None )` means an exact `SHAPE::Collide` is reached inside the
/// verified domain (the port defers there).
fn guard_next(
    list: &List<'_>,
    mut pos: usize,
    reference: &Box2I,
    shape: Option<&dyn Shape>,
    min_distance: i32,
    exact: bool,
) -> Result<Option<usize>> {
    let entries = list.entries();
    while let Some(e) = entries.get(pos) {
        if reference.distance_box(&e.bbox)? <= i64::from(min_distance) {
            if !exact {
                return Ok(Some(pos));
            }
            collide_shape_domain(e.shape, shape, min_distance)?;
            return Ok(None);
        }
        pos += 1;
    }
    Ok(Some(pos))
}

fn list_call<'a>(list: &mut List<'a>, items: &'a [Item], step: &Value) -> Result<Value> {
    let name = text(step, "call")?;
    match name {
        "add" | "remove" => {
            keys(step, &["call", "item"], &[])?;
            let item = read_item(step, "item", items)?;
            if name == "remove" {
                list.remove(item);
            } else {
                list.add(item)?;
            }
            Ok(Value::Null)
        }
        "size" | "clear" => {
            keys(step, &["call"], &[])?;
            if name == "size" {
                return Ok(json!(list.size()));
            }
            list.clear();
            Ok(Value::Null)
        }
        "query" => {
            keys(step, &["call", "shape", "min_distance", "stop"], &["exact"])?;
            let shape = build_shape(field(step, "shape")?)?;
            let min_distance = int(step, "min_distance")?;
            let stop = int(step, "stop")?;
            let exact = boolean_or(step, "exact", true)?;
            // The guard's walk: refuse before the first exact SHAPE::Collide outside the domain.
            let reference = shape.bbox(0)?;
            let min_dist_sq = i64::from(min_distance) * i64::from(min_distance);
            let mut calls = 0;
            for e in list.entries() {
                if reference.squared_distance_box(&e.bbox)? <= min_dist_sq {
                    if exact {
                        collide_shape_domain(e.shape, Some(shape.as_ref()), min_distance)?;
                        break;
                    }
                    calls += 1;
                    if calls >= stop {
                        break;
                    }
                }
            }
            let mut out = Vec::new();
            let mut visits: i32 = 0;
            let mut visitor = |item: Option<ItemRef<'a>>| {
                out.push(json!([visits, wire(item)]));
                visits = visits.wrapping_add(1);
                visits < stop
            };
            let n = list.query(shape.as_ref(), min_distance, &mut visitor, exact)?;
            Ok(json!({"found": n, "visits": out}))
        }
        "iterate" => {
            keys(step, &["call", "ops"], &[])?;
            let ops = array(step, "ops", STEPS)?;
            let mut it = list.begin();
            let end = list.end();
            let null = ListIterator::null();
            let mut out = Vec::new();
            for op in ops {
                let op = op.as_str().ok_or("expected op")?;
                out.push(match op {
                    "deref" => wire(it.deref()?),
                    "inc" | "inc_post" => {
                        it.increment()?;
                        Value::Null
                    }
                    "eq_end" => json!(it == end),
                    "ne_end" => json!(it != end),
                    "eq_null" => json!(it == null),
                    "copy" => {
                        let copy = it;
                        let mut assigned = ListIterator::null();
                        assigned.assign_from(&copy);
                        it = assigned;
                        Value::Null
                    }
                    _ => return Err("unknown op".into()),
                });
            }
            Ok(json!(out))
        }
        "qiterate" => {
            keys(
                step,
                &["call", "shape", "min_distance", "exact", "ops"],
                &[],
            )?;
            let shape = match field(step, "shape")? {
                Value::Null => None,
                spec => Some(build_shape(spec)?),
            };
            let min_distance = int(step, "min_distance")?;
            let exact = boolean(step, "exact")?;
            let ops = array(step, "ops", STEPS)?;
            let shape_ref: Option<&dyn Shape> = shape.as_deref();
            let mut reference = Box2I::default();
            let mut pos = Some(0);
            if let Some(s) = shape_ref {
                reference = s.bbox(0)?;
                pos = guard_next(list, 0, &reference, shape_ref, min_distance, exact)?;
            }
            let mut it = list.qbegin(shape_ref, min_distance, exact)?;
            let end = list.qend()?;
            let mut out = Vec::new();
            for op in ops {
                let op = op.as_str().ok_or("expected op")?;
                out.push(match op {
                    "deref" => wire(it.deref()?),
                    "inc" | "inc_post" => {
                        if let Some(p) = pos {
                            if p < list.entries().len() {
                                pos = guard_next(
                                    list,
                                    p + 1,
                                    &reference,
                                    shape_ref,
                                    min_distance,
                                    exact,
                                )?;
                            }
                        }
                        it.increment()?;
                        Value::Null
                    }
                    "eq_end" => json!(it == end),
                    "ne_end" => json!(it != end),
                    "copy" => {
                        let copy = it.try_copy()?;
                        let mut assigned = end.try_copy()?;
                        assigned.assign_from(&copy)?;
                        it = assigned;
                        Value::Null
                    }
                    "copy_null" => {
                        QueryIterator::<Option<ItemRef<'a>>>::null().try_copy()?;
                        Value::Null
                    }
                    _ => return Err("unknown op".into()),
                });
            }
            Ok(json!(out))
        }
        _ => Err("unknown call".into()),
    }
}

/// The adapter's `compare` call: `operator==` and `operator!=` of two iterators, possibly of
/// different lists (see the adapter for the request shape).
fn compare_iterators<'a>(lists: &[List<'a>], step: &Value) -> Result<Value> {
    keys(step, &["call", "kind", "left", "right"], &[])?;
    let kind = text(step, "kind")?;
    let side = |name: &str, query: bool| -> Result<(&List<'a>, &str, i32)> {
        let spec = field(step, name)?;
        if query {
            keys(spec, &["l", "at"], &[])?;
        } else {
            keys(spec, &["l", "at"], &["inc"])?;
        }
        let list = &lists[slot(spec, "l", LISTS)?];
        let at = text(spec, "at")?;
        let inc = if spec.get("inc").is_some() {
            int(spec, "inc")?
        } else {
            0
        };
        if inc < 0 || inc > STEPS as i32 {
            return Err("invalid increment".into());
        }
        Ok((list, at, inc))
    };
    match kind {
        "iterator" => {
            let make = |name: &str| -> Result<ListIterator<'_, 'a, Option<ItemRef<'a>>>> {
                let (list, at, inc) = side(name, false)?;
                let mut it = match at {
                    "begin" => list.begin(),
                    "end" => list.end(),
                    "null" => ListIterator::null(),
                    _ => return Err("unknown position".into()),
                };
                for _ in 0..inc {
                    it.increment()?;
                }
                Ok(it)
            };
            let left = make("left")?;
            let right = make("right")?;
            Ok(json!({"eq": left == right, "ne": left != right}))
        }
        "query" => {
            let make = |name: &str| -> Result<QueryIterator<'_, 'a, Option<ItemRef<'a>>>> {
                let (list, at, _) = side(name, true)?;
                Ok(match at {
                    "begin" => list.qbegin(None, 0, false)?,
                    "end" => list.qend()?,
                    "null" => QueryIterator::null(),
                    _ => return Err("unknown position".into()),
                })
            };
            let left = make("left")?;
            let right = make("right")?;
            Ok(json!({"eq": left == right, "ne": left != right}))
        }
        _ => Err("unknown kind".into()),
    }
}

fn run_list(v: &Value) -> Result<Value> {
    keys(v, &["op", "items", "steps"], &[])?;
    let items = read_items(v)?;
    let mut lists = [List::new(), List::new()];
    let mut replies = Vec::new();
    for step in array(v, "steps", STEPS)? {
        if text(step, "call")? == "compare" {
            replies.push(compare_iterators(&lists, step)?);
            continue;
        }
        // Every other call acts on list "l" (0 when absent).
        let mut call = step.clone();
        let mut l = 0;
        if step.get("l").is_some() {
            l = slot(step, "l", LISTS)?;
            if let Value::Object(map) = &mut call {
                map.remove("l");
            }
        }
        replies.push(list_call(&mut lists[l], &items, &call)?);
    }
    Ok(json!({"replies": replies}))
}

// Dispatch ---------------------------------------------------------------------------------------------

fn run(v: &Value) -> Result<Value> {
    let op = text(v, "op")?;
    match op {
        "clipper_z_value" => {
            keys(v, &["op"], &["indices", "offset"])?;
            let z = if let Some(indices) = v.get("indices") {
                let a = indices
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or("expected indices")?;
                let pair = (i64_value(&a[0])?, i64_value(&a[1])?);
                let offset = if v.get("offset").is_some() {
                    i64_field(v, "offset")?
                } else {
                    0
                };
                ClipperZValue::from_indices(pair, offset)?
            } else if v.get("offset").is_some() {
                return Err("unexpected or missing field".into());
            } else {
                ClipperZValue::new()
            };
            Ok(json!({"first": z.first_arc_idx, "second": z.second_arc_idx}))
        }
        "intersection" => {
            keys(v, &["op"], &[])?;
            let i = Intersection::default();
            Ok(
                json!({"p": pt(i.p), "index_our": i.index_our, "index_their": i.index_their,
                      "is_corner_our": i.is_corner_our, "is_corner_their": i.is_corner_their,
                      "valid": i.valid}),
            )
        }
        "compare_origin_distance" => {
            keys(v, &["op", "origin", "a", "b"], &[])?;
            let origin = point(v, "origin")?;
            let a = Intersection {
                p: point(v, "a")?,
                ..Intersection::default()
            };
            let b = Intersection {
                p: point(v, "b")?,
                ..Intersection::default()
            };
            Ok(json!(CompareOriginDistance::new(origin).compare(&a, &b)?))
        }
        "chain" => run_chain(v),
        "index" => run_index(v),
        "index_list" => run_list(v),
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
