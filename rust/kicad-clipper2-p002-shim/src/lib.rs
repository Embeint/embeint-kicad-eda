// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for unit `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002`
//! (`kicad_clipper2::engine_p002` and `kicad_clipper2::offset`), mirroring
//! `port/oracle/shims/clipper2_engine_p002.cpp` request for request: the same operations,
//! field order, input domain and resource guards, and replies. Points are `[x, y, z]`; doubles
//! travel as their 16 lowercase hex digit bit patterns.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_clipper2::clipper_engine::{
    ClipType, Clipper64, ClipperD, PolyTree, PolyTree64, PolyTreeD, ZCallback64, ZCallbackD,
};
use kicad_clipper2::offset::{
    self, ClipperOffset, DeltaCallback64, EndType, JoinType, ARC_CONST, FLOATING_POINT_TOLERANCE,
    PI,
};
use kicad_clipper2::{
    Budget, ClipperError, Coord, FillRule, Path, Path64, PathD, Paths64, PathsD, Point64, PointD,
};
use kicad_math::reference_libm::acos;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;

/// This port unit.
pub const UNIT: &str = kicad_clipper2::ENGINE_SRC_P002;

/// Request limits, identical in the C++ adapter.
const MAX_POINTS: usize = 1 << 16;
const MAX_GROUPS: usize = 64;
const MAX_EXECUTES: usize = 4;
const MAX_RUNS: i64 = 2;
const MAX_TABLE: usize = 64;
/// Clipper2's declared coordinate range (`INT64_MAX >> 2`), the engine domain.
const ENGINE_COORD: i64 = i64::MAX >> 2;
/// The offset domain: coordinates and deltas whose offsets stay far inside the engine domain.
const OFFSET_LIMIT: f64 = 1_099_511_627_776.0; // 2^40
const OFFSET_COORD: i64 = 1 << 40;
const MAX_ARC_STEPS: f64 = 65536.0;
const MAX_OFFSET_POINTS: f64 = 4_194_304.0; // 2^22
const Z_BASE: i64 = 1_000_000;

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

