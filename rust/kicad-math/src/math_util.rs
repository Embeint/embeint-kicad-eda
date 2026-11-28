// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad util.cpp, copyright Michael Niedermayer, CERN and KiCad Developers.
//! Exact source-unit arithmetic and logging at KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Rescale functions explicitly select the Linux `__int128` reference contract (including
//! low-bit signed narrowing). They do not certify the MSVC or no-int128 implementations.
//!
//! `kimathLogDebug` and `kimathLogOverflow` are ported in full: the level query, the
//! `wxString` conversions and the formatting they perform, and the wx calls they make, in
//! source order. Those wx calls are host effects ([`KimathLog`]): the logging host
//! (`wxLog::IsLevelEnabled` and the active log target) and the assertion handler
//! (`wxOnAssert`, reached by `wxFAIL_MSG`). No unit ports the wx host, so the production
//! [`PinnedLog`] defers at the first host call. [`RecordingLog`] records each call and returns,
//! as wx 3.2.4 does in the reference process (no `wxApp`: the default assertion handler
//! prints and returns, the default log level is `wxLOG_Max`), so callers' continuations after
//! a logging call can be compared. See the shim's AUDIT.md.

use core::cell::RefCell;

pub const UNIT: &str = "libs.kimath.src.math.util.application.ui.p001";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathUtilError {
    ZeroDenominator,
    /// Checked intermediate operation failed. Unreachable for the public integer widths.
    IntermediateOverflow,
    /// `kimathLogDebug` would format with arguments that do not match its format string (a
    /// missing or mistyped vararg is undefined behaviour). Only reached when the message is
    /// actually formatted, that is when both level queries enabled it.
    UndefinedFormat,
    Unported {
        upstream: &'static str,
        unit: &'static str,
    },
}

/// The source's unsigned-128 sum and final unsigned-64 increment wrap modulo their widths.
/// Preserve that behavior even beyond the header's documented magnitudes of at most 2^63.
/// In particular, this is not an unbounded mathematical hypotenuse for arbitrary u64 inputs.
pub fn rounded_hypot_wide(x: u64, y: u64) -> u64 {
    // Each individual square fits exactly; only the sum can wrap.
    let n = (u128::from(x) * u128::from(x)).wrapping_add(u128::from(y) * u128::from(y));
    let mut floor = 0_u64;
    for bit in (0..64).rev() {
        let candidate = floor | (1_u64 << bit);
        if u128::from(candidate) * u128::from(candidate) <= n {
            floor = candidate;
        }
    }
    // The floor invariant makes subtraction exact. Integer inputs cannot tie at f + 1/2.
    let round_up = n - u128::from(floor) * u128::from(floor) > u128::from(floor);
    floor.wrapping_add(u64::from(round_up))
}

/// KiCad's int specialization, including low-32-bit signed narrowing of the quotient.
/// No numerator/denominator ordering restriction is imposed by the body.
pub fn rescale_i32(numerator: i32, value: i32, denominator: i32) -> Result<i32, MathUtilError> {
    let quotient = rescale_quotient(numerator.into(), value.into(), denominator.into())?;
    Ok(quotient as i32)
}

/// KiCad's `__SIZEOF_INT128__` specialization, including low-64-bit signed narrowing.
/// Halfway results round away from zero, for both signs of the denominator.
pub fn rescale_i64_int128(
    numerator: i64,
    value: i64,
    denominator: i64,
) -> Result<i64, MathUtilError> {
    let quotient = rescale_quotient(numerator, value, denominator)?;
    Ok(quotient as i64)
}

fn rescale_quotient(numerator: i64, value: i64, denominator: i64) -> Result<i128, MathUtilError> {
    if denominator == 0 {
        return Err(MathUtilError::ZeroDenominator);
    }
    let product = i128::from(numerator)
        .checked_mul(i128::from(value))
        .ok_or(MathUtilError::IntermediateOverflow)?;
    // Divide before adding/subtracting, as in C++; signed division truncates toward zero.
    let half = i128::from(denominator / 2);
    let adjusted = if (product < 0) ^ (denominator < 0) {
        product.checked_sub(half)
    } else {
        product.checked_add(half)
    }
    .ok_or(MathUtilError::IntermediateOverflow)?;
    adjusted
        .checked_div(i128::from(denominator))
        .ok_or(MathUtilError::IntermediateOverflow)
}

