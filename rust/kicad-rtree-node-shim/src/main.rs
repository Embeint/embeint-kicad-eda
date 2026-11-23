// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `libs.kimath.include.geometry.rtree.application.pure.p003`.
//!
//! It accepts exactly the request schema of `port/oracle/shims/rtree_node.cpp`,
//! validates fields in the same order with the same messages, and serialises the
//! same state.  Nodes are numbered by first allocation and items by their index.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kicad_rtree_node::{
    hilbert_nd2d, hilbert_xy2d, overlap_mask_2d, Coord, NodeData, NodeHandle, Payload, RtreeError,
    RtreeNode, SlabAllocator, SlabNode, NODES_PER_PAGE,
};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

const UNIT: &str = "libs.kimath.include.geometry.rtree.application.pure.p003";
const MAX_STEPS: usize = 4096;
const MAX_ALLOCATIONS: usize = 4096;
const ITEMS: i64 = 64;
const ALLOCATORS: usize = 4;
const DIMS: [i64; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 16, 21, 22, 32, 33, 64, 65];

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

fn hilbert_xy(input: &Value) -> Outcome {
    keys(input, &["kind", "order", "x", "y"])?;
    let order = int(input, "order")?;
    let x = signed(input, "x", 0, u32::MAX.into())?;
    let y = signed(input, "y", 0, u32::MAX.into())?;
    if !(1..=32).contains(&order) {
        return Err(RtreeError::OrderOutOfDomain.to_string());
    }
    let narrow = |value: i64, name: &str| {
        u32::try_from(value).map_err(|_| format!("integer out of range: {name}"))
    };
    let index = hilbert_xy2d(order, narrow(x, "x")?, narrow(y, "y")?).map_err(|e| e.to_string())?;
    Ok(json!({"index": index}))
}

fn hilbert_nd(input: &Value) -> Outcome {
    keys(input, &["kind", "dims", "order", "coords"])?;
    let dims = signed(input, "dims", 1, 1024)?;
    let order = int(input, "order")?;
    let length = usize::try_from(dims).map_err(|_| "integer out of range: dims")?;
    let coords = values::<u32>(input, "coords", length)?;
    // Same domain as the oracle adapter: see hilbert_nd2d for the source order.
    let morton_shifts = dims <= 64 && order >= 1;
    if (dims == 2 && !(1..=32).contains(&order))
        || (dims != 2 && (order == i32::MIN || (morton_shifts && order > 32)))
    {
        return Err(RtreeError::OrderOutOfDomain.to_string());
    }
    if !DIMS.contains(&dims) {
        return Err("unsupported dimensions".into());
    }
    let index = hilbert_nd2d(order, &coords).map_err(|e| e.to_string())?;
    Ok(json!({"index": index}))
}

fn mask_case<E: Coord + Wire, const FANOUT: usize>(input: &Value, count: i32) -> Outcome {
    let min0 = values::<E>(input, "min0", FANOUT)?;
    let max0 = values::<E>(input, "max0", FANOUT)?;
    let min1 = values::<E>(input, "min1", FANOUT)?;
    let max1 = values::<E>(input, "max1", FANOUT)?;
    let query_min = fixed::<E, 2>(input, "qmin")?;
    let query_max = fixed::<E, 2>(input, "qmax")?;
    if !(0..=31).contains(&count) {
        return Err(RtreeError::CountOutOfDomain.to_string());
    }
    let mask =
        overlap_mask_2d::<E, FANOUT>(&min0, &max0, &min1, &max1, count, &query_min, &query_max)
            .map_err(|e| e.to_string())?;
    Ok(json!({"mask": mask}))
}

fn mask_fanout<E: Coord + Wire>(input: &Value, fanout: i64, count: i32) -> Outcome {
    match fanout {
        4 => mask_case::<E, 4>(input, count),
        8 => mask_case::<E, 8>(input, count),
        12 => mask_case::<E, 12>(input, count),
        16 => mask_case::<E, 16>(input, count),
        20 => mask_case::<E, 20>(input, count),
        24 => mask_case::<E, 24>(input, count),
        28 => mask_case::<E, 28>(input, count),
        _ => Err("unsupported fanout".into()),
    }
}

fn overlap_mask(input: &Value) -> Outcome {
    keys(
        input,
        &[
            "kind", "elem", "fanout", "count", "min0", "max0", "min1", "max1", "qmin", "qmax",
        ],
    )?;
    let elem = input.get("elem").and_then(Value::as_str);
    let fanout = signed(input, "fanout", 0, 1024)?;
    let count = int(input, "count")?;
    match elem {
        Some("i32") => mask_fanout::<i32>(input, fanout, count),
        Some("i64") => mask_fanout::<i64>(input, fanout, count),
        _ => Err("unknown element type".into()),
    }
}

