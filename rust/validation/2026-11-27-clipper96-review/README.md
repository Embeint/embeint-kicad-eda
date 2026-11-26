<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Clipper engine/offset review corrections and manual target evidence

Sol 6.1 reviewed [draft PR #96](https://github.com/Embeint/embeint-kicad-eda/pull/96)
at `6234371e6f9f2eb9ef7ba743eb872287ded1e4b7` and requested changes. The review
covered all 63 canonical owned symbols. Its callback finding remains unresolved:
Rust cannot report callback failure through its current types, and caught native
unwinding loses moved-out groups and the removed Z callback. Review reproduced
C++ retaining one group/one recovery path versus Rust zero, and two C++ Z calls
versus one Rust call. These are source-contract omissions, requiring an additive
fallible interface and retention/lifecycle checks.

Parent code commit `fba034bbf4f5014268c8ccdba09044b8e191fcc5` corrects the three
bounded findings: empty open-path oracle guards, the actual non-Round singleton
ceil-to-int branch, and host-libm-dependent corpus generation. Guards preserve
insignificant offsets and valid larger-delta operations, and follow duplicate
stripping, group reversal, default-delta mutation and actual callback values.
The only changed existing input is `big-star`; 14 valid cases and 21 refusal
controls are added. No reference answer, comparison policy or golden changed.

A fresh dedicated build from raw Git blobs on the locked GCC 13.3/glibc 2.39/
wx 3.2.4/libstdc++ 14.2 runtime passed:

| Unit | Cases | Exact | Deferred | Failures | Controls with continuations |
| --- | ---: | ---: | ---: | ---: | ---: |
| src p002 | 1,571 | 1,551 | 20 | 0 | 44 |
| src p001 | 907 | 901 | 6 | 0 | 49 |
| include p001 | 1,587 | 1,587 | 0 | 0 | 83 |
| src p003 rectclip | 1,574 | 1,574 | 0 | 0 | 11 |

All 15 oracle CTests, focused Rust tests, fmt, clippy with warnings denied,
portable checking and import-free wasm replay pass. Generator checking passes
on Windows and reference Linux, and unit/source-accounting validation passes.
The wasm replay checks native Rust replies and refusal controls; it does not
claim C++ results for genuine include-p002 deferrals. Source export verified
4,085 files, 918 pinned files and 102 compiled overlay files. The dedicated build
used the host lock and -j8; historical evidence and other runtime trees were
preserved. [binding.json](binding.json) binds the tested code tree, compiler,
reference manifest and binary hash. [summary.json](summary.json) includes report
hashes and check results; [corpus-change.json](corpus-change.json) binds input
changes; [runtime.txt](runtime.txt) records the runtime.

Published head `5b145ba4d1f3c8e804031613c5df1a01345321fb` adds only documentation
and an adapter header-comment clarification after the tested code commit. This
manual target evidence does not set a hosted check status or provide independent
approval of the parent corrections. The unit stays draft/partial without a golden.
After callback corrections it still needs fresh independent review and final-head
checks. Original line coverage was not remeasured; other documented source/domain
and evidence limits remain. Private full reports, build/source manifests, logs
and raw exports are retained for the next independent review.
