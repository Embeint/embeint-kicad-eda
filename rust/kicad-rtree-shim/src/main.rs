// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_rtree::{BulkEntry, Coord, DynamicRtree, NodeData};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
const UNIT: &str = "libs.kimath.include.geometry.rtree.application.pure.p001";
type Outcome = Result<Value, String>;
fn keys(value: &Value, required: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("expected object")?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object.keys().any(|key| !required.contains(&key.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

fn signed(object: &Value, name: &str, min: i64, max: i64) -> Result<i64, String> {
    object
        .get(name)
        .and_then(Value::as_i64)
        .filter(|number| (min..=max).contains(number))
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn int(object: &Value, name: &str) -> Result<i32, String> {
    let value = signed(object, name, i32::MIN.into(), i32::MAX.into())?;
    i32::try_from(value).map_err(|_| format!("integer out of range: {name}"))
}

/// A request coordinate type: the node `ELEMTYPE`s plus `uint32_t` Hilbert input.
trait Wire: Copy {
    fn from_wire(value: i64) -> Option<Self>;
}

impl Wire for i32 {
    fn from_wire(value: i64) -> Option<Self> {
        i32::try_from(value).ok()
    }
}

impl Wire for i64 {
    fn from_wire(value: i64) -> Option<Self> {
        Some(value)
    }
}

impl Wire for u32 {
    fn from_wire(value: i64) -> Option<Self> {
        u32::try_from(value).ok()
    }
}

fn values<E: Wire>(object: &Value, name: &str, length: usize) -> Result<Vec<E>, String> {
    let invalid = || "invalid coordinates".to_string();
    let array = object
        .get(name)
        .and_then(Value::as_array)
        .filter(|array| array.len() == length)
        .ok_or_else(invalid)?;
    array
        .iter()
        .map(|value| value.as_i64().and_then(E::from_wire).ok_or_else(invalid))
        .collect()
}

fn fixed<E: Wire, const N: usize>(object: &Value, name: &str) -> Result<[E; N], String> {
    <[E; N]>::try_from(values::<E>(object, name, N)?).map_err(|_| "invalid coordinates".into())
}

#[derive(Clone, Debug, PartialEq)]
struct Item(i32);
impl NodeData for Item {}
fn item(step: &Value) -> Result<Item, String> {
    Ok(Item(signed(step, "data", -1, 65535)? as i32))
}
fn array<'a>(v: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    v.get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("expected array: {name}"))
}
fn entry<E: Coord + Wire, const D: usize>(v: &Value) -> Result<BulkEntry<Item, E, D>, String> {
    keys(v, &["min", "max", "data"])?;
    Ok(BulkEntry {
        min: fixed(v, "min")?,
        max: fixed(v, "max")?,
        data: item(v)?,
    })
}
fn collect<E: Coord, const D: usize, const F: usize>(
    tree: &DynamicRtree<Item, E, D, F>,
    limit: i32,
) -> Outcome {
    let mut it = tree.begin().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while !it.at_end() && (limit < 0 || out.len() < limit as usize) {
        out.push(it.current().ok_or("missing iterator data")?.0);
        it.advance().map_err(|e| e.to_string())?;
    }
    Ok(json!(out))
}
fn run_tree<E: Coord + Wire, const D: usize, const F: usize>(steps: &[Value]) -> Outcome {
    let mut tree = DynamicRtree::<Item, E, D, F>::with_limit(65536).map_err(|e| e.to_string())?;
    let mut outputs = Vec::new();
    for step in steps {
        let op = step
            .get("op")
            .and_then(Value::as_str)
            .ok_or("expected op string")?;
        let map = |e: kicad_rtree::Error| e.to_string();
        let output = match op {
            "insert" => {
                keys(step, &["op", "min", "max", "data"])?;
                tree.insert(fixed(step, "min")?, fixed(step, "max")?, item(step)?)
                    .map_err(map)?;
                Value::Null
            }
            "bulk" => {
                keys(step, &["op", "entries"])?;
                let values = array(step, "entries")?;
                if values.len() > 65536 {
                    return Err("resource budget exceeded".into());
                }
                let entries = values
                    .iter()
                    .map(entry::<E, D>)
                    .collect::<Result<Vec<_>, _>>()?;
                tree.bulk_load(&entries).map_err(map)?;
                Value::Null
            }
            "remove" => {
                keys(step, &["op", "min", "max", "data"])?;
                json!(tree
                    .remove(fixed(step, "min")?, fixed(step, "max")?, &item(step)?)
                    .map_err(map)?)
            }
            "clear" => {
                keys(step, &["op"])?;
                tree.remove_all().map_err(map)?;
                Value::Null
            }
            "iter" => {
                keys(step, &["op", "limit"])?;
                collect(&tree, int(step, "limit")?)?
            }
            "query" => {
                keys(step, &["op", "min", "max", "stop"])?;
                let min = fixed(step, "min")?;
                let max = fixed(step, "max")?;
                let stop = int(step, "stop")?;
                let mut hits = Vec::new();
                let found = tree
                    .search(min, max, &mut |item: &Item| {
                        hits.push(item.0);
                        stop <= 0 || hits.len() < (stop as usize)
                    })
                    .map_err(map)?;
                let range = tree.overlapping(min, max).map_err(map)?;
                let empty = range.empty().map_err(map)?;
                let mut it = range.begin().map_err(map)?;
                let mut lazy = Vec::new();
                while !it.at_end() {
                    lazy.push(it.current().ok_or("missing iterator data")?.0);
                    it.advance().map_err(map)?;
                }
                json!({"found":found,"hits":hits,"lazy":lazy,"empty":empty})
            }
            "nearest" => {
                keys(step, &["op", "point", "k"])?;
                let mut out = vec![(99, Item(65535))];
                tree.nearest_neighbors(fixed(step, "point")?, int(step, "k")?, &mut out)
                    .map_err(map)?;
                json!(out
                    .into_iter()
                    .map(|(d, v)| json!([d, v.0]))
                    .collect::<Vec<_>>())
            }
            "state" => {
                keys(step, &["op"])?;
                let a = tree.begin().map_err(map)?;
                let mut b = tree.begin().map_err(map)?;
                b.advance().map_err(map)?;
                json!({"all":collect(&tree,-1)?,"begin_equal_next":a==b,"end":a.at_end()})
            }
            "move" => {
                keys(step, &["op", "kind"])?;
                let kind = step
                    .get("kind")
                    .and_then(Value::as_str)
                    .ok_or("expected move kind")?;
                if kind == "self" {
                    Value::Null
                } else {
                    let mut dst = DynamicRtree::<Item, E, D, F>::with_limit(65536).map_err(map)?;
                    if kind == "assign" {
                        dst.insert([E::default(); D], [E::default(); D], Item(65535))
                            .map_err(map)?;
                        dst.move_assign(&mut tree).map_err(map)?;
                    } else if kind == "construct" {
                        dst = tree.move_out().map_err(map)?;
                    } else {
                        return Err("unknown move kind".into());
                    }
                    let source = json!({"size":tree.size().map_err(map)?,"empty":tree.empty().map_err(map)?,"memory":tree.memory_usage().map_err(map)?});
                    tree = dst;
                    source
                }
            }
            _ => return Err("unknown op".into()),
        };
        outputs.push(json!({"value":output,"size":tree.size().map_err(map)?,"empty":tree.empty().map_err(map)?,"memory":tree.memory_usage().map_err(map)?}));
    }
    Ok(json!(outputs))
}
fn run(input: &Value) -> Outcome {
    keys(input, &["config", "steps"])?;
    let config = input
        .get("config")
        .and_then(Value::as_str)
        .ok_or("expected config string")?;
    let steps = array(input, "steps")?;
    if steps.is_empty() || steps.len() > 4096 {
        return Err("steps outside 1..4096".into());
    }
    match config {
        "i32.d2.m16" => run_tree::<i32, 2, 16>(steps),
        "i32.d2.m4" => run_tree::<i32, 2, 4>(steps),
        "i32.d2.m28" => run_tree::<i32, 2, 28>(steps),
        "i32.d3.m3" => run_tree::<i32, 3, 3>(steps),
        "i32.d3.m5" => run_tree::<i32, 3, 5>(steps),
        "i32.d1.m8" => run_tree::<i32, 1, 8>(steps),
        "i64.d2.m16" => run_tree::<i64, 2, 16>(steps),
        "i64.d3.m8" => run_tree::<i64, 3, 8>(steps),
        _ => Err("unknown config".into()),
    }
}
fn dispatch(line: &str) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"unit":unit,"case":case,"error":{"kind":kind,"message":message}});
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
    if let Some(value) = request.get("unit").filter(|value| value.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|value| value.is_string()) {
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
        Ok(result) => json!({"unit":unit,"case":case,"result":result}),
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        writeln!(stdout, "{}", dispatch(&line?))?;
        stdout.flush()?;
    }
    Ok(())
}