/// The leaf `DATATYPE`: a pointer to one of the request's 64 items.
#[derive(Clone, Debug, PartialEq)]
struct Item(i64);

impl NodeData for Item {}

type Node<E, const D: usize, const M: usize> = RtreeNode<Item, E, D, M>;

/// A numbered node; `destroyed` once a move assignment destroyed its page.  The
/// handle is kept: a live node may retain it in a slot, and handles never alias.
struct Seen {
    handle: NodeHandle,
    destroyed: bool,
}

struct Program<E: Coord, const D: usize, const M: usize> {
    allocators: [Option<SlabAllocator<Node<E, D, M>>>; ALLOCATORS],
    /// Node numbering by first allocation.
    seen: Vec<Seen>,
    allocated: usize,
}

fn lib(error: RtreeError) -> String {
    error.to_string()
}

impl<E: Coord + Wire + Into<Value>, const D: usize, const M: usize> Program<E, D, M> {
    fn new() -> Self {
        Self {
            allocators: core::array::from_fn(|_| None),
            seen: Vec::new(),
            allocated: 0,
        }
    }

    fn run(&mut self, steps: &[Value]) -> Outcome {
        let mut events = Vec::new();
        for step in steps {
            let op = step
                .as_object()
                .and_then(|object| object.get("op"))
                .and_then(Value::as_str)
                .ok_or("expected op string")?;
            events.push(self.step(op, step)?);
        }
        Ok(json!({"events": events}))
    }

    fn allocator_id(step: &Value, name: &str) -> Result<usize, String> {
        let id = signed(step, name, 0, ALLOCATORS as i64 - 1)?;
        usize::try_from(id).map_err(|_| format!("integer out of range: {name}"))
    }

    fn allocator(&mut self, id: usize) -> Result<&mut SlabAllocator<Node<E, D, M>>, String> {
        self.allocators
            .get_mut(id)
            .and_then(Option::as_mut)
            .ok_or_else(|| "allocator does not exist".into())
    }

    fn node(&self, step: &Value, name: &str) -> Result<NodeHandle, String> {
        let id = signed(step, name, 0, MAX_ALLOCATIONS as i64 - 1)?;
        let seen = usize::try_from(id)
            .ok()
            .and_then(|id| self.seen.get(id))
            .ok_or("node does not exist")?;
        if seen.destroyed {
            return Err("node is destroyed".into());
        }
        Ok(seen.handle.clone())
    }

    fn optional_node(&self, step: &Value, name: &str) -> Result<Option<NodeHandle>, String> {
        if step.get(name).is_some_and(Value::is_null) {
            return Ok(None);
        }
        self.node(step, name).map(Some)
    }

    fn id(&mut self, node: &NodeHandle) -> usize {
        if let Some(index) = self
            .seen
            .iter()
            .position(|seen| !seen.destroyed && seen.handle == *node)
        {
            return index;
        }
        self.seen.push(Seen {
            handle: node.clone(),
            destroyed: false,
        });
        self.seen.len() - 1
    }

    /// A child slot as `{"child":n}`, or `{"destroyed":n}` for a retained pointer to
    /// a node whose page a move assignment destroyed.
    fn child_value(&self, node: &NodeHandle) -> Outcome {
        let (id, seen) = self
            .seen
            .iter()
            .enumerate()
            .find(|(_, seen)| seen.handle == *node)
            .ok_or("unknown pointer in node")?;
        Ok(if seen.destroyed {
            json!({"destroyed": id})
        } else {
            json!({"child": id})
        })
    }

    /// The live node, found in its owning allocator (no use after free).
    fn live(&mut self, node: &NodeHandle) -> Result<&mut Node<E, D, M>, String> {
        let not_live = || "node is not live".to_string();
        let owner = self
            .allocators
            .iter_mut()
            .flatten()
            .find(|allocator| allocator.owns(node))
            .ok_or_else(not_live)?;
        owner.node_mut(node).map_err(|_| not_live())
    }

    /// Move assignment destroys the target's pages: retire their nodes from the
    /// numbering.  Live nodes elsewhere may keep their handles in slots; that is
    /// defined as long as nothing dereferences them.
    fn retire(&mut self, target: usize) -> Result<(), String> {
        let allocator = self.allocators[target]
            .as_ref()
            .ok_or("allocator does not exist")?;
        for seen in &mut self.seen {
            if !seen.destroyed && allocator.owns(&seen.handle) {
                seen.destroyed = true;
            }
        }
        Ok(())
    }

