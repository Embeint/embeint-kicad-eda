<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MD5 local validation, 2026-11-22

Latest reserved-key envelope integration: runtime source
`65d5b0a9732551db35a9348d69d7b2ccfa86a1fb` stacks reviewed PR #48 (exact head
`1ec85a446741c9903963a7282eedddeea06e34e2`, since merged into `rust` by rebase
as `1a2fb924d539d63a68f5696191907dc2aa6d6370` with an identical tree) and closes
the final MD5 review finding. Each real adapter passes 194 complete refusal envelopes and five
complete successes; the previous 154 checks are an unchanged byte prefix. The
registered 5,973-case live check, 1,801-record source validation, oracle CTests
and affected Rust tests pass. See
[the reserved-key proof](validation/2026-11-22-65d5b0a9/README.md). The #39
graph acceptance hold and provisional local green status are unchanged.

Prior corrected graph/nesting integration: runtime source
`a5ed3948399da32b64f7fad93e0394c64615f8ff` passes the registered 5,973-case
live check, 1,801-record source validation, four CTests, 24 affected Rust tests
and 153 complete refusal envelopes plus signed-zero success per real adapter.
See [the additive integration proof](validation/2026-11-22-a5ed3948/README.md).
Protected MD5 scope and historical corpus/goldens/evidence are unchanged.
Parent acceptance of final independent #39 artifact review remains pending;
local green metadata is provisional and does not establish any phase gate.

Prior native parser adoption: source `8259981dde8c9632c3b1194e8cc9362fe87a45b9`
passes all 5,973 existing live cases and 78 complete refusal envelopes plus
integer signed-zero success per real adapter. The reviewed shared parser is now
merged through PR #45; no portable body, corpus, golden or prior evidence changed.
See [the new validation entry](validation/2026-11-22-8259981d/README.md) for actual
commands/provenance and the preserved corrected #39 graph confirmation hold.

All local gates below ran on **code revision
`a295db8bcadb8de7ae084178c2e1705fcf99d762`**. The subsequent baseline/evidence/docs
commit does not change the tested implementation. These are the original
baseline/checkpoint results; the later registration integration is described
below. They do not assert hosted CI or independent review completion.

The local canonical finalizer record confirms
`libs.kimath.src.md5_hash.application.pure.p001`: all 15 definitions in
`libs/kimath/src/md5_hash.cpp:1-289`, 289 physical lines, L0, pure, no hard/soft
dependencies and no coverage gaps. Its raw record SHA-256 is
`71c89c90c9df44c2ba2abb81a367c650c951fd058b1f3e7dd4e788b04c3120ee`.
The fork's MD5 source and supporting header are identical to pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7` (Git diff empty).

## Results

| Actual gate | Result |
| --- | --- |
| Own-tree pinned library build and audit | 59 translation units audited; 822 reference files verified; actual kimath linked |
| Oracle CTest | 3/3 passed: protocol, build guards, dispatch |
| Workspace fmt | passed, `--all --check` |
| Full workspace build | passed, locked, all workspace packages, `-j8` |
| Workspace clippy | passed, locked, all targets, warnings denied, `-j8` |
| Workspace tests | 95 passed, zero failed/ignored; includes 3 wrapper and 3 shim tests |
| Portable wasm builds | embeint-layout, embeint-web and kicad-md5 passed |
| Evidence/CI policy regression tests | 7/7 passed |
| Deterministic corpus regeneration | byte-identical, 5,972 requests |
| Live `compare`, `propose`, `bless`, `check` | each 5,972/5,972; zero mismatches, errors or deferrals |
| Stored baseline versus actual oracle output | byte-identical complete JSONL |
| Committed golden policy against intended PR base | passed at baseline commit `79a9bd58c798ef87912d7caa31b8d58b03c6b0d0`, against `0ab6a0e8635421998a6471d1001fbf7b44c81e71` |

The raw policy-test log contains synthetic fixture messages including “Phase 0:
COMPLETE”; those test fixtures do not establish completion of this real port.
No real phase-zero or all-configuration gate is claimed.

Direct full-corpus subprocess measurements, including the required streamed
512 MiB carry request, were **1.6674611479975283 seconds for C++** and
**13.795774920959957 seconds for debug Rust**; both exited zero. The runner's
unchanged **60 seconds per program** therefore sufficed. All four difftest
actions subsequently ran with that default timeout. No cases were dropped,
sampled or weakened and no timing/profile setting was changed.

## Actual effects and limitations

The run observed 67,617 public state snapshots (16,363 valid, 51,254 invalid)
and 9,631 real equality/inequality pairs (9,627 equal, four unequal). Every
snapshot compares compact, explicitly spaced and omitted-default `Format`
calls, including trailing spaces and the preserved B..G nibble alphabet.

The requests include 10,727 constructions, 4,871 copies, 4,845 distinct-object
assignments, 30,671 byte operations, 120 integer operations, 15,912 finalizations,
236 validity mutations, 234 resets and one explicit inspection. A repeated byte
operation invokes the real reference once per repetition. Cases cover all
selected two-way splits, every short composition, both padding branches,
full/multiblock transitions, copies of partial/finalized contexts, repeated
finalization, hashing afterward and both low-word counter carry paths.
[AUDIT.md](AUDIT.md) maps all 15 owned definitions and supporting inline accessors.

All data is public/generated. Every C++ state is actually constructed and fully
initialized. Private context bytes are tested through continuations, rather than
layout injection. Reference self-assignment is refused for its overlapping
`memcpy` precondition; distinct-object assignment is fully implemented. A full
64-bit bit-count wrap is implemented with unsigned wrapping arithmetic but is
not practically streamed. These are explicit limits, not hidden deferrals.

