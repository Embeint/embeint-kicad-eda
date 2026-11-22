<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-rtree-node

Portable port of `libs/kimath/include/geometry/rtree/rtree_node.h` at KiCad pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, unit
`libs.kimath.include.geometry.rtree.application.pure.p003`. It provides the node,
Hilbert/Morton, overlap-mask and slab-allocator types that the dynamic, COW and packed
trees (other units) are built on. It has no dependencies, `#![forbid(unsafe_code)]`,
no `unwrap`/`expect`/`panic`, no global state, and builds for `wasm32-unknown-unknown`.

## Owned definitions

| Reference | Rust | Notes |
| --- | --- | --- |
| `HilbertXY2D` | `hilbert_xy2d` | Orders 1..=32 (the initial `1U << ( aOrder - 1 )`); modular `uint32_t`/`uint64_t` arithmetic as wrapping ops; coordinates at or above `2^aOrder` behave as in C++ |
| `HilbertND2D<NUMDIMS>` | `hilbert_nd2d` (slice), `hilbert_nd::<N>` | Two dimensions delegate; the Morton fallback keeps `min( aOrder, 64 / NUMDIMS )` high bits. Only its undefined steps are refused: `bits - 1` at `aOrder == INT_MIN`, and a shift of `aOrder - 1 >= 32`, which happens only when the loop runs (`bits >= 1`). Above 64 dimensions every other order returns 0, as do orders <= 0. `NUMDIMS = 0` does not compile in C++ and is refused |
| `OverlapMask2D<ELEMTYPE, FANOUT>` | `overlap_mask_2d::<E, F>`, `overlap_mask_2d_dyn` | Static assertions at compile time (run time in the `_dyn` form); reads all `FANOUT` lanes; `aCount` 0..=31 |
| `RTREE_NODE` constructor, `MINNODES` | `RtreeNode::new`, `MIN_NODES` | Zero bounds, insertion bounds and union, refcount 1 |
| `IsLeaf`, `IsInternal`, `IsFull`, `IsUnderflow` | same names, snake case | Negative levels are neither leaf nor internal, as in C++ |
| `ComputeEnclosingBounds` | `compute_enclosing_bounds` | `max()`/`lowest()` when no child is visited; negative counts visit none |
| `ChildArea`, `ChildPerimeter`, `ChildOverlapArea`, `ChildEnlargement` | same | `int64_t` chains in reference evaluation order; signed overflow (undefined in C++) is `Err(Overflow)` |
| `ChildOverlaps`, `ChildOverlapMask` | same | 2D uses `OverlapMask2D<ELEMTYPE, MAXNODES>` including counts above `MAXNODES` up to 31; other dimensions loop over `count` |
| `Set/GetChildBounds`, `Set/GetInsertBounds` | same | Any slot `0..MAXNODES` |
| `RemoveChild` | `remove_child` | Last slot moved into `i`; the vacated slot's bounds, insertion bounds and pointer stay (stale), `count` decremented |
| `children`/`data` union, `refcount` | `payload`, `child`, `data`, `set_child`, `set_data`, `refcount_*` | See storage below |
| `SLAB_ALLOCATOR`: `PAGE()`, `Allocate`, `Free`, `MemoryUsage`, `Owns`, move constructor/assignment, private `findOwningPage`, `markUsed`, `markUnused` | `SlabAllocator::{allocate, free, memory_usage, owns, take, move_assign, find_owning_page}` | `markUsed`/`markUnused` are the bit updates inside `allocate`/`free` |

## Storage design

* **SoA bounds.** The reference stores `bounds[axis * 2 + side][slot]`; stable Rust
  cannot size an array by `NUMDIMS * 2`, so rows are `[axis][side][slot]` and the
  accessors (`bound`, `set_bound`, `insert_bound`, ...) take the reference row number.
* **The union.** Each slot is a `Payload`: `Null` (the constructor's zero bytes, a null
  pointer for either member), `Child(NodeHandle)` or `Data(T)`. Only pointer-sized
  `DATATYPE`s are ported (every KiCad instantiation stores a pointer), so the union is 8
  bytes per slot and `RemoveChild`'s leaf `data[i] = data[last]` and internal
  `children[i] = children[last]` both copy the same 8 bytes; Rust copies the whole slot.
  Typed reads of the inactive member (`child` on a data slot and vice versa) are refused
  with `InactiveUnionMember`; `payload` reads the raw slot.
* **Reference counts.** `std::atomic<int>` becomes an `i32` with `load`, `store`,
  `fetch_add` and `fetch_sub` that return the previous value and wrap like C++20 signed
  atomics. Portable crates are single-threaded; a host that shares nodes across threads
  serialises access to the allocator, which owns the node storage.
* **Copying.** Like the reference (whose atomic member deletes the copy constructor), a
  node is not `Clone`; a copy-on-write caller copies fields through the accessors, using
  `SlabAllocator::node_pair_mut` for a source and destination at once.

## Node identity and allocator ownership

