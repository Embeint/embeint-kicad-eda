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

`std::clamp<double>` also requires `operator<` to induce a strict weak ordering on its three
values ([alg.clamp]/2; [alg.sorting.general]), as the accepted
`port/upstream-issues/arc-chord-params-nan-clamp.md` applies. A NaN is equivalent to both bounds,
so with distinct bounds (every padding below `INT_MAX`) equivalence is not transitive and the
call is undefined, whatever libstdc++ happens to return; both programs refuse it (the Rust
`geometry_utils::clamp_ordered` returns `GeometryUtilsError::Undefined`). With padding `INT_MAX`
the bounds `-0.0` and `0.0` are equivalent and a NaN is defined: those 24 cases are compared.

`DIRECTION_45( const SHAPE_ARC& )` needs a constructed arc. `SHAPE_ARC()` gives `UNDEFINED`; the
three-point constructor is shape_arc.cpp's (`libs.kimath.src.geometry.shape_arc.application.ui.p001`,
todo), so its 366 cases defer in Rust, for every arc, without a harness bound. The adapter
evaluates the genuine constructor on every arc except those whose construction is undefined,
which `guardArcConstructor` decides by replaying `update_values` on the genuine `CalcArcCenter`,
`EDA_ANGLE` and `rescale`: the `int` differences and `int64_t` products and sums of
`IsEffectiveLine` (through `SEG::mutualDistanceSquared`, including `-1 * INT64_MIN` and
`std::abs( INT64_MIN )`), the `int64_t` cross product of `IsCCW`, the `int` sums of the quadrant
points (the centre is clamped to `INT_MIN + 100 .. INT_MAX - 100`, so a tiny arc at the limits
overflows) and the `int` size `vmax - vmin` of `BOX2I::Compute`. Collinear, closed and
repeated-point arcs, any width and coordinates up to the limits are defined and compared. 198
controls are undefined in the constructor (165 distinct arcs) and 14 after it (`Angle` against
`INT_MIN`, `Opposite` and `Mask` of `UNDEFINED`); the oracle refuses them while Rust defers first.

## Floating point

No observable `double` result depends on GCC's choice between two NaN operands: the snapping
templates compile to bit operations (`andpd`, `andnpd`, `orpd`) for `fabs` and `copysign`, which
keep NaN payloads and signalling NaNs as Rust's `abs` and `copysign` do; `std::clamp` compiles to
`maxsd`/`minsd` and comparisons, equivalent to libstdc++'s `min( max( v, lo ), hi )` for signed
zeros and for a NaN between the equivalent bounds of padding `INT_MAX` (the GCC 13.3 header definition, read from the extracted toolchain); `construct_`
evaluates `360.0 - 180.0 / M_PI * atan2( y, x ) + 90.0` in source order on finite values only,
with `atan2` from the glibc 2.39 facade. The cases include quiet and signalling NaN payloads in
every floating input.

## Review of 7c911843a6

The independent review requested two corrections, both made:

1. The three-point arc guard refused arcs whose C++ construction is defined (a harness bound
   copied from the shape header adapter: coordinates and width within 2^20, distinct points, a
   mid-point sine of at least 1/8), and the Rust shim copied it before its deferral. No deferred
   call needs a verified-domain limit: the guard now refuses exactly the undefined operations
   above, and Rust defers at the constructor for every arc. Of the 79 former "outside the verified
   domain" controls, 71 are public cases and 8 are controls undefined after the constructor; the
   review's `control-direction45-state-3144` (collinear), `-3168` (closed) and `-3216` (start
   beyond 2^20) are the public cases `direction45-state-3144`, `-3168` and `-3216`.
2. The floating clamps compared 195 NaN cases with distinct bounds, outside std::clamp's ordering
   precondition. Both programs now refuse exactly those (Rust: typed `Undefined`) and they are
   controls; the 24 NaN cases with padding `INT_MAX` stay compared.

The review's documentation note is corrected too: `FRACTURE_INDEX` has eight functions, not nine.

## Local evidence

Oracle built from the committed blobs with the extracted Ubuntu GCC 13.3 toolchain in WSL
(`port/oracle/build.sh`, ctest passed), run on the glibc 2.43 host with the hosted
`GLIBC_TUNABLES` and `fma` bound to the locked glibc 2.39 libm. `kicad-difftest compare --partial`:
7635 cases, 7269 compared, 0 failures, 366 deferred; `compare_controls.py`: 1127 controls as
recorded on both programs (212 after the Rust deferral); wasm replay identical. Callers rerun with
0 failures and unchanged counts: shape headers 4255 cases, 4068 compared, 187 deferred (484
controls), arc chord params 1611/1611 (79 controls), arc construction 2912/2912 (20 controls; its
vector and util-box callers 12208/11737/471 and 16066/14605/1461), circle 4978 cases, 4877
compared, 101 deferred (84 controls); their wasm replays are identical.

Diagnostic UBSan probe (not committed): the genuine shape_arc.cpp, seg.cpp, trigo.cpp and util.cpp
recompiled with `-fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all` constructed
18189 arcs (every corpus and control arc and 20000 fixed-seed random arcs at scales from 4 to
2^31, near and at the limits, closed). The adapter's guard refused exactly the 2598 arcs UBSan
diagnosed (signed integer overflow) and none of the 15591 others; the generator's classification
(15254 defined, 2491 undefined, 444 undecided and dropped) never contradicted UBSan.

`generate_cases.py --check` is byte-identical on Windows and Linux. The hosted
`rust-geometry-pure-live` workflow (GCC 13.3, glibc 2.39) is the reference.
