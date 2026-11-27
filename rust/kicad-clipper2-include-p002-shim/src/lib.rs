// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for unit `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`
//! (`kicad_clipper2::clipper`, `clipper_minkowski`, `clipper_triangulation` and the
//! `clipper.offset.h` members), mirroring `port/oracle/reference/clipper2_include_p002_ops.inc`
//! request for request: the same operations, field order, input domain and resource guards,
//! and replies. Points are `[x, y, z]`; doubles travel as their 16 lowercase hex digit bit
//! patterns.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_clipper2::clipper::{self as c2, PathInteger, PathNumber};
use kicad_clipper2::clipper_engine::{ClipType, PolyPathId, PolyTree, PolyTree64, PolyTreeD};
use kicad_clipper2::clipper_minkowski as mk;
use kicad_clipper2::clipper_triangulation::CLIPPER2_VERSION;
use kicad_clipper2::offset::{ClipperOffset, EndType, JoinType, ARC_CONST, PI};
use kicad_clipper2::rectclip::RectClipBudget;
use kicad_clipper2::{
    Budget, ClipperError, Coord, FillRule, Path, Path64, PathD, Paths, Paths64, Point, Rect,
};
use kicad_math::reference_libm::{acos, pow};
use kicad_math::{FmtFlags, StreamState};
use serde_json::{json, Value};
use std::result::Result;

/// This port unit.
pub const UNIT: &str = kicad_clipper2::INCLUDE_P002;

/// Request limits, identical in the C++ adapter.
const MAX_POINTS: usize = 1 << 16;
const MAX_NODES: usize = 4096;
const MAX_VALUES: usize = 4096;
const MAX_SIMPLIFY: usize = 4096;
const MAX_RDP: usize = 1024;
const MAX_QUADS: usize = 1 << 14;
const MAX_FLAGS: usize = 4096;
const MAX_GROUPS: usize = 16;
const MAX_SETS: usize = 16;
const MAX_PREAMBLE: usize = 256;
const STREAM_BOUND: (i64, i64) = (-64, 512);
/// Clipper2's declared coordinate range (`INT64_MAX >> 2`), the engine domain.
const ENGINE_COORD: i64 = i64::MAX >> 2;
/// Minkowski inputs: half the engine domain.
const MINKOWSKI_COORD: i64 = ENGINE_COORD / 2;
const MINKOWSKI_COORD_D: f64 = MINKOWSKI_COORD as f64;
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
/// The offset domain (as the src p002 adapter).
const OFFSET_LIMIT: f64 = 1_099_511_627_776.0; // 2^40
const OFFSET_COORD: i64 = 1 << 40;
const MAX_ARC_STEPS: f64 = 65536.0;
const MAX_OFFSET_POINTS: f64 = 4_194_304.0; // 2^22
const MAX_ELLIPSE_STEPS: f64 = 1_048_576.0; // 2^20
/// `clipper.offset.cpp` `floating_point_tolerance`.
const FP_TOLERANCE: f64 = 1e-12;

/// Host budgets: far above what any request within the guards needs, so they only stop
/// growth the guards did not foresee (reported as a harness limit, never a result).
pub const TREE_BUDGET: Budget = Budget {
    nodes: 1 << 20,
    points: 1 << 24,
};
pub const OFFSET_BUDGET: Budget = Budget {
    nodes: 1 << 20,
    points: 1 << 24,
};
pub const RECT_BUDGET: RectClipBudget = RectClipBudget {
    ops: 1 << 22,
    steps: 1 << 30,
};
pub const ELLIPSE_POINTS: usize = 1 << 21;

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

fn text(v: &Value) -> R<&str> {
    match v.as_str() {
        Some(s) => Ok(s),
        None => invalid("string"),
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
    match v.as_u64() {
        Some(i) => Ok(i),
        None => invalid("uint64"),
    }
}

fn int_of(v: &Value) -> R<i32> {
    let value = i64_of(v)?;
    match i32::try_from(value) {
        Ok(i) => Ok(i),
        Err(_) => invalid("int"),
    }
}

fn enum_of(v: &Value, count: i64) -> R<i64> {
    let value = i64_of(v)?;
    if !(0..count).contains(&value) {
        return invalid("enum");
    }
    Ok(value)
}

fn f64_of(v: &Value) -> R<f64> {
    let Some(s) = v.as_str() else {
        return invalid("double");
    };
    if s.len() != 16
        || !s
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return invalid("double");
    }
    match u64::from_str_radix(s, 16) {
        Ok(bits) => Ok(f64::from_bits(bits)),
        Err(_) => invalid("double"),
    }
}

fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

