// SPDX-License-Identifier: GPL-3.0-or-later
//! The few Python value semantics `symbol_layout.py` relies on, applied to JSON values.
//!
//! The Python implementation is authoritative.  It formats pin fields with `str()`, tests them
//! for truth, strips numbers with `str.strip()` and echoes a whole pin in one error message with
//! `repr()`.  Matching those exactly keeps the outputs and error messages identical for the same
//! JSON request, as decoded by Python's `json` module.

use serde_json::{Map, Number, Value};

/// A JSON number as Python's `json` module decodes it: an `int` when the literal has no
/// fraction or exponent, otherwise a `float`.
enum PyNumber {
    Int(String),
    Float(f64),
}

fn py_number(number: &Number) -> PyNumber {
    // With serde_json's `arbitrary_precision`, the literal is kept exactly as written.
    let text = number.to_string();

    if text.contains(['.', 'e', 'E']) {
        // Rust and Python both round decimal literals correctly, and both overflow to infinity.
        PyNumber::Float(text.parse::<f64>().unwrap_or(f64::NAN))
    } else if text.trim_start_matches('-').bytes().all(|b| b == b'0') {
        PyNumber::Int("0".to_string())
    } else {
        PyNumber::Int(text)
    }
}

/// Python's `bool(value)`.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => match py_number(n) {
            PyNumber::Int(text) => text != "0",
            PyNumber::Float(f) => f != 0.0,
        },
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// The Python type name of an unhashable value, as `TypeError: unhashable type: '...'` names it.
pub fn unhashable_type(value: &Value) -> Option<&'static str> {
    match value {
        Value::Array(_) => Some("list"),
        Value::Object(_) => Some("dict"),
        _ => None,
    }
}

/// Python's `str(value)`.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        _ => py_repr(value),
    }
}

/// Python's `repr(value)`.
pub fn py_repr(value: &Value) -> String {
    let mut out = String::new();
    write_repr(&mut out, value);
    out
}

fn write_repr(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::Number(n) => match py_number(n) {
            PyNumber::Int(text) => out.push_str(&text),
            PyNumber::Float(f) => out.push_str(&float_repr(f)),
        },
        Value::String(s) => write_str_repr(out, s),
        Value::Array(items) => {
            out.push('[');

            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }

                write_repr(out, item);
            }

            out.push(']');
        }
        Value::Object(map) => write_dict_repr(out, map),
    }
}

fn write_dict_repr(out: &mut String, map: &Map<String, Value>) {
    out.push('{');

    for (index, (key, item)) in map.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }

        write_str_repr(out, key);
        out.push_str(": ");
        write_repr(out, item);
    }

    out.push('}');
}

/// Python's `float.__repr__`: the shortest round-trip digits, in fixed notation for decimal
/// exponents from -4 to 15 and in scientific notation (at least two exponent digits) otherwise.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }

    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_string();
    }

    // Rust's `{:e}` also prints the shortest round-trip digits, e.g. "-1.2345e-5" or "0e0".
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();

    if (-4..16).contains(&exponent) {
        let point = exponent + 1; // digits before the decimal point

        let text = if point <= 0 {
            format!("0.{}{}", "0".repeat((-point) as usize), digits)
        } else if (point as usize) >= digits.len() {
            format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };

        format!("{sign}{text}")
    } else {
        let fraction = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        format!(
            "{sign}{}{fraction}e{exponent_sign}{:02}",
            &digits[..1],
            exponent.abs()
        )
    }
}

/// Python's `str.__repr__`.
fn write_str_repr(out: &mut String, text: &str) {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    out.push(quote);

    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ if ch == quote => {
                out.push('\\');
                out.push(ch);
            }
            _ if (ch as u32) < 0x20 || ch as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", ch as u32))
            }
            _ if (ch as u32) < 0x7f || is_printable(ch) => out.push(ch),
            _ if (ch as u32) <= 0xff => out.push_str(&format!("\\x{:02x}", ch as u32)),
            _ if (ch as u32) <= 0xffff => out.push_str(&format!("\\u{:04x}", ch as u32)),
            _ => out.push_str(&format!("\\U{:08x}", ch as u32)),
        }
    }

    out.push(quote);
}