fn read_point(v: &Value, count: &mut Counter) -> R<Point64> {
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

fn read_paths(v: &Value, count: &mut Counter) -> R<Paths64> {
    let mut paths = Paths64::new();
    for path in array(v, "paths")? {
        let mut out = Path64::new();
        for p in array(path, "path")? {
            grow(&mut out, read_point(p, count)?)?;
        }
        grow(&mut paths, out)?;
    }
    Ok(paths)
}

fn read_point_d(v: &Value, count: &mut Counter) -> R<PointD> {
    match v.as_array() {
        Some(a) if a.len() == 3 => {
            count.add(1)?;
            Ok(PointD {
                x: f64_of(&a[0])?,
                y: f64_of(&a[1])?,
                z: i64_of(&a[2])?,
            })
        }
        _ => invalid("point"),
    }
}

fn read_path_d(v: &Value, count: &mut Counter) -> R<PathD> {
    let mut out = PathD::new();
    for p in array(v, "path")? {
        grow(&mut out, read_point_d(p, count)?)?;
    }
    Ok(out)
}

fn read_paths_d(v: &Value, count: &mut Counter) -> R<PathsD> {
    let mut paths = PathsD::new();
    for path in array(v, "paths")? {
        grow(&mut paths, read_path_d(path, count)?)?;
    }
    Ok(paths)
}

fn write_point(p: &Point64) -> Value {
    json!([p.x, p.y, p.z])
}

fn write_point_d(p: &PointD) -> Value {
    json!([bits(p.x), bits(p.y), p.z])
}

fn write_path(path: &Path64) -> Value {
    Value::Array(path.iter().map(write_point).collect())
}

fn write_path_d(path: &PathD) -> Value {
    Value::Array(path.iter().map(write_point_d).collect())
}

fn write_paths(paths: &Paths64) -> Value {
    Value::Array(paths.iter().map(write_path).collect())
}

fn write_paths_d(paths: &PathsD) -> Value {
    Value::Array(paths.iter().map(write_path_d).collect())
}

/// A tree in preorder: each node's parent index (-1 for the root's children), hole flag and
/// polygon, as the C++ adapter writes it.
fn write_tree<T: Coord>(
    tree: &PolyTree<T>,
    write: fn(&Path<T>) -> Value,
) -> Result<Value, ClipperError> {
    let mut out: Vec<Value> = Vec::new();
    // (node, parent index, next child)
    let mut stack = vec![(PolyTree::<T>::ROOT, -1i64, 0usize)];
    while let Some(&(node, parent_index, next)) = stack.last() {
        let children = tree.children(node)?;
        let Some(&child) = children.get(next) else {
            stack.pop();
            continue;
        };
        if let Some(top) = stack.last_mut() {
            top.2 = next + 1;
        }
        let index = out.len() as i64;
        out.push(json!({
            "parent": parent_index,
            "hole": tree.is_hole(child)?,
            "polygon": write(tree.polygon(child)?),
        }));
        stack.push((child, index, 0));
    }
    Ok(Value::Array(out))
}

fn engine_domain(paths: &Paths64) -> R<()> {
    for p in paths.iter().flatten() {
        if p.x < -ENGINE_COORD || p.x > ENGINE_COORD || p.y < -ENGINE_COORD || p.y > ENGINE_COORD {
            return invalid("domain");
        }
    }
    Ok(())
}

fn not_nan(paths: &PathsD) -> R<()> {
    for p in paths.iter().flatten() {
        if p.x.is_nan() || p.y.is_nan() {
            return invalid("domain");
        }
    }
    Ok(())
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

fn runs(v: &Value) -> R<usize> {
    let runs = i64_of(v)?;
    if !(1..=MAX_RUNS).contains(&runs) {
        return invalid("runs");
    }
    Ok(runs as usize)
}

fn recorder(v: &Value) -> R<bool> {
    match text(v)? {
        "none" => Ok(false),
        "recorder" => Ok(true),
        _ => invalid("z"),
    }
}

fn output(v: &Value) -> R<u8> {
    match text(v)? {
        "paths" => Ok(0),
        "paths_open" => Ok(1),
        "tree" => Ok(2),
        "tree_open" => Ok(3),
        _ => invalid("output"),
    }
}

/// A recorder of Z callback calls: it logs the edges and the point as received and sets
/// `pt.z = Z_BASE + calls`.
type Calls = Rc<RefCell<Vec<Value>>>;

fn record(calls: &Calls, entry: Value) -> i64 {
    let mut log = calls.borrow_mut();
    log.push(entry);
    Z_BASE + log.len() as i64
}

fn z_recorder_64(calls: &Calls) -> ZCallback64 {
    let calls = Rc::clone(calls);
    Box::new(
        move |e1b: &Point64, e1t: &Point64, e2b: &Point64, e2t: &Point64, pt: &mut Point64| {
            let entry = json!([
                write_point(e1b),
                write_point(e1t),
                write_point(e2b),
                write_point(e2t),
                write_point(pt)
            ]);
            pt.z = record(&calls, entry);
        },
    )
}

fn z_recorder_d(calls: &Calls) -> ZCallbackD {
    let calls = Rc::clone(calls);
    Box::new(
        move |e1b: &PointD, e1t: &PointD, e2b: &PointD, e2t: &PointD, pt: &mut PointD| {
            let entry = json!([
                write_point_d(e1b),
                write_point_d(e1t),
                write_point_d(e2b),
                write_point_d(e2t),
                write_point_d(pt)
            ]);
            pt.z = record(&calls, entry);
        },
    )
}

fn calls_value(calls: &Calls) -> Value {
    Value::Array(calls.borrow().clone())
}

/// `Clipper2Exception` ends the operation with a compared result; anything else propagates.
fn exception(error: ClipperError) -> R<Value> {
    match error {
        ClipperError::Exception(e) => Ok(json!({"exception": e.what()})),
        other => Err(E::Clip(other)),
    }
}

// --- operations ----------------------------------------------------------------------

fn run_clip64(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op",
            "subjects",
            "open",
            "clips",
            "clip_type",
            "fill_rule",
            "preserve_collinear",
            "reverse_solution",
            "pip_threshold",
            "z",
            "output",
            "runs",
        ],
    )?;
    let mut count = Counter(0);
    let subjects = read_paths(at(v, "subjects")?, &mut count)?;
    let open = read_paths(at(v, "open")?, &mut count)?;
    let clips = read_paths(at(v, "clips")?, &mut count)?;
    let ct = clip_type(at(v, "clip_type")?)?;
    let fr = fill_rule(at(v, "fill_rule")?)?;
    let preserve = boolean(at(v, "preserve_collinear")?)?;
    let reverse = boolean(at(v, "reverse_solution")?)?;
    let pip = i64_of(at(v, "pip_threshold")?)?;
    let use_recorder = recorder(at(v, "z")?)?;
    let output = output(at(v, "output")?)?;
    let runs = runs(at(v, "runs")?)?;
    if pip < 0 {
        return invalid("int64");
    }
    engine_domain(&subjects)?;
    engine_domain(&open)?;
    engine_domain(&clips)?;

    let calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let mut results = Vec::new();
    let body = |results: &mut Vec<Value>| -> Result<(), ClipperError> {
        let mut c = Clipper64::default();
        c.base.set_preserve_collinear(preserve);
        c.base.set_reverse_solution(reverse);
        c.base.set_pip_index_threshold(pip as u64);
        if use_recorder {
            c.set_z_callback(Some(z_recorder_64(&calls)));
        }
        c.add_subject(&subjects)?;
        c.add_open_subject(&open)?;
        c.add_clip(&clips)?;
        for _ in 0..runs {
            let (mut closed, mut open_out) = (Paths64::new(), Paths64::new());
            let mut tree = Value::Null;
            let ok = match output {
                0 => c.execute(ct, fr, &mut closed)?,
                1 => c.execute_with_open(ct, fr, &mut closed, &mut open_out)?,
                _ => {
                    let mut polytree = PolyTree64::new(TREE_BUDGET)?;
                    let ok = if output == 2 {
                        c.execute_tree(ct, fr, &mut polytree)?
                    } else {
                        c.execute_tree_with_open(ct, fr, &mut polytree, &mut open_out)?
                    };
                    tree = write_tree(&polytree, write_path)?;
                    ok
                }
            };
            results.push(json!({
                "succeeded": ok,
                "closed": write_paths(&closed),
                "open": write_paths(&open_out),
                "tree": tree,
                "error_code": c.base.error_code(),
            }));
        }
        Ok(())
    };
    if let Err(error) = body(&mut results) {
        results.push(exception(error)?);
    }
    Ok(json!({"runs": results, "z_calls": calls_value(&calls)}))
}