fn array<'a>(v: &'a Value, what: &str) -> R<&'a Vec<Value>> {
    match v.as_array() {
        Some(a) => Ok(a),
        None => invalid(what),
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

fn grow<T>(v: &mut Vec<T>, item: T) -> R<()> {
    v.try_reserve(1)
        .map_err(|_| E::Clip(ClipperError::Allocation))?;
    v.push(item);
    Ok(())
}

/// The coordinate types on the wire.
trait Wire: Coord {
    const DOUBLE: bool;
    fn read(v: &Value) -> R<Self>;
    fn write(self) -> Value;
}

impl Wire for i64 {
    const DOUBLE: bool = false;
    fn read(v: &Value) -> R<Self> {
        i64_of(v)
    }
    fn write(self) -> Value {
        json!(self)
    }
}

impl Wire for f64 {
    const DOUBLE: bool = true;
    fn read(v: &Value) -> R<Self> {
        f64_of(v)
    }
    fn write(self) -> Value {
        bits(self)
    }
}

fn read_point<T: Wire>(v: &Value, count: &mut Counter) -> R<Point<T>> {
    match v.as_array() {
        Some(a) if a.len() == 3 => {
            count.add(1)?;
            Ok(Point {
                x: T::read(&a[0])?,
                y: T::read(&a[1])?,
                z: i64_of(&a[2])?,
            })
        }
        _ => invalid("point"),
    }
}

fn read_path<T: Wire>(v: &Value, count: &mut Counter) -> R<Path<T>> {
    let mut out = Path::new();
    for p in array(v, "path")? {
        grow(&mut out, read_point::<T>(p, count)?)?;
    }
    Ok(out)
}

fn read_paths<T: Wire>(v: &Value, count: &mut Counter) -> R<Paths<T>> {
    let mut out = Paths::new();
    for p in array(v, "paths")? {
        grow(&mut out, read_path::<T>(p, count)?)?;
    }
    Ok(out)
}

fn read_rect<T: Wire>(v: &Value) -> R<Rect<T>> {
    match v.as_array() {
        Some(a) if a.len() == 4 => Ok(Rect::new(
            T::read(&a[0])?,
            T::read(&a[1])?,
            T::read(&a[2])?,
            T::read(&a[3])?,
        )),
        _ => invalid("rect"),
    }
}

fn write_point<T: Wire>(p: &Point<T>) -> Value {
    json!([p.x.write(), p.y.write(), p.z])
}

fn write_path<T: Wire>(path: &[Point<T>]) -> Value {
    Value::Array(path.iter().map(write_point).collect())
}

fn write_paths<T: Wire>(paths: &[Path<T>]) -> Value {
    Value::Array(paths.iter().map(|p| write_path(p)).collect())
}

/// A tree in preorder below `id`: each node's parent index (-1 for `id`'s children), hole
/// flag and polygon.
fn write_tree<T: Wire>(tree: &PolyTree<T>, id: PolyPathId) -> R<Value> {
    let mut out = Vec::new();
    // (node, its output index or -1, next child)
    let mut stack: Vec<(PolyPathId, i64, usize)> = vec![(id, -1, 0)];
    while let Some(top) = stack.last_mut() {
        let (node, index, next) = *top;
        let Some(&child) = tree.children(node)?.get(next) else {
            stack.pop();
            continue;
        };
        top.2 = next + 1;
        let child_index = out.len() as i64;
        out.push(json!({"parent": index, "hole": tree.is_hole(child)?,
                        "polygon": write_path(tree.polygon(child)?)}));
        stack.push((child, child_index, 0));
    }
    Ok(Value::Array(out))
}

fn engine_domain(paths: &Paths64) -> R<()> {
    for p in paths.iter().flatten() {
        let out = |c: i64| !(-ENGINE_COORD..=ENGINE_COORD).contains(&c);
        if out(p.x) || out(p.y) {
            return invalid("domain");
        }
    }
    Ok(())
}

fn optional_int(v: &Value) -> R<Option<i32>> {
    if v.is_null() {
        Ok(None)
    } else {
        Ok(Some(int_of(v)?))
    }
}

/// `std::pow(10, precision)`.
fn pow10(precision: i32) -> f64 {
    pow(10.0, f64::from(precision))
}

fn precision_in_range(precision: i32) -> bool {
    (-8..=8).contains(&precision)
}

/// The C++ `Guarded` helper: a `Clipper2Exception` is a compared result.
fn guarded(field: &str, result: Result<Value, ClipperError>) -> R<Value> {
    match result {
        Ok(v) => Ok(json!({field: v, "exception": null})),
        Err(ClipperError::Exception(e)) => Ok(json!({field: null, "exception": e.what()})),
        Err(other) => Err(E::Clip(other)),
    }
}

fn clip_type(v: &Value) -> R<ClipType> {
    Ok(match enum_of(v, 5)? {
        0 => ClipType::NoClip,
        1 => ClipType::Intersection,
        2 => ClipType::Union,
        3 => ClipType::Difference,
        _ => ClipType::Xor,
    })
}

fn fill_rule(v: &Value) -> R<FillRule> {
    Ok(match enum_of(v, 4)? {
        0 => FillRule::EvenOdd,
        1 => FillRule::NonZero,
        2 => FillRule::Positive,
        _ => FillRule::Negative,
    })
}

fn join_type(v: &Value) -> R<JoinType> {
    Ok(match enum_of(v, 4)? {
        0 => JoinType::Square,
        1 => JoinType::Bevel,
        2 => JoinType::Round,
        _ => JoinType::Miter,
    })
}

fn end_type(v: &Value) -> R<EndType> {
    Ok(match enum_of(v, 5)? {
        0 => EndType::Polygon,
        1 => EndType::Joined,
        2 => EndType::Butt,
        3 => EndType::Square,
        _ => EndType::Round,
    })
}

// --- boolean operations --------------------------------------------------------------------

/// Typed boolean bodies (`int64_t` and `double` take different wrappers).
trait Boolean: Wire {
    fn add_child(tree: &mut PolyTree<Self>, path: &Path<Self>) -> R<()>;
    fn tree_op(
        ct: ClipType,
        fr: FillRule,
        s: &Paths<Self>,
        c: &Paths<Self>,
        tree: &mut PolyTree<Self>,
        precision: Option<i32>,
    ) -> Result<(), ClipperError>;
    fn paths_op(
        f: &str,
        ct: ClipType,
        fr: FillRule,
        s: &Paths<Self>,
        c: &Paths<Self>,
        precision: Option<i32>,
    ) -> Result<Paths<Self>, ClipperError>;
}

impl Boolean for i64 {
    fn add_child(tree: &mut PolyTree<Self>, path: &Path<Self>) -> R<()> {
        tree.add_child(PolyTree64::ROOT, path)?;
        Ok(())
    }
    fn tree_op(
        ct: ClipType,
        fr: FillRule,
        s: &Paths64,
        c: &Paths64,
        tree: &mut PolyTree64,
        _precision: Option<i32>,
    ) -> Result<(), ClipperError> {
        c2::boolean_op_64_tree(ct, fr, s, c, tree)
    }
    fn paths_op(
        f: &str,
        ct: ClipType,
        fr: FillRule,
        s: &Paths64,
        c: &Paths64,
        _precision: Option<i32>,
    ) -> Result<Paths64, ClipperError> {
        match f {
            "booleanop" => c2::boolean_op_64(ct, fr, s, c),
            "intersect" => c2::intersect_64(s, c, fr),
            "union" => c2::union_64(s, c, fr),
            "difference" => c2::difference_64(s, c, fr),
            "xor" => c2::xor_64(s, c, fr),
            _ => c2::union_subjects_64(s, fr),
        }
    }
}

impl Boolean for f64 {
    fn add_child(tree: &mut PolyTree<Self>, path: &Path<Self>) -> R<()> {
        tree.add_child_d(PolyTreeD::ROOT, path)?;
        Ok(())
    }
    fn tree_op(
        ct: ClipType,
        fr: FillRule,
        s: &Paths<f64>,
        c: &Paths<f64>,
        tree: &mut PolyTreeD,
        precision: Option<i32>,
    ) -> Result<(), ClipperError> {
        c2::boolean_op_d_tree(ct, fr, s, c, tree, precision.unwrap_or(2))
    }
    fn paths_op(
        f: &str,
        ct: ClipType,
        fr: FillRule,
        s: &Paths<f64>,
        c: &Paths<f64>,
        precision: Option<i32>,
    ) -> Result<Paths<f64>, ClipperError> {
        let p = precision.unwrap_or(2);
        match f {
            "booleanop" => c2::boolean_op_d(ct, fr, s, c, p),
            "intersect" => c2::intersect_d(s, c, fr, p),
            "union" => c2::union_d(s, c, fr, p),
            "difference" => c2::difference_d(s, c, fr, p),
            "xor" => c2::xor_d(s, c, fr, p),
            _ => c2::union_subjects_d(s, fr, p),
        }
    }
}

fn run_boolean<T: Boolean>(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op",
            "type",
            "fn",
            "clip_type",
            "fill_rule",
            "subjects",
            "clips",
            "precision",
            "prefill",
        ],
    )?;
    let f = text(at(v, "fn")?)?;
    let op = f == "booleanop" || f == "booleanop_tree";
    let tree = f == "booleanop_tree";
    let single = f == "union_subjects";
    if !op && !single && !matches!(f, "intersect" | "union" | "difference" | "xor") {
        return invalid("fn");
    }
    let mut count = Counter(0);
    let subjects = read_paths::<T>(at(v, "subjects")?, &mut count)?;
    let clips = if single {
        if !at(v, "clips")?.is_null() {
            return invalid("clips");
        }
        Paths::new()
    } else {
        read_paths::<T>(at(v, "clips")?, &mut count)?
    };
    let ct = if op {
        clip_type(at(v, "clip_type")?)?
    } else if !at(v, "clip_type")?.is_null() {
        return invalid("clip_type");
    } else {
        ClipType::NoClip
    };
    let fr = fill_rule(at(v, "fill_rule")?)?;
    let precision = optional_int(at(v, "precision")?)?;
    if !T::DOUBLE && precision.is_some() {
        return invalid("precision");
    }
    let prefill = if tree {
        read_paths::<T>(at(v, "prefill")?, &mut count)?
    } else if !at(v, "prefill")?.is_null() {
        return invalid("prefill");
    } else {
        Paths::new()
    };
    if !T::DOUBLE {
        let to64 = |paths: &Paths<T>| -> Paths64 {
            paths
                .iter()
                .map(|p| {
                    p.iter()
                        .map(|q| Point {
                            x: q.x.to_i64().unwrap_or(0),
                            y: q.y.to_i64().unwrap_or(0),
                            z: q.z,
                        })
                        .collect()
                })
                .collect()
        };
        engine_domain(&to64(&subjects))?;
        engine_domain(&to64(&clips))?;
    }
    if tree {
        let mut polytree = PolyTree::<T>::new(TREE_BUDGET)?;
        for p in &prefill {
            T::add_child(&mut polytree, p)?;
        }
        let exception = match T::tree_op(ct, fr, &subjects, &clips, &mut polytree, precision) {
            Ok(()) => Value::Null,
            Err(ClipperError::Exception(e)) => json!(e.what()),
            Err(other) => return Err(E::Clip(other)),
        };
        return Ok(json!({"tree": write_tree(&polytree, PolyTree::<T>::ROOT)?,
                         "exception": exception}));
    }
    guarded(
        "paths",
        T::paths_op(f, ct, fr, &subjects, &clips, precision).map(|r| write_paths(&r)),
    )
}

