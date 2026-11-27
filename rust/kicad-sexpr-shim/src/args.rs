// SPDX-License-Identifier: GPL-3.0-or-later
//! `Scan` and `AddChildren` arguments, built through the reference's public constructors.
//!
//! A scan argument is `{"kind": "int"|"long_int"|"double"|"string", "initial": ...}` (a
//! destination with its value before the call), `{"kind": "in_string", "symbol": bool,
//! "initial": hex}` (an `_IN_STRING` lvalue), or `{"kind": "const_string"|"string_value"|
//! "c_str", "value": hex}` (a string to compare with, passed as `const std::string*`, as a
//! `std::string` and as `const char*`, which ends at its first NUL).
//!
//! A children argument is `{"kind": "int"|"long_int", "value": integer}`, `{"kind":
//! "double", "value": bits}`, `{"kind": "string"|"c_str", "value": hex}`, `{"kind":
//! "out_string", "symbol": bool, "value": hex}` or `{"kind": "sexpr", "tree": tree|null}`.

use crate::codec::{
    bits64, get, hex, keys, object, read_bool, read_f64, read_hex, read_i32, read_i64,
    read_optional_tree, text, Error, MAX_ARGS,
};
use core::ffi::CStr;
use kicad_sexpr::{as_string, as_symbol, ChildrenArg, InString, ScanArg};
use serde_json::Value;

/// The storage of one scan argument: the destination variable or the comparison string.
pub enum ScanStore {
    Int(i32),
    LongInt(i64),
    Double(f64),
    String(Vec<u8>),
    InString {
        symbol: bool,
        string: Vec<u8>,
    },
    ConstString(Vec<u8>),
    StringValue(Vec<u8>),
    /// The bytes with a terminating NUL appended.
    CStr(Vec<u8>),
}

fn arg_list(value: &Value) -> Result<&Vec<Value>, Error> {
    let items = value.as_array().ok_or("expected arguments")?;
    if items.is_empty() || items.len() > MAX_ARGS {
        return Err("argument count out of range".into());
    }
    Ok(items)
}

pub fn read_scan_args(value: &Value) -> Result<Vec<ScanStore>, Error> {
    arg_list(value)?
        .iter()
        .map(|item| {
            let item = object(item)?;
            Ok(match text(item, "kind")? {
                "int" => {
                    keys(item, &["kind", "initial"])?;
                    ScanStore::Int(read_i32(get(item, "initial")?)?)
                }
                "long_int" => {
                    keys(item, &["kind", "initial"])?;
                    ScanStore::LongInt(read_i64(get(item, "initial")?)?)
                }
                "double" => {
                    keys(item, &["kind", "initial"])?;
                    ScanStore::Double(read_f64(get(item, "initial")?)?)
                }
                "string" => {
                    keys(item, &["kind", "initial"])?;
                    ScanStore::String(read_hex(get(item, "initial")?)?)
                }
                "in_string" => {
                    keys(item, &["kind", "symbol", "initial"])?;
                    ScanStore::InString {
                        symbol: read_bool(get(item, "symbol")?)?,
                        string: read_hex(get(item, "initial")?)?,
                    }
                }
                "const_string" => {
                    keys(item, &["kind", "value"])?;
                    ScanStore::ConstString(read_hex(get(item, "value")?)?)
                }
                "string_value" => {
                    keys(item, &["kind", "value"])?;
                    ScanStore::StringValue(read_hex(get(item, "value")?)?)
                }
                "c_str" => {
                    keys(item, &["kind", "value"])?;
                    let mut bytes = read_hex(get(item, "value")?)?;
                    bytes.push(0);
                    ScanStore::CStr(bytes)
                }
                _ => return Err("unknown argument kind".into()),
            })
        })
        .collect()
}

