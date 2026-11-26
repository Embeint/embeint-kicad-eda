// SPDX-License-Identifier: GPL-3.0-or-later
//! Operation decoding for the shim. Validation order and messages match the C++
//! adapter exactly; only validation, conversion and calls into `kicad-core` live here.

use kicad_core::filter_kruskal::{filter_kruskal, KruskalError};
use kicad_core::helpers::{array_dim, ignore_unused, to_underlying, Underlying};
use kicad_core::minoptmax::{MinOptMax, MinOptMaxValue};
use kicad_core::spinlock::{KiSpinLock, SpinLockError};
use kicad_core::sync_queue::SyncQueue;
use kicad_core::union_find::{KiUnionFind, UnionFindError};
use kicad_core_shim::operations::{bounded_text, fields, signed, unsigned, Error};
use serde_json::{json, Value};

use crate::mt19937::Mt19937;

pub type Result<T> = core::result::Result<T, Error>;

pub const MAX_STEPS: usize = 4096;
pub const MAX_SET: u64 = 8192;
pub const MAX_UF_STEPS: usize = 100_000;
pub const MAX_EDGES: u64 = 65_536;
pub const MAX_LIST: usize = 64;

/// A string field of at most 64 KiB (`Text` in the C++ adapter).
pub fn text<'a>(input: &'a Value, name: &str) -> Result<&'a str> {
    bounded_text(input, name)
}

/// An array field of at most `max` entries.
pub fn array<'a>(input: &'a Value, name: &str, max: usize) -> Result<&'a Vec<Value>> {
    let list = input
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid(format!("expected array: {name}")))?;
    if list.len() > max {
        return Err(format!("array too long: {name}").into());
    }
    Ok(list)
}

/// `Signed( { { name, value } }, name, ... )`: range-check a bare JSON value.
pub fn signed_value(value: &Value, name: &str, min: i64, max: i64) -> Result<i64> {
    signed(&json!({ name: value }), name, min, max)
}

/// `Unsigned( { { name, value } }, name )`.
pub fn unsigned_value(value: &Value, name: &str) -> Result<u64> {
    unsigned(&json!({ name: value }), name)
}

/// 16 lowercase hex digits of a double's bits.
pub fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn length_error(what: &str) -> Value {
    json!({"exception": "length_error", "what": what})
}

// ---- MINOPTMAX ------------------------------------------------------------------------

/// A `MINOPTMAX` value type with its JSON range.
trait Number: MinOptMaxValue + Into<i64> + TryFrom<i64> {
    const LOW: i64;
    const HIGH: i64;
}

impl Number for i32 {
    const LOW: i64 = i32::MIN as i64;
    const HIGH: i64 = i32::MAX as i64;
}

impl Number for i64 {
    const LOW: i64 = i64::MIN;
    const HIGH: i64 = i64::MAX;
}

fn number<T: Number>(value: &Value, name: &str) -> Result<T> {
    T::try_from(signed_value(value, name, T::LOW, T::HIGH)?)
        .map_err(|_| Error::Invalid(format!("integer out of range: {name}")))
}

fn minoptmax_state<T: Number>(value: &MinOptMax<T>, probes: &[T], defaults: &[T]) -> Value {
    let matches: Vec<bool> = probes.iter().map(|p| value.matches(*p)).collect();
    let pinned: Vec<i64> = defaults
        .iter()
        .map(|d| value.pinned_opt_or(*d).into())
        .collect();
    json!({
        "min": Into::<i64>::into(value.min()),
        "max": Into::<i64>::into(value.max()),
        "opt": Into::<i64>::into(value.opt()),
        "pinned_opt": Into::<i64>::into(value.pinned_opt()),
        "has_min": value.has_min(),
        "has_max": value.has_max(),
        "has_opt": value.has_opt(),
        "is_null": value.is_null(),
        "matches": matches,
        "pinned_opt_or": pinned,
    })
}