/// `wxLOG_Warning` (wx/log.h `wxLogLevelValues`), the level `wxVLogWarning` logs at.
pub const WX_LOG_WARNING: u32 = 2;
/// `wxLOG_Debug`, the level `kimathLogDebug` queries.
pub const WX_LOG_DEBUG: u32 = 6;
/// `wxLOG_Max`, wx 3.2.4's initial global log level (`wxLog::ms_logLevel`).
pub const WX_LOG_MAX: u32 = 10000;
/// `wxLOG_COMPONENT` in util.cpp: KiCad does not define it, so it is wx's exported
/// `const char* wxLOG_COMPONENT = ""`.
pub const LOG_COMPONENT: &str = "";
/// The upstream symbol of the assertion handler `wxFAIL_MSG` calls.
pub const WX_ON_ASSERT: &str = "wxOnAssert";
/// The upstream symbol of the log level query.
pub const WX_LOG_IS_LEVEL_ENABLED: &str = "wxLog::IsLevelEnabled";
/// The upstream symbol of the record delivery to the active log target.
pub const WX_LOG_ON_LOG: &str = "wxLog::OnLog";

/// `__LINE__` of `wxVLogWarning` in the pinned util.cpp (`kimathLogDebug`).
pub const LOG_DEBUG_LINE: u32 = 48;
/// `__LINE__` of `wxFAIL_MSG` in the pinned util.cpp (`kimathLogOverflow`).
pub const LOG_OVERFLOW_LINE: u32 = 58;

/// The arguments of the `wxOnAssert( __FILE__, __LINE__, __WXFUNCTION__, cond, msg )` call a
/// `wxFAIL_MSG` expansion makes when `wxTheAssertHandler` is set (wx 3.2 `wxFAIL_MSG( msg )`
/// -> `wxFAIL_COND_MSG( "Assert failure", msg )` -> `wxFAIL_COND_MSG_AT`, which passes
/// `#cond`). `__FILE__` is the compiler's spelling of the util.cpp path and is left to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WxAssertCall {
    /// `__LINE__` in the pinned util.cpp.
    pub line: u32,
    /// `__WXFUNCTION__` (GCC's `__FUNCTION__`).
    pub function: &'static str,
    /// `#cond` of the already quoted `"Assert failure"`, so the text keeps its double quotes.
    pub condition: &'static str,
    /// The formatted message.
    pub message: String,
}

/// The record a `wxLogger` passes to `wxLog::OnLog` (level, the formatted message and the
/// `wxLogRecordInfo` source fields; the timestamp and thread id are left to the host).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WxLogCall {
    pub level: u32,
    pub message: String,
    /// `__LINE__` of the logging macro in the pinned util.cpp.
    pub line: u32,
    /// `__WXFUNCTION__`.
    pub function: &'static str,
    /// `wxLOG_COMPONENT`.
    pub component: &'static str,
}

/// One host call that returned, in call order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEvent {
    Assert(WxAssertCall),
    Log(WxLogCall),
}

/// The wx host effects util.cpp's logging reaches. `Ok` means the host call returned (and, for
/// an assertion, did not request `wxTrap()`); an error stops the evaluation there.
pub trait KimathLog {
    /// `wxLog::IsLevelEnabled( level, component )`.
    fn is_level_enabled(&self, level: u32, component: &str) -> Result<bool, MathUtilError>;
    /// `wxLog::OnLog`: deliver one formatted record to the active log target.
    fn log(&self, call: &WxLogCall) -> Result<(), MathUtilError>;
    /// `wxOnAssert`, called only when `wxTheAssertHandler` is set; a host without a handler
    /// returns `Ok` without effect, exactly as the macro then makes no call.
    fn on_assert(&self, call: &WxAssertCall) -> Result<(), MathUtilError>;
}

/// The production host: no unit ports the wx log and assertion host, so the first call to it
/// defers. The calls are made from this unit's own bodies, so the deferrals name this unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinnedLog;

/// [`PinnedLog`] as a value that callers can borrow for the default (production) host.
pub const PINNED_LOG: PinnedLog = PinnedLog;

