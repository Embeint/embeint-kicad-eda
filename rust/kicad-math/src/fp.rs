// SPDX-License-Identifier: GPL-3.0-or-later
//! Observable NaN results of `double` arithmetic in the pinned oracle build (GCC, x86-64 SSE2,
//! `-O3 -fno-fast-math -ffp-contract=off`).
//!
//! IEEE 754 fixes every non-NaN result of `+`, `-`, `*` and `/`, and Rust computes those exactly
//! on every target. It leaves the sign and payload of a NaN result open, and Rust/wasm do not
//! specify which input NaN survives or which NaN an invalid operation generates. These helpers
//! spell out the reference platform's behaviour, so native and wasm builds give the same bits:
//!
//! - one NaN operand: that operand with its quiet bit set, sign and payload kept;
//! - two NaN operands: the first (left) operand of the C++ expression, quieted;
//! - no NaN operand but a NaN result (`inf - inf`, `0 * inf`): x86's negative "indefinite"
//!   quiet NaN `0xfff8_0000_0000_0000`.
//!
//! Signalling NaNs are therefore always quieted by arithmetic. The first and third rules are the
//! x86-64 hardware's and match the live oracle on every case. The second is not a property of
//! the reference: IEEE 754 leaves the choice between two NaN operands open, x86 keeps the first
//! source operand of the instruction, and which C++ operand GCC places first varies from
//! statement to statement in the compiled reference. The left operand is a fixed, explicit
//! choice; the oracle cases where it differs are recorded as failures, not hidden
//! (`port/findings/kimath-header-hash-bezier-two-nan-payloads.md`). Nothing here alters a value
//! that is only stored or copied.

const EXPONENT: u64 = 0x7ff0_0000_0000_0000;
const FRACTION: u64 = 0x000f_ffff_ffff_ffff;
const QUIET: u64 = 0x0008_0000_0000_0000;
const INDEFINITE: u64 = 0xfff8_0000_0000_0000;

fn is_nan(value: f64) -> bool {
    let bits = value.to_bits();
    bits & EXPONENT == EXPONENT && bits & FRACTION != 0
}

fn quieted(value: f64) -> f64 {
    f64::from_bits(value.to_bits() | QUIET)
}

/// The result of one binary `double` operation whose IEEE result is `result`.
fn binary(first: f64, second: f64, result: f64) -> f64 {
    if is_nan(first) {
        quieted(first)
    } else if is_nan(second) {
        quieted(second)
    } else if is_nan(result) {
        f64::from_bits(INDEFINITE)
    } else {
        result
    }
}

/// `first + second`.
pub(crate) fn add(first: f64, second: f64) -> f64 {
    binary(first, second, first + second)
}

/// `first - second`.
pub(crate) fn sub(first: f64, second: f64) -> f64 {
    binary(first, second, first - second)
}

/// `first * second`.
pub(crate) fn mul(first: f64, second: f64) -> f64 {
    binary(first, second, first * second)
}

#[cfg(test)]
mod tests {
    use super::{add, mul, sub, INDEFINITE};

    const QNAN_A: u64 = 0x7ff8_0000_0000_00aa;
    const SNAN_B: u64 = 0xfff0_0000_0000_00bb;

    #[test]
    fn operand_nans_are_quieted_and_the_left_one_wins() {
        let a = f64::from_bits(QNAN_A);
        let b = f64::from_bits(SNAN_B);
        assert_eq!(add(a, b).to_bits(), QNAN_A);
        assert_eq!(mul(b, a).to_bits(), 0xfff8_0000_0000_00bb);
        assert_eq!(sub(1.0, b).to_bits(), 0xfff8_0000_0000_00bb);
        assert_eq!(mul(a, 2.0).to_bits(), QNAN_A);
    }

    #[test]
    fn invalid_operations_generate_the_indefinite_nan() {
        assert_eq!(sub(f64::INFINITY, f64::INFINITY).to_bits(), INDEFINITE);
        assert_eq!(mul(0.0, f64::NEG_INFINITY).to_bits(), INDEFINITE);
        assert_eq!(add(f64::NEG_INFINITY, f64::INFINITY).to_bits(), INDEFINITE);
        assert_eq!(add(-0.0, -0.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(sub(0.0, 0.0).to_bits(), 0);
    }
}
