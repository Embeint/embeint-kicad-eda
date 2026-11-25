// SPDX-License-Identifier: GPL-3.0-or-later
//! Native JSON adapter; the spatial crate has no protocol or host I/O.
use kicad_rtree_cow_packed::{
    Budget, Builder, BulkEntry, Coord, CowForest, NodeData, NodeHandle, PackedRtree, TreeId,
};
use serde_json::{json, Value};
pub const UNIT: &str = "libs.kimath.include.geometry.rtree.application.pure.p002";
const LIMIT: usize = 8192;
#[derive(Clone, Debug, PartialEq)]
struct Item(i64);
impl NodeData for Item {}
trait Wire: Coord {
    fn from_wire(n: i64) -> Option<Self>;
}
impl Wire for i32 {
    fn from_wire(n: i64) -> Option<Self> {
        i32::try_from(n).ok()
    }
}
impl Wire for i64 {
    fn from_wire(n: i64) -> Option<Self> {
        Some(n)
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
fn wi(v: &Option<Item>) -> Value {
    v.as_ref().map_or(Value::Null, |x| json!(x.0))
}
fn wide<E: Coord, const N: usize>(a: &[E; N]) -> Vec<i64> {
    a.iter().map(|x| x.widen()).collect()
}
fn batch<E: Wire, const N: usize>(s: &Value) -> Result<Vec<BulkEntry<Item, E, N>>, String> {
    let es = s["entries"]
        .as_array()
        .filter(|a| a.len() <= LIMIT)
        .ok_or("invalid entries")?;
    let mut out = Vec::new();
    for e in es {
        keys(e, &["min", "max", "item"])?;
        out.push(BulkEntry {
            min: coords(e, "min")?,
            max: coords(e, "max")?,
            data: item(&e["item"])?,
        });
    }
    Ok(out)
}
fn entries<E: Wire, const N: usize>(es: &[BulkEntry<Item, E, N>]) -> Value {
    json!(es
        .iter()
        .map(|e| json!({"min":wide(&e.min),"max":wide(&e.max),"item":wi(&e.data)}))
        .collect::<Vec<_>>())
}
fn snapshot<E: Wire, const N: usize, const M: usize>(
    f: &CowForest<Item, E, N, M>,
    root: Option<NodeHandle>,
) -> Outcome {
    let Some(root) = root else {
        return Ok(Value::Null);
    };
    // Explicit postorder also keeps the adapter bounded for the reference's
    // pathological non-root split chains. No recursive tree walk in Rust.
    let mut stack = vec![(root, false)];
    let mut results = Vec::new();
    while let Some((h, visited)) = stack.pop() {
        let n = f.node(&h).map_err(|e| e.to_string())?;
        if !visited && n.is_internal() {
            stack.push((h.clone(), true));
            for i in (0..n.count as usize).rev() {
                stack.push((
                    n.child(i)
                        .map_err(|e| e.to_string())?
                        .cloned()
                        .ok_or("missing child")?,
                    false,
                ));
            }
            continue;
        }
        let start = results
            .len()
            .checked_sub(if n.is_internal() { n.count as usize } else { 0 })
            .ok_or("invalid snapshot stack")?;
        let nested = results.split_off(start);
        let mut nested = nested.into_iter();
        let mut children = Vec::new();
        for i in 0..n.count as usize {
            let (lo, hi) = n.get_child_bounds(i).map_err(|e| e.to_string())?;
            let mut c = json!({"min":wide(&lo),"max":wide(&hi)});
            if n.is_leaf() {
                let (il, ih) = n.get_insert_bounds(i).map_err(|e| e.to_string())?;
                c["insert_min"] = json!(wide(&il));
                c["insert_max"] = json!(wide(&ih));
                c["item"] = wi(&n.data(i).map_err(|e| e.to_string())?.cloned());
            } else {
                c["child"] = nested.next().ok_or("missing snapshot child")?;
            }
            children.push(c);
        }
        results.push(json!({"level":n.level,"refs":n.refcount_load(),"children":children}));
    }
    results.pop().ok_or("missing snapshot".into())
}
fn cow<E: Wire, const N: usize, const M: usize>(steps: &[Value]) -> Outcome {
    let mut f = CowForest::<Item, E, N, M>::new(Budget {
        entries: LIMIT,
        trees: 16384,
        nodes: 1000000,
    })
    .map_err(|e| e.to_string())?;
    let mut trees: Vec<Option<TreeId>> = Vec::new();
    for _ in 0..4 {
        trees.push(Some(f.new_tree().map_err(|e| e.to_string())?));
    }
    let mut events = Vec::new();
    for s in steps {
        let op = s
            .get("op")
            .and_then(Value::as_str)
            .ok_or("expected op string")?;
        let a = number(s, "a", 0, 3)? as usize;
        if op == "new" {
            keys(s, &["op", "a"])?;
            if let Some(t) = trees[a].take() {
                f.destroy(&t).map_err(|e| e.to_string())?;
            }
            trees[a] = Some(f.new_tree().map_err(|e| e.to_string())?);
            events.push(json!({}));
            continue;
        }
        let t = trees[a].as_ref().ok_or("missing tree")?.clone();
        let e = match op {
            "insert" | "remove" => {
                keys(s, &["op", "a", "min", "max", "item"])?;
                let lo = coords(s, "min")?;
                let hi = coords(s, "max")?;
                let data = item(&s["item"])?;
                if op == "insert" {
                    f.insert(&t, lo, hi, data).map_err(|e| e.to_string())?;
                    json!({})
                } else {
                    json!({"removed":f.remove(&t,lo,hi,data).map_err(|e|e.to_string())?})
                }
            }
            "bulk" => {
                keys(s, &["op", "a", "entries"])?;
                let mut es = batch(s)?;
                f.bulk_load(&t, &mut es).map_err(|e| e.to_string())?;
                json!({"entries":entries(&es)})
            }
            "clear" => {
                keys(s, &["op", "a"])?;
                f.clear(&t).map_err(|e| e.to_string())?;
                json!({})
            }
            "destroy" => {
                keys(s, &["op", "a"])?;
                f.destroy(&t).map_err(|e| e.to_string())?;
                trees[a] = None;
                json!({})
            }
            "clone" | "move_new" | "move_assign" => {
                keys(s, &["op", "a", "src"])?;
                let src = number(s, "src", 0, 3)? as usize;
                let source = trees[src].as_ref().ok_or("missing tree")?.clone();
                if op == "move_assign" {
                    f.move_assign(&t, &source).map_err(|e| e.to_string())?;
                } else if op == "move_new" && a == src {
                    f.move_assign(&t, &source).map_err(|e| e.to_string())?;
                } else {
                    let next = if op == "clone" {
                        f.clone_tree(&source)
                    } else {
                        f.move_construct(&source)
                    }
                    .map_err(|e| e.to_string())?;
                    f.destroy(&t).map_err(|e| e.to_string())?;
                    trees[a] = Some(next);
                }
                json!({})
            }
            "state" => {
                keys(s, &["op", "a"])?;
                json!({"size":f.len(&t).map_err(|e|e.to_string())?,"empty":f.is_empty(&t).map_err(|e|e.to_string())?,"root":snapshot(&f,f.root_handle(&t).map_err(|e|e.to_string())?)?,"allocators":f.allocator_memory(&t).map_err(|e|e.to_string())?})
            }
            "iterate" => {
                keys(s, &["op", "a", "skip", "take"])?;
                let skip = number(s, "skip", 0, LIMIT as i64)?;
                let take = number(s, "take", 0, LIMIT as i64)?;
                let mut it = f.begin(&t).map_err(|e| e.to_string())?;
                let end = f.end();
                let mut same = it.clone();
                same.advance().map_err(|e| e.to_string())?;
                let equal = it == same;
                for _ in 0..skip {
                    it.advance().map_err(|e| e.to_string())?;
                }
                let mut out = Vec::new();
                for _ in 0..take {
                    if it == end {
                        break;
                    }
                    out.push(wi(it.current()));
                    it.advance().map_err(|e| e.to_string())?;
                }
                json!({"items":out,"at_end":it==end,"current":wi(it.current()),"equal_next":equal})
            }
            "search" => {
                keys(s, &["op", "a", "min", "max", "stop"])?;
                let lo = coords(s, "min")?;
                let hi = coords(s, "max")?;
                let stop = number(s, "stop", 0, LIMIT as i64)?;
                let mut out = Vec::new();
                let found = f
                    .search(&t, lo, hi, |x| {
                        out.push(wi(x));
                        (out.len() as i64) < stop
                    })
                    .map_err(|e| e.to_string())?;
                json!({"items":out,"found":found})
            }
            _ => return Err("unknown op".into()),
        };
        events.push(e);
    }
    // Exercise destructors while siblings still exist, not just arena teardown.
    for t in trees.into_iter().flatten() {
        f.destroy(&t).map_err(|e| e.to_string())?;
    }
    Ok(json!({"events":events}))
}
fn packed<E: Wire, const N: usize, const M: usize>(steps: &[Value]) -> Outcome {
    let mut trees = [
        PackedRtree::<Item, E, N, M>::new().map_err(|e| e.to_string())?,
        PackedRtree::new().map_err(|e| e.to_string())?,
        PackedRtree::new().map_err(|e| e.to_string())?,
        PackedRtree::new().map_err(|e| e.to_string())?,
    ];
    let mut builders = [
        Builder::<Item, E, N, M>::new(LIMIT).map_err(|e| e.to_string())?,
        Builder::new(LIMIT).map_err(|e| e.to_string())?,
        Builder::new(LIMIT).map_err(|e| e.to_string())?,
        Builder::new(LIMIT).map_err(|e| e.to_string())?,
    ];
    let mut events = Vec::new();
    for s in steps {
        let op = s
            .get("op")
            .and_then(Value::as_str)
            .ok_or("expected op string")?;
        let a = number(s, "a", 0, 3)? as usize;
        let e = match op {
            "reserve" => {
                keys(s, &["op", "a", "count"])?;
                let n = number(s, "count", 0, LIMIT as i64)? as usize;
                builders[a].reserve(n).map_err(|e| e.to_string())?;
                json!({})
            }
            "add" => {
                keys(s, &["op", "a", "min", "max", "item"])?;
                builders[a]
                    .add(coords(s, "min")?, coords(s, "max")?, item(&s["item"])?)
                    .map_err(|e| e.to_string())?;
                json!({})
            }
            "build" => {
                keys(s, &["op", "a"])?;
                trees[a] = builders[a].build().map_err(|e| e.to_string())?;
                json!({})
            }
            "state" => {
                keys(s, &["op", "a"])?;
                let t = &trees[a];
                let mut bounds = Vec::new();
                for d in 0..N {
                    for side in 0..2 {
                        bounds.push(
                            t.bounds_row(d, side)
                                .map_err(|e| e.to_string())?
                                .iter()
                                .map(|x| x.widen())
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                json!({"size":t.len(),"empty":t.is_empty(),"memory":t.memory_usage().map_err(|e|e.to_string())?,"bounds":bounds,"counts":t.counts(),"offsets":t.level_offsets()})
            }
            "iterate" => {
                keys(s, &["op", "a", "skip", "take"])?;
                let skip = number(s, "skip", 0, LIMIT as i64)?;
                let take = number(s, "take", 0, LIMIT as i64)?;
                let mut it = trees[a].begin();
                let end = trees[a].end();
                for _ in 0..skip {
                    it.advance().map_err(|_| "iterator outside data")?;
                }
                let mut out = Vec::new();
                for _ in 0..take {
                    if it == end {
                        break;
                    }
                    let old = it.post_advance().map_err(|_| "iterator outside data")?;
                    out.push(wi(old.current().map_err(|_| "iterator outside data")?));
                }
                let cur = if it == end {
                    Value::Null
                } else {
                    wi(it.current().map_err(|_| "iterator outside data")?)
                };
                json!({"items":out,"at_end":it==end,"current":cur})
            }
            "search" => {
                keys(s, &["op", "a", "min", "max", "stop"])?;
                let stop = number(s, "stop", 0, LIMIT as i64)?;
                let mut out = Vec::new();
                let found = trees[a]
                    .search(coords(s, "min")?, coords(s, "max")?, |x| {
                        out.push(wi(x));
                        (out.len() as i64) < stop
                    })
                    .map_err(|e| e.to_string())?;
                json!({"items":out,"found":found})
            }
            "move_new" | "move_assign" => {
                keys(s, &["op", "a", "src"])?;
                let src = number(s, "src", 0, 3)? as usize;
                if a == src {
                    trees[a].move_assign_self();
                } else {
                    let moved =
                        PackedRtree::move_construct(&mut trees[src]).map_err(|e| e.to_string())?;
                    if op == "move_new" {
                        trees[a] = moved;
                    } else {
                        let mut moved = moved;
                        trees[a]
                            .move_assign(&mut moved)
                            .map_err(|e| e.to_string())?;
                    }
                }
                json!({})
            }
            _ => return Err("unknown op".into()),
        };
        events.push(e);
    }
    Ok(json!({"events":events}))
}
pub fn run(input: &Value) -> Outcome {
    keys(input, &["tree", "config", "steps"])?;
    let config = input["config"].as_str().ok_or("expected config string")?;
    let steps = input["steps"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 4096)
        .ok_or("steps outside 1..4096")?;
    let tree = input["tree"].as_str().ok_or("expected tree string")?;
    macro_rules! go {
        ($e:ty,$n:expr,$m:expr) => {
            match tree {
                "cow" => cow::<$e, $n, $m>(steps),
                "packed" => packed::<$e, $n, $m>(steps),
                _ => Err("unknown tree".into()),
            }
        };
    }
    match config {
        "i32.d2.m4" => go!(i32, 2, 4),
        "i32.d2.m8" => go!(i32, 2, 8),
        "i32.d2.m16" => go!(i32, 2, 16),
        "i32.d2.m28" => go!(i32, 2, 28),
        "i32.d3.m5" => go!(i32, 3, 5),
        "i32.d3.m16" => go!(i32, 3, 16),
        "i32.d1.m8" => go!(i32, 1, 8),
        "i32.d1.m3" => go!(i32, 1, 3),
        "i64.d2.m16" => go!(i64, 2, 16),
        "i64.d3.m8" => go!(i64, 3, 8),
        _ => Err("unknown config".into()),
    }
}
