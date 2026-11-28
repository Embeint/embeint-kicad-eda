# Audit: shape_line_chain.h, shape_index.h and shape_index_list.h inline bodies (p004)

Unit `libs.kimath.include.geometry.application.ui.p004`, pin `4cf939778df0`. Rust:
`kicad_math::shape_line_chain`, `kicad_math::shape_index`, `kicad_math::shape_index_list`
(in-crate tests `shape_line_chain_tests.rs`). Oracle: `port/oracle/shims/kimath_include_geometry_p004.cpp`.

## Owned bodies and their deferrals

| C++ | Rust | Out-of-line call (owner) |
| --- | --- | --- |
| `CLIPPER_Z_VALUE()`, `CLIPPER_Z_VALUE( pair, offset )` | `ClipperZValue::{new, from_indices}` | none; a `ssize_t` overflow of `aVal += aOffset` is undefined |
| `INTERSECTION()` | `Intersection::default` | none |
| `compareOriginDistance` ctor, `operator()` | `CompareOriginDistance::{new, compare}` | none (`VECTOR2I` `-` per component, `EuclideanNorm` of the accepted vector port) |
| default ctor, copy ctor | `ShapeLineChain::{new, copy_of}` | `SHAPE_ARC` copy ctor per arc (shape_arc p001) |
| copy assignment | `assign_from`, `assign_arcs` | `SHAPE_ARC` copy ctor for each arc libstdc++'s `vector::operator=` constructs: every source arc when the source exceeds the modelled capacity (before the old arcs change), else the arcs past the current size after the existing ones are assigned with `SHAPE_ARC`'s implicit copy assignment (co-port below) |
| move assignment | `move_assign_from` | none; the source's vectors are left empty, other members copied |
| `Clear`, `IsClosed`, `SetWidth`, `Width`, `SegmentCount`, `PointCount`, `CPoint`, `CPoints`, `CLastPoint`, `CArcs`, `CShapes`, `ArcCount`, `Arc`, `IsSolid` | same names | none |
| `SetClosed` | `set_closed` | `mergeFirstLastPointIfNeeded` (shape_line_chain p001), after `m_closed` is set |
| `CSegment`, `GetSegment` | `csegment`, `get_segment` | `Segment` (p001) |
| `BBox`, `GenerateBBoxCache`, `GetCachedBBox` | trait `bbox`, `generate_bbox_cache`, `get_cached_bbox` / `cached_bbox_mut` | none (accepted BOX2 port) |
| `ReservePoints` | `reserve_points` | none; `std::length_error` above `max_size()` |
| both `Append( point )` | `append_xy`, `append` | none (`SHAPES_ARE_PT` co-port) |
| `Remove( int )` | `remove` | `Remove( int, int )` (p001) |
| `operator!=` | `ne` | none |
| `Move` | trait `move_by` | `SHAPE_ARC::Move` (shape_arc p001) per arc, after every point moved |
| `ArcIndex`, `reversedArcIndex` | `arc_index`, `reversed_arc_index` | `IsSharedPt` (p002), before the `m_shapes` index |
| `amendArcStart`, `amendArcEnd` | `amend_arc_start`, `amend_arc_end` | `amendArc` (p001), after the `m_arcs` index |
| `Distance( VECTOR2I, bool )` | `distance` | `SHAPE_LINE_CHAIN_BASE::SquaredDistance` (p001), then the `int` conversion |
| `GetPoint`, `GetPointCount`, `GetSegmentCount` | `ShapeLineChainBase` impl | none |
| `Clone`, `Collide` ×2, `Rotate` (declared, out-of-line) | trait methods | p002 `Clone`; p001 `Collide`, `Rotate` |
| `shapeFunctor`, `boundingBox`, `collide`, `acceptVisitor`, `queryCallback` | same names (snake case) | `wxOnAssert` (host effect, `deferred_assert` in production); `collide` reaches the shape's `Collide` overloads |
| `SHAPE_INDEX` (ctor, `Clone`, both `Add`, `Remove`, `RemoveAll`, `Accept`, `BulkLoad`, `Reindex`, `Query`, `Begin`, `begin`, `end`, `Size`) and `Iterator` | `ShapeIndex`, `ShapeIndexIterator` over the accepted `CowForest` | none beyond the tree |
| `SHAPE_INDEX_LIST` (`SHAPE_ENTRY`, `Add`, `Remove`, `Size`, `Query`, `Clear`, `qbegin`, `qend`, `begin`, `end`), `iterator`, `query_iterator` | `ShapeIndexList`, `ShapeEntry`, `ListIterator`, `QueryIterator` | `SHAPE::Collide( const SHAPE* )` (shape_collisions p001) in exact queries |

No owned body or continuation is missing. The protected members are reached by the adapter through
derived-class member pointers.

## Modelling decisions

- `size_t`/`ssize_t` are `u64`/`i64` (LP64 reference); `PointCount`/`SegmentCount` keep the C++
  modular `int` conversions.
