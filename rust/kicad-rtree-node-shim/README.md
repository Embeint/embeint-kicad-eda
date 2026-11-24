<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-rtree-node-shim

Native JSONL adapter (`portable = false`) for unit
`libs.kimath.include.geometry.rtree.application.pure.p003`. Its C++ counterpart is
`port/oracle/shims/rtree_node.cpp`, which calls the pinned, unmodified `rtree_node.h`.
Both use the shared transport (`kicad_difftest::request::parse` here) and accept the
same schema, validate fields in the same order and refuse with the same messages.

## Request schema (`input`)

* `{"kind":"hilbert_xy","order":int,"x":u32,"y":u32}` returns `{"index":u64}`.
* `{"kind":"hilbert_nd","dims":N,"order":int,"coords":[u32; N]}` with `N` in
  1-9, 16, 21, 22, 32, 33, 64, 65 returns `{"index":u64}`.
* `{"kind":"overlap_mask","elem":"i32"|"i64","fanout":4..28 step 4,"count":int,
  "min0","max0","min1","max1":[E; fanout],"qmin","qmax":[E; 2]}` returns `{"mask":u32}`.
* `{"kind":"slab","config":C,"steps":[...]}` (1 to 4096 steps) returns
  `{"events":[...]}` with one object per step. `C` is one of `i32.d2.m16`, `i32.d2.m4`,
  `i32.d2.m28`, `i32.d3.m16`, `i32.d3.m5`, `i32.d3.m32`, `i32.d3.m33`, `i32.d1.m8`,
  `i64.d2.m16`, `i64.d3.m8` (`ELEMTYPE`, `NUMDIMS`, `MAXNODES`; `DATATYPE` is a pointer to one of 64
  request-local items).

Allocators `a`/`src` are 0..3. Nodes are numbered by first allocation (a reused node keeps
its number); items by index. No address is serialised.

| Step `op` | Fields | Event |
| --- | --- | --- |
| `layout` | | `node_size`, `node_align`, `page_size`, `page_align`, `nodes_per_page`, `pointer_size`, `min_nodes` |
| `new`, `move_new`, `move_assign`, `destroy` | `a` (+ `src`) | `{}`; `destroy` runs the destructor and frees the slot |
| `alloc`, `alloc_many` | `a` (+ `n` 1..1024) | `node` / `nodes`; at most 4096 allocations per case |
| `free` | `a`, `node` or null | `{}`; any allocator may free any existing node |
| `owns`, `find` | `a`, `node` or null | `owns` / `page`, `offset` (null when not owned, or for a null node) |
| `external` | | `node`: a new caller-owned `NODE n;` outside every page |
| `destroy_external` | `node` | `{}`: its storage ends (only for external nodes) |
| `neighbour` | `node`, `offset` | `node`: `node + offset` inside the same `PAGE::nodes` array (external: offset 0) |
| `memory`, `state` | `a` | `memory` / `pages` (four 64-bit `used` words each), `free` (node numbers), `capacity`, `memory` |
| `set_count`, `set_level`, `ref_add`, `ref_sub`, `ref_store` | `node`, `value` | `{}` or `previous` |
| `ref_load`, `predicates`, `snapshot`, `enclosing` | `node` | value, flags, the complete node state, or `min`/`max` |
| `set_bounds`, `set_insert`, `overlaps`, `overlap_area`, `enlargement` | `node`, `slot`, `min`, `max` | `{}` or `value` |
| `get_bounds`, `get_insert`, `area`, `perimeter`, `remove` | `node`, `slot` | `min`/`max`, `value` or `{}` |
| `set_child`, `set_data` | `node`, `slot`, `child` / `item` (or null) | `{}` |
| `mask` | `node`, `min`, `max` | `value` |

