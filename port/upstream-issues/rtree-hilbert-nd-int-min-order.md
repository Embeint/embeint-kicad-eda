<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HilbertND2D overflows its loop initializer for INT_MIN orders

- **Location:** `libs/kimath/include/geometry/rtree/rtree_node.h:117`, the Morton fallback
  of `KIRTREE::HilbertND2D<NUMDIMS>` for `NUMDIMS != 2`, KiCad pin
  `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
- **Trigger:** `aOrder == INT_MIN`. Then `bits = std::min( aOrder, 64 / NUMDIMS )` is
  `INT_MIN`, and the loop initializer `int bit = bits - 1` overflows `int` before the
  condition is tested. Every other non-positive order is defined and returns 0.
- **Effect:** signed overflow is undefined behaviour. An ordinary optimized build
  returned 0, which hides it.
- **Reproduction:** the independent review of PR 52 built the pinned template with UBSan
  (`HilbertND2D<3>( INT_MIN, {0,0,0} )`). It reported `rtree_node.h:117:18: runtime
  error: signed integer overflow: -2147483648 - 1 cannot be represented in type 'int'`.
  The refusal fixtures `review-nd-d1-order-min`, `review-nd-d3-order-min`,
  `review-nd-d64-order-min` and `review-nd-d65-order-min` now refuse it on both sides,
  without calling the reference.
- **Evidence:** source reading and that UBSan probe. The undefined call is not used for
  any golden.
- **Confirmation:** confirmed as undefined behaviour for that input. KiCad's own callers
  pass order 32, so it is not reachable from them.
