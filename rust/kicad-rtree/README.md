<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Dynamic R-tree

Owns the complete 1,812-line `dynamic_rtree.h` at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, unit
`libs.kimath.include.geometry.rtree.application.pure.p001`. This is an actual
R*-tree, using the accepted `RtreeNode` and `SlabArena` without node, allocator or
vector co-ports. No edits to the node owner, vector worker or numerical util worker
are needed. No open topic is a dependency. The claim starts at accepted
`rust@eb4cd4724ff6f5fa2119c86248c1a4beb4847451`.

## Audit and public surface

| Owned upstream definitions | Bodies/branches in this crate | Observable checks | Remaining scope |
| --- | --- | --- | --- |
| constructor, destructor, allocNode/freeNode/removeAllNodes; size/empty/MemoryUsage | Owning arena; postorder RemoveAll retains pages/free-list; dropping destroys storage | Empty/repeated clear, post-mutation memory, >256-node local test | OOM exception equivalence not claimed |
| move constructor/assignment | move_out leaves empty fresh source; move_assign drops destination and transfers complete arena | Construct, occupied assignment, source memory zero, self-move native no-op | Rust borrowing makes self assignment an implicit no-op at caller boundary |
| Insert/insertImpl/chooseSubtree; overlap helpers | Leaf selection minimizes overlap increase, then area increase, then area; higher levels minimize area increase then area | Incremental vs packed mutations, traversal and query order, duplicate/touching/equal entries | Other coordinate/payload types below |
| overflowTreatment/forcedReinsert | Per-insertion level mask; descending center-distance sort, per-axis truncation then checked addition; close entries retained and far entries reinserted | Repeated insertion at full leaf; fanout 3 reaches reinsert-count-zero fallback | Internal forced-reinsert branch and level>=32 are unreachable through public API: overflowTreatment is called only on a level-0 leaf. Internal split continuation still exists |
| splitNode/splitNodeInternal | Full leaf min/max-axis distributions, perimeter axis choice, overlap/area tie breaks, optional max-order winner; internal min-only sort; root creation and recursive propagation | Different fanouts and dimensions, equal keys, deep insertions | Native branch coverage not yet measured; do not infer it from case counts |
| computeSplitPerimeters/findBestSplitIndex/computeSplitOverlap | Exact integer expression order; split-overlap uses >= while node-overlap uses > | Disjoint/touching/overlapping and zero-area groups | Overflow controls are Rust-only, never knowingly executed in C++ |
| adjustPath/findChildSlot | Update existing child boxes bottom-up | Queries after splits, forced reinsert and removals | Private missing-child no-op cannot be constructed through a valid public tree |
| Remove/removeImpl/reinsertOrphans/condenseRoot | First overlap-guided removal then full-range fallback; swap-last; underflow exemption is based on parent being root; orphan entries retain insertion bounds; roots collapse repeatedly | Present/absent/duplicate and wrong-box removals, packed-tree mutations, full depletion | Native branch coverage of internal orphan path pending measurement |
| reinsertNode/chooseSubtreeAtLevel | Correct target level, area-growth descent, append or internal split | Removal from packed and incremental multilevel trees | Same internal-orphan coverage gap |
| Search/searchImpl | Node overlap mask, low-bit-first traversal, count increment before callback, propagate false | stop 1 and unlimited/nonpositive visitor policy; count and hits compared | Pinned header has no max-hit parameter; the adapter implements that policy in the visitor |
| Iterator (all methods), begin/end | Lazy stack, payload copy, end flag comparison only, retain last current after exhaustion; current None represents default null payload | Empty/nonempty, equality of different non-end positions, repeated increment, prefix limit | Rust references prevent mutation during iterator/range lifetime; native mutation invalidation is not exercised as defined behaviour |
| SearchIterator/SearchRange (all methods), Overlapping | Same lazy stack with scalar per-slot overlap, bounds copied by value, empty probes one traversal | Full ordered lazy results vs visitor, range empty | No result-vector stand-in |
| minDistSq/NearestNeighbors | Checked widened differences/products/sums; clear output first; actual node-distance priority queue | k negative/zero/one/above count, equal distances, seeded output cleared | Queue/sort equivalent-key order targets GCC libstdc++, not libc++/MSVC |

