<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# trigo.h inline body port audit

Unit `libs.kimath.include.application.ui.p001` (`libs/kimath/include/trigo.h`, pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`), ported as `kicad_math::trigo_header`. The out-of-line
declarations of the header are trigo.cpp's (`libs.kimath.src.trigo.application.ui.p001`,
`kicad_math::trigo`). The oracle adapter is `port/oracle/shims/trigo_header_p001.cpp`, which
compiles the pinned header unchanged; this crate mirrors it. Status `partial`.

## Per-symbol audit

| Upstream symbol | Rust | Body and branches implemented | Successful operation checked | Remaining gap |
| --- | --- | --- | --- | --- |
| `RotatePoint( VECTOR2I&, const EDA_ANGLE& )` | `rotate_point` | forwards `&point.x, &point.y` to `RotatePoint( int*, int*, const EDA_ANGLE& )`; the point is written only on success | `rotate_i-*`: every shortcut, diagonals, general angles, NaN/infinite/huge angles (folded to 0), int limits | a saturating `KiROUND` defers at `kimathLogOverflow` (math util) |
| `GetRotated( const VECTOR2I&, const EDA_ANGLE& )` | `get_rotated` | copy, forward, return | `get_rotated_i-*`; crate test checks every public request of either form gives the same reply | as above |
| `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` | `rotate_point_about` | forwards to `RotatePoint( int*, int*, int, int, const EDA_ANGLE& )` | `rotate_i_about-*`: centres at the limits, cardinal and diagonal rotations whose sums reach `INT_MAX`, general angles where every sum is in range | as above |
| `GetRotated( const VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` | `get_rotated_about` | copy, forward, return | `get_rotated_i_about-*` | as above |
| `RotatePoint( VECTOR2D&, const EDA_ANGLE& )` | `rotate_point_d` | forwards to `RotatePoint( double*, double*, const EDA_ANGLE& )` | `rotate_d-*`: signed zeros, NaN payloads (quiet and signalling), infinities, subnormals, random bit patterns | none |
| `RotatePoint( VECTOR2D&, const VECTOR2D&, const EDA_ANGLE& )` | `rotate_point_d_about` | forwards to the double overload about a centre (its compiled `cx + ox` order) | `rotate_d_about-*`: NaN payloads in point and centre, infinities, overflow | none |
| `GetRotated( const VECTOR2D&, const EDA_ANGLE& )` | `get_rotated_d` | copy, forward, return | `get_rotated_d-*` | none |
| `DEG2RAD`, `RAD2DEG`, `RAD2DECIDEG` | `deg2rad`, `rad2deg`, `rad2decideg` | `x * c1 / c2` left to right with the reference NaN rules (`fp::mul`, `eda_angle::div`) | `deg2rad-*`, `rad2deg-*`, `rad2decideg-*`: NaN payloads, overflow to infinity, subnormals, exact multiples of pi | none |
| `NormalizeAnglePos<T>` | `normalize_angle_pos` (`int`, `long long`), `normalize_angle_pos_d` (`double`) | integers: both loops in closed form (Euclidean remainder); `double`: every rounded pass, skipped exactly and the sign-changing pass run literally (below) | `normalize_angle_pos-int-*`, `-ll-*`: every boundary around 0 and 3600, `INT_MIN`/`INT_MAX`, `long long` to the pass budget; `-double-*`: signed zeros, NaN payloads, subnormals, the last pass rounding to 3600, the boundaries, the budget | an angle needing more than 2^24 passes is a harness refusal (below); a `double` loop that never terminates is undefined |
| `NORMALIZE_ANGLE_POS<T>` | `normalize_angle_pos_in_place`, `normalize_angle_pos_in_place_d` | `Angle = NormalizeAnglePos( Angle )` | `normalize_angle_pos_ref-*` | as above |
| `NormalizeAngle180<T>` | `normalize_angle_180`, `normalize_angle_180_d` | integers: both loops in closed form, `( a + 1799 ) mod 3600 - 1799`; `double`: as above | `normalize_angle_180-*`: the -1800/1800 boundaries, the limits, the budget | as above |
| `InterceptsPositiveX`, `InterceptsNegativeX` | `intercepts_positive_x`, `intercepts_negative_x` | `end += 360` when start > end, both comparisons | `intercepts_*-qa-*` (the pinned `test_kimath.cpp` pairs), a 22 x 22 grid of boundary, non-finite and NaN angles, random pairs | none |

`IsPointOnSegment`, `SegmentIntersectsSegment`, the out-of-line `RotatePoint` overloads,
`CalcArcCenter`, `IsStraightArc`, `CalcArcMid` and `TestSegmentHit` are only declared in trigo.h
and belong to the trigo.cpp unit.

## Template instantiations

The pinned tree instantiates the angle templates for `int` (`NORMALIZE_ANGLE_POS` in
`sch_io_kicad_legacy_lib_cache.cpp`) and `long long` (`NormalizeAngle180( UNIT_BINDER::GetValue() )`
in `panel_zone_properties.cpp`). No pinned caller uses a floating-point `T`, so there is no caller
to wire, but the templates are unconstrained and their `double` instantiation is ported too.

Integers: the loops cannot overflow (an addition runs only below -1800 or 0, a subtraction only
above 1800 or 3599) and always terminate, so the closed forms are exact; crate tests check them
against the literal loops.

`double`: GCC 13.3 (`-O3 -ffp-contract=off`) compiles each loop literally, a `comisd` and one
`addsd`/`subsd` of 3600.0 per pass (disassembled from a probe compiled with the adapter's flags), so
each pass rounds to nearest, ties to even. A NaN fails every comparison and is returned unchanged
(no arithmetic, so a signalling NaN keeps its bits), as is `-0.0`. Rounding is symmetric, so both
directions are the magnitude step `m -> fl( m - 3600 )`, which `normalize_angle_pos_d` and
`normalize_angle_180_d` evaluate exactly for every terminating input:

- Up to `2^57` every pass is exact except the one that changes the angle's sign; the port skips
  to the first value below 7200 in one exact step (integer part and exact fraction) and runs the
  remaining passes literally. That reproduces, for example, `-1e-300 -> 3600 -> 0`: the first
  loop's last pass rounds to 3600, so the second loop runs once.
- Above `2^57` the step is constant within a binade (3584 for ulp 64 to 512, 4096 for 1024 to
  4096; at ulp 32 it is a tie, 3616 once from an odd multiple and 3584 after), so whole binades
  are skipped with integer arithmetic. Crate tests compare the skipping with the literal steps
  across every binade boundary from `2^56` to `2^65`.
- For every magnitude above `2^65`, and for an infinite angle, `fl( m - 3600 ) == m`: the loop
  never terminates, which is undefined (a side-effect-free loop must terminate,
  [intro.progress]). The first pass decides it, because below `2^65` every pass progresses by at
  least 3584. Both programs refuse those requests as undefined; the adapter tests the first pass
  before calling the template, Rust's functions return `TrigoError::Undefined`.

Harness budget: because the compiled loops run one pass per 3600, the adapter refuses an angle
whose loops need more than 2^24 passes ("outside the verified domain"); the Rust shim gives the
same refusal after evaluating the request. The passes are counted exactly as quotients
(`template_passes` in both adapters, computed independently with fractions by the generator):
`NormalizeAnglePos` `ceil( -a / 3600 )` below 0 and `floor( a / 3600 )` from 3600,
`NormalizeAngle180` `floor( ( -1800 - a ) / 3600 ) + 1` up to -1800 and `ceil( ( a - 1800 ) /
3600 )` above 1800. The only pass this does not count is the extra second-loop pass after a
`double` first loop rounds up to 3600, which needs an angle above `-2^11` (a single pass) and so
never decides admission. Admitted are exactly `[-3600 * 2^24, 3600 * ( 2^24 + 1 ) - 1]` for
`NormalizeAnglePos` and `[-3600 * 2^24 - 1799, 3600 * 2^24 + 1800]` for `NormalizeAngle180`
(integers; for a `double` the real intervals `[-3600 * 2^24, 3600 * ( 2^24 + 1 ))` and
`(-3600 * 2^24 - 1800, 3600 * 2^24 + 1800]`). This is a harness bound, not an undefined
domain: the Rust functions evaluate every `int` and `long long` and every terminating `double`.
The earlier magnitude envelope `3600 * 2^24` refused four requests needing exactly 2^24 passes;
they are now compared (`*-ll-within-*`), and the budget boundaries are cases and controls
(`*-ll-budget-*`, `*-double-*`).

## Domain and controls

The wrappers are defined wherever the forwarded trigo.cpp overload is: an int rotation is
undefined at the negation of `INT_MIN` in the 90, 180 and 270 degree shortcuts and at an int
overflow in the differences or sums about a centre. The adapter walks that integer evaluation and
refuses those requests; Rust refuses them as `TrigoError::Undefined`. The double wrappers, the
conversions and the comparisons are defined for every value; the `double` angle templates for every
value except where a loop never terminates (above). Controls (397): 302 undefined (242 integer
rotations, 60 `double` template loops that never terminate: infinite angles and magnitudes above
`2^65`), 77 angles beyond the pass budget (17 `long long`, 60 `double`, including terminating
magnitudes from `2^40` to `2^65`), 13 malformed inputs, all refused identically; 5 are undefined
only after a logging `KiROUND` (a diagonal rotation saturates, then the sum with the centre
overflows): the oracle refuses them and Rust defers to `kimathLogOverflow` first.

The generator replays only exact arithmetic: Python integers, IEEE products and sums for the
diagonal `M_SQRT1_2` shortcuts (no contraction), and fractions for `foldToTwoPeriods`' `std::fmod`.
The template passes are counted with fractions, and the one pass that can round (leaving
`NormalizeAnglePos`' first loop) is replayed with Python's correctly rounded fraction-to-float
conversion; termination is the literal first pass. It evaluates no libm function and no float
power, so its output is host independent (checked on Windows and Linux). Rotations about a centre that need sin/cos are admitted only where every value
within the rotation's magnitude bound keeps both sums in range.

## Compiled operand order

The conversions are compiled into the adapter (GCC 13.3, `-O3 -ffp-contract=off`) as `mulsd
const, %xmm0; divsd const, %xmm0` with the input in `%xmm0`, in source order and without
reassociation; only the input can be a NaN, so its quieted payload is the result on both programs.
The `RotatePoint`/`GetRotated` wrappers compile to calls of the out-of-line trigo.cpp overloads,
whose operand orders the trigo.cpp unit verified. In trigo.cpp the chord/angle `CalcArcCenter`
calls the wrapper's callee out of line (`call RotatePoint(double*, double*, EDA_ANGLE const&)`) and
then sums `start + vc + vec2` with `addpd` in source order (`start` the destination), which the Rust
`Vector2D` addition matches.

## Evidence

- `cargo test -p kicad-math` (closed forms against the loops; the `double` templates against the
  literal loops, against GCC 13.3's observed bits, the skipped passes against literal steps across
  every binade from `2^56` to `2^65`, and the non-terminating magnitudes at every exponent;
  conversions, wrappers, callers) and `-p kicad-trigo-header-shim` (deferral table, controls,
  `GetRotated` against `RotatePoint` on every public request, malformed requests, the pass counts
  against the literal loops and at every budget boundary); fmt, clippy `-D warnings`, `cargo check
  -p kicad-math --target wasm32-unknown-unknown`.
- Local diagnostic difftest with the extracted Ubuntu GCC 13.3 toolchain in WSL (glibc 2.43 host,
  hosted `GLIBC_TUNABLES`, `fma` bound to the locked glibc 2.39 libm by the trigo unit's diagnostic
  preload): 4693 cases, 4584 compared, 109 deferred (`kimathLogOverflow`), 0 failures; 397 controls
  as recorded on both programs; every case and control replayed identically in import-free wasm.
  The first 4215 cases are unchanged; the 478 new ones are 468 `double` template requests (156 per
  operation), the four former budget refusals and six budget boundaries.
- The hosted `rust-trigo-header-live` run (GCC 13.3, glibc 2.39) is the reference.
- Callers rerun with the same oracle: see `port/findings/trigo-header-port.md`.
