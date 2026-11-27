// SPDX-License-Identifier: GPL-3.0-or-later
//! Operations of `libs.sexpr.application.pure.p001` (`sexpr.cpp`).
//!
//! - `getter {tree, path, method, ...}`: one `SEXPR` member on the node reached from the root
//!   through `m_children` indices: `GetChildren`, `GetChild {index}`, `GetNumberOfChildren`,
//!   `GetLongInteger`, `GetInteger`, `GetFloat`, `GetDouble`, `GetString`, `GetSymbol`,
//!   `GetList`, `AsString {level}` (null: the default argument) and `AddChild {child}`.
//! - `insert {tree, items}`: `list << item << ...` with every `operator<<` overload.
//! - `extract {tree, targets}`: `list >> target >> ...` with every `operator>>` overload; the
//!   chain stops at the first exception, which is reported with every target's value.
//! - `scan`, `add_children` and `construct_list` as in the header unit, reaching `doScan` and
//!   `doAddChildren`.
//!
//! `AsString` requests are bounded identically in the reference adapter: in the order
//! `AsString` opens nodes, an indentation that `append` refuses (`length_error`) ends the
//! check; before that, an indentation above [`MAX_INDENT`] bytes, an output above
//! [`MAX_OUTPUT`] bytes or a copy work above [`MAX_WORK`] bytes, both counted from the exact
//! bytes each node writes, is refused as a resource bound. The reference builds each list's
//! text in its own frame and appends it to every enclosing frame, so a node's bytes are copied
//! once per frame from it to the root; a deep chain is quadratic in output and cubic in work.

use crate::codec::{
    bits32, bits64, dump, exception, get, hex, keys, object, raised, read_bool, read_f32, read_f64,
    read_hex, read_i32, read_i64, read_list, read_optional_tree, read_tree, read_u64, text, Error,
    MAX_ARGS, MAX_DEPTH,
};
use kicad_sexpr::sexpr_cpp::STRING_MAX_SIZE;
use kicad_sexpr::{
    as_string, as_string_in, as_symbol, as_symbol_in, DefaultIsexprable, Node, Sexpr, SexprList,
    SexprOps, SexprVector, View,
};
use serde_json::{json, Map, Value};

/// The largest `AsString` indentation of one list the adapters attempt.
pub const MAX_INDENT: u64 = 1 << 20;
/// The largest `AsString` output the adapters attempt.
pub const MAX_OUTPUT: u64 = 64 << 20;
/// The largest `AsString` copy work the adapters attempt (each byte times the frames that
/// hold it: the sum of every frame's result length).
pub const MAX_WORK: u64 = 1 << 30;
const TOO_LARGE: &str = "as_string output too large";

pub fn run(op: &str, input: &Map<String, Value>) -> Result<Value, Error> {
    match op {
        "getter" => getter(input),
        "insert" => insert(input),
        "extract" => extract(input),
        "scan" => crate::header_ops::scan(input),
        "add_children" => crate::header_ops::add_children(input),
        "construct_list" => crate::header_ops::construct_list(input),
        _ => Err("unknown operation".into()),
    }
}

fn read_path(value: &Value) -> Result<Vec<u64>, Error> {
    let steps = value.as_array().ok_or("invalid path")?;
    if steps.len() > MAX_DEPTH {
        return Err("invalid path".into());
    }
    steps
        .iter()
        .map(|step| read_u64(step).map_err(|_| "invalid path".into()))
        .collect()
}

/// The node reached through `m_children`: every step a list, an index in range and a
/// non-null child.
fn navigate<'a>(root: &'a mut Sexpr, path: &[u64]) -> Result<&'a mut Sexpr, Error> {
    let mut node = root;
    for &step in path {
        let Sexpr::List(list) = node else {
            return Err("invalid path".into());
        };
        node = usize::try_from(step)
            .ok()
            .and_then(|index| list.children.get_mut(index))
            .and_then(Option::as_deref_mut)
            .ok_or("invalid path")?;
    }
    Ok(node)
}

