<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Dynamic R-tree squared-distance arithmetic can leave its defined domain

- **Location:** `libs/kimath/include/geometry/rtree/dynamic_rtree.h:887` and `:1781`,
  KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`; `forcedReinsert` component
  floating-to-int64 conversions and sums; `minDistSq` subtraction/square/sum.
- **Trigger:** a point at `[INT_MAX,INT_MAX]` and an inserted point box at
  `[INT_MIN,INT_MIN]` make the first nearest squared difference exceed `INT64_MAX`.
  With int64 coordinates the subtraction can overflow before squaring. Large
  reinsertion-center differences can exceed the floating conversion's int64
  domain or overflow its component sum. Node area sums have their separate
  existing owner finding.
- **Effect:** signed arithmetic and out-of-range floating conversions are undefined
  in C++. A small item count does not prevent the fault.
- **Reproduction:** the Rust `arithmetic_is_checked_at_the_evaluated_step...` test
  and the shim's `distance-overflow-*` controls return precise typed refusals. The
  C++ adapter validates the nearest arithmetic using widened intermediates on the
  same evaluated queue path before calling the reference. Bulk conversion has
  analogous `bulk-conversion-*` refusal controls. The undefined operations are
  deliberately not executed to produce an alleged native result. The adapter
  guards only nearest-neighbour arithmetic and bulk conversion: Insert and Remove
  (forced-reinsert conversions and sums, chooseSubtree overlap/enlargement/area
  sums, split perimeter/area products) are unguarded, so those Rust refusals are
  checked on the Rust side only and are not native-compared.
- **Evidence:** source-derived arithmetic proof and checked Rust tests; native
  refusal controls verify the guards. Valid full-domain containing boxes are
  successful native corpus cases, proving there is no blanket coordinate fence.
- **Confirmation:** source-confirmed undefined-input boundary; no KiCad application
  caller reachability or application crash is claimed. This is not a comparison
  exemption. The portable unit remains partial for the documented generic/platform
  and resource boundaries.
