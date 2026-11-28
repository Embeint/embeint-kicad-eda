// SPDX-License-Identifier: GPL-3.0-or-later
//! The scripted host: the counterpart of the `__wrap_` fakes in
//! `port/oracle/shims/kiplatform_unix_ui_p001.cpp`. Each method records the call exactly as the
//! fake does and answers from the case's host script with the same validation and messages;
//! handles are numbered in creation order with the same counter.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use kicad_kiplatform_unix::host::{
    Attribute, Conv, List, Mapping, PendingLookup, PendingStore, PrintSignals, ProxyList,
    SecretSchema, Signal, StatBuf, Trace, UnixHost, WxAssert,
};
use kicad_kiplatform_unix::{GStr, Ptr, UnixError};
use kicad_map_helpers::{WChar, WxString};
use serde_json::{json, Map, Value};

use crate::json::{
    bits, check_keys, hex, read_bits, read_bool, read_c_text, read_hex, read_integer, read_text,
    units, Refusal, MAX_ENTRIES, MAX_HANDLES, MAX_REPLIES,
};

/// A scripted converter rule (`SCRIPTED_CONV`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Bytes and units 0..0x7F.
    Ascii,
    /// Bytes and units 0..0xFF.
    Latin1,
    /// Shortest-form UTF-8 of U+0000..U+10FFFF without surrogates.
    Utf8,
    /// Every conversion of at least one element fails.
    Fail,
}

impl Mode {
    fn read(value: &Value, name: &str) -> Result<Self, Refusal> {
        match read_text(value, name)? {
            "ascii" => Ok(Self::Ascii),
            "latin1" => Ok(Self::Latin1),
            "utf8" => Ok(Self::Utf8),
            "fail" => Ok(Self::Fail),
            _ => Err(Refusal::Input(format!("unknown converter mode: {name}"))),
        }
    }

    fn decode(self, bytes: &[u8]) -> Option<Vec<WChar>> {
        let mut out = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            let c = u32::from(*bytes.get(i)?);
            match self {
                Self::Fail => return None,
                Self::Ascii => {
                    if c >= 0x80 {
                        return None;
                    }
                    out.push(c as WChar);
                    i += 1;
                }
                Self::Latin1 => {
                    out.push(c as WChar);
                    i += 1;
                }
                Self::Utf8 => {
                    let (n, mut code, min) = if c < 0x80 {
                        (1, c, 0)
                    } else if c & 0xE0 == 0xC0 {
                        (2, c & 0x1F, 0x80)
                    } else if c & 0xF0 == 0xE0 {
                        (3, c & 0x0F, 0x800)
                    } else if c & 0xF8 == 0xF0 {
                        (4, c & 0x07, 0x10000)
                    } else {
                        return None;
                    };
                    if i + n > bytes.len() {
                        return None;
                    }
                    for j in 1..n {
                        let b = u32::from(*bytes.get(i + j)?);
                        if b & 0xC0 != 0x80 {
                            return None;
                        }
                        code = (code << 6) | (b & 0x3F);
                    }
                    if code < min || code > 0x10_FFFF || (0xD800..=0xDFFF).contains(&code) {
                        return None;
                    }
                    out.push(code as WChar);
                    i += n;
                }
            }
        }
        Some(out)
    }

    fn encode(self, units: &[WChar]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for &unit in units {
            let code = unit as u32;
            match self {
                Self::Fail => return None,
                Self::Ascii | Self::Latin1 => {
                    if code > if self == Self::Ascii { 0x7F } else { 0xFF } {
                        return None;
                    }
                    out.push(code as u8);
                }
                Self::Utf8 => {
                    if code > 0x10_FFFF || (0xD800..=0xDFFF).contains(&code) {
                        return None;
                    }
                    let mut buf = [0u8; 4];
                    let text = char::from_u32(code)?.encode_utf8(&mut buf);
                    out.extend_from_slice(text.as_bytes());
                }
            }
        }
        Some(out)
    }
}

