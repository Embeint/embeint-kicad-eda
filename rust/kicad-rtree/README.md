<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Dynamic KiCad R*-tree

Unit `libs.kimath.include.geometry.rtree.application.pure.p001`, owning all 1,812
lines of `libs/kimath/include/geometry/rtree/dynamic_rtree.h` at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The immutable extracted record contains
**57 USRs** (the assignment described 56); this implementation audits the full
header including both iterator classes and SearchRange. It adds no vector type.

The accepted base is `eb4cd4724ff6f5fa2119c86248c1a4beb4847451`, confirmed against live
`origin/rust` before edits. Open PRs #50 and #58 were searched by title, path and
symbol; neither overlapped. No topic history was imported. This unit uses accepted
`kicad-rtree-node::{RtreeNode,Payload,SlabArena,AllocatorId,NodeHandle,hilbert_nd}`
without modifying the node crate, its unit status, or historical evidence.
During implementation the parent accepted PR #50 into official `rust` at
`1ac339f229d8ccb8ccb297f489493622903cc468`; this branch merges that official base
only. The dependency is accepted base history, not an open-topic dependency.

```rust
use kicad_rtree::{DynamicRtree, NodeData};
#[derive(Clone, Debug, PartialEq)]
struct Item(u64); // request/document-local pointer identity
impl NodeData for Item {}
fn example() -> Result<(), kicad_rtree::Error> {
    let mut tree = DynamicRtree::<Item, i32>::new(100_000)?;
    tree.insert([0, 0], [10, 10], Some(Item(7)))?;
    let mut hits = Vec::new();
    let found = tree.search([10, 10], [20, 20], &mut |item| {
        hits.push(item.cloned());
        true
    })?;
    assert_eq!(found, 1); // touching bounds overlap
    let mut nearest = Vec::new();
    tree.nearest_neighbors([15, 10], 1, &mut nearest)?;
    assert_eq!(nearest[0].0, 25);
    Ok(())
}
```

`Option<T>` represents the reference pointer value, including null. `NodeData`
must model pointer identity, as in the accepted node crate: no owning payload
with externally visible clone/drop effects. All node arrays, union access,
allocation, recycling and ABI accounting use that crate. The tree is unique;
no `Clone` of the tree or shared mutable arena is exposed.

## Behavior and ownership

* Insertion uses overlap enlargement immediately above leaves, area enlargement
  higher up, first-slot tie selection, R*-axis/distribution splits, 30% forced
  reinsertion and per-insertion level masks. Internal splits deliberately use the
  simpler min-bound-only axis rule in the pin. The root also undergoes forced
  reinsertion; the implementation does not substitute a different R*-tree policy.
* Removal tries the supplied rectangle first, then the full coordinate domain.
  Removal swaps the last slot into the removed slot. Underflow tests the **parent**
  against the root, exactly as the source does. Leaf orphans reinsert using stored
  insertion bounds; internal orphans reinsert children at their original level.
  Each orphan entry starts a fresh reinsertion mask. Root condensation repeats.
* Bulk load derives floating centers and normalization in reference expression
  order, uses the accepted Hilbert/Morton helper, sorts unique `(curve,index)`
  pairs, and packs leaves/internal levels bottom-up. It replaces tree contents
  and leaves input entries in their original order. Empty bulk load clears.
* Visitor search uses actual node overlap masks and propagates false all the way
  out. Its returned count includes the hit whose callback stops. The pinned
  `Search` has no maximum-hit argument; callers impose limits in the visitor.
  Visitor output appends to caller state. `nearest_neighbors` always clears its
  output, including when the tree is empty or `k <= 0`.
* Nearest search is the actual best-first bounding-box tree traversal. It uses
  integer squared distances and distance-only priority ordering. Equal distances
  reproduce libstdc++ heap adjustment; entries are never sorted by item IDs.
* Both lazy iterator bodies keep a traversal stack and cached current value.
  Equality deliberately compares only `atEnd`, so two non-end iterators compare
  equal even at different positions. Increment after end retains the last value;
  default/empty iterator dereference yields the default null value. `current`
  represents dereference/arrow, `advance` increment, `SearchRange` range methods.
* Rust moves transfer the arena and allocator identity with the root. Explicit
  `move_construct`/`move_assign` additionally leave a reusable, empty source with
  zero reference memory. Target assignment drops its previous arena/pages. Handles
  into transferred pages remain valid in the target and invalid in the source;
  target's previous handles become destroyed/foreign. `move_assign_self` is the
  reference's guarded no-op.
