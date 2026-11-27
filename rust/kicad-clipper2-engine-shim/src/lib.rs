// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for the `clipper.engine.cpp` lines 1-2271 port
//! (`kicad_clipper2::engine_p001`), mirroring `port/oracle/shims/clipper2_engine_p001.cpp`
//! request for request. Doubles travel as 16 hex digit bit patterns, points as `[x, y, z]`.
//!
//! Three operations: `stripe_index` builds a `StripeIndex` from a path or an output ring and
//! queries it; the pure helpers (`get_dx`, `top_x`, `area_triangle`, `pts_really_close`,
//! `is_odd`, the sort functors, `scanline`) run on their arguments; `engine` runs a script of
//! steps on one `ClipperBase` and `ReuseableDataContainer64` and dumps the complete engine
//! state afterwards. See README.md for the step list and the refusal rules.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_clipper2::clipper_engine::{
    Active, ActiveId, ClipType, ClipperBase, HorzSegment, IntersectNode, JoinWith, LocalMinimaId,
    OutPtId, OutRecId, PathType, StripeIndex, VertexId, ZCallbackSlot,
};
use kicad_clipper2::engine_p001::{self as p001, ReuseableDataContainer64};
use kicad_clipper2::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;

pub use kicad_clipper2::engine_p001::UNIT;

/// Steps in one `engine` script.
const MAX_STEPS: usize = 512;
/// Bound on every list or ring the harness walks (a guard bound, never reached by a valid
/// state).
const WALK_LIMIT: usize = 1 << 20;
/// Harness domain of the wind counts `set_active` assigns: increments of one per step can
/// never overflow an `int` from inside it.
const WIND_LIMIT: i64 = 1 << 20;
/// Harness domain of the recorder's base z: `base + calls` never overflows.
const Z_LIMIT: i64 = 1 << 40;

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

/// A reference expression with undefined behaviour for this state (a null dereference or a
/// loop that never ends), refused before it runs.
fn ub<T>() -> R<T> {
    Err(E::Clip(ClipperError::Undefined("guard")))
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

fn text<'a>(v: &'a Value, name: &str) -> R<&'a str> {
    match at(v, name)?.as_str() {
        Some(s) => Ok(s),
        None => invalid(name),
    }
}

fn boolean(v: &Value) -> R<bool> {
    match v.as_bool() {
        Some(b) => Ok(b),
        None => invalid("boolean"),
    }
}

fn i64_of(v: &Value) -> R<i64> {
    match v.as_i64() {
        Some(i) => Ok(i),
        None => invalid("int64"),
    }
}

fn u64_of(v: &Value) -> R<u64> {
    match v
        .as_u64()
        .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
    {
        Some(u) => Ok(u),
        None => invalid("uint64"),
    }
}

fn i32_of(v: &Value) -> R<i32> {
    match i32::try_from(i64_of(v)?) {
        Ok(i) => Ok(i),
        Err(_) => invalid("int"),
    }
}

fn index_of(v: &Value) -> R<usize> {
    match usize::try_from(u64_of(v)?) {
        Ok(i) => Ok(i),
        Err(_) => invalid("index"),
    }
}

fn f64_of(v: &Value) -> R<f64> {
    let Some(s) = v.as_str() else {
        return invalid("double");
    };
    if s.len() != 16
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return invalid("double");
    }
    match u64::from_str_radix(s, 16) {
        Ok(bits) => Ok(f64::from_bits(bits)),
        Err(_) => invalid("double"),
    }
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

fn read_point(v: &Value) -> R<Point64> {
    match v.as_array() {
        Some(a) if a.len() == 3 => Ok(Point64 {
            x: i64_of(&a[0])?,
            y: i64_of(&a[1])?,
            z: i64_of(&a[2])?,
        }),
        _ => invalid("point"),
    }
}

fn write_point(p: &Point64) -> Value {
    json!([p.x, p.y, p.z])
}

fn read_path(v: &Value) -> R<Path64> {
    match v.as_array() {
        Some(a) => a.iter().map(read_point).collect(),
        None => invalid("path"),
    }
}

fn write_path(p: &[Point64]) -> Value {
    Value::Array(p.iter().map(write_point).collect())
}

fn read_paths(v: &Value) -> R<Paths64> {
    match v.as_array() {
        Some(a) => a.iter().map(read_path).collect(),
        None => invalid("paths"),
    }
}

fn pip_name(result: PointInPolygonResult) -> &'static str {
    match result {
        PointInPolygonResult::IsOn => "IsOn",
        PointInPolygonResult::IsInside => "IsInside",
        PointInPolygonResult::IsOutside => "IsOutside",
    }
}

fn path_type(v: &Value) -> R<PathType> {
    match v.as_str() {
        Some("subject") => Ok(PathType::Subject),
        Some("clip") => Ok(PathType::Clip),
        _ => invalid("path_type"),
    }
}

fn path_type_name(t: PathType) -> &'static str {
    match t {
        PathType::Subject => "subject",
        PathType::Clip => "clip",
    }
}

fn clip_type(v: &Value) -> R<ClipType> {
    match v.as_str() {
        Some("NoClip") => Ok(ClipType::NoClip),
        Some("Intersection") => Ok(ClipType::Intersection),
        Some("Union") => Ok(ClipType::Union),
        Some("Difference") => Ok(ClipType::Difference),
        Some("Xor") => Ok(ClipType::Xor),
        _ => invalid("clip_type"),
    }
}

fn fill_rule(v: &Value) -> R<FillRule> {
    match v.as_str() {
        Some("EvenOdd") => Ok(FillRule::EvenOdd),
        Some("NonZero") => Ok(FillRule::NonZero),
        Some("Positive") => Ok(FillRule::Positive),
        Some("Negative") => Ok(FillRule::Negative),
        _ => invalid("fill_rule"),
    }
}

