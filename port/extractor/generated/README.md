# Corrected extraction evidence

The raw, reviewed a7 reconciled and corrected runs remain separate. Historical
results were never relabelled as corrected. The root graph/inventory/report and
`port/units/` now use the corrected semantics. No unit is ported or oracle-green.

The independent matching-libclang21 source review at `c8fed1ad89` verified all
four prior fixes and negative controls, but reported one remaining recipe-capture
P2. Parent resolved it at `1343c3a303` with a real-Ninja capture-to-drift regression.
This is not an independent clean disposition for 1343. The storage-only follow-up
`c586fcccf9` validates deterministic gzip recipe evidence against its unchanged
decompressed-byte hash.

The independent full review of `19035296e1` verified the graph, ownership, layers,
indexed bytes and cache replay, but required two recipe-capture fixes: absolute-only
Ninja outputs were queried by a relative name, and Ninja-generated source-tree
files were treated as leaves. Parent fix `57e085ad04` corrects both. The relations
are now republished by a separately identified post-parse refresh (below), not a
new parse.

The independent review of `d2bdddb904` verified those relations but found that
`capture` did not bind its source root or source bytes (P2). Parent fix
`ce9681f06f` adds that guard. The guarded recapture at `2888cd057f`
republished the same relations; its evidence remains below as history.

## Same-USR endpoint adoption (current artifacts)

The reviewed fix in
[the same-USR finding](../../findings/extractor-same-usr-definition-endpoints.md)
resolves each call and override endpoint to the body its own TU defines. The
root graph, inventory, manifest, report, measurement, drift reports and
`port/units/` now use it. This is a read-only replay of the unchanged V5 parse
cache under the new aggregation, not a new parse or parser qualification.

- **Replay:** aggregation identity `30ca71dc` (final tool tree), parse runtime and
  `extractor_sha256` still V5 `e1548c1f`. All 2,511 records were reused with zero
  parses, the cache bytes were unchanged, and the same four `thirdparty/fmt`
  parse failures remain. The replayed AST graph `75410331` and inventory
  `b320d93f` are kept in `replay-extraction/`; the V5 original AST `40861e9c`
  stays in `corrected-extraction/`.
- **Generator dependencies:** the guarded capture was rerun against the replay
  inventory and applied to the replayed canonical graph. Recipes, catalog, source
  and generator relations equal the retained capture; only the capture's code and
  inventory identities differ. The retained capture is correctly rejected on the
  replay inventory.
- **Graph changes:** functions, wx classification, definition copies and variants,
  aliases, unresolved records, translation units and source accounting are
  unchanged. 1,798 edges (1,355 static, 210 virtual, 233 override) and 11
  overrides were removed and none added; each has an endpoint in one of the 207
  same-USR copy groups. 25 edges into the VRML `SHAPE::~SHAPE` body are gone,
  including the five from `libs.kimath.include.geometry.application.ui.p003`.
  The three VRML `TranslateToSG` callers have no TU-local definition and still
  reach both bodies.
- **Unit records:** only `depends_on`, `soft_depends_on` and `layer` change.
  Geometry p003 drops `plugins.3d.vrml.application.ui.p001` and moves from layer
  13 to 5. Hard dependents of VRML p001 fall from 22 units to the VRML v1, v2,
  generated v2 and x3d units. The records were merged with accepted `rust` at
  `98a12c1123`; every status, claim, note, test and difftest field equals the
  accepted record.

Canonical graph `b2de2a7a`, inventory `7a14bda8`, manifest `2889bdc4` and
units report `dcd77209`. Evidence is in `evidence/same-usr-adoption/` (adoption at
`4738a9c23f`) and its `rust-98a12c11/` subdirectory (checks after the merge).

## Freeze CLI compatibility correction

Independent review of `6f02c95d98` closed the guarded-capture PATH substitution
finding and reproduced the complete retained capture and artifacts. It found a
separate freezing-CLI compatibility defect: resolving discovered executable
symlinks changed their invocation names. Code `b9f5a76b58` makes those paths
absolute while retaining the symlink spelling. The actual CLI regression uses
name-dispatching symlinks, absolute/relative PATH entries and a directory with
spaces. It fails against the reviewed freezer and passes with the correction;
the complete WSL suite passes 37 tests. Windows passes 25 with 12 expected skips.
Evidence is in `evidence/freeze-symlink/`.

