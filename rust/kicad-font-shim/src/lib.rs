// SPDX-License-Identifier: GPL-3.0-or-later
//! JSONL adapter for `include.font.application.ui.p001` (the `font.h` and `outline_font.h`
//! inline bodies). It mirrors `port/oracle/shims/include_font_p001.cpp`: the same requests,
//! validation order and messages, and the same "undefined font operation" refusals where the
//! pinned evaluation is undefined. Rust deferrals are reported as `unported` with their owner;
//! the production `wxOnAssert` is one of them. `--assert-host=returning` instead supplies a
//! recording handler that returns, as wx's default handler does in the reference process, so
//! the continuation after the assertion is compared too. The request schema is in `README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use kicad_font::font::{draw, Font, FontBase};
use kicad_font::font_stream::{insert_font_ptr_with, insert_font_with, FontStreamError};
use kicad_font::outline_font::{EmbeddingPermission, FtFace, OutlineFont};
use kicad_font::text_style::{
    is_bold, is_italic, is_subscript, is_superscript, TextStyle, TextStyleFlags, ITALIC_TILT,
};
use kicad_font::wx_conv::{deferred_assert, WxAssert};
use kicad_font::{FontError, UNIT};
use kicad_map_helpers::wx_string::{WChar, WxString};
use kicad_math::Vector2I;
use kicad_wx_stl_compat::ostream::{
    BadAlloc, CallContext, InsertError, IoState, OStream, SentryFlush, StreamBuf, StreamHost,
};
use serde_json::{json, Map, Value};

const UNDEFINED: &str = "undefined font operation";
const MAX_UNITS: usize = 4096;
const MAX_WIDTH: i64 = 4096;
const MAX_CAPACITY: i64 = 4096;
const MAX_CALLS: usize = 8;

/// Process options of the native shim.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// The runtime's `~sentry` behaviour: the reference (GCC 13/14 runtime) is
    /// [`SentryFlush::Unconditional`]; `--sentry-flush=when-good` matches a GCC 15+ runtime
    /// for local diagnostics only.
    pub sentry_flush: SentryFlush,
    /// The `wxOnAssert` callee.
    pub assert_host: AssertHost,
}

/// Which `wxOnAssert` callee the shim supplies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssertHost {
    /// The production callee: a typed `unported` deferral.
    #[default]
    Deferred,
    /// A handler that records the call's arguments and returns, as wx 3.2.4's default handler
    /// does without a `wxApp` (it logs and returns `false`): the C++ continuation follows.
    /// Diagnostic comparison of that continuation only; not production behaviour.
    Returning,
}

/// The `wxOnAssert` calls one operation made, and the callee it uses.
struct Assertions {
    host: AssertHost,
    calls: Vec<WxAssert>,
}

impl Assertions {
    fn new(host: AssertHost) -> Self {
        Self {
            host,
            calls: Vec::new(),
        }
    }

    fn on_assert(&mut self, assert: &WxAssert) -> core::result::Result<(), FontError> {
        match self.host {
            AssertHost::Deferred => deferred_assert(assert),
            AssertHost::Returning => {
                self.calls.push(*assert);
                Ok(())
            }
        }
    }

    /// The recorded calls, as the oracle reports them.
    fn json(&self) -> Value {
        Value::Array(
            self.calls
                .iter()
                .map(|a| {
                    json!({"line": a.line, "function": a.function,
                           "condition": a.condition, "message": a.message})
                })
                .collect(),
        )
    }
}

enum Error {
    Invalid(String),
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Invalid(v)
    }
}

impl From<&str> for Error {
    fn from(v: &str) -> Self {
        Self::Invalid(v.into())
    }
}

impl From<FontError> for Error {
    fn from(e: FontError) -> Self {
        match e {
            FontError::Undefined { .. } => Self::Invalid(UNDEFINED.into()),
            FontError::Alloc { upstream } => {
                Self::Invalid(format!("allocation refused: {upstream}"))
            }
            FontError::Unported { upstream, unit } => Self::Unported { upstream, unit },
        }
    }
}

type Result<T> = core::result::Result<T, Error>;

// Decoding ---------------------------------------------------------------------------------------