/// Every node `AsString( level )` opens, in order, with its level and its depth below the
/// node `AsString` is called on (that node is at depth 1), until a refused indentation or a
/// null child (both left to the library). Each node's own bytes are the exact bytes its frame
/// writes besides its children's text: a list's newline (at a nonzero level), indentation,
/// parentheses and the spaces between its children; an atom's formatted text (a string with
/// its quotes, a symbol as it is, an integer or double as the library formats it). They count
/// once towards the output and once per frame that holds them (the node's depth) towards the
/// work, so the work is the sum of every frame's result length.
fn check_as_string(root: View<'_>, level: u64) -> Result<(), Error> {
    let (mut total, mut work): (u64, u64) = (0, 0);
    let mut stack = vec![(Some(root), level, 1u64)];
    while let Some((node, at, depth)) = stack.pop() {
        let bytes = match node {
            None => return Ok(()),
            Some(View::List(list)) => {
                let local = u64::from(at != 0);
                let indent = at.wrapping_mul(2);
                if STRING_MAX_SIZE - local < indent {
                    return Ok(());
                }
                if indent > MAX_INDENT {
                    return Err(TOO_LARGE.into());
                }
                for child in list.children.iter().rev() {
                    stack.push((
                        child.as_deref().map(Node::view),
                        at.wrapping_add(1),
                        depth.saturating_add(1),
                    ));
                }
                let separators = (list.children.len() as u64).saturating_sub(1);
                local + indent + 2 + separators
            }
            Some(View::String(atom)) => atom.value.len() as u64 + 2,
            Some(View::Symbol(atom)) => atom.value.len() as u64,
            // A formatting failure (bad_alloc) is left to the library, which raises it too.
            Some(View::Integer(atom)) => match atom.as_string(0) {
                Ok(text) => text.len() as u64,
                Err(_) => return Ok(()),
            },
            Some(View::Double(atom)) => match atom.as_string(0) {
                Ok(text) => text.len() as u64,
                Err(_) => return Ok(()),
            },
        };
        total = total.saturating_add(bytes);
        work = work.saturating_add(bytes.saturating_mul(depth));
        if total > MAX_OUTPUT || work > MAX_WORK {
            return Err(TOO_LARGE.into());
        }
    }
    Ok(())
}

fn getter(input: &Map<String, Value>) -> Result<Value, Error> {
    let method = text(input, "method")?;
    let extra: &[&str] = match method {
        "GetChild" => &["index"],
        "AsString" => &["level"],
        "AddChild" => &["child"],
        "GetChildren"
        | "GetNumberOfChildren"
        | "GetLongInteger"
        | "GetInteger"
        | "GetFloat"
        | "GetDouble"
        | "GetString"
        | "GetSymbol"
        | "GetList" => &[],
        _ => return Err("unknown method".into()),
    };
    let mut fields = vec!["op", "tree", "path", "method"];
    fields.extend_from_slice(extra);
    keys(input, &fields)?;
    let mut tree = read_tree(get(input, "tree")?)?;
    let path = read_path(get(input, "path")?)?;
    let node = navigate(&mut tree, &path)?;
    // The list's own m_children, for the identity of what GetChildren/GetChild return.
    let own_children: Option<&SexprVector> = match &*node {
        Sexpr::List(list) => Some(&list.children),
        _ => None,
    };
    let own_children = own_children.map(|children| children as *const SexprVector);
    let outcome = match method {
        "GetChildren" => node.get_children().map(|children| {
            let trees: Vec<Value> = children
                .iter()
                .map(|child| match child.as_deref() {
                    Some(child) => dump(child.view()),
                    None => json!([["null"]]),
                })
                .collect();
            let same = own_children.is_some_and(|own| core::ptr::eq(own, children));
            json!({"children": trees, "same_vector": same})
        }),
        "GetChild" => {
            let index = read_u64(get(input, "index")?)?;
            node.get_child(index).map(|child| {
                // GetChild returns m_children[aIndex] itself (the same pointer, or null).
                let stored = match &*node {
                    Sexpr::List(list) => usize::try_from(index)
                        .ok()
                        .and_then(|i| list.children.get(i))
                        .map(|c| c.as_deref().map(|c| c as *const Sexpr)),
                    _ => None,
                };
                let returned = child.map(|c| c as *const Sexpr);
                json!({
                    "child": child.map(|c| dump(c.view())),
                    "same_object": stored == Some(returned)
                })
            })
        }
        "GetNumberOfChildren" => node.get_number_of_children().map(|n| json!({"value": n})),
        "GetLongInteger" => node.get_long_integer().map(|v| json!({"value": v})),
        "GetInteger" => node.get_integer().map(|v| json!({"value": v})),
        "GetFloat" => node.get_float().map(|v| json!({"value": bits32(v)})),
        "GetDouble" => node.get_double().map(|v| json!({"value": bits64(v)})),
        "GetString" => node.get_string().map(|v| json!({"value": hex(v)})),
        "GetSymbol" => node.get_symbol().map(|v| json!({"value": hex(v)})),
        "GetList" => {
            // GetList() returns `this`: the list stored in this node.
            let own: *const SexprList = match &*node {
                Sexpr::List(list) => list,
                _ => core::ptr::null(),
            };
            node.get_list().map(|list| {
                let same = core::ptr::eq(own, list);
                json!({"same_object": same, "tree": dump(list.view())})
            })
        }
        "AsString" => {
            let level = get(input, "level")?;
            let level = if level.is_null() { 0 } else { read_u64(level)? };
            check_as_string(node.view(), level)?;
            node.as_string(level).map(|v| json!({"value": hex(&v)}))
        }
        "AddChild" => {
            let child = read_optional_tree(get(input, "child")?)?;
            node.add_child(child)
                .map(|()| json!({"tree": dump(node.view())}))
        }
        _ => return Err("unknown method".into()),
    };
    match outcome {
        Ok(value) => Ok(value),
        Err(error) => raised(error),
    }
}

