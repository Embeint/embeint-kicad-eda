// SPDX-License-Identifier: GPL-3.0-or-later
//! Operation decoding for the shim. Validation order and messages match the C++ adapter
//! exactly; only validation, conversion and calls into the port live here.

use std::cell::RefCell;
use std::rc::Rc;

use kicad_kiplatform::environment::ProxyConfig;
use kicad_kiplatform::io::{
    FileLock, FileLockState, IoPlatform, MappedData, MappedFile, TargetAttrs,
    CLOUD_SYNC_BUFFER_SIZE,
};
use kicad_kiplatform::secrets::{SecretsPlatform, StoreDone};
use kicad_kiplatform::touchpad::{
    BadFunctionCall, TouchpadGesture, TouchpadGestureCallback, TouchpadGestureHandler, WxPoint,
};
use kicad_kiplatform::ui::{NativeDisplayType, UiPlatform};
use kicad_kiplatform_sysinfo::sysinfo::{CpuInfo, GpuInfo, MemoryInfo};
use kicad_map_helpers::{WChar, WxString};
use serde_json::{json, Map, Value};

use crate::host::Recorder;

/// Largest number of `wchar_t` units in one string.
pub const MAX_UNITS: usize = 4096;
/// Largest number of bytes in one byte field.
pub const MAX_BYTES: usize = 4096;
/// Largest number of gestures or handler steps in one request.
pub const MAX_ENTRIES: usize = 64;
/// Number of handler owner slots.
pub const HANDLER_SLOTS: usize = 8;

/// Why a request produced no result: a refused input (`invalid_input`) or a failure of the
/// adapter itself (`execution_error`, the C++ adapter's `std::logic_error`).
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Input(String),
    Execution(String),
}

pub type Result<T> = core::result::Result<T, Refusal>;

fn refuse<T>(message: impl Into<String>) -> Result<T> {
    Err(Refusal::Input(message.into()))
}

fn check_keys(input: &Value, fields: &[&str], place: &str) -> Result<()> {
    let ok = input.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    });
    if ok {
        Ok(())
    } else if place.is_empty() {
        refuse("unexpected or missing field")
    } else {
        refuse(format!("unexpected or missing field: {place}"))
    }
}

/// A member whose presence `check_keys` has established.
fn member<'a>(input: &'a Value, name: &str) -> Result<&'a Value> {
    input
        .get(name)
        .ok_or_else(|| Refusal::Input("unexpected or missing field".into()))
}

fn read_text<'a>(input: &'a Value, name: &str) -> Result<&'a str> {
    input
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Refusal::Input(format!("expected string: {name}")))
}

/// Integer in `[min, max]`; like nlohmann, `-0` is a signed zero and accepted.
fn read_integer(value: &Value, name: &str, min: i64, max: i64) -> Result<i64> {
    let refused = || Refusal::Input(format!("integer out of range: {name}"));
    if let Some(number) = value.as_u64() {
        return i64::try_from(number)
            .ok()
            .filter(|n| *n <= max)
            .ok_or_else(refused);
    }
    value
        .as_i64()
        .filter(|n| (min..=max).contains(n))
        .ok_or_else(refused)
}

fn read_bool(value: &Value, name: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| Refusal::Input(format!("expected boolean: {name}")))
}

fn read_array<'a>(value: &'a Value, name: &str, max: usize) -> Result<&'a Vec<Value>> {
    let array = value
        .as_array()
        .ok_or_else(|| Refusal::Input(format!("expected array: {name}")))?;
    if array.len() > max {
        return refuse(format!("too many entries: {name}"));
    }
    Ok(array)
}