// --- offsetting ----------------------------------------------------------------------------

fn std_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// The arc steps per full circle `DoGroupOffset` computes for `abs_delta`.
fn steps_per_360(abs_delta: f64, arc_tolerance: f64) -> f64 {
    let arc_tol = if arc_tolerance > FP_TOLERANCE {
        std_min(abs_delta, arc_tolerance)
    } else {
        abs_delta * ARC_CONST
    };
    std_min(PI / acos(1.0 - arc_tol / abs_delta), abs_delta * PI)
}

/// Whether `ScalePaths<int64_t, double>(paths, scale, ...)` reports `range_error_i`: its own
/// bounds test on `GetBounds<double, double>` (comparisons only, defined for every input).
fn scale_range_error(paths: &[PathD], scale: f64) -> R<bool> {
    let r = kicad_clipper2::clipper_core::get_bounds_paths::<f64, f64>(paths)?;
    let (min, max) = (
        kicad_clipper2::clipper_core::MIN_COORD_D,
        kicad_clipper2::clipper_core::MAX_COORD_D,
    );
    Ok(r.left * scale < min
        || r.right * scale > max
        || r.top * scale < min
        || r.bottom * scale > max)
}

fn offset_finite(value: f64) -> bool {
    value.is_finite() && value.abs() <= OFFSET_LIMIT
}

fn offset_domain(paths: &Paths64) -> R<()> {
    for p in paths.iter().flatten() {
        let out = |c: i64| !(-OFFSET_COORD..=OFFSET_COORD).contains(&c);
        if out(p.x) || out(p.y) {
            return invalid("domain");
        }
    }
    Ok(())
}

fn offset_guard(
    delta: f64,
    miter_limit: f64,
    arc_tolerance: f64,
    round: bool,
    points: usize,
) -> R<()> {
    if !offset_finite(delta) || !offset_finite(miter_limit) || !offset_finite(arc_tolerance) {
        return invalid("domain");
    }
    let abs_delta = delta.abs();
    if round && abs_delta > FP_TOLERANCE {
        let steps = steps_per_360(abs_delta, arc_tolerance);
        // NaN steps fail `steps <= MAX_ARC_STEPS` and are refused, as in the C++ guard.
        let within = steps <= MAX_ARC_STEPS;
        if !within || points as f64 * (steps + 16.0) > MAX_OFFSET_POINTS {
            return invalid("arc steps");
        }
    }
    Ok(())
}

fn run_inflate<T: Wire>(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op",
            "type",
            "paths",
            "delta",
            "join",
            "end",
            "miter_limit",
            "precision",
            "arc_tolerance",
        ],
    )?;
    let mut count = Counter(0);
    let paths = read_paths::<T>(at(v, "paths")?, &mut count)?;
    let delta = f64_of(at(v, "delta")?)?;
    let jt = join_type(at(v, "join")?)?;
    let et = end_type(at(v, "end")?)?;
    let ml_in = at(v, "miter_limit")?;
    let prec_in = at(v, "precision")?;
    let at_in = at(v, "arc_tolerance")?;
    let (has_ml, has_prec, has_at) = (!ml_in.is_null(), !prec_in.is_null(), !at_in.is_null());
    let ml = if has_ml { f64_of(ml_in)? } else { 2.0 };
    let at_v = if has_at { f64_of(at_in)? } else { 0.0 };
    let precision = if has_prec { int_of(prec_in)? } else { 2 };
    if T::DOUBLE {
        if (has_prec && !has_ml) || (has_at && !has_prec) {
            return invalid("defaults");
        }
    } else if has_prec || (has_at && !has_ml) {
        return invalid("defaults");
    }
    let round = jt == JoinType::Round || et == EndType::Round;
    if T::DOUBLE {
        // `ScalePaths<int64_t, double>` throws "Values exceed permitted range" before any
        // conversion or offset work: that defined error needs no offset bound.
        if precision_in_range(precision)
            && delta != 0.0
            && !scale_range_error(&as_paths_d(&paths), pow10(precision))?
        {
            let scale = pow10(precision);
            for p in paths.iter().flatten() {
                if !offset_finite(p.x.to_f64() * scale) || !offset_finite(p.y.to_f64() * scale) {
                    return invalid("domain");
                }
            }
            offset_guard(delta * scale, ml, at_v * scale, round, count.0)?;
        }
    } else if delta != 0.0 {
        offset_domain(&as_paths64(&paths))?;
        offset_guard(delta, ml, at_v, round, count.0)?;
    }
    let result = if T::DOUBLE {
        c2::inflate_paths_d(
            &as_paths_d(&paths),
            delta,
            jt,
            et,
            ml,
            precision,
            at_v,
            OFFSET_BUDGET,
        )
        .map(|r| write_paths(&r))
    } else {
        c2::inflate_paths_64(&as_paths64(&paths), delta, jt, et, ml, at_v, OFFSET_BUDGET)
            .map(|r| write_paths(&r))
    };
    guarded("paths", result)
}