fn join_name(join: JoinWith) -> &'static str {
    match join {
        JoinWith::NoJoin => "NoJoin",
        JoinWith::Left => "Left",
        JoinWith::Right => "Right",
    }
}

fn in_domain(p: &Point64) -> bool {
    (MIN_COORD..=MAX_COORD).contains(&p.x) && (MIN_COORD..=MAX_COORD).contains(&p.y)
}

// --- stripe_index ------------------------------------------------------------------------

/// Builds an output ring of `path` in `base` (the first point is `pts`); `None` when empty.
fn build_ring(
    base: &mut ClipperBase,
    outrec: Option<OutRecId>,
    path: &[Point64],
) -> R<Option<OutPtId>> {
    let mut first: Option<OutPtId> = None;
    let mut last: Option<OutPtId> = None;
    for pt in path {
        let op = base.new_out_pt(pt, outrec)?;
        if let (Some(f), Some(l)) = (first, last) {
            base.out_pt_mut(op)?.prev = l;
            base.out_pt_mut(op)?.next = f;
            base.out_pt_mut(l)?.next = op;
            base.out_pt_mut(f)?.prev = op;
        } else {
            first = Some(op);
        }
        last = Some(op);
    }
    Ok(first)
}

fn run_stripe_index(v: &Value) -> R<Value> {
    keys(v, &["op", "source", "path", "queries", "ys"])?;
    let path = read_path(&v["path"])?;
    let queries = read_path(&v["queries"])?;
    let ys = match v["ys"].as_array() {
        Some(a) => a.iter().map(i64_of).collect::<R<Vec<i64>>>()?,
        None => return invalid("ys"),
    };
    let mut index = StripeIndex::default();
    match text(v, "source")? {
        "path" => index.build_from_path(&path)?,
        "ring" => {
            let mut base = ClipperBase::default();
            let op = build_ring(&mut base, None, &path)?;
            index.build_from_ring(&base, op)?;
        }
        _ => return invalid("source"),
    }
    let mut answers = Vec::new();
    for q in &queries {
        answers.push(json!(pip_name(index.point_in_polygon(q)?)));
    }
    let mut stripes_of = Vec::new();
    for &y in &ys {
        stripes_of.push(json!(index.y_to_stripe(y)?));
    }
    let edges: Vec<Value> = index
        .edges()
        .iter()
        .map(|e| json!([write_point(&e.a), write_point(&e.b)]))
        .collect();
    Ok(json!({
        "y_min": index.y_min(), "stripe_height": index.stripe_height(),
        "stripe_count": index.stripe_count(), "empty": index.empty(),
        "edge_count": index.edge_count(), "edges": edges, "stripes": index.stripes(),
        "queries": answers, "ys": stripes_of,
    }))
}

// --- pure helpers ------------------------------------------------------------------------

fn run_pure(op: &str, v: &Value) -> R<Value> {
    match op {
        "get_dx" => {
            keys(v, &["op", "a", "b"])?;
            Ok(bits(p001::get_dx(
                &read_point(&v["a"])?,
                &read_point(&v["b"])?,
            )?))
        }
        "top_x" => {
            keys(v, &["op", "bot", "top", "dx", "y"])?;
            let e = Active {
                bot: read_point(&v["bot"])?,
                top: read_point(&v["top"])?,
                dx: f64_of(&v["dx"])?,
                ..Active::default()
            };
            Ok(json!(p001::top_x(&e, i64_of(&v["y"])?)?))
        }
        "area_triangle" => {
            keys(v, &["op", "a", "b", "c"])?;
            Ok(bits(p001::area_triangle(
                &read_point(&v["a"])?,
                &read_point(&v["b"])?,
                &read_point(&v["c"])?,
            )?))
        }
        "pts_really_close" => {
            keys(v, &["op", "a", "b"])?;
            Ok(json!(p001::pts_really_close(
                &read_point(&v["a"])?,
                &read_point(&v["b"])?
            )?))
        }
        "is_odd" => {
            keys(v, &["op", "value"])?;
            Ok(json!(p001::is_odd(i32_of(&v["value"])?)))
        }
        "intersect_list_sort" => {
            keys(v, &["op", "a", "b"])?;
            let a = IntersectNode::new(None, None, &read_point(&v["a"])?);
            let b = IntersectNode::new(None, None, &read_point(&v["b"])?);
            Ok(json!(p001::intersect_list_sort(&a, &b)))
        }
        "loc_min_sorter" => {
            keys(v, &["op", "a", "b"])?;
            Ok(json!(p001::loc_min_sorter(
                &read_point(&v["a"])?,
                &read_point(&v["b"])?
            )))
        }
        "scanline" => {
            keys(v, &["op", "y"])?;
            let s = p001::Scanline::new(i64_of(&v["y"])?);
            Ok(json!({"y": s.y, "next_null": s.next.is_none()}))
        }
        _ => invalid("op"),
    }
}

// --- engine ------------------------------------------------------------------------------

struct Engine {
    base: ClipperBase,
    reuse: ReuseableDataContainer64,
    /// Edge handles in registration order; `false` once the edge was deleted.
    handles: Vec<(ActiveId, bool)>,
    /// `AddReuseableData` minima point into the container's arrays.
    base_borrows: bool,
    /// `current_locmin_iter_` is a valid iterator (set by `Reset`/`Clear`, invalidated by
    /// `AddPaths`/`AddReuseableData`).
    iter_valid: bool,
    calls: Rc<RefCell<Vec<Value>>>,
}

