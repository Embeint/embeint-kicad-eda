# Corrected extraction validation

## Generator dependency refresh

Validated code head `57e085ad043d5aee55de7bccd51b411d6c8526fc` plus the refresh command, tests and
generated files committed with this section. The AST was produced by parser
`e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d` (frozen V5 runtime). Its
`clang_graph.py`/`model.py` are unchanged at this head, while `generated_sources.py` changed
only recipe capture. The refresh code identity `1a7ea4bc` is recorded separately.
Commands, logs and scripts are in `evidence/generator-refresh/`.

| Check | Result |
| --- | --- |
| V5 read-only replay (frozen runtime, manifest, cache) | 2,511 cache results reused, 0 parses, no cache artifact changed; graph/inventory semantics equal (`v5-replay.json`) |
| Pre-refresh canonical reproduction | V5 splitter reproduces graph `146e2cca`, report `0aac5390` and all 1,801 unit files byte for byte (`canonical-from-v5.json`) |
| Capture preconditions | 1,612 build files, 178 frozen inputs, tools and CMake/Ninja files equal the parse manifest before and after |
| Refresh invariants | Only declared provenance fields and build-generated `generator_inputs` differ; repeated application byte-identical in WSL and in a Windows CLI replay |
| Canonical regeneration | Two WSL and two Windows split runs keep graph, inventory, report and every unit byte |
| Validation (WSL and Windows) | Default passes; `--require-complete` fails only on the four fmt module parses |
| Extractor suites | WSL 36/36 with no skips (8 real-libclang, 3 real-Ninja); Windows 25 passed, 11 tool-dependent skips (8 libclang, 3 Ninja) |
| Progress / bootstrap / policy suites, progress build, golden policy | 28 / 9 / 7 passed; build and golden policy pass |
| Pinned solidcore loader | All 1,801 records round-trip |
| Reviewed reproducers | Absolute-only stage and source-tree output fixtures now reach the consumer; reverse-output and unrelated controls stay clear (`reviewed-reproducers.json`) |
| Real relation | `pcbnew/pcb_text_help_md.h` -> `pcb_text_help.md` + `markdown2C.cmake`; Markdown-only change reviews 384 units, including the direct consumer `pcbnew/pcb_text.cpp`; the V5 map reviews none (`relations.json`) |
| Generator/configure inputs | 1,202 tracked inputs equal pinned Git blobs (`generation-input-audit.json`) |
| Measurement | Identical apart from the canonical graph/inventory fingerprints; all protected unit metrics and pilots unchanged (`checks.json`) |
| Indexed artifact bytes | 2,270 staged blobs verified: 465 byte-preserved (all 178 frozen sources, history, evidence, published JSON/gzip) and 1,805 LF-normalized text files (1,801 units, top-level logs); `evidence/generator-refresh/index-artifact-audit.json` |
| Blob limit | Largest generated blob 69,906,624 bytes (refreshed graph), below 100 MB |

All-record drift against target `57e085ad04` keeps 56 changed files, 188 direct
findings and 1,715 affected/dependency-review units. 55 of those reviews list
additional library sources, reached through order-only Ninja inputs
(`drift-comparison.json`). A committed drift report names the commit it compared,
which is the parent of the commit publishing it.

The V5 publication checks below were made at `d1667ca05c` and remain historical
evidence for the parse; their `evidence/corrected/` files are unchanged.

## V5 publication

Validated code head `d1667ca05c170ce8aacd2060fa41d7764a6c4885` and the generated files committed with
that report. Parser/cache identity is `e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d`; its four
implementation files matched the frozen runtime at that head. Later validator/source-manifest/
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
