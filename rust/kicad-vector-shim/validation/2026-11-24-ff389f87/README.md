# Live unique-NaN vector comparison

[Hosted run 45390599190](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45390599190)
passed the complete Rust port job for head
`ff389f87e17ba9d85c51024aef0af1fd43bb218e`, Actions merge/overlay revision
`35d2e874c2d345707fd1abc9ea1e48c218b3d591`.

The audited real pinned C++ oracle returned a result for every one of 9,186 valid cases.
Rust compared **8,513 exactly**, deferred **673**, with **zero failures, unprocessed cases,
process faults or extra replies**. No golden was checked or blessed. The native CTest passed
29 UB controls and 12 malformed-input controls; following requests survived every UB refusal.
Workspace format/lint/tests, portable wasm builds and existing live units passed in this run.
This is native execution evidence, not vector wasm runtime evidence.

Reference: upstream `4cf939778df04d2b747bb2c4714fc5014a7565c7`, Ubuntu 24.04 x86_64,
GCC 13.3.0, glibc 2.39, wxWidgets 3.2.4, Rust 1.99.1,
`-fno-fast-math -ffp-contract=off`, native oracle `-j2`. Full source/compile/link provenance
and the complete untouched JSON report are retained as gzip-compressed files. Decompress
with `gzip -dc difftest-report.json.gz`; compression changes no JSON bytes. SHA256SUMS
binds the compressed artifacts and the concise report summary.

| Remaining deferral group | Count |
| --- | ---: |
| Arithmetic sites with distinct quieted NaNs | 428 |
| General double EuclideanNorm std::hypot | 112 |
| Header-owned KiROUND / equals / RoundedHypot / rescale<double> / sign | 124 |
| Source-owned rescale<int64_t> | 6 |
| Double default stream formatting | 3 |

The expanded scalar, alias and 3D cases confirm the unambiguous arithmetic paths; two distinct
quieted NaNs still do not acquire a default operand preference. The code reads no recorded
answers. Current work after this head adds direct public-entry calls, assignment reference
observations, cast boundaries and accepted-base integration. Those changes require their own
live result; this report does not certify them.
