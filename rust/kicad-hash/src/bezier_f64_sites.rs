// SPDX-License-Identifier: GPL-3.0-or-later
//! `BEZIER<double>` evaluation with the operand order of the bound reference build.
//!
//! IEEE 754 fixes every non-NaN result, but not which NaN an operation with two NaN operands
//! returns. On x86-64 SSE2 that is the instruction's destination operand (see [`crate::sse2`]),
//! and which C++ operand GCC allocates to the destination register of a commutative `+` or `*`
//! varies from statement to statement, and between the inlined and out-of-line copies of the
//! same member. This module spells out that order for every arithmetic statement of the owned
//! `BEZIER<double>` bodies, as compiled in the bound reference object:
//!
//! - object `CMakeFiles/kicad-oracle.dir/shims/kimath_header_hash.cpp.o`, sha256
//!   `a6b793f510022586dc4b9f9469f2ff9a63f98de51ec33669ec8467bbd6670012`, linked into
//!   `kicad-oracle` sha256 `5e0a8c3620d69af0b7239be7852d8e26e0bc92e372e82e444613ec9dc3c6610f`;
//! - built from PR #51 `a5ceb98984d29ca9420527ce75fa481abd9c3727` (pin
//!   `4cf939778df04d2b747bb2c4714fc5014a7565c7`) by GCC 13.3.0-6ubuntu2~24.04.1 with
//!   `-O3 -DNDEBUG -std=gnu++20 -fno-fast-math -ffp-contract=off` (x86-64 baseline, SSE2, no
//!   AVX) in the PR #55 candidate reference image `sha256:8b70857a...a3597`.
//!
//! `rust/kicad-hash-shim/reference-bindings.json` records the normalised instruction sequence of
//! these sites; `rust/kicad-hash-shim/check_reference_bindings.py` refuses an oracle whose sites
//! differ, so a compiler, flag or adapter change is detected instead of silently compared.
//!
//! Every call below is `op(dst, src)` in instruction order and cites its instruction (object
//! section offset). The expression trees are the C++ ones; only the operands of `+` and `*` are
//! ever swapped, which no finite result can show, so the finite arithmetic order is unchanged.
//! Only `+` and `*` have a choice: `-` and `/` always take the C++ left operand as `dst`.
//!
//! Two kinds of choice can never be observed and are recorded only for fidelity: in `PointAt`,
//! every product of a coefficient with `Start`, `C2` or `End` feeds a sum whose `dst` is already
//! a NaN whenever the coefficient is (all coefficients are NaN exactly when `aT` is); and
//! `SubCurve` only reaches its degenerate `PointAt` with a clamped, non-NaN `t0`.

use kicad_math::Vector2D;

use crate::bezier::Bezier;
use crate::sse2::{add, mul, sub};

/// Which compiled copy of `BEZIER<double>::PointAt` a call reproduces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointAtSite {
    /// Inlined into the adapter's `point_at` call (`KimathHeaderHash` +0x24bf5..+0x24c74).
    Inline,
    /// The out-of-line `BEZIER<double>::PointAt` (section `.text._ZNK6BEZIERIdE7PointAtEd`),
    /// which the adapter's inlined `SubCurve` calls for an empty or reversed range.
    OutOfLine,
}

/// `PointAt( aT )` at the given site.
pub(crate) fn point_at(curve: &Bezier<f64>, t: f64, site: PointAtSite) -> Vector2D {
    match site {
        PointAtSite::Inline => point_at_inline(curve, t),
        PointAtSite::OutOfLine => point_at_out_of_line(curve, t),
    }
}

/// `KimathHeaderHash` +0x24bf5..+0x24c74. Packed (`mulpd`/`addpd`): x and y share the order.
fn point_at_inline(curve: &Bezier<f64>, t: f64) -> Vector2D {
    let t_m1 = sub(1.0, t); // 24c0d  1.0 - aT
    let t2 = mul(t, t); // 24c11  aT * aT
    let three_t = mul(t, 3.0); // 24c15  3.0 * aT
    let t_m1_2 = mul(t_m1, t_m1); // 24c21  t_m1 * t_m1
    let t3 = mul(t, t2); // 24c25  t2 * aT
    let three_t2 = mul(t2, 3.0); // 24c29  3.0 * t2
    let t_m1_3 = mul(t_m1, t_m1_2); // 24c2d  t_m1_2 * t_m1
    let k_c1 = mul(three_t, t_m1_2); // 24c31  ( 3.0 * aT ) * t_m1_2
    let k_c2 = mul(three_t2, t_m1); // 24c35  ( 3.0 * t2 ) * t_m1
    let component = |start: f64, c1: f64, c2: f64, end: f64| {
        let start = mul(t_m1_3, start); // 24c3d  t_m1_3 * Start
        let c1 = mul(k_c1, c1); // 24c4a  ( ... ) * C1
        let sum = add(c1, start); // 24c50  Start term + C1 term: dst is the C1 term
        let c2 = mul(k_c2, c2); // 24c5c  ( ... ) * C2
        let sum = add(sum, c2); // 24c62
        let end = mul(t3, end); // 24c6e  t3 * End
        add(sum, end) // 24c74
    };
    Vector2D::new(
        component(curve.start.x, curve.c1.x, curve.c2.x, curve.end.x),
        component(curve.start.y, curve.c1.y, curve.c2.y, curve.end.y),
    )
}