`snapshot` returns `count`, `level`, every bounds and insertion-bounds row over all
`MAXNODES` slots, each union slot as `null`, `{"child":n}` or `{"item":k}` (classified
from the union's object representation on the C++ side), and `refcount`.

All allocators of a case share one address space (the Rust adapter uses one
`SlabArena`). Any allocator may free any existing node and later re-allocate it, as in
C++: for a node in another allocator's page, `markUnused` and `markUsed` find no page, so
that page's owner, used bit and `MemoryUsage` are unchanged while the node is destroyed,
pushed onto the freeing allocator's free list, popped and re-constructed. Both adapters
call the real operations. The same holds for a caller-owned external node (the oracle
adapter keeps such nodes on the heap) and for page slots that `PAGE()` constructed but
nobody allocated, reached by `neighbour` (real pointer arithmetic inside the page array,
bounded through `findOwningPage`). Two free lists may then name the same slot, and each
allocation constructs over it.

Node steps require a live node, one whose object exists: it was constructed and has not
been destroyed by `Free` since. This is object lifetime, not the page's used bit, which a
foreign `Free` leaves set. The oracle adapter keeps it per node number, together with a
mirror of every allocator's free list as node numbers. Each `state`, `free` and
allocation checks the mirror against the real private free list, except for entries in
destroyed pages, which are dangling and never compared. Allocating from a free-list entry
in a destroyed page is refused (`node is destroyed`), because the reference would
construct into freed memory.

Inputs outside the reference's defined domain are refused before the call (see the
library README). A move assignment or `destroy` destroys the target's pages. Their nodes keep their
numbers but can no longer be named (`node is destroyed`), so a new page at a reused
address is never mistaken for them. A live node elsewhere may retain such a pointer:
keeping, copying (`remove`) and overwriting it are defined, and `snapshot` reports the
slot as `{"destroyed":n}`. The oracle adapter records these slots while the pointers are
still valid and never compares the stale bits afterwards. Nothing dereferences them.

## Corpus and fixtures

`python rust/kicad-rtree-node-shim/generate_cases.py` (fixed seed 20261122; `--check`
verifies the files are current) writes:

* `port/cases/<unit>.jsonl`: 1525 successful cases. The original 1448 (Hilbert corners,
  exhaustive orders 1-3, random orders and coordinates, all dimensions; overlap masks for
  both element types, every fanout and counts 0, 1, half, fanout - 1, fanout,
  fanout + 1 and 31, extremes, touching edges, inverted boxes; for each of the first nine
  configurations a node program, a page/free-list program, a move program and four random
  programs of 400 steps) are unchanged. `review_cases` appends 50 regressions from the
  first independent review: Morton orders above 64 dimensions and at `INT_MIN + 1`; the
  33-slot mask boundary, plus the fixed programs for `i32.d3.m33`; `RemoveChild` at
  `count - 1`; and moves that leave stale child pointers. The legacy random programs still
  avoid such moves, because changing their generator would change the original cases;
  the review cases cover them. `foreign_cases` then appends 14 regressions from the second
  review: the exact `B.Free( A.Allocate() ); B.Allocate()` sequence with ownership, used
  bits, free lists and memory before and after, for three configurations; repeated
  cross-allocator reuse; free-list order and capacity growth; foreign entries across moves
  and destruction; and eight fixed-seed lifecycle programs (seed 5252) in which any
  allocator frees any node. `storage_cases` appends 13 regressions from the third review:
  null `owns`/`find` before and after allocation and after moves; an external node freed,
  re-allocated and freed by another allocator, as a child, and after its storage ends
  (three configurations); the unused-slot alias sequence through two free lists (three
  configurations); and six fixed-seed programs (seed 5353) with external nodes and
  neighbours.
* `fixtures/refusals.requests.jsonl` and `.replies.jsonl`: 128 malformed, out-of-domain and
  undefined-behaviour requests with the exact refusal both adapters return. `cargo test`
  checks this adapter against them; the validation run checks the oracle. The original
  `move-dangling-child` refusal was removed: that sequence is defined and is now the
  successful case family `review-slab-*-move-stale-*`. Likewise `foreign-free` (a foreign
  `Free`, formerly `node is not owned`) is defined and is now the `review2-*` family; its
  genuinely undefined neighbours (a second destruction, use after free, allocating into a
  destroyed page) are refused. `node-null` (a null `owns`, formerly `integer out of range:
  node`) is likewise defined and is now `review3-slab-null-ownership`.
  `fixtures/reclassified.jsonl` keeps every reclassified refusal with its old reply, the
  replacing cases and the rationale.