The retained artifact runtime is still `c1e8a467da` (published by `6f02c95d98`).
Its Ninja is `/usr/bin/ninja` directly, so the symlink defect did not affect that
capture. Guarded capture and recipe traversal are unchanged. No new full capture,
apply, split, AST extraction or numerical run was performed for this CLI-only
correction; the independently reproduced artifacts keep their original identities.
To reproduce their recorded capture/apply hashes, use that exact runtime commit,
not the later freezer source. Future captures correctly identify their new code.
A new independent review of this bounded correction remains required.

## Verified Ninja recapture (historical)

The independent review of `2888cd057f` verified the guarded artifacts but found
that capture checked the recorded Ninja binary while invoking `ninja` through
PATH. Code `c1e8a467da` binds every catalog, query and command listing to the
manifest's verified absolute executable. Missing or relative recorded paths are
rejected. The real-Ninja regression uses a PATH wrapper that selects a second
graph without the Markdown dependency; the fixed capture never invokes it.
Changing the executable actually invoked after querying is also rejected. The
same focused test fails against the published 2888 implementation, as retained
in `evidence/verified-ninja/logs/old-implementation-control.log.gz`.

A fresh recapture from the original V5 manifest `a2466270`, inventory `b9e6986b`
and canonical graph `146e2cca` used the unchanged `bf7a1689` audit. It invoked
the recorded `/usr/bin/ninja` under the shared lock. All 9,655 source files,
1,612 build files, 178 frozen inputs and recorded tool identities were checked
before and after. Recipes and the 5,709-record catalog are byte-identical to D2;
both dependency maps are unchanged. The only graph changes remain the refresh
record and manifest fingerprint. Every unit file and the original AST parse
identities are unchanged. No new AST parse or full cache replay was performed.

Retained capture runtime code `2b4eca5d`, manifest `58bccc83`, graph `014f0514` and
inventory `41600e43` have their full identities in
`evidence/verified-ninja/checks.json`. That directory retains the capture,
repeated application, canonical regeneration, guard controls, validation and
commands/logs. The D2 and 2888 evidence directories remain byte-identical.
The prior full replay and ownership proofs remain separately identified and are
reused only after input-hash checks. A new independent review of this head is
required; no numerical parity, green unit or phase-zero acceptance is claimed.

## Guarded source recapture (historical 2888)

The guarded `capture` and `apply` ran from the same V5 base: manifest `a2466270`,
original inventory `b9e6986b`, and pre-refresh canonical graph `146e2cca`. Both
steps used the byte-exact D2 pinned input audit
(`evidence/source-guard/base-generation-input-audit.json`, `bf7a1689`, 1,202 inputs)
in place of a rewritten audit. Evidence is in `evidence/source-guard/`.

- **Source attestation:** root `/home/karlvonrichter/kicad-port/extract-source`,
  commit `3cd2c9af`, tracked inventory `da0820c0`, inventory `b9e6986b` and audit
  `bf7a1689`. 9,655 files (tracked C/C++ plus audited generator inputs) have
  snapshot hash `3c0d9651`. All of them, the 1,612 build files, the 178 frozen
  inputs and the tools were verified before and after the real Ninja queries,
  under the shared lock.
- **Unchanged relations:** recaptured recipes `f44049e2` are byte-identical to D2,
  and so are the gzip `f5a91a1c` and the 5,709-record Ninja catalog. Both relation
  maps equal D2. The graph equals the D2 graph apart from the refresh record and
  manifest fingerprint, and the inventory files, report fields and every unit file
  are unchanged.
- **Changed identities:** capture/refresh code `01ad5e63`, capture record,
  manifest `b6fd50ad`, graph `a04dffc7`, inventory `33268efa`. The parse identities
  (`e1548c1f`, `a2466270`, `dff5b79a`, `3d52afc6`) are unchanged.
- **Controls:** 14 controls on the real snapshot and build are rejected. They
  include refreshed inventory, foreign root, stale or missing Markdown identity,
  audit/tracked conflict, foreign pin, unrecorded Ninja source input and target,
  and forged or absent capture identities at apply. The d2 review's reproducer is
  now accepted only when intact (Markdown drift reaches both consumers). It is
  rejected for a foreign root, deleted or stale leaf/output/C++ files, an
  unrecorded source node, a source, tool or frozen-copy change after the query,
  and an added build file. No AST replay was repeated: the D2 replay and canonical
  reproduction proofs are reused after checking every input hash they bind.

