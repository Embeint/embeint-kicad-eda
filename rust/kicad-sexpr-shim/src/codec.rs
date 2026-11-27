// SPDX-License-Identifier: GPL-3.0-or-later
//! The request and reply encoding shared with `port/oracle/shims/libs_sexpr.cpp`.
//!
//! - `std::string` values are lowercase hex byte strings; `double` and `float` are their exact
//!   IEEE bits as 16 and 8 lowercase hex digits; `int32_t`/`int64_t`/`size_t` are JSON
//!   integers.
//! - A tree is a flat pre-order array of nodes, so its depth is not limited by the JSON
//!   transport's container nesting: `["list", line, n]` followed by its `n` children,
//!   `["integer", value, line]`, `["double", bits, line]`, `["string", hex, line]`,
//!   `["symbol", hex, line]` and `["null"]` (a null child). In a request `line` is null (the
//!   one-argument constructor, line 1) or an `int` (the constructor taking `int aLineNumber`);
//!   a reply always reports the stored `size_t` line number.
//! - Each request is bounded: [`MAX_NODES`] nodes per tree, [`MAX_DEPTH`] levels and
//!   [`MAX_STRING`] bytes per string, so the reference's recursive destructor and `AsString`
//!   stay within its stack.

use kicad_sexpr::{Node, Sexpr, SexprDouble, SexprError, SexprInteger, SexprList};
use kicad_sexpr::{SexprString, SexprSymbol, StdException, View};
use serde_json::{json, Map, Value};

/// The refusal for a request whose pinned evaluation is undefined.
pub const UNDEFINED: &str = "undefined sexpr operation";
/// The most nodes a tree may have.
pub const MAX_NODES: usize = 20_000;
/// The deepest a tree may be (the root is at depth 1).
pub const MAX_DEPTH: usize = 4_000;
/// The longest string, in bytes.
pub const MAX_STRING: usize = 65_536;
/// The most arguments of one `Scan`/`AddChildren` call (the reference adapter instantiates
/// the variadic templates for 1 to 16 arguments).
pub const MAX_ARGS: usize = 16;

/// A refused or deferred request.
pub enum Error {
    /// `invalid_input` with this message.
    Invalid(String),
    /// A deferral to an unported unit.
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Invalid(message.into())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

/// The reply for a library failure: an exception the reference throws is a result (its type
/// and `what()`); an undefined evaluation is refused and an unported call deferred.
pub fn raised(error: SexprError) -> Result<Value, Error> {
    let (kind, what): (&str, Vec<u8>) = match error {
        SexprError::InvalidType(exception) => {
            ("SEXPR::INVALID_TYPE_EXCEPTION", exception.what().to_vec())
        }
        SexprError::InvalidArgument(what) => ("std::invalid_argument", what.as_bytes().to_vec()),
        SexprError::LengthError(what) => ("std::length_error", what.as_bytes().to_vec()),
        SexprError::BadAlloc => ("std::bad_alloc", b"std::bad_alloc".to_vec()),
        SexprError::Undefined => return Err(UNDEFINED.into()),
        SexprError::Unported { upstream, unit } => return Err(Error::Unported { upstream, unit }),
    };
    Ok(json!({"exception": {"type": kind, "what": hex(&what)}}))
}

pub fn object(value: &Value) -> Result<&Map<String, Value>, Error> {
    value.as_object().ok_or_else(|| "expected object".into())
}

/// Require exactly these fields.
pub fn keys(value: &Map<String, Value>, fields: &[&str]) -> Result<(), Error> {
    if value.len() != fields.len() || fields.iter().any(|f| !value.contains_key(*f)) {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

pub fn get<'a>(value: &'a Map<String, Value>, name: &str) -> Result<&'a Value, Error> {
    value
        .get(name)
        .ok_or_else(|| format!("missing: {name}").into())
}

pub fn text<'a>(value: &'a Map<String, Value>, name: &str) -> Result<&'a str, Error> {
    get(value, name)?
        .as_str()
        .ok_or_else(|| format!("expected string: {name}").into())
}

pub fn read_bool(value: &Value) -> Result<bool, Error> {
    value.as_bool().ok_or_else(|| "expected boolean".into())
}

pub fn read_i64(value: &Value) -> Result<i64, Error> {
    if !(value.is_i64() || value.is_u64()) {
        return Err("integer out of range".into());
    }
    value.as_i64().ok_or_else(|| "integer out of range".into())
}

pub fn read_i32(value: &Value) -> Result<i32, Error> {
    i32::try_from(read_i64(value)?).map_err(|_| "integer out of range".into())
}