fn run_minoptmax<T: Number>(input: &Value) -> Result<Value> {
    let probes = array(input, "probes", MAX_LIST)?
        .iter()
        .map(|p| number::<T>(p, "value"))
        .collect::<Result<Vec<T>>>()?;
    let defaults = array(input, "defaults", MAX_LIST)?
        .iter()
        .map(|d| number::<T>(d, "value"))
        .collect::<Result<Vec<T>>>()?;
    let mut sets = Vec::new();
    for set in array(input, "sets", MAX_LIST)? {
        fields(set, &["set", "value"], &[])?;
        let which = text(set, "set")?;
        if !matches!(which, "min" | "max" | "opt") {
            return Err("unknown setter".into());
        }
        let value = number::<T>(set.get("value").unwrap_or(&Value::Null), "value")?;
        sets.push((which, value));
    }

    let mut value: MinOptMax<T> = MinOptMax::new();
    let mut states = vec![minoptmax_state(&value, &probes, &defaults)];
    for (which, number) in sets {
        match which {
            "min" => value.set_min(number),
            "max" => value.set_max(number),
            _ => value.set_opt(number),
        }
        states.push(minoptmax_state(&value, &probes, &defaults));
    }
    Ok(json!({ "states": states }))
}

fn minoptmax(input: &Value) -> Result<Value> {
    fields(input, &["op", "type", "sets", "probes", "defaults"], &[])?;
    match text(input, "type")? {
        "int" => run_minoptmax::<i32>(input),
        "long long" => run_minoptmax::<i64>(input),
        _ => Err("unknown type".into()),
    }
}

// ---- KI_UNION_FIND --------------------------------------------------------------------

fn require_count(count: u64) -> Result<()> {
    if count > MAX_SET && count <= u64::from(u32::MAX) {
        return Err("count too large".into());
    }
    Ok(())
}

fn parents(forest: &KiUnionFind) -> Value {
    json!(forest.parents())
}

fn union_find_error(error: UnionFindError, index_message: &str) -> Error {
    match error {
        UnionFindError::IndexOutOfRange => index_message.into(),
        UnionFindError::LengthError => UnionFindError::LENGTH_ERROR_WHAT.into(),
        UnionFindError::Allocation => "allocation failed".into(),
    }
}

fn union_find(input: &Value) -> Result<Value> {
    fields(input, &["op", "count", "steps"], &[])?;
    let count = unsigned(input, "count")?;
    let steps = array(input, "steps", MAX_UF_STEPS)?;
    require_count(count)?;

    for step in steps {
        match text(step, "op")? {
            "unite" | "connected" => {
                fields(step, &["op", "a", "b"], &[])?;
                unsigned(step, "a")?;
                unsigned(step, "b")?;
            }
            "find" | "find_compress" => {
                fields(step, &["op", "x"], &[])?;
                unsigned(step, "x")?;
            }
            "reset" => {
                fields(step, &["op", "count"], &[])?;
                require_count(unsigned(step, "count")?)?;
            }
            "size" | "component_count" => {
                fields(step, &["op"], &[])?;
            }
            _ => return Err("unknown operation".into()),
        }
    }

    let mut forest = match KiUnionFind::new(count) {
        Ok(forest) => forest,
        Err(UnionFindError::LengthError) => {
            return Ok(json!({
                "constructed": length_error(UnionFindError::LENGTH_ERROR_WHAT)
            }))
        }
        Err(error) => return Err(union_find_error(error, "index out of range")),
    };

    let index = |error| union_find_error(error, "index out of range");
    let mut results = Vec::new();
    for step in steps {
        let result = match text(step, "op")? {
            "unite" => json!(forest
                .unite(unsigned(step, "a")?, unsigned(step, "b")?)
                .map_err(index)?),
            "connected" => json!(forest
                .connected(unsigned(step, "a")?, unsigned(step, "b")?)
                .map_err(index)?),
            "find" => json!(forest.find(unsigned(step, "x")?).map_err(index)?),
            "find_compress" => json!(forest.find_compress(unsigned(step, "x")?).map_err(index)?),
            "reset" => match forest.reset(unsigned(step, "count")?) {
                Ok(()) => Value::Null,
                Err(UnionFindError::LengthError) => length_error(UnionFindError::LENGTH_ERROR_WHAT),
                Err(error) => return Err(index(error)),
            },
            "size" => json!(forest.size()),
            _ => json!(forest.component_count()),
        };
        results.push(result);
    }

    Ok(json!({
        "constructed": true,
        "results": results,
        "components": forest.component_count(),
        "parents": parents(&forest),
    }))
}