fn run_clip_d(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op",
            "precision",
            "subjects",
            "open",
            "clips",
            "clip_type",
            "fill_rule",
            "preserve_collinear",
            "reverse_solution",
            "z",
            "output",
            "runs",
        ],
    )?;
    let precision = i64_of(at(v, "precision")?)?;
    let mut count = Counter(0);
    let subjects = read_paths_d(at(v, "subjects")?, &mut count)?;
    let open = read_paths_d(at(v, "open")?, &mut count)?;
    let clips = read_paths_d(at(v, "clips")?, &mut count)?;
    let ct = clip_type(at(v, "clip_type")?)?;
    let fr = fill_rule(at(v, "fill_rule")?)?;
    let preserve = boolean(at(v, "preserve_collinear")?)?;
    let reverse = boolean(at(v, "reverse_solution")?)?;
    let use_recorder = recorder(at(v, "z")?)?;
    let output = output(at(v, "output")?)?;
    let runs = runs(at(v, "runs")?)?;
    if !(-64..=64).contains(&precision) {
        return invalid("int");
    }
    not_nan(&subjects)?;
    not_nan(&open)?;
    not_nan(&clips)?;

    let calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let mut results = Vec::new();
    let body = |results: &mut Vec<Value>| -> Result<(), ClipperError> {
        let mut c = ClipperD::new(precision as i32)?;
        c.base.set_preserve_collinear(preserve);
        c.base.set_reverse_solution(reverse);
        if use_recorder {
            c.set_z_callback(Some(z_recorder_d(&calls)))?;
        }
        c.add_subject(&subjects)?;
        c.add_open_subject(&open)?;
        c.add_clip(&clips)?;
        for _ in 0..runs {
            let (mut closed, mut open_out) = (PathsD::new(), PathsD::new());
            let mut tree = Value::Null;
            let ok = match output {
                0 => c.execute(ct, fr, &mut closed)?,
                1 => c.execute_with_open(ct, fr, &mut closed, &mut open_out)?,
                _ => {
                    let mut polytree = PolyTreeD::new(TREE_BUDGET)?;
                    let ok = if output == 2 {
                        c.execute_tree(ct, fr, &mut polytree)?
                    } else {
                        c.execute_tree_with_open(ct, fr, &mut polytree, &mut open_out)?
                    };
                    tree = write_tree(&polytree, write_path_d)?;
                    ok
                }
            };
            results.push(json!({
                "succeeded": ok,
                "closed": write_paths_d(&closed),
                "open": write_paths_d(&open_out),
                "tree": tree,
                "error_code": c.base.error_code(),
            }));
        }
        Ok(())
    };
    if let Err(error) = body(&mut results) {
        results.push(exception(error)?);
    }
    Ok(json!({"runs": results, "z_calls": calls_value(&calls)}))
}