/// A borrowed scan argument before its `SEXPR_SCAN_ARG` is constructed; an `_IN_STRING`
/// must outlive the argument that points to it.
enum Prepared<'a> {
    Int(&'a mut i32),
    LongInt(&'a mut i64),
    Double(&'a mut f64),
    String(&'a mut Vec<u8>),
    InString(InString<'a>),
    ConstString(&'a [u8]),
    StringValue(Vec<u8>),
    CStr(&'a CStr),
}

/// Construct every `SEXPR_SCAN_ARG` from `stores` and run `call` with them.
pub fn with_scan_args<R>(
    stores: &mut [ScanStore],
    call: impl FnOnce(&mut [ScanArg<'_>]) -> R,
) -> Result<R, Error> {
    let mut prepared = stores
        .iter_mut()
        .map(|store| {
            Ok(match store {
                ScanStore::Int(v) => Prepared::Int(v),
                ScanStore::LongInt(v) => Prepared::LongInt(v),
                ScanStore::Double(v) => Prepared::Double(v),
                ScanStore::String(v) => Prepared::String(v),
                ScanStore::InString { symbol, string } => Prepared::InString(InString {
                    symbol: *symbol,
                    string,
                }),
                ScanStore::ConstString(v) => Prepared::ConstString(v),
                ScanStore::StringValue(v) => Prepared::StringValue(v.clone()),
                ScanStore::CStr(v) => Prepared::CStr(
                    CStr::from_bytes_until_nul(v).map_err(|_| Error::from("invalid c_str"))?,
                ),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut args: Vec<ScanArg<'_>> = prepared
        .iter_mut()
        .map(|item| match item {
            Prepared::Int(v) => ScanArg::int(v),
            Prepared::LongInt(v) => ScanArg::long_int(v),
            Prepared::Double(v) => ScanArg::double(v),
            Prepared::String(v) => ScanArg::string(v),
            Prepared::InString(v) => ScanArg::sexpr_string(v),
            Prepared::ConstString(v) => ScanArg::const_string(v),
            Prepared::StringValue(v) => ScanArg::string_value(core::mem::take(v)),
            Prepared::CStr(v) => ScanArg::c_str(v),
        })
        .collect();
    Ok(call(&mut args))
}

/// The value of every destination after a scan; comparison arguments are null.
pub fn scan_results(stores: &[ScanStore]) -> Value {
    Value::Array(
        stores
            .iter()
            .map(|store| match store {
                ScanStore::Int(v) => Value::from(*v),
                ScanStore::LongInt(v) => Value::from(*v),
                ScanStore::Double(v) => bits64(*v),
                ScanStore::String(v) | ScanStore::InString { string: v, .. } => hex(v),
                ScanStore::ConstString(_) | ScanStore::StringValue(_) | ScanStore::CStr(_) => {
                    Value::Null
                }
            })
            .collect(),
    )
}

pub fn read_children_args(value: &Value) -> Result<Vec<ChildrenArg>, Error> {
    arg_list(value)?
        .iter()
        .map(|item| {
            let item = object(item)?;
            Ok(match text(item, "kind")? {
                "int" => {
                    keys(item, &["kind", "value"])?;
                    ChildrenArg::int(read_i32(get(item, "value")?)?)
                }
                "long_int" => {
                    keys(item, &["kind", "value"])?;
                    ChildrenArg::long_int(read_i64(get(item, "value")?)?)
                }
                "double" => {
                    keys(item, &["kind", "value"])?;
                    ChildrenArg::double(read_f64(get(item, "value")?)?)
                }
                "string" => {
                    keys(item, &["kind", "value"])?;
                    ChildrenArg::string(&read_hex(get(item, "value")?)?)
                }
                "c_str" => {
                    keys(item, &["kind", "value"])?;
                    let mut bytes = read_hex(get(item, "value")?)?;
                    bytes.push(0);
                    let c_str = CStr::from_bytes_until_nul(&bytes)
                        .map_err(|_| Error::from("invalid c_str"))?;
                    ChildrenArg::c_str(c_str)
                }
                "out_string" => {
                    keys(item, &["kind", "symbol", "value"])?;
                    let bytes = read_hex(get(item, "value")?)?;
                    let out = if read_bool(get(item, "symbol")?)? {
                        as_symbol(&bytes)
                    } else {
                        as_string(&bytes)
                    };
                    ChildrenArg::out_string(&out)
                }
                "sexpr" => {
                    keys(item, &["kind", "tree"])?;
                    ChildrenArg::sexpr(read_optional_tree(get(item, "tree")?)?)
                }
                _ => return Err("unknown argument kind".into()),
            })
        })
        .collect()
}