pub fn read_u64(value: &Value) -> Result<u64, Error> {
    if !value.is_u64() {
        return Err("integer out of range".into());
    }
    value.as_u64().ok_or_else(|| "integer out of range".into())
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// A lowercase hex byte string of at most [`MAX_STRING`] bytes.
pub fn read_hex(value: &Value) -> Result<Vec<u8>, Error> {
    let text = value.as_str().ok_or("invalid hex")?.as_bytes();
    if text.len() % 2 != 0 || text.len() > 2 * MAX_STRING {
        return Err("invalid hex".into());
    }
    text.chunks_exact(2)
        .map(|pair| match pair {
            [high, low] => match (hex_digit(*high), hex_digit(*low)) {
                (Some(high), Some(low)) => Ok(high * 16 + low),
                _ => Err("invalid hex".into()),
            },
            _ => Err("invalid hex".into()),
        })
        .collect()
}

pub fn hex(bytes: &[u8]) -> Value {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    Value::String(out)
}

fn read_bits_text(value: &Value, digits: usize) -> Result<u64, Error> {
    let text = value.as_str().ok_or("invalid bits")?;
    if text.len() != digits || !text.bytes().all(|b| hex_digit(b).is_some()) {
        return Err("invalid bits".into());
    }
    u64::from_str_radix(text, 16).map_err(|_| "invalid bits".into())
}

/// A `double` from its 16 hex digits.
pub fn read_f64(value: &Value) -> Result<f64, Error> {
    read_bits_text(value, 16).map(f64::from_bits)
}

/// A `float` from its 8 hex digits.
pub fn read_f32(value: &Value) -> Result<f32, Error> {
    let bits = read_bits_text(value, 8)?;
    u32::try_from(bits)
        .map(f32::from_bits)
        .map_err(|_| "invalid bits".into())
}

pub fn bits64(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

pub fn bits32(value: f32) -> Value {
    json!(format!("{:08x}", value.to_bits()))
}

/// One decoded request node.
enum Parsed {
    List(SexprList, u64),
    Atom(Sexpr),
    Null,
}

/// The `line` field: null for the one-argument constructor, otherwise its `int`.
fn read_line(value: &Value) -> Result<Option<i32>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    read_i32(value).map(Some)
}

fn parse_node(value: &Value) -> Result<Parsed, Error> {
    let invalid = || Error::from("invalid node");
    let parts = value.as_array().ok_or_else(invalid)?;
    let kind = parts.first().and_then(Value::as_str).ok_or_else(invalid)?;
    if kind == "null" {
        return if parts.len() == 1 {
            Ok(Parsed::Null)
        } else {
            Err(invalid())
        };
    }
    let [_, payload, extra] = parts.as_slice() else {
        return Err(invalid());
    };
    Ok(match kind {
        "list" => {
            let count = read_u64(extra).map_err(|_| invalid())?;
            if count > MAX_NODES as u64 {
                return Err(invalid());
            }
            let list = match read_line(payload).map_err(|_| invalid())? {
                None => SexprList::new(),
                Some(line) => SexprList::with_line_number(line),
            };
            Parsed::List(list, count)
        }
        "integer" => {
            let value = read_i64(payload).map_err(|_| invalid())?;
            Parsed::Atom(Sexpr::from(
                match read_line(extra).map_err(|_| invalid())? {
                    None => SexprInteger::new(value),
                    Some(line) => SexprInteger::with_line_number(value, line),
                },
            ))
        }
        "double" => {
            let value = read_f64(payload).map_err(|_| invalid())?;
            Parsed::Atom(Sexpr::from(
                match read_line(extra).map_err(|_| invalid())? {
                    None => SexprDouble::new(value),
                    Some(line) => SexprDouble::with_line_number(value, line),
                },
            ))
        }
        "string" => {
            let value = read_hex(payload).map_err(|_| invalid())?;
            Parsed::Atom(Sexpr::from(
                match read_line(extra).map_err(|_| invalid())? {
                    None => SexprString::new(&value),
                    Some(line) => SexprString::with_line_number(&value, line),
                },
            ))
        }
        "symbol" => {
            let value = read_hex(payload).map_err(|_| invalid())?;
            Parsed::Atom(Sexpr::from(
                match read_line(extra).map_err(|_| invalid())? {
                    None => SexprSymbol::new(&value),
                    Some(line) => SexprSymbol::with_line_number(&value, line),
                },
            ))
        }
        _ => return Err(invalid()),
    })
}

struct Frame {
    list: SexprList,
    remaining: u64,
}

/// Decode a tree, adding children through the public `m_children` member exactly as the
/// reference adapter does. The root may not be null.
pub fn read_tree(value: &Value) -> Result<Sexpr, Error> {
    let nodes = value.as_array().ok_or("expected tree")?;
    if nodes.is_empty() {
        return Err("empty tree".into());
    }
    if nodes.len() > MAX_NODES {
        return Err("tree too large".into());
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: Option<Sexpr> = None;
    for (index, raw) in nodes.iter().enumerate() {
        if stack.is_empty() && index > 0 {
            return Err("trailing nodes".into());
        }
        let parsed = parse_node(raw)?;
        if stack.is_empty() && matches!(parsed, Parsed::Null) {
            return Err("null root".into());
        }
        if stack.len() + 1 > MAX_DEPTH {
            return Err("tree too deep".into());
        }
        let mut finished = match parsed {
            Parsed::List(list, remaining) => {
                stack.push(Frame { list, remaining });
                None
            }
            Parsed::Atom(atom) => Some(Some(Box::new(atom))),
            Parsed::Null => Some(None),
        };
        loop {
            if let Some(child) = finished.take() {
                match stack.last_mut() {
                    Some(parent) => {
                        parent.list.children.push(child);
                        parent.remaining = parent.remaining.saturating_sub(1);
                    }
                    None => root = child.map(|node| *node),
                }
            }
            match stack.last() {
                Some(top) if top.remaining == 0 => {
                    if let Some(frame) = stack.pop() {
                        finished = Some(Some(Box::new(Sexpr::List(frame.list))));
                    }
                }
                _ => break,
            }
        }
    }
    if !stack.is_empty() {
        return Err("truncated tree".into());
    }
    root.ok_or_else(|| "empty tree".into())
}

/// Decode a tree whose root must be a list.
pub fn read_list(value: &Value) -> Result<SexprList, Error> {
    match read_tree(value)? {
        Sexpr::List(list) => Ok(list),
        _ => Err("expected list root".into()),
    }
}

/// Decode a tree, or null for a null pointer.
pub fn read_optional_tree(value: &Value) -> Result<Option<Box<Sexpr>>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    read_tree(value).map(|node| Some(Box::new(node)))
}

fn entry(node: Option<View<'_>>) -> Value {
    match node {
        None => json!(["null"]),
        Some(View::List(list)) => json!(["list", list.get_line_number(), list.children.len()]),
        Some(View::Integer(atom)) => json!(["integer", atom.value, atom.get_line_number()]),
        Some(View::Double(atom)) => json!(["double", bits64(atom.value), atom.get_line_number()]),
        Some(View::String(atom)) => json!(["string", hex(&atom.value), atom.get_line_number()]),
        Some(View::Symbol(atom)) => json!(["symbol", hex(&atom.value), atom.get_line_number()]),
    }
}

/// Visit `root` and its descendants in pre-order, without recursion.
pub fn preorder<'a>(root: View<'a>, mut visit: impl FnMut(Option<View<'a>>)) {
    visit(Some(root));
    let mut stack = Vec::new();
    if let View::List(list) = root {
        stack.push(list.children.iter());
    }
    while let Some(top) = stack.last_mut() {
        match top.next() {
            Some(child) => {
                let view = child.as_deref().map(Node::view);
                visit(view);
                if let Some(View::List(list)) = view {
                    stack.push(list.children.iter());
                }
            }
            None => {
                stack.pop();
            }
        }
    }
}

/// The reply encoding of a tree.
pub fn dump(root: View<'_>) -> Value {
    let mut out = Vec::new();
    preorder(root, |node| out.push(entry(node)));
    Value::Array(out)
}

/// Every node's five predicates (`IsList`, `IsSymbol`, `IsString`, `IsDouble`, `IsInteger`)
/// and `GetLineNumber`, in pre-order; a null child is `null`.
pub fn predicates(root: View<'_>) -> Value {
    let mut out = Vec::new();
    preorder(root, |node| {
        out.push(match node {
            None => Value::Null,
            Some(view) => {
                let node: &dyn Node = match view {
                    View::List(n) => n,
                    View::Integer(n) => n,
                    View::Double(n) => n,
                    View::String(n) => n,
                    View::Symbol(n) => n,
                };
                json!([
                    node.is_list(),
                    node.is_symbol(),
                    node.is_string(),
                    node.is_double(),
                    node.is_integer(),
                    node.get_line_number()
                ])
            }
        })
    });
    Value::Array(out)
}