fn object(v: &Value) -> Result<&Map<String, Value>> {
    Ok(v.as_object().ok_or("expected object")?)
}

/// Every required field present (checked first), then no field outside required/optional.
fn keys(v: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let o = object(v)?;
    if required.iter().any(|f| !o.contains_key(*f))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(())
}

fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value> {
    Ok(v.get(name).ok_or("unexpected or missing field")?)
}

fn boolean(v: &Value, name: &str) -> Result<bool> {
    field(v, name)?
        .as_bool()
        .ok_or_else(|| format!("expected bool: {name}").into())
}

/// A JSON integer token within `[min, max]`.
fn integer(v: &Value, min: i64, max: i64, name: &str) -> Result<i64> {
    let out_of_range = || Error::Invalid(format!("integer out of range: {name}"));
    let number = v.as_number().ok_or_else(out_of_range)?;
    let value = if let Some(n) = number.as_i64() {
        n
    } else if number.as_u64().is_some() {
        // Above i64::MAX: beyond every bound used here.
        return Err(out_of_range());
    } else {
        return Err(out_of_range());
    };
    if value < min || value > max {
        return Err(out_of_range());
    }
    Ok(value)
}

fn units(v: &Value, name: &str) -> Result<Vec<WChar>> {
    let items = v
        .as_array()
        .filter(|a| a.len() <= MAX_UNITS)
        .ok_or_else(|| format!("expected units: {name}"))?;
    items
        .iter()
        .map(|unit| {
            integer(unit, i64::from(i32::MIN), i64::from(i32::MAX), name).map(|n| n as WChar)
        })
        .collect()
}

fn text(units: &[WChar]) -> Result<WxString> {
    WxString::from_units(units).map_err(|_| Error::Invalid("string allocation refused".into()))
}

fn units_json(text: &WxString) -> Value {
    Value::Array(text.units().iter().map(|&u| json!(u)).collect())
}

fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}

fn from_bits(v: &Value, name: &str) -> Result<f64> {
    let invalid = || Error::Invalid(format!("invalid hex: {name}"));
    let t = v.as_str().filter(|t| t.len() == 16).ok_or_else(invalid)?;
    if !t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(invalid());
    }
    u64::from_str_radix(t, 16)
        .map(f64::from_bits)
        .map_err(|_| invalid())
}

fn flags_value(v: &Value) -> Result<TextStyleFlags> {
    Ok(integer(v, 0, i64::from(u32::MAX), "flags")? as TextStyleFlags)
}

// Fonts -----------------------------------------------------------------------------------------

/// The fonts a request can construct: the adapter's FONT subclasses (defaults, or scripted
/// style queries) and a genuine OUTLINE_FONT.
enum ShimFont {
    Base(FontBase),
    Scripted(FontBase, [bool; 4]),
    Outline(OutlineFont),
}

impl Font for ShimFont {
    fn font_base(&self) -> &FontBase {
        match self {
            Self::Base(base) | Self::Scripted(base, _) => base,
            Self::Outline(font) => font.font_base(),
        }
    }
    fn is_stroke(&self) -> bool {
        match self {
            Self::Base(_) => false,
            Self::Scripted(_, s) => s[0],
            Self::Outline(font) => font.is_stroke(),
        }
    }
    fn is_outline(&self) -> bool {
        match self {
            Self::Base(_) => false,
            Self::Scripted(_, s) => s[1],
            Self::Outline(font) => font.is_outline(),
        }
    }
    fn is_bold(&self) -> bool {
        match self {
            Self::Base(_) => false,
            Self::Scripted(_, s) => s[2],
            Self::Outline(font) => font.is_bold(),
        }
    }
    fn is_italic(&self) -> bool {
        match self {
            Self::Base(_) => false,
            Self::Scripted(_, s) => s[3],
            Self::Outline(font) => font.is_italic(),
        }
    }
    // The adapter's FONT subclasses implement the pure virtuals only to be concrete; no
    // operation calls them (the C++ adapter throws if one ever did).
    fn get_super_sub_size_multiplier(&self) -> f64 {
        match self {
            Self::Outline(font) => font.get_super_sub_size_multiplier(),
            _ => f64::NAN,
        }
    }
    fn get_super_sub_baseline_offset(&self, text_style: TextStyleFlags) -> f64 {
        match self {
            Self::Outline(font) => font.get_super_sub_baseline_offset(text_style),
            _ => f64::NAN,
        }
    }
}

