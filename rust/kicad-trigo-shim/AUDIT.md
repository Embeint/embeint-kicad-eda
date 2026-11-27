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
| `RotatePoint( double*, double*, double, double, const EDA_ANGLE& )` | `rotate_point_d_about` | differences, rotation, sums in the compiled order (`cx + ox`, `cy + oy`: the centre's NaN survives) | `rot-da-*`, `rot-da-nan-*` (NaN payloads in point and centre on both axes, every shortcut and the sin/cos path), `rot-da-review-payload` | none |
| `CalcArcCenter( VECTOR2D, VECTOR2D, EDA_ANGLE )` | `calc_arc_center_angle` | both swaps, chord, `sinHalfAngle == 0` midpoint return, `d` with the non-finite reset, both `Resize` calls, rotation, final sum | zero-angle cases complete (`center-angle-*`); others defer | `RotatePoint( VECTOR2D&, const EDA_ANGLE& )` (trigo.h) |
| `degenerateArcCenter` | `degenerate_arc_center` | libstdc++ `std::minmax` order, cluster mean, both coincidence tests (compiled `mid + start` and `start + end` sums: at most one NaN per axis reaches them, so the order is not observable) | cluster and coincident-pair cases, `arc_center_d-nan-1` (y `{ 0, NaN, 0 }`: cluster mean `( 1, NaN )`) | two different NaNs in one coincidence difference defer to the vector owner's `SquaredEuclideanNorm` (34 cases) |
| `collinearArcCenter` | `collinear_arc_center` | midpoint plus `( y, -x ).Resize( 1e17 )` | collinear cases (`arc_center_*`, `is_straight_arc`) | none |
| `det2` | `det2` | Kahan's product, two `std::fma` calls and sum, with each inlined site's compiled operand order (`Det2Order`: denominator `fma( cx, -by, w )`, `f + e`; ux `by * c2`, `fma( by, -c2, w )`, `fma( cy, b2, -w )`, `e + f`; uy `fma( cx, -b2, w )`, `fma( bx, c2, -w )`, `e + f`); `std::fma` is glibc 2.39 `__fma_sse2` through `reference_libm::fma` | every general three-point case; `arc_center_d-nan-r*`, `-sv-r*`, `-big-r*`, `-review-*` reach NaN results chosen by these orders | none |
| `snapArcCenter` | `snap_arc_center` | exact-integer bound, 100 then 10 grid, radii agreement | lattice circles about 1/10/100 grids | none |
| `unclampedArcCenter` | `unclamped_arc_center` | degenerate return, int64 deltas, `KI_INT128` numerators in source order, collinear return, two divisions, snap | `arc_center_i-*`, `is_straight_arc-*` | none |
| `CalcArcCenter( VECTOR2D, VECTOR2D, VECTOR2D )` | `calc_arc_center_d` | degenerate return, relative deltas, two `std::fma` norms, `det2`, collinear return, centre sums in the compiled order (`ux + start.x`), snap | `arc_center_d-*`: finite, signed zeros, infinities, huge, subnormal, NaNs that keep `std::minmax` defined | a coordinate triple with a NaN and two non-equivalent values is refused (`std::minmax` precondition) |
| `CalcArcCenter( VECTOR2I, VECTOR2I, VECTOR2I )` | `calc_arc_center_i` | `std::clamp` order, `KiROUND` (never logs after the clamp) | as above | none |
| `IsStraightArc` | `is_straight_arc` | `aStart == aEnd`, exact `CrossWide`, clamp-range test of the unclamped centre | collinear, coincident, near-collinear | none |

## Domain

The adapter refuses, and the generator classifies as controls, exactly the requests whose pinned
evaluation is undefined: int and int64 signed overflow in C++ evaluation order, the negation of
`INT_MIN` in the rotation shortcuts and of `INT64_MIN` in `SegmentIntersectsSegment`, `std::abs`
of `INT_MIN` in `TestSegmentHit`, and a coordinate triple of the three-point `VECTOR2D`
`CalcArcCenter` that breaks the strict weak ordering `std::minmax` requires on its values: a NaN
together with two non-equivalent values (`trigo::minmax_precondition_holds`, mirrored by the
adapter's `breaksStrictWeakOrder`). A NaN with equivalent values (`{ 0, NaN, -0 }`), or only NaNs,
keeps the ordering and is compared; snapArcCenter's radii `std::minmax` is reached only for a finite
centre and never sees a NaN. `KI_INT128` arithmetic in
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
  toolchain (WSL, the hosted `GLIBC_TUNABLES`): 5,987 cases, 5,372 compared, 615 deferred (311
  and 214 at the trigo.h wrappers, 56 `kimathLogOverflow`, 34 at the vector owner's distinct-NaN
  `SquaredEuclideanNorm`), 0 failures; all 260 controls as recorded on both programs (245
  refused identically, 15 refused by the oracle after a Rust deferral); every case and control
  replayed identically in import-free wasm (Node 22). The WSL host's libm is glibc 2.43, whose
  `fma` returns other NaNs than the reference's 2.39 body: the run binds `fma` to the
  hash-checked locked 2.39 libm with a diagnostic preload (`__fma_sse2`, libm+0x32810). Without
  it 150 NaN-result cases differ on the 2.43 host only.
- `std::fma` backend: 1,145,213 fresh locked-glibc 2.39 records (`port/glibc-libm`,
  `fma_domain.py`), 0 mismatches native debug/release and import-free wasm debug/release; the
  hosted `rust-glibc-libm` job adds WASI and repeats all six.
- `std::minmax( { a, b, c } )` checked against GCC 13.3's `bits/stl_algo.h` `__minmax_element`.
- Compiled operand orders read from the GCC 13.3 `-O3 -ffp-contract=off` trigo.cpp.o of the
  oracle build (offsets cited in `trigo.rs`); `RotatePoint( double*, double*, EDA_ANGLE )`,
  `snapArcCenter`, `collinearArcCenter` and the chord/angle zero-angle midpoint already match
  the source order or cannot receive two NaNs.
- The hosted `rust-trigo-live` run (GCC 13.3, glibc 2.39) is the reference.

## Callers wired

- `BOX2::Intersects( p, q )` (`libs.kimath.include.math.application.ui.p001`): 63 deferrals to 0;
  135 continuation cases.
- `CIRCLE::ConstructFromTanTanPt` (`libs.kimath.src.geometry.circle.application.ui.p001`): the 70
  cases and 2 controls deferred at `CalcArcMid` now defer at its trigo.h wrapper.