/// The `int64_t` paths of a `Wire` value read as `i64` (identity).
fn as_paths64<T: Wire>(paths: &Paths<T>) -> Paths64 {
    paths
        .iter()
        .map(|p| {
            p.iter()
                .map(|q| Point {
                    x: q.x.to_i64().unwrap_or(0),
                    y: q.y.to_i64().unwrap_or(0),
                    z: q.z,
                })
                .collect()
        })
        .collect()
}

/// The `double` paths of a `Wire` value read as `f64` (identity).
fn as_paths_d<T: Wire>(paths: &Paths<T>) -> Paths<f64> {
    paths
        .iter()
        .map(|p| {
            p.iter()
                .map(|q| Point {
                    x: q.x.to_f64(),
                    y: q.y.to_f64(),
                    z: q.z,
                })
                .collect()
        })
        .collect()
}

fn run_offset_members(v: &Value) -> R<Value> {
    keys(v, &["op", "ctor", "set", "groups", "clear", "delta"])?;
    let ctor = array(at(v, "ctor")?, "ctor")?;
    if ctor.len() > 4 {
        return invalid("ctor");
    }
    let (mut ml, mut at_v, mut pc, mut rs) = (2.0, 0.0, false, false);
    if let Some(x) = ctor.first() {
        ml = f64_of(x)?;
    }
    if let Some(x) = ctor.get(1) {
        at_v = f64_of(x)?;
    }
    if let Some(x) = ctor.get(2) {
        pc = boolean(x)?;
    }
    if let Some(x) = ctor.get(3) {
        rs = boolean(x)?;
    }
    enum Set {
        Miter(f64),
        Arc(f64),
        Preserve(bool),
        Reverse(bool),
    }
    let set_in = array(at(v, "set")?, "set")?;
    if set_in.len() > MAX_SETS {
        return invalid("set");
    }
    let (mut eff_ml, mut eff_at) = (ml, at_v);
    let mut sets = Vec::new();
    for s in set_in {
        let Some(pair) = s.as_array().filter(|a| a.len() == 2) else {
            return invalid("set");
        };
        let set = match text(&pair[0])? {
            "miter_limit" => {
                let x = f64_of(&pair[1])?;
                eff_ml = x;
                Set::Miter(x)
            }
            "arc_tolerance" => {
                let x = f64_of(&pair[1])?;
                eff_at = x;
                Set::Arc(x)
            }
            "preserve_collinear" => Set::Preserve(boolean(&pair[1])?),
            "reverse_solution" => Set::Reverse(boolean(&pair[1])?),
            _ => return invalid("set"),
        };
        sets.push(set);
    }
    let mut count = Counter(0);
    let groups_in = array(at(v, "groups")?, "groups")?;
    if groups_in.len() > MAX_GROUPS {
        return invalid("size");
    }
    let mut groups = Vec::new();
    let mut round = false;
    for g in groups_in {
        keys(g, &["paths", "join", "end", "single"])?;
        let paths = read_paths::<i64>(at(g, "paths")?, &mut count)?;
        let jt = join_type(at(g, "join")?)?;
        let et = end_type(at(g, "end")?)?;
        let single = boolean(at(g, "single")?)?;
        if single && paths.len() != 1 {
            return invalid("single");
        }
        round = round || jt == JoinType::Round || et == EndType::Round;
        groups.push((paths, jt, et, single));
    }
    let clear = boolean(at(v, "clear")?)?;
    let delta_in = at(v, "delta")?;
    let mut delta = 0.0;
    if !delta_in.is_null() {
        delta = f64_of(delta_in)?;
        for g in &groups {
            offset_domain(&g.0)?;
        }
        offset_guard(delta, eff_ml, eff_at, round, count.0)?;
    }
    let mut co = match ctor.len() {
        0 => ClipperOffset::with_defaults(OFFSET_BUDGET),
        _ => ClipperOffset::new(ml, at_v, pc, rs, OFFSET_BUDGET),
    };
    for s in sets {
        match s {
            Set::Miter(x) => co.set_miter_limit(x),
            Set::Arc(x) => co.set_arc_tolerance(x),
            Set::Preserve(b) => co.set_preserve_collinear(b),
            Set::Reverse(b) => co.set_reverse_solution(b),
        }
    }
    for (paths, jt, et, single) in &groups {
        if *single {
            let Some(first) = paths.first() else {
                return invalid("single");
            };
            co.add_path(first, *jt, *et)?;
        } else {
            co.add_paths(paths, *jt, *et)?;
        }
    }
    if clear {
        co.clear();
    }
    let mut solution = Value::Null;
    if !delta_in.is_null() {
        let mut out = Paths64::new();
        co.execute(delta, &mut out)?;
        solution = write_paths(&out);
    }
    Ok(json!({
        "miter_limit": bits(co.miter_limit()),
        "arc_tolerance": bits(co.arc_tolerance()),
        "preserve_collinear": co.preserve_collinear(),
        "reverse_solution": co.reverse_solution(),
        "error_code": co.error_code(),
        "paths": solution,
    }))
}

// --- translation and rectangle clipping ----------------------------------------------------

fn single_path(v: &Value) -> R<bool> {
    match text(at(v, "form")?)? {
        "path" => Ok(true),
        "paths" => Ok(false),
        _ => invalid("form"),
    }
}

fn run_translate<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "form", "value", "dx", "dy"])?;
    let mut count = Counter(0);
    let single = single_path(v)?;
    let dx = T::read(at(v, "dx")?)?;
    let dy = T::read(at(v, "dy")?)?;
    if single {
        let path = read_path::<T>(at(v, "value")?, &mut count)?;
        return Ok(write_path(&c2::translate_path(&path, dx, dy)?));
    }
    let paths = read_paths::<T>(at(v, "value")?, &mut count)?;
    Ok(write_paths(&c2::translate_paths(&paths, dx, dy)?))
}