fn arg_items(value: &Value) -> Result<&Vec<Value>, Error> {
    let items = value.as_array().ok_or("expected arguments")?;
    if items.is_empty() || items.len() > MAX_ARGS {
        return Err("argument count out of range".into());
    }
    Ok(items)
}

/// One `operator<<` right operand.
enum Item {
    Double(f64),
    Float(f32),
    Int64(i64),
    Int32(i32),
    String(Vec<u8>),
    OutString(bool, Vec<u8>),
    Isexprable,
    List(Option<Box<SexprList>>),
    Sexpr(Option<Box<Sexpr>>),
}

fn read_item(value: &Value) -> Result<Item, Error> {
    let item = object(value)?;
    Ok(match text(item, "kind")? {
        "double" => {
            keys(item, &["kind", "value"])?;
            Item::Double(read_f64(get(item, "value")?)?)
        }
        "float" => {
            keys(item, &["kind", "value"])?;
            Item::Float(read_f32(get(item, "value")?)?)
        }
        "int64" => {
            keys(item, &["kind", "value"])?;
            Item::Int64(read_i64(get(item, "value")?)?)
        }
        "int32" => {
            keys(item, &["kind", "value"])?;
            Item::Int32(read_i32(get(item, "value")?)?)
        }
        "string" => {
            keys(item, &["kind", "value"])?;
            Item::String(read_hex(get(item, "value")?)?)
        }
        "out_string" => {
            keys(item, &["kind", "symbol", "value"])?;
            let bytes = read_hex(get(item, "value")?)?;
            Item::OutString(read_bool(get(item, "symbol")?)?, bytes)
        }
        "isexprable" => {
            keys(item, &["kind"])?;
            Item::Isexprable
        }
        "list" => {
            keys(item, &["kind", "tree"])?;
            let tree = get(item, "tree")?;
            Item::List(if tree.is_null() {
                None
            } else {
                Some(Box::new(read_list(tree)?))
            })
        }
        "sexpr" => {
            keys(item, &["kind", "tree"])?;
            Item::Sexpr(read_optional_tree(get(item, "tree")?)?)
        }
        _ => return Err("unknown argument kind".into()),
    })
}

fn insert(input: &Map<String, Value>) -> Result<Value, Error> {
    keys(input, &["op", "tree", "items"])?;
    let mut list = read_list(get(input, "tree")?)?;
    let items = arg_items(get(input, "items")?)?
        .iter()
        .map(read_item)
        .collect::<Result<Vec<_>, _>>()?;
    let mut returns_self = true;
    for item in items {
        let before: *const SexprList = &list;
        let returned: *const SexprList = match item {
            Item::Double(v) => list.insert_f64(v),
            Item::Float(v) => list.insert_f32(v),
            Item::Int64(v) => list.insert_i64(v),
            Item::Int32(v) => list.insert_i32(v),
            Item::String(v) => list.insert_string(v),
            Item::OutString(symbol, bytes) => list.insert_out_string(if symbol {
                as_symbol(&bytes)
            } else {
                as_string(&bytes)
            }),
            Item::Isexprable => list.insert_isexprable(&DefaultIsexprable),
            Item::List(v) => list.insert_list(v),
            Item::Sexpr(v) => list.insert_sexpr(v),
        };
        returns_self &= core::ptr::eq(before, returned);
    }
    Ok(json!({
        "in_stream_child": list.in_stream_child(),
        "returns_self": returns_self,
        "tree": dump(list.view())
    }))
}

