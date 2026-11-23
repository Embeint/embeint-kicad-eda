// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit `std::ostream` state for the `operator<<( std::ostream&, const MATRIX3x3<T>& )`
//! port.
//!
//! The C++ operator writes into the caller's stream, so its output depends on that stream's
//! format flags, precision, width and fill. This module models that state explicitly and
//! reproduces what libstdc++ (the oracle's library) produces for it in the classic "C" locale:
//!
//! - Floating-point insertion (`num_put::_M_insert_float`) formats through the C library with
//!   the conversion chosen by `floatfield`: `%f` (fixed, always lowercase), `%e`/`%E`
//!   (scientific), `%a`/`%A` (fixed | scientific, no precision) or `%g`/`%G` (otherwise).
//!   `showpos` adds `+`, `showpoint` adds `#`. A negative precision means 6.
//! - Integer insertion (`num_put::_M_insert_int`) is decimal unless `basefield` is exactly
//!   `oct` or exactly `hex`; `operator<<( int )` passes octal/hex values as `unsigned int`.
//!   `showpos` applies to decimal only, `showbase` adds `0`/`0x`/`0X` to nonzero values,
//!   `uppercase` selects `X` and `A-F`.
//! - String insertion (`__ostream_insert`) pads to `width` with `fill`, after the text when
//!   `adjustfield` is exactly `left` and before it otherwise.
//! - Every insertion resets `width` to 0.
//!
//! Locales other than the classic one (decimal point, grouping) are not supported: the oracle
//! never changes the global locale, so its streams always use the classic locale.
//!
//! Public insertions return a typed resource error before formatting or changing stream
//! state when precision or width exceeds [`MAX_STREAM_FIELD`]. This is an implementation
//! resource limit, not a restriction on the C++ operator's defined domain.

/// Maximum positive precision or width accepted by a public insertion.
pub const MAX_STREAM_FIELD: i64 = 512;

/// A formatting request exceeds the portable implementation's resource budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
    ResourceLimit {
        field: &'static str,
        requested: i64,
        maximum: i64,
    },
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ResourceLimit {
                field,
                requested,
                maximum,
            } => write!(
                formatter,
                "stream {field} exceeds resource limit {maximum}: {requested}"
            ),
        }
    }
}

/// `std::ios_base::fmtflags`, one field per flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FmtFlags {
    pub boolalpha: bool,
    pub dec: bool,
    pub fixed: bool,
    pub hex: bool,
    pub internal: bool,
    pub left: bool,
    pub oct: bool,
    pub right: bool,
    pub scientific: bool,
    pub showbase: bool,
    pub showpoint: bool,
    pub showpos: bool,
    pub skipws: bool,
    pub unitbuf: bool,
    pub uppercase: bool,
}

/// The formatting state of a `std::ostream` that `operator<<` reads and updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamState {
    pub flags: FmtFlags,
    /// `ios_base::precision()`; negative values mean the default 6 for floating point.
    pub precision: i64,
    /// `ios_base::width()`; reset to 0 by every insertion.
    pub width: i64,
    /// `basic_ios::fill()`.
    pub fill: char,
}

impl StreamState {
    /// Check resource bounds without changing the stream. Negative precision and width
    /// retain their C++ meanings (default precision and no padding, respectively).
    pub fn validate(&self) -> Result<(), StreamError> {
        for (field, requested) in [("precision", self.precision), ("width", self.width)] {
            if requested > MAX_STREAM_FIELD {
                return Err(StreamError::ResourceLimit {
                    field,
                    requested,
                    maximum: MAX_STREAM_FIELD,
                });
            }
        }
        Ok(())
    }
}

impl Default for StreamState {
    /// A freshly constructed stream: `skipws | dec`, precision 6, width 0, fill `' '`.
    fn default() -> Self {
        Self {
            flags: FmtFlags {
                skipws: true,
                dec: true,
                ..FmtFlags::default()
            },
            precision: 6,
            width: 0,
            fill: ' ',
        }
    }
}

