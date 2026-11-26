// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for unit `libs.core.application.thin.replacement.p001`. It mirrors
//! `port/oracle/shims/core_wx_stl_compat.cpp` field for field, including validation order
//! and refusal messages; the operation schema is in README.md. Only validation, conversion
//! and calls into `kicad-wx-stl-compat` live here.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use kicad_math::stream::FmtFlags;
use kicad_wx_stl_compat::less::{less_wx_point, WxPointKey};
use kicad_wx_stl_compat::ostream::{
    insert_wx_point, insert_wx_size, InsertError, IoState, NumPunct, OStream, SentryFlush,
    StreamBuf, MAX_SINK_CAPACITY, MAX_UNBOUNDED_WIDTH,
};
use kicad_wx_stl_compat::to_wx_string::{to_wx_string, LibcCtype};
use kicad_wx_stl_compat::{WxPoint, WxSize};
use serde_json::{json, Value};

pub use kicad_wx_stl_compat::UNIT;

/// Largest `std::map<wxPoint, int>` request.
pub const MAX_ENTRIES: usize = 256;
/// Largest `std::string_view`, in bytes.
pub const MAX_VIEW: usize = 4096;
/// Largest `numpunct::grouping()` string.
pub const MAX_GROUPING: usize = 16;

const IOSTATE: [&str; 3] = ["bad", "eof", "fail"];
const FMTFLAGS: [&str; 15] = [
    "boolalpha",
    "dec",
    "fixed",
    "hex",
    "internal",
    "left",
    "oct",
    "right",
    "scientific",
    "showbase",
    "showpoint",
    "showpos",
    "skipws",
    "unitbuf",
    "uppercase",
];
const STREAM_FIELDS: [&str; 10] = [
    "state",
    "exceptions",
    "flags",
    "width",
    "precision",
    "fill",
    "thousands_sep",
    "grouping",
    "capacity",
    "sync_fails",
];

type Result<T> = core::result::Result<T, String>;

fn keys(v: &Value, fields: &[&str], message: &str) -> Result<()> {
    let o = v.as_object().ok_or(message)?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err(message.into());
    }
    Ok(())
}

fn int32(v: &Value, message: &str) -> Result<i32> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| message.to_string())
}

/// `[x, y]`: exactly two JSON integers in the 32-bit `int` range.
fn point(v: &Value) -> Result<(i32, i32)> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    let mut values = a.iter().map(|n| int32(n, "component out of range"));
    let x = values.next().ok_or("expected point")??;
    let y = values.next().ok_or("expected point")??;
    Ok((x, y))
}

fn point_json(p: &WxPoint) -> Value {
    json!([p.x, p.y])
}