fn run_rect_clip<T: Wire>(v: &Value) -> R<Value> {
    keys(
        v,
        &["op", "type", "lines", "form", "rect", "value", "precision"],
    )?;
    let mut count = Counter(0);
    let lines = boolean(at(v, "lines")?)?;
    let single = single_path(v)?;
    let rect = read_rect::<T>(at(v, "rect")?)?;
    let (path, paths) = if single {
        (read_path::<T>(at(v, "value")?, &mut count)?, Paths::new())
    } else {
        (Path::new(), read_paths::<T>(at(v, "value")?, &mut count)?)
    };
    let precision = optional_int(at(v, "precision")?)?;
    if !T::DOUBLE && precision.is_some() {
        return invalid("precision");
    }
    let result = if T::DOUBLE {
        let r = Rect::new(
            rect.left.to_f64(),
            rect.top.to_f64(),
            rect.right.to_f64(),
            rect.bottom.to_f64(),
        );
        let p = precision.unwrap_or(2);
        let path_d: Vec<_> = as_paths_d(&vec![path]).into_iter().flatten().collect();
        let paths_d = as_paths_d(&paths);
        match (lines, single) {
            (true, true) => c2::rect_clip_lines_d_path(&r, &path_d, p, RECT_BUDGET),
            (true, false) => c2::rect_clip_lines_d(&r, &paths_d, p, RECT_BUDGET),
            (false, true) => c2::rect_clip_d_path(&r, &path_d, p, RECT_BUDGET),
            (false, false) => c2::rect_clip_d(&r, &paths_d, p, RECT_BUDGET),
        }
        .map(|r| write_paths(&r))
    } else {
        let r = Rect::new(
            rect.left.to_i64()?,
            rect.top.to_i64()?,
            rect.right.to_i64()?,
            rect.bottom.to_i64()?,
        );
        let path64: Path64 = as_paths64(&vec![path]).into_iter().flatten().collect();
        let paths64 = as_paths64(&paths);
        match (lines, single) {
            (true, true) => c2::rect_clip_lines_64_path(&r, &path64, RECT_BUDGET),
            (true, false) => c2::rect_clip_lines_64(&r, &paths64, RECT_BUDGET),
            (false, true) => c2::rect_clip_64_path(&r, &path64, RECT_BUDGET),
            (false, false) => c2::rect_clip_64(&r, &paths64, RECT_BUDGET),
        }
        .map(|r| write_paths(&r))
    };
    guarded("paths", result)
}

// --- polytrees and streams -----------------------------------------------------------------

const FLAG_NAMES: [&str; 15] = [
    "boolalpha",
    "dec",
    "fixed",
    "hex",
    "internal",
    "left",
    "oct",
    "right",
    "scientific",
    "showbase",
    "showpoint",
    "showpos",
    "skipws",
    "unitbuf",
    "uppercase",
];

fn stream_state(v: &Value) -> R<StreamState> {
    keys(v, &["flags", "precision", "width", "fill"])?;
    let names = array(&v["flags"], "state")?;
    let mut flags = FmtFlags::default();
    let mut seen = Vec::new();
    for name in names {
        let name = text(name)?;
        if seen.contains(&name) || !FLAG_NAMES.contains(&name) {
            return invalid("state");
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
            _ => &mut flags.uppercase,
        };
        *flag = true;
    }
    let bounded = |name: &str| -> R<i64> {
        let value = i64_of(&v[name])?;
        if (STREAM_BOUND.0..=STREAM_BOUND.1).contains(&value) {
            Ok(value)
        } else {
            invalid("state")
        }
    };
    let precision = bounded("precision")?;
    let width = bounded("width")?;
    let fill = text(&v["fill"])?;
    let Some(fill) = Some(fill)
        .filter(|t| t.len() == 1 && t.bytes().all(|c| (0x20..0x7f).contains(&c)))
        .and_then(|t| t.chars().next())
    else {
        return invalid("state");
    };
    Ok(StreamState {
        flags,
        precision,
        width,
        fill,
    })
}

fn optional_state(v: &Value) -> R<StreamState> {
    if v.is_null() {
        Ok(StreamState::default())
    } else {
        stream_state(v)
    }
}

/// Typed tree bodies.
trait TreeOps: Boolean {
    fn add_node(tree: &mut PolyTree<Self>, parent: PolyPathId, path: &Path<Self>) -> R<PolyPathId>;
    fn stream(
        tree: &PolyTree<Self>,
        id: PolyPathId,
        state: &mut StreamState,
    ) -> Result<String, ClipperError>;
}

impl TreeOps for i64 {
    fn add_node(tree: &mut PolyTree64, parent: PolyPathId, path: &Path64) -> R<PolyPathId> {
        Ok(tree.add_child(parent, path)?)
    }
    fn stream(
        tree: &PolyTree64,
        id: PolyPathId,
        state: &mut StreamState,
    ) -> Result<String, ClipperError> {
        c2::insert_polytree64(tree, id, state)
    }
}

impl TreeOps for f64 {
    fn add_node(tree: &mut PolyTreeD, parent: PolyPathId, path: &PathD) -> R<PolyPathId> {
        Ok(tree.add_child_d(parent, path)?)
    }
    fn stream(
        tree: &PolyTreeD,
        id: PolyPathId,
        state: &mut StreamState,
    ) -> Result<String, ClipperError> {
        c2::insert_polytree_d(tree, id, state)
    }
}

fn run_poly_tree<T: TreeOps>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "nodes", "fn", "node", "state"])?;
    let mut count = Counter(0);
    let nodes_in = array(at(v, "nodes")?, "nodes")?;
    let f = text(at(v, "fn")?)?;
    if nodes_in.len() > MAX_NODES {
        return invalid("size");
    }
    let mut tree = PolyTree::<T>::new(TREE_BUDGET)?;
    let mut nodes: Vec<PolyPathId> = Vec::new();
    for n in nodes_in {
        keys(n, &["parent", "polygon"])?;
        let parent = i64_of(at(n, "parent")?)?;
        let polygon = read_path::<T>(at(n, "polygon")?, &mut count)?;
        if parent < -1 || parent >= nodes.len() as i64 {
            return invalid("parent");
        }
        let owner = if parent < 0 {
            PolyTree::<T>::ROOT
        } else {
            nodes[parent as usize]
        };
        let id = T::add_node(&mut tree, owner, &polygon)?;
        nodes.push(id);
    }
    let node = i64_of(at(v, "node")?)?;
    if node < -1 || node >= nodes.len() as i64 {
        return invalid("node");
    }
    let pp = if node < 0 {
        PolyTree::<T>::ROOT
    } else {
        nodes[node as usize]
    };
    let state = at(v, "state")?;
    if f != "stream" && !state.is_null() {
        return invalid("state");
    }
    match f {
        "to_paths" => Ok(write_paths(&c2::poly_tree_to_paths(&tree, pp)?)),
        "path_to_paths" => {
            let mut out = Paths::new();
            c2::poly_path_to_paths(&tree, pp, &mut out)?;
            Ok(write_paths(&out))
        }
        "contains_children" | "node_contains" => {
            if T::DOUBLE {
                return invalid("fn");
            }
            let tree64: PolyTree64 = rebuild64(nodes_in, &mut Counter(0))?;
            let pp64 = if node < 0 {
                PolyTree64::ROOT
            } else {
                tree64_ids(&tree64)?[node as usize]
            };
            Ok(json!(if f == "contains_children" {
                c2::check_polytree_fully_contains_children(&tree64, pp64)?
            } else {
                c2::poly_path64_contains_children(&tree64, pp64)?
            }))
        }
        "stream" => {
            let mut state = optional_state(state)?;
            let text = T::stream(&tree, pp, &mut state)?;
            Ok(json!({"text": text, "width_after": state.width}))
        }
        _ => invalid("fn"),
    }
}

