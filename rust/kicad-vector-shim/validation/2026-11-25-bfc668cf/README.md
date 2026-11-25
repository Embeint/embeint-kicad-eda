# Live vector comparison after numeric utility integration

[Hosted run 45569917252](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45569917252)
passed the Rust port job for implementation head
`bfc668cfd61fd9016a791949302e361f6cc802d7`, Actions merge/overlay revision
`a41899c5485787d53a6ec0f9efe55871c3f42e33`, based on accepted `rust@e27917baae`.
Both previously unpublished commits `069a4203b9` and `7e7841bd52` are preserved.

The real pinned C++ oracle returned a result for every one of **9,582 valid cases**.
Rust compared **8,860 exactly**, deferred **722**, with **zero failures, unprocessed cases,
process faults or extra replies**. All ten oracle CTests passed, including the vector test
with 30 UB refusals, 30 surviving subsequent requests and 12 malformed-input controls.
Workspace format/lint/tests, portable wasm builds and the strict phase-zero gate passed
(3/3 green layer-zero pilots, 1,801 valid unit records). This unit stays **partial** with
no golden; compilation does not establish vector wasm runtime parity.

Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`, Ubuntu 24.04 x86_64,
GCC 13.3.0, glibc 2.39, wxWidgets 3.2.4, Rust 1.99.1, no fast-math, FP contraction off.
The complete untouched report and provenance are gzip-compressed; SHA256SUMS binds those
bytes and summary.json. This historical snapshot certifies the stated implementation head;
PR #58's current checks are the authority for subsequent documentation commits.

| Deferred group | Count | Owner |
| --- | ---: | --- |
| Distinct quieted arithmetic NaNs | 428 | vector unit `libs.kimath.include.math.application.ui.p002` |
| General double norm std::hypot | 112 | vector unit |
| Double default stream formatting | 3 | vector unit |
| KiROUND<double, int> | 143 | header unit `libs.kimath.include.math.application.ui.p001` |
| RoundedHypot(int64_t, int64_t) | 10 | header unit |
| Scalar equals<double> | 21 | header unit |
| rescale<double> | 3 | header unit |
| sign<T> | 2 | header unit |

The accepted source-unit `rescale<int64_t>` is now called by integral Resize, including
its `__int128` intermediate and the following sqrt calls. Six old source-owned deferrals
now reach header-owned KiROUND. The 49 new valid Resize cases also reach KiROUND; this
explains why wiring the dependency does not remove all downstream deferrals. Direct tests
check intermediate lengths and an int128-only product before that still-missing helper.
RoundedHypotWide is public in the accepted source unit, but vectors call it only through
the missing header-owned RoundedHypot dispatcher. No header helpers were duplicated.

Fresh local macOS ARM64, Rust 1.96.1, one rust/target, -j4 validation on the same head:
89 scoped math/vector-shim/hash tests passed, two existing reference-only tests ignored;
fmt, all-target clippy -D warnings, full locked metadata, portable kicad-math wasm build,
unit-record validation, deterministic corpus regeneration and input controls passed.
The Rust-only corpus produced 8,860 results and 722 deferrals, matching this native run's
classification. Mixed-type native qualification, other template instantiations, custom
stream state and vector wasm runtime parity remain unqualified; see TYPE_SUPPORT.md.
