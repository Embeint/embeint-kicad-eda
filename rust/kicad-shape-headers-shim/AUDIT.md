# seg.h and shape header audit

Unit `libs.kimath.include.geometry.application.ui.p003`: the inline bodies of
`libs/kimath/include/geometry/seg.h`, `shape.h`, `shape_arc.h`, `shape_circle.h`,
`shape_compound.h` and `shape_ellipse.h` (1,686 lines, pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). Implementation: `rust/kicad-math/src/seg_header.rs`,
`shape.rs`, `shape_arc.rs`, `shape_circle.rs`, `shape_compound.rs`, `shape_ellipse.rs` (in-crate
tests `shape_tests.rs`). Oracle adapter: `port/oracle/shims/kimath_include_geometry_p003.cpp`.

## Adopted co-ports

| Symbol | First co-ported by | Now |
| --- | --- | --- |
| `SEG::A`, `B`, `m_index`, the four constructors, copy constructor and assignment, `operator==`, `Index` | seg.cpp unit, PR #74 (`seg.rs`) | moved unchanged into `seg_header.rs`; `seg::Seg` re-exports it |
| `SHAPE::MIN_PRECISION_IU` (= 4) | circle.cpp unit, PR #95 (`circle.rs`) | `shape::MIN_PRECISION_IU`; `circle::MIN_PRECISION_IU` re-exports it |

## Co-ported here

| Symbol | Owner | Rust |
| --- | --- | --- |
| `operator<<( std::ostream&, const VECTOR2<T>& )` (vector2d.h:699-704) at `T = int`, into a caller's stream state | `libs.kimath.include.math.application.ui.p002` (recorded in its notes) | `impl StreamInsert for Vector2<i32>` in `vector.rs`: five insertions through the accepted `stream::insert_str` and `i32` insertion; the owner's default-state `Vector2::stream` is unchanged |

## Owned bodies

| Header | Bodies | Port |
| --- | --- | --- |
| seg.h | constructors, copy/assign, `==`, `!=`, `Square`, `Side`, `IntersectLines`, `CanonicalCoefs`, `Collinear`, `Overlaps`, `ApproxOverlaps`, `Contains( SEG )`, `Length`, `SquaredLength`, `TCoef`, `Index`, `Reverse`, `Reversed`, `Center`, `operator<`, `operator<<` | all implemented; `operator<` is `Seg::less` (a `PartialOrd` would disagree with the index-blind `==`); `operator<<` is `impl StreamInsert for Seg`, writing into the caller's `StreamState` (flags reach every coordinate, the width pads only the first insertion `"[ "`, which resets it) |
| shape.h | `SHAPE_TYPE_asString`; `SHAPE_BASE` constructor, `Type`, `TypeName`, the three indexable-subshape virtuals; `SHAPE` constructor, `Clone` (base `nullptr` under `NDEBUG`), `IsNull`, `Collide( VECTOR2I )` forwarding to `Collide( SEG )`, `Centre` (`BBox( 0 ).Centre()`), `GetStart`, `GetEnd`, `GetWidth`, `SetWidth`; `SHAPE_LINE_CHAIN_BASE` constructor, `GetCachedBBox`, empty `TransformToPolygon` | trait `Shape` (default methods) and subtrait `ShapeLineChainBase`; `MIN_PRECISION_IU`; `SHAPE_TYPE_asString` takes a `ShapeTypeValue`, any value of the enumeration's range 0 to 15, and returns `""` for the unnamed 11 to 15 |
| shape_circle.h | every member except `Format` and `TransformToPolygon` | `ShapeCircle`, all implemented except `Rotate` (deferred, see below) |
| shape_arc.h | default constructor, `Clone`, `GetP0`, `GetP1`, `GetArcMid`, `GetStart`, `GetEnd`, `SetWidth`, `GetWidth`, `IsSolid`, `GetChord`, `DefaultAccuracyForPCB` (`ARC_HIGH_DEF` = 5000), `operator==`, `IsCCW`, `IsClockwise`, `Collide( SHAPE* )` forwarder | `ShapeArc`; `Clone` reaches the out-of-line copy constructor |
| shape_compound.h | default constructor, both `AddShape`, `Shapes`, `Empty`, `Size`, `UniqueSubshape`, the indexable-subshape overrides, both `Collide( SHAPE* )` forwarders | `ShapeCompound`; null `SHAPE*` elements are `None`. `AddShape( SHAPE* )` takes an `Option` and stores a null pointer unchanged; `AddShape( std::shared_ptr<SHAPE> )` takes an `Option` and refuses a null pointer as undefined (`aShape->Clone()`) |
| shape_ellipse.h | defaulted copy/assign, `operator==`, `Clone`, the six getters, `IsArc`, `IsSolid`, `Collide( SHAPE* )` forwarder | `ShapeEllipse`; reachable only on memberwise state (every constructor is out-of-line) |

