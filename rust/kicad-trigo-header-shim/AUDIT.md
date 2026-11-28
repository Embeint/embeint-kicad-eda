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
| `NormalizeAnglePos<T>` | `normalize_angle_pos` | both loops in closed form (Euclidean remainder), `int` and `long long` | `normalize_angle_pos-int-*`, `-ll-*`: every boundary around 0 and 3600, `INT_MIN`/`INT_MAX`, `long long` to the loop bound | `long long` beyond `3600 * 2^24` is a harness refusal (below) |
| `NORMALIZE_ANGLE_POS<T>` | `normalize_angle_pos_in_place` | `Angle = NormalizeAnglePos( Angle )` | `normalize_angle_pos_ref-*` | as above |
| `NormalizeAngle180<T>` | `normalize_angle_180` | both loops in closed form, `( a + 1799 ) mod 3600 - 1799` | `normalize_angle_180-*`: the -1800/1800 boundaries, the limits | as above |
| `InterceptsPositiveX`, `InterceptsNegativeX` | `intercepts_positive_x`, `intercepts_negative_x` | `end += 360` when start > end, both comparisons | `intercepts_*-qa-*` (the pinned `test_kimath.cpp` pairs), a 22 x 22 grid of boundary, non-finite and NaN angles, random pairs | none |

`IsPointOnSegment`, `SegmentIntersectsSegment`, the out-of-line `RotatePoint` overloads,
`CalcArcCenter`, `IsStraightArc`, `CalcArcMid` and `TestSegmentHit` are only declared in trigo.h
and belong to the trigo.cpp unit.

## Template instantiations

The pinned tree instantiates the angle templates for `int` (`NORMALIZE_ANGLE_POS` in
`sch_io_kicad_legacy_lib_cache.cpp`) and `long long` (`NormalizeAngle180( UNIT_BINDER::GetValue() )`
in `panel_zone_properties.cpp`); no floating-point `T` exists. The loops cannot overflow (an
addition runs only below -1800 or 0, a subtraction only above 1800 or 3599) and always terminate,
so the closed forms are exact; crate tests check them against the literal loops. GCC 13.3 inlines
the loops into the adapter as loops, one pass per 3600, so the adapter evaluates every `int` but
refuses a `long long` whose magnitude exceeds `3600 * 2^24` ("outside the verified domain", at
most 2^24 passes); the Rust shim gives the same refusal. This is a harness bound, not an undefined
domain: the Rust functions accept every `long long`.

## Domain and controls

The wrappers are defined wherever the forwarded trigo.cpp overload is: an int rotation is
undefined at the negation of `INT_MIN` in the 90, 180 and 270 degree shortcuts and at an int
overflow in the differences or sums about a centre. The adapter walks that integer evaluation and
refuses those requests; Rust refuses them as `TrigoError::Undefined`. The double bodies, the
conversions and the comparisons are defined for every value. Controls (274): 242 undefined, 15
`long long` angles beyond the loop bound, 12 malformed inputs, all refused identically; 5 are
undefined only after a logging `KiROUND` (a diagonal rotation saturates, then the sum with the
centre overflows): the oracle refuses them and Rust defers to `kimathLogOverflow` first.

The generator replays only exact arithmetic: Python integers, IEEE products and sums for the
diagonal `M_SQRT1_2` shortcuts (no contraction), and fractions for `foldToTwoPeriods`' `std::fmod`.
It evaluates no libm function and no float power, so its output is host independent (checked on
Windows and Linux). Rotations about a centre that need sin/cos are admitted only where every value
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

- `cargo test -p kicad-math` (closed forms against the loops, conversions, wrappers, callers) and
  `-p kicad-trigo-header-shim` (deferral table, controls, `GetRotated` against `RotatePoint` on
  every public request, malformed requests); fmt, clippy `-D warnings`, `cargo check -p kicad-math
  --target wasm32-unknown-unknown`.
- Local diagnostic difftest with the extracted Ubuntu GCC 13.3 toolchain in WSL (glibc 2.43 host,
  hosted `GLIBC_TUNABLES`, `fma` bound to the locked glibc 2.39 libm by the trigo unit's diagnostic
  preload): 4215 cases, 4106 compared, 109 deferred (`kimathLogOverflow`), 0 failures; 274 controls
  as recorded on both programs; every case and control replayed identically in import-free wasm.
- The hosted `rust-trigo-header-live` run (GCC 13.3, glibc 2.39) is the reference.
- Callers rerun with the same oracle: see `port/findings/trigo-header-port.md`.
