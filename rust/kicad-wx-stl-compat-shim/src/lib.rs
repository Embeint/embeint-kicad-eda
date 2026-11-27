// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for unit `libs.core.application.thin.replacement.p001`. It mirrors
//! `port/oracle/shims/core_wx_stl_compat.cpp` field for field, including validation order
//! and refusal messages; the operation schema is in README.md. Only validation, conversion
//! and calls into `kicad-wx-stl-compat` live here.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use std::collections::BTreeMap;

use kicad_math::stream::FmtFlags;
use kicad_wx_stl_compat::less::{less_wx_point, WxPointKey};
use kicad_wx_stl_compat::ostream::{
    insert_wx_point, insert_wx_size, CallContext, InsertError, IoState, NumPunct, OStream,
    SentryFlush, MAX_SINK_CAPACITY, MAX_UNBOUNDED_WIDTH,
};
use kicad_wx_stl_compat::to_wx_string::{to_wx_string, LibcCtype};
use kicad_wx_stl_compat::{WxPoint, WxSize};
use serde_json::{json, Value};

pub use kicad_wx_stl_compat::UNIT;

pub mod host;

use host::{
    CallIndex, Cancel, CancelOp, HostException, IosChange, PutScript, PutThrow, Sink, SinkScripts,
    TieSink,
};

/// Largest `std::map<wxPoint, int>` request.
pub const MAX_ENTRIES: usize = 256;
/// Largest `std::string_view`, in bytes.
pub const MAX_VIEW: usize = 4096;
/// Largest `numpunct::grouping()` string.
pub const MAX_GROUPING: usize = 16;
/// Largest scripted `sync()` or `do_put` call index.
pub const MAX_SCRIPT_CALL: u64 = 64;
/// Largest width the scripted facet sets (the stock body pads to it in stack storage).
pub const MAX_FACET_WIDTH: i64 = 4096;
/// Most stock `do_put` calls one scripted facet call makes.
pub const MAX_DELEGATE: u64 = 2;

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
const HOST_FIELDS: [&str; 4] = ["overflow_throw", "sync_throw", "num_put", "tie"];
const HOST_OPTIONAL: [&str; 2] = ["null_rdbuf", "cancel"];
const TIE_FIELDS: [&str; 5] = ["state", "exceptions", "flags", "sync_fails", "sync_throw"];
const TIE_OPTIONAL: [&str; 1] = ["null_rdbuf"];
const PUT_FIELDS: [&str; 2] = ["call", "throw"];
const PUT_OPTIONAL: [&str; 5] = ["set_before", "set_after", "fill", "value", "delegate"];
const CHANGE_FIELDS: [&str; 3] = ["width", "precision", "flags"];

type Result<T> = core::result::Result<T, String>;

fn keys(v: &Value, fields: &[&str], message: &str) -> Result<()> {
    let o = v.as_object().ok_or(message)?;
    if o.len() != fields.len() || fields.iter().any(|f| !o.contains_key(*f)) {
        return Err(message.into());
    }
    Ok(())
}

/// An object with every `required` field and no field outside `required` and `optional`.
fn keys_with(v: &Value, required: &[&str], optional: &[&str], message: &str) -> Result<()> {
    let o = v.as_object().ok_or(message)?;
    if required.iter().any(|f| !o.contains_key(*f))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
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
    fmtflags_or(v, "expected fmtflags")
}