* Clear frees nodes in reference postorder, retains pages/free-list capacity,
  and permits reuse. Drop releases the whole arena. This is equivalent for the
  supported pointer-identity data, with no external node/allocator observer.

`order.rs` translates the libstdc++ introsort/heap algorithms needed to reproduce
unspecified equal-key ordering, rather than pretending Rust's unstable sort has
that order. Original GNU/HP/SGI notices are in `GNU-ALGORITHMS-NOTICE.txt`.
GPL-3.0-or-later is compatible with the original GNU code and its runtime exception.
These algorithms were checked against GCC 12's actual `bits/stl_algo.h` and
`bits/stl_heap.h`; hosted GCC 13 is a separate live comparison, not assumed proof.

## Domain, resources and failure lifecycle

There is no coordinate magnitude fence and no blanket rejection of inverted
bounds. For example, an entire `i32` or `i64` domain box can be inserted into an
empty leaf and searched without evaluating an area; those are successful native
corpus cases. Checks occur at the evaluated subtraction, product, addition or
floating conversion, including sums of otherwise representable node overlaps and
split perimeters/areas. Forced reinsertion converts **each** squared floating
component to `i64` before summing. Bulk conversion accepts negative fractions
in `(-1,0)` because C++ truncates those to zero. Floating contraction stays off.

The sealed accepted `Coord` trait supports `i32` and `i64`. Templates require
positive dimensions and fanout >= 2; budgeted operations use at most 64 levels.
The public constructor takes an explicit maximum entry count. This is a host
resource budget, not an assertion that larger native trees are undefined.
`ResourceLimit`, `Conversion`, `Overflow`, node errors and
`InterruptedMutation` are distinct typed refusals; no missing dependency is
silently converted into a successful empty result. There are currently **zero
Unported calls or dependency deferrals** for the supported interface.

A mutation that encounters undefined reference arithmetic after changing nodes
marks the tree interrupted. Further spatial reads/edits refuse; `clear` then
releases the entire arena and restores a usable empty tree. This avoids reporting
success on partially changed topology. C++ defines no resulting state after that
undefined operation. Budget and bulk-conversion failures discovered before
mutation preserve the old tree. Read-only distance failure does not poison the
index; output has already been cleared, matching the evaluated native lifecycle.
This is not a strong exception guarantee for actual allocation exhaustion.

Tree scratch vectors use fallible reservation where their caller supplies a
count. The accepted SlabArena still uses infallible `Vec` page/metadata allocation,
and payload/entry clones may allocate. Actual allocator exhaustion is therefore
**not certified as a catchable refusal**. No second allocator was introduced to
hide that boundary. The explicit budget bounds normal host workloads; resolving
fallible slab allocation belongs to an agreed extension with the node owner.

## ABI accounting

`memory_usage` is the accepted **reference accounting**, not Rust heap consumption:
`pages * sizeof(C++ PAGE) + libstdc++ freeList.capacity() * sizeof(NODE*)`.
The native `layout` operation measures node/page size and alignment, pointer size,
coordinate size, minimum fill and reinsertion count for every configuration.
State observations compare real C++ `MemoryUsage` after growth, free/reuse, clear,
bulk replacement and moves. Linux LP64 facts do not prove Mac/Windows allocator
behavior or physical heap usage. No such allocation proof is claimed.

## Owned-body audit and branch evidence

Every row maps real source bodies; helper consolidation does not defer their
continuations. The corpus and tests are described in the shim README. Live evidence
is revision-specific; generated gcov records expose unexecuted instantiated paths.