    fn slot_domain(slot: i32) -> Result<usize, String> {
        usize::try_from(slot)
            .ok()
            .filter(|slot| *slot < M)
            .ok_or_else(|| lib(RtreeError::SlotOutOfRange))
    }

    fn count_domain(node: &Node<E, D, M>) -> Result<(), String> {
        if i64::from(node.count) > M as i64 {
            return Err(lib(RtreeError::CountOutOfDomain));
        }
        Ok(())
    }

    fn query(step: &Value) -> Result<([E; D], [E; D]), String> {
        Ok((fixed::<E, D>(step, "min")?, fixed::<E, D>(step, "max")?))
    }

    fn boxed(min: [E; D], max: [E; D]) -> Value {
        let list = |values: [E; D]| Value::from(values.map(Into::into).to_vec());
        json!({"min": list(min), "max": list(max)})
    }

    fn snapshot(&self, node: &Node<E, D, M>) -> Outcome {
        let rows = |inserted: bool| -> Result<Vec<Value>, String> {
            (0..D * 2)
                .map(|row| {
                    (0..M)
                        .map(|slot| {
                            if inserted {
                                node.insert_bound(row, slot)
                            } else {
                                node.bound(row, slot)
                            }
                            .map(Into::into)
                            .map_err(lib)
                        })
                        .collect::<Result<Vec<Value>, String>>()
                        .map(Value::from)
                })
                .collect()
        };
        let mut payload = Vec::new();
        for slot in 0..M {
            payload.push(match node.payload(slot).map_err(lib)? {
                Payload::Null => Value::Null,
                Payload::Data(Item(item)) => json!({"item": item}),
                Payload::Child(child) => self.child_value(child)?,
            });
        }
        Ok(
            json!({"count": node.count, "level": node.level, "bounds": rows(false)?,
            "insert": rows(true)?, "payload": payload, "refcount": node.refcount_load()}),
        )
    }

    fn state(&mut self, id: usize) -> Outcome {
        let allocator = self.allocator(id)?;
        let pages = (0..allocator.page_count())
            .map(|page| allocator.page_used_words(page).map(|words| words.to_vec()))
            .collect::<Option<Vec<_>>>()
            .ok_or("page missing")?;
        let free_list = allocator.free_list().to_vec();
        let capacity = allocator.free_list_capacity();
        let memory = allocator.memory_usage().map_err(lib)?;
        let free = free_list
            .iter()
            .map(|node| self.id(node))
            .collect::<Vec<_>>();
        Ok(json!({"pages": pages, "free": free, "capacity": capacity, "memory": memory}))
    }

    fn step(&mut self, op: &str, step: &Value) -> Outcome {
        let empty = || Ok(Value::Object(Map::new()));
        match op {
            "layout" => {
                keys(step, &["op"])?;
                let node = Node::<E, D, M>::reference_layout().map_err(lib)?;
                let page = SlabAllocator::<Node<E, D, M>>::reference_page_layout().map_err(lib)?;
                Ok(json!({"node_size": node.size, "node_align": node.align,
                    "page_size": page.size, "page_align": page.align,
                    "nodes_per_page": NODES_PER_PAGE, "pointer_size": 8,
                    "min_nodes": Node::<E, D, M>::MIN_NODES}))
            }
            "new" | "move_new" | "move_assign" => {
                if op == "new" {
                    keys(step, &["op", "a"])?;
                } else {
                    keys(step, &["op", "a", "src"])?;
                }
                let id = Self::allocator_id(step, "a")?;
                let source = if op == "new" {
                    0
                } else {
                    Self::allocator_id(step, "src")?
                };
                if op == "move_assign" {
                    self.allocator(id)?;
                    self.allocator(source)?;
                    // The reference's operator= ignores a self move.
                    if id != source {
                        self.retire(id)?;
                        let mut moved = self.allocators[source]
                            .take()
                            .ok_or("allocator does not exist")?;
                        self.allocator(id)?.move_assign(&mut moved);
                        self.allocators[source] = Some(moved);
                    }
                    return empty();
                }
                if self.allocators[id].is_some() {
                    return Err("allocator exists".into());
                }
                self.allocators[id] = Some(if op == "new" {
                    SlabAllocator::new()
                } else {
                    self.allocator(source)?.take()
                });
                empty()
            }
            "alloc" | "alloc_many" => {
                if op == "alloc" {
                    keys(step, &["op", "a"])?;
                } else {
                    keys(step, &["op", "a", "n"])?;
                }
                let id = Self::allocator_id(step, "a")?;
                let count = if op == "alloc" {
                    1
                } else {
                    usize::try_from(signed(step, "n", 1, 1024)?)
                        .map_err(|_| "integer out of range: n")?
                };
                self.allocator(id)?;
                if self.allocated + count > MAX_ALLOCATIONS {
                    return Err("allocation budget exceeded".into());
                }
                let mut nodes = Vec::new();
                for _ in 0..count {
                    let node = self.allocator(id)?.allocate().map_err(lib)?;
                    nodes.push(self.id(&node));
                    self.allocated += 1;
                }
                if op == "alloc" {
                    return Ok(json!({"node": nodes.first()}));
                }
                Ok(json!({"nodes": nodes}))
            }
            "free" | "owns" | "find" => {
                keys(step, &["op", "a", "node"])?;
                let id = Self::allocator_id(step, "a")?;
                let node = if op == "free" {
                    self.optional_node(step, "node")?
                } else {
                    Some(self.node(step, "node")?)
                };
                let allocator = self.allocator(id)?;
                match (op, node) {
                    ("owns", Some(node)) => Ok(json!({"owns": allocator.owns(&node)})),
                    ("find", Some(node)) => Ok(match allocator.find_owning_page(&node) {
                        Some((page, offset)) => json!({"page": page, "offset": offset}),
                        None => json!({"page": null, "offset": null}),
                    }),
                    (_, node) => {
                        allocator.free(node.as_ref()).map_err(lib)?;
                        empty()
                    }
                }
            }
            "memory" | "state" => {
                keys(step, &["op", "a"])?;
                let id = Self::allocator_id(step, "a")?;
                if op == "memory" {
                    let memory = self.allocator(id)?.memory_usage().map_err(lib)?;
                    return Ok(json!({"memory": memory}));
                }
                self.state(id)
            }
            _ => self.node_step(op, step),
        }
    }