fn fmtflags_or(v: &Value, message: &str) -> Result<FmtFlags> {
    let set = names(v, &FMTFLAGS, message)?;
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

/// A scripted call index in `[0, maximum]`, or `null` for none.
fn call_index(v: &Value, maximum: u64, message: &str) -> Result<CallIndex> {
    if v.is_null() {
        return Ok(None);
    }
    v.as_u64()
        .filter(|&n| n <= maximum)
        .map(Some)
        .ok_or_else(|| message.to_string())
}

/// The tied stream's configuration.
struct TieConfig {
    state: IoState,
    exceptions: IoState,
    flags: FmtFlags,
    sync_fails: bool,
    sync_throw: CallIndex,
    null_rdbuf: bool,
}

/// The `host` object: the buffer, facet, tie and cancellation scripts.
struct HostConfig {
    overflow_throw: CallIndex,
    sync_throw: CallIndex,
    put: Option<PutScript>,
    tie: Option<TieConfig>,
    null_rdbuf: bool,
    cancel: Option<Cancel>,
}

/// A JSON integer in `[minimum, maximum]`.
fn integer(v: &Value, minimum: i64, maximum: i64) -> Option<i64> {
    v.as_i64().filter(|n| (minimum..=maximum).contains(n))
}

/// An optional boolean field: absent is false.
fn optional_bool(v: &Value, field: &str, message: &str) -> Result<bool> {
    match v.get(field) {
        None => Ok(false),
        Some(flag) => flag.as_bool().ok_or_else(|| message.to_string()),
    }
}

/// `set_before`/`set_after`: `{"width", "precision", "flags"}`, each `null` or a value.
fn ios_change(v: Option<&Value>) -> Result<IosChange> {
    const MESSAGE: &str = "expected num_put";
    let Some(v) = v else {
        return Ok(IosChange::default());
    };
    keys(v, &CHANGE_FIELDS, MESSAGE)?;
    let width = match &v["width"] {
        Value::Null => None,
        w => Some(integer(w, i64::MIN, MAX_FACET_WIDTH).ok_or(MESSAGE)?),
    };
    let precision = match &v["precision"] {
        Value::Null => None,
        p => Some(p.as_i64().ok_or(MESSAGE)?),
    };
    let flags = match &v["flags"] {
        Value::Null => None,
        f => Some(fmtflags_or(f, MESSAGE)?),
    };
    Ok(IosChange {
        width,
        precision,
        flags,
    })
}

fn put_script(put: &Value) -> Result<PutScript> {
    const MESSAGE: &str = "expected num_put";
    keys_with(put, &PUT_FIELDS, &PUT_OPTIONAL, MESSAGE)?;
    let call = call_index(&put["call"], MAX_SCRIPT_CALL, MESSAGE)?.ok_or(MESSAGE)?;
    let throw = match put["throw"].as_str() {
        Some("before") => PutThrow::Before,
        Some("after") => PutThrow::After,
        Some("none") => PutThrow::None,
        _ => return Err(MESSAGE.into()),
    };
    let set_before = ios_change(put.get("set_before"))?;
    let set_after = ios_change(put.get("set_after"))?;
    let fill = match put.get("fill") {
        None => None,
        Some(f) => Some(byte(f).map_err(|_| MESSAGE.to_string())?),
    };
    let value = match put.get("value") {
        None => None,
        Some(v) => Some(v.as_i64().ok_or(MESSAGE)?),
    };
    let delegate = match put.get("delegate") {
        None => 1,
        Some(d) => d
            .as_u64()
            .filter(|&n| n <= MAX_DELEGATE)
            .and_then(|n| u8::try_from(n).ok())
            .ok_or(MESSAGE)?,
    };
    Ok(PutScript {
        call,
        throw,
        set_before,
        set_after,
        fill,
        value,
        delegate,
    })
}

fn cancel_script(v: Option<&Value>) -> Result<Option<Cancel>> {
    const MESSAGE: &str = "expected cancel";
    let Some(v) = v else {
        return Ok(None);
    };
    keys(v, &["op", "call"], MESSAGE)?;
    let op = match v["op"].as_str() {
        Some("overflow") => CancelOp::Overflow,
        Some("sync") => CancelOp::Sync,
        Some("num_put") => CancelOp::NumPut,
        Some("tie_sync") => CancelOp::TieSync,
        _ => return Err(MESSAGE.into()),
    };
    let maximum = if op == CancelOp::Overflow {
        MAX_SINK_CAPACITY as u64
    } else {
        MAX_SCRIPT_CALL
    };
    let call = call_index(&v["call"], maximum, MESSAGE)?.ok_or(MESSAGE)?;
    Ok(Some(Cancel { op, call }))
}

fn host_config(v: &Value) -> Result<HostConfig> {
    keys_with(
        v,
        &HOST_FIELDS,
        &HOST_OPTIONAL,
        "unexpected or missing host field",
    )?;
    let overflow_throw = call_index(
        &v["overflow_throw"],
        MAX_SINK_CAPACITY as u64,
        "expected overflow_throw",
    )?;
    let sync_throw = call_index(&v["sync_throw"], MAX_SCRIPT_CALL, "expected sync_throw")?;
    let put = match &v["num_put"] {
        Value::Null => None,
        put => Some(put_script(put)?),
    };
    let tie = match &v["tie"] {
        Value::Null => None,
        tie => {
            keys_with(
                tie,
                &TIE_FIELDS,
                &TIE_OPTIONAL,
                "unexpected or missing tie field",
            )?;
            Some(TieConfig {
                state: iostate(&tie["state"])?,
                exceptions: iostate(&tie["exceptions"])?,
                flags: fmtflags(&tie["flags"])?,
                sync_fails: tie["sync_fails"].as_bool().ok_or("expected sync_fails")?,
                sync_throw: call_index(&tie["sync_throw"], MAX_SCRIPT_CALL, "expected sync_throw")?,
                null_rdbuf: optional_bool(tie, "null_rdbuf", "expected null_rdbuf")?,
            })
        }
    };
    let null_rdbuf = optional_bool(v, "null_rdbuf", "expected null_rdbuf")?;
    let cancel = cancel_script(v.get("cancel"))?;
    Ok(HostConfig {
        overflow_throw,
        sync_throw,
        put,
        tie,
        null_rdbuf,
        cancel,
    })
}

/// The caller's stream after its setup.
struct Setup {
    out: OStream<Sink>,
    /// `exceptions( mask )` threw `ios_base::failure` (caught; the stream stays usable).
    setup_failure: bool,
    /// The same for the tied stream.
    tie_setup_failure: bool,
    /// A `host` object was given: the reply adds the trace and the tie.
    scripted: bool,
}

/// The caller's stream, configured as the C++ adapter configures it: buffer, the tied
/// stream (flags, `clear( state )`, then `exceptions( mask )`, whose throw is caught), the
/// imbued `numpunct` (and scripted `num_put`), flags, width, precision, fill, `tie`,
/// `clear( state )`, then `exceptions( mask )`, whose throw is caught. `unwinding` is the
/// request's call context, which excludes a cancellation script.
fn stream(v: &Value, flush: SentryFlush, unwinding: bool) -> Result<Setup> {
    let fields = v.as_object().ok_or("unexpected or missing stream field")?;
    let scripted = fields.contains_key("host");
    let expected = STREAM_FIELDS.len() + usize::from(scripted);
    if fields.len() != expected || STREAM_FIELDS.iter().any(|f| !fields.contains_key(*f)) {
        return Err("unexpected or missing stream field".into());
    }
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
    let host = if scripted {
        Some(host_config(&v["host"])?)
    } else {
        None
    };
    let cancel = host.as_ref().and_then(|h| h.cancel);
    if unwinding && cancel.is_some() {
        return Err("unwinding excludes cancel".into());
    }
    if capacity.is_none() && width > MAX_UNBOUNDED_WIDTH {
        return Err("unbounded width exceeds limit".into());
    }
    let scripts = host
        .as_ref()
        .map_or(SinkScripts::default(), |h| SinkScripts {
            overflow_throw: h.overflow_throw,
            sync_throw: h.sync_throw,
            put: h.put,
            cancel: h.cancel,
            null_rdbuf: h.null_rdbuf,
        });
    let mut sink = Sink::new(capacity, sync_fails, scripts).map_err(|_| "expected capacity")?;
    let mut tie_setup_failure = false;
    if let Some(tie) = host.and_then(|h| h.tie) {
        let mut tied = OStream::new(TieSink::new(
            tie.sync_fails,
            tie.sync_throw,
            cancel,
            tie.null_rdbuf,
        ));
        tied.sentry_flush = flush;
        tied.flags = tie.flags;
        // clear() cannot throw before a mask is set.
        let _ = tied.clear(tie.state);
        tie_setup_failure = tied.set_exceptions(tie.exceptions).is_err();
        sink.set_tie(tied);
    }
    let mut out = OStream::new(sink);
    out.numpunct = NumPunct::new(thousands_sep, &grouping);
    out.sentry_flush = flush;
    out.flags = flags;
    out.width = width;
    out.precision = precision;
    out.fill = fill;
    let _ = out.clear(state);
    let setup_failure = out.set_exceptions(exceptions).is_err();
    Ok(Setup {
        out,
        setup_failure,
        tie_setup_failure,
        scripted,
    })
}

fn insert(input: &Value, flush: SentryFlush, size: bool) -> Result<Value> {
    keys_with(
        input,
        &["op", "value", "stream"],
        &["unwinding"],
        "unexpected or missing field",
    )?;
    let (x, y) = point(&input["value"])?;
    let unwinding = optional_bool(input, "unwinding", "expected unwinding")?;
    let Setup {
        mut out,
        setup_failure,
        tie_setup_failure,
        scripted,
    } = stream(&input["stream"], flush, unwinding)?;
    // Called from a destructor while std::runtime_error( "outer" ) propagates, which the
    // operator cannot affect.
    let context = if unwinding {
        CallContext::UNWINDING
    } else {
        CallContext::NORMAL
    };
    let start: *const OStream<Sink> = &out;
    let reply = if size {
        insert_wx_size(&mut out, &WxSize::new(x, y), context).map(|r| core::ptr::eq(r, start))
    } else {
        insert_wx_point(&mut out, &WxPoint::new(x, y), context).map(|r| core::ptr::eq(r, start))
    };
    let mut what = None;
    let (outcome, same) = match reply {
        Ok(same) => ("returned", same),
        Err(InsertError::Failure) => ("ios_failure", true),
        Err(InsertError::Host(HostException::Runtime(message))) => {
            what = Some(message);
            ("runtime_error", true)
        }
        Err(InsertError::Host(HostException::BadAlloc)) => ("bad_alloc", true),
        // The cancelled thread ended; the adapter reports from the joining thread.
        Err(InsertError::Host(HostException::ForcedUnwind(_))) => ("forced_unwind", true),
        Err(InsertError::Terminate) => ("terminate", true),
        Err(InsertError::Abort) => ("abort", true),
        // Refused up front by the adapter contract (widths bounded); never reached.
        Err(InsertError::Refused(_)) => return Err("unbounded width exceeds limit".into()),
    };
    let sink = out.host();
    if sink.overrun() {
        return Err("event limit exceeded".into());
    }
    let mut result = json!({
        "bytes": hex(sink.bytes()),
        "exceptions": iostate_json(out.exceptions()),
        "fill": out.fill,
        "flags": fmtflags_json(&out.flags),
        "outcome": outcome,
        "precision": out.precision,
        "same_stream": same,
        "state": iostate_json(out.rdstate()),
        "sync_calls": sink.sync_calls(),
        "width": out.width,
    });
    if setup_failure {
        result["setup_failure"] = json!(true);
    }
    if unwinding {
        result["outer_preserved"] = json!(true);
    }
    if let Some(message) = what {
        result["what"] = json!(message);
    }
    if scripted {
        result["events"] = sink.events_json();
        if let Some(tie) = sink.tie() {
            result["tie"] = json!({
                "exceptions": iostate_json(tie.exceptions()),
                "setup_failure": tie_setup_failure,
                "state": iostate_json(tie.rdstate()),
                "sync_calls": tie.host().sync_calls(),
            });
        }
    }
    // Keys in nlohmann::json's sorted order, so the replies are byte-identical too.
    let sorted: BTreeMap<String, Value> = match result {
        Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    Ok(Value::Object(sorted.into_iter().collect()))
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
pub const CORPUS_CASES: usize = 5709;
