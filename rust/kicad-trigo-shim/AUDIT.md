<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# trigo.cpp source port audit

Unit `libs.kimath.src.trigo.application.ui.p001` (`libs/kimath/src/trigo.cpp`, pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`), ported as `kicad_math::trigo`. The oracle adapter is
`port/oracle/shims/trigo_source_p001.cpp`; this crate mirrors it. Status `partial`.

## Per-symbol audit

| Upstream symbol | Rust | Branches and order implemented | Successful operation checked | Remaining gap |
| --- | --- | --- | --- | --- |
| `CircleCenterFrom3Points` | none | compiled out (`USE_ALTERNATE_CENTER_ALGO` undefined) | not compiled in the reference | none |
| `IsPointOnSegment` | `is_point_on_segment` | int differences, long long cross (always defined), dot and squared norm sums (can reach 2^63) | `pos-*`: on, off, behind, beyond, degenerate, limits | none |
| `SegmentIntersectsSegment` | `segment_intersects_segment` | int64 deltas, `den` and both numerators in order, parallel return before the point, point written before the bounds tests (x then y, `KiROUND` and int sums), negations, four bound tests | `ssi-*`: crossing, parallel, collinear, touching, misses, null and given output points | the y offset uses `num_b` (suspected reference defect, kept) |
| `TestSegmentHit` | `test_segment_hit` | delta first, swaps, short-circuited box tests, both easy cases (`std::abs`), `SEG::SquaredDistance` against `SEG::Square( aDist + 1 )` | `hit-*`: inside, outside, axis-aligned easy cases, SEG path, negative and extreme distances | none |
| `CalcArcMid` | `calc_arc_mid` | differences, `EDA_ANGLE( VECTOR2D )` of each, `Normalize180() / 2`, `+= ANGLE_180` for `!aMinArcAngle`, default argument | Rust defers at the trigo.h wrapper for every `mid-*` case; the continuation is checked in crate tests (oracle value for one general angle) | `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` (trigo.h) |
| `RotatePoint( int*, int*, const EDA_ANGLE& )` | `rotate_point_i` | `Normalize`, 0/90/180/270 shortcuts (negations), general `KiROUND` of sin/cos expressions | `rot-i-*`: every shortcut, diagonals, tiny/huge/NaN/infinite angles, limits | `KiROUND` overflow logs (`kimathLogOverflow` deferral) |
| `RotatePoint( int*, int*, int, int, const EDA_ANGLE& )` | `rotate_point_i_about` | differences, rotation, sums; outputs written only on success | `rot-ia-*` | as above |
| `RotatePoint( double*, double*, const EDA_ANGLE& )` | `rotate_point_d` | shortcuts with sign-bit negation, general sin/cos | `rot-d-*`: signed zeros, NaN, infinities, subnormals | none |
| `RotatePoint( double*, double*, double, double, const EDA_ANGLE& )` | `rotate_point_d_about` | differences, rotation, sums | `rot-da-*` | none |
| `CalcArcCenter( VECTOR2D, VECTOR2D, EDA_ANGLE )` | `calc_arc_center_angle` | both swaps, chord, `sinHalfAngle == 0` midpoint return, `d` with the non-finite reset, both `Resize` calls, rotation, final sum | zero-angle cases complete (`center-angle-*`); others defer | `RotatePoint( VECTOR2D&, const EDA_ANGLE& )` (trigo.h) |
| `degenerateArcCenter` | `degenerate_arc_center` | libstdc++ `std::minmax` order, cluster mean, both coincidence tests | cluster and coincident-pair cases | none |
| `collinearArcCenter` | `collinear_arc_center` | midpoint plus `( y, -x ).Resize( 1e17 )` | collinear cases (`arc_center_*`, `is_straight_arc`) | none |
| `det2` | `det2` | Kahan's two `std::fma` calls and sum | every general three-point case | none |
| `snapArcCenter` | `snap_arc_center` | exact-integer bound, 100 then 10 grid, radii agreement | lattice circles about 1/10/100 grids | none |
| `unclampedArcCenter` | `unclamped_arc_center` | degenerate return, int64 deltas, `KI_INT128` numerators in source order, collinear return, two divisions, snap | `arc_center_i-*`, `is_straight_arc-*` | none |
| `CalcArcCenter( VECTOR2D, VECTOR2D, VECTOR2D )` | `calc_arc_center_d` | degenerate return, relative deltas, two `std::fma` norms, `det2`, collinear return, snap | `arc_center_d-*`: finite, signed zeros, infinities, huge, subnormal | NaN input refused (library precondition) |
| `CalcArcCenter( VECTOR2I, VECTOR2I, VECTOR2I )` | `calc_arc_center_i` | `std::clamp` order, `KiROUND` (never logs after the clamp) | as above | none |
| `IsStraightArc` | `is_straight_arc` | `aStart == aEnd`, exact `CrossWide`, clamp-range test of the unclamped centre | collinear, coincident, near-collinear | none |

## Domain

The adapter refuses, and the generator classifies as controls, exactly the requests whose pinned
evaluation is undefined: int and int64 signed overflow in C++ evaluation order, the negation of
`INT_MIN` in the rotation shortcuts and of `INT64_MIN` in `SegmentIntersectsSegment`, `std::abs`
of `INT_MIN` in `TestSegmentHit`, and a NaN coordinate given to the three-point `VECTOR2D`
`CalcArcCenter`, whose `std::minmax` requires a strict weak ordering. `KI_INT128` arithmetic in
`unclampedArcCenter` stays below 2^100 and the double bodies are defined for every input, so the
integer three-point form, `IsStraightArc` and the double rotations have no refusal. Fifteen
controls are undefined only after a call Rust defers (2 after the CalcArcMid wrapper, 13 after a
logging `KiROUND`): the oracle refuses them and Rust defers to that owner.

Requests whose split would need sin, cos or atan2 (a rotation away from the shortcuts followed by
an int sum near the range) are generated only where every value within the rotation's magnitude
bound keeps the sums in range; the generator drops the rest rather than guess.

## Evidence

- `cargo test -p kicad-math --lib trigo` and `-p kicad-trigo-shim` (deferral table, controls,
  malformed requests); fmt, clippy `-D warnings`, `cargo check -p kicad-math --target
  wasm32-unknown-unknown`.
- Local diagnostic difftest against the pinned oracle built with the extracted Ubuntu GCC 13.3
  toolchain (WSL, glibc 2.43, the hosted `GLIBC_TUNABLES`): 4,106 cases, 3,525 compared, 581
  deferred (311 and 214 at the trigo.h wrappers, 56 `kimathLogOverflow`), 0 failures; all 226
  controls as recorded on both programs; every case and control replayed identically in
  import-free wasm (Node 22).
- `std::minmax( { a, b, c } )` checked against GCC 13.3's `bits/stl_algo.h` `__minmax_element`.
- The hosted `rust-trigo-live` run (GCC 13.3, glibc 2.39) is the reference.

## Callers wired

- `BOX2::Intersects( p, q )` (`libs.kimath.include.math.application.ui.p001`): 63 deferrals to 0;
  135 continuation cases.
- `CIRCLE::ConstructFromTanTanPt` (`libs.kimath.src.geometry.circle.application.ui.p001`): the 70
  cases and 2 controls deferred at `CalcArcMid` now defer at its trigo.h wrapper.
