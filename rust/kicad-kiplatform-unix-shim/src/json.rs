// SPDX-License-Identifier: GPL-3.0-or-later
//! Request decoding helpers, with the C++ adapter's validation and messages.

use kicad_map_helpers::{WChar, WxString};
use serde_json::Value;

/// Largest number of `wchar_t` units in one string.
pub const MAX_UNITS: usize = 1024;
/// Largest number of bytes in one byte field.
pub const MAX_BYTES: usize = 4096;
/// Largest number of replies queued for one function.
pub const MAX_REPLIES: usize = 256;
/// Largest number of list entries, settings, emissions or environment variables.
pub const MAX_ENTRIES: usize = 64;
/// Largest number of handles one case may create.
pub const MAX_HANDLES: usize = 4000;

/// Why a request produced no result: a refused input (`invalid_input`) or a failure of the
/// adapter itself (`execution_error`).
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `invalid_input`.
    Input(String),
    /// `execution_error`.
    Execution(String),
}

/// Shorthand.
pub type Result<T> = core::result::Result<T, Refusal>;

/// A refused input.
pub fn refuse<T>(message: impl Into<String>) -> Result<T> {
    Err(Refusal::Input(message.into()))
}

/// `checkKeys( value, fields, where )`.
pub fn check_keys(value: &Value, fields: &[&str], place: &str) -> Result<()> {
    let ok = value.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    });
    if ok {
        Ok(())
    } else {
        refuse(format!("unexpected or missing field: {place}"))
    }
}

/// `member( input, name )`.
pub fn member<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    match value.as_object().and_then(|object| object.get(name)) {
        Some(found) => Ok(found),
        None => refuse(format!("missing field: {name}")),
    }
}

/// `readBool`.
pub fn read_bool(value: &Value, name: &str) -> Result<bool> {
    match value.as_bool() {
        Some(b) => Ok(b),
        None => refuse(format!("expected boolean: {name}")),
    }
}

/// `readInteger`: an integer token in `[min, max]` (nlohmann's `is_number_integer`).
pub fn read_integer(value: &Value, name: &str, min: i64, max: i64) -> Result<i64> {
    if let Some(number) = value.as_u64() {
        // number > static_cast<uint64_t>( aMax ) is refused; every max here is non-negative.
        return match u64::try_from(max) {
            Ok(limit) if number <= limit => Ok(number as i64),
            _ => refuse(format!("integer out of range: {name}")),
        };
    }
    match value.as_i64() {
        Some(n) if (min..=max).contains(&n) => Ok(n),
        Some(_) => refuse(format!("integer out of range: {name}")),
        None => refuse(format!("expected integer: {name}")),
    }
}

/// `readInteger` for an unsigned range above `i64::MAX` is never used; `u32` fields go
/// through [`read_integer`].
pub fn read_text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    match value.as_str() {
        Some(text) => Ok(text),
        None => refuse(format!("expected string: {name}")),
    }
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// `readHex`: lower-case hexadecimal bytes, at most [`MAX_BYTES`].
pub fn read_hex(value: &Value, name: &str) -> Result<Vec<u8>> {
    let text = read_text(value, name)?.as_bytes();
    if text.len() % 2 != 0 || text.len() / 2 > MAX_BYTES {
        return refuse(format!("bad hex: {name}"));
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in text.chunks_exact(2) {
        let (Some(high), Some(low)) = (
            pair.first().copied().and_then(hex_digit),
            pair.get(1).copied().and_then(hex_digit),
        ) else {
            return refuse(format!("bad hex: {name}"));
        };
        out.push((high << 4) | low);
    }
    Ok(out)
}

/// `readCText`: hex bytes without a NUL.
pub fn read_c_text(value: &Value, name: &str) -> Result<Vec<u8>> {
    let bytes = read_hex(value, name)?;
    if bytes.contains(&0) {
        return refuse(format!("NUL in C string: {name}"));
    }
    Ok(bytes)
}

/// `readUnits`.
pub fn read_units(value: &Value, name: &str) -> Result<WxString> {
    let Some(array) = value.as_array().filter(|a| a.len() <= MAX_UNITS) else {
        return refuse(format!("expected unit array: {name}"));
    };
    let mut units = Vec::with_capacity(array.len());
    for unit in array {
        let n = read_integer(unit, name, i64::from(i32::MIN), i64::from(i32::MAX))?;
        units.push(
            WChar::try_from(n)
                .map_err(|_| Refusal::Input(format!("integer out of range: {name}")))?,
        );
    }
    WxString::from_units(&units).map_err(|_| Refusal::Execution("allocation refused".into()))
}

/// `readBits`: a double as 16 lower-case hex digits of its bits.
pub fn read_bits(value: &Value, name: &str) -> Result<f64> {
    let text = read_text(value, name)?.as_bytes();
    if text.len() != 16 {
        return refuse(format!("bad double bits: {name}"));
    }
    let mut bits = 0u64;
    for &c in text {
        let Some(d) = hex_digit(c) else {
            return refuse(format!("bad double bits: {name}"));
        };
        bits = bits * 16 + u64::from(d);
    }
    Ok(f64::from_bits(bits))
}

/// `hexOf`.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

/// `bitsOf`.
pub fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

/// `unitsOf`.
pub fn units(text: &WxString) -> Value {
    Value::from(text.units().to_vec())
}