/// `std::min` as `ClipperOffset` uses it.
fn std_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// The arc steps per full circle `DoGroupOffset`/`DoRound` compute for `abs_delta`.
fn steps_per_360(abs_delta: f64, arc_tolerance: f64) -> f64 {
    let arc_tol = if arc_tolerance > FLOATING_POINT_TOLERANCE {
        std_min(abs_delta, arc_tolerance)
    } else {
        abs_delta * ARC_CONST
    };
    std_min(PI / acos(1.0 - arc_tol / abs_delta), abs_delta * PI)
}

fn offset_value(v: &Value) -> R<f64> {
    let value = f64_of(v)?;
    if !value.is_finite() || value.abs() > OFFSET_LIMIT {
        return invalid("delta");
    }
    Ok(value)
}

struct GroupIn {
    paths: Paths64,
    join: JoinType,
    end: EndType,
    single: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Paths,
    Tree,
    Callback,
}

/// The recording delta callback: returns `table[calls % len]` and logs
/// `[curr, prev, path.size(), normals.size()]`.
fn delta_recorder(table: &Rc<Vec<f64>>, calls: &Calls) -> DeltaCallback64 {
    let (table, calls) = (Rc::clone(table), Rc::clone(calls));
    Box::new(
        move |path: &Path64, normals: &PathD, curr: usize, prev: usize| {
            let mut log = calls.borrow_mut();
            let value = table
                .get(log.len() % table.len().max(1))
                .copied()
                .unwrap_or(0.0);
            log.push(json!([curr, prev, path.len(), normals.len()]));
            value
        },
    )
}

