// SPDX-License-Identifier: GPL-3.0-or-later
//! Native I/O adapter. The portable tree contains no JSON or host operations.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kicad_rtree::{BulkEntry, Coord, DynamicRtree, NodeData, NodeHandle};
use kicad_rtree_node::{RtreeNode, SlabArena, SlabNode};
use serde_json::{json, Value};
pub const UNIT: &str = "libs.kimath.include.geometry.rtree.application.pure.p001";
const LIMIT: usize = 8192;
#[derive(Clone, Debug, PartialEq)]
struct Item(i64);
impl NodeData for Item {}
trait Wire: Coord {
    fn from_wire(v: i64) -> Option<Self>;
}
impl Wire for i32 {
    fn from_wire(v: i64) -> Option<Self> {
        i32::try_from(v).ok()
    }
}
impl Wire for i64 {
    fn from_wire(v: i64) -> Option<Self> {
        Some(v)
    }
}
type Outcome = Result<Value, String>;
fn keys(v: &Value, ks: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("expected object")?;
    if ks.iter().any(|k| !o.contains_key(*k)) || o.keys().any(|k| !ks.contains(&k.as_str())) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}
fn number(v: &Value, k: &str, lo: i64, hi: i64) -> Result<i64, String> {
    v.get(k)
        .and_then(Value::as_i64)
        .filter(|x| (lo..=hi).contains(x))
        .ok_or_else(|| format!("integer out of range: {k}"))
}
fn coords<E: Wire, const N: usize>(v: &Value, k: &str) -> Result<[E; N], String> {
    let a = v
        .get(k)
        .and_then(Value::as_array)
        .filter(|a| a.len() == N)
        .ok_or("invalid coordinates")?;
    let mut out = [E::default(); N];
    for (i, x) in a.iter().enumerate() {
        out[i] = x
            .as_i64()
            .and_then(E::from_wire)
            .ok_or("invalid coordinates")?;
    }
    Ok(out)
}
fn item(v: &Value) -> Result<Option<Item>, String> {
    if v.is_null() {
        Ok(None)
    } else {
        v.as_i64()
            .filter(|x| (0..LIMIT as i64).contains(x))
            .map(|x| Some(Item(x)))
            .ok_or("invalid item".into())
    }
}
fn wire_item(v: &Option<Item>) -> Value {
    v.as_ref().map_or(Value::Null, |x| json!(x.0))
}
fn results(out: &[(i64, Option<Item>)]) -> Value {
    json!(out
        .iter()
        .map(|(d, x)| json!([d, wire_item(x)]))
        .collect::<Vec<_>>())
}
fn snapshot<E: Wire, const N: usize, const M: usize>(
    tree: &DynamicRtree<Item, E, N, M>,
    h: &NodeHandle,
) -> Outcome {
    let node = tree.node(h).map_err(|e| e.to_string())?;
    let mut children = Vec::new();
    for i in 0..node.count as usize {
        let (lo, hi) = node.get_child_bounds(i).map_err(|e| e.to_string())?;
        let min: Vec<i64> = lo.iter().map(|e| e.widen()).collect();
        let max: Vec<i64> = hi.iter().map(|e| e.widen()).collect();
        if node.is_leaf() {
            let (il, ih) = node.get_insert_bounds(i).map_err(|e| e.to_string())?;
            children.push(json!({"min":min,"max":max,"insert_min":il.iter().map(|e|e.widen()).collect::<Vec<_>>(),
                "insert_max":ih.iter().map(|e|e.widen()).collect::<Vec<_>>(),"item":wire_item(&node.data(i).map_err(|e|e.to_string())?.cloned())}));
        } else {
            children.push(json!({"min":min,"max":max,"child":snapshot(tree,node.child(i).map_err(|e|e.to_string())?.ok_or("missing child")?)?}));
        }
    }
    Ok(json!({"level":node.level,"children":children}))
}
fn program<E: Wire, const N: usize, const M: usize>(steps: &[Value]) -> Outcome {
    let mut trees = [
        DynamicRtree::<Item, E, N, M>::new(LIMIT).map_err(|e| e.to_string())?,
        DynamicRtree::<Item, E, N, M>::new(LIMIT).map_err(|e| e.to_string())?,
    ];
    let mut events = Vec::new();
    for s in steps {
        let op = s
            .get("op")
            .and_then(Value::as_str)
            .ok_or("expected op string")?;
        let a = number(s, "a", 0, 1)? as usize;
        let event = match op {
            "layout" => {
                keys(s, &["op", "a"])?;
                let node =
                    RtreeNode::<Item, E, N, M>::reference_layout().map_err(|e| e.to_string())?;
                let page = SlabArena::<RtreeNode<Item, E, N, M>>::reference_page_layout()
                    .map_err(|e| e.to_string())?;
                json!({"node_size":node.size,"node_align":node.align,"page_size":page.size,"page_align":page.align,
                    "pointer_size":8,"elem_size":E::REFERENCE_SIZE,"min_nodes":M*2/5,"reinsert_count":M*3/10})
            }
            "insert" | "remove" => {
                keys(s, &["op", "a", "min", "max", "item"])?;
                let min = coords(s, "min")?;
                let max = coords(s, "max")?;
                let data = item(&s["item"])?;
                if op == "insert" {
                    trees[a].insert(min, max, data).map_err(|e| e.to_string())?;
                    json!({})
                } else {
                    json!({"removed":trees[a].remove(min,max,data).map_err(|e|e.to_string())?})
                }
            }
            "bulk" => {
                keys(s, &["op", "a", "entries"])?;
                let entries = s["entries"]
                    .as_array()
                    .filter(|e| e.len() <= LIMIT)
                    .ok_or("invalid entries")?;
                let mut batch = Vec::new();
                for e in entries {
                    keys(e, &["min", "max", "item"])?;
                    batch.push(BulkEntry {
                        min: coords(e, "min")?,
                        max: coords(e, "max")?,
                        data: item(&e["item"])?,
                    });
                }
                trees[a].bulk_load(&batch).map_err(|e| e.to_string())?;
                json!({"entries":entries})
            }
            "clear" => {
                keys(s, &["op", "a"])?;
                trees[a].clear().map_err(|e| e.to_string())?;
                json!({})
            }
            "state" => {
                keys(s, &["op", "a"])?;
                let root = trees[a]
                    .root_handle()
                    .map(|h| snapshot(&trees[a], &h))
                    .transpose()?
                    .unwrap_or(Value::Null);
                json!({"size":trees[a].len(),"empty":trees[a].is_empty(),"memory":trees[a].memory_usage().map_err(|e|e.to_string())?,"root":root})
            }
            "iterate" => {
                keys(s, &["op", "a", "skip", "take"])?;
                let skip = number(s, "skip", 0, LIMIT as i64)? as usize;
                let take = number(s, "take", 0, LIMIT as i64)? as usize;
                let mut it = trees[a].begin().map_err(|e| e.to_string())?;
                let end = trees[a].end();
                let mut same = it.clone();
                same.advance().map_err(|e| e.to_string())?;
                let equal_next = it == same;
                for _ in 0..skip {
                    it.advance().map_err(|e| e.to_string())?;
                }
                let mut items = Vec::new();
                for _ in 0..take {
                    if it == end {
                        break;
                    }
                    items.push(wire_item(it.current()));
                    it.advance().map_err(|e| e.to_string())?;
                }
                json!({"items":items,"at_end":it==end,"current":wire_item(it.current()),"equal_next":equal_next})
            }
            "overlap" => {
                keys(s, &["op", "a", "min", "max", "take"])?;
                let min = coords(s, "min")?;
                let max = coords(s, "max")?;
                let take = number(s, "take", 0, LIMIT as i64)? as usize;
                let range = trees[a].overlapping(min, max).map_err(|e| e.to_string())?;
                let mut it = range.begin().map_err(|e| e.to_string())?;
                let end = range.end();
                let mut same = it.clone();
                same.advance().map_err(|e| e.to_string())?;
                let equal_next = it == same;
                let mut items = Vec::new();
                for _ in 0..take {
                    if it == end {
                        break;
                    }
                    items.push(wire_item(it.current()));
                    it.advance().map_err(|e| e.to_string())?;
                }
                json!({"items":items,"empty":range.is_empty().map_err(|e|e.to_string())?,"at_end":it==end,"current":wire_item(it.current()),"equal_next":equal_next})
            }
            "search" => {
                keys(s, &["op", "a", "min", "max", "stop", "seed"])?;
                let min = coords(s, "min")?;
                let max = coords(s, "max")?;
                let stop = number(s, "stop", 0, LIMIT as i64)? as usize;
                let seed = s["seed"]
                    .as_array()
                    .filter(|e| e.len() <= LIMIT)
                    .ok_or("invalid seed")?;
                let mut items = Vec::new();
                for x in seed {
                    items.push(wire_item(&item(x)?));
                }
                let mut hits = 0;
                let found = trees[a]
                    .search(min, max, &mut |x| {
                        items.push(x.map_or(Value::Null, |v| json!(v.0)));
                        hits += 1;
                        hits < stop
                    })
                    .map_err(|e| e.to_string())?;
                json!({"items":items,"found":found})
            }
            "nearest" => {
                keys(s, &["op", "a", "point", "k", "seed"])?;
                let point = coords(s, "point")?;
                let k = number(s, "k", i32::MIN as i64, i32::MAX as i64)? as i32;
                let seed = s["seed"]
                    .as_array()
                    .filter(|e| e.len() <= LIMIT)
                    .ok_or("invalid seed")?;
                let mut out = Vec::new();
                for x in seed {
                    out.push((-7, item(x)?));
                }
                trees[a]
                    .nearest_neighbors(point, k, &mut out)
                    .map_err(|e| e.to_string())?;
                json!({"neighbors":results(&out)})
            }
            "move_new" | "move_assign" => {
                keys(s, &["op", "a", "src"])?;
                let src = number(s, "src", 0, 1)? as usize;
                if a == src {
                    trees[a].move_assign_self();
                } else {
                    let moved =
                        DynamicRtree::move_construct(&mut trees[src]).map_err(|e| e.to_string())?;
                    trees[a] = moved;
                }
                json!({})
            }
            _ => return Err("unknown op".into()),
        };
        events.push(event);
    }
    Ok(json!({"events":events}))
}
pub fn run(input: &Value) -> Outcome {
    keys(input, &["config", "steps"])?;
    let config = input["config"].as_str().ok_or("expected config string")?;
    let steps = input["steps"]
        .as_array()
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or("steps outside 1..4096")?;
    match config {
        "i32.d2.m4" => program::<i32, 2, 4>(steps),
        "i32.d2.m8" => program::<i32, 2, 8>(steps),
        "i32.d2.m16" => program::<i32, 2, 16>(steps),
        "i32.d2.m28" => program::<i32, 2, 28>(steps),
        "i32.d3.m5" => program::<i32, 3, 5>(steps),
        "i32.d3.m16" => program::<i32, 3, 16>(steps),
        "i32.d1.m8" => program::<i32, 1, 8>(steps),
        "i64.d2.m16" => program::<i64, 2, 16>(steps),
        "i64.d3.m8" => program::<i64, 3, 8>(steps),
        "i32.d1.m3" => program::<i32, 1, 3>(steps),
        _ => Err("unknown config".into()),
    }
}
