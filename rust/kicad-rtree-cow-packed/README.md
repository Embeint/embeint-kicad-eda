<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# COW and packed KiCad R-trees

Owns the complete `dynamic_rtree_cow.h` (834 lines) and `packed_rtree.h`
(528 lines), unit `libs.kimath.include.geometry.rtree.application.pure.p002`, at
KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`. Official starting base:
`98cbf0b63eb68541eb5b4ba7d5cc2d03b71d2cf2` on `origin/rust`. No open overlapping
claims existed. No topic branch was imported. GPL-3.0-or-later.

The accepted `kicad-rtree-node` owns `RtreeNode`, `Payload`, `SlabArena`, handles,
allocator accounting, SIMD-equivalent overlap predicates, Hilbert and Morton
helpers. `kicad-rtree` owns the dynamic R*-tree and GNU ordering implementation.
This unit only makes its existing `order` module and `sort` function public;
no algorithms, other unit statuses, or merged evidence were changed. The GNU
notices remain with that implementation. Neither pinned owned header defines a
nearest-neighbor API; that algorithm remains in p001 and is not copied here.

`Option<T>` models the pointer DATATYPE, including null. As with the accepted
node crate, `NodeData` must model non-owning pointer identity, not payloads with
externally visible clone/drop side effects. The supported template interface uses positive dimensions and fanout >=2.
Coordinates use the accepted sealed
`Coord` (`i32` and `i64`), without a magnitude fence or bound normalization.

## Ownership and API

`CowForest<T,E,N,M>` is the shared address space, following the accepted slab
interface. `new_tree` returns an opaque forest-bound `TreeId`; `clone_tree` shares
its root in O(1), increments its reference count, gives the clone a new allocator,
and prepends one persistent allocator-chain cell. Chains have explicit reference
counts and iterative destruction. Parent destruction cannot end storage retained
by a descendant. `destroy` implements the per-tree destructor, including owner
allocator lookup and postorder node release. Dropping the forest ends all remaining
allocator lifetimes together; there is no recursive `Rc` graph destruction.
Tree IDs are explicit lifetime handles: dropping an ID alone does not destroy a
tree; call `destroy`. The forest is single-threaded, like the accepted arena.

Every mutation first copies each shared node it visits. Copying includes all
bounds and insertion-bound slots (even stale slots), live payloads, and increments
internal child refcounts. Failed removals still copy the visited path, as C++ does.
Insert uses minimum area enlargement with first-slot ties, and median splitting
on the longest axis. The reference's non-root split creates a new root above the
old tree and sibling; it does not balance/propagate the split like p001. Internal
root bounds can intentionally overestimate after that split. No repair is applied.
Removal swaps the last child, updates only the successful path, frees empty
children through their actual owning allocator, and repeatedly condenses the root.
There is no p001-style fallback search or underflow reinsertion.

COW bulk load clears first, sorts by the 16-bit XY Hilbert curve in **every**
dimension count >=2, reorders the caller's entries, then packs leaves and internal
levels. A nonempty 1D COW bulk load is undefined (unconditional axis-1 access) and
is refused; empty 1D bulk load remains supported. The callback's false result
stops only its leaf, because the pinned recursive helper returns void. Counts
include each stopping item and subsequent matching leaves. COW cursors preserve
equality-by-end, cached last/default null dereference and increment after end.
Cursors borrow the forest, preventing invalidation. Explicit move operations
transfer root/allocator/chain and leave a null allocator in the empty source,
matching C++; queries/clear/clone of that source work, insertion requires a new
tree. Self move assignment is a no-op.

`Builder` implements reserve/add/build; build clears items while retaining builder
capacity. Packed centers use `min/2 + max/2` with separate integer truncations;
normalization and accepted order-32 Hilbert/Morton evaluation preserve expression
order. Equal sort keys use p001's libstdc++ introsort, not a stable/item-ID sort.
`PackedRtree` stores contiguous SoA rows, counts, data and bottom-up level offsets.
All unused bounds are initialized to `ELEMTYPE::max`. Parent boxes union actual
child slots. Searches traverse this hierarchy; the 2D path calls the accepted
mask helper and higher dimensions use the pinned scalar predicate. False stops
globally. Packed cursors retain position equality, prefix/postfix increments and
borrowed dereference/arrow access; end dereference and increment are refused.
Moves transfer every buffer and leave an empty source. `memory_usage` reports
Linux LP64 C++ vector accounting (fresh final-size buffers), not Rust heap usage.

## Safety and status

The portable crate forbids unsafe code, unwrap/expect/panic, host I/O and global
state. Traversals, mutation paths and releases use explicit stacks. There is no
fixed 64-level traversal limit in this unit. It does **not** call p001's dynamic
tree/cursors, so p001's documented fixed-depth and borrowed-cursor limitations do
not become missing calls here; that owner and its status are unchanged.

Hosts provide `Budget { entries, trees, nodes }`: entry count per live tree,
lifetime tree creations, and lifetime node allocation calls (including reuse).
These bound retained slab pages, tombstone metadata, chains and traversal work.
The host can raise them; budget refusal does not assert native input is undefined.
Scratch vectors reserve fallibly; checked arithmetic distinguishes signed native
UB, invalid conversions, and host limits. Partial mutation failures poison that
tree until clear; unreachable nodes allocated before failure remain owned by its
slab until final lease destruction. Actual allocator exhaustion in the accepted
p003 slab (and allocating user payload clones) is **not** guaranteed catchable;
no second allocator was introduced. Cursor cloning has the same inherited
infallible-allocation boundary. C++ allocator failure postconditions are not
certified.

Status remains **partial** for that p003 allocation/type boundary, platforms other
than the measured libstdc++ contract, and live iterators across mutation/moves
(prevented by borrowing). There are no missing owned bodies or dependency calls
for the supported interface, and zero `Unported` paths. Undefined/private malformed
node topologies are not exposed. No tolerance, lookup, exclusions or golden.

## Body and branch evidence map

| Owned source bodies | Implementation | Observable evidence | Remaining boundary |
| --- | --- | --- | --- |
| COW constructor, destructor, Clone, moves | `new_tree`, `destroy`/`dispose`, `clone_tree`, move methods; persistent lease/chain refcounts | shared roots/refcounts before divergence; parent/sibling destruction; allocator-memory chain after frees/reuse; moved/null sources; self moves | explicit forest/ID lifetimes; actual OOM |
| ensureWritable; releaseNode/releaseTree/freeToOwningAllocator | `writable`, `release`, `free_owner`, lease release | topology plus refcounts and allocator memory after both successful and missing removals, clear/rebuild and ancestor destruction | private not-owned assertion is unreachable from valid API |
| Insert/insertSimple/simpleSplit | `insert`, `split`; iterative ancestor continuation | root and non-root splits, equal centers, tied axes, inverted/touching/point boxes; >64-level traversal | checked UB refusals are Rust tests, not executed native UB |
| Remove/removeImpl/RemoveAll | `remove`, `clear`, iterative root condensation | shuffled/full deletion, absent/null/duplicate identities, retained siblings after divergence | no private malformed topology injection |
| COW BulkLoad | `bulk_load`; accepted 16-bit XY helper; GNU sort and bottom-up packing | argument reorder, tied Hilbert values, >2 levels, empty replacement and clones | nonempty 1D reference body is undefined |
| COW Search/searchImpl; size/empty | `search`, `len`, `is_empty` | per-leaf stop=0/3/all, no-hit/inverted/touching/full-domain boxes, callback order and count | no mutation from a borrowed visitor |
| COW iterator constructors/advance/operators/begin/end | `CowCursor` and forest cursor methods | skip/take, copy/increment equality, default/last dereference and after-end increment | no dangling native cursor support |
| packed Builder Reserve/Add/Build | `Builder`; center normalization, GNU sort, SoA packing | repeated builder reuse; 0/1/M/M+1/M²+7 items; odd negative halves; all bound rows, counts and offsets | supported type/platform boundary |
| packed Search/searchNode/childOverlaps | `search`, `mask`, `overlaps` | 2D mask plus 1D/3D scalar paths; stop propagation/order, touching/inverted/empty queries | no private malformed layout injection |
| packed move constructor/assignment, MemoryUsage, size/empty | move methods, `memory_usage`, `len`, `is_empty` | every buffer, memory count and source-empty state; populated target replacement; self move | LP64 accounting, not physical allocation measurement |
| packed iterator constructor/operators/begin/end | `PackedCursor` | position equality, skip, postfix old value, arrow read, exhaustion | end dereference/increment are undefined in C++ and refused |

See the sibling shim README for commands, corpus counts and revision-specific
native/wasm evidence. All 49 extracted USRs are included in this full-header audit.
Five portable behavior tests additionally check lifetime,
stopping, deep paths, exact arithmetic refusal and move/budget contracts.
