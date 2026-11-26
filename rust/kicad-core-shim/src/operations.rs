// SPDX-License-Identifier: GPL-3.0-or-later
//! Operation decoding for the shim. Validation order and messages match the C++
//! adapter exactly; only validation, conversion and calls into `kicad-core` live here.

use kicad_core::profile::{
    self, Checkpoint, ClockReading, LatencyProbe, ProfileError, RunningClock, Timespec,
};
use kicad_core::{base64, version_compare};
use serde_json::{json, Map, Value};

/// Largest byte input accepted for base64 operations (after `repeat`).
pub const MAX_BYTES: usize = 1 << 20;
/// Largest literal hex field, in bytes.
pub const MAX_HEX_BYTES: usize = 1 << 16;
/// Largest accepted string field, in bytes.
pub const MAX_TEXT: usize = 1 << 16;
/// Largest number of latency probe checkpoints.
pub const MAX_CHECKPOINTS: usize = 256;

/// A refusal or a deferral.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::Invalid(message.to_owned())
    }
}

pub type Result<T> = core::result::Result<T, Error>;

/// Require exactly the required keys plus any subset of the optional ones.
pub fn fields<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a Map<String, Value>> {
    let object = value.as_object().ok_or("unexpected or missing field")?;
    if required.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err("unexpected or missing field".into());
    }
    Ok(object)
}

pub fn text<'a>(input: &'a Value, name: &str) -> Result<&'a str> {
    input
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("expected string: {name}")))
}

pub fn bounded_text<'a>(input: &'a Value, name: &str) -> Result<&'a str> {
    let value = text(input, name)?;
    if value.len() > MAX_TEXT {
        return Err(format!("string too long: {name}").into());
    }
    Ok(value)
}

pub fn signed(input: &Value, name: &str, min: i64, max: i64) -> Result<i64> {
    input
        .get(name)
        .and_then(Value::as_i64)
        .filter(|v| *v >= min && *v <= max)
        .ok_or_else(|| Error::Invalid(format!("integer out of range: {name}")))
}

pub fn unsigned(input: &Value, name: &str) -> Result<u64> {
    input
        .get(name)
        .and_then(|value| {
            // Arbitrary-precision parsing preserves integer -0, which as_u64 refuses.
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|v| u64::try_from(v).ok()))
        })
        .ok_or_else(|| Error::Invalid(format!("integer out of range: {name}")))
}

pub fn boolean(input: &Value, name: &str) -> Result<bool> {
    input
        .get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| Error::Invalid(format!("expected boolean: {name}")))
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Lowercase hex string of at most [`MAX_HEX_BYTES`] bytes.
pub fn hex(input: &Value, name: &str) -> Result<Vec<u8>> {
    let invalid = || Error::Invalid(format!("invalid hex: {name}"));
    let digits = text(input, name).map_err(|_| invalid())?.as_bytes();
    if digits.len() % 2 != 0 {
        return Err(invalid());
    }
    if digits.len() / 2 > MAX_HEX_BYTES {
        return Err(format!("byte array too large: {name}").into());
    }
    digits
        .chunks_exact(2)
        .map(|pair| match (nibble(pair[0]), nibble(pair[1])) {
            (Some(high), Some(low)) => Ok((high << 4) | low),
            _ => Err(invalid()),
        })
        .collect()
}

pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}

/// IEEE-754 bit pattern as exactly 16 lowercase hex digits.
pub fn bits(input: &Value, name: &str) -> Result<f64> {
    let invalid = || Error::Invalid(format!("invalid bits: {name}"));
    let digits = text(input, name).map_err(|_| invalid())?;
    if digits.len() != 16 || !digits.bytes().all(|b| nibble(b).is_some()) {
        return Err(invalid());
    }
    u64::from_str_radix(digits, 16)
        .map(f64::from_bits)
        .map_err(|_| invalid())
}

fn base64_operation(input: &Value, encode: bool) -> Result<Value> {
    fields(input, &["op", "input"], &["output", "repeat"])?;
    let data = hex(input, "input")?;
    let mut output = if input.get("output").is_some() {
        hex(input, "output")?
    } else {
        Vec::new()
    };
    let repeat = if input.get("repeat").is_some() {
        signed(input, "repeat", 1, 65536)?
    } else {
        1
    };
    let repeat = usize::try_from(repeat).map_err(|_| "integer out of range: repeat")?;
    if data
        .len()
        .checked_mul(repeat)
        .is_none_or(|total| total > MAX_BYTES)
    {
        return Err("input too large".into());
    }
    let data = data.repeat(repeat);
    let result = if encode {
        base64::encode(&data, &mut output)
    } else {
        base64::decode(&data, &mut output)
    };
    result.map_err(|_| "allocation failed")?;
    Ok(json!({"output": to_hex(&output)}))
}