struct Holder {
    font: ShimFont,
}

fn make_font(spec: &Value, outline_only: bool) -> Result<Holder> {
    let kind = spec
        .as_object()
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str)
        .ok_or("expected font")?;
    let (mut font, name) = match kind {
        "base" if !outline_only => {
            keys(spec, &["kind", "name"], &[])?;
            let name = units(field(spec, "name")?, "name")?;
            (ShimFont::Base(FontBase::new()), name)
        }
        "scripted" if !outline_only => {
            keys(
                spec,
                &["kind", "name", "stroke", "outline", "bold", "italic"],
                &[],
            )?;
            let name = units(field(spec, "name")?, "name")?;
            let stroke = boolean(spec, "stroke")?;
            let outline = boolean(spec, "outline")?;
            let bold = boolean(spec, "bold")?;
            let italic = boolean(spec, "italic")?;
            (
                ShimFont::Scripted(FontBase::new(), [stroke, outline, bold, italic]),
                name,
            )
        }
        "outline" => {
            keys(
                spec,
                &["kind", "name", "file", "face", "fake_bold", "fake_ital"],
                &[],
            )?;
            let name = units(field(spec, "name")?, "name")?;
            let file = units(field(spec, "file")?, "file")?;
            let face = field(spec, "face")?;
            let style_flags = if face.is_null() {
                None
            } else {
                keys(face, &["style_flags"], &[])?;
                Some(integer(
                    field(face, "style_flags")?,
                    i64::MIN,
                    i64::MAX,
                    "style_flags",
                )?)
            };
            let fake_bold = boolean(spec, "fake_bold")?;
            let fake_ital = boolean(spec, "fake_ital")?;
            let mut outline = OutlineFont::new();
            outline.font_base_mut().font_file_name = text(&file)?;
            if let Some(style_flags) = style_flags {
                outline.set_face(Some(FtFace { style_flags }));
            }
            if fake_bold {
                outline.set_fake_bold();
            }
            if fake_ital {
                outline.set_fake_ital();
            }
            (ShimFont::Outline(outline), name)
        }
        _ => return Err("unknown font kind".into()),
    };
    let name_text = text(&name)?;
    match &mut font {
        ShimFont::Base(base) | ShimFont::Scripted(base, _) => base.font_name = name_text,
        ShimFont::Outline(outline) => outline.font_base_mut().font_name = name_text,
    }
    Ok(Holder { font })
}

fn outline_of(holder: &mut Holder) -> Result<&mut OutlineFont> {
    match &mut holder.font {
        ShimFont::Outline(font) => Ok(font),
        _ => Err("unknown font kind".into()),
    }
}

// Streams ---------------------------------------------------------------------------------------

/// The adapter's `SINK`, or no buffer at all (`std::ostream( nullptr )`).
struct Sink(Option<StreamBuf>);

impl StreamHost for Sink {
    type Exception = BadAlloc;

    fn has_rdbuf(&self) -> bool {
        self.0.is_some()
    }
    fn overflow(&mut self, byte: u8) -> core::result::Result<bool, BadAlloc> {
        match &mut self.0 {
            Some(buffer) => buffer.overflow(byte),
            None => Ok(false),
        }
    }
    fn sync(&mut self) -> core::result::Result<i32, BadAlloc> {
        match &mut self.0 {
            Some(buffer) => buffer.sync(),
            None => Ok(0),
        }
    }
    fn capacity_left(&self) -> Option<usize> {
        match &self.0 {
            Some(buffer) => buffer.capacity_left(),
            None => Some(0),
        }
    }
}

fn state(v: &Value) -> Result<IoState> {
    keys(v, &["bad", "eof", "fail"], &[])?;
    let bad = boolean(v, "bad")?;
    let eof = boolean(v, "eof")?;
    let fail = boolean(v, "fail")?;
    Ok(IoState { bad, eof, fail })
}