impl KimathLog for PinnedLog {
    fn is_level_enabled(&self, _level: u32, _component: &str) -> Result<bool, MathUtilError> {
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): wxLog::IsLevelEnabled
        Err(MathUtilError::Unported {
            upstream: WX_LOG_IS_LEVEL_ENABLED,
            unit: UNIT,
        })
    }

    fn log(&self, _call: &WxLogCall) -> Result<(), MathUtilError> {
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): wxLog::OnLog
        Err(MathUtilError::Unported {
            upstream: WX_LOG_ON_LOG,
            unit: UNIT,
        })
    }

    fn on_assert(&self, _call: &WxAssertCall) -> Result<(), MathUtilError> {
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): wxOnAssert
        Err(MathUtilError::Unported {
            upstream: WX_ON_ASSERT,
            unit: UNIT,
        })
    }
}

/// A host that records every call and returns, as the reference process does: wx 3.2.4 with
/// no `wxApp` has an assertion handler (`wxDefaultAssertHandler` prints the message and
/// returns `false`, so `wxTrapInAssert` stays false) and an active log target. The level
/// query models the global level and `wxLog::EnableLogging` (the component is always
/// `wxLOG_COMPONENT = ""`, whose level is the global one). Diagnostic comparison of the
/// continuation only; not production behaviour.
#[derive(Debug)]
pub struct RecordingLog {
    level: u32,
    enabled: bool,
    events: RefCell<Vec<LogEvent>>,
}

impl Default for RecordingLog {
    fn default() -> Self {
        Self::new(WX_LOG_MAX, true)
    }
}

impl RecordingLog {
    /// A recorder for global level `level` with logging enabled (`EnableLogging( enabled )`).
    pub fn new(level: u32, enabled: bool) -> Self {
        Self {
            level,
            enabled,
            events: RefCell::new(Vec::new()),
        }
    }

    /// The calls made so far, in order.
    pub fn events(&self) -> Vec<LogEvent> {
        self.events.borrow().clone()
    }

    fn push(&self, event: LogEvent) -> Result<(), MathUtilError> {
        // The host calls never nest, so the cell is never borrowed here already.
        self.events.borrow_mut().push(event);
        Ok(())
    }
}

impl KimathLog for RecordingLog {
    fn is_level_enabled(&self, level: u32, _component: &str) -> Result<bool, MathUtilError> {
        // IsEnabled() && level <= GetComponentLevel( "" ), the global level.
        Ok(self.enabled && level <= self.level)
    }

    fn log(&self, call: &WxLogCall) -> Result<(), MathUtilError> {
        self.push(LogEvent::Log(call.clone()))
    }

    fn on_assert(&self, call: &WxAssertCall) -> Result<(), MathUtilError> {
        self.push(LogEvent::Assert(call.clone()))
    }
}

/// Typed varargs of `kimathLogDebug`: `Signed` is a `long long`, `Float` a `double` (after
/// default argument promotion). `Unsigned` and `Text` have no conversion in the modelled format
/// grammar, so formatting them is [`MathUtilError::UndefinedFormat`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogArgument<'a> {
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Text(&'a str),
}

/// One conversion of the modelled printf grammar: literal text, `%%`, `%lld` and `%f`. No
/// flags, width, precision or other length modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatPiece<'a> {
    Literal(&'a str),
    Percent,
    LongLong,
    Double,
}

/// Splits a format into [`FormatPiece`]s; `None` when it uses anything outside the grammar.
/// The pinned caller's `"Overflow in rescale (%lld * %lld + %lld) / %lld"` is inside it.
pub fn parse_format(format: &str) -> Option<Vec<FormatPiece<'_>>> {
    let mut pieces = Vec::new();
    let mut rest = format;
    while !rest.is_empty() {
        match rest.find('%') {
            Some(0) => {
                let (piece, len) = if rest.starts_with("%%") {
                    (FormatPiece::Percent, 2)
                } else if rest.starts_with("%lld") {
                    (FormatPiece::LongLong, 4)
                } else if rest.starts_with("%f") {
                    (FormatPiece::Double, 2)
                } else {
                    return None;
                };
                pieces.push(piece);
                rest = rest.get(len..)?;
            }
            Some(at) => {
                pieces.push(FormatPiece::Literal(rest.get(..at)?));
                rest = rest.get(at..)?;
            }
            None => {
                pieces.push(FormatPiece::Literal(rest));
                rest = "";
            }
        }
    }
    Some(pieces)
}

