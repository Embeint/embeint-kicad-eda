// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/version_compare.cpp`.
//!
//! `parseVersionString` repeatedly applies `std::from_chars<int>` (base 10, optional
//! leading `-`, no `+`, no whitespace) and skips at most one `.` after each attempt.
//! A failed conversion leaves the appended component at zero; an out-of-range number
//! also leaves zero but consumes all of its digits.
//!
//! When `from_chars` matches nothing and the current byte is not `.`, the reference
//! never advances: it appends zeros until allocation fails. That input is outside
//! the reference's defined domain and is reported here as
//! [`VersionError::NoProgress`] instead of looping.

/// Errors outside the reference's terminating domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionError {
    /// The reference loop would not advance at this byte offset (it never returns).
    NoProgress { offset: usize },
    /// The component vector could not grow.
    Allocation,
}

/// Result of `std::from_chars( first, last, int& )` on the pinned libstdc++.
struct FromChars {
    /// Bytes consumed (`result.ptr - first`); zero when the pattern does not match.
    consumed: usize,
    /// The converted value, or `None` when the value is left unmodified
    /// (`errc::invalid_argument` or `errc::result_out_of_range`).
    value: Option<i32>,
}

/// `std::from_chars` for `int`, base 10.
fn from_chars_i32(text: &[u8]) -> FromChars {
    let negative = text.first() == Some(&b'-');
    let start = usize::from(negative);
    let digits = text
        .get(start..)
        .unwrap_or_default()
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();

    if digits == 0 {
        // No characters match the pattern: ptr == first, value unmodified.
        return FromChars {
            consumed: 0,
            value: None,
        };
    }

    // Magnitude limit: INT_MAX, or -INT_MIN for a negative number.
    let limit: u64 = if negative { 1 << 31 } else { (1 << 31) - 1 };
    let mut magnitude: u64 = 0;
    let mut in_range = true;

    for byte in text.iter().skip(start).take(digits) {
        if in_range {
            match magnitude
                .checked_mul(10)
                .and_then(|m| m.checked_add(u64::from(byte - b'0')))
            {
                Some(next) if next <= limit => magnitude = next,
                _ => in_range = false,
            }
        }
    }

    let consumed = start + digits;
    let value = if !in_range {
        None
    } else if negative {
        // magnitude <= 2^31, so the negation fits in i64 and the result in i32.
        i32::try_from(-(magnitude as i64)).ok()
    } else {
        i32::try_from(magnitude).ok()
    };

    FromChars { consumed, value }
}

/// `parseVersionString`: split a version string into integer components.
///
/// # Errors
/// [`VersionError::NoProgress`] for input on which the reference does not terminate.
pub fn parse_version_string(version: &[u8]) -> Result<Vec<i32>, VersionError> {
    let mut components: Vec<i32> = Vec::new();
    let end = version.len();
    let mut it = 0usize;

    while it != end {
        components
            .try_reserve(1)
            .map_err(|_| VersionError::Allocation)?;
        components.push(0);

        // On error, the last element will be 0
        let rest = version.get(it..).unwrap_or_default();
        let result = from_chars_i32(rest);

        if let (Some(value), Some(last)) = (result.value, components.last_mut()) {
            *last = value;
        }

        let before = it;
        it += result.consumed; // Update the iterator

        if it != end && version.get(it) == Some(&b'.') {
            it += 1; // Skip the dot separator
        }

        if it == before {
            return Err(VersionError::NoProgress { offset: it });
        }
    }

    Ok(components)
}

/// `compareVersionStrings`: the first differing component decides (`true` when `a`'s
/// is smaller). With an equal common prefix the result is `ver1.size() >= ver2.size()`,
/// so `"1.2.0"` vs `"1.2"` is `true` and `"1.2"` vs `"1.2.0"` is `false`.
///
/// # Errors
/// [`VersionError`] when either string is outside the terminating domain.
pub fn compare_version_strings(a: &[u8], b: &[u8]) -> Result<bool, VersionError> {
    let ver1 = parse_version_string(a)?;
    let ver2 = parse_version_string(b)?;

    // Compare each component
    for (left, right) in ver1.iter().zip(ver2.iter()) {
        if left < right {
            return Ok(true); // aVersionStr1 < aVersionStr2
        } else if left > right {
            return Ok(false); // aVersionStr1 > aVersionStr2
        }
    }

    Ok(ver1.len() >= ver2.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Vec<i32>, VersionError> {
        parse_version_string(text.as_bytes())
    }

    #[test]
    fn components() {
        assert_eq!(parse(""), Ok(vec![]));
        assert_eq!(parse("1.2.3"), Ok(vec![1, 2, 3]));
        assert_eq!(parse("1..2"), Ok(vec![1, 0, 2]));
        assert_eq!(parse(".1"), Ok(vec![0, 1]));
        assert_eq!(parse("1."), Ok(vec![1]));
        assert_eq!(parse("007"), Ok(vec![7]));
        assert_eq!(parse("1-2"), Ok(vec![1, -2]));
        assert_eq!(parse("-2147483648"), Ok(vec![i32::MIN]));
        assert_eq!(parse("2147483648.5"), Ok(vec![0, 5]));
        assert_eq!(parse("-2147483649"), Ok(vec![0]));
        assert_eq!(parse("1.a"), Err(VersionError::NoProgress { offset: 2 }));
        assert_eq!(parse("-"), Err(VersionError::NoProgress { offset: 0 }));
        assert_eq!(parse("+1"), Err(VersionError::NoProgress { offset: 0 }));
    }

    #[test]
    fn comparison() {
        let cmp = |a: &str, b: &str| compare_version_strings(a.as_bytes(), b.as_bytes());
        assert_eq!(cmp("1.2", "1.3"), Ok(true));
        assert_eq!(cmp("1.3", "1.2"), Ok(false));
        assert_eq!(cmp("1.2", "1.2"), Ok(true));
        assert_eq!(cmp("1.2.0", "1.2"), Ok(true));
        assert_eq!(cmp("1.2", "1.2.0"), Ok(false));
    }
}