impl Engine {
    fn new() -> Self {
        Self {
            base: ClipperBase::default(),
            reuse: ReuseableDataContainer64::default(),
            handles: Vec::new(),
            base_borrows: false,
            iter_valid: false,
            calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn edge(&self, v: &Value) -> R<ActiveId> {
        match self.handles.get(index_of(v)?) {
            Some(&(id, true)) => Ok(id),
            _ => invalid("edge"),
        }
    }

    fn handle_of(&self, id: Option<ActiveId>) -> Value {
        let Some(id) = id else { return Value::Null };
        match self.handles.iter().position(|&(h, alive)| alive && h == id) {
            Some(i) => json!(i),
            None => json!("unknown"),
        }
    }

    fn walk_list(&self, head: Option<ActiveId>, sel: bool) -> R<Vec<ActiveId>> {
        let mut list = Vec::new();
        let mut e = head;
        while let Some(x) = e {
            if list.len() >= WALK_LIMIT {
                return invalid("malformed list");
            }
            list.push(x);
            let a = self.base.active(x)?;
            e = if sel { a.next_in_sel } else { a.next_in_ael };
        }
        Ok(list)
    }

    fn ael(&self) -> R<Vec<ActiveId>> {
        self.walk_list(self.base.actives_head(), false)
    }

    fn sel(&self) -> R<Vec<ActiveId>> {
        self.walk_list(self.base.sel_head(), true)
    }

    /// Register the edges reachable from `actives_` and then `sel_` that have no live handle.
    fn register(&mut self) -> R<()> {
        for id in self.ael()?.into_iter().chain(self.sel()?) {
            if !self.handles.iter().any(|&(h, alive)| alive && h == id) {
                self.handles.push((id, true));
            }
        }
        Ok(())
    }

    /// `DeleteEdges` deletes the AEL; the harness forgets every edge then (the adapter
    /// deletes the ones outside the AEL).
    fn forget_edges(&mut self) -> R<()> {
        for handle in &mut self.handles {
            handle.1 = false;
        }
        // CleanUp preserves the source SEL head, whose edges have just been deleted.
        // Refuse the adapter's mandatory observation, not the defined cleanup itself.
        if self.base.sel_head().is_some() {
            return invalid("deleted list observation");
        }
        Ok(())
    }

    fn outrec(&self, v: &Value) -> R<OutRecId> {
        let i = index_of(v)?;
        if i < self.base.outrecs().len() {
            Ok(OutRecId(i))
        } else {
            invalid("outrec")
        }
    }

    fn opt_outrec(&self, v: &Value) -> R<Option<OutRecId>> {
        if v.is_null() {
            Ok(None)
        } else {
            Ok(Some(self.outrec(v)?))
        }
    }

    fn ring(&self, start: OutPtId) -> R<Vec<OutPtId>> {
        let mut ring = vec![start];
        let mut op = self.base.out_pt(start)?.next;
        while op != start {
            if ring.len() >= WALK_LIMIT {
                return invalid("malformed ring");
            }
            ring.push(op);
            op = self.base.out_pt(op)?.next;
        }
        Ok(ring)
    }

    /// The output record's points: `pts` must be set.
    fn pts(&self, o: OutRecId) -> R<OutPtId> {
        match self.base.outrec(o)?.pts {
            Some(p) => Ok(p),
            None => invalid("ring"),
        }
    }

    /// `[outrec, k]`: the k-th point after the record's `pts`.
    fn outpt(&self, v: &Value) -> R<OutPtId> {
        match v.as_array() {
            Some(a) if a.len() == 2 => {
                let o = self.outrec(&a[0])?;
                let ring = self.ring(self.pts(o)?)?;
                match ring.get(index_of(&a[1])?) {
                    Some(&op) => Ok(op),
                    None => invalid("outpt"),
                }
            }
            _ => invalid("outpt"),
        }
    }

    fn outpt_json(&self, op: Option<OutPtId>) -> R<Value> {
        let Some(op) = op else { return Ok(Value::Null) };
        let o = self.base.out_pt(op)?;
        Ok(json!({"pt": write_point(&o.pt), "outrec": o.outrec.map(|r| r.0)}))
    }

    fn hot(&self, e: ActiveId) -> R<bool> {
        Ok(self.base.active(e)?.outrec.is_some())
    }

    fn in_list(&self, e: ActiveId, sel: bool) -> R<bool> {
        Ok(if sel { self.sel()? } else { self.ael()? }.contains(&e))
    }

    /// Every live edge's output record has points and, when closed, both sides.
    fn consistent(&self) -> R<()> {
        for &(e, alive) in &self.handles {
            if !alive {
                continue;
            }
            if let Some(o) = self.base.active(e)?.outrec {
                let rec = self.base.outrec(o)?;
                if rec.pts.is_none()
                    || (!rec.is_open && (rec.front_edge.is_none() || rec.back_edge.is_none()))
                {
                    return invalid("inconsistent output record");
                }
            }
        }
        Ok(())
    }

    /// The harness domain of arithmetic steps: every vertex and output point, and the step's
    /// own points, inside Clipper2's declared `MIN_COORD..MAX_COORD`.
    fn domain(&self, points: &[Point64]) -> R<()> {
        const OUTSIDE: &str = "outside the guarded engine domain: engine";
        if !points.iter().all(in_domain) {
            return invalid(OUTSIDE);
        }
        let chunks = self
            .base
            .vertex_list_slots()
            .iter()
            .filter(|s| s.owned())
            .map(|s| s.chunk())
            .chain(self.reuse.vertex_lists().iter());
        for chunk in chunks {
            for i in 0..chunk.len() {
                if !in_domain(&chunk.get(i)?.pt) {
                    return invalid(OUTSIDE);
                }
            }
        }
        for rec in self.base.outrecs() {
            if let Some(pts) = rec.pts {
                for op in self.ring(pts)? {
                    if !in_domain(&self.base.out_pt(op)?.pt) {
                        return invalid(OUTSIDE);
                    }
                }
            }
        }
        Ok(())
    }

    fn vref(&self, id: Option<VertexId>) -> Value {
        let Some(id) = id else { return Value::Null };
        let slots = self.base.vertex_list_slots();
        let Some(slot) = slots.get(id.list) else {
            return json!("dangling");
        };
        if slot.owned() {
            let ordinal = slots.iter().take(id.list).filter(|s| s.owned()).count();
            return json!(["b", ordinal, id.index]);
        }
        match self
            .reuse
            .vertex_lists()
            .iter()
            .position(|c| Rc::ptr_eq(c, slot.chunk()))
        {
            Some(list) => json!(["r", list, id.index]),
            None => json!("dangling"),
        }
    }

    fn minima_position(&self, id: Option<LocalMinimaId>) -> Value {
        let Some(id) = id else { return Value::Null };
        match self.base.minima_list().iter().position(|&m| m == id) {
            Some(i) => json!(i),
            None => json!("unknown"),
        }
    }

    fn minima_index(&self, v: &Value) -> R<LocalMinimaId> {
        match self.base.minima_list().get(index_of(v)?) {
            Some(&id) => Ok(id),
            None => invalid("minima"),
        }
    }

    fn dump(&self) -> R<Value> {
        let array = |tag: &str, chunk: &Rc<p001::VertexChunk>, list: usize| -> R<Value> {
            let mut out = Vec::new();
            for i in 0..chunk.len() {
                let v = chunk.get(i)?;
                let link = |l: Option<VertexId>| match l {
                    None => Value::Null,
                    Some(l) => json!([tag, list, l.index]),
                };
                out.push(json!({"pt": write_point(&v.pt), "next": link(v.next),
                                "prev": link(v.prev), "flags": v.flags.0}));
            }
            Ok(Value::Array(out))
        };
        let mut base_arrays = Vec::new();
        for slot in self.base.vertex_list_slots().iter().filter(|s| s.owned()) {
            base_arrays.push(array("b", slot.chunk(), base_arrays.len())?);
        }
        let mut reuse_arrays = Vec::new();
        for (i, chunk) in self.reuse.vertex_lists().iter().enumerate() {
            reuse_arrays.push(array("r", chunk, i)?);
        }
        let mut minima = Vec::new();
        for &id in self.base.minima_list() {
            let lm = self.base.local_minima(id)?;
            minima.push(json!({"vertex": self.vref(Some(lm.vertex)),
                               "polytype": path_type_name(lm.polytype), "is_open": lm.is_open}));
        }
        let reuse_minima: Vec<Value> = self
            .reuse
            .minima()
            .iter()
            .map(|lm| {
                json!({"vertex": ["r", lm.vertex.list, lm.vertex.index],
                       "polytype": path_type_name(lm.polytype), "is_open": lm.is_open})
            })
            .collect();
        let handles = |list: Vec<ActiveId>| -> Value {
            Value::Array(list.into_iter().map(|e| self.handle_of(Some(e))).collect())
        };
        let mut edges = Vec::new();
        for &(e, alive) in &self.handles {
            if !alive {
                edges.push(Value::Null);
                continue;
            }
            let a = self.base.active(e)?;
            edges.push(json!({
                "bot": write_point(&a.bot), "top": write_point(&a.top), "curr_x": a.curr_x,
                "dx": bits(a.dx), "wind_dx": a.wind_dx, "wind_cnt": a.wind_cnt,
                "wind_cnt2": a.wind_cnt2, "outrec": a.outrec.map(|o| o.0),
                "prev_in_ael": self.handle_of(a.prev_in_ael), "next_in_ael": self.handle_of(a.next_in_ael),
                "prev_in_sel": self.handle_of(a.prev_in_sel), "next_in_sel": self.handle_of(a.next_in_sel),
                "jump": self.handle_of(a.jump), "vertex_top": self.vref(a.vertex_top),
                "local_min": self.minima_position(a.local_min), "is_left_bound": a.is_left_bound,
                "join_with": join_name(a.join_with),
            }));
        }
        let mut outrecs = Vec::new();
        for rec in self.base.outrecs() {
            let (ring, prev_ok) = match rec.pts {
                None => (Value::Null, true),
                Some(pts) => {
                    let ring = self.ring(pts)?;
                    let mut points = Vec::new();
                    let mut prev_ok = true;
                    for &op in &ring {
                        let o = self.base.out_pt(op)?;
                        prev_ok &= self.base.out_pt(o.next)?.prev == op;
                        points.push(json!([
                            o.pt.x,
                            o.pt.y,
                            o.pt.z,
                            o.outrec.map_or(-1, |r| r.0 as i64)
                        ]));
                    }
                    (Value::Array(points), prev_ok)
                }
            };
            outrecs.push(json!({
                "idx": rec.idx, "owner": rec.owner.map(|o| o.0),
                "front_edge": self.handle_of(rec.front_edge), "back_edge": self.handle_of(rec.back_edge),
                "is_open": rec.is_open, "pts": ring, "prev_ok": prev_ok,
                "splits": rec.splits.as_ref().map(|s| s.iter().map(|o| o.0).collect::<Vec<_>>()),
                "pip_index": rec.pip_index.as_ref().map(|i| json!({"edge_count": i.edge_count(), "empty": i.empty()})),
                "path": write_path(&rec.path),
            }));
        }
        let calls = match self.calls.try_borrow() {
            Ok(calls) => Value::Array(calls.clone()),
            Err(_) => return invalid("recorder"),
        };
        Ok(json!({
            "vertices": {"base": base_arrays, "reuse": reuse_arrays},
            "minima": minima, "reuse_minima": reuse_minima,
            "locmin_iter": if self.iter_valid { json!(self.base.current_locmin_iter()) } else { Value::Null },
            "sorted": self.base.minima_list_sorted(), "has_open_paths": self.base.has_open_paths(),
            "succeeded": self.base.succeeded(), "scanlines": self.base.scanlines()?,
            "ael": handles(self.ael()?), "sel": handles(self.sel()?),
            "edges": edges, "outrecs": outrecs, "z_calls": calls,
        }))
    }

    fn step(&mut self, s: &Value) -> R<Value> {
        let name = text(s, "s")?;
        let result = self.run_step(name, s)?;
        self.register()?;
        Ok(result)
    }

    #[allow(clippy::too_many_lines)]
    fn run_step(&mut self, name: &str, s: &Value) -> R<Value> {
        match name {
            "add" => {
                keys(s, &["s", "target", "path_type", "is_open", "paths"])?;
                let polytype = path_type(&s["path_type"])?;
                let is_open = boolean(&s["is_open"])?;
                let paths = read_paths(&s["paths"])?;
                match text(s, "target")? {
                    "base" => {
                        self.base.add_paths(&paths, polytype, is_open)?;
                        self.iter_valid = false;
                    }
                    "reuse" => self.reuse.add_paths(&paths, polytype, is_open)?,
                    _ => return invalid("target"),
                }
                Ok(Value::Null)
            }
            "add_path" => {
                keys(s, &["s", "path_type", "is_open", "path"])?;
                let polytype = path_type(&s["path_type"])?;
                let is_open = boolean(&s["is_open"])?;
                self.base
                    .add_path(&read_path(&s["path"])?, polytype, is_open)?;
                self.iter_valid = false;
                Ok(Value::Null)
            }
            "add_reuseable_data" => {
                keys(s, &["s"])?;
                self.base.add_reuseable_data(&self.reuse)?;
                self.base_borrows = true;
                self.iter_valid = false;
                Ok(Value::Null)
            }
            "clear" => {
                keys(s, &["s", "target"])?;
                match text(s, "target")? {
                    "base" => {
                        self.base.clear()?;
                        self.forget_edges()?;
                        self.base_borrows = false;
                        self.iter_valid = true;
                    }
                    "reuse" => {
                        if self.base_borrows {
                            return invalid("reuse cleared while borrowed");
                        }
                        self.reuse.clear();
                    }
                    _ => return invalid("target"),
                }
                Ok(Value::Null)
            }
            "clean_up" => {
                keys(s, &["s"])?;
                self.base.clean_up()?;
                self.forget_edges()?;
                Ok(Value::Null)
            }
            "reset" => {
                keys(s, &["s"])?;
                self.base.reset()?;
                self.iter_valid = true;
                Ok(Value::Null)
            }
            "settings" => {
                keys(
                    s,
                    &[
                        "s",
                        "clip_type",
                        "fill_rule",
                        "using_polytree",
                        "preserve_collinear",
                        "z",
                        "default_z",
                    ],
                )?;
                let ct = clip_type(&s["clip_type"])?;
                let fr = fill_rule(&s["fill_rule"])?;
                let polytree = boolean(&s["using_polytree"])?;
                let preserve = boolean(&s["preserve_collinear"])?;
                let default_z = i64_of(&s["default_z"])?;
                let slot = if s["z"].is_null() {
                    ZCallbackSlot::Empty
                } else {
                    let z_base = i64_of(&s["z"])?;
                    if !(-Z_LIMIT..=Z_LIMIT).contains(&z_base) {
                        return invalid("z");
                    }
                    let calls = Rc::clone(&self.calls);
                    ZCallbackSlot::User(Box::new(move |e1b, e1t, e2b, e2t, pt: &mut Point64| {
                        if let Ok(mut calls) = calls.try_borrow_mut() {
                            calls.push(json!([
                                write_point(e1b),
                                write_point(e1t),
                                write_point(e2b),
                                write_point(e2t),
                                write_point(pt)
                            ]));
                            pt.z = z_base + calls.len() as i64;
                        }
                    }))
                };
                self.base.set_clip_type(ct);
                self.base.set_fill_rule(fr);
                self.base.set_using_polytree(polytree);
                self.base.set_preserve_collinear(preserve);
                self.base.set_z_callback_slot(slot);
                self.base.default_z = default_z;
                Ok(Value::Null)
            }
            "insert_scanline" => {
                keys(s, &["s", "y"])?;
                self.base.insert_scanline(i64_of(&s["y"])?)?;
                Ok(Value::Null)
            }
            "pop_scanline" => {
                keys(s, &["s"])?;
                Ok(json!({"y": self.base.pop_scanline()}))
            }
            "pop_local_minima" => {
                keys(s, &["s", "y"])?;
                if !self.iter_valid {
                    return invalid("iterator");
                }
                let lm = self.base.pop_local_minima(i64_of(&s["y"])?)?;
                Ok(self.minima_position(lm))
            }
            "insert_local_minima" => {
                keys(s, &["s", "y"])?;
                if !self.iter_valid {
                    return invalid("iterator");
                }
                self.domain(&[])?;
                self.consistent()?;
                self.base.insert_local_minima_into_ael(i64_of(&s["y"])?)?;
                Ok(Value::Null)
            }
            "new_active" => {
                keys(s, &["s", "minima", "bound"])?;
                let lm = self.minima_index(&s["minima"])?;
                let descending = match text(s, "bound")? {
                    "left" => true,
                    "right" => false,
                    _ => return invalid("bound"),
                };
                let id = self.base.new_bound(lm, descending)?;
                self.handles.push((id, true));
                Ok(json!(self.handles.len() - 1))
            }
            "set_active" => {
                keys(
                    s,
                    &[
                        "s",
                        "e",
                        "wind_cnt",
                        "wind_cnt2",
                        "curr_x",
                        "is_left_bound",
                        "dx",
                    ],
                )?;
                let e = self.edge(&s["e"])?;
                let wind = |v: &Value| -> R<Option<i32>> {
                    if v.is_null() {
                        return Ok(None);
                    }
                    let w = i64_of(v)?;
                    if !(-WIND_LIMIT..=WIND_LIMIT).contains(&w) {
                        return invalid("wind");
                    }
                    Ok(Some(w as i32))
                };
                let wc = wind(&s["wind_cnt"])?;
                let wc2 = wind(&s["wind_cnt2"])?;
                let curr_x = if s["curr_x"].is_null() {
                    None
                } else {
                    Some(i64_of(&s["curr_x"])?)
                };
                let left = if s["is_left_bound"].is_null() {
                    None
                } else {
                    Some(boolean(&s["is_left_bound"])?)
                };
                let dx = if s["dx"].is_null() {
                    None
                } else {
                    Some(f64_of(&s["dx"])?)
                };
                let a = self.base.active_mut(e)?;
                if let Some(w) = wc {
                    a.wind_cnt = w;
                }
                if let Some(w) = wc2 {
                    a.wind_cnt2 = w;
                }
                if let Some(x) = curr_x {
                    a.curr_x = x;
                }
                if let Some(l) = left {
                    a.is_left_bound = l;
                }
                if let Some(d) = dx {
                    a.dx = d;
                }
                Ok(Value::Null)
            }
            "insert_left_edge" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                if self.in_list(e, false)? {
                    return invalid("list");
                }
                self.domain(&[])?;
                self.base.insert_left_edge(e)?;
                Ok(Value::Null)
            }
            "insert_right_edge" => {
                keys(s, &["s", "e", "e2"])?;
                let e = self.edge(&s["e"])?;
                let e2 = self.edge(&s["e2"])?;
                if e == e2 || self.in_list(e2, false)? {
                    return invalid("list");
                }
                self.base.insert_right_edge(e, e2)?;
                Ok(Value::Null)
            }
            "push_horz" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                if self.in_list(e, true)? {
                    return invalid("list");
                }
                self.base.push_horz(e)?;
                Ok(Value::Null)
            }
            "pop_horz" => {
                keys(s, &["s"])?;
                let e = self.base.pop_horz()?;
                Ok(self.handle_of(e))
            }
            "extract_from_sel" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                let sel = self.sel()?;
                let Some(pos) = sel.iter().position(|&x| x == e) else {
                    return invalid("list");
                };
                let prev = self.base.active(e)?.prev_in_sel;
                if prev.is_none() {
                    return ub();
                }
                if pos == 0 || prev != sel.get(pos - 1).copied() {
                    return invalid("list");
                }
                let r = self.base.extract_from_sel(e)?;
                Ok(self.handle_of(r))
            }
            "insert_before_sel" => {
                keys(s, &["s", "e1", "e2"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                let sel = self.sel()?;
                let Some(pos) = sel.iter().position(|&x| x == e2) else {
                    return invalid("list");
                };
                let expected = if pos == 0 {
                    None
                } else {
                    sel.get(pos - 1).copied()
                };
                if sel.contains(&e1) || self.base.active(e2)?.prev_in_sel != expected {
                    return invalid("list");
                }
                self.base.insert1_before2_in_sel(e1, e2)?;
                Ok(Value::Null)
            }
            "wind_closed" | "wind_open" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                if !self.in_list(e, false)? {
                    return invalid("list");
                }
                if name == "wind_closed" {
                    self.base.set_wind_count_for_closed_path_edge(e)?;
                } else {
                    self.base.set_wind_count_for_open_path_edge(e)?;
                }
                Ok(Value::Null)
            }
            "contributing" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                Ok(json!({"closed": self.base.is_contributing_closed(e)?,
                          "open": self.base.is_contributing_open(e)?}))
            }
            "valid_ael_order" => {
                keys(s, &["s", "a", "b"])?;
                let a = self.edge(&s["a"])?;
                let b = self.edge(&s["b"])?;
                self.domain(&[])?;
                Ok(json!(self.base.is_valid_ael_order(a, b)?))
            }
            "local_min_poly" => {
                keys(s, &["s", "e1", "e2", "pt", "is_new"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                let pt = read_point(&s["pt"])?;
                // The default argument is `is_new = false`.
                let is_new = if s["is_new"].is_null() {
                    false
                } else {
                    boolean(&s["is_new"])?
                };
                let op = self.base.add_local_min_poly(e1, e2, &pt, is_new)?;
                self.outpt_json(Some(op))
            }
            "local_max_poly" | "intersect_edges" => {
                keys(s, &["s", "e1", "e2", "pt"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                let pt = read_point(&s["pt"])?;
                if e1 == e2 {
                    return invalid("edge");
                }
                self.consistent()?;
                if name == "intersect_edges" {
                    self.base.intersect_edges(e1, e2, &pt)?;
                    return Ok(Value::Null);
                }
                if !self.hot(e1)? || !self.hot(e2)? {
                    return ub();
                }
                let (o1, o2) = (self.base.active(e1)?.outrec, self.base.active(e2)?.outrec);
                if let (Some(o), true) = (o1, o1 == o2) {
                    let rec = self.base.outrec(o)?;
                    if rec.front_edge.is_none() || rec.back_edge.is_none() {
                        return ub();
                    }
                }
                let op = self.base.add_local_max_poly(e1, e2, &pt)?;
                self.outpt_json(op)
            }
            "join_outrec_paths" => {
                keys(s, &["s", "e1", "e2"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                self.consistent()?;
                if !self.hot(e1)? || !self.hot(e2)? {
                    return ub();
                }
                if self.base.active(e1)?.outrec == self.base.active(e2)?.outrec {
                    return invalid("owner");
                }
                self.base.join_outrec_paths(e1, e2)?;
                Ok(Value::Null)
            }
            "add_out_pt" => {
                keys(s, &["s", "e", "pt"])?;
                let e = self.edge(&s["e"])?;
                let pt = read_point(&s["pt"])?;
                self.consistent()?;
                if !self.hot(e)? {
                    return ub();
                }
                let op = self.base.add_out_pt(e, &pt)?;
                self.outpt_json(Some(op))
            }
            "start_open_path" => {
                keys(s, &["s", "e", "pt"])?;
                let e = self.edge(&s["e"])?;
                let op = self.base.start_open_path(e, &read_point(&s["pt"])?)?;
                self.outpt_json(Some(op))
            }
            "swap_outrecs" => {
                keys(s, &["s", "e1", "e2"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                if !self.hot(e1)? && !self.hot(e2)? {
                    return ub();
                }
                self.base.swap_outrecs(e1, e2)?;
                Ok(Value::Null)
            }
            "uncouple" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                if let Some(o) = self.base.active(e)?.outrec {
                    let rec = self.base.outrec(o)?;
                    if rec.front_edge.is_none() || rec.back_edge.is_none() {
                        return ub();
                    }
                }
                self.base.uncouple_outrec(e)?;
                Ok(Value::Null)
            }
            "prev_hot_edge" | "matching_loc_min" | "maxima_pair" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                let r = match name {
                    "prev_hot_edge" => self.base.get_prev_hot_edge(e)?,
                    "matching_loc_min" => self.base.find_edge_with_matching_loc_min(e)?,
                    _ => self.base.get_maxima_pair(e)?,
                };
                Ok(self.handle_of(r))
            }
            "curr_y_maxima" => {
                keys(s, &["s", "e", "open"])?;
                let e = self.edge(&s["e"])?;
                let r = if boolean(&s["open"])? {
                    self.base.get_curr_y_maxima_vertex_open(e)?
                } else {
                    self.base.get_curr_y_maxima_vertex(e)?
                };
                Ok(self.vref(r))
            }
            "next_vertex" | "prev_prev_vertex" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                let r = if name == "next_vertex" {
                    self.base.next_vertex(e)?
                } else {
                    self.base.prev_prev_vertex(e)?
                };
                Ok(self.vref(r))
            }
            "trim_horz" => {
                keys(s, &["s", "e", "preserve_collinear"])?;
                let e = self.edge(&s["e"])?;
                let preserve = boolean(&s["preserve_collinear"])?;
                self.domain(&[])?;
                self.base.trim_horz(e, preserve)?;
                Ok(Value::Null)
            }
            "update_edge" => {
                keys(s, &["s", "e"])?;
                let e = self.edge(&s["e"])?;
                self.domain(&[])?;
                self.consistent()?;
                self.base.update_edge_into_ael(e)?;
                Ok(Value::Null)
            }
            "top_x" => {
                keys(s, &["s", "e", "y"])?;
                let e = self.edge(&s["e"])?;
                Ok(json!(p001::top_x(self.base.active(e)?, i64_of(&s["y"])?)?))
            }
            "set_z" => {
                keys(s, &["s", "e1", "e2", "pt"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                let mut pt = read_point(&s["pt"])?;
                self.base.set_z(e1, e2, &mut pt)?;
                Ok(write_point(&pt))
            }
            "edges_adjacent" => {
                keys(s, &["s", "e1", "e2"])?;
                let e1 = self.edge(&s["e1"])?;
                let e2 = self.edge(&s["e2"])?;
                let node = IntersectNode::new(Some(e1), Some(e2), &Point64::default());
                Ok(json!(self.base.edges_adjacent_in_ael(&node)?))
            }
            "new_outrec" => {
                keys(s, &["s", "path", "owner", "is_open"])?;
                let path = read_path(&s["path"])?;
                let owner = self.opt_outrec(&s["owner"])?;
                let is_open = boolean(&s["is_open"])?;
                let o = self.base.new_outrec()?;
                let rec = self.base.outrec_mut(o)?;
                rec.owner = owner;
                rec.is_open = is_open;
                let pts = build_ring(&mut self.base, Some(o), &path)?;
                self.base.outrec_mut(o)?.pts = pts;
                Ok(json!(o.0))
            }
            "set_path" => {
                keys(s, &["s", "o", "path"])?;
                let o = self.outrec(&s["o"])?;
                let path = read_path(&s["path"])?;
                self.base.outrec_mut(o)?.path = path;
                Ok(Value::Null)
            }
            "set_owner" => {
                keys(s, &["s", "o", "new_owner"])?;
                let o = self.outrec(&s["o"])?;
                let n = self.outrec(&s["new_owner"])?;
                if o == n {
                    return invalid("owner");
                }
                self.base.set_owner(o, n)?;
                Ok(Value::Null)
            }
            "real_outrec" => {
                keys(s, &["s", "o"])?;
                let o = self.opt_outrec(&s["o"])?;
                Ok(json!(self.base.get_real_outrec(o)?.map(|r| r.0)))
            }
            "valid_owner" => {
                keys(s, &["s", "o", "test"])?;
                let o = self.opt_outrec(&s["o"])?;
                let t = self.opt_outrec(&s["test"])?;
                Ok(json!(self.base.is_valid_owner(o, t)?))
            }
            "clean_collinear" => {
                keys(s, &["s", "o"])?;
                let o = self.opt_outrec(&s["o"])?;
                self.domain(&[])?;
                self.base.clean_collinear(o)?;
                Ok(Value::Null)
            }
            "fix_self_intersects" | "do_split_op" => {
                keys(
                    s,
                    if name == "do_split_op" {
                        &["s", "o", "at"]
                    } else {
                        &["s", "o"]
                    },
                )?;
                let o = self.outrec(&s["o"])?;
                let ring = self.ring(self.pts(o)?)?;
                if ring.len() < 3 {
                    return invalid("ring");
                }
                self.domain(&[])?;
                if name == "fix_self_intersects" {
                    self.base.fix_self_intersects(o)?;
                } else {
                    let Some(&op) = ring.get(index_of(&s["at"])?) else {
                        return invalid("outpt");
                    };
                    self.base.do_split_op(o, op)?;
                }
                Ok(Value::Null)
            }
            "area"
            | "point_count"
            | "very_small_triangle"
            | "swap_sides"
            | "update_outrec_owner"
            | "dispose_out_pts"
            | "clean_path" => {
                keys(s, &["s", "o"])?;
                let o = self.outrec(&s["o"])?;
                let pts = self.pts(o)?;
                match name {
                    "area" => {
                        self.domain(&[])?;
                        Ok(bits(self.base.area_of(pts)?))
                    }
                    "point_count" => Ok(json!(self.base.point_count(pts)?)),
                    "very_small_triangle" => {
                        self.domain(&[])?;
                        Ok(json!(self.base.is_very_small_triangle(pts)?))
                    }
                    "swap_sides" => {
                        self.base.swap_sides(o)?;
                        Ok(Value::Null)
                    }
                    "update_outrec_owner" => {
                        self.base.update_outrec_owner(o)?;
                        Ok(Value::Null)
                    }
                    "dispose_out_pts" => {
                        self.base.dispose_out_pts(o)?;
                        Ok(Value::Null)
                    }
                    _ => Ok(write_path(&self.base.get_clean_path(pts)?)),
                }
            }
            "valid_closed_path" | "reverse_out_pts" | "invalid_path" => {
                keys(s, &["s", "o"])?;
                let o = self.outrec(&s["o"])?;
                let pts = self.base.outrec(o)?.pts;
                match name {
                    "valid_closed_path" => {
                        self.domain(&[])?;
                        Ok(json!(self.base.is_valid_closed_path(pts)?))
                    }
                    "invalid_path" => Ok(json!(self.base.is_invalid_path(pts)?)),
                    _ => {
                        self.base.reverse_out_pts(pts)?;
                        Ok(Value::Null)
                    }
                }
            }
            "duplicate_op" => {
                keys(s, &["s", "op", "after"])?;
                let op = self.outpt(&s["op"])?;
                let after = boolean(&s["after"])?;
                let r = self.base.duplicate_op(op, after)?;
                self.outpt_json(Some(r))
            }
            "dispose_out_pt" => {
                keys(s, &["s", "op"])?;
                let op = self.outpt(&s["op"])?;
                if self.base.outrecs().iter().any(|rec| rec.pts == Some(op)) {
                    return invalid("outpt");
                }
                let r = self.base.dispose_out_pt(op)?;
                self.outpt_json(Some(r))
            }
            "point_in_op_polygon" => {
                keys(s, &["s", "pt", "o"])?;
                let pt = read_point(&s["pt"])?;
                let o = self.outrec(&s["o"])?;
                let pts = self.pts(o)?;
                self.domain(&[pt])?;
                Ok(json!(pip_name(self.base.point_in_op_polygon(&pt, pts)?)))
            }
            "contains" | "contains_indexed" => {
                let indexed = name == "contains_indexed";
                if indexed {
                    keys(s, &["s", "o1", "o2", "threshold"])?;
                } else {
                    keys(s, &["s", "o1", "o2"])?;
                }
                let o1 = self.outrec(&s["o1"])?;
                let o2 = self.outrec(&s["o2"])?;
                let p1 = self.pts(o1)?;
                let p2 = self.pts(o2)?;
                self.domain(&[])?;
                if indexed {
                    let threshold = u64_of(&s["threshold"])?;
                    Ok(json!(self
                        .base
                        .path2_contains_path1_indexed(p1, o2, threshold)?))
                } else {
                    Ok(json!(self.base.path2_contains_path1(p1, Some(p2))?))
                }
            }
            "horz_seg_sorter" => {
                keys(s, &["s", "a", "b"])?;
                let seg = |v: &Value| -> R<HorzSegment> {
                    keys(v, &["left", "right"])?;
                    let op = |x: &Value| -> R<Option<OutPtId>> {
                        if x.is_null() {
                            Ok(None)
                        } else {
                            Ok(Some(self.outpt(x)?))
                        }
                    };
                    Ok(HorzSegment {
                        left_op: op(&v["left"])?,
                        right_op: op(&v["right"])?,
                        left_to_right: true,
                    })
                };
                let (a, b) = (seg(&s["a"])?, seg(&s["b"])?);
                if a.right_op.is_some()
                    && b.right_op.is_some()
                    && (a.left_op.is_none() || b.left_op.is_none())
                {
                    return ub();
                }
                Ok(json!(p001::horz_seg_sorter(&self.base, &a, &b)?))
            }
            _ => invalid("step"),
        }
    }
}

fn run_engine(v: &Value) -> R<Value> {
    keys(v, &["op", "steps"])?;
    let Some(steps) = v["steps"].as_array() else {
        return invalid("steps");
    };
    if steps.len() > MAX_STEPS {
        return invalid("steps");
    }
    let mut engine = Engine::new();
    let mut results = Vec::new();
    for s in steps {
        results.push(engine.step(s)?);
    }
    Ok(json!({"results": results, "state": engine.dump()?}))
}

fn run(v: &Value) -> R<Value> {
    let op = text(v, "op")?;
    match op {
        "stripe_index" => run_stripe_index(v),
        "engine" => run_engine(v),
        _ => run_pure(op, v),
    }
}

/// Answer one JSONL request exactly as the C++ oracle adapter does.
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
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match run(input) {
        Ok(result) => json!({"unit": unit, "case": case, "result": result}),
        Err(E::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(E::Clip(ClipperError::Undefined(_))) => failure(
            unit,
            case,
            "invalid_input",
            &format!("undefined behaviour: {op}"),
        ),
        Err(E::Clip(ClipperError::Unported {
            upstream,
            unit: owner,
        })) => {
            json!({"unit": unit, "case": case, "error": {"kind": "unported", "upstream": upstream, "unit": owner}})
        }
        Err(E::Clip(ClipperError::Exception(e))) => {
            failure(unit, case, "execution_error", e.what())
        }
        Err(E::Clip(other)) => failure(unit, case, "execution_error", &other.to_string()),
    }
}