fn read_units(value: &Value, name: &str) -> Result<Vec<WChar>> {
    read_array(value, name, MAX_UNITS)?
        .iter()
        .map(|unit| {
            read_integer(unit, name, i64::from(i32::MIN), i64::from(i32::MAX))
                .and_then(|n| WChar::try_from(n).map_err(|_| Refusal::Input(name.into())))
        })
        .collect()
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn read_bytes(value: &Value, name: &str) -> Result<Vec<u8>> {
    let invalid = || Refusal::Input(format!("invalid hex: {name}"));
    let text = value.as_str().ok_or_else(invalid)?.as_bytes();
    if text.len() % 2 != 0 || text.len() > 2 * MAX_BYTES || text.iter().any(|c| digit(*c).is_none())
    {
        return Err(invalid());
    }
    text.chunks_exact(2)
        .map(|pair| match pair {
            [high, low] => Some((digit(*high)? << 4) | digit(*low)?),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(invalid)
}

fn read_double(value: &Value, name: &str) -> Result<f64> {
    let invalid = || Refusal::Input(format!("invalid double: {name}"));
    let text = value.as_str().ok_or_else(invalid)?;
    if text.len() != 16 || text.bytes().any(|c| digit(c).is_none()) {
        return Err(invalid());
    }
    u64::from_str_radix(text, 16)
        .map(f64::from_bits)
        .map_err(|_| invalid())
}

fn hex(data: &[u8]) -> Value {
    Value::String(data.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn bits(value: f64) -> Value {
    Value::String(format!("{:016x}", value.to_bits()))
}

fn units(text: &WxString) -> Value {
    Value::from(text.units().to_vec())
}

fn text(units: &[WChar]) -> Result<WxString> {
    WxString::from_units(units).map_err(|_| Refusal::Execution("std::bad_alloc".into()))
}

/// nlohmann objects are `std::map`s: keys in byte order. Sort recursively to match.
fn sorted(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<(String, Value)> = object.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

fn gesture_json(gesture: &TouchpadGesture) -> Value {
    json!({
        "pan_x": bits(gesture.pan_x),
        "pan_y": bits(gesture.pan_y),
        "zoom_anchor": [gesture.zoom_anchor.x, gesture.zoom_anchor.y],
        "zoom_factor": bits(gesture.zoom_factor),
    })
}

fn attrs_json(attrs: &TargetAttrs) -> Value {
    json!({"captured": attrs.captured, "value": attrs.value})
}

fn proxy_json(config: &ProxyConfig) -> Value {
    json!({
        "host": units(&config.host),
        "password": units(&config.password),
        "username": units(&config.username),
    })
}

fn lock_json(lock: &FileLock) -> Value {
    json!({"fd": lock.fd(), "state": lock.state().value()})
}

fn constants() -> Value {
    let states = [
        ("NONE", FileLockState::None),
        ("HELD", FileLockState::Held),
        ("BUSY", FileLockState::Busy),
        ("UNSUPPORTED", FileLockState::Unsupported),
    ];
    let displays = [
        ("DISPLAY_UNKNOWN", NativeDisplayType::Unknown),
        ("DISPLAY_X11", NativeDisplayType::X11),
        ("DISPLAY_WAYLAND", NativeDisplayType::Wayland),
        ("DISPLAY_WIN32", NativeDisplayType::Win32),
        ("DISPLAY_COCOA", NativeDisplayType::Cocoa),
    ];
    json!({
        "cloud_sync_buffer_size": CLOUD_SYNC_BUFFER_SIZE,
        "file_lock_states": states
            .iter()
            .map(|(name, state)| ((*name).to_owned(), Value::from(state.value())))
            .collect::<Map<String, Value>>(),
        "native_display_types": displays
            .iter()
            .map(|(name, display)| ((*name).to_owned(), Value::from(display.value())))
            .collect::<Map<String, Value>>(),
    })
}

fn value_init() -> Value {
    let gpu = GpuInfo::default();
    let cpu = CpuInfo::default();
    let memory = MemoryInfo::default();
    json!({
        "cpu_info_value_init": {
            "manufacturer": hex(&cpu.manufacturer),
            "name": hex(&cpu.name),
            "number_cores": cpu.number_cores,
            "number_logical": cpu.number_logical,
        },
        "file_lock_default_init": lock_json(&FileLock::new()),
        "file_lock_value_init": lock_json(&FileLock::default()),
        "gpu_info_value_init": {
            "driver_version": hex(&gpu.driver_version),
            "manufacturer": hex(&gpu.manufacturer),
            "memory_size": gpu.memory_size,
            "name": hex(&gpu.name),
        },
        "memory_info_value_init": {
            "free_paging": memory.free_paging,
            "free_physical": memory.free_physical,
            "free_virtual": memory.free_virtual,
            "total_paging": memory.total_paging,
            "total_physical": memory.total_physical,
            "total_virtual": memory.total_virtual,
            "usage": memory.usage,
        },
        "proxy_config_default_init": proxy_json(&ProxyConfig::default()),
        "proxy_config_value_init": proxy_json(&ProxyConfig::default()),
        "target_attrs_default_init": attrs_json(&TargetAttrs::default()),
        "target_attrs_value_init": attrs_json(&TargetAttrs::default()),
        "touchpad_gesture_value_init": gesture_json(&TouchpadGesture::default()),
    })
}

fn target_attrs(input: &Value) -> Result<Value> {
    let both = input.get("captured").is_some();
    if both {
        check_keys(input, &["op", "value", "captured"], "")?;
    } else {
        check_keys(input, &["op", "value"], "")?;
    }
    let value = read_integer(member(input, "value")?, "value", 0, i64::from(u32::MAX))?;
    let value =
        u32::try_from(value).map_err(|_| Refusal::Input("integer out of range: value".into()))?;
    // TARGET_ATTRS{ value, captured } or TARGET_ATTRS{ value }: an omitted member takes its
    // default member initializer.
    let braced = if both {
        TargetAttrs {
            value,
            captured: read_bool(member(input, "captured")?, "captured")?,
        }
    } else {
        TargetAttrs {
            value,
            ..TargetAttrs::default()
        }
    };
    let copy = braced;
    Ok(json!({"braced": attrs_json(&braced), "copy": attrs_json(&copy)}))
}

fn proxy_config(input: &Value) -> Result<Value> {
    check_keys(input, &["op", "host", "username", "password"], "")?;
    let host = read_units(member(input, "host")?, "host")?;
    let username = read_units(member(input, "username")?, "username")?;
    let password = read_units(member(input, "password")?, "password")?;
    let mut config = ProxyConfig::default();
    let default = proxy_json(&config);
    config.host = text(&host)?;
    config.username = text(&username)?;
    config.password = text(&password)?;
    let copy = config
        .try_clone()
        .map_err(|_| Refusal::Execution("std::bad_alloc".into()))?;
    Ok(json!({
        "assigned": proxy_json(&config),
        "copy": proxy_json(&copy),
        "default": default,
    }))
}

fn touchpad_callback(input: &Value) -> Result<Value> {
    check_keys(input, &["op", "callback", "gestures"], "")?;
    let kind = read_text(input, "callback")?;
    if kind != "set" && kind != "empty" {
        return refuse("unknown callback");
    }
    let mut gestures = Vec::new();
    for entry in read_array(member(input, "gestures")?, "gestures", MAX_ENTRIES)? {
        check_keys(
            entry,
            &["pan_x", "pan_y", "zoom_factor", "zoom_anchor"],
            "gestures",
        )?;
        let pan_x = read_double(member(entry, "pan_x")?, "gestures.pan_x")?;
        let pan_y = read_double(member(entry, "pan_y")?, "gestures.pan_y")?;
        let zoom_factor = read_double(member(entry, "zoom_factor")?, "gestures.zoom_factor")?;
        let anchor = match member(entry, "zoom_anchor")?.as_array() {
            Some(anchor) if anchor.len() == 2 => anchor,
            _ => return refuse("expected point: gestures.zoom_anchor"),
        };
        let coordinate = |index: usize| -> Result<i32> {
            let value = anchor
                .get(index)
                .ok_or_else(|| Refusal::Input("expected point: gestures.zoom_anchor".into()))?;
            let number = read_integer(
                value,
                "gestures.zoom_anchor",
                i64::from(i32::MIN),
                i64::from(i32::MAX),
            )?;
            i32::try_from(number)
                .map_err(|_| Refusal::Input("integer out of range: gestures.zoom_anchor".into()))
        };
        let zoom_anchor = WxPoint::new(coordinate(0)?, coordinate(1)?);
        gestures.push(TouchpadGesture {
            pan_x,
            pan_y,
            zoom_factor,
            zoom_anchor,
        });
    }
    let received = Rc::new(RefCell::new(Vec::<TouchpadGesture>::new()));
    let mut callback = if kind == "set" {
        let sink = Rc::clone(&received);
        TouchpadGestureCallback::new(move |gesture: &TouchpadGesture| {
            sink.borrow_mut().push(*gesture);
        })
    } else {
        TouchpadGestureCallback::empty()
    };
    let mut calls = Vec::new();
    for gesture in &gestures {
        let before = received.borrow().len();
        match callback.call(gesture) {
            Ok(()) => {
                let seen = received.borrow();
                match seen.last() {
                    Some(last) if seen.len() == before + 1 => {
                        calls.push(json!({"received": gesture_json(last)}));
                    }
                    _ => {
                        return Err(Refusal::Execution(
                            "callback target did not run once".into(),
                        ))
                    }
                }
            }
            Err(BadFunctionCall) => calls.push(json!({"error": "bad_function_call"})),
        }
    }
    Ok(json!({"calls": calls, "is_set": callback.is_set()}))
}

/// A handler that records its construction and destruction.
struct RecordingHandler {
    id: u64,
    events: Rc<RefCell<Vec<Value>>>,
}

impl RecordingHandler {
    fn new(id: u64, events: &Rc<RefCell<Vec<Value>>>) -> Self {
        events
            .borrow_mut()
            .push(json!({"event": "construct", "id": id}));
        Self {
            id,
            events: Rc::clone(events),
        }
    }
}

impl TouchpadGestureHandler for RecordingHandler {}

impl Drop for RecordingHandler {
    fn drop(&mut self) {
        self.events
            .borrow_mut()
            .push(json!({"event": "destroy", "id": self.id}));
    }
}

fn touchpad_handlers(input: &Value) -> Result<Value> {
    check_keys(input, &["op", "steps"], "")?;
    let mut steps = Vec::new();
    for entry in read_array(member(input, "steps")?, "steps", MAX_ENTRIES)? {
        check_keys(entry, &["action", "slot"], "steps")?;
        let action = read_text(entry, "action")?;
        if action != "create" && action != "release" {
            return refuse("unknown action");
        }
        let maximum = i64::try_from(HANDLER_SLOTS - 1).unwrap_or(0);
        let slot = read_integer(member(entry, "slot")?, "steps.slot", 0, maximum)?;
        let slot = usize::try_from(slot)
            .map_err(|_| Refusal::Input("integer out of range: steps.slot".into()))?;
        steps.push((action == "create", slot));
    }
    let events = Rc::new(RefCell::new(Vec::new()));
    {
        // Owners as CreateTouchpadGestureHandler returns them: boxed trait objects.
        let mut slots: Vec<Option<Box<dyn TouchpadGestureHandler>>> =
            (0..HANDLER_SLOTS).map(|_| None).collect();
        let mut next_id = 0u64;
        for (create, slot) in steps {
            let owner = slots
                .get_mut(slot)
                .ok_or_else(|| Refusal::Execution("slot out of range".into()))?;
            if create {
                // As in `slots[slot] = std::make_unique<...>()`: the new handler is built,
                // then the old one destroyed.
                *owner = Some(Box::new(RecordingHandler::new(next_id, &events)));
                next_id += 1;
            } else {
                *owner = None;
            }
        }
        for owner in &mut slots {
            *owner = None;
        }
    }
    let events = events.borrow().clone();
    Ok(json!({"events": events}))
}

fn mapped_file(input: &Value) -> Result<Value> {
    check_keys(input, &["op", "state", "bytes"], "")?;
    let state = read_text(input, "state")?;
    if state != "empty_file" && state != "mapped" && state != "fallback" {
        return refuse("unknown state");
    }
    let bytes = read_bytes(member(input, "bytes")?, "bytes")?;
    if state == "empty_file" && !bytes.is_empty() {
        return refuse("empty_file state has no bytes");
    }
    if state == "mapped" && bytes.is_empty() {
        return refuse("mapped state needs at least one byte");
    }
    // The members the constructor leaves (the host's side of MAPPED_FILE).
    let length = bytes.len();
    let file = match state {
        "empty_file" => MappedFile::from_members(MappedData::Null, 0, false, Vec::new()),
        "mapped" => MappedFile::from_members(MappedData::Mapping(bytes), length, true, Vec::new()),
        _ => MappedFile::from_members(MappedData::FallbackBuffer, length, false, bytes),
    };
    let size = file.size();
    let (data, points_to) = match file.data() {
        None => (Value::Null, "null"),
        Some(view) => {
            let shown = view
                .get(..size)
                .ok_or_else(|| Refusal::Execution("Data() is shorter than Size()".into()))?;
            let target = match file.data_member() {
                MappedData::Mapping(_) => "mapping",
                MappedData::FallbackBuffer => "fallback_buffer",
                MappedData::Null => {
                    return Err(Refusal::Execution(
                        "Data() points at an unknown buffer".into(),
                    ))
                }
            };
            (hex(shown), target)
        }
    };
    Ok(json!({"data": data, "points_to": points_to, "size": size}))
}

fn call_json(recorder: &Recorder, returned: Option<bool>, returned_null: Option<bool>) -> Value {
    let mut result = Map::new();
    if let Some(data) = &recorder.data {
        result.insert("data".into(), hex(data));
    }
    let flags: Map<String, Value> = recorder
        .flags
        .iter()
        .map(|(name, value)| ((*name).to_owned(), Value::Bool(*value)))
        .collect();
    result.insert("flags".into(), Value::Object(flags));
    if let Some(returned) = returned {
        result.insert("returned".into(), Value::Bool(returned));
    }
    if let Some(returned_null) = returned_null {
        result.insert("returned_null".into(), Value::Bool(returned_null));
    }
    let strings: Vec<Value> = recorder
        .strings
        .iter()
        .map(|units| Value::from(units.clone()))
        .collect();
    result.insert("strings".into(), Value::Array(strings));
    Value::Object(result)
}

fn default_arguments(input: &Value) -> Result<Value> {
    let function = read_text(input, "function")?;
    let (count, has_data, has_returned) = match function {
        "atomic_rename" => {
            check_keys(
                input,
                &["op", "function", "strings", "explicit", "returned"],
                "",
            )?;
            (2, false, true)
        }
        "open_unique_sibling_temp_file" => {
            check_keys(input, &["op", "function", "strings", "explicit"], "")?;
            (2, false, false)
        }
        "commit_temp_file" => {
            check_keys(
                input,
                &["op", "function", "strings", "explicit", "returned"],
                "",
            )?;
            (2, false, true)
        }
        "atomic_write_file" => {
            check_keys(
                input,
                &["op", "function", "strings", "data", "explicit", "returned"],
                "",
            )?;
            (1, true, true)
        }
        "store_secret_async" => {
            check_keys(input, &["op", "function", "strings", "explicit"], "")?;
            (3, false, false)
        }
        "get_native_window_handles" => {
            check_keys(input, &["op", "function", "explicit"], "")?;
            (0, false, false)
        }
        _ => return refuse("unknown function"),
    };
    let mut strings = Vec::new();
    if count > 0 {
        let array = read_array(member(input, "strings")?, "strings", count)?;
        if array.len() != count {
            return refuse("wrong string count");
        }
        for value in array {
            strings.push(text(&read_units(value, "strings")?)?);
        }
    }
    let data = if has_data {
        read_bytes(member(input, "data")?, "data")?
    } else {
        Vec::new()
    };
    let explicit = read_bool(member(input, "explicit")?, "explicit")?;
    let returned = if has_returned {
        read_bool(member(input, "returned")?, "returned")?
    } else {
        false
    };
    let argument = |index: usize| {
        strings
            .get(index)
            .ok_or_else(|| Refusal::Execution("missing string argument".into()))
    };
    let mut host = Recorder {
        returned,
        ..Recorder::default()
    };
    let mut error = WxString::default();
    let result = match function {
        "atomic_rename" => {
            let (src, dst) = (argument(0)?, argument(1)?);
            let value = if explicit {
                host.atomic_rename(src, dst, Some(&mut error))
            } else {
                host.atomic_rename_without_error(src, dst)
            };
            call_json(&host, Some(value), None)
        }
        "open_unique_sibling_temp_file" => {
            let (target, mode) = (argument(0)?, argument(1)?);
            let mut temp_path = WxString::default();
            let file = if explicit {
                host.open_unique_sibling_temp_file(
                    target,
                    mode,
                    Some(&mut temp_path),
                    Some(&mut error),
                )
            } else {
                host.open_unique_sibling_temp_file_without_error(target, mode, Some(&mut temp_path))
            };
            call_json(&host, None, Some(file.is_none()))
        }
        "commit_temp_file" => {
            let (temp, target) = (argument(0)?, argument(1)?);
            let value = if explicit {
                host.commit_temp_file(temp, target, Some(&mut error))
            } else {
                host.commit_temp_file_without_error(temp, target)
            };
            call_json(&host, Some(value), None)
        }
        "atomic_write_file" => {
            let target = argument(0)?;
            let value = if explicit {
                host.atomic_write_file(target, &data, Some(&mut error))
            } else {
                host.atomic_write_file_without_error(target, &data)
            };
            call_json(&host, Some(value), None)
        }
        "store_secret_async" => {
            let (service, key, secret) = (argument(0)?, argument(1)?, argument(2)?);
            if explicit {
                let done: StoreDone = Some(Box::new(|_: bool| {}));
                host.store_secret_async(service, key, secret, done);
            } else {
                host.store_secret_async_without_done(service, key, secret);
            }
            call_json(&host, None, None)
        }
        _ => {
            let (mut handle, mut display, mut display_type) = ((), (), 0i32);
            if explicit {
                host.get_native_window_handles(
                    None,
                    Some(&mut handle),
                    Some(&mut display),
                    Some(&mut display_type),
                );
            } else {
                host.get_native_window_handles_without_display_type(
                    None,
                    Some(&mut handle),
                    Some(&mut display),
                );
            }
            call_json(&host, None, None)
        }
    };
    Ok(result)
}

/// Run one validated operation.
pub fn run(input: &Value) -> Result<Value> {
    let op = read_text(input, "op")?;
    let result = match op {
        "constants" | "value_init" => {
            check_keys(input, &["op"], "")?;
            if op == "constants" {
                constants()
            } else {
                value_init()
            }
        }
        "target_attrs" => target_attrs(input)?,
        "proxy_config" => proxy_config(input)?,
        "touchpad_callback" => touchpad_callback(input)?,
        "touchpad_handlers" => touchpad_handlers(input)?,
        "mapped_file" => mapped_file(input)?,
        "default_arguments" => default_arguments(input)?,
        _ => return refuse("unknown operation"),
    };
    Ok(sorted(result))
}