fn state_json(s: IoState) -> Value {
    json!({"bad": s.bad, "eof": s.eof, "fail": s.fail})
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn run_insert(input: &Value, options: Options) -> Result<Value> {
    keys(input, &["op", "font", "pointer", "stream"], &[])?;
    let pointer = boolean(input, "pointer")?;
    let font_spec = field(input, "font")?;
    let spec = field(input, "stream")?;
    keys(
        spec,
        &["state", "exceptions", "flags", "width", "fill", "buffer"],
        &[],
    )?;
    let initial = state(field(spec, "state")?)?;
    let mask = state(field(spec, "exceptions")?)?;
    let list = field(spec, "flags")?
        .as_array()
        .filter(|a| a.len() <= 4)
        .ok_or("expected flags")?;
    let mut flags = [false; 4];
    for flag in list {
        let index = match flag.as_str() {
            Some("left") => 0,
            Some("right") => 1,
            Some("internal") => 2,
            Some("unitbuf") => 3,
            _ => return Err("expected flags".into()),
        };
        let slot = flags.get_mut(index).ok_or("expected flags")?;
        if *slot {
            return Err("expected flags".into());
        }
        *slot = true;
    }
    let [left, right, internal, unitbuf] = flags;
    let width = integer(field(spec, "width")?, -MAX_WIDTH, MAX_WIDTH, "width")?;
    let fill = integer(field(spec, "fill")?, 0, 255, "fill")? as u8;
    let buffer = field(spec, "buffer")?;
    let mut capacity = None;
    let mut sync_fails = false;
    if !buffer.is_null() {
        keys(buffer, &["capacity", "sync_fails"], &[])?;
        let c = field(buffer, "capacity")?;
        if !c.is_null() {
            capacity = Some(integer(c, 0, MAX_CAPACITY, "capacity")? as usize);
        }
        sync_fails = boolean(buffer, "sync_fails")?;
    }
    if font_spec.is_null() {
        if !pointer {
            return Err("expected font".into());
        }
        // os << *aFont with a null aFont.
        return Err(UNDEFINED.into());
    }
    let holder = make_font(font_spec, false)?;
    let sink = if buffer.is_null() {
        Sink(None)
    } else {
        Sink(Some(StreamBuf::new(capacity, sync_fails).map_err(
            |_| Error::Invalid("stream buffer refused".into()),
        )?))
    };
    let mut out = OStream::new(sink);
    out.sentry_flush = options.sentry_flush;
    out.flags.left = left;
    out.flags.right = right;
    out.flags.internal = internal;
    out.flags.unitbuf = unitbuf;
    out.width = width;
    out.fill = fill;
    // The mask is still empty, so clear() cannot throw.
    let _ = out.clear(initial);
    let setup_threw = out.set_exceptions(mask).is_err();
    let mut assertions = Assertions::new(options.assert_host);
    let mut on_assert = |assert: &WxAssert| assertions.on_assert(assert);
    let reply = if pointer {
        insert_font_ptr_with(
            &mut out,
            Some(&holder.font),
            CallContext::NORMAL,
            &mut on_assert,
        )
        .map(|_| ())
    } else {
        insert_font_with(&mut out, &holder.font, CallContext::NORMAL, &mut on_assert).map(|_| ())
    };
    let outcome = match reply {
        Ok(()) => "returned",
        Err(FontStreamError::Stream(InsertError::Failure)) => "failure",
        // A failing sync() of a unitbuf stream whose setstate( badbit ) throws from the
        // noexcept ~sentry: the reference process calls std::terminate with the state reached.
        Err(FontStreamError::Stream(InsertError::Terminate)) => "terminate",
        Err(FontStreamError::Font(error)) => return Err(error.into()),
        Err(other) => return Err(format!("unexpected stream outcome: {other:?}").into()),
    };
    let (bytes, syncs) = match &out.host().0 {
        Some(buffer) => (hex(buffer.bytes()), buffer.sync_calls()),
        None => (String::new(), 0),
    };
    Ok(json!({
        "bytes": bytes,
        "state": state_json(out.rdstate()),
        "width": out.width,
        "sync_calls": syncs,
        "outcome": outcome,
        "setup_threw": setup_threw,
        "assertions": assertions.json(),
    }))
}

// Operations ------------------------------------------------------------------------------------

fn point(v: &Value) -> Result<Vector2I> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("expected point")?;
    let mut values = [0i32; 2];
    for (slot, item) in values.iter_mut().zip(a) {
        *slot = integer(item, i64::from(i32::MIN), i64::from(i32::MAX), "point")? as i32;
    }
    Ok(Vector2I::new(values[0], values[1]))
}