The D2 section below and `evidence/generator-refresh/` are retained byte-identical
as historical evidence of the first refresh (`1a7ea4bc` code, graph `8c39a7f4`).

## Generator dependency refresh (D2, historical)

`refresh_generator_dependencies.py` recaptured the dependency map from the
unchanged private build and applied it to the existing parse. The AST,
definitions, calls, coverage, source bytes, unit records and every unit metric
are those of the V5 parse; only generator dependency metadata and the
fingerprints derived from it changed. Evidence is in
`evidence/generator-refresh/`.

- **Parse identities, unchanged:** parser `e1548c1f`, parse manifest `a2466270`,
  parse recipes `dff5b79a`, parse inventory snapshot `3d52afc6`. The graph's
  `extractor_sha256` still names that parser. The V5 manifest and recipes now sit
  in `corrected-extraction/` beside the V5 AST they produced, byte-identical.
- **Refresh identities:** capture/refresh code `1a7ea4bc`, capture record
  `f4126f13`, refreshed manifest `071a18f4`, recaptured recipes `f44049e2`
  (1,640,524,798 raw bytes, deterministic gzip 31,784,116 bytes `f5a91a1c`).
- **Base reproduction:** the frozen V5 runtime replayed all 2,511 V5 cache results
  read-only with zero parses, and its semantics match the V5 graph/inventory.
  Its splitter reproduces the pre-refresh canonical graph `146e2cca`, report and
  all 1,801 unit files byte for byte. The refresh was applied to that graph.
- **Capture checks:** all 1,612 private build files (including the CMake/Ninja
  files) matched the parse manifest before and after the capture. The 178 frozen
  inputs and the tool binaries were checked only before it. The d2 review found
  that the earlier "before and after" wording overstated this; the guarded
  recapture above checks both sides.
  The Ninja catalog has 5,709 target records, 237 alias groups and 359
  absolute-only outputs. 1,435 recipe records were captured. 1,272 of the 1,273 V5
  queries are identical. The other is the build directory node `@build/.`,
  which had no input edges and is no longer an input.