## Exact provenance and baseline

Host: WSL Ubuntu x86_64; Rust 1.99.1, GCC 15.2.0
(`g++ (Ubuntu 15.2.0-16ubuntu1) 15.2.0`), wxWidgets 3.2.9. The dedicated source,
build, Cargo targets and runtime files are under
`/home/karlvonrichter/kicad-port/pilot-md5`. Each build/runtime phase acquired the
real shared mkdir lock and released only its own lock; compiles used at most
eight jobs. No global oracle tree, other worker tree or Windows target was used.

The oracle's actual `--provenance` records the exact compiler, source manifest,
overlay hashes, configuration hash and every build/compile/link command. It
reports `overlay_dirty_at_export=false`, tested overlay revision `a295db8b…`,
`-fno-fast-math` and `-ffp-contract=off`. The unchanged audit and guard checks ran.

| Identity | SHA-256 |
| --- | --- |
| Case JSONL | `c0e63d835781b88cda40135b1b9d5c83a420964c885afeb65c6fc06c94021715` |
| Oracle executable | `52d3fd4859fcc05a979eb3a115c435524f938758d8c19e79da84acd85b5d8608` |
| Rust shim executable | `c0257fb2a59666810b3ebda64ce7665f97156a512ee7477b512cce0b2eea0c78` |
| Oracle output / baseline JSONL | `c897427df65bb0c0ac199fc1bb8d89519382da0476119e01cc49094e1aa15132` |
| Rust output JSONL | `da0d39f85f9a8a8e20a4eb13cf6756448fb6c0b878275a5b642ad15442e7f30f` |

Output byte hashes differ because object key ordering differs; the existing exact
comparator permits JSON object-key order and compares every value, numeric token,
array order and case identity. It reported exact agreement on every request.

The additional full workspace binary build also passed. Cargo's workspace
feature unification produced a different shim executable identity; rebuilding the
selected `kicad-md5-shim` and `kicad-difftest` packages restored the exact blessed
shim hash above. Copies of the blessed oracle, shim and harness are retained under
the private WSL `runners/` directory. No baseline/evidence file was changed.

The new per-unit baseline is in `port/goldens/`. Its new immutable
`port/goldens/evidence/libs.kimath.src.md5_hash.application.pure.p001-baseline.json`
records all 5,972 case IDs, null old values/indexes, complete new replies/indexes,
both executable identities, actual oracle provenance, case/golden hashes and a
nonempty reason. `propose` first reran live comparison; all proposed additions
were checked against actual reference replies before filling the reason. `bless`
reran comparison/provenance checks and durably wrote evidence before the golden.
`check` then reran both programs and the baseline at the default budget. No
visual result is involved, so no image evidence is applicable.

Raw logs, commands, measurements, summaries, actual provenance and all four
comparison reports are retained in [validation/2026-11-22-a295db8b](validation/2026-11-22-a295db8b).
The original full stdout files and resumable build caches remain in the dedicated
WSL tree. CI URLs: none yet; CI awaits final committed scope and registration.

## Canonical dependency and unit registration

Parent explicitly confirmed PR #39's published head
`a7ec77beffd3b2e5d60409423fa28b6c116c6e00` and the identical canonical MD5 record.
Merge commit `6737e3edf20c25ed7f9cfb524fb5ec9b73f243b0` integrates that exact
dependency. Only the owned unit's permitted port metadata and native difftest
registration change; all 15 USRs, source paths/ranges, physical lines, layer,
classification, gaps and graph dependencies remain byte-for-byte equivalent as
parsed TOML. No other unit or workflow is changed by the pilot.

The validator requires a qualified lowercase module path. The implementation now
lives in the actual `kicad_md5::md5_hash` module, with unchanged crate-root public
re-exports. This is a namespace-only source move; the original compression,
buffering, finalization and wrapper bodies are unchanged. The original baseline
and immutable evidence remain intact. Integration checks rerun the relevant Rust
gates, full unit/source validator and registered live oracle/Rust/golden path;
their separate report records the new tested source revision.

Integration source revision `352773eb4b7f00d554140c5ccdb90543a1d547bb` passed
fmt, warnings-denied clippy, all six relevant Rust tests and portable wasm.
Both Windows and the private WSL copy passed full unit/source validation. The
registered live path checked all 5,972 cases with zero failures/deferrals and
reported **1,570 validated records, 1/3 live matched L0 pilots, phase zero
INCOMPLETE**. Its actual reports are retained in
[validation/2026-11-22-352773eb](validation/2026-11-22-352773eb).
The later evidence/claim-timestamp update changes no implementation, cases,
goldens or accounting fields.

The unit's local `green` metadata describes complete owned implementation and
zero local comparison failures/deferrals. The PR remains draft, dependent on
#39 and pending independent review and hosted CI. `PHASE0_REQUIRED=false` remains
unchanged; one pilot does not satisfy the three-pilot phase-zero gate.

## Adapter review corrections

Revision `536a79601d3116e8a30befacc07127ef05b052f1` accepts literal integer
negative-zero bytes and aligns missing-ID and assignment validation diagnostics.
Both real processes pass eight complete refusal regressions. All 5,973 live
cases pass compare/propose/bless/check at the unchanged 60-second budget; every
original golden reply remains byte-identical. A new complete initial-baseline
artifact accompanies the untouched original evidence.
[Commands, reports and actual provenance](validation/2026-11-22-536a7960/README.md)
record the clean overlay, four CTests and affected Rust checks.

That checkpoint left the numeric-overflow finding open; the latest adoption
entry above resolves it. Corrected graph scope/layer confirmation, updated
hosted CI and parent review remain merge gates.
