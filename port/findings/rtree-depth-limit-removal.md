<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Dynamic rtree: fixed depth limit removed

Unit `libs.kimath.include.geometry.rtree.application.pure.p001`
(`libs/kimath/include/geometry/rtree/dynamic_rtree.h` at KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`). Follow-up to merged #61. It reuses
the approach of the closed duplicate #60 (explicit stacks and an active-node
budget) without merging that branch. The accepted `kicad-rtree-node` crate,
its API, owner, status and evidence are unchanged.

## Trigger

With fanout 3 or 4 the minimum fill (`MAXNODES * 2 / 5`) is 1, so each split
can peel off a single entry and monotone insertion grows the height linearly.
The reference recurses or loops as deep as needed. The port refused a valid
insertion once a path reached 64 levels, after splits had already changed the
tree, leaving it interrupted.

## Guards removed

| Location (pre-change `rust/kicad-rtree`) | Guard | Replacement |
| --- | --- | --- |
| `src/lib.rs` `choose` | `path.len() >= 64` -> `ResourceLimit` | path grows by fallible push; height bounded by the node budget |
| `src/lib.rs` `split`, root branch | `level > 64` -> `ResourceLimit` | removed; `checked_add` on the level remains |
| `src/lib.rs` `remove_impl` | `depth > 64` -> `ResourceLimit` | explicit frame stack, no recursion |
| `src/lib.rs` `search_impl` | `depth > 64` -> `ResourceLimit` | explicit frame stack, no recursion |
| `src/iter.rs` `Cursor::advance` | `stack.len() >= 65` -> `ResourceLimit` | removed; fallible push |

## Recursion and call-path audit

| Path | Before | After | Order argument |
| --- | --- | --- | --- |
| `removeImpl` | recursive per level | frames `(node, next slot)`; success unwinds innermost first | leaf frames scan with the reference data-then-overlap test; an internal frame resumes at the slot after a failed child; on success the parent runs the reference continuation (bounds, parent-root underflow quirk, orphan or empty-child free) for the slot it entered, whose siblings cannot change in the meantime |
| `searchImpl` | recursive per level | frames `(node, remaining mask)` | mask computed on entry from the same immutable node; ascending bits; count before callback; false ends the search |
| `splitNode`/`splitNodeInternal` chain | tail recursion per overflowing ancestor | loop carrying parent and sibling entry | identical step sequence; the internal comparator applies from the second pass, as the recursive call passed it |
| `chooseSubtree(AtLevel)` | loop with capped path | loop | unchanged decisions |
| insert -> forced reinsert -> insert | mutual recursion | unchanged | not height-driven: overflow treatment is reached only for leaves, and the level-0 mask bit sends nested overflow to the split loop, so nesting is at most two insertions; the `level >= 32` arm is the reference's shift guard, not a depth limit |
| orphan reinsertion -> insert / reinsertNode -> split loop | iterative over orphans | unchanged | no recursion into removal |
| `removeAllNodes`, iterators, nearest | explicit stacks or heap | unchanged except the cursor guard | - |
| shim `state` snapshot | recursion bounded by the old guard | recursion bounded by the root-level-39 refusal | the new flat `topology` observation has no recursion |

## Resource budget

`DynamicRtree::new(max_entries, max_nodes)` adds an independent active-node
budget (`Error::NodeBudget`), counted at every allocation and free. It bounds
height and therefore every traversal stack. An empty tree's first-root refusal
changes nothing; a refusal during insertion or orphan reinsertion interrupts
the tree (clear restores it with the same budgets); bulk load computes its
packed node count and refuses before freeing the old tree. The comparison
adapter uses no node budget (`usize::MAX`), because C++ has none and monotone
programs within the 4,096-step limit reach millions of nodes (fanout 3,
ascending 1D: about n^2/4 live nodes).

## Evidence

Tested revision `7d375e39ad830ae72707955adde97ccb2f2ddaf9`; details in
`rust/kicad-rtree-shim/validation/2026-11-26-7d375e39/`.

| Operation | Implemented body / branches | Successful observable checked | Remaining gap |
| --- | --- | --- | --- |
| Insert, choose, split chain | loop propagation, no level cap | native topology at root levels 79 (fanout 3, 160 boxes), 79 (fanout 4, 240 unit strips) and 74 (fanout 4, the 1,200 strips of #60), with checkpoints crossing 64; Rust counters: 6,084-9,165 internal splits, 74-79 root splits, forced non-root reinserts at fanout 4 | other coordinate/data instantiations |
| Remove, fallback, condense | explicit frames | deep fallback misses, deep-end direct removal through 74-79 root condensations to empty and regrowth, topology checkpoints; Rust counters 6,241-9,401 empty-child frees | orphan reinsertion cannot occur at fanout 3/4 (minimum fill 1 makes every underfull child empty); trees that deep at fanout >= 5 are impractical, so orphan continuations are exercised only at shallow depth |
| Search, cursors, ranges, nearest | explicit frames; heap stacks | full and offset iteration, pruned, empty and truncated ranges, visitor stops 1/7/all, nearest ties and k 0/1/3/5/8192 on the deep trees | live cursor across moves (borrowing) |
| Moves, bulk, clear, memory | budgets carried by moves and resets | deep move construct/assign, moved-from reuse, bulk replacement of a deep tree, clear and regrowth; reference memory and reachable node counts compared natively with Rust `active_nodes` | allocation exhaustion |
| Node budget lifecycle | `NodeBudget` | portable tests: zero-budget first root, mid-insert interruption and reset, bulk pre-check preserving the old tree | Rust-only by design; not natively comparable |
| Nested state boundary | refusal above root level 39 in both adapters | state at level 39 compared; level-40 refusal control on both processes | protocol limit, not a tree limit |

Counts: 259 programs / 76,820 steps compared exactly (the original 249 /
67,713 unchanged byte for byte, with Rust replies identical to the pre-change
shim), 0 deferred, 0 failures; 27 refusal controls on both processes; complete
import-free wasm replay; 10 portable behaviour tests and the shim test. Status
stays **partial** for the gaps already recorded in the unit (other
instantiations, allocation exhaustion, live cursor across moves, Rust-only
insert/remove overflow refusals that the C++ adapter does not guard or compare,
unexercised private branches). Confirmation: hosted CI and a separate review
are pending.