fn version_error(error: version_compare::VersionError) -> Error {
    match error {
        version_compare::VersionError::NoProgress { .. } => {
            "version string does not terminate".into()
        }
        version_compare::VersionError::Allocation => "allocation failed".into(),
    }
}

/// Host clock script: one scripted `CLOCK_MONOTONIC` reading, counting reads.
struct ScriptedClock {
    now: Timespec,
    calls: u32,
}

impl RunningClock for ScriptedClock {
    fn read_running_clock(&mut self) -> ClockReading {
        self.calls = self.calls.saturating_add(1);
        ClockReading::Monotonic(self.now)
    }
}

fn latency_probe(input: &Value) -> Result<Value> {
    fields(input, &["op", "name", "start", "checkpoints"], &[])?;
    let name = bounded_text(input, "name")?.to_owned();
    let start = signed(input, "start", i64::MIN, i64::MAX)?;
    let list = input
        .get("checkpoints")
        .and_then(Value::as_array)
        .ok_or("expected array: checkpoints")?;
    if list.len() > MAX_CHECKPOINTS {
        return Err("too many checkpoints".into());
    }
    let mut checkpoints = Vec::with_capacity(list.len());
    for item in list {
        fields(item, &["name", "timestamp", "timer", "delta"], &[])?;
        checkpoints.push(Checkpoint {
            name: bounded_text(item, "name")?.to_owned(),
            timestamp: signed(item, "timestamp", i64::MIN, i64::MAX)?,
            is_timer: boolean(item, "timer")?,
            timer_delta: bits(item, "delta")?,
        });
    }
    // Keep the time point subtractions in delta_ms defined: `a - b` on signed
    // nanosecond durations must not overflow.
    let mut prev = start;
    for checkpoint in checkpoints.iter().filter(|c| !c.is_timer) {
        if checkpoint.timestamp.checked_sub(prev).is_none()
            || checkpoint.timestamp.checked_sub(start).is_none()
        {
            return Err("time point difference overflows".into());
        }
        prev = checkpoint.timestamp;
    }
    let probe = LatencyProbe {
        start,
        name,
        checkpoints,
    };
    match probe.to_string() {
        Ok(text) => Ok(json!({"text": text})),
        Err(ProfileError::Unported { upstream, unit }) => Err(Error::Unported { upstream, unit }),
        Err(ProfileError::Overflow) => Err("signed overflow".into()),
        // Not produced by to_string (it allocates nothing and reads no timer).
        Err(
            ProfileError::LengthError(_) | ProfileError::Allocation | ProfileError::Indeterminate,
        ) => Err("unexpected profile error".into()),
    }
}

/// Run one request input.
pub fn run(input: &Value) -> Result<Value> {
    let op = text(input, "op")?;
    match op {
        "encode_length" | "decode_length" => {
            fields(input, &["op", "len"], &[])?;
            let len = unsigned(input, "len")?;
            let value = if op == "encode_length" {
                base64::encode_length(len)
            } else {
                base64::decode_length(len)
            };
            Ok(json!({"value": value}))
        }
        "encode" => base64_operation(input, true),
        "decode" => base64_operation(input, false),
        "parse_version" => {
            fields(input, &["op", "text"], &[])?;
            let text = bounded_text(input, "text")?;
            let components =
                version_compare::parse_version_string(text.as_bytes()).map_err(version_error)?;
            Ok(json!({"components": components}))
        }
        "compare_versions" => {
            fields(input, &["op", "a", "b"], &[])?;
            let a = bounded_text(input, "a")?;
            let b = bounded_text(input, "b")?;
            let value = version_compare::compare_version_strings(a.as_bytes(), b.as_bytes())
                .map_err(version_error)?;
            Ok(json!({"value": value}))
        }
        "running_micro_secs" => {
            fields(input, &["op", "tv_sec", "tv_nsec"], &[])?;
            let tv_sec = signed(input, "tv_sec", i64::MIN, i64::MAX)?;
            let tv_nsec = signed(input, "tv_nsec", i64::MIN, i64::MAX)?;
            if !(0..1_000_000_000).contains(&tv_nsec) {
                return Err("invalid timespec: tv_nsec".into());
            }
            let mut clock = ScriptedClock {
                now: Timespec { tv_sec, tv_nsec },
                calls: 0,
            };
            let value =
                profile::get_running_micro_secs(&mut clock).map_err(|_| "signed overflow")?;
            // The Monotonic reading form is the CLOCK_MONOTONIC (Linux id 1) read.
            Ok(json!({"value": value, "clock_calls": clock.calls, "clock_id": 1}))
        }
        "latency_probe" => latency_probe(input),
        "observable" => crate::observable_driver::run(input),
        _ => Err("unknown operation".into()),
    }
}