fn run_offset(v: &Value) -> R<Value> {
    keys(
        v,
        &[
            "op",
            "groups",
            "miter_limit",
            "arc_tolerance",
            "preserve_collinear",
            "reverse_solution",
            "z",
            "delta_table",
            "set_callback",
            "executes",
        ],
    )?;
    let mut count = Counter(0);
    let groups_in = array(at(v, "groups")?, "groups")?;
    if groups_in.len() > MAX_GROUPS {
        return invalid("size");
    }
    let mut groups: Vec<GroupIn> = Vec::new();
    for g in groups_in {
        keys(g, &["paths", "join", "end", "single"])?;
        let paths = read_paths(at(g, "paths")?, &mut count)?;
        let join = join_type(at(g, "join")?)?;
        let end = end_type(at(g, "end")?)?;
        let single = boolean(at(g, "single")?)?;
        if single && paths.len() != 1 {
            return invalid("single");
        }
        for p in paths.iter().flatten() {
            if p.x < -OFFSET_COORD
                || p.x > OFFSET_COORD
                || p.y < -OFFSET_COORD
                || p.y > OFFSET_COORD
            {
                return invalid("domain");
            }
        }
        grow(
            &mut groups,
            GroupIn {
                paths,
                join,
                end,
                single,
            },
        )?;
    }

    let miter_limit = offset_value(at(v, "miter_limit")?)?;
    let arc_tolerance = offset_value(at(v, "arc_tolerance")?)?;
    let preserve = boolean(at(v, "preserve_collinear")?)?;
    let reverse = boolean(at(v, "reverse_solution")?)?;
    let use_recorder = recorder(at(v, "z")?)?;

    let mut table: Vec<f64> = Vec::new();
    let table_in = at(v, "delta_table")?;
    if !table_in.is_null() {
        let values = array(table_in, "delta_table")?;
        if values.is_empty() || values.len() > MAX_TABLE {
            return invalid("delta_table");
        }
        for value in values {
            grow(&mut table, offset_value(value)?)?;
        }
    }

    let set_callback = boolean(at(v, "set_callback")?)?;
    if set_callback && table.is_empty() {
        return invalid("callback");
    }

    let executes_in = array(at(v, "executes")?, "executes")?;
    if executes_in.is_empty() || executes_in.len() > MAX_EXECUTES {
        return invalid("executes");
    }
    let mut executes: Vec<(Kind, f64)> = Vec::new();
    for e in executes_in {
        keys(e, &["kind", "delta"])?;
        let kind = match text(at(e, "kind")?)? {
            "paths" => Kind::Paths,
            "tree" => Kind::Tree,
            "callback" => Kind::Callback,
            _ => return invalid("kind"),
        };
        let delta = offset_value(at(e, "delta")?)?;
        if matches!(kind, Kind::Callback) && table.is_empty() {
            return invalid("callback");
        }
        grow(&mut executes, (kind, delta))?;
    }

    // Resource guard: round joins and ends emit at most (steps per circle + 16) points per
    // input vertex at any delta the request can produce.
    let round = groups
        .iter()
        .any(|g| g.join == JoinType::Round || g.end == EndType::Round);
    if round {
        let mut deltas = table.clone();
        for (kind, delta) in &executes {
            deltas.push(if matches!(kind, Kind::Callback) {
                1.0
            } else {
                *delta
            });
        }
        for d in deltas {
            let abs_delta = d.abs();
            if abs_delta <= FLOATING_POINT_TOLERANCE {
                continue;
            }
            let steps = steps_per_360(abs_delta, arc_tolerance);
            if steps.is_nan()
                || steps > MAX_ARC_STEPS
                || count.0 as f64 * (steps + 16.0) > MAX_OFFSET_POINTS
            {
                return invalid("arc steps");
            }
        }
    }

    let z_calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let delta_calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let table = Rc::new(table);
    let mut results = Vec::new();
    let mut groups_out = Vec::new();
    let mut norms = Value::Array(Vec::new());

    let mut co = ClipperOffset::new(miter_limit, arc_tolerance, preserve, reverse, OFFSET_BUDGET);
    let mut body = |results: &mut Vec<Value>| -> Result<(), ClipperError> {
        if use_recorder {
            co.set_z_callback(Some(z_recorder_64(&z_calls)));
        }
        if set_callback {
            co.set_delta_callback(Some(delta_recorder(&table, &delta_calls)));
        }
        for g in &groups {
            if g.single {
                let first = g.paths.first().ok_or(ClipperError::Undefined("single"))?;
                co.add_path(first, g.join, g.end)?;
            } else {
                co.add_paths(&g.paths, g.join, g.end)?;
            }
        }
        for group in co.groups() {
            groups_out.push(json!({
                "paths_in": write_paths(&group.paths_in),
                "lowest_path_idx": group.lowest_path_idx,
                "is_reversed": group.is_reversed,
            }));
        }
        for &(kind, delta) in &executes {
            let mut paths = Paths64::new();
            let mut tree = Value::Null;
            match kind {
                Kind::Paths => co.execute(delta, &mut paths)?,
                Kind::Tree => {
                    let mut polytree = PolyTree64::new(TREE_BUDGET)?;
                    co.execute_tree(delta, &mut polytree)?;
                    tree = write_tree(&polytree, write_path)?;
                }
                Kind::Callback => co.execute_with_delta_callback(
                    Some(delta_recorder(&table, &delta_calls)),
                    &mut paths,
                )?,
            }
            results.push(json!({
                "paths": write_paths(&paths),
                "tree": tree,
                "error_code": co.error_code(),
            }));
        }
        norms = write_path_d(co.norms());
        Ok(())
    };
    if let Err(error) = body(&mut results) {
        results.push(exception(error)?);
    }
    Ok(json!({
        "groups": groups_out,
        "executes": results,
        "norms": norms,
        "z_calls": calls_value(&z_calls),
        "delta_calls": calls_value(&delta_calls),
    }))
}

