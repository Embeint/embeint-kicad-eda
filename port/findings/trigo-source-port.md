<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# trigo.cpp source port: deferrals, domain and caller wiring

Unit: `libs.kimath.src.trigo.application.ui.p001` (`libs/kimath/src/trigo.cpp` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`).

## Header wrappers stay deferred

Two trigo.cpp bodies call trigo.h inline wrappers owned by `libs.kimath.include.application.ui.p001`
(status `todo`): `CalcArcMid` calls `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )`
and the chord/angle `CalcArcCenter` calls `RotatePoint( VECTOR2D&, const EDA_ANGLE& )`. Both are
one-line forwards to trigo.cpp overloads this unit ports, but they belong to the header unit, so
they remain typed deferrals behind one `HeaderCallees` seam; the continuations after them are
implemented and checked in crate tests with a forwarding seam (one general-angle `CalcArcMid`
value and two complete circle tan-tan constructions are asserted against oracle replies). When
the header unit ports those wrappers, replacing `PinnedHeader` removes 525 trigo deferrals and the
70 circle deferrals listed below.

## Undefined domain

The pinned bodies are undefined for int and int64 signed overflow, negating `INT_MIN`/`INT64_MIN`,
`std::abs( INT_MIN )`, and (as a library precondition) a `std::minmax` call in the three-point
`VECTOR2D` `CalcArcCenter` whose values break the strict weak ordering `operator<` must induce on
them ([alg.sorting.general]). For doubles only the transitivity of equivalence can fail: a NaN is
equivalent to every value, so a coordinate triple is undefined exactly when it holds a NaN and two
non-equivalent values (`{ 0, 1, NaN }`). `{ 0, NaN, 0 }`, `{ 0, NaN, -0 }`, two NaNs with one value
and three NaNs are defined; the first review of PR #103 showed the original blanket NaN refusal
removed them (the former `control-arc_center_d-nan-1` returns `( 1, NaN )` from the cluster
mean). The oracle adapter guards each case in C++ order and the Rust port refuses the same
requests (`TrigoError::Undefined`). `snapArcCenter`'s radii `std::minmax` is reached only for a
finite centre, which needs finite inputs, so it never sees a NaN. The `KI_INT128` numerators of `unclampedArcCenter` stay below
2^100. Request generation avoids sin/cos/atan2: rotations away from the shortcuts are admitted
only where the following int sums are safe for every value within the magnitude bound.

## Reference toolchain

`std::minmax( initializer_list )` was checked against GCC 13.3's `__minmax_element` (first pair
ordered; the last element replaces the minimum when strictly smaller, otherwise the maximum when
not smaller).

On the baseline x86-64 target GCC emits `std::fma` as `fma@plt`; under the reference tunables
glibc 2.39's IFUNC selects `__fma_sse2`, the generic dbl-64 s_fma.c. It is ported as
`kicad_math::glibc_libm::fma`, forwarded by `reference_libm::fma` and qualified against fresh
locked-libm records like the other backends (`port/glibc-libm`). The earlier `f64::mul_add` with a
first-NaN rule was not that body: glibc evaluates `x * y` before adding `z`, so
`fma( inf, 0, qNaN )` is the invalid product's `0xfff8000000000000`, not z's NaN.

SSE2 keeps the NaN of an instruction's destination operand, and GCC 13.3 picks the destination
per compiled site. In the three-point `CalcArcCenter` the inlined `det2` calls pass the `std::fma`
factors swapped and sum `e + f` for both numerators, the centre is `ux + start.x`, and
`RotatePoint` about a centre adds `cx + ox`; `trigo.rs` writes each in that order with the
instruction offset. Finite inputs reach these NaNs too (overflow to infinity, then `inf - inf`):
`( 1e-300, 100 ), ( 0, 1e300 ), ( 1e-300, -0 )` returns `( -inf, -NaN )`.

Local evidence used a GCC 13.3 build on a glibc 2.43 host with the hosted `GLIBC_TUNABLES`. That
host's `fma` is a different body with different NaN results (150 cases differ), so local runs
bind `fma` to the hash-checked locked 2.39 libm with a diagnostic preload; the hosted run, on
glibc 2.39 itself, is the reference.

`geometry/arc_construction.cpp`'s `crossZ` (accepted, another unit) also calls `std::fma` and
uses `f64::mul_add`; its inputs are finite, where both agree, but it bypasses the facade. Routing
it through `reference_libm::fma` is left to that unit's owner.

## Caller deferrals before and after

Counts from the Rust shims on accepted `origin/rust` (b23f1fb5d8) and on this branch:

| Caller unit | Deferral | Before | After |
| --- | --- | --- | --- |
| `libs.kimath.include.math.application.ui.p001` (BOX2) | `SegmentIntersectsSegment` (trigo) | 63 | 0 (all compared exactly) |
| `libs.kimath.src.geometry.circle.application.ui.p001` | `CalcArcMid` (trigo) | 70 cases, 2 controls | 0 |
| same | `RotatePoint( VECTOR2I&, const VECTOR2I&, EDA_ANGLE )` (trigo.h, via CalcArcMid) | 0 | 70 cases, 2 controls |

The vector and geometry corpora, which the BOX2 workflow also runs as callers, are unchanged.
Retained header-unit deferrals in accepted callers: `box2.rs` `RotatePoint(VECTOR2I&, EDA_ANGLE)`
and `RotatePoint(VECTOR2I&, VECTOR2I, EDA_ANGLE)` (78 BOX2 cases), `circle.rs`
`RotatePoint(VECTOR2I&, EDA_ANGLE)` (193 cases, 4 controls) and `shape_circle.rs`
`RotatePoint(VECTOR2I&, const VECTOR2I&, EDA_ANGLE)`.
