# Hosted exact comparison snapshot

Implementation head `5b6d689175a9f80a58814bb094d68b2956184b3e` integrates accepted
`origin/rust` `4b18bdb371bd4e624df1942c110302af0eb9bd51` by merge. Initial base was
`37f4bc0a32`. The actual tested PR merge is recorded in summary.json and provenance.
Run: https://github.com/Embeint/embeint-kicad-eda/actions/runs/45618853377.

| Unit | Cases | Compared exactly | Deferred | Failures |
| --- | ---: | ---: | ---: | ---: |
| util/box headers ui.p001 | 15,905 | 14,298 | 1,607 | 0 |
| vector headers ui.p002 | 10,286 | 9,639 | 647 | 0 |
| geometry headers ui.p001 | 21,538 | 19,102 | 2,436 | 0 |

Every public case ran against actual pinned C++ and Rust, without tolerance,
golden, exclusions or lookup. All reports have zero unprocessed cases, process
faults or extra replies. Deferred records preserve actual C++ results and typed
Rust errors with real owners. All 16 separate undefined-domain controls, a later
valid request and all 15,905 freshly executed native replies in import-free wasm
passed. Format, clippy with warnings denied, tests, wasm build and source/flag
audits also passed in this run.

Caller regression checks retain all 8,860 accepted vector and 17,721 current
accepted geometry comparisons, unlock 139/997 accepted deferrals, and preserve
all accepted input objects. Geometry's accepted backend gains belong to the
integrated base; this PR only adds 384 deterministic geometry cases and wires the
utility wrappers. The original 12,433 passing geometry subset also remains
compared. All three units remain partial.

This snapshot includes the successful box-round/narrowing continuations, promoted
and unsigned-wrapping sqrt helpers, live Merge point aliases and selected-edge
farthest-point evaluation. The earlier d7a17b8e4c snapshot stays historical.
Compressed files contain original full reports. Summary and caller checks are
machine-derived; SHA256SUMS binds every retained file. This evidence qualifies
the named revision, not arbitrary custom template types, constexpr evaluation,
missing callees, assertion effects or unrestricted extreme rotation/SEG/arc
native-oracle inputs. No separate review is claimed.