/// The same tree as `int64_t` nodes (for the `PolyPath64`-only functions).
fn rebuild64(nodes_in: &[Value], count: &mut Counter) -> R<PolyTree64> {
    let mut tree = PolyTree64::new(TREE_BUDGET)?;
    let mut ids: Vec<PolyPathId> = Vec::new();
    for n in nodes_in {
        let parent = i64_of(at(n, "parent")?)?;
        let polygon = read_path::<i64>(at(n, "polygon")?, count)?;
        let owner = if parent < 0 {
            PolyTree64::ROOT
        } else {
            ids[parent as usize]
        };
        ids.push(tree.add_child(owner, &polygon)?);
    }
    Ok(tree)
}

/// Every node of `tree` in preorder, the order `rebuild64` created them.
fn tree64_ids(tree: &PolyTree64) -> R<Vec<PolyPathId>> {
    let mut out = Vec::new();
    let mut stack = vec![PolyTree64::ROOT];
    while let Some(id) = stack.pop() {
        if id != PolyTree64::ROOT {
            out.push(id);
        }
        stack.extend(tree.children(id)?.iter().rev().copied());
    }
    Ok(out)
}

fn run_outline(v: &Value) -> R<Value> {
    keys(v, &["op", "idx", "is_hole", "count", "preamble", "state"])?;
    let idx = u64_of(at(v, "idx")?)?;
    let is_hole = boolean(at(v, "is_hole")?)?;
    let cnt = u64_of(at(v, "count")?)?;
    let preamble = text(at(v, "preamble")?)?;
    if preamble.len() > MAX_PREAMBLE {
        return invalid("preamble");
    }
    if !preamble.bytes().all(|c| (0x20..=0x7e).contains(&c)) {
        return invalid("preamble");
    }
    let mut state = optional_state(at(v, "state")?)?;
    let text = c2::outline_poly_path(idx, is_hole, cnt, preamble, &mut state)?;
    Ok(json!({"text": text, "width_after": state.width}))
}

// --- MakePath ------------------------------------------------------------------------------

/// The element types on the wire: integers as JSON numbers in the type's range, doubles as
/// bit patterns.
trait Element: PathNumber {
    fn read(v: &Value) -> R<Self>;
}

macro_rules! int_element {
    ($($t:ty),*) => {$(
        impl Element for $t {
            fn read(v: &Value) -> R<Self> {
                match <$t>::try_from(i64_of(v)?) {
                    Ok(x) => Ok(x),
                    Err(_) => invalid("element"),
                }
            }
        }
    )*};
}
int_element!(i64, i32, u8);

impl Element for u64 {
    fn read(v: &Value) -> R<Self> {
        u64_of(v)
    }
}

impl Element for f64 {
    fn read(v: &Value) -> R<Self> {
        f64_of(v)
    }
}

fn read_values<N: Element>(v: &Value) -> R<Vec<N>> {
    let values_in = array(at(v, "values")?, "values")?;
    if values_in.len() > MAX_VALUES {
        return invalid("size");
    }
    let mut values = Vec::new();
    for x in values_in {
        values.push(N::read(x)?);
    }
    Ok(values)
}

/// `T list[N]` from the values (the caller checked the count).
fn fixed<N: Copy, const LEN: usize>(values: &[N]) -> R<[N; LEN]> {
    match <[N; LEN]>::try_from(values) {
        Ok(a) => Ok(a),
        Err(_) => invalid("array size"),
    }
}

/// `MakePathD` and `MakePathZD`, for any element type.
fn make_any<N: Element>(f: &str, values: &[N], array: bool) -> R<Value> {
    let path = if !array {
        c2::make_path_d(values)
    } else if f == "make_path_d" {
        match values.len() {
            2 => c2::make_path_d_array(&fixed::<N, 2>(values)?),
            4 => c2::make_path_d_array(&fixed::<N, 4>(values)?),
            6 => c2::make_path_d_array(&fixed::<N, 6>(values)?),
            8 => c2::make_path_d_array(&fixed::<N, 8>(values)?),
            _ => return invalid("array size"),
        }
    } else {
        match values.len() {
            3 => c2::make_path_zd(&fixed::<N, 3>(values)?),
            6 => c2::make_path_zd(&fixed::<N, 6>(values)?),
            9 => c2::make_path_zd(&fixed::<N, 9>(values)?),
            _ => return invalid("array size"),
        }
    };
    guarded("path", path.map(|p| write_path(&p)))
}

/// `MakePath` and `MakePathZ` as well, for integral element types.
fn make_integer<N: Element + PathInteger>(f: &str, values: &[N], array: bool) -> R<Value> {
    if f == "make_path_d" || f == "make_path_zd" {
        return make_any(f, values, array);
    }
    let path = if !array {
        c2::make_path(values)
    } else if f == "make_path" {
        match values.len() {
            2 => c2::make_path_array(&fixed::<N, 2>(values)?),
            4 => c2::make_path_array(&fixed::<N, 4>(values)?),
            6 => c2::make_path_array(&fixed::<N, 6>(values)?),
            8 => c2::make_path_array(&fixed::<N, 8>(values)?),
            _ => return invalid("array size"),
        }
    } else {
        match values.len() {
            3 => c2::make_path_z(&fixed::<N, 3>(values)?),
            6 => c2::make_path_z(&fixed::<N, 6>(values)?),
            9 => c2::make_path_z(&fixed::<N, 9>(values)?),
            _ => return invalid("array size"),
        }
    };
    guarded("path", path.map(|p| write_path(&p)))
}

fn run_make_path(v: &Value) -> R<Value> {
    keys(v, &["op", "fn", "elem", "values", "array"])?;
    let f = text(at(v, "fn")?)?;
    if !matches!(
        f,
        "make_path" | "make_path_d" | "make_path_z" | "make_path_zd"
    ) {
        return invalid("fn");
    }
    let elem = text(at(v, "elem")?)?;
    let pair = f == "make_path" || f == "make_path_d";
    let integral_only = f == "make_path" || f == "make_path_z";
    // Values, then the array flag, then the type and form checks, as the C++ evaluates them.
    macro_rules! typed {
        ($t:ty, $integer:expr, $call:ident) => {{
            let values = read_values::<$t>(v)?;
            let array = boolean(at(v, "array")?)?;
            if !$integer && integral_only {
                return invalid("elem");
            }
            if !pair && !array {
                return invalid("array");
            }
            $call::<$t>(f, &values, array)
        }};
    }
    match elem {
        "i64" => typed!(i64, true, make_integer),
        "i32" => typed!(i32, true, make_integer),
        "u64" => typed!(u64, true, make_integer),
        "u8" => typed!(u8, true, make_integer),
        "f64" => typed!(f64, false, make_any),
        _ => invalid("elem"),
    }
}

// --- path utilities ------------------------------------------------------------------------