`dynamic_cast<SHAPE_COMPOUND*>` is `as_any().downcast_ref::<ShapeCompound>()`; no KiCad class
derives from `SHAPE_COMPOUND`, so the exact-type downcast is the same decision. Default arguments
of virtual members are passed explicitly (every declaration repeats the same defaults).

## Deferred calls (typed `Unported`)

| Call | Owner | Reached by |
| --- | --- | --- |
| `SHAPE::Collide( const SHAPE*, ... )` (both overloads) | `libs.kimath.src.geometry.shape_collisions.application.ui.p001` | every `Collide( SHAPE* )` forwarder and default |
| `SHAPE_ARC` constructors other than the default, copy constructor (`Clone`), `BBox`, `Collide( SEG )`, `Collide( VECTOR2I )`, `Move`, `Rotate` | `libs.kimath.src.geometry.shape_arc.application.ui.p001` | `ShapeArc` |
| `SHAPE_COMPOUND` vector and copy constructors, `Clone`, `Collide( SEG )`, `BBox`, `Distance( SEG )`, `Move`, `Rotate`, `IsSolid` | `libs.kimath.src.geometry.shape_compound.application.ui.p001` | `ShapeCompound`; `Centre` and `Collide( VECTOR2I )` through them |
| every `SHAPE_ELLIPSE` constructor, `Collide( SEG )`, `BBox`, `Move`, `Rotate` | `libs.kimath.src.geometry.shape_ellipse.application.ui.p001` | `ShapeEllipse` |
| `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` | `libs.kimath.include.application.ui.p001` (trigo.h) | `SHAPE_CIRCLE::Rotate` |
| `kimathLogOverflow` | `libs.kimath.src.math.util.application.ui.p001` | a saturating `KiROUND` inside `CIRCLE::Intersect( SEG )` (none in this corpus) |

Not ported here (out-of-line members of `SHAPE` itself, shape.cpp): `GetClearance`, `Distance`,
`SquaredDistance`, `NearestPoints`, `PointInside`, `Parse`, `Format`; `SHAPE_LINE_CHAIN_BASE`'s
out-of-line overrides (shape_line_chain.cpp); `SHAPE_CIRCLE::Format`, `TransformToPolygon`.

## Undefined behaviour and verified domains

Signed `int`/`int64_t` overflow, `std::abs` of the minimum and an out-of-range floating-to-integer
conversion are refused (`ShapeError::Undefined` / `SegError::Undefined`) at the first such
operation. Found while porting:

- `CanonicalCoefs` is checked operation by operation, but in value `qC = A.x * B.y - A.y * B.x`,
  so with 32-bit points none of its operations can overflow; `Collinear`'s line values can.
- `SHAPE_CIRCLE::Collide( SEG )` with `aActual`: `(int) sqrt( dist_sq )` is undefined when
  `dist_sq` rounds to 2^62 as a `double` (e.g. `minDist = INT_MIN`, `dist_sq = 2^62 - 44`); the
  collision itself is defined. Controls `control-shape-circle-root-*-1` cover it.
- `SHAPE_ARC::IsCCW`'s `int64_t` cross products overflow for spans near 2^32.
- `SHAPE_TYPE` is unscoped without a fixed underlying type and its enumerators span 0 to 10, so
  its values are 0 to 15 ([dcl.enum]/8). Converting a value outside 0 to 15 is undefined
  (controls `control-shape_type_as_string-{below,above,int-min,int-max}`); 11 to 15 are defined.
- A null `SHAPE*` entry is defined to store and to report (`Empty`, `Size`, `UniqueSubshape`,
  `Shapes`, `GetIndexableSubshapes`), but dereferenced by the shared `AddShape` with a null
  pointer, by flattening a compound that holds one (`subshape->Clone()`) and by
  `SHAPE_COMPOUND`'s copy constructor (`Clone`): controls `control-shape-compound-null-*`.
  Flattening `[ arc, null ]` and cloning stop at the Rust deferral first (recorded
  `rust_unported`); the oracle refuses them as undefined.

The adapter cannot replay the undefined domain of the out-of-line bodies it calls for deferred
cases, so those calls are accepted only inside a verified domain (coordinates, radii, widths and
clearances within 2^20; compounds of circles only, so not of null entries; three-point arcs
whose mid-point angle has sine >= 1/8) and refused as "outside the verified domain" otherwise,
by both programs.

