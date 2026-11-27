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

The first review later stopped on a provider content flag; it is not a completed
review or approval. [partial-review.json](partial-review.json) records its
reported findings and the parent test/audit corrections published at6b36ef0440.
Assertion host effects and the adapter safety argument remain unresolved.
A fresh independent review is pending on that corrected head; the target
results above remain historical evidence for48e156721a only. No numerical
comparison or public corpus changed.

The follow-up review was also stopped by the provider content flag before its
verdict. Current head0aa3752c16 additionally retracts the stale intermediate
bound in adapter source comments. No third review retry is launched. Both
interrupted reviews remain partial evidence; neither approves the unresolved
assertion or domain findings. Guard code and numerical behavior remain unchanged.

The user-clarified original review subsequently completed on its second run.
[completed-review.md](completed-review.md) and [completed-review.json](completed-review.json)
supersede the earlier claim that no completed verdict exists. The verdict requests
changes at the original `48e156721a` head, without approving later parent fixes.
It establishes an actual P1 admitted-domain defect: lines `(0,0)->(65536,1)` and
`(0,0)->(1,65536)`, point `(32766,-32768)`, reach tangent point
`(INT_MIN,-32768)` and then `INT_MIN - 1` in PerpendicularSeg. The existing genuine
SEG guard rejects that next operation. The oracle needs a source-derived guard
and a separate control; Rust must remain honestly deferred at the earlier genuine
SEG::Square call. The P2 missing assertion-handler effect also remains.

The midpoint, unsupported remainder coverage and assertion counts were corrected
on later heads, which this verdict did not review. Its raw source and committed
LF report digests are both retained; no report contents were rewritten. The
separate follow-up review remains interrupted. No new parent retry or reviewer
model substitution was used to obtain this completed result. PR #95 remains held.