// ---- KI_MST::FilterKruskal ------------------------------------------------------------

#[derive(Clone, Copy)]
struct Edge {
    u: u32,
    v: u32,
    weight: i64,
    id: u32,
}

/// The C++ adapter's `GenerateEdges`: QA-shaped edges from `std::mt19937`.
fn generate_edges(spec: &Value, vertices: u64, seeds: &mut Vec<(u64, u64)>) -> Result<Vec<Edge>> {
    fields(
        spec,
        &["seed", "discard", "count", "weight_range"],
        &["seeds", "islands", "weight_add", "weight_set"],
    )?;
    let seed = unsigned(spec, "seed")?;
    let discard = unsigned(spec, "discard")?;
    let count = unsigned(spec, "count")?;
    let range = signed(spec, "weight_range", 1, i64::from(u32::MAX))?;
    let seed_count = if spec.get("seeds").is_some() {
        unsigned(spec, "seeds")?
    } else {
        0
    };
    let islands = if spec.get("islands").is_some() {
        unsigned(spec, "islands")?
    } else {
        0
    };
    let add = if spec.get("weight_add").is_some() {
        signed(spec, "weight_add", -(1 << 30), 1 << 30)?
    } else {
        0
    };
    let set = if spec.get("weight_set").is_some() {
        Some(signed(spec, "weight_set", -(1 << 30), 1 << 30)?)
    } else {
        None
    };

    if seed > u64::from(u32::MAX)
        || discard > 1 << 24
        || count > MAX_EDGES
        || seed_count > MAX_EDGES
    {
        return Err("generator out of range".into());
    }
    if vertices == 0 || (islands != 0 && (islands > vertices || vertices % islands != 0)) {
        return Err("generator needs vertices".into());
    }

    let mut rng = Mt19937::new(seed as u32);
    rng.discard(discard);
    let mut draw = || u64::from(rng.next_u32());

    for _ in 0..seed_count {
        let a = draw() % vertices;
        let b = draw() % vertices;
        seeds.push((a, b));
    }

    let range = range as u64;
    let mut edges = Vec::new();
    for id in 0..count {
        let (u, v, weight) = if let Some(size) = vertices.checked_div(islands) {
            let island = draw() % islands;
            let u = island * size + draw() % size;
            let v = island * size + draw() % size;
            (u, v, (draw() % range) as i64)
        } else {
            let u = draw() % vertices;
            let v = draw() % vertices;
            (u, v, (draw() % range) as i64)
        };
        edges.push(Edge {
            u: u as u32,
            v: v as u32,
            weight: set.unwrap_or(weight + add),
            id: id as u32,
        });
    }
    Ok(edges)
}

fn run_kruskal<L>(edges: &mut [Edge], forest: &mut KiUnionFind, less: L) -> Result<Value>
where
    L: FnMut(&Edge, &Edge) -> bool,
{
    let mut emitted = Vec::new();
    let returned = filter_kruskal(
        edges,
        forest,
        less,
        |edge: &Edge| (u64::from(edge.u), u64::from(edge.v)),
        |edge: &Edge| emitted.push(edge.id),
    )
    .map_err(|error| match error {
        KruskalError::UnionFind(error) => union_find_error(error, "endpoint out of range"),
        KruskalError::InvalidOrdering => Error::Invalid("invalid ordering".into()),
    })?;
    let order: Vec<u32> = edges.iter().map(|edge| edge.id).collect();
    Ok(json!({
        "emitted": emitted,
        "returned": returned,
        "order": order,
        "components": forest.component_count(),
        "parents": parents(forest),
    }))
}