/// `.text._ZNK6BEZIERIdE7PointAtEd` +0x1b..+0x92. Packed: x and y share the order.
fn point_at_out_of_line(curve: &Bezier<f64>, t: f64) -> Vector2D {
    let t2 = mul(t, t); // 1b
    let t_m1 = sub(1.0, t); // 27
    let three_t = mul(t, 3.0); // 2b
    let t3 = mul(t, t2); // 2f  t2 * aT
    let t_m1_2 = mul(t_m1, t_m1); // 3b
    let three_t2 = mul(t2, 3.0); // 3f
    let t_m1_3 = mul(t_m1, t_m1_2); // 48  t_m1_2 * t_m1
    let k_c1 = mul(three_t, t_m1_2); // 4c
    let k_c2 = mul(t_m1, three_t2); // 50  ( 3.0 * t2 ) * t_m1: dst is t_m1
    let component = |start: f64, c1: f64, c2: f64, end: f64| {
        let start = mul(t_m1_3, start); // 60
        let c1 = mul(k_c1, c1); // 69
        let sum = add(c1, start); // 6d  dst is the C1 term
        let c2 = mul(k_c2, c2); // 81
        let sum = add(sum, c2); // 85
        let end = mul(end, t3); // 8e  t3 * End: dst is End
        add(sum, end) // 92
    };
    Vector2D::new(
        component(curve.start.x, curve.c1.x, curve.c2.x, curve.end.x),
        component(curve.start.y, curve.c1.y, curve.c2.y, curve.end.y),
    )
}

/// Operand order of one `a + aT * ( b - a )` statement of `Split`.
#[derive(Clone, Copy)]
enum ProductDst {
    /// `mulpd d, t`: the difference is `dst`.
    Difference,
    /// `mulpd t, d`: `aT` is `dst`.
    Parameter,
}

/// One `a + aT * ( b - a )` statement: `subpd` with `b` as `dst`, the product as given, then
/// `addpd` with the product as `dst`.
fn lerp(a: Vector2D, b: Vector2D, t: f64, product: ProductDst) -> Vector2D {
    let component = |a: f64, b: f64| {
        let difference = sub(b, a);
        let scaled = match product {
            ProductDst::Difference => mul(difference, t),
            ProductDst::Parameter => mul(t, difference),
        };
        add(scaled, a)
    };
    Vector2D::new(component(a.x, b.x), component(a.y, b.y))
}

/// `Split( aT, aLeft, aRight )`: the out-of-line `BEZIER<double>::Split`
/// (`.text._ZNK6BEZIERIdE5SplitEdRS0_S1_`), which every split of the adapter calls, including
/// both splits of its inlined `SubCurve`. Packed: x and y share the order.
pub(crate) fn split(curve: &Bezier<f64>, t: f64) -> (Bezier<f64>, Bezier<f64>) {
    use ProductDst::{Difference, Parameter};
    let start_c1 = lerp(curve.start, curve.c1, t, Difference); // 26 2e 3e
    let c1_c2 = lerp(curve.c1, curve.c2, t, Difference); // 22 2a 32
    let c2_end = lerp(curve.c2, curve.end, t, Difference); // 42 52 5a
    let left_c2 = lerp(start_c1, c1_c2, t, Difference); // 4a 56 5e
    let right_c1 = lerp(c1_c2, c2_end, t, Difference); // 66 6e 72
    let shared = lerp(left_c2, right_c1, t, Parameter); // 7a 7e 82
    (
        Bezier::new(curve.start, start_c1, left_c2, shared),
        Bezier::new(shared, right_c1, c2_end, curve.end),
    )
}