## Callers rewired

- `seg.rs` `ApproxCollinear`, `ApproxParallel` (and `ApproxPerpendicular`): `SEG::Square`,
  `SEG::SquaredLength`. Seg corpus: 858 deferrals -> 0 (9503 public cases, all results); 197
  candidates previously dropped as "undefined only after a deferral" became controls (1690 ->
  1887).
- `circle.rs` `ConstructFromTanTanPt`: `SEG::Center`, `SEG::IntersectLines` (and `Square`
  through `ApproxParallel`). Circle corpus: 362 deferrals -> 342 (107 `SEG::Square` removed; the
  cases now yield 20 more results, 70 `CalcArcMid` and 17 `wxOnAssert` deferrals); two tan-tan
  controls now defer at `CalcArcMid`, one is refused identically.
- `poly_ystripes.rs` `POLY_YSTRIPES_INDEX::Contains` (unit
  `libs.kimath.include.geometry.application.ui.p001`, recorded in its notes): `SEG::Square` in
  the `aAccuracy > 1` fallback, then the accepted `SEG::SquaredDistance` and the hole skip.
  Geometry corpus: 553 `SEG::Square` deferrals -> 0 (20474 -> 21027 results on the unchanged
  22171 requests); 28 appended hand cases `square-continuation-*` reach the continuation (point
  [11,5] at accuracy 2 on a 10x10 square is `true`, the squared-distance boundary, hole edges
  skipped, unindexed horizontal edges, clamped stripes, accuracy `INT_MAX`, rebuilds). The four
  extreme-y controls that recorded the deferral (`poly-13-14-{2,10}`, `poly-14-15-{2,10}`) are
  now refused by both programs at the same `SquaredDistance` overflow, so their `rust_error`
  is removed.

## Corpus

`generate_cases.py` (fixed seed) writes 4227 public cases and 466 controls (450 undefined, 16
outside the verified domain). The first 4101 cases are unchanged; the 126 added ones use no
randomness: `shape_type_as_string-11` to `-15`, 115 `seg_stream-state-*` (5 segments x 23
stream states: bases, `showbase`, `showpos`, `uppercase`, `left`/`right`/`internal`, combined
basefields, no flags, every flag, widths -5 to 512, fills, precisions) and 6
`shape-compound-null-*` (a null entry alone, mixed with circles and an arc, added by a call,
next to a flattened compound, with `SetWidth`). Rust: 4006 results, 221 typed deferrals (71
`SHAPE_ARC` copy constructor, 30 three-point constructor, 2 `SHAPE_ARC::BBox`, 34 `RotatePoint`,
20 `SHAPE::Collide`, 40 `SHAPE_COMPOUND` out-of-line, 24 `SHAPE_ELLIPSE` constructors).

## Evidence

Local diagnostic run on 73180197b5 (GCC 15.2 / glibc 2.43 oracle in WSL built from the committed
blobs, ctest 16/16; native Rust on Windows, debug and release; kicad-difftest):

| Corpus | Cases | Compared | Deferred | Failures | Controls |
| --- | --- | --- | --- | --- | --- |
| p003 | 4227 | 4006 | 221 | 0 | 466/466 as recorded (3 after a Rust deferral) |
| seg | 9503 | 9503 | 0 | 0 | 1887/1887 identical |
| circle | 4978 | 4636 | 342 | 0 | 84/84 as recorded |
| geometry p001 | 22199 | 21042 (21049 with the hosted `GLIBC_TUNABLES`) | 1144 | 13 (6) | 7/7 identical, 6/6 priority |

The geometry failures are local toolchain effects in code this PR does not touch: seven
`unary-tan-*`/`unary-cos-303` last-bit differences disappear under the hosted workflow's
`GLIBC_TUNABLES` (no AVX/FMA libm variants), and six `stream-{6,7,8}-False-fixed-*` cases print
`INF`/`NAN` where the reference libstdc++ (GCC 13) prints `inf`/`nan` for `fixed | uppercase`.
Every poly case (2323, the 28 continuations and the 553 former deferrals included) compares
exactly. Import-free wasm replays identically: p003 (4693 requests), geometry (22199), circle
(5062). The hosted `rust-shape-headers-live`, `rust-seg-live`, `rust-circle-live` and
`rust-geometry-live` runs (GCC 13.3 / glibc 2.39) are the reference. Details and the
verified-domain rationale: `port/findings/shape-headers-p003-port.md`.