`order.rs` ports the observable introsort and heap permutations used by libstdc++
(GCC 13): equivalent-key permutations affect traversal, duplicate removals and NN
ties. These are derived from GNU libstdc++ `bits/stl_algo.h` and `bits/stl_heap.h`,
Copyright Free Software Foundation, GPL-3.0-or-later with GCC Runtime Library
Exception. No upstream KiCad source is modified. Fixed floating operation order,
no FMA/reassociation, integer-nanometre coordinates, no unsafe/global state.

## Domain, resources and failure lifecycle

`Coord` is the accepted sealed i32/i64 trait. Payloads implement the accepted
pointer-sized-data `NodeData` abstraction. The generic tree currently supports
positive dimensions and fanouts 3..31 (2D requires fanout divisible by four, as the
reference overlap helper requires). Other template instantiations, including
floating and unsigned coordinates and larger non-2D fanouts, remain unsupported;
this is an explicit partial port, not a statement that those C++ types are invalid.
There are no dependency stubs or manufactured `Unported` results.

No blanket coordinate, area, query, or min<=max guard excludes a valid operation.
Every actually evaluated signed operation is checked in reference order. Float
casts check the truncated value's C++ representability, including the exclusive
2^63/2^32 endpoints. For example extreme coordinates remain accepted when an empty
or k<=0 nearest search does no distance arithmetic, or an insertion into a nonfull
leaf does no area arithmetic. Bulk normalization is validated before dropping the
previous tree. Rust tests distinguish those valid paths from overflow refusals.

The host supplies an item budget (`with_limit`, default one million, maximum
INT_MAX). `with_resources` also supplies a maximum active-node budget (default 65,536).
Slab storage is rounded to 256-node pages and recycled, so the page bound follows
from that node budget. All depth-dependent tree algorithms use explicit stacks;
there is no coordinate or depth exclusion. The monotone fanout-4 1,200-item case
reaches a tree height above 64 and then fully removes/condenses it. Resource
exhaustion is Resource, not an assertion of undefined C++ input. Temporary
vectors use try_reserve; allocations inside the accepted slab and payload Clone
retain their existing allocation-failure boundary.

A query failure clears or partially fills its caller-owned output according to the
upstream order; NN clears before checking root/k. A failed arithmetic mutation has
no defined C++ result. The safe tree becomes FailedMutation and refuses subsequent
reads/mutations until RemoveAll resets its arena (it does not present partial state
as success). Pre-mutation item-budget and bulk-normalization refusals preserve the
old tree. This does not claim transactional recovery from C++ bad_alloc.

MemoryUsage reports the accepted slab's reference accounting, not Rust heap usage:
LP64 Linux x86-64 GCC/libstdc++ node/PAGE layout and modeled free-list capacity.
The node owner has measured those layouts; this unit additionally compares tree
allocation/free sequences live. Other allocators/platforms and libc++ vector
capacity are not certified by that formula.

## Validation boundary

The native JSONL shim is `../kicad-rtree-shim`. The registered C++ adapter calls the
real pinned templates with stable pointers to real ITEM objects, not list/search
stand-ins. Both retain duplicate data identities and insertion boxes. The corpus
contains only public fixed-seed generated inputs. Goldens are intentionally absent.
The current corpus has 201 cases / 56,023 operations. Status stays partial pending
hosted live evidence and independent review.

Current local evidence (macOS ARM64, Rust 1.96.1, -j2): 9 tree and 19 existing node
tests pass; scoped clippy, format, portable wasm compilation and 47 refusal/47
following valid requests pass. These results do not qualify GCC ordering.

Remaining oracle boundary: the C++ adapter validates schema, resource counts and
exact bulk float-cast preconditions, but has not implemented a complete preflight
for every internal signed overflow on arbitrary external sequences. Known signed
UB controls run on Rust only. Run only the deterministic audited corpus/refusal
controls on C++ until this is closed; native UBSan qualification of those cases is
required. Do not call this a general safe native service or mark the unit green.
