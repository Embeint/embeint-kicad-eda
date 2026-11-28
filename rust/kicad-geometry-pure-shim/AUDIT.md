<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath geometry pure p001 header port audit

Unit `libs.kimath.include.geometry.application.pure.p001` (17 headers in
`libs/kimath/include/geometry/`, pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`), ported into
`kicad_math` one module per header. The oracle adapter is
`port/oracle/shims/kimath_include_geometry_p001.cpp`, which compiles the pinned headers unchanged;
this crate mirrors it. Status `partial`.

## Per-symbol audit

| Upstream symbol | Rust | Body and branches implemented | Successful operation checked | Remaining gap |
| --- | --- | --- | --- | --- |
| `DIRECTION_45( Directions = UNDEFINED )` | `Direction45::new`, `Default` | `m_dir( aDir )`, `m_90deg( false )`; any `int` (fixed underlying type) | `direction45-*` with every member on 33 `int` values from `INT_MIN` to `INT_MAX` | none |
| `DIRECTION_45( const VECTOR2I&, bool a90 = false )` | `from_vector` | `vec.y = -vec.y` (undefined for `INT_MIN`), `construct_` | compass vectors, 22.5 degree boundaries at 1e3 and 1e9, `INT_MIN`/`INT_MAX` components, `a90` omitted/false/true | none |
| `DIRECTION_45( const SEG&, bool a90 = false )` | `from_seg` | `aSeg.B - aSeg.A` (int overflow undefined), then as above | overflowing and in-range differences, a zero segment | none |
| `DIRECTION_45( const SHAPE_ARC&, bool a90 = false )` | `from_arc` | `GetP1() - GetP0()`, then as above | `SHAPE_ARC()` (UNDEFINED); three-point arcs compared in the oracle | the three-point `SHAPE_ARC` constructor is shape_arc.cpp's: 81 cases defer to `libs.kimath.src.geometry.shape_arc.application.ui.p001` |
| `construct_` | `Direction45::construct` | zero vector UNDEFINED; `360 - 180/pi * atan2 + 90`, the two folds, `( mag + 22.5 ) / 45` truncated, the two wraps; `atan2` through `reference_libm` (glibc 2.39) | every constructor case | none |
| `Format` | `format` | every `case` and `default` (`"<Error>"`) | all directions, out-of-range values | none |
| `Opposite` | `opposite` | `OppositeMap[m_dir]`; index outside 0..8 undefined | 0..8 compared, the rest controls | none |
| `Angle`, `IsObtuse` | `angle`, `is_obtuse` | UNDEFINED shortcut; `std::abs( m_dir - aOther.m_dir )` (overflow and `abs( INT_MIN )` undefined); every `d` branch | pairs over the 33 values, compass pairs | none |
| `IsDiagonal`, `IsDefined`, `operator==`, `operator!=` | same names, `PartialEq` | truncating `%`; `m_dir` comparison only (not `m_90deg`) | every value, negative values | none |
| `Right`, `Left` | `right`, `left` | default-constructed result (flag false), steps 1/2 with `m_90deg`, `( m_dir + LAST - k ) % LAST` left to right; overflow undefined | every value, both flags, `INT_MAX - 1`, `INT_MAX - 7` | none |
| `ToVector`, `Mask`, `AllDirectionsMask` | same | the switch with `default`; `1 << m_dir` (count outside 0..31 undefined, `1 << 31` is `INT_MIN` in C++20) | 0..31 compared | none |
| `KIGEOM::OrientationSign` | `orientation_sign` | `int64_t` differences, `CrossWide` (accepted `wide_int`), sign | qa `test_wide_int.cpp` and `test_poly_triangulation.cpp` triangles, full-range points, collinear | none |
| `KIGEOM::detail::inCircleLegalDouble` | `in_circle_legal_double` | source-order `double` determinant and tie margin | called directly on the same quads as the exact test, beyond-bound quads | none |
| `KIGEOM::InCircleDelaunayLegal` | `in_circle_delaunay_legal` | the `kMaxSafeDiff` filter (at exactly 1.5e9 and one past), exact `KI_INT128` determinant | qa cocircular/inside/outside quads, near-cocircular large circles | none |
| `KIGEOM::IsSliverTriangle` | `is_sliver_triangle` | squared edges, `std::max`/`std::min` of three, zero shortest | qa needle, degenerate triangles | none |
| `KIGEOM::FRACTURE_INDEX::*` (9 functions, 5 constants) | `fracture_edge_index_utils` | `size_t` as `u64` on every target; the wraps of `aStripeCount + 1`, `( MAX_BUCKET_SPAN + 2 ) * aHoleCount`, `aStripeCount - 1`; `std::sqrt` and `std::clamp`; short-circuit order; `aBytes` null/non-null | boundaries of every helper (`u64` extremes, perfect squares, `2^53 + 1`, inverted extents, zero stripes) | none |
| `GetVectorSnapped45<T>` | `get_vector_snapped45_i`, `_d` | the four branches, `ext_type` (`int64_t` / `double`), `std::copysign`; `std::abs( INT_MIN )` undefined | `int` grid around the 2:1 and 1:1 boundaries, `only45` omitted/false/true; `double` NaN payloads (signalling too), signed zeros, overflow of `* 2` | none |
| `GetVectorSnapped90<T>` | `get_vector_snapped90_i`, `_d` | `>=` tie to `x` | as above | none |
| `GetClampedCoords<in, ret, unsigned>` | `get_clamped_coords_i`, `_d`, `_d_to_i` | `INT_MAX - aPadding` in `int64_t` / `double`, `std::clamp` as `min( max( v, lo ), hi )` (GCC 13.3 libstdc++), quiet `KiROUND<double, int>`; padding above `INT_MAX` undefined | the three pinned instantiations, default padding, padding 0 to `UINT32_MAX`, every bound and its neighbours, NaN and infinities | none |
| `IsVec2SafeXY<T>` | `is_vec2_safe_xy_d`, `_i` | strict comparisons with `INT_MIN`/`INT_MAX` as `T` | `+-2^31`, one step inside and outside, NaN | none |
| `HALF_LINE` (2 constructors, `GetStart`, `GetContainedPoint`, `operator==`, `GetContainedSeg`) | `half_line::HalfLine` | `m_seg` copy (index kept) or `SEG( a, b )` (index -1); `SEG::operator==` | both constructors, equal and reversed segments, extreme indices | none |
| `LINE` (2 constructors, `operator==`, `GetContainedSeg`) | `line::Line` | as above; `LINE::ecoord` | as above | none |
| `ARC_CHORD_PARAMS` layout and 15 accessors | `arc_chord_params::ArcChordParams` (PR #70 co-port, adopted) | member order and initializers; each accessor reads its field | `arc_chord-*`: default, up to four `Compute` calls (members kept across a refused one), every accessor | none |
| `KIGEOM::ARC_SOLUTION` | `arc_construction::ArcSolution` (PR #85, adopted) | six fields, defaults | `header_types` | none |
| `CIRCLE` members, `operator==` | `circle::Circle` (PR #95, adopted) | `Radius` then `Center`, defaulted `==`, implicit copy assignment | `circle_eq-*`: 49 pairs of the two constructors | none |
| `ERROR_LOC`, `CORNER_STRATEGY`, `LEADER_MODE`, `OVAL_KEY_POINTS`, `OVAL_KEY_POINT_FLAGS`, `DIRECTION_45` enums | `approximation`, `corner_strategy`, `geometry_utils`, `oval`, `direction45` | enumerator values | `header_types` | none |
| `CHAMFER_PARAMS`, `CHAMFER_RESULT`, `DOGBONE_RESULT`, `INTERSECTION_CONTACT` | `corner_operations`, `intersection` | members; `Default` is value initialization / the member initializers | `header_types` | none |
| `INTERSECTABLE_GEOM`, `NEARABLE_GEOM` | `intersection::IntersectableGeom`, `nearest::NearableGeom` | alternatives in variant order | type only | no observable body |

`INTERSECTION_VISITOR` has no inline body: its two constructors (which bind its reference members)
and every `operator()` are intersection.cpp's, so the visitor is left to
`libs.kimath.src.geometry.intersection.application.ui.p001`. The out-of-line declarations of the
headers belong to their source units: `ARC_CHORD_PARAMS::Compute` and the four getters
(arc_chord_params, accepted), the `KIGEOM` arc constructions (arc_construction, accepted), the
CIRCLE bodies (circle, accepted), `BuildConvexHull` (convex_hull), `ComputeChamferPoints` and
`ComputeDogbone` (corner_operations), `DIRECTION_45::BuildInitialTrace` (direction_45),
`GetDeltasForDistributeBy*` (distribute, accepted), `GetArcToSegmentCount`,
`CircleToEndSegmentDeltaRadius`, `DISABLE_ARC_RADIUS_CORRECTION`, `GetCircleToPolyCorrection`,
`ClipLine`, `BoxHitTest`, `ShapeHitTest` (geometry_utils), the `HALF_LINE` and `LINE` members and
`ClipHalfLineToBox` (half_line, line), `GetNearestPoint` (nearest), `ConvertToChain` and
`GetOvalKeyPoints` (oval). None of them is called by an owned inline body.

## Template instantiations

The header templates are ported for the instantiations the pinned tree uses:
`GetVectorSnapped45<int>` (edit_relations.cpp, two_point_geom_manager.h, drawing_tool.cpp),
`GetVectorSnapped45<double>` (polygon_geom_manager.cpp), `GetVectorSnapped90<int>`,
`GetClampedCoords<int, int>` (drawing_tool.cpp, item_modification_routine.cpp),
`<double, double>` (wx_view_controls.cpp, tool_dispatcher.cpp, the router tools),
`<double, int>` (pcb_point_editor.cpp), always with `unsigned int` padding, and
`IsVec2SafeXY<double>` (reference_image.cpp). `GetVectorSnapped90<double>` and
`IsVec2SafeXY<int>` have no pinned caller and are ported with them.

## Domain and controls

The adapter refuses only undefined evaluations, as "undefined geometry header operation": the
negation of `INT_MIN` and the `int` differences of the `DIRECTION_45` constructors, an
`OppositeMap` index outside 0..8, a `Mask` shift count outside 0..31, `int` overflow and
`std::abs( INT_MIN )` in `Angle`, `Right` and `Left`, `std::abs( INT_MIN )` in the `int`
snapping templates and a `GetClampedCoords` padding above `INT_MAX` (the clamp's lower bound
would exceed its upper bound, [alg.clamp]). The Rust functions decide these themselves and return
a typed `Undefined`; the generator classifies them independently with exact integer replay.
Every other body is defined for every input and compared.

The three-point `SHAPE_ARC` constructor is not replayed: as in the shape header adapter it runs
only inside that adapter's verified domain (coordinates and width within 2^20, distinct start and
end, a mid-point angle with a sine of at least 1/8), and both programs refuse anything else as
"outside the verified domain" (harness bound, not an undefined domain).

Controls (805): 690 undefined and refused identically (430 `direction45`, 242 `clamped_coords`
paddings above `INT_MAX`, 13 `snapped45` and 5 `snapped90` with an `INT_MIN` coordinate); 6
undefined only after the three-point arc constructor that Rust defers (`rust_unported`: `Angle`
against `INT_MIN`); 79 three-point arcs outside the verified domain; 30 malformed inputs with
identical messages (every multi-field read is sequenced in the adapter, and the two orderings
that matter, `a90` before the constructor guard and the coordinates before the padding guard,
have their own controls).

Cases (7545) by operation: `direction45` 3122, `clamped_coords` 1150, `snapped45` 998,
`snapped90` 620, `map_y_to_stripe` 528, `is_vec2_safe_xy` 186, `in_circle_delaunay_legal` 99,
`in_circle_legal_double` 99, `stripe_span` 80, `orientation_sign` 79, `is_sliver_triangle` 79,
`actual_capacity_fits` 78, `stripe_count_for` 75, `arc_chord_params` 75, `capacity_fits` 71,
`half_line` 64, `line` 64, `circle_eq` 49, `checked_add` 13, `capacity_budget` 9,
`should_index` 6, `header_types` 1.

## Floating point

No double result of this unit comes from arithmetic that could produce a NaN: GCC 13.3 compiles
`construct_` in source order (`mulsd` by the folded `180 / M_PI`, `subsd` from 360, `addsd` 90,
`comisd`, `subsd`/`addsd` 360, `addsd` 22.5, `divsd` 45) on finite values only; `fabs` and
`copysign` in the snapping templates are `andpd`/`andnpd`/`orpd` bit operations, which keep NaN
payloads (signalling ones too) exactly as Rust's `abs` and `copysign`; `std::clamp` is
`maxsd`/`minsd` plus comparisons, equivalent to `min( max( v, lo ), hi )` for NaN and signed
zeros; the `<double, int>` rounding is the inlined quiet `KiROUND`. The predicates return
booleans of finite products. Disassembled from the adapter object built with the hosted flags.

## Evidence

Local diagnostic (extracted Ubuntu GCC 13.3 oracle on the WSL glibc 2.43 host with the hosted
`GLIBC_TUNABLES` and `fma` bound to the locked glibc 2.39 libm): 7545 cases, 7464 compared,
0 failures, 81 deferred at the three-point `SHAPE_ARC` constructor; all 805 controls refused
identically by both programs (6 after the Rust deferral). The hosted `rust-geometry-pure-live`
run (GCC 13.3, glibc 2.39) is the reference.