| Upstream bodies | Implementation / preserved branches | Observable test | Remaining limitation |
| --- | --- | --- | --- |
| constructor, destructor, move constructor/assignment; allocNode/freeNode/removeAllNodes | `new`, ownership drop, move methods, accepted slab calls, explicit postorder | empty/source reuse, populated target replacement, transferred handles, post-clear memory | allocation exhaustion; implicit drop has no separate externally observable free-list state |
| Insert, insertImpl, chooseSubtree, chooseSubtreeAtLevel | `insert`, `insert_impl`, `choose`; overlap/area keys and first tie | leaf/internal growth; complete node hierarchy; query order after mixed insert/remove | depth guard 64; other coordinate/data types |
| overflowTreatment, forcedReinsert, reinsertNode | `overflow`, `forced`, `reinsert_node`; masks, per-component cast, retained insertion boxes, level targeting | equal/touching/random distributions; nonroot reinserts; M=3 no-reinsert split fallback | level >= 32 fallback and forcedReinsert internal-node arm are implemented but not public-corpus reached |
| splitNode, splitNodeInternal | `split` with separate leaf/general versus internal comparators and axis policies; recursive parent propagation | root/internal splits; min/max/distribution ties; actual topology | every winning-axis/distribution branch is not separately certified |
| computeSplitPerimeters, findBestSplitIndex, computeSplitOverlap | checked `split_perimeters`, `best_split`, `split_overlap`; strict `lo >= hi` cutoff; earliest tied split | degenerate/touching boxes, equal keys, 1D/2D/3D | extreme overflow guards are Rust tests, not executed native UB |
| computeOverlap, computeOverlapEnlarged, adjustPath, findChildSlot | checked overlap sums, existing node enlargement, reversed ancestor update, missing child returns no slot | full hierarchy after forced reinserts and splits | missing-child path is unreachable from the exposed valid tree API |
| Remove, removeImpl, reinsertOrphans, condenseRoot | direct/fallback search, swap-remove, parent-root exception, empty-child free, leaf/internal orphan continuations, repeated root collapse | shuffled full deletion; M=8 explicit 800-entry cascade; missing/null/duplicate items | private malformed topology is not exposed |
| RemoveAll, size, empty, MemoryUsage | clear, len, is_empty, accepted accounting | empty, grown, cleared, bulk-replaced and moved states | reference ABI scope above |
| Search, searchImpl | real child masks, ascending set bits, count-before-callback, early stop propagation | stop=0/7/all; seeded output append; no hits/touching/inverted queries | visitor cannot mutate a borrowed tree |
| NearestNeighbors, minDistSq | checked integer distance per axis; actual node priority queue; replace output; k handling | ties including all-zero distances, nonpositive/max k, preseeded outputs, near/far queries | libstdc++ ordering contract; no libc++/MSVC claim |
| Iterator constructors, advance, *, ->, ++, ==, !=; begin/end | `Cursor` without query; cached default/last value, lazy stack, equality-by-end | skip/take, copy then increment and compare, arrow read, increment at/after end | borrow prevents invalidation; a live C++ iterator across a tree move is not exposed as a Rust operation |
| SearchIterator constructors, advance, *, ++, ==, !=; SearchRange ctor/begin/end/empty; Overlapping | `Cursor` with owned query, `SearchRange`; inclusive child predicates; lazy descent | empty/matching/pruned ranges, early take, cached dereference and equality | same borrowing restriction |

## Status and validation

Status is **partial**, not green. Other ELEMTYPE/DATATYPE instantiations remain at
the accepted node boundary; platform/library order and allocation failures remain
limited as above. All meaningful owned algorithms and iterator continuations are
implemented. No sorted-list or brute-force substitute is used for spatial queries.
No golden is committed and no case is excluded from native comparison.

At runtime implementation/integration revision
`48a80874093c78521a826219a0d7227582d252bf`: six portable behavior tests, the
26-control shim integration test, focused fmt/clippy, portable wasm check, **249
native programs / 67,713 steps** (249 compared, zero failures/deferrals), **26
native/Rust refusal controls**, and full import-free wasm runtime replay against
fresh native replies all pass. Native reference is Linux arm64 GCC 12.2/libstdc++
with UBSan/float-cast-overflow; Rust is Mac aarch64 1.96.1; wasm runs under Node
v24.16.0 / V8 13.6.233.17-node.49. Retained source/binary binding and gcov evidence
is in `../kicad-rtree-shim/validation/2026-11-24-48a80874/`. Native compilation
used an earlier dirty export whose actual source hashes match that revision;
this is explicitly not a clean hosted x86-64 build claim. A source-known
full-domain/near-2^53/INT64_MAX/negative-fraction bulk extension is included.

Initial 207-program / 59,365-step success was earlier working-tree evidence,
superseded by the complete 249-program run. Native coverage observed 644/660
executed function instantiations, 6,756/7,256 executed line entries and
4,159/5,480 taken branches, retaining unexecuted paths. The dedicated hosted
x86-64 workflow and normal port workflow execute fresh comparisons; hosted CI
and separate review remain pending. No stored output is counted as CI.
