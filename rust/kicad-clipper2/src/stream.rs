// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 (Copyright Angus Johnson 2010-2025), Boost Software
// License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `std::ostream` insertions used by the Clipper2 `operator<<` overloads.
//!
//! The overloads write into the caller's stream, so their text depends on its formatting
//! state. [`kicad_math::StreamState`] models that state as libstdc++ (the oracle's library)
//! applies it in the classic locale; `double` and `const char*` insertions reuse
//! `kicad_math`. This module adds `operator<<( long )`, which `int64_t` is on the LP64
//! oracle, and `std::endl`.

use kicad_math::stream::insert_str;
use kicad_math::{StreamError, StreamInsert, StreamState};

/// `operator<<( long )`: libstdc++ `num_put::_M_insert_int` for a signed `long`.
///
/// Decimal unless `basefield` is exactly `oct` or exactly `hex`. Octal and hexadecimal print
/// the value converted to `unsigned long` (two's complement, 64 bits); `showbase` adds `0`,
/// `0x` or `0X` to nonzero values. Decimal prints `-` for negatives and `+` with `showpos`.
pub fn insert_i64(value: i64, state: &mut StreamState) -> Result<String, StreamError> {
    state.validate()?;
    let flags = state.flags;
    let octal = flags.oct && !flags.hex && !flags.dec;
    let hexadecimal = flags.hex && !flags.oct && !flags.dec;
    let text = if octal || hexadecimal {
        let unsigned = u64::from_ne_bytes(value.to_ne_bytes());
        let digits = if octal {
            format!("{unsigned:o}")
        } else if flags.uppercase {
            format!("{unsigned:X}")
        } else {
            format!("{unsigned:x}")
        };
        let base = match (flags.showbase && value != 0, octal, flags.uppercase) {
            (false, _, _) => "",
            (true, true, _) => "0",
            (true, false, false) => "0x",
            (true, false, true) => "0X",
        };
        format!("{base}{digits}")
    } else if value < 0 {
        format!("-{}", value.unsigned_abs())
    } else if flags.showpos {
        format!("+{value}")
    } else {
        format!("{value}")
    };
    Ok(pad_number(text, state))
}

/// `operator<<( double )`, as modelled by `kicad_math`.
pub fn insert_f64(value: f64, state: &mut StreamState) -> Result<String, StreamError> {
    value.insert(state)
}

/// `operator<<( const char* )`, as modelled by `kicad_math`.
pub fn insert_text(text: &str, state: &mut StreamState) -> Result<String, StreamError> {
    insert_str(text, state)
}

/// `std::endl`: `put('\n')` then `flush()`. Unformatted output leaves the width unchanged.
pub fn endl() -> &'static str {
    "\n"
}

/// libstdc++ `__pad` for a formatted number: `left` pads after, `internal` pads after a
/// leading sign or `0x`/`0X`, anything else pads before. Resets the width.
fn pad_number(text: String, state: &mut StreamState) -> String {
    let length = text.chars().count();
    let padding = usize::try_from(state.width)
        .unwrap_or(0)
        .saturating_sub(length);
    state.width = 0;
    if padding == 0 {
        return text;
    }
    let fill: String = ::core::iter::repeat_n(state.fill, padding).collect();
    let flags = &state.flags;
    match (flags.left, flags.right, flags.internal) {
        (true, false, false) => format!("{text}{fill}"),
        (false, false, true) => {
            let bytes = text.as_bytes();
            let split = if matches!(bytes.first(), Some(b'+' | b'-')) {
                1
            } else if bytes.len() >= 2
                && bytes.first() == Some(&b'0')
                && matches!(bytes.get(1), Some(b'x' | b'X'))
            {
                2
            } else {
                0
            };
            let (head, tail) = text.split_at(split);
            format!("{head}{fill}{tail}")
        }
        _ => format!("{fill}{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_insertion_matches_libstdcxx_rules() {
        let mut state = StreamState::default();
        assert_eq!(
            insert_i64(i64::MIN, &mut state),
            Ok("-9223372036854775808".into())
        );
        state.flags.dec = false;
        state.flags.hex = true;
        state.flags.showbase = true;
        assert_eq!(insert_i64(-1, &mut state), Ok("0xffffffffffffffff".into()));
        assert_eq!(insert_i64(0, &mut state), Ok("0".into()));
        let mut state = StreamState {
            width: 6,
            fill: '*',
            ..StreamState::default()
        };
        state.flags.internal = true;
        assert_eq!(insert_i64(-12, &mut state), Ok("-***12".into()));
        assert_eq!(state.width, 0);
    }
}
