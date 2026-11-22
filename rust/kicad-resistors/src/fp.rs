// SPDX-License-Identifier: GPL-3.0-or-later
//! Observable NaN rules of the pinned GCC/x86-64 resistor value helpers.
//!
//! Rust/wasm arithmetic does not specify which input NaN payload survives. The
//! compiled reference helpers select the second NaN operand, or the first when
//! only it is NaN, and set its quiet bit. Retain every other sign/payload bit.
//! With no NaN input, an invalid operation generates x86's negative indefinite
//! NaN. These are measured reference operation semantics, not normalization of
//! input values, stored parts, constructor results, or compared replies.

const EXPONENT: u64 = 0x7ff0_0000_0000_0000;
const FRACTION: u64 = 0x000f_ffff_ffff_ffff;
const QUIET: u64 = 0x0008_0000_0000_0000;
const INDEFINITE: u64 = 0xfff8_0000_0000_0000;

pub(crate) fn is_nan(value: f64) -> bool {
    let bits = value.to_bits();
    bits & EXPONENT == EXPONENT && bits & FRACTION != 0
}

fn operand_nan(first: f64, second: f64) -> Option<f64> {
    if is_nan(second) {
        Some(f64::from_bits(second.to_bits() | QUIET))
    } else if is_nan(first) {
        Some(f64::from_bits(first.to_bits() | QUIET))
    } else {
        None
    }
}

fn generated_result(value: f64) -> f64 {
    if is_nan(value) {
        f64::from_bits(INDEFINITE)
    } else {
        value
    }
}

pub(crate) fn serial(first: f64, second: f64) -> f64 {
    if let Some(value) = operand_nan(first, second) {
        return value;
    }
    generated_result(first + second)
}

pub(crate) fn parallel(first: f64, second: f64) -> f64 {
    if let Some(value) = operand_nan(first, second) {
        return value;
    }
    generated_result(first * second / (first + second))
}

// buildSeriesData supplies a non-NaN decade/u16 multiplier and a u16 member.
// A zero first member yields infinity; infinity times zero generates indefinite.
pub(crate) fn scale(multiplier: f64, member: u16) -> f64 {
    generated_result(multiplier * f64::from(member))
}
