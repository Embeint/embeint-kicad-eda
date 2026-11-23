<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# RTREE_NODE area arithmetic can overflow int64_t

- **Location:** `libs/kimath/include/geometry/rtree/rtree_node.h`, `RTREE_NODE::ChildArea`,
  `ChildOverlapArea`, `ChildEnlargement` and `ChildPerimeter`, KiCad pin
  `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
- **Trigger:** with `ELEMTYPE = int` and two dimensions, a box whose extents both exceed
  about 3.04e9 (for example `[INT_MIN, INT_MAX]` on both axes) has an area above
  `INT64_MAX`; three dimensions overflow at extents above about 2.1e6 (2^21) per axis.
  With `ELEMTYPE = int64_t`, the extent `max - min` itself can overflow.
- **Effect:** signed overflow is undefined behaviour. The compiled oracle would likely wrap,
  giving a negative or meaningless area or enlargement that steers the R*-tree's
  subtree choice.
- **Reproduction:** `rust/kicad-rtree-node-shim/fixtures/refusals.requests.jsonl` cases
  `overflow-area`, `overflow-enlargement`, `overflow-overlap-area`, `overflow-area-3d`,
  `overflow-i64-extent`, `overflow-i64-perimeter` and `overflow-i64-doubled`. Both
  adapters refuse them before (C++) or during (Rust) the arithmetic.
- **Evidence:** reasoning from the source and the checked overflow tests; the undefined
  results were deliberately not executed.
- **Confirmation:** suspected, not confirmed as a reachable fault. Whether KiCad's callers
  can insert boxes this large (for example "infinite" view items) has not been checked.
  This port refuses the inputs; it is not a comparison exemption.