/// One `operator>>` right operand with its value.
enum Target {
    Int32(i32),
    Int64(i64),
    Float(f32),
    Double(f64),
    String(Vec<u8>),
    InString(bool, Vec<u8>),
    Isexprable,
}

fn read_target(value: &Value) -> Result<Target, Error> {
    let item = object(value)?;
    Ok(match text(item, "kind")? {
        "int32" => {
            keys(item, &["kind", "initial"])?;
            Target::Int32(read_i32(get(item, "initial")?)?)
        }
        "int64" => {
            keys(item, &["kind", "initial"])?;
            Target::Int64(read_i64(get(item, "initial")?)?)
        }
        "float" => {
            keys(item, &["kind", "initial"])?;
            Target::Float(read_f32(get(item, "initial")?)?)
        }
        "double" => {
            keys(item, &["kind", "initial"])?;
            Target::Double(read_f64(get(item, "initial")?)?)
        }
        "string" => {
            keys(item, &["kind", "initial"])?;
            Target::String(read_hex(get(item, "initial")?)?)
        }
        "in_string" => {
            keys(item, &["kind", "symbol", "initial"])?;
            let symbol = read_bool(get(item, "symbol")?)?;
            Target::InString(symbol, read_hex(get(item, "initial")?)?)
        }
        "isexprable" => {
            keys(item, &["kind"])?;
            Target::Isexprable
        }
        _ => return Err("unknown argument kind".into()),
    })
}

fn extract(input: &Map<String, Value>) -> Result<Value, Error> {
    keys(input, &["op", "tree", "targets"])?;
    let mut list = read_list(get(input, "tree")?)?;
    let mut targets = arg_items(get(input, "targets")?)?
        .iter()
        .map(read_target)
        .collect::<Result<Vec<_>, _>>()?;
    let mut completed = 0usize;
    let mut returns_self = true;
    let mut raised_exception = Value::Null;
    for target in targets.iter_mut() {
        let before: *const SexprList = &list;
        let returned = match target {
            Target::Int32(v) => list.extract_i32(v).map(|l| l as *const SexprList),
            Target::Int64(v) => list.extract_i64(v).map(|l| l as *const SexprList),
            Target::Float(v) => list.extract_f32(v).map(|l| l as *const SexprList),
            Target::Double(v) => list.extract_f64(v).map(|l| l as *const SexprList),
            Target::String(v) => list.extract_string(v).map(|l| l as *const SexprList),
            Target::InString(symbol, v) => {
                let wrapper = if *symbol {
                    as_symbol_in(v)
                } else {
                    as_string_in(v)
                };
                list.extract_in_string(wrapper)
                    .map(|l| l as *const SexprList)
            }
            Target::Isexprable => {
                let mut object = DefaultIsexprable;
                Ok(list.extract_isexprable(&mut object) as *const SexprList)
            }
        };
        match returned {
            Ok(returned) => {
                returns_self &= core::ptr::eq(before, returned);
                completed += 1;
            }
            Err(error) => {
                raised_exception = exception(error)?;
                break;
            }
        }
    }
    let values: Vec<Value> = targets
        .iter()
        .map(|target| match target {
            Target::Int32(v) => json!(v),
            Target::Int64(v) => json!(v),
            Target::Float(v) => bits32(*v),
            Target::Double(v) => bits64(*v),
            Target::String(v) | Target::InString(_, v) => hex(v),
            Target::Isexprable => Value::Null,
        })
        .collect();
    Ok(json!({
        "completed": completed,
        "exception": raised_exception,
        "in_stream_child": list.in_stream_child(),
        "returns_self": returns_self,
        "targets": values
    }))
}