/// `ITEM`: one Secret Service search result.
#[derive(Clone, Debug, Default)]
struct Item {
    locked: bool,
    has_secret: bool,
    text: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Context {
    has_cairo: bool,
    cairo: Option<Ptr>,
    width: f64,
    height: f64,
}

enum Pending {
    Store(Box<PendingStore>),
    Lookup(Box<PendingLookup>),
}

/// The scripted host of one case.
pub struct ScriptedHost {
    /// The recorded calls, shared with the completion callbacks the case installs.
    pub calls: Rc<RefCell<Vec<Value>>>,
    replies: BTreeMap<String, VecDeque<Value>>,
    env: Map<String, Value>,
    setenv: bool,
    trace: bool,
    libc: Mode,
    file_name: Mode,
    next_handle: u64,
    labels: BTreeMap<u64, String>,
    /// `GtkPrintSettings` contents by handle.
    pub settings: BTreeMap<u64, BTreeMap<Vec<u8>, Vec<u8>>>,
    pages: BTreeMap<u64, (f64, f64)>,
    contexts: BTreeMap<u64, Context>,
    items: BTreeMap<u64, Item>,
    values: BTreeMap<u64, u64>,
    errors: BTreeMap<u64, Vec<u8>>,
    pending: Vec<Pending>,
    connections: Vec<Signal>,
    op_settings: BTreeMap<u64, Ptr>,
    /// The fit flag of the running `PrintPDF` call.
    pub current_fit: bool,
    in_draw_page: bool,
    /// `_exit`: how this process ends when a body terminates it.
    pub exit: fn(i32) -> !,
}

/// The default `_exit`: a scripted host in a shared process never terminates; the shim runs
/// `TerminateWithCode` in a child process instead.
fn abort_exit(_status: i32) -> ! {
    std::process::abort()
}

/// The functions whose replies a host script may queue (`REPLY_FUNCTIONS`).
pub const REPLY_FUNCTIONS: &[&str] = &[
    "GetHandle",
    "access",
    "chmod",
    "close",
    "closedir",
    "fclose",
    "fflush",
    "fileno",
    "fnmatch",
    "fopen",
    "fread",
    "fseek",
    "fstat",
    "fsync",
    "ftell",
    "g_drive_is_removable",
    "g_file_find_enclosing_mount",
    "g_file_get_basename",
    "g_file_get_parent",
    "g_file_get_path",
    "g_file_trash",
    "g_filename_to_uri",
    "g_get_home_dir",
    "g_get_user_cache_dir",
    "g_get_user_config_dir",
    "g_get_user_data_dir",
    "g_mount_get_drive",
    "g_proxy_resolver_get_default",
    "g_proxy_resolver_lookup",
    "gtk_print_operation_new",
    "gtk_print_operation_run",
    "gtk_widget_get_toplevel",
    "gtk_widget_is_toplevel",
    "lstat",
    "madvise",
    "mmap",
    "munmap",
    "open",
    "opendir",
    "poppler_document_get_n_pages",
    "poppler_document_get_page",
    "poppler_document_new_from_file",
    "posix_fadvise",
    "readdir",
    "readlink",
    "realpath",
    "secret_password_clear_sync",
    "secret_password_lookup_finish",
    "secret_password_lookup_sync",
    "secret_password_store_finish",
    "secret_password_store_sync",
    "secret_service_search_sync",
    "setpriority",
    "stat",
    "syscall",
    "wxGetActiveWindow",
    "wxGetTopLevelParent",
];

/// A refusal raised inside a host call, carried through the port as a host error.
fn host_refusal(message: String) -> UnixError {
    // The message is leaked only for the lifetime of the process: the port's error carries a
    // &'static str. Refusals are rare and end the case.
    UnixError::Host(Box::leak(message.into_boxed_str()))
}

type HostResult<T> = Result<T, UnixError>;

fn lift<T>(result: Result<T, Refusal>) -> HostResult<T> {
    result.map_err(|refusal| match refusal {
        Refusal::Input(message) | Refusal::Execution(message) => host_refusal(message),
    })
}

fn schema_json(schema: &SecretSchema) -> Value {
    let attributes: Vec<Value> = schema
        .attributes
        .iter()
        .map(|(name, kind)| json!([String::from_utf8_lossy(name), kind]))
        .collect();
    json!({
        "name": String::from_utf8_lossy(schema.name),
        "flags": schema.flags,
        "attributes": attributes,
        "terminator_type": schema.terminator_type,
    })
}

fn attributes_json(attributes: &[Attribute<'_>]) -> Value {
    Value::from(
        attributes
            .iter()
            .map(|(name, value)| json!([hex(name), hex(value)]))
            .collect::<Vec<_>>(),
    )
}

impl ScriptedHost {
    /// A host for the request's `host` script (`readHost`).
    pub fn new(script: &Value) -> Result<Self, Refusal> {
        let Some(object) = script.as_object() else {
            return Err(Refusal::Input("expected host object".into()));
        };
        let mut host = Self {
            calls: Rc::new(RefCell::new(Vec::new())),
            replies: BTreeMap::new(),
            env: Map::new(),
            setenv: true,
            trace: false,
            libc: Mode::Ascii,
            file_name: Mode::Utf8,
            next_handle: 1,
            labels: BTreeMap::new(),
            settings: BTreeMap::new(),
            pages: BTreeMap::new(),
            contexts: BTreeMap::new(),
            items: BTreeMap::new(),
            values: BTreeMap::new(),
            errors: BTreeMap::new(),
            pending: Vec::new(),
            connections: Vec::new(),
            op_settings: BTreeMap::new(),
            current_fit: true,
            in_draw_page: false,
            exit: abort_exit,
        };
        // nlohmann iterates object keys in sorted order.
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        for key in keys {
            let value = &object[key.as_str()];
            match key.as_str() {
                "libc" => host.libc = Mode::read(value, "libc")?,
                "filename" => host.file_name = Mode::read(value, "filename")?,
                "env" => {
                    let Some(env) = value.as_object().filter(|e| e.len() <= MAX_ENTRIES) else {
                        return Err(Refusal::Input("expected env object".into()));
                    };
                    let mut names: Vec<&String> = env.keys().collect();
                    names.sort();
                    for name in names {
                        let setting = &env[name.as_str()];
                        if !setting.is_null() {
                            crate::json::read_units(setting, "env")?;
                        }
                    }
                    host.env = env.clone();
                }
                "setenv" => host.setenv = read_bool(value, "setenv")?,
                "trace" => host.trace = read_bool(value, "trace")?,
                "replies" => {
                    let Some(replies) = value.as_object() else {
                        return Err(Refusal::Input("expected replies object".into()));
                    };
                    let mut functions: Vec<&String> = replies.keys().collect();
                    functions.sort();
                    for function in functions {
                        if !REPLY_FUNCTIONS.contains(&function.as_str()) {
                            return Err(Refusal::Input(format!(
                                "unknown reply function: {function}"
                            )));
                        }
                        let Some(queue) = replies[function.as_str()]
                            .as_array()
                            .filter(|q| q.len() <= MAX_REPLIES)
                        else {
                            return Err(Refusal::Input(format!(
                                "expected reply array: {function}"
                            )));
                        };
                        host.replies
                            .insert(function.clone(), queue.iter().cloned().collect());
                    }
                }
                _ => return Err(Refusal::Input(format!("unknown host key: {key}"))),
            }
        }
        Ok(host)
    }

    fn record(&self, call: Value) {
        self.calls.borrow_mut().push(call);
    }

    /// `label( pointer, kind )` / `newHandle( kind )`.
    pub fn new_handle(&mut self, kind: &str) -> HostResult<Ptr> {
        if self.next_handle > MAX_HANDLES as u64 {
            return Err(host_refusal("too many handles".into()));
        }
        let id = self.next_handle;
        self.next_handle += 1;
        self.labels.insert(id, format!("{kind}#{id}"));
        Ok(Ptr(id))
    }

    /// `nameOf( pointer )`.
    pub fn name(&self, ptr: Option<Ptr>) -> Value {
        match ptr {
            None => Value::Null,
            Some(Ptr(id)) => self
                .labels
                .get(&id)
                .map_or_else(|| Value::from("unknown"), |l| Value::from(l.as_str())),
        }
    }

    fn reply(&mut self, function: &str) -> HostResult<Value> {
        match self.replies.get_mut(function).and_then(VecDeque::pop_front) {
            Some(value) => Ok(value),
            None => Err(host_refusal(format!("missing reply: {function}"))),
        }
    }

    fn reply_integer(&mut self, function: &str, min: i64, max: i64) -> HostResult<i64> {
        let value = self.reply(function)?;
        lift(read_integer(&value, &format!("reply {function}"), min, max))
    }

    fn reply_i32(&mut self, function: &str) -> HostResult<i32> {
        Ok(self.reply_integer(function, i64::from(i32::MIN), i64::from(i32::MAX))? as i32)
    }

    fn reply_bool(&mut self, function: &str) -> HostResult<bool> {
        let value = self.reply(function)?;
        lift(read_bool(&value, &format!("reply {function}")))
    }

    fn reply_c_text(&mut self, function: &str) -> HostResult<Option<Vec<u8>>> {
        let value = self.reply(function)?;
        if value.is_null() {
            return Ok(None);
        }
        Ok(Some(lift(read_c_text(
            &value,
            &format!("reply {function}"),
        ))?))
    }

    fn optional_handle(&mut self, function: &str, kind: &str) -> HostResult<Option<Ptr>> {
        if self.reply_bool(function)? {
            Ok(Some(self.new_handle(kind)?))
        } else {
            Ok(None)
        }
    }

    fn new_error(&mut self, message: &Value, name: &str) -> HostResult<Option<Ptr>> {
        if message.is_null() {
            return Ok(None);
        }
        let bytes = lift(read_c_text(message, name))?;
        let ptr = self.new_handle("error")?;
        self.errors.insert(ptr.0, bytes);
        Ok(Some(ptr))
    }

    fn new_gstr(&mut self, bytes: Vec<u8>) -> HostResult<GStr> {
        Ok(GStr {
            ptr: self.new_handle("gstr")?,
            bytes,
        })
    }

    fn checked(&self, value: &Value, fields: &[&str], place: &str) -> HostResult<()> {
        lift(check_keys(value, fields, place))
    }

    fn reply_stat(&mut self, function: &str) -> HostResult<(i32, StatBuf)> {
        let value = self.reply(function)?;
        let name = format!("reply {function}");
        self.checked(&value, &["result", "mode", "mtime", "size"], &name)?;
        let result = lift(read_integer(
            &value["result"],
            &name,
            i64::from(i32::MIN),
            i64::from(i32::MAX),
        ))?;
        let mode = lift(read_integer(&value["mode"], &name, 0, i64::from(u32::MAX)))?;
        let mtime = lift(read_integer(&value["mtime"], &name, i64::MIN, i64::MAX))?;
        let size = lift(read_integer(&value["size"], &name, i64::MIN, i64::MAX))?;
        Ok((
            result as i32,
            StatBuf {
                mode: mode as u32,
                mtime,
                size,
            },
        ))
    }

    fn settings_of(&mut self, ptr: Ptr) -> HostResult<&mut BTreeMap<Vec<u8>, Vec<u8>>> {
        self.settings
            .get_mut(&ptr.0)
            .ok_or(UnixError::Host("unknown print settings"))
    }

    /// `newSettings( entries )`.
    pub fn new_settings(&mut self, entries: BTreeMap<Vec<u8>, Vec<u8>>) -> HostResult<Ptr> {
        let ptr = self.new_handle("settings")?;
        self.settings.insert(ptr.0, entries);
        Ok(ptr)
    }

    /// The settings contents as the adapter reports them.
    pub fn settings_json(&self, ptr: Ptr) -> Value {
        let entries = self.settings.get(&ptr.0).cloned().unwrap_or_default();
        Value::from(
            entries
                .iter()
                .map(|(k, v)| json!([hex(k), hex(v)]))
                .collect::<Vec<_>>(),
        )
    }

    /// `completePending( count )`: complete the first pending asynchronous calls in order.
    pub fn complete_pending(&mut self, count: usize) -> HostResult<()> {
        let pending: Vec<Pending> = std::mem::take(&mut self.pending);
        for (index, call) in pending.into_iter().enumerate() {
            if index >= count {
                break;
            }
            let result = self.new_handle("result")?;
            match call {
                Pending::Store(store) => {
                    kicad_kiplatform_unix::secrets::store_finished(self, None, result, store)?;
                }
                Pending::Lookup(lookup) => {
                    kicad_kiplatform_unix::secrets::lookup_finished(self, None, result, lookup)?;
                }
            }
        }
        Ok(())
    }

    /// The `wxString` a scripted `wxConvLibc` makes of an ASCII literal, as wx's logging and
    /// formatting convert their narrow strings.
    fn libc_literal(&self, text: &[u8]) -> Option<Vec<WChar>> {
        self.libc.decode(text)
    }

    /// A narrow literal converted with `wxConvLibc`, as UTF-8 text (empty when it fails).
    fn libc_text(&self, text: &str) -> String {
        self.libc_literal(text.as_bytes())
            .map(|units| {
                units
                    .iter()
                    .map(|&u| char::from_u32(u as u32).unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `%s` of a narrow argument in a wide format: converted with `wxConvLibc`, or `(null)`.
    fn libc_argument(&self, bytes: &[u8]) -> Vec<WChar> {
        let text = kicad_kiplatform_unix::c_str(bytes);
        self.libc
            .decode(text)
            .unwrap_or_else(|| b"(null)".iter().map(|&b| WChar::from(b)).collect())
    }
}

fn literal(text: &str) -> Vec<WChar> {
    text.bytes().map(WChar::from).collect()
}

impl UnixHost for ScriptedHost {
    fn wx_get_env(&mut self, var: &WxString, want_value: bool) -> HostResult<Option<WxString>> {
        self.record(json!({"fn": "wxGetEnv", "var": units(var), "value_ptr": want_value}));
        let name: String = var
            .units()
            .iter()
            .map(|&u| char::from_u32(u as u32).unwrap_or('\u{FFFD}'))
            .collect();
        match self.env.get(&name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => {
                if !want_value {
                    return Ok(Some(WxString::new()));
                }
                Ok(Some(lift(crate::json::read_units(value, "env"))?))
            }
        }
    }

    fn wx_set_env(&mut self, variable: &WxString, value: &WxString) -> HostResult<bool> {
        self.record(json!({"fn": "wxSetEnv", "var": units(variable), "value": units(value)}));
        Ok(self.setenv)
    }

    fn mb_to_wc(&mut self, conv: Conv, bytes: &[u8]) -> HostResult<Option<Vec<WChar>>> {
        let mode = match conv {
            Conv::Libc => self.libc,
            Conv::FileName => self.file_name,
        };
        // A NUL-terminated input converts its terminator too, so FAIL fails even when empty.
        if mode == Mode::Fail {
            return Ok(None);
        }
        Ok(mode.decode(bytes))
    }

    fn wc_to_mb(&mut self, conv: Conv, units: &[WChar]) -> HostResult<Option<Vec<u8>>> {
        let mode = match conv {
            Conv::Libc => self.libc,
            Conv::FileName => self.file_name,
        };
        Ok(mode.encode(units))
    }

    fn towlower(&mut self, c: WChar) -> HostResult<WChar> {
        // glibc's "C" locale maps only ASCII letters.
        Ok(if (WChar::from(b'A')..=WChar::from(b'Z')).contains(&c) {
            c + 32
        } else {
            c
        })
    }

    fn on_assert(&mut self, assert: &WxAssert) -> HostResult<()> {
        // The narrow __FUNCTION__ and condition become wxStrings through wxConvLibc (empty when
        // it fails); the message is a wide literal. The adapter's handler reports them as UTF-8.
        let function = self.libc_text(assert.function);
        let condition = self.libc_text(assert.condition);
        self.record(json!({
            "fn": "wxOnAssert",
            "line": assert.line,
            "function": function,
            "condition": condition,
            "message": assert.message,
        }));
        Ok(())
    }

    fn wx_log_error(&mut self, msgid: &str, arg: &str) -> HostResult<()> {
        // _( msgid ) converts the literal with wxConvLibc; the message is the format with its
        // %s replaced by the (wide) argument.
        let format = self.libc_literal(msgid.as_bytes()).unwrap_or_default();
        let mut message = Vec::new();
        let mut i = 0usize;
        while i < format.len() {
            if format.get(i) == Some(&WChar::from(b'%'))
                && format.get(i + 1) == Some(&WChar::from(b's'))
            {
                message.extend(literal(arg));
                i += 2;
            } else {
                message.push(format[i]);
                i += 1;
            }
        }
        self.record(json!({"fn": "wxLogError", "message": message}));
        Ok(())
    }

    fn wx_log_trace(&mut self, mask: &str, trace: Trace<'_>) -> HostResult<()> {
        // The mask is converted with wxConvLibc before IsAllowedTraceMask.
        if !self.trace || self.libc_literal(mask.as_bytes()).as_deref() != Some(&literal(mask)[..])
        {
            return Ok(());
        }
        // wxLog prefixes a trace message with its mask: "(mask) ".
        let mut message = literal("(");
        message.extend(literal(mask));
        message.extend(literal(") "));
        match trace {
            Trace::PopplerLoad {
                file,
                message: text,
            } => {
                message.extend(literal("Poppler could not load "));
                message.extend(self.libc_argument(file));
                message.extend(literal(": "));
                message.extend(self.libc_argument(text));
            }
            Trace::GtkPrint { message: text } => {
                message.extend(literal("GTK print failed: "));
                message.extend(self.libc_argument(text));
            }
        }
        self.record(json!({"fn": "wxLogTrace", "message": message}));
        Ok(())
    }

    fn wx_get_active_window(&mut self) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "wxGetActiveWindow"}));
        self.optional_handle("wxGetActiveWindow", "window")
    }

    fn wx_get_top_level_parent(&mut self, win: Option<Ptr>) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "wxGetTopLevelParent", "win": self.name(win)}));
        self.optional_handle("wxGetTopLevelParent", "window")
    }

    fn wx_window_get_handle(&mut self, win: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "GetHandle", "win": self.name(Some(win))}));
        let value = self.reply("GetHandle")?;
        if lift(read_bool(&value, "reply GetHandle"))? {
            Ok(Some(self.new_handle("widget")?))
        } else {
            Ok(None)
        }
    }

    fn g_log_set_writer_func(&mut self, writer: fn(u32, usize) -> u32) -> HostResult<()> {
        // The fake calls the writer with G_LOG_LEVEL_MESSAGE, no fields and null user data.
        let result = writer(1 << 5, 0);
        self.record(json!({
            "fn": "g_log_set_writer_func",
            "writer_result": result,
            "user_data_null": true,
            "destroy_null": true,
        }));
        Ok(())
    }

    fn g_error_message(&mut self, error: Ptr) -> HostResult<Vec<u8>> {
        self.errors
            .get(&error.0)
            .cloned()
            .ok_or(UnixError::Host("unknown GError"))
    }

    fn g_error_free(&mut self, error: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "g_error_free", "error": self.name(Some(error))}));
        Ok(())
    }

    fn g_clear_error(&mut self, error: Option<Ptr>) -> HostResult<()> {
        self.record(json!({"fn": "g_clear_error", "error": self.name(error)}));
        Ok(())
    }

    fn g_object_unref(&mut self, object: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "g_object_unref", "object": self.name(Some(object))}));
        Ok(())
    }

    fn g_free(&mut self, mem: Option<Ptr>) -> HostResult<()> {
        self.record(json!({"fn": "g_free", "mem": self.name(mem)}));
        Ok(())
    }

    fn g_strfreev(&mut self, list: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "g_strfreev", "list": self.name(Some(list))}));
        Ok(())
    }

    fn g_get_user_data_dir(&mut self) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "g_get_user_data_dir"}));
        self.reply_c_text("g_get_user_data_dir")
    }

    fn g_get_user_config_dir(&mut self) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "g_get_user_config_dir"}));
        self.reply_c_text("g_get_user_config_dir")
    }

    fn g_get_user_cache_dir(&mut self) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "g_get_user_cache_dir"}));
        self.reply_c_text("g_get_user_cache_dir")
    }

    fn g_get_home_dir(&mut self) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "g_get_home_dir"}));
        self.reply_c_text("g_get_home_dir")
    }

    fn g_filename_to_uri(&mut self, filename: &[u8]) -> HostResult<Option<GStr>> {
        self.record(json!({
            "fn": "g_filename_to_uri",
            "filename": hex(filename),
            "hostname_null": true,
            "error_null": true,
        }));
        match self.reply_c_text("g_filename_to_uri")? {
            None => Ok(None),
            Some(bytes) => Ok(Some(self.new_gstr(bytes)?)),
        }
    }

    fn g_signal_connect(&mut self, instance: Ptr, signal: Signal) -> HostResult<u64> {
        self.record(json!({
            "fn": "g_signal_connect",
            "instance": self.name(Some(instance)),
            "signal": signal.name(),
            "destroy_null": true,
            "flags": 0,
        }));
        self.connections.push(signal);
        Ok(self.connections.len() as u64)
    }

    fn g_file_new_for_path(&mut self, path: &[u8]) -> HostResult<Ptr> {
        self.record(json!({"fn": "g_file_new_for_path", "path": hex(path)}));
        self.new_handle("file")
    }

    fn g_file_new_for_uri(&mut self, uri: &[u8]) -> HostResult<Ptr> {
        self.record(json!({"fn": "g_file_new_for_uri", "uri": hex(uri)}));
        self.new_handle("file")
    }

    fn g_file_trash(&mut self, file: Ptr) -> HostResult<(bool, Option<Ptr>)> {
        self.record(json!({
            "fn": "g_file_trash",
            "file": self.name(Some(file)),
            "cancellable_null": true,
            "error_null": false,
        }));
        let value = self.reply("g_file_trash")?;
        self.checked(&value, &["result", "error"], "reply g_file_trash")?;
        let result = lift(read_bool(&value["result"], "reply g_file_trash"))?;
        let error = self.new_error(&value["error"], "reply g_file_trash")?;
        Ok((result, error))
    }

    fn g_file_find_enclosing_mount(&mut self, file: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({
            "fn": "g_file_find_enclosing_mount",
            "file": self.name(Some(file)),
            "cancellable_null": true,
            "error_null": true,
        }));
        self.optional_handle("g_file_find_enclosing_mount", "mount")
    }

    fn g_mount_get_drive(&mut self, mount: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "g_mount_get_drive", "mount": self.name(Some(mount))}));
        self.optional_handle("g_mount_get_drive", "drive")
    }

    fn g_drive_is_removable(&mut self, drive: Ptr) -> HostResult<bool> {
        self.record(json!({"fn": "g_drive_is_removable", "drive": self.name(Some(drive))}));
        self.reply_bool("g_drive_is_removable")
    }

    fn g_file_get_parent(&mut self, file: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "g_file_get_parent", "file": self.name(Some(file))}));
        self.optional_handle("g_file_get_parent", "file")
    }

    fn g_file_get_basename(&mut self, file: Ptr) -> HostResult<Option<GStr>> {
        self.record(json!({"fn": "g_file_get_basename", "file": self.name(Some(file))}));
        match self.reply_c_text("g_file_get_basename")? {
            None => Ok(None),
            Some(bytes) => Ok(Some(self.new_gstr(bytes)?)),
        }
    }

    fn g_file_get_path(&mut self, file: Ptr) -> HostResult<Option<GStr>> {
        self.record(json!({"fn": "g_file_get_path", "file": self.name(Some(file))}));
        match self.reply_c_text("g_file_get_path")? {
            None => Ok(None),
            Some(bytes) => Ok(Some(self.new_gstr(bytes)?)),
        }
    }

    fn g_proxy_resolver_get_default(&mut self) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "g_proxy_resolver_get_default"}));
        self.optional_handle("g_proxy_resolver_get_default", "resolver")
    }

    fn g_proxy_resolver_lookup(
        &mut self,
        resolver: Ptr,
        uri: &[u8],
    ) -> HostResult<(Option<ProxyList>, Option<Ptr>)> {
        self.record(json!({
            "fn": "g_proxy_resolver_lookup",
            "resolver": self.name(Some(resolver)),
            "uri": hex(uri),
            "cancellable_null": true,
            "error_null": false,
        }));
        let value = self.reply("g_proxy_resolver_lookup")?;
        self.checked(&value, &["list", "error"], "reply g_proxy_resolver_lookup")?;
        let mut list = None;
        if !value["list"].is_null() {
            let Some(entries) = value["list"].as_array().filter(|e| e.len() <= MAX_ENTRIES) else {
                return Err(host_refusal("bad reply g_proxy_resolver_lookup".into()));
            };
            let mut out = Vec::new();
            for entry in entries {
                out.push(lift(read_c_text(entry, "reply g_proxy_resolver_lookup"))?);
            }
            list = Some((self.new_handle("strv")?, out));
        }
        let error = self.new_error(&value["error"], "reply g_proxy_resolver_lookup")?;
        Ok((list, error))
    }

    fn syscall_gettid(&mut self) -> HostResult<i64> {
        self.record(json!({"fn": "syscall", "number": 186}));
        self.reply_integer("syscall", i64::MIN, i64::MAX)
    }

    fn setpriority(&mut self, which: i32, who: u32, prio: i32) -> HostResult<i32> {
        self.record(json!({"fn": "setpriority", "which": which, "who": who, "prio": prio}));
        self.reply_i32("setpriority")
    }

    fn exit(&mut self, status: i32) -> ! {
        (self.exit)(status)
    }

    fn realpath(&mut self, path: &[u8]) -> HostResult<Option<GStr>> {
        self.record(json!({"fn": "realpath", "path": hex(path), "resolved_null": true}));
        match self.reply_c_text("realpath")? {
            None => Ok(None),
            Some(bytes) => Ok(Some(GStr {
                ptr: self.new_handle("cstr")?,
                bytes,
            })),
        }
    }

    fn free(&mut self, ptr: Option<Ptr>) -> HostResult<()> {
        if ptr.is_some() {
            self.record(json!({"fn": "free", "ptr": self.name(ptr)}));
        }
        Ok(())
    }

    fn fopen(&mut self, path: &[u8], mode: &[u8]) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "fopen", "path": hex(path), "mode": hex(mode)}));
        self.optional_handle("fopen", "FILE")
    }

    fn fileno(&mut self, stream: Ptr) -> HostResult<i32> {
        self.record(json!({"fn": "fileno", "stream": self.name(Some(stream))}));
        self.reply_i32("fileno")
    }

    fn posix_fadvise(&mut self, fd: i32, offset: i64, len: i64, advice: i32) -> HostResult<i32> {
        self.record(json!({"fn": "posix_fadvise", "fd": fd, "offset": offset, "len": len, "advice": advice}));
        self.reply_i32("posix_fadvise")
    }

    fn fclose(&mut self, stream: Ptr) -> HostResult<i32> {
        self.record(json!({"fn": "fclose", "stream": self.name(Some(stream))}));
        self.reply_i32("fclose")
    }

    fn fflush(&mut self, stream: Ptr) -> HostResult<i32> {
        self.record(json!({"fn": "fflush", "stream": self.name(Some(stream))}));
        self.reply_i32("fflush")
    }

    fn fsync(&mut self, fd: i32) -> HostResult<i32> {
        self.record(json!({"fn": "fsync", "fd": fd}));
        self.reply_i32("fsync")
    }

    fn stat(&mut self, path: &[u8]) -> HostResult<(i32, StatBuf)> {
        self.record(json!({"fn": "stat", "path": hex(path)}));
        self.reply_stat("stat")
    }

    fn lstat(&mut self, path: &[u8]) -> HostResult<(i32, StatBuf)> {
        self.record(json!({"fn": "lstat", "path": hex(path)}));
        self.reply_stat("lstat")
    }

    fn chmod(&mut self, path: &[u8], mode: u32) -> HostResult<i32> {
        self.record(json!({"fn": "chmod", "path": hex(path), "mode": mode}));
        self.reply_i32("chmod")
    }

    fn opendir(&mut self, name: &[u8]) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "opendir", "name": hex(name)}));
        self.optional_handle("opendir", "DIR")
    }

    fn readdir(&mut self, dir: Ptr) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "readdir", "dir": self.name(Some(dir))}));
        let name = self.reply_c_text("readdir")?;
        if name.as_ref().is_some_and(|n| n.len() >= 256) {
            return Err(host_refusal("bad reply readdir".into()));
        }
        Ok(name)
    }

    fn closedir(&mut self, dir: Ptr) -> HostResult<i32> {
        self.record(json!({"fn": "closedir", "dir": self.name(Some(dir))}));
        self.reply_i32("closedir")
    }

    fn readlink(&mut self, path: &[u8], bufsiz: usize) -> HostResult<(isize, Vec<u8>)> {
        self.record(json!({"fn": "readlink", "path": hex(path), "bufsiz": bufsiz}));
        let value = self.reply("readlink")?;
        self.checked(&value, &["result", "bytes"], "reply readlink")?;
        let result = lift(read_integer(
            &value["result"],
            "reply readlink",
            -1,
            bufsiz as i64,
        ))?;
        let bytes = lift(read_hex(&value["bytes"], "reply readlink"))?;
        if bytes.len() > bufsiz || (result > 0 && bytes.len() != result as usize) {
            return Err(host_refusal("bad reply readlink".into()));
        }
        Ok((result as isize, bytes))
    }

    fn fnmatch(&mut self, pattern: &[u8], string: &[u8], flags: i32) -> HostResult<i32> {
        self.record(json!({"fn": "fnmatch", "pattern": hex(pattern), "string": hex(string), "flags": flags}));
        self.reply_i32("fnmatch")
    }

    fn open(&mut self, path: &[u8], flags: i32) -> HostResult<i32> {
        self.record(json!({"fn": "open", "path": hex(path), "flags": flags}));
        self.reply_i32("open")
    }

    fn fstat(&mut self, fd: i32) -> HostResult<(i32, StatBuf)> {
        self.record(json!({"fn": "fstat", "fd": fd}));
        self.reply_stat("fstat")
    }

    fn close(&mut self, fd: i32) -> HostResult<i32> {
        self.record(json!({"fn": "close", "fd": fd}));
        self.reply_i32("close")
    }

    fn mmap(
        &mut self,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> HostResult<Option<Mapping>> {
        self.record(json!({
            "fn": "mmap",
            "addr_null": true,
            "length": length,
            "prot": prot,
            "flags": flags,
            "fd": fd,
            "offset": offset,
        }));
        let value = self.reply("mmap")?;
        if value.is_null() {
            return Ok(None);
        }
        let bytes = lift(read_hex(&value, "reply mmap"))?;
        if bytes.len() != length {
            return Err(host_refusal("bad reply mmap".into()));
        }
        Ok(Some(Mapping {
            ptr: self.new_handle("map")?,
            bytes,
        }))
    }

    fn madvise(&mut self, addr: Ptr, length: usize, advice: i32) -> HostResult<i32> {
        self.record(json!({"fn": "madvise", "addr": self.name(Some(addr)), "length": length, "advice": advice}));
        self.reply_i32("madvise")
    }

    fn munmap(&mut self, addr: Ptr, length: usize) -> HostResult<i32> {
        self.record(json!({"fn": "munmap", "addr": self.name(Some(addr)), "length": length}));
        self.reply_i32("munmap")
    }

    fn access(&mut self, path: &[u8], mode: i32) -> HostResult<i32> {
        self.record(json!({"fn": "access", "path": hex(path), "mode": mode}));
        self.reply_i32("access")
    }

    fn secret_password_store_sync(
        &mut self,
        schema: &SecretSchema,
        collection: &[u8],
        label: &[u8],
        password: &[u8],
        attributes: &[Attribute<'_>],
    ) -> HostResult<(bool, Option<Ptr>)> {
        self.record(json!({
            "fn": "secret_password_store_sync",
            "schema": schema_json(schema),
            "collection": hex(collection),
            "label": hex(label),
            "password": hex(password),
            "cancellable_null": true,
            "error_null": false,
            "attributes": attributes_json(attributes),
        }));
        let value = self.reply("secret_password_store_sync")?;
        self.checked(
            &value,
            &["result", "error"],
            "reply secret_password_store_sync",
        )?;
        let result = lift(read_bool(
            &value["result"],
            "reply secret_password_store_sync",
        ))?;
        let error = self.new_error(&value["error"], "reply secret_password_store_sync")?;
        Ok((result, error))
    }

    fn secret_password_store(
        &mut self,
        schema: &SecretSchema,
        collection: &[u8],
        label: &[u8],
        password: &[u8],
        attributes: &[Attribute<'_>],
        pending: Box<PendingStore>,
    ) -> HostResult<()> {
        self.record(json!({
            "fn": "secret_password_store",
            "schema": schema_json(schema),
            "collection": hex(collection),
            "label": hex(label),
            "password": hex(password),
            "cancellable_null": true,
            "attributes": attributes_json(attributes),
        }));
        self.pending.push(Pending::Store(pending));
        Ok(())
    }

    fn secret_password_store_finish(&mut self, result: Ptr) -> HostResult<bool> {
        self.record(json!({"fn": "secret_password_store_finish", "result": self.name(Some(result)), "error_null": true}));
        self.reply_bool("secret_password_store_finish")
    }

    fn secret_password_lookup_sync(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
    ) -> HostResult<(Option<GStr>, Option<Ptr>)> {
        self.record(json!({
            "fn": "secret_password_lookup_sync",
            "schema": schema_json(schema),
            "cancellable_null": true,
            "error_null": false,
            "attributes": attributes_json(attributes),
        }));
        let value = self.reply("secret_password_lookup_sync")?;
        self.checked(
            &value,
            &["secret", "error"],
            "reply secret_password_lookup_sync",
        )?;
        let mut secret = None;
        if !value["secret"].is_null() {
            let bytes = lift(read_c_text(
                &value["secret"],
                "reply secret_password_lookup_sync",
            ))?;
            secret = Some(self.new_gstr(bytes)?);
        }
        let error = self.new_error(&value["error"], "reply secret_password_lookup_sync")?;
        Ok((secret, error))
    }

    fn secret_password_lookup(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
        pending: Box<PendingLookup>,
    ) -> HostResult<()> {
        self.record(json!({
            "fn": "secret_password_lookup",
            "schema": schema_json(schema),
            "cancellable_null": true,
            "attributes": attributes_json(attributes),
        }));
        self.pending.push(Pending::Lookup(pending));
        Ok(())
    }

    fn secret_password_lookup_finish(&mut self, result: Ptr) -> HostResult<Option<GStr>> {
        self.record(json!({"fn": "secret_password_lookup_finish", "result": self.name(Some(result)), "error_null": true}));
        match self.reply_c_text("secret_password_lookup_finish")? {
            None => Ok(None),
            Some(bytes) => Ok(Some(self.new_gstr(bytes)?)),
        }
    }

    fn secret_attributes_build(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
    ) -> HostResult<Ptr> {
        self.record(json!({
            "fn": "secret_attributes_build",
            "schema": schema_json(schema),
            "attributes": attributes_json(attributes),
        }));
        self.new_handle("table")
    }

    fn secret_service_search_sync(
        &mut self,
        schema: &SecretSchema,
        attributes: Ptr,
        flags: i32,
    ) -> HostResult<(List, Option<Ptr>)> {
        self.record(json!({
            "fn": "secret_service_search_sync",
            "schema": schema_json(schema),
            "service_null": true,
            "attributes": self.name(Some(attributes)),
            "flags": flags,
            "cancellable_null": true,
            "error_null": false,
        }));
        let value = self.reply("secret_service_search_sync")?;
        self.checked(
            &value,
            &["items", "error"],
            "reply secret_service_search_sync",
        )?;
        let Some(specs) = value["items"].as_array().filter(|i| i.len() <= MAX_ENTRIES) else {
            return Err(host_refusal("bad reply secret_service_search_sync".into()));
        };
        let mut items = Vec::new();
        for spec in specs {
            self.checked(
                spec,
                &["locked", "secret"],
                "reply secret_service_search_sync item",
            )?;
            let mut item = Item {
                locked: lift(read_bool(&spec["locked"], "item locked"))?,
                ..Item::default()
            };
            if !spec["secret"].is_null() {
                item.has_secret = true;
                let text = &spec["secret"];
                self.checked(text, &["text"], "item secret")?;
                if !text["text"].is_null() {
                    item.text = Some(lift(read_c_text(&text["text"], "item text"))?);
                }
            }
            let handle = self.new_handle("item")?;
            self.items.insert(handle.0, item);
            items.push(handle);
        }
        let head = if items.is_empty() {
            None
        } else {
            Some(self.new_handle("list")?)
        };
        let error = self.new_error(&value["error"], "reply secret_service_search_sync")?;
        Ok((List { head, items }, error))
    }

    fn g_hash_table_unref(&mut self, table: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "g_hash_table_unref", "table": self.name(Some(table))}));
        Ok(())
    }

    fn secret_item_get_locked(&mut self, item: Ptr) -> HostResult<bool> {
        self.record(json!({"fn": "secret_item_get_locked", "item": self.name(Some(item))}));
        Ok(self.items.get(&item.0).is_some_and(|i| i.locked))
    }

    fn secret_item_get_secret(&mut self, item: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "secret_item_get_secret", "item": self.name(Some(item))}));
        if !self.items.get(&item.0).is_some_and(|i| i.has_secret) {
            return Ok(None);
        }
        let value = self.new_handle("value")?;
        self.values.insert(value.0, item.0);
        Ok(Some(value))
    }

    fn secret_value_get_text(&mut self, value: Ptr) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "secret_value_get_text", "value": self.name(Some(value))}));
        let item = self.values.get(&value.0).and_then(|id| self.items.get(id));
        Ok(item.and_then(|i| i.text.clone()))
    }

    fn secret_value_unref(&mut self, value: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "secret_value_unref", "value": self.name(Some(value))}));
        Ok(())
    }

    fn g_list_free_full_unref(&mut self, list: &List) -> HostResult<()> {
        self.record(json!({"fn": "g_list_free_full", "list": self.name(list.head)}));
        for &item in &list.items {
            self.g_object_unref(item)?;
        }
        Ok(())
    }

    fn secret_password_clear_sync(
        &mut self,
        schema: &SecretSchema,
        attributes: &[Attribute<'_>],
    ) -> HostResult<(bool, Option<Ptr>)> {
        self.record(json!({
            "fn": "secret_password_clear_sync",
            "schema": schema_json(schema),
            "cancellable_null": true,
            "error_null": false,
            "attributes": attributes_json(attributes),
        }));
        let value = self.reply("secret_password_clear_sync")?;
        self.checked(
            &value,
            &["result", "error"],
            "reply secret_password_clear_sync",
        )?;
        let result = lift(read_bool(
            &value["result"],
            "reply secret_password_clear_sync",
        ))?;
        let error = self.new_error(&value["error"], "reply secret_password_clear_sync")?;
        Ok((result, error))
    }

    fn poppler_document_new_from_file(
        &mut self,
        uri: &[u8],
    ) -> HostResult<(Option<Ptr>, Option<Ptr>)> {
        self.record(json!({
            "fn": "poppler_document_new_from_file",
            "uri": hex(uri),
            "password_null": true,
            "error_null": false,
        }));
        let value = self.reply("poppler_document_new_from_file")?;
        self.checked(
            &value,
            &["doc", "error"],
            "reply poppler_document_new_from_file",
        )?;
        let doc = if lift(read_bool(
            &value["doc"],
            "reply poppler_document_new_from_file",
        ))? {
            Some(self.new_handle("doc")?)
        } else {
            None
        };
        let error = self.new_error(&value["error"], "reply poppler_document_new_from_file")?;
        Ok((doc, error))
    }

    fn poppler_document_get_n_pages(&mut self, document: Ptr) -> HostResult<i32> {
        self.record(
            json!({"fn": "poppler_document_get_n_pages", "document": self.name(Some(document))}),
        );
        self.reply_i32("poppler_document_get_n_pages")
    }

    fn poppler_document_get_page(&mut self, document: Ptr, index: i32) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "poppler_document_get_page", "document": self.name(Some(document)), "index": index}));
        let value = self.reply("poppler_document_get_page")?;
        if value.is_null() {
            return Ok(None);
        }
        self.checked(
            &value,
            &["width", "height"],
            "reply poppler_document_get_page",
        )?;
        let width = lift(read_bits(&value["width"], "page width"))?;
        let height = lift(read_bits(&value["height"], "page height"))?;
        let page = self.new_handle("page")?;
        self.pages.insert(page.0, (width, height));
        Ok(Some(page))
    }

    fn poppler_page_get_size(&mut self, page: Ptr) -> HostResult<(f64, f64)> {
        self.record(json!({"fn": "poppler_page_get_size", "page": self.name(Some(page))}));
        let size = self
            .pages
            .get(&page.0)
            .copied()
            .ok_or(UnixError::Host("unknown page"))?;
        if self.in_draw_page && self.current_fit && (size.0 == 0.0 || size.1 == 0.0) {
            return Err(UnixError::Undefined(
                "floating division by zero in draw_page",
            ));
        }
        Ok(size)
    }

    fn poppler_page_render(&mut self, page: Ptr, cairo: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "poppler_page_render", "page": self.name(Some(page)), "cairo": self.name(Some(cairo))}));
        Ok(())
    }

    fn gtk_print_operation_new(&mut self) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "gtk_print_operation_new"}));
        self.optional_handle("gtk_print_operation_new", "op")
    }

    fn gtk_print_operation_set_use_full_page(
        &mut self,
        op: Ptr,
        full_page: bool,
    ) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_operation_set_use_full_page", "op": self.name(Some(op)), "full_page": full_page}));
        Ok(())
    }

    fn gtk_print_operation_set_unit(&mut self, op: Ptr, unit: i32) -> HostResult<()> {
        self.record(
            json!({"fn": "gtk_print_operation_set_unit", "op": self.name(Some(op)), "unit": unit}),
        );
        Ok(())
    }

    fn gtk_print_operation_set_print_settings(&mut self, op: Ptr, settings: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_operation_set_print_settings", "op": self.name(Some(op)), "settings": self.name(Some(settings))}));
        self.op_settings.insert(op.0, settings);
        Ok(())
    }

    fn gtk_print_operation_set_job_name(&mut self, op: Ptr, job_name: &[u8]) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_operation_set_job_name", "op": self.name(Some(op)), "job_name": hex(job_name)}));
        Ok(())
    }

    fn gtk_print_operation_set_current_page(
        &mut self,
        op: Ptr,
        current_page: i32,
    ) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_operation_set_current_page", "op": self.name(Some(op)), "current_page": current_page}));
        Ok(())
    }

    fn gtk_print_operation_run(
        &mut self,
        op: Ptr,
        action: i32,
        parent: Option<Ptr>,
        signals: &mut dyn PrintSignals<Self>,
    ) -> HostResult<(i32, Option<Ptr>)> {
        self.record(json!({
            "fn": "gtk_print_operation_run",
            "op": self.name(Some(op)),
            "action": action,
            "parent": self.name(parent),
            "error_null": false,
        }));
        let value = self.reply("gtk_print_operation_run")?;
        self.checked(
            &value,
            &["context", "emit", "settings", "result", "error"],
            "reply gtk_print_operation_run",
        )?;
        let spec = &value["context"];
        self.checked(
            spec,
            &["cairo", "width", "height"],
            "reply gtk_print_operation_run context",
        )?;
        let context = self.new_handle("context")?;
        let state = Context {
            has_cairo: lift(read_bool(&spec["cairo"], "context cairo"))?,
            cairo: None,
            width: lift(read_bits(&spec["width"], "context width"))?,
            height: lift(read_bits(&spec["height"], "context height"))?,
        };
        self.contexts.insert(context.0, state);
        let Some(emits) = value["emit"].as_array().filter(|e| e.len() <= MAX_ENTRIES) else {
            return Err(host_refusal("bad reply gtk_print_operation_run".into()));
        };
        for emit in emits {
            let signal = lift(
                crate::json::member(emit, "signal").and_then(|s| read_text(s, "emit signal")),
            )?
            .to_owned();
            let mut page = 0i32;
            let kind = match signal.as_str() {
                "begin-print" => {
                    self.checked(emit, &["signal"], "emit")?;
                    Signal::BeginPrint
                }
                "draw-page" | "request-page-setup" => {
                    self.checked(emit, &["signal", "page"], "emit")?;
                    page = lift(read_integer(
                        &emit["page"],
                        "emit page",
                        i64::from(i32::MIN),
                        i64::from(i32::MAX),
                    ))? as i32;
                    if signal == "draw-page" {
                        Signal::DrawPage
                    } else {
                        Signal::RequestPageSetup
                    }
                }
                _ => return Err(host_refusal("unknown signal".into())),
            };
            let connected: Vec<Signal> = self.connections.clone();
            for connection in connected {
                if connection != kind {
                    continue;
                }
                match kind {
                    Signal::BeginPrint => signals.begin_print(self, op, context)?,
                    Signal::DrawPage => {
                        self.in_draw_page = true;
                        signals.draw_page(self, op, context, page)?;
                        self.in_draw_page = false;
                    }
                    Signal::RequestPageSetup => {
                        let setup = self.new_handle("setup")?;
                        signals.request_page_setup(self, op, context, page, setup)?;
                    }
                }
            }
        }
        if !value["settings"].is_null() {
            let entries = lift(read_settings(&value["settings"], "run settings"))?;
            let settings = self.new_settings(entries)?;
            self.op_settings.insert(op.0, settings);
        }
        let result = lift(read_integer(
            &value["result"],
            "run result",
            i64::from(i32::MIN),
            i64::from(i32::MAX),
        ))? as i32;
        let error = self.new_error(&value["error"], "reply gtk_print_operation_run")?;
        Ok((result, error))
    }

    fn gtk_print_operation_get_print_settings(&mut self, op: Ptr) -> HostResult<Option<Ptr>> {
        self.record(
            json!({"fn": "gtk_print_operation_get_print_settings", "op": self.name(Some(op))}),
        );
        Ok(self.op_settings.get(&op.0).copied())
    }

    fn gtk_print_operation_cancel(&mut self, op: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_operation_cancel", "op": self.name(Some(op))}));
        Ok(())
    }

    fn gtk_print_operation_set_n_pages(&mut self, op: Ptr, n_pages: i32) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_operation_set_n_pages", "op": self.name(Some(op)), "n_pages": n_pages}));
        Ok(())
    }

    fn gtk_print_settings_new(&mut self) -> HostResult<Ptr> {
        self.record(json!({"fn": "gtk_print_settings_new"}));
        self.new_settings(BTreeMap::new())
    }

    fn gtk_print_settings_copy(&mut self, other: Ptr) -> HostResult<Ptr> {
        self.record(json!({"fn": "gtk_print_settings_copy", "other": self.name(Some(other))}));
        let entries = self.settings_of(other)?.clone();
        self.new_settings(entries)
    }

    fn gtk_print_settings_get(&mut self, settings: Ptr, key: &[u8]) -> HostResult<Option<Vec<u8>>> {
        self.record(json!({"fn": "gtk_print_settings_get", "settings": self.name(Some(settings)), "key": hex(key)}));
        Ok(self.settings_of(settings)?.get(key).cloned())
    }

    fn gtk_print_settings_set(
        &mut self,
        settings: Ptr,
        key: &[u8],
        value: &[u8],
    ) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_settings_set", "settings": self.name(Some(settings)), "key": hex(key), "value": hex(value)}));
        self.settings_of(settings)?
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn gtk_print_settings_unset(&mut self, settings: Ptr, key: &[u8]) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_settings_unset", "settings": self.name(Some(settings)), "key": hex(key)}));
        self.settings_of(settings)?.remove(key);
        Ok(())
    }

    fn gtk_print_settings_set_print_pages(&mut self, settings: Ptr, pages: i32) -> HostResult<()> {
        self.record(json!({"fn": "gtk_print_settings_set_print_pages", "settings": self.name(Some(settings)), "pages": pages}));
        Ok(())
    }

    fn gtk_print_context_get_cairo_context(&mut self, context: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "gtk_print_context_get_cairo_context", "context": self.name(Some(context))}));
        let state = self
            .contexts
            .get(&context.0)
            .copied()
            .ok_or(UnixError::Host("unknown context"))?;
        if !state.has_cairo {
            return Ok(None);
        }
        if let Some(cairo) = state.cairo {
            return Ok(Some(cairo));
        }
        let cairo = self.new_handle("cairo")?;
        if let Some(stored) = self.contexts.get_mut(&context.0) {
            stored.cairo = Some(cairo);
        }
        Ok(Some(cairo))
    }

    fn gtk_print_context_get_width(&mut self, context: Ptr) -> HostResult<f64> {
        self.record(
            json!({"fn": "gtk_print_context_get_width", "context": self.name(Some(context))}),
        );
        Ok(self.contexts.get(&context.0).map_or(0.0, |c| c.width))
    }

    fn gtk_print_context_get_height(&mut self, context: Ptr) -> HostResult<f64> {
        self.record(
            json!({"fn": "gtk_print_context_get_height", "context": self.name(Some(context))}),
        );
        Ok(self.contexts.get(&context.0).map_or(0.0, |c| c.height))
    }

    fn gtk_page_setup_set_orientation(&mut self, setup: Ptr, orientation: i32) -> HostResult<()> {
        self.record(json!({"fn": "gtk_page_setup_set_orientation", "setup": self.name(Some(setup)), "orientation": orientation}));
        Ok(())
    }

    fn gtk_widget_get_toplevel(&mut self, widget: Ptr) -> HostResult<Option<Ptr>> {
        self.record(json!({"fn": "gtk_widget_get_toplevel", "widget": self.name(Some(widget))}));
        self.optional_handle("gtk_widget_get_toplevel", "widget")
    }

    fn gtk_widget_is_toplevel(&mut self, widget: Option<Ptr>) -> HostResult<bool> {
        self.record(json!({"fn": "gtk_widget_is_toplevel", "widget": self.name(widget)}));
        self.reply_bool("gtk_widget_is_toplevel")
    }

    fn cairo_save(&mut self, cr: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "cairo_save", "cr": self.name(Some(cr))}));
        Ok(())
    }

    fn cairo_restore(&mut self, cr: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "cairo_restore", "cr": self.name(Some(cr))}));
        Ok(())
    }

    fn cairo_translate(&mut self, cr: Ptr, tx: f64, ty: f64) -> HostResult<()> {
        self.record(json!({"fn": "cairo_translate", "cr": self.name(Some(cr)), "tx": bits(tx), "ty": bits(ty)}));
        Ok(())
    }

    fn cairo_scale(&mut self, cr: Ptr, sx: f64, sy: f64) -> HostResult<()> {
        self.record(
            json!({"fn": "cairo_scale", "cr": self.name(Some(cr)), "sx": bits(sx), "sy": bits(sy)}),
        );
        Ok(())
    }

    fn cairo_set_source_rgb(&mut self, cr: Ptr, red: f64, green: f64, blue: f64) -> HostResult<()> {
        self.record(json!({"fn": "cairo_set_source_rgb", "cr": self.name(Some(cr)), "red": bits(red), "green": bits(green), "blue": bits(blue)}));
        Ok(())
    }

    fn cairo_paint(&mut self, cr: Ptr) -> HostResult<()> {
        self.record(json!({"fn": "cairo_paint", "cr": self.name(Some(cr))}));
        Ok(())
    }
}

/// `readSettings`: `[[key, value], ...]` of C strings.
pub fn read_settings(value: &Value, name: &str) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, Refusal> {
    let Some(entries) = value.as_array().filter(|e| e.len() <= MAX_ENTRIES) else {
        return Err(Refusal::Input(format!("bad settings: {name}")));
    };
    let mut out = BTreeMap::new();
    for entry in entries {
        let Some([key, val]) = entry
            .as_array()
            .map(Vec::as_slice)
            .and_then(|s| <&[Value; 2]>::try_from(s).ok())
        else {
            return Err(Refusal::Input(format!("bad settings: {name}")));
        };
        out.insert(read_c_text(key, name)?, read_c_text(val, name)?);
    }
    Ok(out)
}
