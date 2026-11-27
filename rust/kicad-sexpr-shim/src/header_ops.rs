// SPDX-License-Identifier: GPL-3.0-or-later
//! Operations of `libs.sexpr.include.sexpr.application.pure.p001` (the headers).
//!
//! - `predicates {tree}`: every node's five `Is*` predicates and `GetLineNumber`, and the
//!   tree as decoded through the atom and list constructors.
//! - `construct_list {ctor: "default"|"line"|"variadic", line?, args?}`: `SEXPR_LIST()`,
//!   `SEXPR_LIST( int )` and the variadic template constructor.
//! - `exception {class: "PARSE_EXCEPTION"|"INVALID_TYPE_EXCEPTION", message}`: `what()` of
//!   the object, and of it thrown and caught as `std::exception`.
//! - `isexprable {tree}`: the default `SerializeSEXPR` and `DeserializeSEXPR` bodies.
//! - `out_string {which: "symbol"|"string", value}` and `in_string {which, value, assign}`:
//!   `AsSymbol`/`AsString` on a const and a non-const string.
//! - `scan {tree, args}` and `add_children {tree, args}`: the `Scan` and `AddChildren`
//!   templates on a list.

use crate::args::{read_children_args, read_scan_args, scan_results, with_scan_args};
use crate::codec::{
    dump, get, hex, keys, predicates, raised, read_hex, read_i32, read_list, read_tree, text, Error,
};
use kicad_sexpr::{
    as_string, as_string_in, as_symbol, as_symbol_in, DefaultIsexprable, InvalidTypeException,
    Isexprable, Node, ParseException, SexprList, StdException,
};
use serde_json::{json, Map, Value};

pub fn run(op: &str, input: &Map<String, Value>) -> Result<Value, Error> {
    match op {
        "predicates" => {
            keys(input, &["op", "tree"])?;
            let tree = read_tree(get(input, "tree")?)?;
            Ok(json!({"predicates": predicates(tree.view()), "tree": dump(tree.view())}))
        }
        "construct_list" => construct_list(input),
        "exception" => {
            keys(input, &["op", "class", "message"])?;
            let message = read_hex(get(input, "message")?)?;
            let what = match text(input, "class")? {
                "PARSE_EXCEPTION" => ParseException::new(&message).what().to_vec(),
                "INVALID_TYPE_EXCEPTION" => InvalidTypeException::new(&message).what().to_vec(),
                _ => return Err("unknown exception class".into()),
            };
            // Thrown and caught as std::exception, what() dispatches to the same override.
            Ok(json!({"what": hex(&what), "what_via_std_exception": hex(&what)}))
        }
        "isexprable" => {
            keys(input, &["op", "tree"])?;
            let mut tree = read_tree(get(input, "tree")?)?;
            let mut object = DefaultIsexprable;
            let serialized = object.serialize_sexpr();
            object.deserialize_sexpr(&mut tree);
            Ok(json!({
                "serialized": serialized.map(|node| dump(node.view())),
                "tree": dump(tree.view())
            }))
        }
        "out_string" => {
            keys(input, &["op", "which", "value"])?;
            let value = read_hex(get(input, "value")?)?;
            let out = match text(input, "which")? {
                "symbol" => as_symbol(&value),
                "string" => as_string(&value),
                _ => return Err("unknown wrapper".into()),
            };
            Ok(json!({
                "same_object": core::ptr::eq(out.string, value.as_slice()),
                "string": hex(out.string),
                "symbol": out.symbol
            }))
        }
        "in_string" => {
            keys(input, &["op", "which", "value", "assign"])?;
            let mut value = read_hex(get(input, "value")?)?;
            let assign = read_hex(get(input, "assign")?)?;
            let original: *const Vec<u8> = &value;
            let wrapper = match text(input, "which")? {
                "symbol" => as_symbol_in(&mut value),
                "string" => as_string_in(&mut value),
                _ => return Err("unknown wrapper".into()),
            };
            let symbol = wrapper.symbol;
            let same_object = core::ptr::eq(&*wrapper.string, original);
            *wrapper.string = assign;
            Ok(json!({"after": hex(&value), "same_object": same_object, "symbol": symbol}))
        }
        "scan" => scan(input),
        "add_children" => add_children(input),
        _ => Err("unknown operation".into()),
    }
}

pub fn construct_list(input: &Map<String, Value>) -> Result<Value, Error> {
    let list = match text(input, "ctor")? {
        "default" => {
            keys(input, &["op", "ctor"])?;
            SexprList::new()
        }
        "line" => {
            keys(input, &["op", "ctor", "line"])?;
            SexprList::with_line_number(read_i32(get(input, "line")?)?)
        }
        "variadic" => {
            keys(input, &["op", "ctor", "args"])?;
            let args = read_children_args(get(input, "args")?)?;
            match SexprList::from_args(args) {
                Ok(list) => list,
                Err(error) => return raised(error),
            }
        }
        _ => return Err("unknown constructor".into()),
    };
    Ok(json!({"in_stream_child": list.in_stream_child(), "tree": dump(list.view())}))
}

/// `Scan( args... )` on the list root; the returned count and every destination after it.
pub fn scan(input: &Map<String, Value>) -> Result<Value, Error> {
    keys(input, &["op", "tree", "args"])?;
    let mut list = read_list(get(input, "tree")?)?;
    let mut stores = read_scan_args(get(input, "args")?)?;
    match with_scan_args(&mut stores, |args| list.scan(args))? {
        Ok(returned) => Ok(json!({"args": scan_results(&stores), "returned": returned})),
        Err(error) => raised(error),
    }
}

/// `AddChildren( args... )` on the list root; the tree after it.
pub fn add_children(input: &Map<String, Value>) -> Result<Value, Error> {
    keys(input, &["op", "tree", "args"])?;
    let mut list = read_list(get(input, "tree")?)?;
    let args = read_children_args(get(input, "args")?)?;
    match list.add_children(args) {
        Ok(()) => Ok(json!({"in_stream_child": list.in_stream_child(), "tree": dump(list.view())})),
        Err(error) => raised(error),
    }
}
