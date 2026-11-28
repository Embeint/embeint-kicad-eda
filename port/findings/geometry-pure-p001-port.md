<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath geometry pure p001 headers: callers, co-ports and domain

Unit: `libs.kimath.include.geometry.application.pure.p001` (the 17 headers
`libs/kimath/include/geometry/{approximation,arc_chord_params,arc_construction,circle,convex_hull,
corner_operations,corner_strategy,direction45,distribute,fracture_edge_index_utils,
geometry_predicates,geometry_utils,half_line,intersection,line,nearest,oval}.h` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). This file is also the addendum to the records of
the units whose co-ports it adopts (arc chord params, arc construction, circle) and of the shape
header unit, whose `ERROR_LOC` placeholder it replaces.

## Callers and their deferrals

On accepted `rust` (a8c7c72535), `git grep -n "libs.kimath.include.geometry.application.pure.p001"`
finds no `UNPORTED` tag and no `Unported` error naming this unit: only the three co-port records
(`arc_chord_params.rs` `HEADER_UNIT`, `arc_construction.rs`, `circle.rs` `CIRCLE_HEADER_UNIT`)
and their audits. No accepted Rust re-implements an owned inline body (searched for every owned
symbol: `DIRECTION_45`, the snapping and clamping templates, the predicates, `FRACTURE_INDEX`,
`HALF_LINE`, `LINE`). Caller deferrals naming this unit: 0 before, 0 after.

One accepted body used a placeholder for an owned type: `SHAPE_LINE_CHAIN_BASE::TransformToPolygon`
(`libs.kimath.include.geometry.application.ui.p003`, `kicad_math::shape`) took `ERROR_LOC` as a type
parameter because approximation.h was unported. It now takes `approximation::ErrorLoc`; the body
is still empty, as in the header, and only its crate test calls it. The shape header corpus is
unchanged (local: 4255 cases, 4068 compared, 187 deferred, 0 failures, 484 controls as recorded).

## Co-ports audited and adopted

Each was compared with the pinned header field by field and adopted unchanged; the module docs
now say so and this unit's corpus observes them:

| Co-port | Header content | Rust | Audit | Observed by |
| --- | --- | --- | --- | --- |
| PR #70 (arc chord params source unit) | `ARC_CHORD_PARAMS` members in declaration order with their `= 0.0`/`false` initializers; the 15 inline accessors | `arc_chord_params::ArcChordParams` (`Default`), `is_valid` .. `get_mid_y` | each accessor returns its field; `GetChordUnitVec`, `GetNormalUnitVec`, `GetChordMidpoint` build `VECTOR2D( x, y )` in that order | `arc_chord_params` cases: default object and up to four `Compute` calls, every accessor's bits |
| PR #85 (arc construction source unit) | `KIGEOM::ARC_SOLUTION`: `valid = false`, three `VECTOR2I`, `VECTOR2D center`, `radius = 0.0` | `arc_construction::ArcSolution` (`Default`) | field order and defaults | `header_types` |
| PR #95 (circle source unit) | `CIRCLE::Radius`, `CIRCLE::Center` (that order), defaulted `operator==` | `circle::Circle`, `#[derive(PartialEq, Eq)]`, `Copy` | the derived equality compares in declaration order; the implicit copy assignment is `Copy` | `circle_eq` cases: both constructors, 49 pairs, equality and copy assignment |

The constructors and every out-of-line body stay with their source units.

## Domain

`Directions` has the fixed underlying type `int`, so `DIRECTION_45( (Directions) k )` is defined for
every `int` and the members are compared on values from `INT_MIN` to `INT_MAX`. The undefined
evaluations are `OppositeMap[m_dir]` outside 0..8 (`Opposite` of `UNDEFINED` is one), `1 << m_dir`
outside 0..31, `int` overflow in `Right`, `Left` and `Angle`, `std::abs( INT_MIN )` in `Angle`,
the negation of `INT_MIN` and the `int` difference in the three vector constructors. `Right` and
`Left` return a default-constructed direction, so their result's 90 degree flag is always false;
the adapter reads `m_dir` and `m_90deg` through explicit instantiation of member pointers and both
programs report them.

`GetClampedCoords` converts its `unsigned int` padding to `int64_t` or `double` before
subtracting it from `INT_MAX`; a padding above `INT_MAX` makes `-max > max`, which violates
`std::clamp`'s precondition and is refused. A padding of exactly `INT_MAX` clamps to `[-0.0, 0.0]`
for doubles, so a negative value becomes `-0.0` (both programs). Pinned callers pass 1 (the
default), the tools' `COORDS_PADDING` of 20 mm or item_modification_routine.cpp's 200 mm.

`DIRECTION_45( const SHAPE_ARC& )` needs a constructed arc. `SHAPE_ARC()` gives `UNDEFINED`; the
three-point constructor is shape_arc.cpp's (`libs.kimath.src.geometry.shape_arc.application.ui.p001`,
todo), so its 81 cases defer in Rust. The adapter calls it only inside the shape header adapter's
verified domain; 79 arcs outside it are controls refused by both programs, and 6 controls are
undefined only after that constructor (`Angle` against `INT_MIN`), refused by the oracle while
Rust defers first.

## Floating point

No observable `double` result depends on GCC's choice between two NaN operands: the snapping
templates compile to bit operations (`andpd`, `andnpd`, `orpd`) for `fabs` and `copysign`, which
keep NaN payloads and signalling NaNs as Rust's `abs` and `copysign` do; `std::clamp` compiles to
`maxsd`/`minsd` and comparisons, equivalent to libstdc++'s `min( max( v, lo ), hi )` for NaN and
signed zeros (the GCC 13.3 header definition, read from the extracted toolchain); `construct_`
evaluates `360.0 - 180.0 / M_PI * atan2( y, x ) + 90.0` in source order on finite values only,
with `atan2` from the glibc 2.39 facade. The cases include quiet and signalling NaN payloads in
every floating input.

## Local evidence

Oracle built from the committed blobs with the extracted Ubuntu GCC 13.3 toolchain in WSL
(`port/oracle/build.sh`, ctest 17/17), run on the glibc 2.43 host with the hosted
`GLIBC_TUNABLES` and `fma` bound to the locked glibc 2.39 libm. `kicad-difftest compare --partial`:
7545 cases, 7464 compared, 0 failures, 81 deferred; `compare_controls.py`: 805 controls as
recorded on both programs; wasm replay identical. Callers rerun with 0 failures and unchanged
counts: shape headers (above), arc chord params 1611/1611, arc construction 2912/2912, circle
4978 cases, 4877 compared, 101 deferred. `generate_cases.py --check` is byte-identical on Windows
and Linux. The hosted `rust-geometry-pure-live` workflow (GCC 13.3, glibc 2.39) is the reference.