fn run_trim<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "path", "precision", "open"])?;
    let mut count = Counter(0);
    let path = read_path::<T>(at(v, "path")?, &mut count)?;
    let open_in = at(v, "open")?;
    let open = if open_in.is_null() {
        false
    } else {
        boolean(open_in)?
    };
    if T::DOUBLE {
        let precision = int_of(at(v, "precision")?)?;
        let path_d: PathD = as_paths_d(&vec![path]).into_iter().flatten().collect();
        return guarded(
            "path",
            c2::trim_collinear_d(&path_d, precision, open).map(|p| write_path(&p)),
        );
    }
    if !at(v, "precision")?.is_null() {
        return invalid("precision");
    }
    let path64: Path64 = as_paths64(&vec![path]).into_iter().flatten().collect();
    guarded(
        "path",
        c2::trim_collinear_64(&path64, open).map(|p| write_path(&p)),
    )
}

fn run_distance<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "a", "b"])?;
    let mut count = Counter(0);
    let a = read_point::<T>(at(v, "a")?, &mut count)?;
    let b = read_point::<T>(at(v, "b")?, &mut count)?;
    Ok(bits(c2::distance(&a, &b)?))
}

fn run_length<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "path", "closed"])?;
    let mut count = Counter(0);
    let path = read_path::<T>(at(v, "path")?, &mut count)?;
    let closed = at(v, "closed")?;
    let closed = if closed.is_null() {
        false
    } else {
        boolean(closed)?
    };
    Ok(bits(c2::length(&path, closed)?))
}

fn run_near_collinear<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "a", "b", "c", "value"])?;
    let mut count = Counter(0);
    let a = read_point::<T>(at(v, "a")?, &mut count)?;
    let b = read_point::<T>(at(v, "b")?, &mut count)?;
    let c = read_point::<T>(at(v, "c")?, &mut count)?;
    Ok(json!(c2::near_collinear(
        &a,
        &b,
        &c,
        f64_of(at(v, "value")?)?
    )?))
}

fn ellipse_guard(radius_x: f64, radius_y: f64, steps: u64) -> R<()> {
    if radius_x <= 0.0 {
        return Ok(());
    }
    if steps > 2 {
        if steps as f64 > MAX_ELLIPSE_STEPS {
            return invalid("steps");
        }
        return Ok(());
    }
    let radius_y = if radius_y <= 0.0 { radius_x } else { radius_y };
    let estimate = PI * ((radius_x + radius_y) / 2.0).sqrt();
    if estimate > MAX_ELLIPSE_STEPS {
        return invalid("steps");
    }
    Ok(())
}

fn run_ellipse<T: Wire>(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op", "type", "form", "center", "rect", "radius_x", "radius_y", "steps",
        ],
    )?;
    let mut count = Counter(0);
    let form = text(at(v, "form")?)?;
    let steps_in = at(v, "steps")?;
    let has_steps = !steps_in.is_null();
    let steps = if has_steps { u64_of(steps_in)? } else { 0 };
    if form == "rect" {
        if !at(v, "center")?.is_null()
            || !at(v, "radius_x")?.is_null()
            || !at(v, "radius_y")?.is_null()
        {
            return invalid("form");
        }
        let rect = read_rect::<T>(at(v, "rect")?)?;
        let rx = (rect.right.to_f64() - rect.left.to_f64()) * 0.5;
        let ry = (rect.bottom.to_f64() - rect.top.to_f64()) * 0.5;
        ellipse_guard(rx, ry, steps)?;
        return Ok(write_path(&c2::ellipse_rect(&rect, steps, ELLIPSE_POINTS)?));
    }
    if form != "center" || !at(v, "rect")?.is_null() {
        return invalid("form");
    }
    let center = read_point::<T>(at(v, "center")?, &mut count)?;
    let rx = f64_of(at(v, "radius_x")?)?;
    let ry_in = at(v, "radius_y")?;
    let has_ry = !ry_in.is_null();
    let ry = if has_ry { f64_of(ry_in)? } else { 0.0 };
    if has_steps && !has_ry {
        return invalid("defaults");
    }
    ellipse_guard(rx, ry, steps)?;
    Ok(write_path(&c2::ellipse(
        &center,
        rx,
        ry,
        steps,
        ELLIPSE_POINTS,
    )?))
}

fn max_path_length<T>(paths: &[Vec<T>], max: usize) -> R<()> {
    if paths.iter().any(|p| p.len() > max) {
        return invalid("size");
    }
    Ok(())
}

fn read_form<T: Wire>(v: &Value, count: &mut Counter) -> R<(bool, Paths<T>)> {
    let single = single_path(v)?;
    let paths = if single {
        vec![read_path::<T>(at(v, "value")?, count)?]
    } else {
        read_paths::<T>(at(v, "value")?, count)?
    };
    Ok((single, paths))
}

fn run_simplify<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "form", "value", "epsilon", "closed"])?;
    let mut count = Counter(0);
    let (single, paths) = read_form::<T>(v, &mut count)?;
    max_path_length(&paths, MAX_SIMPLIFY)?;
    let epsilon = f64_of(at(v, "epsilon")?)?;
    let closed_in = at(v, "closed")?;
    // `isClosedPath = true` by default.
    let closed = if closed_in.is_null() {
        true
    } else {
        boolean(closed_in)?
    };
    if single {
        let Some(path) = paths.first() else {
            return invalid("path");
        };
        return Ok(write_path(&c2::simplify_path(path, epsilon, closed)?));
    }
    Ok(write_paths(&c2::simplify_paths(&paths, epsilon, closed)?))
}

fn run_contains<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "path1", "path2"])?;
    let mut count = Counter(0);
    let path1 = read_path::<T>(at(v, "path1")?, &mut count)?;
    let path2 = read_path::<T>(at(v, "path2")?, &mut count)?;
    Ok(json!(c2::path2_contains_path1(&path1, &path2)?))
}

/// RDP's call bound: a non-NaN epsilon needs at most `2 * len + 1` calls.
fn rdp_calls(len: usize) -> usize {
    len.saturating_mul(4).saturating_add(16)
}

/// The C++ adapter's `RdpNanGuard`: follow `RDP(path, 0, len - 1, ...)`'s calls in the
/// reference's order and refuse the request if one repeats an active ancestor's arguments
/// (a recursion that never ends) or the calls exceed `max_calls`. Each step uses the port's
/// comparisons and `PerpendicDistFromLineSqrd` in the reference's order, so an overflow the
/// reference reaches first is reported first here too.
fn rdp_nan_guard<T: Coord>(path: &[Point<T>], eps_sqrd: f64, max_calls: usize) -> R<()> {
    let point = |i: usize| {
        path.get(i)
            .ok_or(E::Clip(ClipperError::Undefined("RDP index")))
    };
    let mut pending: Vec<(usize, usize, usize)> = vec![(0, path.len().saturating_sub(1), 0)];
    let mut chain: Vec<(usize, usize)> = Vec::new();
    let mut calls = 0usize;
    while let Some((begin, end_in, depth)) = pending.pop() {
        calls += 1;
        if calls > max_calls {
            return invalid("epsilon");
        }
        chain.truncate(depth);
        if chain.contains(&(begin, end_in)) {
            return invalid("epsilon");
        }
        chain.push((begin, end_in));
        let (mut end, mut idx, mut max_d) = (end_in, 0usize, 0.0);
        while end > begin && point(begin)? == point(end)? {
            end -= 1;
        }
        for i in begin.wrapping_add(1)..end {
            let d = kicad_clipper2::clipper_core::perpendic_dist_from_line_sqrd(
                point(i)?,
                point(begin)?,
                point(end)?,
            )?;
            if d <= max_d {
                continue;
            }
            max_d = d;
            idx = i;
        }
        if max_d <= eps_sqrd {
            continue;
        }
        if idx < end.wrapping_sub(1) {
            pending.push((idx, end, depth + 1));
        }
        if idx > begin.wrapping_add(1) {
            pending.push((begin, idx, depth + 1));
        }
    }
    Ok(())
}

