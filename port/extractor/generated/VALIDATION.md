# Corrected extraction validation

## Freeze CLI compatibility correction (latest code)

Code `b9f5a76b5820945e8e06101718f380ff648b1422` preserves the discovered
tool symlink's invocation name while making its path absolute. The actual
freezer succeeds with name-dispatching symlinks under absolute and relative PATH
entries, including a directory with spaces. The same regression fails against
`6f02c95d98` with the expected dispatcher refusal. WSL passes all 37 tests; Windows
passes 25 with 12 expected tool-dependent skips. See `evidence/freeze-symlink/`.

The previous capture/artifact review independently reproduced and qualified the
records below and closed the PATH-substitution P2. The freezing-CLI change does
not alter guarded capture, traversal or those artifact bytes. No full capture,
apply/split, AST replay or pilot qualification was repeated. Their recorded source
runtime stays `c1e8a467da`; replaying that evidence requires the recorded runtime,
not substituting the later CLI source and changing its code-identity fields.
Every indexed artifact and unit record remains unchanged. Fresh independent
review of the bounded compatibility correction and all parent/gate holds remain.

## Verified Ninja recapture (retained current artifacts)

Validated clean code `c1e8a467da149eac67341d31de0228e498916ba3`. Evidence:
`evidence/verified-ninja/`. The AST parser and original V5 identities are unchanged.

| Check | Result |
| --- | --- |
| Executable binding | Every catalog/query/command invokes the manifest's verified absolute Ninja; missing/relative records refused |
| Regression controls | PATH wrapper selects a second real graph and is never invoked by guarded capture; invoked-tool mutation after querying refused; published 2888 implementation fails the new focused regression |
| Fresh real capture | Original V5 inputs, unchanged pinned audit; 9,655 source files, 1,612 build files, 178 frozen inputs and tools checked before/after under the shared lock |
| Dependency equivalence | Raw recipes and catalog byte-identical to D2; both relation maps unchanged; graph and inventory differ only in refresh metadata and derived identities |
| Application/regeneration | Two WSL applications and two canonical splits byte-identical; Windows pure apply and two canonical splits also preserve every byte |
| Extractor suites | WSL 36 passed, no skips; Windows 25 passed and 11 expected libclang/Ninja skips |
| Source validation | Default passes on WSL and Windows; strict fails only on the four known fmt module parses |
| Records/measurement | All 1,801 units unchanged and round-trip through pinned solidcore; only graph/inventory measurement hashes change; all pilot scopes/GetReader owner preserved |
| Prior guards | All 14 actual-snapshot and 10 synthetic invalid-source controls rejected; valid Markdown drift still reaches consumers |
| Maintained checks | Progress/bootstrap/policy 28/9/7, progress build and committed golden-change policy pass |

No full AST parse/replay or pilot numerical validation was performed. V5/D2
replay and ownership evidence is reused after checking its bound inputs. Historical
V5, D2 and source-guard evidence remains unchanged. Fresh independent review,
parent acceptance and the three-pilot gate remain outstanding.

## Guarded source recapture (historical 2888)

Validated code head `ce9681f06faa189eadb41d8475ef03ea784e4e2b` and the generated files committed with
this section. AST parser `e1548c1f` and the V5 parse identities are unchanged; the
capture/refresh code identity is `01ad5e63`. Commands, logs and scripts are in
`evidence/source-guard/`. The D2 workflow root and `evidence/generator-refresh/`
are unchanged.

| Check | Result |
| --- | --- |
| Base inputs | V5 manifest `a2466270`, original inventory `b9e6986b`, pre-refresh graph `146e2cca`, D2 pinned audit `bf7a1689` (byte-exact copy, used by capture and apply) |
| Guarded capture (real Ninja, shared lock) | 9,655 source files (snapshot `3c0d9651`), 1,612 build files, 178 frozen inputs and the tools verified before and after; recipes `f44049e2` and catalog byte-identical to D2 |
| Relation and record equivalence | Both maps equal D2. The graph differs only in the refresh record and manifest fingerprint, the inventory files are equal, and all 1,801 unit files are byte-identical (`d2-equivalence.json`) |
| Pure apply | Repeated WSL application byte-identical; Windows CLI replay from the `19035296` blobs reproduces graph, inventory, manifest and report without the WSL source path |
| Canonical regeneration | Two WSL and two Windows split runs keep graph, inventory, report and every unit byte |
| Validation (WSL and Windows) | Default passes; `--require-complete` fails only on the four fmt module parses |
| Real-data controls | 14 of 14 rejected: refreshed inventory, foreign root, stale/missing Markdown identity, audit/tracked conflict, foreign pin, unrecorded Ninja source input and target, forged/absent capture identity or different audit at apply, refreshed graph (`actual-controls.json`) |
| d2 reproducer, guarded API | Intact source accepted; Markdown drift reaches direct and build consumers. 10 of 10 rejected: foreign root, deleted/stale Markdown, header and C++, unrecorded source node, source/tool/frozen-copy change after the query, added build file (`repro-controls.json`) |
| Extractor suites | WSL 36/36, no skips; Windows 25 passed, 11 tool-dependent skips (8 libclang, 3 Ninja) |
| Progress / bootstrap / policy, progress build, golden policy | 28 / 9 / 7 passed; build and golden policy pass |
| Pinned solidcore loader, measurement, pilots | All 1,801 records round-trip; measurement differs from D2 only in the canonical graph/inventory fingerprints; pilot and GetReader records unchanged (`checks.json`) |
| Drift | Default and all-record reports equal D2 apart from the target commit (`drift-comparison.json`) |
| Indexed artifact bytes | 2,311 staged blobs: 506 byte-preserved (all 178 frozen sources, history, D2 and guarded evidence, published JSON/gzip) and 1,805 LF-normalized text files; `evidence/source-guard/index-artifact-audit.json` |
| Blob limit | Largest generated blob 69,906,936 bytes (guarded graph), below 100 MB |

No AST replay was repeated. The D2 V5 replay (`v5-replay.json`) and canonical
reproduction (`canonical-from-v5.json`) are reused after checking every input hash
they bind (`code-and-input-identities.json`).

## Generator dependency refresh (D2, historical)

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
| Capture preconditions | 1,612 build files (including CMake/Ninja files) equal the parse manifest before and after; 178 frozen inputs and tools checked before only (earlier wording overstated this, as the d2 review found) |
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