fn kruskal(input: &Value) -> Result<Value> {
    fields(
        input,
        &["op", "vertices", "less"],
        &["edges", "generate", "unite"],
    )?;
    let vertices = unsigned(input, "vertices")?;
    let less = text(input, "less")?;
    if vertices > MAX_SET {
        return Err("count too large".into());
    }
    if !matches!(less, "weight" | "weight_id" | "weight_uv") {
        return Err("unknown order".into());
    }
    if input.get("edges").is_some() == input.get("generate").is_some() {
        return Err("expected edges or generate".into());
    }

    let mut seeds = Vec::new();
    let mut edges = Vec::new();
    if input.get("edges").is_some() {
        for item in array(input, "edges", MAX_SET as usize)? {
            let triple = item
                .as_array()
                .filter(|t| t.len() == 3)
                .ok_or("expected edge triple")?;
            let id = edges.len() as u32;
            edges.push(Edge {
                u: signed_value(&triple[0], "u", 0, i64::from(u32::MAX))? as u32,
                v: signed_value(&triple[1], "v", 0, i64::from(u32::MAX))? as u32,
                weight: signed_value(&triple[2], "w", i64::MIN, i64::MAX)?,
                id,
            });
        }
    } else {
        edges = generate_edges(
            input.get("generate").unwrap_or(&Value::Null),
            vertices,
            &mut seeds,
        )?;
    }

    if input.get("unite").is_some() {
        for pair in array(input, "unite", MAX_SET as usize)? {
            let pair = pair
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or("expected index pair")?;
            seeds.push((
                unsigned_value(&pair[0], "a")?,
                unsigned_value(&pair[1], "b")?,
            ));
        }
    }

    let mut forest = KiUnionFind::new(vertices)
        .map_err(|error| union_find_error(error, "index out of range"))?;
    for (a, b) in seeds {
        forest
            .unite(a, b)
            .map_err(|error| union_find_error(error, "index out of range"))?;
    }

    match less {
        "weight" => run_kruskal(&mut edges, &mut forest, |a, b| a.weight < b.weight),
        "weight_id" => run_kruskal(&mut edges, &mut forest, |a, b| {
            (a.weight, a.id) < (b.weight, b.id)
        }),
        _ => run_kruskal(&mut edges, &mut forest, |a, b| {
            (a.weight, a.u, a.v) < (b.weight, b.u, b.v)
        }),
    }
}

// ---- KISPINLOCK and SYNC_QUEUE ---------------------------------------------------------

fn spinlock(input: &Value) -> Result<Value> {
    fields(input, &["op", "steps"], &[])?;
    let steps = array(input, "steps", MAX_STEPS)?;
    for step in steps {
        if !matches!(step.as_str(), Some("lock" | "try_lock" | "unlock" | "test")) {
            return Err("unknown operation".into());
        }
    }
    let mut lock = KiSpinLock::new();
    let mut results = Vec::new();
    for step in steps {
        results.push(match step.as_str() {
            Some("lock") => {
                lock.lock().map_err(|SpinLockError::WouldSpinForever| {
                    Error::Invalid("lock would spin forever".into())
                })?;
                Value::Null
            }
            Some("try_lock") => json!(lock.try_lock()),
            Some("unlock") => {
                lock.unlock();
                Value::Null
            }
            _ => json!(lock.test()),
        });
    }
    Ok(json!({ "results": results }))
}

fn sync_queue(input: &Value) -> Result<Value> {
    fields(input, &["op", "receiver", "steps"], &[])?;
    let mut receiver = text(input, "receiver")?.to_owned();
    let steps = array(input, "steps", MAX_STEPS)?;
    for step in steps {
        match text(step, "op")? {
            "push" | "move_push" => {
                fields(step, &["op", "value"], &[])?;
                text(step, "value")?;
            }
            "pop" | "empty" | "size" | "clear" => {
                fields(step, &["op"], &[])?;
            }
            _ => return Err("unknown operation".into()),
        }
    }
    let mut queue: SyncQueue<String> = SyncQueue::new();
    let mut results = Vec::new();
    for step in steps {
        results.push(match text(step, "op")? {
            "push" => {
                let value = text(step, "value")?.to_owned();
                queue
                    .push(&value)
                    .map_err(|_| Error::Invalid("allocation failed".into()))?;
                Value::Null
            }
            "move_push" => {
                let value = text(step, "value")?.to_owned();
                queue
                    .move_push(value)
                    .map_err(|_| Error::Invalid("allocation failed".into()))?;
                Value::Null
            }
            "pop" => {
                let popped = queue.pop(&mut receiver);
                json!({"popped": popped, "receiver": receiver})
            }
            "empty" => json!(queue.empty()),
            "size" => json!(queue.size()),
            _ => {
                queue.clear();
                Value::Null
            }
        });
    }
    Ok(json!({ "results": results }))
}