    fn node_step(&mut self, op: &str, step: &Value) -> Outcome {
        let empty = || Ok(Value::Object(Map::new()));
        match op {
            "set_count" | "set_level" | "ref_add" | "ref_sub" | "ref_store" => {
                keys(step, &["op", "node", "value"])?;
                let handle = self.node(step, "node")?;
                let value = int(step, "value")?;
                let node = self.live(&handle)?;
                match op {
                    "set_count" => node.count = value,
                    "set_level" => node.level = value,
                    "ref_add" => return Ok(json!({"previous": node.refcount_fetch_add(value)})),
                    "ref_sub" => return Ok(json!({"previous": node.refcount_fetch_sub(value)})),
                    _ => node.refcount_store(value),
                }
                empty()
            }
            "ref_load" | "predicates" | "snapshot" | "enclosing" => {
                keys(step, &["op", "node"])?;
                let handle = self.node(step, "node")?;
                let node = self.live(&handle)?;
                match op {
                    "ref_load" => Ok(json!({"value": node.refcount_load()})),
                    "predicates" => Ok(json!({"leaf": node.is_leaf(),
                        "internal": node.is_internal(), "full": node.is_full(),
                        "underflow": node.is_underflow()})),
                    "snapshot" => {
                        // Re-borrow immutably so node ids can be looked up.
                        let owner = self
                            .allocators
                            .iter()
                            .flatten()
                            .find(|allocator| allocator.owns(&handle))
                            .ok_or("node is not live")?;
                        let node = owner.node(&handle).map_err(lib)?;
                        self.snapshot(node)
                    }
                    _ => {
                        Self::count_domain(node)?;
                        let (min, max) = node.compute_enclosing_bounds().map_err(lib)?;
                        Ok(Self::boxed(min, max))
                    }
                }
            }
            "mask" => {
                keys(step, &["op", "node", "min", "max"])?;
                let handle = self.node(step, "node")?;
                let (min, max) = Self::query(step)?;
                let node = self.live(&handle)?;
                if D == 2 {
                    if !(0..=31).contains(&node.count) {
                        return Err(lib(RtreeError::CountOutOfDomain));
                    }
                } else {
                    // An overlapping slot from 32 upwards is refused by the library.
                    Self::count_domain(node)?;
                }
                let mask = node.child_overlap_mask(&min, &max).map_err(lib)?;
                Ok(json!({"value": mask}))
            }
            "set_bounds" | "set_insert" | "overlaps" | "overlap_area" | "enlargement" => {
                keys(step, &["op", "node", "slot", "min", "max"])?;
                let handle = self.node(step, "node")?;
                let slot = int(step, "slot")?;
                let (min, max) = Self::query(step)?;
                let node = self.live(&handle)?;
                let slot = Self::slot_domain(slot)?;
                match op {
                    "set_bounds" => node.set_child_bounds(slot, &min, &max).map_err(lib)?,
                    "set_insert" => node.set_insert_bounds(slot, &min, &max).map_err(lib)?,
                    "overlaps" => {
                        let value = node.child_overlaps(slot, &min, &max).map_err(lib)?;
                        return Ok(json!({"value": value}));
                    }
                    "overlap_area" => {
                        let value = node.child_overlap_area(slot, &min, &max).map_err(lib)?;
                        return Ok(json!({"value": value}));
                    }
                    _ => {
                        let value = node.child_enlargement(slot, &min, &max).map_err(lib)?;
                        return Ok(json!({"value": value}));
                    }
                }
                empty()
            }
            "get_bounds" | "get_insert" | "area" | "perimeter" | "remove" => {
                keys(step, &["op", "node", "slot"])?;
                let handle = self.node(step, "node")?;
                let slot = int(step, "slot")?;
                let node = self.live(&handle)?;
                if op == "remove" {
                    // The library applies RemoveChild's precondition and array bounds
                    // in the oracle adapter's order.
                    node.remove_child(slot).map_err(lib)?;
                    return empty();
                }
                let index = Self::slot_domain(slot)?;
                match op {
                    "area" => Ok(json!({"value": node.child_area(index).map_err(lib)?})),
                    "perimeter" => Ok(json!({"value": node.child_perimeter(index).map_err(lib)?})),
                    "get_bounds" => {
                        let (min, max) = node.get_child_bounds(index).map_err(lib)?;
                        Ok(Self::boxed(min, max))
                    }
                    _ => {
                        let (min, max) = node.get_insert_bounds(index).map_err(lib)?;
                        Ok(Self::boxed(min, max))
                    }
                }
            }
            "set_child" | "set_data" => {
                let field = if op == "set_child" { "child" } else { "item" };
                keys(step, &["op", "node", "slot", field])?;
                let handle = self.node(step, "node")?;
                let slot = int(step, "slot")?;
                let child = if op == "set_child" {
                    self.optional_node(step, "child")?
                } else {
                    None
                };
                let item = if op == "set_data" && !step.get("item").is_some_and(Value::is_null) {
                    Some(Item(signed(step, "item", 0, ITEMS - 1)?))
                } else {
                    None
                };
                let node = self.live(&handle)?;
                let slot = Self::slot_domain(slot)?;
                if op == "set_child" {
                    node.set_child(slot, child).map_err(lib)?;
                } else {
                    node.set_data(slot, item).map_err(lib)?;
                }
                empty()
            }
            _ => Err("unknown operation".into()),
        }
    }
}

