// SPDX-License-Identifier: GPL-3.0-or-later
//! Private scalar interface: the x86-64 SSE2 two-operand `addsd/subsd/mulsd/divsd` results,
//! with operands in instruction order (`dst = dst OP src`).
//!
//! This replaces the crate's earlier private `fp` module. The operand order belongs to the
//! bound Bézier sites, not a public vector-arithmetic API. Keeping the helpers private avoids
//! making this unit depend on an open matrix implementation or exposing compiler-specific
//! operations through `kicad-math`. The rules agree with the separately reviewed matrix
//! prototype (`nanproto::sse2`, sha256
//! `6104c06356cb670be191b54ae9ff503ecdc6781f2545bc05b1712cbdd7f6ab7f`).
//!
//! Intel SDM vol. 1, table 4-7 ("Rules for handling NaNs", SSE/SSE2 column):
//!
//! 1. `dst` is a NaN: the result is `dst`, quieted (the quiet bit is set; sign and payload are
//!    kept), whatever `src` is;
//! 2. otherwise `src` is a NaN: the result is `src`, quieted;
//! 3. otherwise an invalid operation (`inf - inf`, `0 * inf`, `0 / 0`, `inf / inf`) gives the
//!    default NaN `0xfff8_0000_0000_0000`;
//! 4. otherwise the IEEE 754 result (round to nearest even; the reference never changes MXCSR).
//!
//! Rules 1-3 are applied explicitly, never inherited from the host, so native debug, native
//! release and wasm give the same bits. Rule 4 uses the host operation, which IEEE 754 fixes.
//! For `+` and `*` the instruction order is a compiler choice that no non-NaN result shows
//! (IEEE addition and multiplication are exactly commutative, signed zeros and infinities
//! included), so each call site passes the order of the bound reference build. For `-` and `/`
//! the encoding forces `dst` to be the left operand.

/// The quiet bit (most significant fraction bit) of a binary64 NaN.
const QUIET_BIT: u64 = 0x0008_0000_0000_0000;

/// The x86 SSE default NaN ("real indefinite") produced by an invalid operation.
pub(crate) const DEFAULT_NAN_BITS: u64 = 0xfff8_0000_0000_0000;

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
pub(crate) fn mul(dst: f64, src: f64) -> f64 {
    select(dst, src, dst * src)
}

/// `addsd src, dst`: `dst + src`.
#[inline]
pub(crate) fn add(dst: f64, src: f64) -> f64 {
    select(dst, src, dst + src)
}

/// `subsd src, dst`: `dst - src` (the left operand is always `dst`).
#[inline]
pub(crate) fn sub(dst: f64, src: f64) -> f64 {
    select(dst, src, dst - src)
}

/// `divsd src, dst`: `dst / src` (the left operand is always `dst`).
#[inline]
pub(crate) fn div(dst: f64, src: f64) -> f64 {
    select(dst, src, dst / src)
}

#[cfg(test)]
mod tests {
    use super::{add, div, mul, sub, DEFAULT_NAN_BITS};

    const QNAN_A: u64 = 0x7ff8_0000_0000_00aa;
    const SNAN_B: u64 = 0xfff0_0000_0000_00bb;

    fn f(bits: u64) -> f64 {
        f64::from_bits(bits)
    }

    #[test]
    fn destination_nan_wins_quieted_then_source_nan() {
        // An SNaN in src does not take precedence over a NaN dst.
        assert_eq!(add(f(QNAN_A), f(SNAN_B)).to_bits(), QNAN_A);
        assert_eq!(mul(f(SNAN_B), f(QNAN_A)).to_bits(), 0xfff8_0000_0000_00bb);
        assert_eq!(sub(1.0, f(SNAN_B)).to_bits(), 0xfff8_0000_0000_00bb);
        assert_eq!(div(f(QNAN_A), 0.0).to_bits(), QNAN_A);
        assert_eq!(mul(2.0, f(QNAN_A)).to_bits(), QNAN_A);
    }

    #[test]
    fn invalid_operations_without_nan_give_the_default_nan() {
        assert_eq!(
            sub(f64::INFINITY, f64::INFINITY).to_bits(),
            DEFAULT_NAN_BITS
        );
        assert_eq!(mul(0.0, f64::NEG_INFINITY).to_bits(), DEFAULT_NAN_BITS);
        assert_eq!(
            add(f64::NEG_INFINITY, f64::INFINITY).to_bits(),
            DEFAULT_NAN_BITS
        );
        assert_eq!(div(0.0, -0.0).to_bits(), DEFAULT_NAN_BITS);
        assert_eq!(
            div(f64::INFINITY, f64::NEG_INFINITY).to_bits(),
            DEFAULT_NAN_BITS
        );
    }

    #[test]
    fn non_nan_results_are_ieee_and_order_free() {
        assert_eq!(add(-0.0, -0.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(sub(0.0, 0.0).to_bits(), 0);
        assert_eq!(mul(-0.0, 5.0).to_bits(), mul(5.0, -0.0).to_bits());
        assert_eq!(div(1.0, -0.0), f64::NEG_INFINITY);
        assert_eq!(mul(f64::MAX, 2.0), f64::INFINITY);
        let tiny = f64::from_bits(1);
        assert_eq!(mul(tiny, 0.5).to_bits(), 0);
    }
}