// ---- small helpers --------------------------------------------------------------------

macro_rules! underlying_enum {
    ($name:ident, $repr:ty) => {
        /// An enumeration with a fixed underlying type and no named enumerators; every
        /// value of the type is a valid value of the enumeration, as in C++.
        #[derive(Clone, Copy)]
        struct $name($repr);

        impl Underlying for $name {
            type Repr = $repr;
            fn into_underlying(self) -> $repr {
                self.0
            }
        }
    };
}

underlying_enum!(EI8, i8);
underlying_enum!(EU8, u8);
underlying_enum!(EI16, i16);
underlying_enum!(EU32, u32);
underlying_enum!(EI64, i64);
underlying_enum!(EU64, u64);
underlying_enum!(EInt, i32);

/// `PAIR` of the C++ adapter (an aggregate element type).
#[derive(Clone, Copy, Default)]
struct Pair {
    _a: i32,
    _b: f64,
}

fn helpers(input: &Value, op: &str) -> Result<Value> {
    match op {
        "array_dim" => {
            fields(input, &["op", "kind"], &[])?;
            let value = match text(input, "kind")? {
                "char[1]" => array_dim(&[0u8; 1]),
                "int[3]" => array_dim(&[0i32; 3]),
                "double[16]" => array_dim(&[0f64; 16]),
                "pair[7]" => array_dim(&[Pair::default(); 7]),
                "char[1000]" => array_dim(&[0u8; 1000]),
                _ => return Err("unknown kind".into()),
            };
            Ok(json!({ "value": value }))
        }
        "to_underlying" => {
            fields(input, &["op", "type", "value"], &[])?;
            let value = match text(input, "type")? {
                "int8" => {
                    json!(to_underlying(EI8(
                        signed(input, "value", i8::MIN.into(), i8::MAX.into())? as i8
                    )))
                }
                "uint8" => json!(to_underlying(EU8(
                    signed(input, "value", 0, u8::MAX.into())? as u8
                ))),
                "int16" => json!(to_underlying(EI16(signed(
                    input,
                    "value",
                    i16::MIN.into(),
                    i16::MAX.into()
                )? as i16))),
                "uint32" => json!(to_underlying(EU32(
                    signed(input, "value", 0, u32::MAX.into())? as u32
                ))),
                "int64" => json!(to_underlying(EI64(signed(
                    input,
                    "value",
                    i64::MIN,
                    i64::MAX
                )?))),
                "uint64" => json!(to_underlying(EU64(unsigned(input, "value")?))),
                "int" => json!(to_underlying(EInt(signed(
                    input,
                    "value",
                    i32::MIN.into(),
                    i32::MAX.into()
                )? as i32))),
                _ => return Err("unknown type".into()),
            };
            Ok(json!({ "value": value }))
        }
        _ => {
            fields(input, &["op", "value"], &[])?;
            let value = text(input, "value")?.to_owned();
            ignore_unused(&value);
            Ok(json!({ "value": value }))
        }
    }
}

/// Run one request input.
pub fn run(input: &Value) -> Result<Value> {
    let op = text(input, "op")?;
    match op {
        "minoptmax" => minoptmax(input),
        "union_find" => union_find(input),
        "filter_kruskal" => kruskal(input),
        "spinlock" => spinlock(input),
        "sync_queue" => sync_queue(input),
        "profile" => crate::profile_script::run(input),
        "observable" => kicad_core_shim::observable_driver::run(input),
        "array_dim" | "to_underlying" | "ignore_unused" => helpers(input, op),
        _ => Err("unknown operation".into()),
    }
}