- Vector iterators are libstdc++ element pointers: a position is the list's allocation and an
  offset into it, so iterators of two live lists never compare equal. A null pointer is `None`,
  which is what a default iterator holds and what `begin()`/`end()` return before the list's first
  allocation (libstdc++ and Rust both keep their allocation through `erase`/`clear`); it equals only
  another null pointer. The allocation's address is compared, never observed. End dereferences and
  increments are refused.
- `m_arcs.capacity()` is modelled as libstdc++ keeps it (exact after copy construction, kept by a
  shrinking assignment and `Clear`, taken by move assignment): copy assignment's branch depends on
  it. Diagnostic GCC 13.3 probe (an element type whose copy constructor reports the destination,
  then throws): shrinking keeps capacity 3; growing within it shows the existing element already
  assigned (width 9) at the first copy construction; growing past it shows it unchanged (77).
- A default `query_iterator` keeps its indeterminate members as `None`; copying it is refused.
- `Accept` and `queryCallback` copy the visitor per call, as `acceptVisitor` takes it by value.
- `SHAPE_INDEX` borrows the accepted `CowForest`; `destroy` is the destructor, `clone_index` the
  `Clone` with its move assignment from the temporary.
- Self copy/move assignment cannot be expressed with Rust borrows; the adapter's self-assignment
  steps are no-ops in both programs, as in C++.
- Doubles: only `BOX2::Distance` (`KiROUND( sqrt )`, accepted port) and the deferred `Distance`
  conversion; each is a single correctly rounded operation, so operand order cannot matter.

## Minimum co-ports

- `SHAPE_LINE_CHAIN::SHAPE_IS_PT` and `SHAPES_ARE_PT` (shape_line_chain.cpp:42-43,
  `libs.kimath.src.geometry.shape_line_chain.application.ui.p001`): `shape_line_chain::{SHAPE_IS_PT, SHAPES_ARE_PT}`.
- `SHAPE_ARC`'s implicit copy assignment (shape_arc.h, `libs.kimath.include.geometry.application.ui.p003`):
  `ShapeArc::assign_from`.

## Guards

Every owned inline body and the accepted COW R-tree are guarded exactly. Before an insertion the
adapter walks the genuine tree read-only along the path `insertSimple` takes, evaluating
`ChildEnlargement` (`ChildArea` and the enlarged area as `int64_t` products, then their difference)
for every child of each internal node and, at a full leaf, `simpleSplit`'s `int` extents; `Reindex`
replays its insertions on a scratch tree; `BulkLoad`'s Hilbert coordinates are checked before the
`uint32_t` conversion. The Rust shim has no tree guard: the accepted tree reports the same undefined
operations (`kicad_math` maps the tree's and the node's overflow and conversion errors to
`Undefined`). Insertions beyond 2^29 are compared (a first insertion at 536870913, 40 boxes over
[-2^30, 2^30) that split, query, reindex and bulk load); controls cover an overflowing split extent,
`ChildEnlargement` product, reindex and Hilbert conversion.

## Verified domains (out-of-line calls the port defers, whose undefined domain is not replayed)

Arc-free chains (every chain the adapter builds) for shape_line_chain.cpp; `Segment` with an index
its `wxCHECK` accepts; `Remove` with an index of at least `-PointCount()`; `SquaredDistance` with
every point, the width and the query point within 2^20; `SHAPE::Collide( const SHAPE* )` for two
circles within 2^20 and a clearance in [0, 2^20]; `Collide( SEG / VECTOR2I )` with everything within
2^20. Rust defers at each of these calls, so the domains only keep the oracle's own evaluation
defined; their owners replace them with exact guards when they port the bodies.

## Harness limit

`ReservePoints` above 2^22 points and up to `max_size()` is a defined allocation whose success
depends on the host; both programs refuse it as "harness allocation budget exceeded" (a control),
not as an invalid or out-of-domain input. `ReservePoints( 1048577 )` and 2^22 are compared; sizes
above `max_size()` throw `std::length_error` before allocating and are compared.

## Evidence (local, diagnostic; hosted CI is the reference)

GCC 13.3 (extracted Ubuntu 13.3.0-6ubuntu2~24.04.1) oracle in WSL with the hosted `GLIBC_TUNABLES`
and `fma` bound to the locked glibc 2.39 libm; ctest 17/17.

| Check | Result |
| --- | --- |
| difftest `--partial` | 4256 cases, 4053 compared, 203 deferred, 0 failures, 0 unprocessed |
| controls | 1548 refused as recorded on both programs (169 after a Rust deferral) |
| wasm replay | every case and control identical, import-free |
| generator `--check` | identical in WSL (LF) and on Windows with the files converted to CRLF and in a fresh `core.autocrlf=true` checkout |

Deferred cases by owner: `mergeFirstLastPointIfNeeded` 76, `SHAPE::Collide( const SHAPE* )` 52,
`Remove( int, int )` 36, `IsSharedPt` 14, `SHAPE_LINE_CHAIN::Collide( SEG )` 11,
`SHAPE_LINE_CHAIN::Collide( VECTOR2I )` 2, `Segment` 8, `SquaredDistance` 4. Controls: 978
undefined (169 after a Rust deferral), 480 outside a deferred call's verified domain, 69 harness
allocation budget, 21 malformed. 345 index cases record `wxOnAssert` calls (returning host).