fn run_helper(v: &Value) -> R<Value> {
    let name = text(at(v, "fn")?)?;
    let mut count = Counter(0);
    match name {
        "hypot" => {
            keys(v, &["op", "fn", "x", "y"])?;
            let (x, y) = (f64_of(at(v, "x")?)?, f64_of(at(v, "y")?)?);
            Ok(bits(offset::hypot(x, y)))
        }
        "unit_normal" => {
            keys(v, &["op", "fn", "a", "b"])?;
            let a = read_point(at(v, "a")?, &mut count)?;
            let b = read_point(at(v, "b")?, &mut count)?;
            let out = |c: i64| !(-ENGINE_COORD..=ENGINE_COORD).contains(&c);
            if out(a.x) || out(a.y) || out(b.x) || out(b.y) {
                return invalid("domain");
            }
            Ok(write_point_d(&offset::get_unit_normal(&a, &b)?))
        }
        "normalize" => {
            keys(v, &["op", "fn", "v"])?;
            let p = read_point_d(at(v, "v")?, &mut count)?;
            Ok(write_point_d(&offset::normalize_vector(&p)))
        }
        "avg_unit" => {
            keys(v, &["op", "fn", "a", "b"])?;
            let a = read_point_d(at(v, "a")?, &mut count)?;
            let b = read_point_d(at(v, "b")?, &mut count)?;
            Ok(write_point_d(&offset::get_avg_unit_vector(&a, &b)))
        }
        "almost_zero" => {
            keys(v, &["op", "fn", "v", "epsilon"])?;
            let value = f64_of(at(v, "v")?)?;
            let eps = at(v, "epsilon")?;
            let epsilon = if eps.is_null() { 0.001 } else { f64_of(eps)? };
            Ok(json!(offset::almost_zero(value, epsilon)))
        }
        "perpendic" | "perpendic_d" => {
            keys(v, &["op", "fn", "pt", "norm", "delta"])?;
            let pt = read_point(at(v, "pt")?, &mut count)?;
            let norm = read_point_d(at(v, "norm")?, &mut count)?;
            let delta = f64_of(at(v, "delta")?)?;
            let out = |c: i64| !(-OFFSET_COORD..=OFFSET_COORD).contains(&c);
            if out(pt.x)
                || out(pt.y)
                || !norm.x.is_finite()
                || !norm.y.is_finite()
                || norm.x.abs() > 2.0
                || norm.y.abs() > 2.0
                || !delta.is_finite()
                || delta.abs() > OFFSET_LIMIT
            {
                return invalid("domain");
            }
            if name == "perpendic" {
                Ok(write_point(&offset::get_perpendic(&pt, &norm, delta)?))
            } else {
                Ok(write_point_d(&offset::get_perpendic_d(&pt, &norm, delta)?))
            }
        }
        "negate_path" => {
            keys(v, &["op", "fn", "path"])?;
            let mut path = read_path_d(at(v, "path")?, &mut count)?;
            offset::negate_path(&mut path);
            Ok(write_path_d(&path))
        }
        "is_closed_path" => {
            keys(v, &["op", "fn", "end"])?;
            Ok(json!(offset::is_closed_path(end_type(at(v, "end")?)?)))
        }
        "lowest_closed_path_info" => {
            keys(v, &["op", "fn", "paths", "is_neg_area"])?;
            let paths = read_paths(at(v, "paths")?, &mut count)?;
            engine_domain(&paths)?;
            let mut idx = Some(7); // reset by the call
            let mut is_neg_area = boolean(at(v, "is_neg_area")?)?;
            offset::get_lowest_closed_path_info(&paths, &mut idx, &mut is_neg_area)?;
            Ok(json!({"idx": idx, "is_neg_area": is_neg_area}))
        }
        _ => invalid("fn"),
    }
}

fn run(v: &Value) -> R<Value> {
    match text(at(v, "op")?)? {
        "clip64" => run_clip64(v),
        "clipd" => run_clip_d(v),
        "offset" => run_offset(v),
        "helper" => run_helper(v),
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
