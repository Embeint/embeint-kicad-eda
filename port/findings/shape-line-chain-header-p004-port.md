# shape_line_chain.h, shape_index.h and shape_index_list.h port (p004): domains and evidence

Unit: `libs.kimath.include.geometry.application.ui.p004` (KiCad 4cf939778df0). Details per body are
in `rust/kicad-shape-line-chain-shim/AUDIT.md`.

## Callers

No accepted `UNPORTED` tag named this unit before the port (`git grep` over `rust/` found none), so
there is nothing to wire and no caller corpus changes: 0 caller deferrals before and after. The
SHAPE hierarchy's `ShapeLineChainBase` trait (p003) gains its first concrete implementation.

## Undefined behaviour found in the inline bodies

- `CPoint` with an index outside `[-PointCount(), 2 * PointCount())`, and `CLastPoint` on an empty
  chain (`static_cast<size_t>( 0 ) - 1`), index outside `m_points`.
- `Arc`, `amendArcStart` and `amendArcEnd` index `m_arcs` before any other work; every chain the
  inline API can build has no arcs, so these are always undefined there.
- `ArcIndex`/`reversedArcIndex` index `m_shapes` after `IsSharedPt` (which checks its own bound):
  undefined for a segment at or past the point count, after the call the port defers.
- `Distance` converts `sqrt( SquaredDistance )` to `int`: undefined for a chain without segments,
  where `SquaredDistance` returns `ECOORD_MAX` (sqrt about 3.04e9).
- `BBox`'s `aClearance + m_width` and `BOX2::Compute`'s `vmax - vmin` are `int` operations;
  `Move` adds to every point and to the cached box's origin.
- `BOX2::SquaredDistance( BOX2 )` squares `int64_t` gaps up to 2^33 (list queries on far boxes).
- `SHAPE_INDEX_LIST` iterators: dereferencing or incrementing the end, including the null
  `begin()` of a list that never allocated, and copying a default `query_iterator` (indeterminate
  `bool`, pointer and `int` members).
- `defaultShapeFunctor` on a null item, and `SHAPE_ENTRY` on an item without a shape for layer -1.
- Through `SHAPE_INDEX`, the accepted COW R-tree's insertion arithmetic: `ChildEnlargement`'s
  `int64_t` products (and their difference) at an internal node, and `simpleSplit`'s `int` extent
  `hi - lo` when a full leaf splits (`index-edge-split-extent-overflow`,
  `index-edge-enlargement-overflow`, `index-edge-reindex-split-overflow`); `BulkLoad`'s
  `uint32_t` conversion of a Hilbert coordinate whose centre lies outside the global bounds (an
  entry whose `GetRight` wrapped, `index-edge-bulk-conversion-overflow`).

## Defined behaviour worth noting

- A cached box moved past `INT_MAX` wraps in `GetRight` (modular conversion) and the next `Merge`
  stores a negative size through `SetEnd`, which the following `Merge` normalises
  (`chain-edge-move-wraps-right`).
- `Clear` keeps the width and the cached box; the next `Append` resets the box.
- `GetWidth` is not overridden: a chain reports `SHAPE`'s 0 whatever its `Width()`.
- The COW tree iterator value-initialises its current item, so `SHAPE_INDEX::Iterator`'s
  `operator*` is defined at the end: `nullptr` before any item, else the last item reached.
- `Accept` copies the visitor for every item (`acceptVisitor` takes it by value), whereas `Query`
  passes it by reference to the tree's search, which stops only the current leaf on `false`.
- `SHAPE_LINE_CHAIN`'s copy assignment follows libstdc++'s `vector::operator=` for `m_arcs`: past
  the capacity every arc is copy-constructed into new storage before the old arcs change; within
  it the existing arcs are assigned first and the rest copy-constructed after them. A chain whose
  arcs shrank keeps the capacity, so a later growing assignment has already assigned its first arc
  when the (deferred) `SHAPE_ARC` copy constructor runs. Only chains built through out-of-line
  members hold arcs, so this is covered by in-crate tests and a GCC 13.3 probe, not the corpus.
- `SHAPE_INDEX_LIST` iterators compare `std::vector` iterators, which libstdc++ compares as
  element pointers: iterators of two live lists differ even at the same offset (`begin()` and
  `qbegin( nullptr, 0, false )` of two one-entry lists, `list-edge-two-lists`, `list-pair-*`). The
  standard leaves `==` across different sequences outside the forward-iterator domain; the pinned
  build (libstdc++ without `_GLIBCXX_DEBUG`) defines it as that pointer comparison, which the port
  follows.
- The COW R-tree does no arithmetic until a leaf is full, so the first sixteen insertions are
  defined for any box (`index-edge-defined-index`, a box at 536870913); a later insertion is
  defined while every extent fits an `int` and every area product an `int64_t`
  (`index-edge-large-split-query-reindex`). A wrapped `GetRight` stores an inverted box, which the
  tree handles without arithmetic until it splits or bulk loads (`index-edge-bulk-wrapped-defined`).
- A failed `wxCHECK` reaches `wxOnAssert` with the doubly stringified condition and an empty
  message (`(const char*)NULL`); wx 3.2's default handler logs and returns, so the check's value is
  returned. Recorded in both programs.

## Guards, verified domains and the harness limit

The owned bodies and the accepted COW R-tree are guarded exactly (the adapter replays the tree's
insertion and bulk-load arithmetic on the genuine tree; see AUDIT.md). Only the out-of-line bodies
the port defers (shape_line_chain.cpp, shape_collisions.cpp) keep a verified domain, listed in the
adapter header and AUDIT.md; anything outside it is refused as "outside the verified domain" before
the call, and their owners replace the domain with exact guards when they port the bodies.
`ReservePoints` above 2^22 points (up to `max_size()`) is refused as "harness allocation budget
exceeded": a host-dependent allocation the harness does not run, not an invalid input.

## Local diagnostic evidence

GCC 13.3 oracle (WSL, glibc 2.43 with the hosted `GLIBC_TUNABLES` and `fma` bound to glibc 2.39
libm): 4256 cases, 4053 compared, 203 deferred, 0 failures; 1548 controls as recorded on both
programs; wasm identical. The review reproductions (two lists' `begin()` and `qbegin()`,
`ReservePoints( 1048577 )`, a first insertion at 536870913) are compared cases. The hosted
`rust-shape-line-chain-live` run (GCC 13.3, glibc 2.39) is the reference.
