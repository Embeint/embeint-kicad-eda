<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# SEG source port audit

Unit `libs.kimath.src.geometry.seg.application.ui.p001`: `libs/kimath/src/geometry/seg.cpp` at
pin `4cf939778df04d2b747bb2c4714fc5014a7565c7` (788 lines, 25 owned definitions), ported to
`kicad_math::seg` (`rust/kicad-math/src/seg.rs`).

## Method

- **Port.** Every body follows the C++ evaluation order. Coordinates are `i32`, `ecoord` is
  `i64`. Callees are the accepted ports: `VECTOR2` arithmetic (`kicad_math::vector`), util.h
  `KiROUND`, `isqrt`, `std::min`/`std::max` semantics (`kicad_math::util`), util.cpp
  `rescale<int>` and the Linux `__int128` `rescale<int64_t>` (`kicad_math::math_util`),
  `CrossWide`/`ToDouble` (`kicad_math::wide_int`) and `EDA_ANGLE` with the glibc 2.39 `atan2`
  (`kicad_math::eda_angle`).
- **Undefined domain.** Signed overflow, `-INT_MIN` and an out-of-range `static_cast<ecoord>`
  are undefined in the C++. The port returns `SegError::Undefined` at the first such
  operation. Conversions the C++ defines keep their values: C++20 modular `int64_t -> int`
  (`Distance`, `Collide`'s `aActual`, `LineDistance`), the clamping `VECTOR2<ecoord> ->
  VECTOR2I` converting constructor (`NearestPoint`, `ReflectPoint`, `LineProject`) and the
  modular narrowing inside `rescale`.
- **Oracle adapter** (`port/oracle/shims/kimath_src_seg.cpp`). It calls the genuine pinned
  functions. Before each call a guard walks the same evaluation with `__int128` checks and
  refuses an undefined one. Where control flow depends on a value, the guard takes it from the
  genuine function, called only after that function's own guard has passed.
- **Corpus** (`generate_cases.py`). An exact Python mirror (arbitrary-precision integers, IEEE
  doubles) classifies each fixed-seed candidate and labels the branches it reaches:
  - *defined*: enters `port/cases`.
  - *undefined before any deferred call*: becomes a control in
    `port/oracle/fixtures/seg-controls.jsonl`.
  - *undefined only after a call Rust defers*: dropped and counted (197). Rust would stop at
    the deferral while C++ refuses later.

  The Rust port, the C++ guards and the Python mirror are three independent models of the
  undefined domain. The controls and the public corpus check that they agree.

## Evidence table

The public corpus has 9,503 cases. Local non-reference counts are from WSL Ubuntu 26.04 (GCC 15.2,
glibc 2.43). The reference counts come from the hosted Ubuntu 24.04 `rust-seg-live` run on the PR head.

| Upstream symbol | Implemented body and branches | Successful operation checked (observable) | Remaining gap |
| --- | --- | --- | --- |
| `sgn<T>` | `( 0 < v ) - ( v < 0 )` | through `LineDistance( aP, true )` signed results | none |
| `SquaredDistance( const SEG& )` | this point, other point, intersecting -> 0, four endpoint distances (in order), saturating norms | int64 value; all five branches labelled | none |
| `Angle` | `A - B` for both, `EDA_ANGLE( VECTOR2D )`, `Normalize180`, difference, `Normalize180`, `std::abs` | IEEE bits of degrees, 281 cases | none |
| `NearestPoint( const SEG& )` | `Intersect` hit; four candidates, first minimal distance | point | none |
| `NearestPoints` | intersection -> both points, dist 0; otherwise the four pairs | returned, both points, dist | none |
| `checkCollinearOverlap` (private) | x/y axis, overlap test, endpoint-touch ignore, midpoint `int` add and truncating `/ 2`, `rescale<int>` interpolation or degenerate axis | through `Intersect`, `Intersects`, `Collide`: endpoint-touch-ignored 9, zero-overlap-not-endpoint 5, interpolated 164, degenerate axis 45 | The disjoint return is unreachable: the caller's bounding-box test already guarantees overlap. Ported but not exercised |
| `intersects` (private) | bbox rejection, determinant, parallel, collinear lines (other point, this point, `KiROUND` midpoint), collinear segments, parameter bounds for both signs, endpoint ignore, `rescale<int64_t>` point, coordinate-overflow refusal | through `Intersect` (all argument variants, defaults omitted), `Intersects`, `Collide`, `SquaredDistance`, `NearestPoint(s)`: each labelled branch reached, including point overflow (2) | none |
| `Intersects` | `intersects( aSeg )` with defaults | bool | none |
| `Intersect` | the three C++ argument forms | point or null | none |
| `IntersectsLine` | vertical hit/miss (compare before rounding), `static_cast<ecoord>( aSlope * 1000 )`, parallel collinear midpoint / miss, parametric `t` in [0, 1] with `KiROUND( x, y )` | returned flag, out-parameter kept on false; NaN, infinities, signed zero and huge slopes and offsets | none |
| `PerpendicularSeg`, `ParallelSeg` | `B - A`, `Perpendicular`, `+ aP`, index -1 | both points and index | none |
| `Collide` | negative clearance, both degenerate branches, intersection, four distances with zero short-circuit, `isqrt` within / clear | returned and `aActual` (omitted -> nullptr, or an initial value) | none |
| `Contains( const VECTOR2I&, int )` | `SquaredDistance( aP ) <= threshold`, default 3 | bool | none |
| `NearestPoint( const VECTOR2I& )` | degenerate, before A, after B, inside with `rescale<int64_t>` and clamping return | point | none |
| `ReflectPoint` | degenerate (`c = aP`), general, `2 * c - aP` with clamping return | point | none |
| `LineProject` | degenerate, general with clamping return | point | none |
| `Distance` (both) | `int( isqrt( ... ) )` modular | int | none |
| `SquaredDistance( const VECTOR2I& )` | before A, after B (`bp.Dot( bp )`), Lagrange identity through `CrossWide`/`ToDouble`, the `g > INT64_MAX` / NaN -> 0 branch (6 cases), nonquiet `KiROUND<double, int64_t>` | int64 value | `g == 2^63` would log through `kimathLogOverflow` (util.cpp deferral). Not reached by the corpus |
| `LineDistance` | coefficients, `rescale( det, det, l )` only when `l > 0`, negative narrowed quotient into `isqrt`, `sgn * dist`, modular cast | int, both argument forms | none |
| `mutualDistanceSquared` (private) | swap by `SquaredLength`, coefficients, `l == 0`, signed `rescale` distances | not reached: deferred at `SEG::SquaredLength` | The coefficient and determinant helpers are shared with `LineDistance` and covered there |
| `ApproxCollinear`, `ApproxParallel` | `Square`, mutual distances, `std::abs` (undefined for `INT64_MIN`), short-circuit order | deferred: `SEG::Square` (858 cases across the three) | Owner `libs.kimath.include.geometry.application.ui.p003` |
| `ApproxPerpendicular` | `PerpendicularSeg( A )`, then `aSeg.ApproxParallel( perp )` | Undefined `PerpendicularSeg` refused (64 controls); otherwise deferred at `SEG::Square` | as above |

## Deferrals and co-ports

| Deferred call | Owner | Reached by |
| --- | --- | --- |
| `SEG::Square` (seg.h) | `libs.kimath.include.geometry.application.ui.p003` | `ApproxCollinear`, `ApproxParallel`, `ApproxPerpendicular`: 858 public cases |
| `SEG::SquaredLength` (seg.h) | `libs.kimath.include.geometry.application.ui.p003` | `mutualDistanceSquared`. Unreachable in Rust until `Square` is ported |
| `kimathLogOverflow` (util.cpp) | `libs.kimath.src.math.util.application.ui.p001` | nonquiet `KiROUND` overflow in `SquaredDistance( VECTOR2I )` (`g == 2^63`). Not in the corpus |

Co-port (minimum type): the `SEG` members `A`, `B`, `m_index`, its four constructors (index
-1 or given), `operator==` (points only) and `Index` from `seg.h` are reproduced as
`kicad_math::seg::Seg`. The owner keeps every other body of `seg.h`.

## Caller wiring

`kicad_math::poly_ystripes` (`POLY_YSTRIPES_INDEX::Contains`, unit
`libs.kimath.include.geometry.application.ui.p001`) now calls `Seg::squared_distance_point`
instead of its `UNPORTED(...seg...)` stub. The accuracy branch computes `SEG::Square(
aAccuracy )` first, so those geometry cases still defer at `SEG::Square`. The geometry rerun
keeps every accepted comparison and its 2,436 deferrals unchanged. No other port module called
into seg.cpp.

## Validation (local, non-reference)

- `cargo fmt`, `cargo clippy -D warnings`, `cargo test` for `kicad-math`, `kicad-seg-shim` and
  `kicad-geometry-shim`, and `cargo check -p kicad-math --target wasm32-unknown-unknown`: pass.
- Own oracle build (`port/oracle/build.sh`, `ORACLE_JOBS=8`, build lock): 11/11 oracle ctest
  checks pass.
- `kicad-difftest compare --partial`: 9,503 cases, 8,645 compared exactly, 858 deferred (all
  `SEG::Square`), 0 failures, 0 unprocessed.
- `compare_controls.py`: 1,690 controls refused identically by both programs. The C++ guards
  and the Rust port agree with the Python mirror on every one.
- Caller reruns on the same oracle. Each line gives cases / compared / deferred / failures:
  - geometry: 21,538 / 19,096 / 2,436 / 6. The six failures are the documented libstdc++
    LWG 4084 `INF`/`inf` stream cases of this GCC 15 host
    (`port/findings/kimath-matrix-stream-libstdcxx-lwg4084.md`), not SEG. Hosted GCC 13.3
    compares them.
  - util/box: 15,931 / 14,324 / 1,607 / 0.
  - vector: 10,286 / 9,639 / 647 / 0.
  - arc chord: 1,611 / 1,611 / 0 / 0.
  - eda angle source: 4,704 / 4,704 / 0 / 0.
