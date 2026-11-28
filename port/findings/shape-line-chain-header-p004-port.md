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
- A failed `wxCHECK` reaches `wxOnAssert` with the doubly stringified condition and an empty
  message (`(const char*)NULL`); wx 3.2's default handler logs and returns, so the check's value is
  returned. Recorded in both programs.

## Verified domains

Out-of-line bodies (shape_line_chain.cpp, shape_collisions.cpp) and the COW R-tree insertion and
bulk loading run in the oracle only inside the domains listed in the adapter header and AUDIT.md;
everything else is refused as "outside the verified domain" by both programs before the call.
Those owners replace a domain with exact guards when they port the bodies.

## Local diagnostic evidence

GCC 13.3 oracle (WSL, hosted `GLIBC_TUNABLES`, `fma` bound to glibc 2.39 libm): 4093 cases, 3898
compared, 195 deferred, 0 failures; 1572 controls as recorded; wasm identical; goldens policy
`--base` (Windows), `--wasm` and `--ci` (WSL, every live unit, 0 failures) exit 0. The hosted
`rust-shape-line-chain-live` run (GCC 13.3, glibc 2.39) is the reference.