/// Python's `str.isprintable()` for one non-ASCII character, without Unicode category tables:
/// it knows the control, separator, format and private-use characters, but treats unassigned
/// code points as printable (Python escapes them).  Only `repr()` of a pin in the "needs a
/// number" error message uses it.
fn is_printable(ch: char) -> bool {
    const NOT_PRINTABLE: &[(u32, u32)] = &[
        (0x0080, 0x00a0),
        (0x00ad, 0x00ad),
        (0x0600, 0x0605),
        (0x061c, 0x061c),
        (0x06dd, 0x06dd),
        (0x070f, 0x070f),
        (0x0890, 0x0891),
        (0x08e2, 0x08e2),
        (0x1680, 0x1680),
        (0x180e, 0x180e),
        (0x2000, 0x200f),
        (0x2028, 0x202f),
        (0x205f, 0x2064),
        (0x2066, 0x206f),
        (0x3000, 0x3000),
        (0xd800, 0xf8ff),
        (0xfeff, 0xfeff),
        (0xfff9, 0xfffb),
        (0x110bd, 0x110bd),
        (0x110cd, 0x110cd),
        (0x13430, 0x1343f),
        (0x1bca0, 0x1bca3),
        (0x1d173, 0x1d17a),
        (0xe0001, 0xe0001),
        (0xe0020, 0xe007f),
        (0xf0000, 0x10ffff),
    ];
    let code = ch as u32;
    !NOT_PRINTABLE
        .iter()
        .any(|(low, high)| (*low..=*high).contains(&code))
}

/// Python's `str.strip()` with no argument (the characters `str.isspace()` accepts).
pub fn py_strip(text: &str) -> &str {
    fn is_space(ch: char) -> bool {
        matches!(ch,
            '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | '\u{1c}'..='\u{1f}' | ' ' | '\u{85}' | '\u{a0}'
            | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}'
            | '\u{3000}')
    }

    text.trim_matches(is_space)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn floats_print_as_python_repr() {
        for (value, expected) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (2.54, "2.54"),
            (1e16, "1e+16"),
            (1.5e-5, "1.5e-05"),
            (0.0001, "0.0001"),
            (123456789012345.6, "123456789012345.6"),
            (1e100, "1e+100"),
            (-7.62, "-7.62"),
            (100.0, "100.0"),
            (f64::INFINITY, "inf"),
        ] {
            assert_eq!(float_repr(value), expected);
        }
    }

    #[test]
    fn values_print_as_python_str_and_repr() {
        assert_eq!(py_str(&parse("1")), "1");
        assert_eq!(py_str(&parse("-0")), "0");
        assert_eq!(py_str(&parse("1.0")), "1.0");
        assert_eq!(py_str(&parse("1E2")), "100.0");
        assert_eq!(
            py_str(&parse("123456789012345678901234567890")),
            "123456789012345678901234567890"
        );
        assert_eq!(py_str(&json!(null)), "None");
        assert_eq!(py_str(&json!(true)), "True");
        assert_eq!(
            py_repr(&parse(r#"{"b": [1, "x"], "a": {"it's": "\"q\"\n"}}"#)),
            r#"{'b': [1, 'x'], 'a': {"it's": '"q"\n'}}"#
        );
        assert_eq!(
            py_repr(&json!("\u{7f}\u{a0}\u{e9}\u{2028}")),
            "'\\x7f\\xa0\u{e9}\\u2028'"
        );
    }

    #[test]
    fn truth_and_strip_follow_python() {
        assert!(!truthy(&parse("0.0")) && !truthy(&parse("-0")) && truthy(&json!("0")));
        assert_eq!(py_strip("\u{1c} 7\u{3000}\n"), "7");
    }
}
