# Corrected extraction validation

Validated code head `d1667ca05c170ce8aacd2060fa41d7764a6c4885` and the generated files committed with
this report. Parser/cache identity is `e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d`; its four
implementation files match the frozen runtime. Later validator/source-manifest/
drift checks do not change AST semantics. Parent wiring fix `77c144493f` remains.

| Check | Result |
| --- | --- |
| Final WSL extractor suite | 34/34 passed, including eight real-libclang integration tests and one real-Ninja capture-to-drift test |
| Windows extractor suite | 25 passed; 9 tool-dependent skips |
| Progress / bootstrap / policy suites | 28 / 9 / 7 passed |
| Merged-base native harness | 13 Rust tests passed, including 2 real C++ protocol fixture tests; fmt and clippy passed |
| Windows and private WSL source-hash/structural validation | Passed with corrected records and frozen generated sources |
| `--require-complete` | Expected failure: four fmt GCC-module commands remain parse failures |
| Pinned solidcore loader | All 1,801 records round-trip with core fields/extensions intact |
| Repeat generation | Graph/report and every unit file byte-identical; 178 generated inputs unchanged |
| Full cached aggregation replay | 2,511 exact V5 cache results reused; zero new clang parses; graph/inventory semantics identical after cache-trace exclusion and exact failure-payload comparison independent of reporting order |
| Post-adoption canonical CLI regeneration | All graph/report/unit bytes identical with maintained metadata retained |
| Source generation provenance | 1,164 generator/configure input files match pinned Git; tool versions/recipes/output bytes retained |
| Artifact audit | Raw/reviewed/corrected/canonical hashes and decompressed database/log hashes verified; blobs below GitHub limit |
| Indexed artifact bytes | 2,172 listed artifacts match their verified bytes, including all 178 frozen sources and 1,801 maintained unit files; `evidence/corrected/index-artifact-audit.json` |
| Actual source regressions | EDA_ITEM virtual destructor/overrides, canonical NETINFO_LIST wx type, generated lexer ownership verified |
| Progress build and current-record policy | Passed; 1,801 records, 0/3 live green L0 pilots, phase 0 INCOMPLETE |
| Golden-change policy | Passed; no new goldens |

Commands and logs are in `evidence/corrected/final-checks.json`; real WSL results
are in `final-extractor-tests-wsl.log` and `final-validate-snapshot.json`.
Default and strict validation, drift and scope artifacts are recorded separately.
Byte-preserved inputs, transcripts and historical snapshots are exempt from
whitespace normalization; authored code/docs remain checked. Frozen protobuf and
CMake output whitespace and captured log endings are retained, with indexed byte
hashes checked after applying the scoped attributes. Build-ignore matches such as
`config.h` were explicitly included in the snapshot.
The current-record policy has no cases/green records, so it invoked no oracle or
Rust runner; this is not live parity evidence. No new C++ application build or
Rust implementation was needed for these extractor changes.

Default drift has zero ported units, proving no parity. All-record drift has
56 changed files, 188 direct findings,
1,715 affected units and 1,715
conservative dependency reviews against the pin. Recipe/CMake inputs are included;
fork changes already present at extraction remain part of this audit queue.

Remaining limits: four vendor module failures; uncompiled/inactive configurations;
unresolved dependent/indirect calls; and the explicitly diagnosed implicit lifetime
paths. Unit wx classes describe coupling and source scope, not portable behaviour.
New exact [pilot scopes](../PILOTS.md) require parent/source-worker confirmation
and a new independent review before any merge or green/parity claim.
