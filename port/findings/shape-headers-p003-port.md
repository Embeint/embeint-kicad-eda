# seg.h and shape header port (p003): caller rewiring, domains and evidence

Unit: `libs.kimath.include.geometry.application.ui.p003` (seg.h, shape.h, shape_arc.h,
shape_circle.h, shape_compound.h, shape_ellipse.h at KiCad 4cf939778df0).

## Caller deferrals removed

The accepted SEG (`libs.kimath.src.geometry.seg.application.ui.p001`) and CIRCLE
(`libs.kimath.src.geometry.circle.application.ui.p001`) ports called this unit through typed
deferrals. They now call the ported bodies:

| Caller | Deferral removed | Corpus effect (public corpora unchanged) |
| --- | --- | --- |
| seg.cpp `ApproxCollinear`, `ApproxParallel`, `ApproxPerpendicular` | `SEG::Square`, `SEG::SquaredLength` | 858 deferred cases -> 0; all 9503 public cases now compare. 197 candidates the generator had dropped as "undefined only after a deferral" are now controls (1690 -> 1887), refused identically. |
| circle.cpp `ConstructFromTanTanPt` | `SEG::Square` (via `ApproxParallel`), `SEG::Center`, `SEG::IntersectLines` | 362 deferred -> 342: the 107 `SEG::Square` cases now give 20 more compared results and stop at `CalcArcMid` (70, trigo.cpp) or the `wxOnAssert` host effect (17). Two tan-tan controls now defer at `CalcArcMid`; `approx-parallel-length` is refused identically by both programs. |
| poly_ystripes_index.h `POLY_YSTRIPES_INDEX::Contains` (`libs.kimath.include.geometry.application.ui.p001`) | `SEG::Square` in the `aAccuracy > 1` fallback | 553 deferred -> 0 on the unchanged 22171 requests (20474 -> 21027 results); the fallback then runs the accepted `SEG::SquaredDistance` with the hole skip. 28 appended `square-continuation-*` cases reach it. The four extreme-y controls that recorded the deferral are now refused by both programs at the same `SquaredDistance` overflow. Missed in the first revision; found by the review of 8758bb0152. |

The SEG and CIRCLE unit records are not edited here (merged, owned elsewhere); the generators,
control fixtures and shim tests that recorded the old deferral counts were updated in this
unit's PR. The geometry header unit's record notes the POLY_YSTRIPES_INDEX caller change, and
the vector unit's record notes the `VECTOR2<int>` stream co-port below.

## Review corrections (8758bb0152)

The independent review of 8758bb0152 requested four corrections; all are made in 73180197b5.

| Finding | Correction | Evidence |
| --- | --- | --- |
| SEG `operator<<` ignored the caller's stream state | `impl StreamInsert for Seg` writes `"[ " << A << " - " << B << " ]"` into a `StreamState` through a minimum co-port of `VECTOR2<int>`'s `operator<<` (vector2d.h:699-704, `impl StreamInsert for Vector2<i32>`), built only from the accepted `stream::insert_str` and `i32` insertion; no stream state is unavailable, so nothing defers. The adapters take an optional `state` and reply `{text, width_after}`. | 115 `seg_stream-state-*` cases, e.g. `hex|showbase|uppercase`, fill `_`, width 5 on `SEG( 10, 11, 12, 13 )` gives `___[ [ 0XA | 0XB ] - [ 0XC | 0XD ] ]` with width 0 after; the 13 default-stream cases are unchanged |
| Raw `AddShape( SHAPE* )` refused `nullptr` | `add_shape( Option<Box<dyn Shape>> )` stores a null entry (the `dynamic_cast` of null is null); the shared overload takes an `Option` and refuses null as undefined | `shape-compound-null-only` (`Empty` false, `Size` 1, `UniqueSubshape` null, `GetIndexableSubshapes` `[null]`) and five more successful cases; shared null, flatten of a null entry and `Clone` of one are controls |
| `SHAPE_TYPE_asString` excluded the defined values 11 to 15 | `ShapeTypeValue` holds the whole value range 0 to 15; unnamed values return `""` | `shape_type_as_string-11` to `-15` succeed with `""`; -1, 16, `INT_MIN`, `INT_MAX` are undefined-conversion controls |
| `POLY_YSTRIPES_INDEX::Contains` still deferred at `SEG::Square` | calls `Seg::square` | table above; the review's repro (point [11,5], accuracy 2, 10x10 square) is `square-continuation-review`, `true` in both programs |