/// Lowercase hexadecimal bytes; `null` is a view with a null `data()`.
fn view(v: &Value) -> Result<Option<Vec<u8>>> {
    if v.is_null() {
        return Ok(None);
    }
    let text = v
        .as_str()
        .filter(|t| t.len() % 2 == 0 && t.len() <= 2 * MAX_VIEW)
        .ok_or("expected view")?;
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    text.as_bytes()
        .chunks(2)
        .map(|pair| match pair {
            [high, low] => Some(nibble(*high)? << 4 | nibble(*low)?),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()
        .map(Some)
        .ok_or_else(|| "expected view".into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A JSON array of distinct names from `names`, as the set of those present.
fn names(v: &Value, names: &[&str], message: &str) -> Result<Vec<bool>> {
    let a = v.as_array().ok_or(message)?;
    let mut present = vec![false; names.len()];
    for item in a {
        let index = item
            .as_str()
            .and_then(|s| names.iter().position(|n| *n == s))
            .ok_or(message)?;
        let slot = present.get_mut(index).ok_or(message)?;
        if *slot {
            return Err(message.into());
        }
        *slot = true;
    }
    Ok(present)
}

fn iostate(v: &Value) -> Result<IoState> {
    let set = names(v, &IOSTATE, "expected iostate")?;
    let has = |i: usize| set.get(i).copied().unwrap_or(false);
    Ok(IoState {
        bad: has(0),
        eof: has(1),
        fail: has(2),
    })
}

fn iostate_json(s: IoState) -> Value {
    let mut out = Vec::new();
    for (name, on) in IOSTATE.iter().zip([s.bad, s.eof, s.fail]) {
        if on {
            out.push(json!(name));
        }
    }
    Value::Array(out)
}

fn flag_values(f: &FmtFlags) -> [bool; 15] {
    [
        f.boolalpha,
        f.dec,
        f.fixed,
        f.hex,
        f.internal,
        f.left,
        f.oct,
        f.right,
        f.scientific,
        f.showbase,
        f.showpoint,
        f.showpos,
        f.skipws,
        f.unitbuf,
        f.uppercase,
    ]
}

fn fmtflags(v: &Value) -> Result<FmtFlags> {
    let set = names(v, &FMTFLAGS, "expected fmtflags")?;
    let has = |i: usize| set.get(i).copied().unwrap_or(false);
    Ok(FmtFlags {
        boolalpha: has(0),
        dec: has(1),
        fixed: has(2),
        hex: has(3),
        internal: has(4),
        left: has(5),
        oct: has(6),
        right: has(7),
        scientific: has(8),
        showbase: has(9),
        showpoint: has(10),
        showpos: has(11),
        skipws: has(12),
        unitbuf: has(13),
        uppercase: has(14),
    })
}

fn fmtflags_json(f: &FmtFlags) -> Value {
    let mut out = Vec::new();
    for (name, on) in FMTFLAGS.iter().zip(flag_values(f)) {
        if on {
            out.push(json!(name));
        }
    }
    Value::Array(out)
}

fn streamsize(v: &Value) -> Result<i64> {
    v.as_i64().ok_or_else(|| "expected streamsize".into())
}

fn byte(v: &Value) -> Result<u8> {
    v.as_i64()
        .and_then(|n| u8::try_from(n).ok())
        .ok_or_else(|| "expected byte".into())
}

fn grouping(v: &Value) -> Result<Vec<u8>> {
    let a = v
        .as_array()
        .filter(|a| a.len() <= MAX_GROUPING)
        .ok_or("expected grouping")?;
    a.iter()
        .map(|b| byte(b).map_err(|_| "expected grouping".to_string()))
        .collect()
}

fn capacity(v: &Value) -> Result<Option<usize>> {
    if v.is_null() {
        return Ok(None);
    }
    v.as_i64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|&n| n <= MAX_SINK_CAPACITY)
        .map(Some)
        .ok_or_else(|| "expected capacity".into())
}

/// The caller's stream, configured as the C++ adapter configures it: buffer, imbued
/// `numpunct`, flags, width, precision, fill, `clear( state )`, then `exceptions( mask )`.
fn stream(v: &Value, flush: SentryFlush) -> Result<OStream> {
    keys(v, &STREAM_FIELDS, "unexpected or missing stream field")?;
    let state = iostate(&v["state"])?;
    let exceptions = iostate(&v["exceptions"])?;
    let flags = fmtflags(&v["flags"])?;
    let width = streamsize(&v["width"])?;
    let precision = streamsize(&v["precision"])?;
    let fill = byte(&v["fill"])?;
    let thousands_sep = byte(&v["thousands_sep"])?;
    let grouping = grouping(&v["grouping"])?;
    let capacity = capacity(&v["capacity"])?;
    let sync_fails = v["sync_fails"].as_bool().ok_or("expected sync_fails")?;
    // Constructing such a stream throws from exceptions( mask ).
    if state.intersects(exceptions) {
        return Err("state intersects exceptions".into());
    }
    // ~sentry would call std::terminate on the reference runtime.
    if flags.unitbuf && sync_fails && exceptions.bad {
        return Err("sentry destructor would terminate".into());
    }
    if capacity.is_none() && width > MAX_UNBOUNDED_WIDTH {
        return Err("unbounded width exceeds limit".into());
    }
    let buf = StreamBuf::new(capacity, sync_fails).map_err(|_| "expected capacity")?;
    let mut out = OStream::new(buf);
    out.numpunct = NumPunct {
        thousands_sep,
        grouping,
    };
    out.sentry_flush = flush;
    out.flags = flags;
    out.width = width;
    out.precision = precision;
    out.fill = fill;
    out.clear(state)
        .and_then(|()| out.set_exceptions(exceptions))
        .map_err(|_| "state intersects exceptions")?;
    Ok(out)
}

fn insert(input: &Value, flush: SentryFlush, size: bool) -> Result<Value> {
    keys(
        input,
        &["op", "value", "stream"],
        "unexpected or missing field",
    )?;
    let (x, y) = point(&input["value"])?;
    let mut out = stream(&input["stream"], flush)?;
    let start: *const OStream = &out;
    let reply = if size {
        insert_wx_size(&mut out, &WxSize::new(x, y)).map(|r| core::ptr::eq(r, start))
    } else {
        insert_wx_point(&mut out, &WxPoint::new(x, y)).map(|r| core::ptr::eq(r, start))
    };
    let (outcome, same) = match reply {
        Ok(same) => ("returned", same),
        Err(InsertError::Failure) => ("ios_failure", true),
        // Refused up front by the adapter contract; never reached.
        Err(InsertError::Terminate) => return Err("sentry destructor would terminate".into()),
        Err(InsertError::Refused(_)) => return Err("unbounded width exceeds limit".into()),
    };
    Ok(json!({
        "bytes": hex(out.buf().bytes()),
        "exceptions": iostate_json(out.exceptions()),
        "fill": out.fill,
        "flags": fmtflags_json(&out.flags),
        "outcome": outcome,
        "precision": out.precision,
        "same_stream": same,
        "state": iostate_json(out.rdstate()),
        "sync_calls": out.buf().sync_calls(),
        "width": out.width,
    }))
}

fn run(v: &Value, flush: SentryFlush) -> Result<Value> {
    let op = v
        .get("op")
        .and_then(Value::as_str)
        .ok_or("expected string: op")?;
    match op {
        "less_point" => {
            keys(v, &["op", "a", "b"], "unexpected or missing field")?;
            let (ax, ay) = point(&v["a"])?;
            let (bx, by) = point(&v["b"])?;
            let less = less_wx_point(&WxPoint::new(ax, ay), &WxPoint::new(bx, by));
            Ok(json!({ "less": less }))
        }
        "point_map" => {
            keys(v, &["op", "entries"], "unexpected or missing field")?;
            let entries = v["entries"]
                .as_array()
                .filter(|a| a.len() <= MAX_ENTRIES)
                .ok_or("expected entries")?;
            // std::map<wxPoint, int>::emplace in request order: the first value is kept.
            let mut map = BTreeMap::new();
            for entry in entries {
                let pair = entry
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or("expected entry")?;
                let (x, y) = point(&pair[0])?;
                let value = int32(&pair[1], "value out of range")?;
                map.entry(WxPointKey(WxPoint::new(x, y))).or_insert(value);
            }
            let keys: Vec<Value> = map.keys().map(|k| point_json(&k.0)).collect();
            let values: Vec<Value> = map.values().map(|v| json!(v)).collect();
            Ok(json!({ "keys": keys, "values": values }))
        }
        "to_wx_string" => {
            keys(v, &["op", "ctype", "view"], "unexpected or missing field")?;
            let ctype = match v["ctype"].as_str() {
                Some("C") => LibcCtype::C,
                Some("C.UTF-8") => LibcCtype::Utf8,
                _ => return Err("expected ctype".into()),
            };
            let bytes = view(&v["view"])?;
            let text = to_wx_string(bytes.as_deref(), ctype).map_err(|_| "allocation refused")?;
            Ok(json!({ "units": text.units() }))
        }
        "insert_size" => insert(v, flush, true),
        "insert_point" => insert(v, flush, false),
        _ => Err("unknown operation".into()),
    }
}

/// One request line with the reference runtime's sentry ([`SentryFlush::Unconditional`]).
pub fn dispatch(line: &str) -> Value {
    dispatch_with(line, SentryFlush::Unconditional)
}

/// One request line with an explicit libstdc++ `~sentry` behaviour, for comparison against
/// an oracle built on a different runtime.
pub fn dispatch_with(line: &str, flush: SentryFlush) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case": case, "error": {"kind": kind, "message": message}, "unit": unit});
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
    if let Some(value) = request.get("unit").filter(|v| v.is_string()) {
        unit = value.clone();
    }
    if let Some(value) = request.get("case").filter(|v| v.is_string()) {
        case = value.clone();
    }
    if keys(&request, &["unit", "case", "input"], "").is_err()
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
    match run(&request["input"], flush) {
        Ok(result) => json!({"case": case, "result": result, "unit": unit}),
        Err(message) => failure(unit, case, "invalid_input", &message),
    }
}

/// The number of requests in the committed public corpus.
pub const CORPUS_CASES: usize = 3329;
