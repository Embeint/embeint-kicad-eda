// SPDX-License-Identifier: GPL-3.0-or-later
use super::dispatch;
use serde_json::{json, Value};

fn reply(input: Value) -> Value {
    let line = json!({"unit": super::UNIT, "case": "t", "input": input}).to_string();
    dispatch(&line)
}

#[test]
fn chain_script_and_deferral() {
    let r = reply(json!({"op": "chain", "steps": [
        {"call": "append", "a": 0, "point": [1, 2]},
        {"call": "append_xy", "a": 0, "x": 5, "y": -3},
        {"call": "bbox", "a": 0, "clearance": 3},
        {"call": "segment_count", "a": 0}]}));
    assert_eq!(r["result"]["replies"][2]["pos"], json!([-2, -6]));
    assert_eq!(r["result"]["replies"][3], json!(1));
    let r = reply(json!({"op": "chain", "steps": [
        {"call": "set_closed", "a": 0, "closed": true}]}));
    assert_eq!(r["error"]["kind"], json!("unported"));
    assert_eq!(
        r["error"]["upstream"],
        json!("SHAPE_LINE_CHAIN::mergeFirstLastPointIfNeeded")
    );
}

#[test]
fn refusals() {
    let r = reply(json!({"op": "chain", "steps": [{"call": "clast_point", "a": 0}]}));
    assert_eq!(r["error"]["message"], json!("undefined p004 operation"));
    // A defined allocation above the harness budget is a harness limit, not a domain refusal.
    let r = reply(json!({"op": "chain", "steps": [
        {"call": "reserve_points", "a": 0, "size": 1u64 << 30}]}));
    assert_eq!(
        r["error"]["message"],
        json!("harness allocation budget exceeded")
    );
    let r = reply(json!({"op": "chain", "steps": [
        {"call": "reserve_points", "a": 0, "size": 1_048_577u64},
        {"call": "point_count", "a": 0}]}));
    assert_eq!(r["result"]["replies"], json!([null, 0]));
    let r = reply(json!({"op": "chain", "steps": [
        {"call": "reserve_points", "a": 0, "size": 1u64 << 60}]}));
    assert_eq!(r["result"]["replies"][0], json!({"threw": "length_error"}));
}

#[test]
fn index_records_assertions() {
    let r = reply(
        json!({"op": "index", "items": [{"shape": null, "layers": []}], "steps": [
        {"call": "new", "a": 0, "layer": 0},
        {"call": "add", "a": 0, "item": null},
        {"call": "add", "a": 0, "item": 0},
        {"call": "size", "a": 0}]}),
    );
    assert_eq!(r["result"]["replies"][3], json!(2));
    assert_eq!(r["result"]["assertions"].as_array().map(Vec::len), Some(3));
}

#[test]
fn defined_tree_and_list_identity() {
    // Review reproductions: a first insertion beyond 2^29 is defined; iterators of two lists
    // differ.
    let r = reply(json!({"op": "index", "items": [], "steps": [
        {"call": "new", "a": 0, "layer": 0},
        {"call": "add_bbox", "a": 0, "item": null, "pos": [536_870_913, 0], "size": [0, 0]},
        {"call": "size", "a": 0},
        {"call": "range", "a": 0}]}));
    assert_eq!(r["result"]["replies"], json!([null, null, 1, [null]]));
    let item = json!({"shape": {"kind": "circle", "center": [0, 0], "radius": 1}, "layers": [-1]});
    let r = reply(json!({"op": "index_list", "items": [item], "steps": [
        {"call": "add", "item": 0},
        {"call": "add", "item": 0, "l": 1},
        {"call": "compare", "kind": "iterator", "left": {"l": 0, "at": "begin"},
         "right": {"l": 1, "at": "begin"}},
        {"call": "compare", "kind": "query", "left": {"l": 0, "at": "begin"},
         "right": {"l": 1, "at": "begin"}}]}));
    assert_eq!(r["result"]["replies"][2], json!({"eq": false, "ne": true}));
    assert_eq!(r["result"]["replies"][3], json!({"eq": false, "ne": true}));
}
