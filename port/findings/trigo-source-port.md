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
`std::abs( INT_MIN )`, and (as a library precondition) a NaN given to `std::minmax` in the
three-point `VECTOR2D` `CalcArcCenter`. The oracle adapter guards each in C++ order and the Rust
port refuses the same requests (`TrigoError::Undefined`). A NaN can reach `std::minmax` only from
a NaN input there: with non-NaN inputs, `snapArcCenter`'s radii are computed only for finite
candidates, so no NaN appears. The `KI_INT128` numerators of `unclampedArcCenter` stay below
2^100. Request generation avoids sin/cos/atan2: rotations away from the shortcuts are admitted
only where the following int sums are safe for every value within the magnitude bound.

## Reference toolchain

`std::minmax( initializer_list )` was checked against GCC 13.3's `__minmax_element` (first pair
ordered; the last element replaces the minimum when strictly smaller, otherwise the maximum when
not smaller). `std::fma` is the correctly rounded IEEE operation on both the glibc software path
and Rust's `f64::mul_add`; NaN results follow x86 rules explicitly. Local evidence used a GCC 13.3
build on glibc 2.43 with the hosted `GLIBC_TUNABLES`; the hosted run is the reference.

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