## Verified domains for deferred out-of-line calls

The public corpus must hold an oracle result for every case. Cases that defer in Rust still run
out-of-line C++ bodies this unit does not port (shape_arc.cpp, shape_compound.cpp,
shape_collisions.cpp), whose undefined domain the adapter does not replay. Instead of claiming a
guard it cannot prove, the adapter (and the Rust shim, identically) accepts those calls only
inside a stated domain and refuses the rest as "outside the verified domain":

- three-point `SHAPE_ARC`: coordinates and width within 2^20, distinct points, mid-point angle
  sine >= 1/8 (so `update_values`' centre and quadrant points stay below 2^25);
- `SHAPE::Collide( SHAPE* )`: a circle or a compound of circles against a circle, all within
  2^20, clearance in [0, 2^20];
- out-of-line `SHAPE_COMPOUND` members: compounds of circles within 2^20, arguments within 2^20;
- `SHAPE_ARC::BBox`: the default arc with width and clearance within 2^20.

This restricts only deferred evidence; every owned inline body is guarded exactly (no domain).
Those owners must replace the domain with exact guards when they port the bodies.

## Undefined behaviour found

- `SHAPE_CIRCLE::Collide( SEG )` with `aActual`: `(int) sqrt( dist_sq )` is undefined when
  `minDist` is `INT_MIN` and `dist_sq` (< 2^62) rounds to 2^62 as a `double`; e.g. a radius-0
  circle at the origin, clearance `INT_MIN`, point (2147483524, 729778): `dist_sq = 2^62 - 44`.
  The collision itself (`true`) is defined. Recorded as controls.
- `SHAPE_ARC::IsCCW`: the `int64_t` cross product overflows for spans near 2^32 (reachable only
  through shape_arc.cpp constructors; in-crate test).
- `SEG::CanonicalCoefs` cannot overflow for 32-bit points (`qC = A.x * B.y - A.y * B.x`), but
  `SEG::Collinear`'s line values can.
- `static_cast<SHAPE_TYPE>` of a value outside the enumeration's range 0 to 15 ([dcl.enum]/8).
- A null `SHAPE_COMPOUND` entry is dereferenced by the shared `AddShape( nullptr )`, by
  flattening the compound into another (`subshape->Clone()`) and by its copy constructor.

## Local diagnostic evidence (not the reference)

On 73180197b5: GCC 15.2 / glibc 2.43 oracle (WSL, `~/kicad-port-p003`, built from a raw-blob
export of the commit, ctest 16/16), native Rust (Windows), debug and release identical:

| Corpus | Cases | Compared | Deferred | Failures | Controls |
| --- | --- | --- | --- | --- | --- |
| p003 | 4227 | 4006 | 221 | 0 | 466/466 as recorded (3 after a Rust deferral) |
| seg | 9503 | 9503 | 0 | 0 | 1887/1887 identical |
| circle | 4978 | 4636 | 342 | 0 | 84/84 as recorded |
| geometry p001 | 22199 | 21042 | 1144 | 13 | 7/7 identical, 6/6 priority |

The 13 geometry failures are in code this PR does not touch and are local toolchain effects:
with the hosted workflow's `GLIBC_TUNABLES` (no AVX/FMA libm variants) the seven `unary-tan-*`
and `unary-cos-303` differences disappear (21049 compared), and the remaining six
`stream-{6,7,8}-False-fixed-*` print `INF`/`NAN` for `fixed | uppercase` under libstdc++ 16
where the reference GCC 13 libstdc++ prints `inf`/`nan`. All 2323 poly cases compare exactly.
Import-free wasm replays identically: p003 4693 requests, geometry 22199, circle 5062. The hosted
workflows (`rust-shape-headers-live`, `rust-seg-live`, `rust-circle-live`, `rust-geometry-live`,
GCC 13.3 / glibc 2.39) are the reference evidence.