fn run_rdp<T: Wire>(v: &Value) -> R<Value> {
    keys(v, &["op", "type", "form", "value", "epsilon"])?;
    let mut count = Counter(0);
    let (single, paths) = read_form::<T>(v, &mut count)?;
    max_path_length(&paths, MAX_RDP)?;
    let epsilon = f64_of(at(v, "epsilon")?)?;
    let calls = rdp_calls(MAX_RDP);
    // A NaN epsilon can make RDP recurse forever; only those paths (and ones beyond the call
    // bound) are refused. Shorter paths are copied without an RDP call.
    if epsilon.is_nan() {
        for path in paths.iter().filter(|p| p.len() >= 5) {
            rdp_nan_guard(path, kicad_clipper2::clipper_core::sqr(epsilon), calls)?;
        }
    }
    if single {
        let Some(path) = paths.first() else {
            return invalid("path");
        };
        return Ok(write_path(&c2::ramer_douglas_peucker(
            path, epsilon, calls,
        )?));
    }
    Ok(write_paths(&c2::ramer_douglas_peucker_paths(
        &paths, epsilon, calls,
    )?))
}

fn run_minkowski<T: Wire>(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op",
            "type",
            "sum",
            "pattern",
            "path",
            "closed",
            "decimal_places",
        ],
    )?;
    let mut count = Counter(0);
    let sum = boolean(at(v, "sum")?)?;
    let pattern = read_path::<T>(at(v, "pattern")?, &mut count)?;
    let path = read_path::<T>(at(v, "path")?, &mut count)?;
    let closed = boolean(at(v, "closed")?)?;
    let dp = optional_int(at(v, "decimal_places")?)?;
    if !T::DOUBLE && dp.is_some() {
        return invalid("decimal_places");
    }
    if pattern.len().saturating_mul(path.len()) > MAX_QUADS {
        return invalid("size");
    }
    if T::DOUBLE {
        let scale = pow10(dp.unwrap_or(2));
        for p in pattern.iter().chain(path.iter()) {
            for value in [p.x.to_f64() * scale, p.y.to_f64() * scale] {
                if value.abs() > MINKOWSKI_COORD_D && value.abs() < TWO_POW_63 {
                    return invalid("domain");
                }
            }
        }
        let pat: PathD = as_paths_d(&vec![pattern]).into_iter().flatten().collect();
        let pth: PathD = as_paths_d(&vec![path]).into_iter().flatten().collect();
        let d = dp.unwrap_or(2);
        let result = if sum {
            mk::minkowski_sum_d(&pat, &pth, closed, d, MAX_QUADS)
        } else {
            mk::minkowski_diff_d(&pat, &pth, closed, d, MAX_QUADS)
        };
        return guarded("paths", result.map(|r| write_paths(&r)));
    }
    let pat: Path64 = as_paths64(&vec![pattern]).into_iter().flatten().collect();
    let pth: Path64 = as_paths64(&vec![path]).into_iter().flatten().collect();
    for p in pat.iter().chain(pth.iter()) {
        let out = |c: i64| !(-MINKOWSKI_COORD..=MINKOWSKI_COORD).contains(&c);
        if out(p.x) || out(p.y) {
            return invalid("domain");
        }
    }
    let result = if sum {
        mk::minkowski_sum_64(&pat, &pth, closed, MAX_QUADS)
    } else {
        mk::minkowski_diff_64(&pat, &pth, closed, MAX_QUADS)
    };
    guarded("paths", result.map(|r| write_paths(&r)))
}

fn run_detail(v: &Value) -> R<Value> {
    keys(v, &["op", "fn", "current", "high", "flags"])?;
    let f = text(at(v, "fn")?)?;
    let flags_in = array(at(v, "flags")?, "flags")?;
    if flags_in.is_empty() || flags_in.len() > MAX_FLAGS {
        return invalid("size");
    }
    let mut flags = Vec::new();
    for x in flags_in {
        flags.push(boolean(x)?);
    }
    let high = u64_of(at(v, "high")?)?;
    let current = u64_of(at(v, "current")?)?;
    if high.wrapping_add(1) != flags.len() as u64 || current > high {
        return invalid("detail");
    }
    // Both fit: high < flags.len() <= MAX_FLAGS.
    let (high, current) = (high as usize, current as usize);
    match f {
        "get_next" => Ok(json!(c2::get_next(current, high, &flags)?)),
        "get_prior" => Ok(json!(c2::get_prior(current, high, &flags)?)),
        _ => invalid("fn"),
    }
}

fn run_typed<T: TreeOps>(op: &str, v: &Value) -> R<Value> {
    match op {
        "boolean" => run_boolean::<T>(v),
        "inflate" => run_inflate::<T>(v),
        "translate" => run_translate::<T>(v),
        "rectclip" => run_rect_clip::<T>(v),
        "polytree" => run_poly_tree::<T>(v),
        "trim" => run_trim::<T>(v),
        "distance" => run_distance::<T>(v),
        "length" => run_length::<T>(v),
        "near_collinear" => run_near_collinear::<T>(v),
        "ellipse" => run_ellipse::<T>(v),
        "simplify" => run_simplify::<T>(v),
        "contains" => run_contains::<T>(v),
        "rdp" => run_rdp::<T>(v),
        "minkowski" => run_minkowski::<T>(v),
        _ => invalid("op"),
    }
}

fn run(v: &Value) -> R<Value> {
    // Run: a missing or non-string op is refused before evaluation.
    let Some(op) = v.get("op").and_then(Value::as_str) else {
        return invalid("op");
    };
    match op {
        "make_path" => run_make_path(v),
        "outline" => run_outline(v),
        "detail" => run_detail(v),
        "offset_members" => run_offset_members(v),
        "version" => {
            keys(v, &["op"])?;
            Ok(json!({"version": CLIPPER2_VERSION}))
        }
        _ => match text(at(v, "type")?)? {
            "i64" => run_typed::<i64>(op, v),
            "f64" => run_typed::<f64>(op, v),
            _ => invalid("type"),
        },
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
    let op = input.get("op").and_then(Value::as_str).unwrap_or("");
    match run(input) {
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
