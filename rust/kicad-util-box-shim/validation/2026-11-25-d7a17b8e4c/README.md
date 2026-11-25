# Hosted exact comparison snapshot

Implementation head `d7a17b8e4c6413ffdca9c44dc781af663949938f`, based on `37f4bc0a32`.
The actual tested merge is in summary.json and oracle-provenance.json.gz.
Run: https://github.com/Embeint/embeint-kicad-eda/actions/runs/45607429865.

| Unit | Cases | Compared exactly | Deferred | Failures |
| --- | ---: | ---: | ---: | ---: |
| util/box headers ui.p001 | 15,828 | 14,221 | 1,607 | 0 |
| vector headers ui.p002 | 10,286 | 9,639 | 647 | 0 |
| geometry headers ui.p001 | 18,434 | 13,814 | 4,620 | 0 |

All reports have zero unprocessed cases, process faults or extra replies. Every
case ran on both actual pinned C++ and Rust; deferred records retain real C++
results and typed Rust errors with owners. No golden or tolerance was used.
The run also passed all 11 separate undefined-domain controls, a later valid
request, and all 15,828 freshly executed native replies in import-free wasm.
The caller check retains all 8,860 accepted vector and 12,433 geometry comparisons,
unlocks 139/997 accepted deferrals, and verifies unchanged accepted input objects.
The scoped workflow passed format, clippy with warnings denied, tests, wasm build,
source/flag audits and oracle controls. All three units remain partial.

Compressed files are the original full reports, not edited outcomes. The summary
and caller report are machine-derived; SHA256SUMS binds every retained file.
This snapshot qualifies its named revision. PR CI revalidates later closeout-only
changes; it does not make unqualified template types, absent callees, native
extreme rotation inputs or assertion-side effects into passing evidence.