/// The C++ adapter's `FaceSizeDefined`, expressed through the library's own refusal.
fn size_argument(input: &Value) -> Result<i32> {
    Ok(integer(
        field(input, "size")?,
        i64::from(i32::MIN),
        i64::from(i32::MAX),
        "size",
    )? as i32)
}

fn run(input: &Value, options: Options) -> Result<Value> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or("expected op")?;
    match op {
        "text_style" => {
            keys(input, &["op"], &[])?;
            Ok(json!({
                "values": TextStyle::ALL.map(TextStyle::bits),
                "italic_tilt": bits(ITALIC_TILT),
                "embedding_permission": EmbeddingPermission::ALL.map(|p| p as i32),
            }))
        }
        "style_predicates" => {
            keys(input, &["op", "flags"], &[])?;
            let flags = flags_value(field(input, "flags")?)?;
            Ok(json!({
                "bold": is_bold(flags), "italic": is_italic(flags),
                "subscript": is_subscript(flags), "superscript": is_superscript(flags),
            }))
        }
        "queries" => {
            keys(input, &["op", "font"], &[])?;
            let holder = make_font(field(input, "font")?, false)?;
            let font = &holder.font;
            Ok(json!({
                "stroke": font.is_stroke(), "outline": font.is_outline(),
                "bold": font.is_bold(), "italic": font.is_italic(),
                "name": units_json(font.get_name()),
            }))
        }
        "lines_count" => {
            keys(input, &["op", "font", "text"], &[])?;
            let holder = make_font(field(input, "font")?, false)?;
            let value = text(&units(field(input, "text")?, "text")?)?;
            Ok(json!({"count": holder.font.lines_count(&value)}))
        }
        "name_as_token" => {
            keys(input, &["op", "font"], &[])?;
            let holder = make_font(field(input, "font")?, false)?;
            let mut assertions = Assertions::new(options.assert_host);
            let token = holder
                .font
                .name_as_token_with(&mut |assert: &WxAssert| assertions.on_assert(assert))?;
            Ok(json!({"bytes": hex(&token), "assertions": assertions.json()}))
        }
        "insert" => run_insert(input, options),
        "draw" => {
            keys(
                input,
                &["op", "font", "text", "position", "mouse_pos", "active_url"],
                &[],
            )?;
            let holder = make_font(field(input, "font")?, false)?;
            let value = text(&units(field(input, "text")?, "text")?)?;
            let position = point(field(input, "position")?)?;
            let mouse = field(input, "mouse_pos")?;
            let mouse = if mouse.is_null() {
                None
            } else {
                Some(point(mouse)?)
            };
            let url = field(input, "active_url")?;
            let mut url = if url.is_null() {
                None
            } else {
                Some(text(&units(url, "active_url")?)?)
            };
            // No GAL exists in the oracle: aGal is nullptr. TEXT_ATTRIBUTES and METRICS are
            // not ported; the deferred Draw receives them as opaque values.
            draw::<_, (), (), ()>(
                &holder.font,
                None,
                &value,
                position,
                &(),
                &(),
                mouse,
                url.as_mut(),
            )?;
            Ok(json!({"active_url": url.as_ref().map(units_json)}))
        }
        "outline" => {
            keys(input, &["op", "font"], &[])?;
            let mut holder = make_font(field(input, "font")?, true)?;
            let font = outline_of(&mut holder)?;
            let face = font
                .get_face()
                .map(|face| json!({"style_flags": face.style_flags}));
            Ok(json!({
                "stroke": font.is_stroke(),
                "outline": font.is_outline(),
                "bold": font.is_bold(),
                "italic": font.is_italic(),
                "fake_bold": font.is_fake_bold(),
                "fake_italic": font.is_fake_italic(),
                "name": units_json(font.get_name()),
                "file": units_json(font.get_file_name()),
                "face": face,
                "multiplier": bits(font.get_super_sub_size_multiplier()),
                "face_size": font.face_size()?,
                "subscript_size": font.subscript_size()?,
            }))
        }
        "set_fake" => {
            keys(input, &["op", "font", "calls"], &[])?;
            let mut holder = make_font(field(input, "font")?, true)?;
            let calls = field(input, "calls")?
                .as_array()
                .filter(|a| a.len() <= MAX_CALLS)
                .ok_or("expected calls")?;
            let font = outline_of(&mut holder)?;
            let mut states = Vec::new();
            for call in calls {
                match call.as_str() {
                    Some("bold") => font.set_fake_bold(),
                    Some("ital") => font.set_fake_ital(),
                    _ => return Err("expected calls".into()),
                }
                states.push(json!([
                    font.is_fake_bold(),
                    font.is_fake_italic(),
                    font.is_bold(),
                    font.is_italic()
                ]));
            }
            Ok(json!({"states": states}))
        }
        "em_size" => {
            keys(input, &["op", "font", "size"], &[])?;
            let mut holder = make_font(field(input, "font")?, true)?;
            let size = from_bits(field(input, "size")?, "size")?;
            Ok(json!({"value": bits(outline_of(&mut holder)?.get_em_size(size))}))
        }
        "baseline_offset" => {
            keys(input, &["op", "font", "flags"], &[])?;
            let holder = make_font(field(input, "font")?, true)?;
            let flags = flags_value(field(input, "flags")?)?;
            Ok(json!({"value": bits(holder.font.get_super_sub_baseline_offset(flags))}))
        }
        "face_size" | "subscript_size" => {
            keys(input, &["op", "font", "size"], &[])?;
            let mut holder = make_font(field(input, "font")?, true)?;
            let size = size_argument(input)?;
            let font = outline_of(&mut holder)?;
            let value = if op == "face_size" {
                font.face_size_of(size)?
            } else {
                font.subscript_size_of(size)?
            };
            Ok(json!({"value": value}))
        }
        _ => Err("unknown op".into()),
    }
}