/// glibc's `%f` (precision 6, no flags) of a binary64 in the C locale: the exact decimal
/// value rounded to six fraction digits, ties to even (round-to-nearest), the sign of
/// negative zero and of negative values rounding to zero kept, `inf`/`-inf` and `nan`/`-nan`
/// by the sign bit. Core's fixed-precision formatting rounds the exact value ties to even.
pub fn printf_f(value: f64) -> String {
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        }
        .into();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.into();
    }
    // Display keeps the sign of -0.0 and of negative values that round to zero, as glibc does.
    format!("{value:.6}")
}

/// `wxString::FormatV` of a modelled format with its varargs (glibc `vswprintf` in the C
/// locale). Excess arguments are evaluated but ignored, as C specifies; a missing or
/// mistyped argument is [`MathUtilError::UndefinedFormat`].
pub fn format_printf(format: &str, arguments: &[LogArgument<'_>]) -> Result<String, MathUtilError> {
    let pieces = parse_format(format).ok_or(MathUtilError::UndefinedFormat)?;
    let mut out = String::new();
    let mut next = arguments.iter();
    for piece in pieces {
        match piece {
            FormatPiece::Literal(text) => out.push_str(text),
            FormatPiece::Percent => out.push('%'),
            FormatPiece::LongLong => match next.next() {
                Some(LogArgument::Signed(v)) => out.push_str(&v.to_string()),
                _ => return Err(MathUtilError::UndefinedFormat),
            },
            FormatPiece::Double => match next.next() {
                Some(LogArgument::Float(v)) => out.push_str(&printf_f(*v)),
                _ => return Err(MathUtilError::UndefinedFormat),
            },
        }
    }
    Ok(out)
}

/// `wxString( const char* )` (and `wxString::FromAscii` of the same ASCII text): the bytes up
/// to the first NUL, converted with `wxConvLibc` in the C locale. glibc's C locale converts
/// only ASCII, so any byte above 0x7F fails the whole conversion and wx yields an empty string.
pub fn wx_string_from_c(text: &str) -> &str {
    let text = text.split('\0').next().unwrap_or_default();
    if text.is_ascii() {
        text
    } else {
        ""
    }
}

/// `void kimathLogDebug( const char* aFormatString, ... )`:
/// `if( wxLog::IsLevelEnabled( wxLOG_Debug, wxString::FromAscii( wxLOG_COMPONENT ) ) )`
/// `wxVLogWarning( aFormatString, argList )`. Despite its name it logs a **warning**.
/// `wxVLogWarning` is `wxLogger( wxLOG_Warning, __FILE__, __LINE__, __WXFUNCTION__,
/// wxLOG_COMPONENT ).LogV( format, argptr )`, which queries the warning level again, then
/// formats (`wxString::FormatV`) and passes the record to `wxLog::OnLog`.
pub fn kimath_log_debug(
    log: &dyn KimathLog,
    format: &str,
    arguments: &[LogArgument<'_>],
) -> Result<(), MathUtilError> {
    if log.is_level_enabled(WX_LOG_DEBUG, wx_string_from_c(LOG_COMPONENT))? {
        // The const char* format converts to the wxString parameter of LogV.
        let format = wx_string_from_c(format);
        if log.is_level_enabled(WX_LOG_WARNING, LOG_COMPONENT)? {
            let message = format_printf(format, arguments)?;
            log.log(&WxLogCall {
                level: WX_LOG_WARNING,
                message,
                line: LOG_DEBUG_LINE,
                function: "kimathLogDebug",
                component: LOG_COMPONENT,
            })?;
        }
    }
    Ok(())
}

/// The message of `kimathLogOverflow`: `wxString::Format( wxT( "\n\nOverflow converting value
/// %f to %s." ), v, typeName )` with `wxString typeName( aTypeName )`.
pub fn overflow_message(value: f64, type_name: &str) -> String {
    format!(
        "\n\nOverflow converting value {} to {}.",
        printf_f(value),
        wx_string_from_c(type_name)
    )
}

/// `void kimathLogOverflow( double v, const char* aTypeName )`: builds the type name and the
/// message, then `wxFAIL_MSG`, which calls `wxOnAssert` when a handler is installed and
/// returns when the handler does. The function returns nothing.
pub fn kimath_log_overflow(
    log: &dyn KimathLog,
    value: f64,
    type_name: &str,
) -> Result<(), MathUtilError> {
    let message = overflow_message(value, type_name);
    log.on_assert(&WxAssertCall {
        line: LOG_OVERFLOW_LINE,
        function: "kimathLogOverflow",
        condition: "\"Assert failure\"",
        message,
    })
}