- **Relation changes:** 163 source-tree generator outputs are now published in
  `source_generator_dependencies`, 27 with recipe inputs. Four build-generated
  entries gained `pcbnew/pcb_text_help.md` and `cmake/BuildSteps/markdown2C.cmake`
  through `pcbnew/pcb_text_help_md.h`. All 56 V5 entries dropped only `@build/.`.
  No other input was removed. 1,202 tracked generator/configure inputs (38 beyond
  the V5 audit's 1,164) equal pinned Git blobs.
- **Real drift proof:** in a disposable repository that changes only
  `pcbnew/pcb_text_help.md`, the refreshed graph sends 384 units to review. They
  include the owner of `pcbnew/pcb_text.cpp`, which includes the tracked header
  but no build-generated route. The V5 map sends none. An unrelated Markdown
  change sends none.
- **Limits:** order-only and implicit Ninja inputs are followed conservatively,
  as in V5. The 56 V5 build entries already carried 23,072 object-node
  references. 14 source-tree help/formula headers now do the same, which adds
  library sources to the all-record drift inputs of 55 units without adding or
  removing any reviewed unit. Two CMake-configure/probe outputs have no Ninja rule
  and remain labelled, without guessed edges.

| Refresh artifact | SHA256 |
| --- | --- |
| canonical graph.json.gz (same-USR adoption, current) | `b2de2a7a99cbfedbfb4f714c9939561418a12f873e86731f617afe44e8b7f9a1` |
| canonical inventory.json (same-USR adoption, current) | `7a14bda8bd191801b2832a8b46f2cd4b668da12c9d262e55fba01a0bbb13392f` |
| generated-sources.json (same-USR adoption, current) | `2889bdc4bd68d337e28d67cb82975429cece6eb9bade574abda2d90c697e6797` |
| replayed AST graph.json.gz (same-USR adoption) | `754103312d4602dab3e4388f041bad413b3615d8452477810e262f8383d1be73` |
| replayed inventory.json (same-USR adoption) | `b320d93f3eb2d9f450d40edd2a22dcd850d59565810540dd1476b773822bfd47` |
| canonical graph.json.gz (verified Ninja, historical) | `014f05141168b634cd4ef39a9cea434b696ae2d6d6230048e41af3f31738f342` |
| canonical inventory.json (verified Ninja, historical) | `41600e43a380594f88e7e9747398e01458c1e71e436d59c31ca23f92dfb603ed` |
| generated-sources.json (verified Ninja, historical) | `58bccc83c8a9d18c538d1e0df8d062c307cd0ac8816d96a0b126a74914277760` |
| canonical graph.json.gz (source-guarded 2888, historical) | `a04dffc7da914fa204ca0018a42fe53bfb9dbbeac65c2e279883cf8a6d691ba3` |
| canonical inventory.json (source-guarded 2888, historical) | `33268efa748421dd0bf74e4d89399a19f95c350cfad43b31ad1c5f0f2f75b452` |
| generated-sources.json (source-guarded 2888, historical, LF) | `b6fd50ad295ac1c78077e392fb7ffd26e9e64e2f6ef0ec05c0d95f74baf6a7b7` |
| guarded source snapshot (9,655 files) | `3c0d96516f2c20ca5e7883aa7a549ed0e412a190b27af60012a4edfb0621122e` |
| pinned input audit (D2 bytes, both steps) | `bf7a1689587de011b7f2b8879e7d6284827a451ee43155ad72b8963fe30a6de6` |
| D2 canonical graph.json.gz (historical) | `8c39a7f481663b011fb37404004874b781aa77a0e35397b8f563d08417f498c8` |
| D2 canonical inventory.json (historical) | `9b6f09351a5f7b59ca790c1d80301c69373e3f6a04fc5a28082733171fb4b742` |
| D2 generated-sources.json (historical, LF) | `071a18f4c9ad6d0a6ed39ed84e4cbbc33f984f8bc4896b45f4e9de8b4a132792` |
| generated-recipes.json.gz (D2 and guarded) | `f5a91a1c6f5e2357aeb1db4f6499be3ba7bca7cc3b46fea793c74de42257064e` |
| pre-refresh canonical graph.json.gz | `146e2ccac0e8d0ba6d37ceadb26ae5a84d0223609dd55abebb0b5451bfe45735` |
| units.json (same-USR adoption, current) | `dcd772091f0ae91899f78fa7a5b56b6267c1e24f62415464d4c2b3e88b078cd9` |
| units.json (before the same-USR adoption) | `0aac539054de1437248d5f97c510efee16de779cf546919c136a23938b023753` |

| Run | Recorded commands | Successful parses | Definition entries | Generated-build definitions | Edges |
| --- | ---: | ---: | ---: | ---: | ---: |
| Raw historical | 2,471/2,471 | 2,462 | 68,939 | 0 | 573,759 |
| Reviewed a7 reconciled | 2,511/2,511 | 2,507 | 69,416 | 0 | 559,555 |
| Corrected (V5 original AST) | 2,511/2,511 | 2,507 | 107,254 | 37,838 | 649,723 |
| Corrected (same-USR replay, measured) | 2,511/2,511 (reused) | 2,507 | 107,254 | 37,838 | 647,925 |

| Artifact | SHA256 |
| --- | --- |
| raw database | `10519703c213d66a3abf4e00fa091bdaa616d9865c4ec47ecad0acca91c2fa8b` |
| raw original graph gzip | `a4768bc824e8947f3373dbec4afc939c91ac94ee6ac9ff22355eeedf70a2e516` |
| reconciled database | `bc402ade70f0304ed5018793a5d8c37749df978dfd7859ee87e655ee3a616196` |
| reconciled original graph gzip | `133c45e58c4f42fade25280971cf2f9c67433bbe83efb0051cbeddef24e3967f` |
| corrected database | `bc402ade70f0304ed5018793a5d8c37749df978dfd7859ee87e655ee3a616196` |
| corrected original graph gzip | `40861e9ca678464cedc513561e6e559cbcb5f902bb5b221fe2b6c378fb803e18` |
| pre-refresh canonical graph.json.gz | `146e2ccac0e8d0ba6d37ceadb26ae5a84d0223609dd55abebb0b5451bfe45735` |
| corrected inventory.json (V5 parse) | `b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682` |
| canonical units.json | `0aac539054de1437248d5f97c510efee16de779cf546919c136a23938b023753` |

`raw/` retains the historical graph/inventory and deletion-aware coverage.
`reconciled-extraction/` retains the reviewed original AST, while
`history/reviewed-a7/` preserves its canonical EOF-normalized graph, measurement,
reports and diagnostics. Original unit records remain in Git at a7/77c144.
`corrected-extraction/` retains the new original AST separately from the current
canonical EOF-normalized graph. The interrupted preliminary corrected attempts
are retained as diagnostics in `evidence/corrected/interrupted-attempts.json`;
their runtime/cache/logs remain in the private WSL tree. None is a success claim.

The source pin and original 9,401-file tracked inventory are unchanged: 1,946,361
application/generated lines, 340,033 QA and 1,059,199 third-party lines, separate.
The corrected inventory additionally freezes 178 build
inputs and their exact public bytes. Generated application lines are measured
separately from CMake compiler-ID tooling probes. `generated-sources.json` maps
logical `@build/<relative>` locations to `sources/<relative>`; these are explicitly
not pinned Git paths. The refreshed `generated-recipes.json.gz` (manifest binds its raw-byte
hash), tool identities and the 1,202-file pinned generator/configure-input audit
make the generation provenance reviewable; the V5 1,164-file audit is retained.
CI validates committed source snapshots, with `-text` preserving generated bytes.
Future source manifests exclude port tooling/artifact copies to avoid double counts.

There are **1,801 units / 2,380,947 owned physical lines**,
**377 layers**, **46,709 hard edges** and
**9,496 back-edges**, dropped weight **105,471** (before the same-USR adoption:
392 layers, 46,894 hard edges, 9,548 back-edges, weight 106,538).
282 units exceed the 2,000-line target because functions
and conservative enclosing ranges remain intact. The weighted greedy heuristic
is deterministic, not a minimum feedback-arc solution. wx coupling is not a
literal UI effort estimate or proof of portability.

The corrected parser includes virtual destructors and conversion operators,
unused transitive overrides, explicit destructor/delete dispatch, and visible
local/by-value/base/member cleanup dependencies. Implicit derived destructor
overrides, unexposed deallocation selection and temporary lifetime/elision sites
are diagnosed rather than invented. Exception-unwind, global and union/variant
lifetime paths are not exhaustively lowered. Canonical aliases, nested templates,
pointers/references and recursive record storage now contribute wx coupling;
named wx classes remain atomic policy inputs. Opaque template layouts conservatively union visible fixed primary/partial storage and bases; dependent T alone is never assumed wx. The complete private build-file fingerprint closes added/macro-only include cache invalidation, and every generated include requires a frozen mapping. Ninja capture retains and traverses generated intermediates, including non-C++ stages, to tracked source leaves for drift.

Actual-source proofs in `evidence/corrected/final-artifact-audit.json` verify
EDA_ITEM's virtual destructor and overrides, NETINFO_LIST::NetsByName's wxString
coupling, and owned generated lexer constructors/TokenName definitions.
All 86 active generated-build TUs have frozen logical-source mappings. Validation
rejects the old zero-definition/unmapped-TU case even if parsing succeeds.

All raw diagnostics remain: four proven-deleted/excluded commands (one cached
failure preserved, three excluded without parsing) and five other failures.
Private generated headers resolve the raw protobuf failure. Both reconciled runs
retain the four fmt GCC-module failures, without stripped/invented flags.
Inventory/database processing are complete; parsing/extractor gate and all-
configuration completeness remain false. There are 63
uncovered implementation sources and 64 blocked units.
8,899,609 unresolved TU/configuration records remain explicit.

Canonical range normalization preserves original clang extents and source/body/
input/splitter hashes. Historical failed EOF-generation diagnostics remain in
the evidence tree. Repeat generation matched every unit/report/graph byte, and
all 178 generated inputs stayed unchanged. All raw/reviewed/corrected/canonical
artifact identities and decompressed database/log hashes passed audit.
Full aggregation replay reused all 2,511 exact V5 cache results without new clang
parses and matched graph/inventory semantics after excluding the explicitly
recorded cache-trace audit fields and comparing full failure records in sorted order. All failure file/status/severity/message payloads match exactly. Raw failure reporting order follows job completion and is nondeterministic; both original graphs and the initially failed comparison are preserved. Original traces remain intact. Post-adoption
canonical CLI regeneration also preserved every graph/report/unit byte, including
maintained port metadata. These checks are distinct from source/branch coverage.

See [VALIDATION.md](VALIDATION.md), [PILOTS.md](../PILOTS.md), the migration ledger
and [measurement.json](measurement.json). The pilot scopes must be re-confirmed
against these records; no old ID alone certifies unchanged scope. Stored cases
and the three-live-green-pilot gate are separate from extraction validation.
