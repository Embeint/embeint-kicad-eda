<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Circle hosted reference evidence

PR #95 head `48e156721a92296c99b4b9e8222a73855fadc30b`, tree
`3fb9617db557e7b6d460c5e16d622a290c74a443`, merges only accepted rust
`70888b572202033a6522f2f271b10cc908f74dfe`. The parent verified the hosted
merge's parents and exact tree against this source head and hashed its binary,
provenance, compile records, source manifest and full log.

[Scoped hosted run 45893397767](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45893397767)
passes on GCC 13.3/glibc 2.39/wxWidgets 3.2.4:

- 4,978 public cases: 4,616 exact comparisons, 362 explicit deferrals, zero
  failures/unprocessed. These are target results, separate from the worker's
  earlier diagnostic GCC 15/glibc 2.43 evidence.
- 81 separate controls: 77 matching refusals; four oracle refusals occur after
  Rust has already deferred at RotatePoint. They are recorded as such.
- All 5,059 public/control requests agree between import-free wasm and freshly
  executed native Rust. This is not raw C++ parity on deferred paths.
- The global hosted rust-port check and other applicable live checks also pass.
  The queued isolated preflight 45893393494 is cancelled and replaced with
  exact-head hosted fallback 45896654915, pending at this recording.

A fresh independent Sol 6.1 review (`rust-review-circle95-sol61-20261127`) is
pending on this exact head. Draft/partial status and absence of a golden remain.
ConstructFromTanTanPt has no successful Rust oracle case because its first
ApproxParallel call reaches the genuine missing SEG::Square; test-only seam
callees do not qualify those bodies. Nonempty circle intersection reaches the
missing RotatePoint. The tan-tan adapter's sufficient-domain bound is a harness
limitation, not a claim that other defined C++ calls are invalid.

[verification.json](verification.json) retains exact source/merge/runtime/binary
bindings, comparison counts/hashes, separate control classification, wasm scope
and reference log evidence. No readiness verdict follows from CI without review.