The reference returns `NODE*` into 256-node pages and decides ownership by comparing a
pointer with each page's address range. Here every page owns an `Rc<PageToken>` and a
`NodeHandle` is a `Weak` reference to that token plus the slot offset:

* Handles are equal exactly when they name the same slot of the same page, wherever that
  page has moved. No address is compared numerically or reaches any output (`Debug`
  prints only the slot).
* `owns`/`find_owning_page` look for the handle's token among this allocator's pages, in
  page order: page membership, including freed slots, not live occupancy.
* Node storage lives in the pages, so nodes are read and written through
  `node`/`node_mut`, which require the slot to be allocated. A freed slot is unreachable
  until `allocate` re-constructs it, matching the ended lifetime after `~NODE()`.
* `take` (move constructor) and `move_assign` (move assignment) move pages, free list and
  free-list capacity together; the source keeps none, as libstdc++ leaves moved-from
  vectors empty with zero capacity. Handles follow their pages. `move_assign` destroys the
  target's previous pages; their handles are then owned by no allocator. The reference's
  `this != &aOther` guard cannot be expressed with two exclusive borrows.
* A live `Weak` keeps its token allocation reserved, so a stale handle never matches a
  page created later. The C++ reference can reuse a destroyed page's address for a new
  page, making stale pointers alias new nodes; that address-dependent aliasing is not
  reproduced (see below).

`allocate` follows the reference order exactly: the most recently freed node (re-marked
used and re-constructed), otherwise the first unused slot of the first non-full page,
otherwise slot 0 of a new page (whose 256 nodes are default-constructed).

## Reference accounting

`memory_usage` returns `m_pages.size() * sizeof( PAGE ) + m_freeList.capacity() *
sizeof( NODE* )` as the pinned reference computes it on Linux x86-64 (LP64, GCC
libstdc++). `layout` derives `sizeof`/`alignof( RTREE_NODE )` from the declared members
(two `int`s, the bounds, the pointer union, the insertion bounds, `std::atomic<int>`) and
`sizeof( PAGE )` from `alignas( 64 ) NODE nodes[256]` plus a 32-byte `std::bitset<256>`.
The free-list capacity follows libstdc++'s `push_back` growth (`size + max( size, 1 )`
when full; `pop_back` keeps capacity). These are labelled reference values: they are not
this port's heap use. The difftest's `layout`, `memory` and `state` steps compare node
size/alignment, page size/alignment, `MINNODES`, `MemoryUsage` and `capacity()` with the
live oracle for every configuration.

## Refused inputs (undefined in the reference)

| Input | Error |
| --- | --- |
| Hilbert order outside 1..=32 (2D); Morton order `INT_MIN`, or above 32 with at most 64 dimensions; zero dimensions | `OrderOutOfDomain`, `DimensionsOutOfDomain` |
| `OverlapMask2D` count outside 0..=31, short lanes, invalid fanout | `CountOutOfDomain`, `SliceTooShort`, `FanoutOutOfDomain` |
| Slot outside `0..MAXNODES`; `RemoveChild` slot not below `count` (the reference asserts) | `SlotOutOfRange` |
| `count` above `MAXNODES` for loops that index with it; a non-2D mask whose overlapping slot is 32 or above (`1U << i` runs only for overlapping slots) | `CountOutOfDomain` |
| Signed 64-bit overflow in area, perimeter, overlap area or enlargement | `Overflow` |
| Access to or `Free` of a freed node (use after free, double destruction) | `NodeNotLive` |
| Typed read of the inactive union member | `InactiveUnionMember` |

## Deliberately unported reference branches

These are not deferrals of another unit; they are reference behaviours this safe port
does not reproduce, and the unit is `partial` until they are resolved by review:

1. **Nodes outside the allocator's pages.** `Free` of such a node runs `markUnused`'s
   not-found branch, destroys the node and pushes it; a later `Allocate` runs
   `markUsed`'s not-found branch and placement-constructs at that foreign address. Here
   `free` returns `ForeignNode` and nothing changes.
2. **Other instantiations.** Only `ELEMTYPE` `int`/`int64_t` and pointer-sized `DATATYPE`
   are supported (the `Coord` trait is sealed). On x86-64 the SSE2 `OverlapMask2D` path
   compares any 4-byte `ELEMTYPE` as signed 32-bit lanes, which differs from the scalar
   path for `uint32_t` values at or above 2^31 and for `float` bit patterns; no KiCad
   instantiation uses those types with this function.
3. **Other platforms.** The oracle is Linux x86-64 with SSE2. The NEON and plain scalar
   paths agree with the implemented predicate for `int`, but are not oracle-checked.
4. **Address reuse.** After a page is destroyed, the reference may place a new page at
   the same address so that stale pointers alias new nodes; handles here never alias.

## Tests

`cargo test -p kicad-rtree-node` runs the behaviour tests in `tests/behaviour.rs`.
Exact agreement with the oracle is checked by the registered difftest
(`kicad-rtree-node-shim`, corpus `port/cases/<unit>.jsonl`); see that crate's README.
