// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from KiCad util.cpp, copyright Michael Niedermayer, CERN and KiCad Developers.
//! Exact source-unit arithmetic at KiCad pin 4cf939778df04d2b747bb2c4714fc5014a7565c7.
//!
//! Rescale functions explicitly select the Linux `__int128` reference contract (including
//! low-bit signed narrowing). They do not certify the MSVC or no-int128 implementations.
//! Host logging is deliberately deferred; see the shim's AUDIT.md for its required effects.

pub const UNIT: &str = "libs.kimath.src.math.util.application.ui.p001";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathUtilError {
    ZeroDenominator,
    /// Checked intermediate operation failed. Unreachable for the public integer widths.
    IntermediateOverflow,
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

/// Typed argument transport for a future native printf/wx bridge, not a Rust formatter.
/// Format validation and C variadic ABI conversion belong to that bridge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogArgument<'a> {
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Text(&'a str),
}

/// Requires a host checking wxLOG_Debug for wxLOG_COMPONENT and calling wxVLogWarning
/// with the original format/arguments when enabled. It must not emit a debug-level message.
pub fn kimath_log_debug(
    _format: &str,
    _arguments: &[LogArgument<'_>],
) -> Result<(), MathUtilError> {
    // UNPORTED(libs.kimath.src.math.util.application.ui.p001): kimathLogDebug
    Err(MathUtilError::Unported {
        upstream: "kimathLogDebug",
        unit: UNIT,
    })
}

/// Requires a host performing wxString conversion/formatting and wxFAIL_MSG with the
/// host's assertion build policy. Returning this error never represents assertion success.
pub fn kimath_log_overflow(_value: f64, _type_name: &str) -> Result<(), MathUtilError> {
    // UNPORTED(libs.kimath.src.math.util.application.ui.p001): kimathLogOverflow
    Err(MathUtilError::Unported {
        upstream: "kimathLogOverflow",
        unit: UNIT,
    })
}