/// One request line to one reply line, as the C++ oracle's `Dispatch`.
pub fn dispatch(line: &str) -> Value {
    dispatch_with(line, Options::default())
}

/// [`dispatch`] with explicit process options.
pub fn dispatch_with(line: &str, options: Options) -> Value {
    let mut unit = Value::Null;
    let mut case = Value::Null;
    let failure = |unit: Value, case: Value, kind: &str, message: &str| json!({"case":case,"error":{"kind":kind,"message":message},"unit":unit});
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
    if keys(&request, &["unit", "case", "input"], &[]).is_err()
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
    match run(&request["input"], options) {
        Ok(result) => json!({"case":case,"result":result,"unit":unit}),
        Err(Error::Invalid(message)) => failure(unit, case, "invalid_input", &message),
        Err(Error::Unported {
            upstream,
            unit: owner,
        }) => {
            json!({"case":case,"error":{"kind":"unported","unit":owner,"upstream":upstream},"unit":unit})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(input: Value) -> Value {
        dispatch(&json!({"unit": UNIT, "case": "t", "input": input}).to_string())
    }

    #[test]
    fn insert_formats_an_outline_font() {
        let font = json!({"kind": "outline", "name": [0x41, 0xE9], "file": [], "face": {"style_flags": 3},
                          "fake_bold": false, "fake_ital": false});
        let stream = json!({"state": {"bad": false, "eof": false, "fail": false},
                            "exceptions": {"bad": false, "eof": false, "fail": false},
                            "flags": [], "width": 0, "fill": 32,
                            "buffer": {"capacity": null, "sync_fails": false}});
        let out = reply(json!({"op": "insert", "font": font, "pointer": true, "stream": stream}));
        assert_eq!(
            out["result"]["bytes"],
            json!(hex(b"[Font \"A\xc3\xa9\" outline bold italic]"))
        );
    }

    #[test]
    fn refusals_and_deferrals() {
        let base = json!({"kind": "base", "name": []});
        assert_eq!(
            reply(json!({"op": "name_as_token", "font": base}))["result"]["bytes"],
            json!("")
        );
        let outline = json!({"kind": "outline", "name": [], "file": [], "face": null,
                             "fake_bold": false, "fake_ital": false});
        assert_eq!(
            reply(json!({"op": "face_size", "font": outline, "size": 23967452}))["error"]
                ["message"],
            json!(UNDEFINED)
        );
        assert_eq!(
            reply(json!({"op": "face_size", "font": outline, "size": 23967451}))["result"]["value"],
            json!(2147483609)
        );
        let draw = reply(
            json!({"op": "draw", "font": base, "text": [], "position": [0, 0],
                                "mouse_pos": null, "active_url": null}),
        );
        assert_eq!(draw["error"]["kind"], json!("unported"));
        assert_eq!(draw["error"]["unit"], json!(kicad_font::FONT_CPP_UNIT));
    }

    fn unitbuf_stream(mask: Value) -> Value {
        json!({"state": {"bad": false, "eof": false, "fail": false}, "exceptions": mask,
               "flags": ["internal", "unitbuf"], "width": 3, "fill": 113,
               "buffer": {"capacity": null, "sync_fails": true}})
    }

    #[test]
    fn failing_unitbuf_sync_returns_or_terminates_by_mask() {
        let font = json!({"kind": "scripted", "name": [0x4B], "stroke": true, "outline": true,
                          "bold": false, "italic": true});
        // The review's control insert-random-1871: an eof-only mask does not intersect the
        // badbit the failing sync sets, so the eight insertions return (each syncs).
        let eof = json!({"bad": false, "eof": true, "fail": false});
        let out = reply(json!({"op": "insert", "font": font, "pointer": true,
                               "stream": unitbuf_stream(eof)}));
        assert_eq!(out["result"]["outcome"], json!("returned"));
        assert_eq!(out["result"]["bytes"], json!(hex(b"[Font \"")));
        assert_eq!(out["result"]["sync_calls"], json!(8));
        assert_eq!(
            out["result"]["state"],
            json!({"bad": true, "eof": false, "fail": true})
        );
        // With badbit in the mask the first ~sentry's setstate( badbit ) terminates.
        let bad = json!({"bad": true, "eof": false, "fail": false});
        let out = reply(json!({"op": "insert", "font": font, "pointer": false,
                               "stream": unitbuf_stream(bad)}));
        assert_eq!(out["result"]["outcome"], json!("terminate"));
        assert_eq!(out["result"]["bytes"], json!(hex(b"[Font \"")));
        assert_eq!(out["result"]["sync_calls"], json!(1));
        assert_eq!(out["result"]["width"], json!(0));
    }

    #[test]
    fn assertion_defers_or_continues_with_a_returning_host() {
        let font = json!({"kind": "base", "name": [0x41, 0x110000]});
        let request = json!({"unit": UNIT, "case": "t",
                             "input": {"op": "name_as_token", "font": font}})
        .to_string();
        let deferred = dispatch(&request);
        assert_eq!(
            deferred["error"],
            json!({"kind": "unported", "unit": UNIT, "upstream": "wxOnAssert"})
        );
        let returning = dispatch_with(
            &request,
            Options {
                assert_host: AssertHost::Returning,
                ..Options::default()
            },
        );
        assert_eq!(
            returning["result"],
            json!({"bytes": "", "assertions": [{"line": 1165, "function": "FromWChar",
                   "condition": "\"Assert failure\"",
                   "message": "trying to encode undefined Unicode character"}]})
        );
        // A first insertion that throws ends the operator before the name is converted.
        let stream = json!({"state": {"bad": true, "eof": false, "fail": false},
                            "exceptions": {"bad": false, "eof": false, "fail": true},
                            "flags": [], "width": 0, "fill": 32,
                            "buffer": {"capacity": null, "sync_fails": false}});
        let out = reply(json!({"op": "insert", "font": font, "pointer": false, "stream": stream}));
        assert_eq!(out["result"]["outcome"], json!("failure"));
        assert_eq!(out["result"]["assertions"], json!([]));
    }
}