fn run_slab<E: Coord + Wire + Into<Value>, const D: usize, const M: usize>(
    steps: &[Value],
) -> Outcome {
    Program::<E, D, M>::new().run(steps)
}

fn slab(input: &Value) -> Outcome {
    keys(input, &["kind", "config", "steps"])?;
    let config = input
        .get("config")
        .and_then(Value::as_str)
        .ok_or("unknown config")?;
    const CONFIGS: [&str; 10] = [
        "i32.d2.m16",
        "i32.d2.m4",
        "i32.d2.m28",
        "i32.d3.m16",
        "i32.d3.m5",
        "i32.d3.m32",
        "i32.d3.m33",
        "i32.d1.m8",
        "i64.d2.m16",
        "i64.d3.m8",
    ];
    if !CONFIGS.contains(&config) {
        return Err("unknown config".into());
    }
    let steps = input
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("expected steps array")?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err("steps outside 1..4096".into());
    }
    match config {
        "i32.d2.m16" => run_slab::<i32, 2, 16>(steps),
        "i32.d2.m4" => run_slab::<i32, 2, 4>(steps),
        "i32.d2.m28" => run_slab::<i32, 2, 28>(steps),
        "i32.d3.m16" => run_slab::<i32, 3, 16>(steps),
        "i32.d3.m5" => run_slab::<i32, 3, 5>(steps),
        "i32.d3.m32" => run_slab::<i32, 3, 32>(steps),
        "i32.d3.m33" => run_slab::<i32, 3, 33>(steps),
        "i32.d1.m8" => run_slab::<i32, 1, 8>(steps),
        "i64.d2.m16" => run_slab::<i64, 2, 16>(steps),
        _ => run_slab::<i64, 3, 8>(steps),
    }
}

fn run(input: &Value) -> Outcome {
    let kind = input
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("expected kind string")?;
    match kind {
        "hilbert_xy" => hilbert_xy(input),
        "hilbert_nd" => hilbert_nd(input),
        "overlap_mask" => overlap_mask(input),
        "slab" => slab(input),
        _ => Err("unknown kind".into()),
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