/// A value `operator<<` can insert into the modelled stream.
pub trait StreamInsert {
    /// Format `self` as the C++ insertion would, then reset the width as it does.
    fn insert(&self, state: &mut StreamState) -> Result<String, StreamError>;
}

/// `operator<<( std::ostream&, const char* )`: pad to the width, then reset it.
pub fn insert_str(text: &str, state: &mut StreamState) -> Result<String, StreamError> {
    state.validate()?;
    let length = text.chars().count();
    let padding = usize::try_from(state.width)
        .unwrap_or(0)
        .saturating_sub(length);
    state.width = 0;
    let fill: String = core::iter::repeat_n(state.fill, padding).collect();
    let left = state.flags.left && !state.flags.right && !state.flags.internal;
    Ok(if left {
        format!("{text}{fill}")
    } else {
        format!("{fill}{text}")
    })
}

/// Pad a formatted number to the width with libstdc++'s `__pad`: `left` pads after,
/// `internal` pads after a leading sign or `0x`/`0X` prefix, otherwise padding goes before.
fn pad_number(text: String, state: &mut StreamState) -> String {
    let length = text.chars().count();
    let padding = usize::try_from(state.width)
        .unwrap_or(0)
        .saturating_sub(length);
    state.width = 0;
    if padding == 0 {
        return text;
    }
    let fill: String = core::iter::repeat_n(state.fill, padding).collect();
    let flags = &state.flags;
    let adjust = (flags.left, flags.right, flags.internal);
    match adjust {
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

impl StreamInsert for i32 {
    fn insert(&self, state: &mut StreamState) -> Result<String, StreamError> {
        state.validate()?;
        let flags = state.flags;
        let octal = flags.oct && !flags.hex && !flags.dec;
        let hexadecimal = flags.hex && !flags.oct && !flags.dec;
        let text = if octal || hexadecimal {
            // operator<<( int ) inserts static_cast<long>( static_cast<unsigned int>( n ) ).
            let value = u32::from_ne_bytes(self.to_ne_bytes());
            let digits = if octal {
                format!("{value:o}")
            } else if flags.uppercase {
                format!("{value:X}")
            } else {
                format!("{value:x}")
            };
            let base = match (flags.showbase && value != 0, octal, flags.uppercase) {
                (false, _, _) => "",
                (true, true, _) => "0",
                (true, false, false) => "0x",
                (true, false, true) => "0X",
            };
            format!("{base}{digits}")
        } else if *self < 0 {
            format!("-{}", self.unsigned_abs())
        } else if flags.showpos {
            format!("+{self}")
        } else {
            format!("{self}")
        };
        Ok(pad_number(text, state))
    }
}

impl StreamInsert for f64 {
    fn insert(&self, state: &mut StreamState) -> Result<String, StreamError> {
        state.validate()?;
        let text = format_double(*self, state);
        Ok(pad_number(text, state))
    }
}

/// The C conversion libstdc++ selects for floating-point output.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Conversion {
    Fixed,
    Scientific,
    Hex,
    General,
}

/// `snprintf( "%[+][#].*<conversion>", precision, value )` with glibc's output.
fn format_double(value: f64, state: &StreamState) -> String {
    let flags = &state.flags;
    let conversion = match (flags.fixed, flags.scientific) {
        (true, false) => Conversion::Fixed,
        (false, true) => Conversion::Scientific,
        (true, true) => Conversion::Hex,
        (false, false) => Conversion::General,
    };
    // libstdc++ uppercases e/a/g conversions only; fixed always uses %f.
    let upper = flags.uppercase && conversion != Conversion::Fixed;
    // "Use default precision if out of range."
    let precision = if state.precision < 0 {
        6
    } else {
        usize::try_from(state.precision).unwrap_or(usize::MAX)
    };
    let alternate = flags.showpoint;

    let sign = if value.is_sign_negative() {
        "-"
    } else if flags.showpos {
        "+"
    } else {
        ""
    };
    let body = if value.is_nan() {
        String::from(if upper { "NAN" } else { "nan" })
    } else if value.is_infinite() {
        String::from(if upper { "INF" } else { "inf" })
    } else {
        let magnitude = value.abs();
        match conversion {
            Conversion::Fixed => fixed(magnitude, precision, alternate),
            Conversion::Scientific => scientific(magnitude, precision, alternate, upper),
            Conversion::Hex => hexadecimal(magnitude, alternate, upper),
            Conversion::General => general(magnitude, precision, alternate, upper),
        }
    };
    format!("{sign}{body}")
}

/// `%.Pf` of a finite nonnegative value (exact decimal expansion, ties to even).
fn fixed(magnitude: f64, precision: usize, alternate: bool) -> String {
    let mut text = format!("{magnitude:.precision$}");
    if alternate && precision == 0 {
        text.push('.');
    }
    text
}

/// The digits and decimal exponent of `%.Pe` for a finite nonnegative value.
fn exponent_digits(magnitude: f64, precision: usize) -> (String, i32) {
    let text = format!("{magnitude:.precision$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((text.as_str(), "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    (digits, exponent.parse().unwrap_or(0))
}

fn exponent_suffix(exponent: i32, upper: bool) -> String {
    let sign = if exponent < 0 { '-' } else { '+' };
    let letter = if upper { 'E' } else { 'e' };
    format!("{letter}{sign}{:02}", exponent.unsigned_abs())
}

/// `%.Pe` of a finite nonnegative value.
fn scientific(magnitude: f64, precision: usize, alternate: bool, upper: bool) -> String {
    let (digits, exponent) = exponent_digits(magnitude, precision);
    let (first, rest) = digits.split_at(digits.len().min(1));
    let point = if !rest.is_empty() || alternate {
        "."
    } else {
        ""
    };
    format!("{first}{point}{rest}{}", exponent_suffix(exponent, upper))
}

/// `%.Pg` of a finite nonnegative value: P = 0 means 1; the exponent X of `%.(P-1)e` selects
/// `%.(P-1-X)f` when P > X >= -4 and `%.(P-1)e` otherwise; without `#`, trailing fraction
/// zeros and a bare point are removed.
fn general(magnitude: f64, precision: usize, alternate: bool, upper: bool) -> String {
    let significant = precision.max(1);
    let (_, exponent) = exponent_digits(magnitude, significant - 1);
    let fixed_form = i64::from(exponent) >= -4 && (i64::from(exponent) < significant as i64);
    let text = if magnitude == 0.0 {
        fixed(magnitude, significant - 1, alternate)
    } else if fixed_form {
        let decimals = (significant as i64 - 1 - i64::from(exponent)).max(0) as usize;
        fixed(magnitude, decimals, alternate)
    } else {
        scientific(magnitude, significant - 1, alternate, upper)
    };
    if alternate {
        return text;
    }
    let (number, suffix) = match text.find(['e', 'E']) {
        Some(index) => text.split_at(index),
        None => (text.as_str(), ""),
    };
    let number = if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        number
    };
    format!("{number}{suffix}")
}

/// `%a` (no precision) of a finite nonnegative value, as glibc prints binary64: normal
/// values as `0x1.<hex>p<exp>`, subnormals as `0x0.<hex>p-1022`, zero as `0x0p+0`; trailing
/// zero hex digits are dropped and `#` keeps the point.
fn hexadecimal(magnitude: f64, alternate: bool, upper: bool) -> String {
    let bits = magnitude.to_bits();
    let biased = (bits >> 52) & 0x7ff;
    let mantissa = bits & ((1_u64 << 52) - 1);
    let (lead, exponent) = if biased == 0 {
        if mantissa == 0 {
            (0, 0)
        } else {
            (0, -1022)
        }
    } else {
        (1, biased as i64 - 1023)
    };
    let mut fraction = format!("{mantissa:013x}");
    while fraction.ends_with('0') {
        fraction.pop();
    }
    let point = if !fraction.is_empty() || alternate {
        "."
    } else {
        ""
    };
    let sign = if exponent < 0 { '-' } else { '+' };
    let text = format!(
        "0x{lead}{point}{fraction}p{sign}{}",
        exponent.unsigned_abs()
    );
    if upper {
        text.to_ascii_uppercase()
    } else {
        text
    }
}
