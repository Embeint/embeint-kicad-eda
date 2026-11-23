<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# OverlapMask2D SSE2 path compares any 4-byte ELEMTYPE as signed int

- **Location:** `libs/kimath/include/geometry/rtree/rtree_node.h`, `OverlapMask2D`, the
  `#if defined( __SSE2__ )` branch, KiCad pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
- **Trigger:** the branch is selected by `sizeof( ELEMTYPE ) == 4` and uses
  `_mm_cmpgt_epi32`, a signed 32-bit comparison, after `static_cast<int>` of the query.
  For `uint32_t` coordinates at or above 2^31, or for `float` (whose bit patterns are
  loaded as integers), the result differs from the scalar fallback used on other
  platforms. The NEON branch has the same structure.
- **Effect:** latent. Every current instantiation found (`DYNAMIC_RTREE`, `COW_RTREE`,
  `PACKED_RTREE`) uses `ELEMTYPE = int`, for which the SIMD and scalar paths agree.
- **Reproduction:** none run. The port supports only `int` and `int64_t` element types and
  refuses others at compile time (sealed `Coord` trait).
- **Evidence:** source reading only.
- **Confirmation:** suspected, unconfirmed; no oracle instantiation with `uint32_t` or
  `float` was built.
