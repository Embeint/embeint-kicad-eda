// SPDX-License-Identifier: GPL-3.0-or-later
//! Portable binary64 arithmetic with the NaN results of the x86-64 SSE2 two-operand scalar
//! instructions `addsd/subsd/mulsd/divsd src, dst` (AT&T order; `dst = dst OP src`).
//!
//! Intel SDM vol. 1, table 4-7 ("Rules for handling NaNs", SSE/SSE2 column):
//!
//! 1. `dst` is a NaN: the result is `dst`, quieted (an SNaN gets the quiet bit; sign and
//!    payload are kept). This holds whatever `src` is.
//! 2. otherwise `src` is a NaN: the result is `src`, quieted.
//! 3. otherwise an invalid operation (`inf - inf`, `0 * inf`, `0 / 0`, `inf / inf`): the
//!    default "real indefinite" QNaN `0xfff8_0000_0000_0000`.
//! 4. otherwise the IEEE 754 correctly rounded result (round to nearest even, no FTZ/DAZ:
//!    the default MXCSR, which the oracle never changes).
//!
//! The functions take `dst` and `src` in *instruction* order. For `+` and `*` that order is a
//! compiler register-allocation choice that is invisible for non-NaN results (IEEE 754
//! addition and multiplication are exactly commutative, including signed zeros and
//! infinities), so each call site in [`crate::generated`] states the order the pinned oracle
//! binary actually uses. For `-` and `/` the SSE2 encoding makes `dst` the left operand.
//!
//! Rules 1-3 are applied explicitly, never inherited from the host: native x86-64, AArch64,
//! debug, release and wasm (where the NaN produced by arithmetic is nondeterministic) give
//! the same bits. Rule 4 uses the host operation, which IEEE 754 fully specifies.
//!
//! No libm, no global state: only `is_nan`, bit casts and the four basic operations.

/// The quiet bit (most significant fraction bit) of a binary64 NaN.
const QUIET_BIT: u64 = 0x0008_0000_0000_0000;

/// The x86 SSE default NaN ("real indefinite") produced by an invalid operation.
pub const DEFAULT_NAN_BITS: u64 = 0xfff8_0000_0000_0000;

/// `x` with the quiet bit set. Only bit operations: the sign and payload are preserved.
#[inline]
fn quiet(x: f64) -> f64 {
    f64::from_bits(x.to_bits() | QUIET_BIT)
}

/// Applies rules 1-3 to an instruction's operands and its host-computed value.
#[inline]
fn select(dst: f64, src: f64, value: f64) -> f64 {
    if dst.is_nan() {
        quiet(dst)
    } else if src.is_nan() {
        quiet(src)
    } else if value.is_nan() {
        f64::from_bits(DEFAULT_NAN_BITS)
    } else {
        value
    }
}

/// `mulsd src, dst`: `dst * src`.
#[inline]
pub fn mul(dst: f64, src: f64) -> f64 {
    select(dst, src, dst * src)
}

/// `addsd src, dst`: `dst + src`.
#[inline]
pub fn add(dst: f64, src: f64) -> f64 {
    select(dst, src, dst + src)
}

/// `subsd src, dst`: `dst - src` (the left operand is always `dst`).
#[inline]
pub fn sub(dst: f64, src: f64) -> f64 {
    select(dst, src, dst - src)
}

/// `divsd src, dst`: `dst / src` (the left operand is always `dst`).
#[inline]
pub fn div(dst: f64, src: f64) -> f64 {
    select(dst, src, dst / src)
}

#[cfg(test)]
mod tests {
    use super::{add, div, mul, sub, DEFAULT_NAN_BITS};

    /// 6,916 cases measured on x86-64 from explicit `addsd/subsd/mulsd/divsd` instructions
    /// (`op dst src result`, binary64 words in hex): 1,729 operand pairs per operation over
    /// signed zeros, subnormals, normals, infinities and QNaNs/SNaNs of both signs with
    /// distinct payloads; 473 pairs per operation have a NaN operand, 121 have two.
    const PRIMITIVES: &str = include_str!("../tests/data/sse2_primitives.txt");

    fn word(text: &str) -> u64 {
        let parsed = u64::from_str_radix(text, 16);
        assert!(parsed.is_ok(), "malformed word {text:?}");
        parsed.unwrap_or_default()
    }

    #[test]
    fn every_measured_sse2_primitive_matches() {
        let mut checked = 0;
        for line in PRIMITIVES.lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 4, "{line}");
            let (dst, src) = (
                f64::from_bits(word(fields[1])),
                f64::from_bits(word(fields[2])),
            );
            let got = match fields[0] {
                "add" => Some(add(dst, src)),
                "sub" => Some(sub(dst, src)),
                "mul" => Some(mul(dst, src)),
                "div" => Some(div(dst, src)),
                _ => None,
            };
            assert_eq!(got.map(f64::to_bits), Some(word(fields[3])), "{line}");
            checked += 1;
        }
        assert_eq!(checked, 6916);
    }

    #[test]
    fn invalid_operations_give_the_sse2_default_nan() {
        let (inf, zero) = (f64::INFINITY, 0.0_f64);
        for got in [
            sub(inf, inf),
            add(inf, -inf),
            mul(zero, inf),
            mul(-inf, -zero),
            div(zero, -zero),
            div(-inf, inf),
        ] {
            assert_eq!(got.to_bits(), DEFAULT_NAN_BITS);
        }
    }

    #[test]
    fn destination_nan_wins_then_source_nan_quieted() {
        let snan = f64::from_bits(0x7ff0_0000_0000_0005);
        let qnan = f64::from_bits(0xfff8_0000_0000_0009);
        assert_eq!(mul(snan, qnan).to_bits(), 0x7ff8_0000_0000_0005);
        assert_eq!(mul(qnan, snan).to_bits(), 0xfff8_0000_0000_0009);
        assert_eq!(add(1.0, snan).to_bits(), 0x7ff8_0000_0000_0005);
        assert_eq!(div(snan, 0.0).to_bits(), 0x7ff8_0000_0000_0005);
    }
}
