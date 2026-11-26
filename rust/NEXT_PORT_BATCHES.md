<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Next Rust port batches

Planning snapshot: 27 November 2026, accepted `rust` at
`70888b572202033a6522f2f271b10cc908f74dfe`. Maintain this plan on `main` as the
port advances. This is a scheduling guide, not a claim list or a change to the
port gates. The scoped draft PRs remain the authoritative claims.
Recheck accepted dependencies and open
claims before assigning work. Implementation uses Opus 5.5; each implementation
receives an independent Sol 6.1 review of its exact head.

## Current integration and correction queue

Unix policy #89 is accepted as `925fab5de0c5721d0bd5c82c370386ea26e22001`.
All applicable hosted checks and exact-head hosted fallback `45856821086`
passed; the rebase merge preserved tested tree
`6b2c6b73c1c481faf1f1f90f4e43ccf980fba205`. Source review and locked-reference
manual checks passed. [Acceptance evidence](validation/2026-11-27-kiplatform-policy/README.md).

Unix system-info #90 is accepted as
`b98201bc6b4c9e90cf725a0ab62b29d7b7b37a42`, after merging only accepted rust
into reviewed head `397629f3ec38ed1df8951cca4a5edae8a4444573`. A fresh Sol 6.1
integration review found no actionable findings. User-authorized manual final CI
on this exact combined tree passes: 29 units, 140,712 cases, 135,944 exact,
4,768 explicit deferrals, zero failures/unprocessed; 460 workspace tests;
15 oracle CTests; both platform control suites and import-free wasm replays;
fmt, clippy, Rust 1.85 and portable builds. Previous 28 unit input/reply hashes
are unchanged. GitHub refused rebase; the permitted normal merge preserves
exact tested tree `bd5c03643276b1156c87bad6186b53e1e2d822b6`. Fresh hosted jobs
were running at merge; manual results are recorded separately from their
statuses. Original-head scoped hosted gates and fallback `45858418153` passed.
[Combined acceptance evidence](validation/2026-11-27-kiplatform-sysinfo/combined/README.md).
Both Unix units remain partial without goldens. Both merged PRs are unwatched.

Graph adoption #94 is accepted as
`70888b572202033a6522f2f271b10cc908f74dfe`, rebase-merged from reviewed head
`d3d65cb5ecf2f9f67b6b46e6a71b205e2db52d9c`. Accepted tree
`73ca919b04fab75da9b2250f40b722a22d4bde7a` equals the reviewed/tested tree.
The original source review and fresh independent Sol 6.1 integration review
found no actionable findings. All 1,801 records preserve accepted non-graph
metadata and reviewed graph fields; two splitter runs, measurement and both
drift outputs reproduce exactly. Counts are 3 green, 26 partial, 64 blocked,
1,708 todo and 29 ported units. The graph/code are unchanged by this refresh;
no new parse or completeness qualification is claimed.

The authorized manual gate on this exact head passes 140,712 cases with
135,944 exact, 4,768 explicit deferrals, zero failures/unprocessed. All 29 units'
inputs/replies equal the accepted baseline, and all 15 oracle CTests pass.
Source-bound native binaries were reused because every runtime/harness/workflow
input equals the already tested accepted tree; this is not a fresh build or
workspace/wasm rerun. Full source accounting, fmt and policy tests pass. Initial
local scaffolding omissions and the BOM error remain documented separately.
All completed hosted checks, including every live job and MSRV, pass; isolated
preflight 45867748826 is replaced by successful exact-head hosted fallback
45868907057. Hosted rust-port was still running at the decision; manual results
are not its status. #94 is merged and unwatched.
[Acceptance evidence](validation/2026-11-27-graph-adoption/README.md).

Core utility headers #92 at `b48b95f893125e53b6f4bb484156d2abee43dcd5`
received changes requested despite passing GCC 13 corpus qualification.
INPLACE_FUNCTION must preserve no-wrapper-allocation and observable callable
move/destruction/fallible-copy behavior. LEAK_AT_EXIT needs stable surviving
storage and fallible initialization/retry. Generic IsA/dyn_cast fixtures and a
safe mutable projection API are needed. Evaluate the reviewed safe stack_dst
compatibility route before proposing a weaker source contract or any unsafe
exception. MIRRORVAL target compiler order is already qualified.

wx STL compatibility #93 at `6f272c818852adf3d640aee315d1f4756d86eaa8`
received changes requested: implement throwing buffer/facet, sync and tied-stream
paths; narrow the termination guard that rejects defined unwinding; replace
infallible stream allocations; document algorithm licence/attribution. Record
state/mask overlap as a harness restriction. Preserve the reference sentry
profile and qualify its actual runtime/binary separately from GCC 15 diagnostics.
Both PR descriptions now record these blockers and remain draft. Passing existing
corpora does not clear them. Substantial corrections return to Opus and need
fresh independent Sol reviews; no golden is proposed.

The reported Opus quota reset has now passed. At 20:30:13 UTC on 26 November,
the original circle #95 delegate automatically resumed in its preserved checkout;
its executed model is confirmed as `claude-opus-5-5`. The scheduled parent
continuation first saw no pending tasks, but a separate one-off continuation
probe detected the original writer's edits and exited without changing anything.
Thread/process ownership was then reconciled. The original circle worker remains
the sole writer; it has been sent accepted `70888b5722` integration requirements.
The preserved backup and unpublished work remain intact. Do not resume another
circle implementer or use the probe's stale checkout snapshot as current state.

Two substantive correction tasks were submitted on the same requested route:
`rust-correct-core92-opus55-20261127` and
`rust-correct-wx93-opus55-20261127`. Each owns its existing clean original
checkout, must merge only accepted rust, and must correct the actual review
findings before a new independent Sol 6.1 review. They were starting at dispatch;
that state alone is not proof of implementation or completion.

There are three assigned implementation streams: the resumed circle task and
the two correction tasks. The fourth refill is the dependency-ready, unclaimed
`thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002`: engine lines
2272-3337 and offset lines 1-661, 1,727 owned lines. Both hard dependencies are
accepted partials. The parent is moving coordination into a tool-managed
`origin/rust` checkout before delegating a separate prepared checkout from that
accepted base. The scoped draft PR, once created after a fresh overlap check,
will be its claim; this plan does not reserve the unit.

No model/provider substitution or quota retry loop occurred. Another route
failure must preserve/report work without repeated dispatch. The single
scheduled continuation is now executing; no further timer is created.

The following historical notes preserve earlier heads and pending states. The
manual final integration section records their accepted outcome.

The earlier batch owned `seg.cpp`, Clipper2 source p001 and the pure core headers.
SEG has delivered draft [PR #74](https://github.com/Embeint/embeint-kicad-eda/pull/74),
implementation `f85e09907681ad748217ae5f91c31f55177010fc`. Sol 6.1 reviewed those
source bodies with no behavioral finding; its ownership-note finding is fixed
in head `0e9845fad7eb4f38fb7d2fe21cf6d56fc467ecbc`. Its dedicated SEG and geometry
checks passed. Developer preflight timed out during dependency bootstrap on the
isolated runner before Rust checks; [hosted fallback run 45726912995](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45726912995)
was dispatched for that earlier head. SEG imported accepted pow at
`24df53e57c` and completed all 24 checks (19 success, five inapplicable skips).
After extractor #79 advanced rust, the parent imported that accepted delta as
`b908f439a09859dab06aac21a06ddfebeebc9534`; final-head CI is required again.
The import equals exactly the accepted extractor diff, and the resulting tree
equals precomputed `d99888b961c00991a4917b1f9f44fdfffb73da9a`. SEG source
and header deferrals are unchanged. Use accepted rust for new topics, never an
open topic.

Pow [PR #80](https://github.com/Embeint/embeint-kicad-eda/pull/80) is accepted:
reviewed implementation `e055b8ff6b5ccb474db28f947cb65aeb05b1b9a1` passed Sol
review and all applicable checks, then rebase-merged as `a862a3eb8e` on
26 November at 00:30 UTC. The computed merge tree and accepted tree both equal
the reviewed tree `efe3862d1a3fb0722f0d94de92df814d56c08bbf`. The unit remains
partial for its separately owned dependencies.

Pending topics #73, #74 and #76 merge the accepted pow baseline. Each
merge was conflict-free; the imported diff equals exactly the accepted pow diff,
and each resulting tree equals its precomputed merge tree. Earlier fallback
runs/results are historical source evidence. Extractor PR #79 has since passed
all applicable checks and rebase-merged as `4738a9c23f`; its accepted tree equals
reviewed tree `b060f4ffd13e113a5e43c639859b690683681e08`. Pending topics must
import that accepted tool change before integration if their computed merge
tree would otherwise differ from their reviewed head.

Core header PR #73 has now passed all applicable checks on integration head
`ab6fd8f3ea4a546e628a5fc88dff5ec7e85717f4` and rebase-merged as
`7a984a76e8073b7b6cdc01895da4bae126f05641`. Its accepted tree equals the
reviewed tree `1dce6d847ddba23d8ba5641ad7d706e253978394`. Other pending topics
and interrupted recoveries must use this new accepted baseline before final
integration. Preserve existing local changes and accepted unit metadata when
recovering graph regeneration; do not overwrite the newly accepted core status.

## Core integration conflict refresh

After accepting core headers #73, nine topics conflicted in shared workspace
and oracle build registrations. The parent merged accepted `7a984a76e8` into
each topic and pushed all nine reviewed merge heads in one atomic fast-forward
publication. No other open topic was imported. Independent Sol review passed
on the exact heads below. This table supersedes earlier pending-head references;
previous CI and review records remain historical evidence for those heads.

| PR | Published integration head | Reviewed tree |
| --- | --- | --- |
| #74 | `3a24cb5c7f71221e132191c376f37d42083939f0` | `df93202fbb3455a013674814c5326812ec6a03a4` |
| #76 | `58067c565265d1e401aa5c608441d92290ebb41c` | `22d7f1500b2a3068993561f49a5a14176cbeb23e` |
| #77 | `23117820ad222697c4a2fc5aafd2b0fadcde7e6f` | `eef1a3f7a2ed5c44e7d139fae46d848ee5362a96` |
| #78 | `b96acf7125f66c0e4ef11f90be93f86a03a16936` | `3dadb6e2684e628e5cb073ef4aa7a554d23612c3` |
| #81 | `5e060eeb08d9d1b8b1d5577abbb931d4383eaa9d` | `5153105d5ac2d3b7875080c9224a8494fe35b903` |
| #82 | `2057cbe19449fd20aadbc9cdb2e8dc3c22963f63` | `b25ba9a85047cb1a06b17c6eb84a5e5d3a4b18a5` |
| #84 | `d528c0fbe4839724e39ab575846d01673beb146e` | `9be9a030cac804594a4086a05e532b272d68e030` |
| #85 | `7fbeed303c72ba9b93a59ab37044708a1bbb171c` | `ba1d5cf3c57f191ea08bb714108e843bef58f1be` |
| #86 | `a3ab12f1b738863ac0df01d058249f7465404079` | `bba5e22160bfbdd393ad4c66d21e208e723345e6` |

Locked Cargo metadata passed on all nine topics. Independent review verified
an exact three-way union of workspace members, lock packages, oracle registry
entries and tests; all nonshared tracked blobs match their contributing heads.
Both accepted clock wrappers are retained. Engine private includes and both
rectclip replay links are preserved, together with existing adapter ordering.
There are no implementation, corpus, refusal, golden or unit-status changes.
Fresh hosted checks on these published heads are required before integration;
this metadata review does not replace the original source reviews or CI.

## R-tree and arc CI follow-up

R-tree #83 completed all 15 checks on reviewed source `813623a66b` (12 success,
three inapplicable skips). Its merge tree differed from that source after
accepted rust advanced. The parent imported accepted `7a984a76e8` conflict-free
as `0fdf2763e7b5013b1871dee778a6436128f1d41c`; its imported diff exactly equals
the accepted delta and its tree equals precomputed
`607657cb09a264e0928758a3a56f6d2b8215a98f`. Locked Cargo metadata passes.
Runtime R-tree bodies are unchanged; fresh hosted checks remain required.

Arc #85's earlier `arc-construction-live` run 45739086562 failed generator
`--check` before any live comparison. Host sin/cos produced different tangent
input bits on Windows and Linux. The parent corrected input construction to
use deterministic decimal sine/cosine and powers of ten, retaining every case
ID and changing only 19 random tangent inputs. Both platforms now pass generator
checks. Fresh local comparison gives 2,892 exact results, 20 existing deferrals,
zero failures and 20 matching controls; import-free wasm replay and five shim
tests pass. The local GCC 15/glibc 2.43 oracle remains non-reference evidence.

Current arc head `317c7b571c64eb4f0d2483a68033afaf470910f0`, tree
`a829f00521e53f22fbeffd5bb74c99334d722490`, supersedes its batch integration
head above. No runtime, guard, golden or status changed. Original source Sol
review remains recorded; the parent handled this small generator fix directly.
Unit validation passes. See `port/findings/arc-construction-portable-input-generation.md`
on the arc topic. Final-head hosted CI must qualify the updated inputs before
integration; earlier corpus comparisons remain historical evidence.

## Runner outage and hosted preflight fallback

Runner audit on 26 November found `proxmox-kicad-isolated-01` offline, with
15 developer preflight runs queued against `kicad-ci-isolated-linux-x64`.
These are self-hosted waits. The authoritative hosted checks on current heads
of #74, #76, #77, #78, #81, #82, #83, #84, #85, #86 and #87 have finished
successfully. #88 has a separate `glibc-libm` failure in the source/harness
check step, before numerical qualification, which still requires investigation.

The parent cancelled the queued offline preflights for those 12 implementation
PRs and dispatched the existing workflow's `use_hosted=true` fallback on each
unchanged topic head. All 12 fallback runs were verified in progress. A job
inspection confirms `ubuntu-24.04` and a GitHub Actions runner; hosted capacity
is available. No runner configuration, check requirement, implementation or
qualification rule was changed. The two interrupted claim-only topics #89/#90
and the accepted-baseline preflight remain outside this dispatch batch.

Fallback runs: #74 `45767628816`, #76 `45767638812`, #77 `45767647190`,
#78 `45767655217`, #81 `45767663885`, #82 `45767671810`, #83 `45767681151`,
#84 `45767689492`, #85 `45767698030`, #86 `45767706903`, #87 `45767715635`,
and #88 `45767723841`. Await these outcomes before claiming complete checks.
The offline runner remains an infrastructure issue; this existing fallback
removes it from the current implementation PRs' completion path.

## Hypot build correction and completed fallback batch

All 12 hosted fallback preflights in the preceding batch completed successfully
on their recorded source heads. Current hosted checks on #74, #76, #77, #78,
#81, #82, #83, #84, #85, #86 and #87 have no remaining build failure.
The #88 failure was investigated from the complete job log: its source hashes,
coefficients and 12 Python harness tests passed, then standalone wasm harness
`cargo fmt --check` failed on module ordering. Numerical qualification had not
started; the subsequent missing-comparison report failure was expected.

The parent applied rustfmt ordering and kept the hypot comment beside its
module. It also imported accepted `7a984a76e8` conflict-free, verifying that
the imported diff equals the accepted delta. Current published hypot head is
`846c58813e72355ad3ebaf24ce4427975f3fee64`, tree
`f3eccd6cc6ad83ed2e5ff47c380415ed7e21e707`. Runtime bodies, qualification
inputs, guards and status are unchanged. Original Sol source review is retained;
the parent handled this small formatting fix directly.

Local standalone/workspace formatting, all 12 Python harness tests, kicad-math
clippy and locked wasm debug/release builds pass. Both wasm profiles replay all
1,607,712 existing locked hypot records with zero mismatches. No golden was
added. See `port/findings/hypot-wasm-harness-format-check.md` on the hypot topic.
Fresh complete hosted qualification on the corrected head remains required;
its new hosted fallback preflight is run `45823810813`.

<!-- manual-integration:start -->
## Manual final integration

The user requested manual final CI while merging the reviewed implementation
PRs. The parent integrates one topic at a time with accepted rust, preserves
owned source blobs and shared registrations, and tests the resulting exact tree
before publication and merge. The initial SEG head had all applicable hosted
checks plus its exact-head hosted preflight fallback complete. Subsequent trees
run the CI commands manually on the locked reference runtime.

The private Linux validation root uses the unchanged input lock: all 249
package archives and every recorded compiler/runtime file hash are verified.
It runs GCC 13.3, glibc 2.39, libstdc++ 14 and wxWidgets 3.2.4, with the SSE2
dispatch tunables. Rust is 1.99.1. This validates these integration trees; it
does not adopt a new reference image or claim broader candidate qualification.

Each manual gate runs workspace formatting/clippy, affected Rust tests, the
genuine pinned C++ build and CTests, the resistor runtime check, the full
registered-unit exact comparison/golden policy gate, refusal controls and
portable wasm builds. Full workspace tests ran at the start and end; final combined
workspace, MSRV, generator, control, layout and wasm replay checks pass.

| PR | Accepted commit | Tested tree |
| --- | --- | --- |
| #74 | `978956b9f700ad2a7d0e4fa2ebf96ae7ec7805d0` | `df93202fbb3455a013674814c5326812ec6a03a4` |
| #76 | `8a5239afef31a33b4aae2dfe7cc0daace49ead04` | `659f371720baf03ac2c589e887c7ccbc8016a901` |
| #77 | `4e77e346d3c2f086b5101ccff0839acd93389ebe` | `d10df659ae7189ab0cde70958887bff63b104b02` |
| #78 | `8e4adefd4212c384759bc4e81948ef3d49c01839` | `855f6f2d5b8edaa9c92666d68583a7bcfd5f9389` |
| #82 | `4aaad9d2eb6673eb6f977b7a87a475e522b4b007` | `2d1c6d8e2218b00d5e64d56775d7af268f7a77f7` |
| #83 | `70462b87fb7996c4671a31e8995db420a01b111c` | `939292ae773fb3165438023666d888305f448ff1` |
| #84 | `c21a89d6680adf2a8cfaba594b22352cae0b9cec` | `d330b67903c24065decf4a4a980d9b1cf359bd63` |
| #86 | `a7dd17d5f6953bfa58f70a36a826107895c79fc5` | `e5085bc8c2afae7f31fa78aab41db8643357c4da` |
| #87 | `82564b2c8af450facc6e3ee66224044e1947665e` | `8c9574d902189a7cd60e58061b628dd05728d69c` |
| #85 | `d607f5ec13a6af575af73bda586a7f427bae3cf4` | `dff142d439434981fec40f14dc3d43e917170df7` |
| #88 | `66140fbe5269747429352e3dbf0c690d14abcfcc` | `2737596c95437476d52632a29ad37fcd7a351a1a` |
| #81 | `98a12c1123efc4904e0f0c5a59299452a7a15e5d` | `dbfb66a55e0854cc8436d8c500617940c935c1ea` |

All 12 implementation PRs in this cycle are accepted. Final canonical-byte
verification passes: 140,550 public cases, 135,782 exact results,
4,768 explicit deferrals, zero failures, all 16 corpus wasm replays,
workspace tests, Rust 1.85, generators, controls and layout parity. Full hypot
clears all arc deferrals; ellipse retains 100 logging deferrals and covers 15
newly exposed guarded programs in controls, without dropping any request.
[Final evidence](validation/2026-11-27-final-integration/README.md) records
commands, source/runtime bindings, hashes and the remaining qualification limits.

Every accepted merge tree is checked against its manually tested tree. Normal
merge commits are used only after GitHub explicitly refuses the rebase merge.
Per-step source bindings, command logs, oracle hashes and exact reports are
retained in the private manual-validation workspace.
<!-- manual-integration:end -->

## Continuous four-stream cycle

Historical recovery details below are superseded by the current queue above.

PR #75 is accepted on `main` at
`0a012ba1afb4a192c0ccbbb34ff4bbe9314fe722`; its computed merge tree was
verified. The completed implementation batch remains accepted on `rust` at
`98a12c1123efc4904e0f0c5a59299452a7a15e5d`.

On the user's instruction to continue, four recovery assignments were submitted
to the existing `claudeAgent` provider with `claude-opus-5-5`: core utility
headers, corrected graph adoption, Unix policy #89 and Unix system info #90.
The first recovery has now delivered: Unix policy #89 at
`4f482553db5ded162af1c3d35552ef51ca731a78`, with an executed-model record of
`claude-opus-5-5`. This proves that the requested route executed successfully;
it does not retroactively qualify the previous interrupted work. Source review
by a fresh Sol 6.1 delegate found no findings at this exact head. Local evidence
is 36 exact public cases, zero deferrals and 25 separate refusal controls on
GCC 15/glibc 2.43/wx 3.2.9. The parent has now supplied GCC 13/glibc 2.39/wx 3.2.4 evidence:
36 exact policy cases, 25 matching controls, wasm replay, Rust 1.85 and the full
registered-unit gate pass. All 27 previously accepted unit input/output hashes
are unchanged. Hosted CI still runs.
[Reference evidence](validation/2026-11-27-kiplatform-policy/README.md) records
commands, hashes and scoped limitations.
The unit stays partial, with no blessed golden and the inherited wx/harness
qualification limits retained.

Runner inspection found `proxmox-kicad-isolated-01` still offline. Policy's
queued isolated developer preflight `45852707850` was cancelled, and the existing
`use_hosted=true` fallback was dispatched as
[run 45856821086](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45856821086).
Its workflow head is the same reviewed `4f482553db`, and it is in progress.
This supplies ordinary developer preflight on the available hosted runner;
manual numerical/source reference evidence is recorded separately above.
Repository settings and required checks are unchanged.

The freed implementation stream is assigned
`libs.core.application.thin.replacement.p001` (`wx_stl_compat.cpp`), reusing
accepted wxString/wxPoint/wxSize models. Draft #93 at
`a0f9d6396d759eae2f86aaf6aeb63c0c879c3d9d` is a claim only. Its worker ended
before implementation, so the parent submitted a continuation in that same
prepared checkout to finish the bodies, byte/locale contracts and validation.
No implementation or review evidence is claimed for #93 yet. Core utility
headers have claim #92 at `060319e6542579d70a36b56db249d0577ef3ceaf`;
that is also ownership evidence, not delivery.

Unix system-info #90 is delivered at
`ab2d7b683c9ea101540adc084492c989b23c4161`, with fresh Sol 6.1 review reporting
no findings. Its scoped hosted reference run `45852710376` and `rust-port` pass:
126 exact cases, zero deferrals, 67 matching controls with continuation, and
wasm parity on GCC 13/glibc 2.39/wx 3.2.4. The parent verified that the hosted
merge tree equals the reviewed tree and retained the artifact/log hashes in
[the evidence](validation/2026-11-27-kiplatform-sysinfo/README.md). No duplicate
local qualification run is needed. The queued isolated preflight `45852706498`
is replaced by exact-head hosted fallback `45858418153`, still pending.

The freed implementation slot is assigned
`libs.kimath.src.geometry.circle.application.ui.p001` (`circle.cpp`, 380 lines).
Its four hard dependencies are accepted partials, and it was unclaimed at
assignment. Opus 5.5 receives a separate prepared checkout from accepted rust
`98a12c1123`; it must finish implementation and validation after its scoped
claim. The independent review waits for a concrete finished head.

Use completion events, not status polling or retry loops. Any renewed routing
failure must be reported without changing the requested model/provider.

The earlier provider interruption stopped all four Opus assignments around
02:10-02:11 UTC on 26 November with `503 AgentFleet routing authorization
denied or unavailable`. Existing changes and claims remain preserved. Recovery
workers have exclusive scope in those original checkouts and must safely merge
accepted rust, retain all newly accepted metadata/registrations, and never import
an open topic. Each completed implementation or graph adoption receives a fresh
independent Sol 6.1 review of its concrete head before integration.

The original 26 November recovery points are preserved in local patch/
untracked-file backup archives. The following records describe those historical
checkpoints; current delivery and assignment states are recorded above. Graph adoption remains at accepted
`4738a9c23f` with 1,087 changed entries on
`t3/adopt-canonical-graph-units`; its semantic/qualification checks and
publication are unfinished. The original Unix policy [claim #89](https://github.com/Embeint/embeint-kicad-eda/pull/89)
is at `82782c9c76` with 14 local changed entries. Unix system-info
[claim #90](https://github.com/Embeint/embeint-kicad-eda/pull/90) is at
`913d4ac93f` with 15 local changed entries. These are claim commits, not
published implementation evidence. The core utility header checkout is clean
at `4738a9c23f`, with no claim or implementation at that snapshot. Preserve these
recovery anchors and continue the current assignments without duplicate claims.

Keep four independent implementation streams moving. The parent coordinates
ownership, integration, reviews and refills; workers deliver one scoped unit or
prerequisite fix at a time. Opus 5.5 implements and a separate Sol 6.1 reviewer
checks a concrete exact head. Review and CI can continue while an implementer
takes the next independent assignment. Do not count an idle worker waiting on
CI as a reason to leave an otherwise usable implementation slot empty.

| Stream | Current assignment | Next dependency-ready work |
| --- | --- | --- |
| Geometry | Core utility #92 corrections assigned; original circle #95 automatically resumed on Opus 5.5. | Complete existing circle and core work, then fresh Sol reviews; geometry p003 follows accepted circle. |
| Polygon operations | Graph #94 is accepted at `70888b5722` after fresh review and exact-tree manual gate; metadata refresh is complete. | Engine/offset source p002 selected for a fresh isolated refill from accepted rust; claim pending. |
| Core and general utilities | Unix policy #89 is accepted; wx STL #93 needs stream exception/guard/allocation corrections. | The wx stream exception/guard/allocation correction task is assigned to Opus; fresh Sol review follows delivery. |
| Numeric backends and spatial follow-ups | Unix system-info #90 is accepted with exact-tree manual final CI. Circle #95 resumed automatically at reset with existing work preserved. | Finish the original circle assignment; logging effects and copy-on-write depth remain queued. |

On each implementation completion, immediately inspect the result, start the
independent Sol review and select the next unclaimed dependency-ready assignment.
Fix small review findings directly; send substantial implementation corrections
to the Opus implementer. Every fresh independent review/recheck gets a new
review task. Keep pending PRs draft until required checks and review pass.
Only the parent integrates, and only after all applicable checks pass and the
computed merge tree equals the reviewed tree. After accepted merges, refresh
`origin/rust`, recompute readiness and rerun affected deferred callers.

Update this document on assignment/delivery, review disposition, accepted merge,
newly exposed dependency/deferral, or a change to the queue. Record exact heads
and actual evidence when reporting acceptance; a short planning turn is not a
reason to lose the continuous cycle. If a stream cannot advance, take independent
fallback work and leave its specific blocker visible. Never weaken parity or
merge gates to sustain throughput. Use delegate completion events and PR check
watches for continuation rather than polling, sleep loops or scheduled tasks.

Keep topics in separate worktrees, based on accepted `rust`; merge only accepted
`origin/rust` for integration. Serialize small shared registration/workspace edits
when bringing results together and preserve all owners' changes. Heavy WSL builds
still share the build lock and job cap: four streams do not mean four unrestricted
builds. Main-branch planning updates use their own topic/PR, separate from port
implementation PRs against `rust`.

Provider interruption, 26 November: the core implementer received an HTTP 429
at 23:50 UTC on 25 November, with a reported reset in 4h10m13s (approximately
14:00 AEST on 26 November). It had finished the source and local comparison but
had not committed delivery. Preserve and publish its scoped work for review;
do not confuse the claim-only PR's earlier checks with implementation evidence.
Keep Opus 5.5 as the implementation model and Sol 6.1 as the review model.
Do not silently switch models/providers, hammer the limited account with retries,
or claim four active implementers while the provider is unavailable. Continue
available reviews, validation and integration; refill implementation slots once
the requested provider can run again. Other running workers report their own
outcomes through completion events.

Subsequent core recovery verification completed successfully. The parent published
the review documentation fix and assigned the next independent core unit
(`libs.core.include.core.application.thin.replacement.p001`, map_helpers.h,
122 lines, seven functions, no recorded dependencies) and polygon unit (rectclip
source p003) to Opus. These are new work assignments, not repeated retries of the
failed long-context run; any further provider refusal remains a visible blocker.

Core preflight run 45725447405 was cancelled during workspace testing, with no
test assertion failure in the retained log; its core tests had passed. The exact
cancellation cause is not established. [Hosted fallback run 45727170169](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45727170169)
was dispatched for the then-current reviewed head `5d99a6f8a1`; its acceptance
role is superseded by the new integration head. Do not describe this
as the same bootstrap timeout observed on SEG or count the interrupted run as a
completed workspace validation.

Core PR #73 subsequently completed all 19 checks at `26f11478b7` (16 success,
three inapplicable skips). Because extractor #79 had advanced accepted rust,
its prospective merge tree differed from that reviewed head. The parent merged
accepted `4738a9c23f` conflict-free into #73 as
`ab6fd8f3ea4a546e628a5fc88dff5ec7e85717f4`. The imported diff equals exactly
the accepted extractor delta, and the resulting tree equals the precomputed
merge tree `1dce6d847ddba23d8ba5641ad7d706e253978394`. Core runtime code and
partial contracts are unchanged. Final-head hosted checks must pass before
integration; the earlier green run is historical evidence for the prior head.

The final `ab6fd8f3ea` checks completed: 16 success and three inapplicable
skips. The parent verified the current merge result equals its reviewed
`1dce6d847ddba23d8ba5641ad7d706e253978394` tree, marked ready, and rebase-merged
#73 on 26 November at 02:20 UTC. Accepted commit `7a984a76e8` has that exact
tree. The unit explicitly remains partial for allocation, concurrent
interleavings, nondefault stream behavior and scoped-timer drop semantics.

## Recommended next assignments

| Order | Assignment | Start condition | Result to demonstrate |
| --- | --- | --- | --- |
| 1 | Adopt corrected geometry dependency graph | Existing Opus work interrupted by routing failure; generated output preserved | Complete final-head provenance, retained dependency capture and semantic validation before publication/review. |
| 2 | `circle.cpp` (380 lines), then geometry headers UI p003 (1,686 lines), as separate unit PRs | Accepted SEG; resolve the VRML edge before claiming the header unit | Complete SEG inline operations and shared shape interfaces; rerun SEG and `poly_ystripes` continuations. |
| 3 | Clipper2 source pure p002 (1,727 lines) | Accepted current engine p001 | Real `ExecuteInternal`, output builders and owned offset bodies; rerun the existing Execute cases through successful polygon operations. |
| 4 | Audit kimathLogOverflow host effects | Independent ownership and assertion contract audit | Define faithful portable/host behavior for the 1,096 expanded-corpus logging deferrals before assigning implementation. Pow is already accepted. |

Use the four streams above for the geometry sequence, Clipper2, core/general
utilities and numeric/spatial follow-ups. Use the small units below when a
dependency, review or build lock holds up one of those streams.
Reviews begin only after a concrete implementation head is available.

## Geometry dependency audit

Tool fix delivered as [PR #79](https://github.com/Embeint/embeint-kicad-eda/pull/79),
implementation `664493feb0684460b82ad9580d972cb6a7a5b27a`, independently reviewed
by Sol with no blocking findings. The parent fixed its optional replay cache-race
finding at `90c8c3a1ea241001c4e4d63e19ea7da931ca7e09`; all 40 extractor tests
pass under WSL/libclang and Windows passes 27 with 13 expected skips. The old
large replay evidence remains bound to its original aggregation identity.
It resolves endpoints to each TU's local definition before aggregation while
retaining all-body fallback for declaration-only cross-TU calls. Reported V5
cache replay removes 1,798 edges, including 32 SHAPE cross-edges, without function,
copy or wx-classification changes. The replay retains the original four fmt
parse failures; it is not new completeness evidence. Canonical graph/units are
unchanged: adoption is a separate task after the accepted tool change, including retained generated
dependency capture, provenance, measurement, drift and accounting updates. The
committed p003-to-VRML scheduling edge remains until that adoption is accepted.

Final integration head `59fa8ff9c58ea926f439e482cab9905452252050` passed
rust-preflight, rust-port, MSRV and progress-page build (deploy and resistor
migration were inapplicable skips). The parent verified its delta from the
reviewed tool head is exactly the accepted pow delta and its computed merge
tree equals the reviewed integration tree. Rebase merge completed on
26 November at 01:02 UTC as `4738a9c23f8c6068784d26ba991a690044343892`,
with accepted tree `b060f4ffd13e113a5e43c639859b690683681e08`.
No canonical graph or unit dependency change is implied by accepting the tool.
An Opus 5.5 worker is now assigned the separate canonical adoption on that
accepted baseline. It must preserve existing claims/statuses/notes, retain the
verified generator-dependency capture, and record final-aggregator replay
provenance; the old diagnostic replay is not final-head adoption evidence.

The current unit records do **not** make `trigo.cpp`, `shape_arc.cpp` or
`intersection.cpp` ready after SEG alone. `trigo.cpp` also requires
`libs.kimath.include.geometry.application.ui.p003`, the unit containing `seg.h`,
`shape.h`, `shape_arc.h`, `shape_circle.h`, `shape_compound.h` and
`shape_ellipse.h`. That header unit requires `circle.cpp`, SEG, and a recorded
VRML plugin dependency.

Inspection of the committed corrected-extraction graph found five edges from
geometry code to `SHAPE::~SHAPE()` in `plugins/3d/vrml/wrlfacet.cpp:675`:
`SHAPE_ARC`, `SHAPE_CIRCLE`, `SHAPE_ELLIPSE` and `SHAPE_LINE_CHAIN_BASE`
destruction, and `SHAPE_COMPOUND::AddShape`. The geometry base destructor is
instead defined inline in `libs/kimath/include/geometry/shape.h:138`. These
separate global `SHAPE` definitions share a Clang USR; the graph already records
definition variants, but these cross-target edges need a source/TU-context audit.

The audit should reproduce this collision, establish which definition each call
can reach in its real translation unit/link target, and check the variant and
virtual-dispatch resolution rules. If incorrect, fix the extractor, add a small
regression and regenerate through the supported tooling with provenance. Do not
hand-delete dependencies or edit generated reports to make units schedulable.
Until resolved, treat the edge as a scheduling blocker, not a reason to port the
whole 3D plugin stack ahead of 2D geometry.

SEG's delivered implementation reports 858 of 9,503 cases deferred at `SEG::Square`;
`SEG::SquaredLength` is also unavailable. Its type co-port must be recorded in
the owning header unit's notes (done in the review fix). The header port should
adopt/audit those types,
implement owned inline bodies, update the generator's explicit deferral events,
and reconsider the 197 omitted candidates whose undefined operations were
previously hidden behind a deferred dependency. No claim of zero
deferrals is justified by implementing only those two methods.

After `circle` and header p003, the intended sequence is:

1. `trigo.cpp` (562 lines), then `libs.kimath.include.application.ui.p001`
   (255 lines; its recorded hard dependency is trigo). Wire the existing
   `BOX2::SegmentIntersectsSegment` and rotation deferrals.
2. Geometry headers UI p004 (1,666 lines), then `shape_arc.cpp` (1,168 lines).
   The arc unit also requires circle and the common kimath headers above.
3. Geometry headers UI p005 and the geometry pure p001 headers; build the
   required shape/line utilities before scheduling `intersection.cpp`.
4. Line-chain source p002 before p001 according to the recorded hard edges;
   then polygon-set work once both geometry and Clipper2 prerequisites exist.

`line.cpp` (70 lines) becomes available after SEG. `half_line.cpp` additionally
needs geometry header units. `intersection.cpp` (680 lines) needs circle, line,
half-line, shape arc, shape ellipse and shape utilities as well as headers;
it belongs later than the next wave.

Line-chain p001 also depends on the **UI core header unit**
`libs.core.include.core.application.ui.p001` (1,123 lines), which is different
from the pure core headers being implemented now. Its platform dependencies
need their own interface audit; the current core worker does not remove that
blocker.

## Clipper2 completion sequence

Rectclip source p003 is delivered as [PR #77](https://github.com/Embeint/embeint-kicad-eda/pull/77),
implementation `f72b23229619869935b4faf0c165ed709505d120`. Sol independently
replayed 1,574 exact replies and 11 controls with no identified clipping-result or
lifecycle defect. Header-owner co-port notes were completed at `87e2913afa`;
source p003 remains partial and wrappers remain todo pending their own work.
All 19 checks completed at `87e2913afa` (16 success, three inapplicable skips).
The parent then imported accepted extractor `4738a9c23f` conflict-free as
`b88832a7d29434c62845f66bc8fdf440b896bdea`. The imported diff equals exactly
the accepted extractor delta and the resulting tree equals the precomputed
merge tree `3694d4734fee268101e6caa637104a7bc57c3b5c`. Clipping runtime code
and qualification limits are unchanged; final-head hosted CI is required
before integration.
The local global-gate case stream-6-False-fixed-0 reproduces the same uppercase
INF reply in an independent earlier oracle without rectangle clipping, while the
accepted Rust shim uses pre-GCC-15 lowercase inf. This supports the existing
libstdc++ explanation for that case without waiving the full hosted gate.
Public rectangle operations call the genuine library; private helpers use a
namespace-isolated source compilation, and UBSan child preflight retains its
documented qualification limits. The waiting polygon slot delivered transform.cpp
while engine p001's acceptance blocks source p002, and now takes graph adoption.

Engine source p001 covers lines 1–2,271.
PR #76 completed all 19 checks at `6f58bc73fa` (16 success, three inapplicable
skips). The parent then imported accepted extractor `4738a9c23f` conflict-free
as `401ebc26240160c939f28ca5522bdca52c050a6e`. The imported diff equals
exactly the accepted extractor delta, and the resulting tree equals the
precomputed `d16d2228f900159b9ce957ad2c88af5d076f3efe`. Engine runtime code,
refusal controls and partial qualification are unchanged. Final-head hosted
checks must pass before acceptance; source p002 remains queued behind it.
Source pure p002 owns engine lines 2,272–3,337 and `clipper.offset.cpp` lines
1–661. This includes `ExecuteInternal`, `BuildPaths64`, `BuildTree64`,
`BuildPathsD` and `BuildTreeD`. Consequently the first engine unit alone cannot
remove every one of #72's 31 deferred cases or finish polygon booleans.

Source pure p003 (`clipper.rectclip.cpp`, 1,027 lines) was implemented independently
while p002 remained blocked. After source p002 and p003 are accepted, port include
pure p002 (1,151 lines: public wrappers, Minkowski and associated headers), whose
hard dependencies include both source units.

Validate all four boolean operations, fill rules, holes/tree hierarchy, open
paths, Z handling, collinear preservation, reverse solutions and repeated
execution. Replace #72's temporary coordinate-range restriction only once
precise guards cover the arithmetic reached across both engine units. Keep
the remaining restriction explicit until then. `SHAPE_POLY_SET` still has
geometry prerequisites after Clipper2 is complete.

## Independent fallback queue

The units below have satisfied recorded hard dependencies at the snapshot.
Keep one unit per claim/PR, even if one worker handles several sequentially.

| Priority | Unit | Scope and validation focus |
| --- | --- | --- |
| A | `ellipse.cpp` — 127 lines; delivered PR #81 | Target CI's 11 NaN mismatches repaired at `fb6dd8632c`; fresh Sol review and complete recorded-reference replay pass. Final-head hosted CI pending. |
| B | `transform.cpp` — 127 lines; delivered PR #82 | Sol found no implementation defect; local-reference prose fixed at `b2918f6b31`. Final-head hosted CI pending. |
| C | `arc_construction.cpp` — 207 lines; delivered PR #85 | Sol source review passed; co-port notes fixed. Stale util/box caller count assertion and scoped workflow corrected at `207f67b2cb`; fresh hosted CI pending. |
| D | `distribute.cpp` — 89 lines; delivered PR #84 | Sol independently replayed all 2,758 cases and 679 controls at `33fa41f687` without findings; hosted reference CI pending. |
| E | `poisson_disk.cpp` — 143 lines; delivered PR #87 | Sol source review passed. Standalone wasm lockfile fixed, early guard added and accepted core imported at `19e7851592`; fresh hosted CI pending. |
| F | `vector2wx.h` — 56 lines; delivered PR #86 | Sol found no correctness blocker; wasm evidence comment corrected at `3861aaa7b2`. Final-head hosted CI pending. |
| G | Unix `policy.cpp` — 35 lines; provider interrupted, claim #89 | Two genuine constant-return platform bodies; retain the ignored key argument without importing wxString behavior. Recover existing local work. |
| H | Unix `drivers.cpp` + `sysinfo.cpp` — 70 lines; provider interrupted, claim #90 | Six genuine platform bodies; unchanged initialized output parameters, empty lifecycle and no synthetic OS queries. Recover existing local work. |
| I | `include/core` pure utility headers — 1,046 lines; provider interrupted before implementation | Callable copy/move/lifetime, leak ownership, mirror, injected-clock throttle and type classification. Separate from #73's libs/core headers. |

Ellipse's local GCC 15.2/glibc 2.43 comparison reports 2,239 cases: 2,103 exact,
136 deferred (100 logging and 36 hypot), zero failures, plus 234 identical
undefined-behavior controls. Four header callers now return results, giving
20,474 results and 1,697 deferrals. Sol independently checked implementation,
defaults, continuations, scoped Rust checks and caller counts, but did not rerun
the live oracle. Compiled NaN forms remain subject to final-head GCC 13/glibc
2.39 hosted qualification. FLIP_DIRECTION is now recorded in its owning core
header unit, which remains todo.

Ellipse reference CI on `5c42b69677` subsequently found a genuine parity gap:
run 45732444817, rust-port job 139617508389 processed all 2,239 cases with
2,092 compared, 136 deferred and 11 mismatches. Every mismatch is a NaN sign
or payload in the rotated point's second coordinate; finite results agree.
The local GCC 15 operand-order qualification did not establish the target
GCC 13 behavior. The initial source review does not supersede this failure.
Parent downloaded the genuine hosted oracle and report from artifact
15079665501; oracle SHA-256 is
`9b4dce98ce61384dd70ab2d2cf9f21afe11987a00330a8b26ce97cb2659e57b5`.
A focused Opus repair runs alongside the four independent port streams.
It must confirm target disassembly, preserve all cases and guards, import only
accepted rust, replay the complete corpus and controls, and receive a fresh Sol
review plus final-head hosted CI. Running the downloaded binary under local
glibc 2.43 is not equivalent to its original target runtime.
The dedicated ellipse-live run 45732444601, job 139617502795, also failed.
Its artifact 15079837395 confirms exactly the same 11 case IDs and identical
oracle/Rust failed replies, with the same 2,092/136/11 counts. It is folded
into the existing focused repair rather than treated as a second defect.

The focused Opus repair is published as
`fb6dd8632cda29ace4dc7ae536071588eb0ed495`. Hosted GCC 13 disassembly
selects the sine term first in `ry = ex*sinR + ey*cosR`; the port now follows
that order. A regression covers the three observed NaN shapes and fails with
the old order. Replaying all 2,239 recorded hosted oracle replies gives 2,103
compared, 136 deferred and zero failures; controls and native/wasm replay pass.
The worker also reports byte-identical caller geometry replies and a passing
350-test workspace run. This is recorded-reference replay, not a fresh target
oracle run.

The parent verified that the preceding merge imports exactly the accepted
extractor delta, and the final merge tree equals head tree
`8aa98fab6e15b37f4261a80d0a36e7828c91acae`. No other open topic is imported.
The implementation fix touches ellipse code/test, its own unit notes and a new
additive finding. Fresh independent Sol review passed at that exact head:
the reviewer verified the hosted binary hash and disassembly, independently
replayed 2,239 recorded replies (2,103 exact / 136 deferred / zero failures),
and confirmed that restoring only the old operand order makes the regression
fail. Final-head hosted checks remain pending; replay alone cannot qualify
the corrected head.

Map helpers PR #78 initially claimed green after 3,084 exact replies. Sol found
the implicit generic default skipped a copy and the UTF-8 logical limit counted
the terminator. Both are fixed with regressions at `1cadc88eb8`. The parent
reran 3,084 live C++/Rust/golden replies, 336 controls plus successful followups
on both processes, and import-free wasm replay; all agree. The baseline prose
incorrectly named wx 3.2.4, whereas its nested provenance records the actual
local wx 3.2.9 binary. Immutable golden/evidence remain historical, corrected by
an additive finding; target-runner evidence subsequently passed as recorded below.
Sol's fresh recheck of the corrected head found no remaining issue and
independently passed seven library tests, two corpus/control tests and all
3,084 fresh C++/Rust/golden comparisons.

All 19 checks then completed at `1cadc88eb8` (16 success, three inapplicable
skips). Dedicated run 45732438430 / job 139617483587 / artifact 15078975842
records GCC 13.3.0, glibc 2.39 and wx 3.2.4. All 3,084 cases compare exactly,
336 controls on each native process have successful continuations, and fresh
C++ replies replay in import-free wasm. Reply hash
`f7103e18b3969d9821cad7e364c16fbb085ced4cd05d065935d0d8185a7c4691`
matches the historical baseline.

The parent imported exactly the accepted extractor delta as `90d741a445`,
then recorded target evidence additively at
`d8ab26658ff94262de66700292e2bccc497b4999`. Runtime is unchanged.
The current merge tree equals head tree
`292a82549fac7d973509b4097b146f586dc87c40`; final-head hosted checks must
pass before acceptance. Unit status remains partial, with historical baseline
evidence and the documented capacity/platform/generic limits preserved.

Transform PR #82 delivered all six definitions at `7633f3b366`, based on
accepted `4738a9c23f`. The local GCC 15.2/glibc 2.43 SSE2 run reports 7,678
exact public cases, zero deferrals/failures, 1,696 matching refusal controls
and import-free wasm agreement for all 9,374 requests. Sol found no
implementation defect and independently passed 36 tests, clippy and generator
verification. Its provenance finding in the MapAngles note is fixed at
`b2918f6b3195c23d43ff7b6eb15eef400e273ff8`; only that documentation changed.
The source-confirmed always-swap behavior is preserved. Mutable DefaultTransform
remains explicitly host-owned. The unit stays partial without a golden and
awaits final-head target-host CI; the local comparison does not certify it.

Distribute PR #84 delivered both functions at
`33fa41f68788f7d3014baa706fecae18eac2f7e1`, including the unused final
target advance and default non-quiet KiROUND call. Sol found no actionable defect
and independently reproduced 2,758 exact public cases with zero failures or
deferrals, plus 679 control programs containing 707 refused calls and 723
successful calls. Reply hash:
`bb07a75fe6f2cac76445161cc71225e3842653a9cae0b6c0f8eba02f53b2865b`.
The reviewer reused the genuine local oracle rather than rebuilding it and did
not independently rerun wasm. The worker reports wasm corpus/control agreement.
Local GCC 15.2/glibc 2.43 evidence does not replace final-head hosted reference
CI. The unit remains partial without a golden.

The freed distribute slot now implements pinned glibc `hypot` as a focused
follow-up to `libs.kimath.include.math.application.ui.p002`. The existing
`VECTOR2<double>::EuclideanNorm` deferral is the measured target; ellipse #81
also reports 36 affected cases, which must be rerun after both topics are
accepted. This is existing vector ownership, not an invented glibc unit.
Reuse the accepted glibc 2.39 SSE2 source/package/dispatch qualification approach,
including exact NaN bits, scaling, subnormals and native/wasm agreement.
Do not import or edit unaccepted ellipse code to demonstrate the dependency.

Hypot delivered as PR #88 at
`980bcf964ae55a64e3368d20afaab040a3b339d6`. The accepted finite kernel is
unchanged; the public special-value path and vector norm wiring are added.
The locked glibc binding is `hypot@@GLIBC_2.35`, a plain function distinct from
the old compatibility entry. The worker reports 1,607,712 locked records with
zero mismatches on native Windows/Linux and import-free wasm, debug/release.
Sol independently rebuilt and replayed all records on Windows debug/release,
verified record/source archive hashes, source constants and the disassembly,
and passed Python qualification tests. Reversing NaN priority disagrees on
205 records, confirming meaningful discrimination. No blocking implementation
finding; hosted CI and WASI qualification remain pending.

Caller reports show 11,737 exact vector cases / 471 deferred out of 12,208,
and util/box 14,329 exact / 1,602 deferred, with zero local C++ mismatches.
All 2,070 vector and five util/box hypot deferrals become results without
changing earlier successes or introducing new deferrals. The new vector wiring
subsumes #85's finite-only branch; both make the same util/box count assertion
update. Resolve this through accepted rust and rerun arc/ellipse continuations
after integration. Four Windows release workspace failures were reported to
match the base, but that equivalence was not independently substantiated by
the reviewer; retain it as a local validation limit, not a passing check.

The freed Opus slot takes `include.core.application.pure.p001`: inplace_function,
leak_at_exit, mirror, throttle and typeinfo headers. Its canonical FLIP_DIRECTION
must eventually replace/reexport ellipse #81's minimum co-port through accepted
integration; do not import the open ellipse topic to build this unit.

Arc construction PR #85 implements all eight definitions and reports 2,912
public cases: 2,892 exact, 20 deferred, zero failures. Twenty malformed-request
controls agree; the worker reports import-free wasm agreement for cases and
controls. Sol reviewed source head `3167f167f2` with no concrete correctness
finding and independently passed generator and scoped native math/shim tests;
four existing reference-probe tests were ignored. It did not independently
rebuild the C++ oracle or rerun wasm. Parent documentation fix
`eaa34442d1dcf74ed373d2d48be6499f7bfbc1d9` records ARC_SOLUTION fields and
defaults in its actual owning geometry header unit, which remains todo.

That PR wires the already accepted finite-input hypot backend into the vector
norm. Local caller reruns report vector 9,723 exact/563 deferred and util/box
14,329 exact/1,602 deferred, with zero failures. The parent agreed this finite
wiring boundary with the full-hypot worker; integrate through accepted rust
only and resolve the overlap without duplicating the backend. Nonfinite
tangents still cause 20 arc deferrals. Hosted Ubuntu 24.04 evidence must qualify
the final head; local glibc 2.43 results do not certify target glibc 2.39.

Arc #85 layout-parity run 45735176879 failed in the full release workspace
tests before layout comparison. The util/box corpus test still expected
14,324 results / 1,607 deferrals after finite norm wiring resolved five of
those deferrals. Parent fix `207f67b2cb11cb1b51a24ed37a692be8b3109d3c`
requires the actual 14,329 / 1,602 counts and adds vector/util caller tests to
the scoped arc workflow. Eight scoped release tests pass; a fresh local live
replay gives those exact counts over all 15,931 cases with zero failures.
No runtime behavior, case, refusal guard or golden value changed. The additive
finding records the failed assertion and the non-reference local toolchain;
final-head hosted checks, including layout-parity, remain required.

The worker's suggested geometry pure header p001 is not a small arc-chord unit:
it owns 1,935 lines across 17 headers and depends on geometry UI p003.
Existing arc-chord accessors are only a minimum co-port, so the larger unit
remains blocked by its actual prerequisites. The fourth slot instead takes
`libs.kimath.include.math.application.thin.replacement.p001` (vector2wx.h),
whose six conversions have no recorded hard dependencies.

Vector2wx PR #86 delivered all six conversions at `ebefd12bbe`. The worker
reports 396 exact C++/Rust replies, 45 identical malformed controls with
successful continuations, and import-free wasm agreement for the successful
corpus. The actual local runtime is GCC 15.2/glibc 2.43/wx 3.2.9. Sol found no
correctness blocker and independently passed scoped Windows tests, generator,
fmt, clippy and portable wasm compilation; it did not rebuild the C++ oracle
or execute wasm parity. Parent documentation-only fix
`3861aaa7b28da15f21b7ec30cb84f18d011cbd90` narrows the wasm example's comment:
it replays 396 successful public requests, while malformed controls and their
continuations are checked in native processes. Unit status stays partial pending
final-head hosted reference CI.

The freed slot now takes `libs.kiplatform.os.unix.application.thin.replacement.p001`,
the two functions in Unix policy.cpp. Both ignore their wxString argument,
returning NOT_CONFIGURED and nullopt. Preserve the parameter through a minimal
host-boundary value without adopting the unaccepted wxString model or porting
the separate policy header's template body.

Poisson disk PR #87 delivered at `1ce518fe29`, reporting 499 exact public
cases, 57 matching refusals with successful followups, eight exact sample-limit
probes and 613 native/wasm replies. Sol found no concrete defect in the complete
sampling body, Boost RNG/distributions, resource bounds or oracle adapter.
It independently passed 80 tests (four preexisting reference tests ignored) and
generator verification for 499 cases plus 114 control requests. It did not
independently rebuild the C++ oracle or execute wasm parity.
Documentation-only `bed62f0c5b3f8b1d5b90336323d89bf27762a1e6` removes the
categorical target-identity claim: draw order was observed with local GCC 15.2,
and the local Boost 1.90/glibc 2.43 run does not certify the target GCC 13 /
Boost 1.83 / glibc 2.39 environment. Final-head hosted checks remain required.
The new GPL/Boost-licensed random crate and source provenance require continued
review when its consumers expand beyond this qualified engine/distribution use.

Poisson #87's glibc-libm run 45738462726 failed because the independent wasm
qualification Cargo.lock omitted the new local Boost dependency of kicad-math.
The locked build stopped before import-free wasm comparisons; those comparisons
were not run. Cargo regenerated the lockfile without numerical or dependency
version changes. An early locked-metadata check now catches this before
qualification record generation, and was tested against both stale and fresh
lockfiles. Locked debug/release wasm builds pass locally with no host imports.

Final correction head `19e78515922da5b50546203887e0e7f74c3cfedb` also imports
accepted core `7a984a76e8`. Shared workspace and registry conflicts preserve
both ports; the parent verified lock package unions and exact imported/owned
source blobs. The computed merge tree equals head tree
`5511099ced7e40affba49215073429a1534dffae`. Sampling, RNG and numerical
implementations are unchanged. Fresh hosted checks must complete every required
comparison; the earlier missing wasm results are not waived.

The freed stream takes `libs.kiplatform.os.unix.application.pure.p001`:
Valid3DConnexionDriverVersion, the empty SYSINFO constructor/destructor and
three SYSINFO queries. The pinned Unix bodies genuinely return false and leave
outputs untouched; fixtures must initialize every field before observation.
Keep complete output values and lifecycle observable without inventing system
queries or default initialization for uninitialized C++ aggregate fields.

Poisson disk is short but less predictable than its line count suggests.
Check the pinned sources rather than the moving upstream version: even random
argument evaluation order can affect the reference sample sequence.

## Follow-ups and acceptance

The `pow` follow-up belongs to the existing angle/math ownership, not a new
invented extractor unit. Reuse the pinned glibc source/licence/dispatch
qualification approach; model public wrapper semantics, NaN payload/sign,
signed zero, subnormals and extreme exponents without host `powf` substitution.
The reported 1,642 angle deferrals are a rerun target, not a promised reduction
until measured against the current accepted corpus.

Pow is now delivered as [PR #80](https://github.com/Embeint/embeint-kicad-eda/pull/80),
implementation `e055b8ff6b5ccb474db28f947cb65aeb05b1b9a1`. The worker reports
1,535,736 genuine locked-glibc records matching Windows native and import-free
wasm in debug/release. Native Linux/WASI qualification and geometry C++ comparison subsequently passed
hosted CI, and Sol review passed. All 1,642 pow deferrals disappear on the original
geometry corpus, but only 877 become results: 765 now reach kimathLogOverflow.
The added 633 cases give 491 results and 142 logging deferrals. Do not confuse
the base/head Rust corpus comparison with native C++ agreement. A later focused
logging follow-up must implement the pinned host/assertion effects, not silently
swallow overflow to turn these cases into successes; the expanded corpus reports
1,096 total kimathLogOverflow deferrals (954 in the original corpus plus 142
in the added cases). Sol independently reproduced the transition identities,
verified all 657 table words against locked libm, and inspected passing hosted
pow results on all six configurations. All remaining applicable CI passed before
the exact-tree merge into rust. The fourth stream has delivered the R-tree
depth-limit follow-up and now takes poisson_disk.

Remove #61's fixed 64-level R-tree limit as a separate focused follow-up. Reuse
the explicit-stack/active-node-budget approach demonstrated by closed #60,
without merging its obsolete topic branch. Preserve insertion/removal/query and
cursor order, test defined trees deeper than 64, and check allocation refusal
and interrupted-mutation lifecycle. Audit every remaining depth guard rather
than raising the constant.

R-tree follow-up delivered as [PR #83](https://github.com/Embeint/embeint-kicad-eda/pull/83),
head `813623a66be8edd8957d69aca228e01d83bcb235`, passed Sol review and pending
hosted checks. Opus reports 259 exact programs (76,820 steps), 27 matching
refusal controls and import-free wasm agreement, including root levels up to 79.
Search/removal and split propagation use explicit stacks/loops, with a separate
active-node budget. A flat topology observation permits deep evidence; the
nested state harness explicitly refuses above level 39. Review must assess
that harness limitation separately from the removed portable depth guards.
Deep orphan handling, other types and allocation exhaustion remain qualification
gaps; status stays partial. The copy-on-write unit's depth tracking is a suggested
audit, not evidence of the same fixed-limit defect.
Sol independently checked stack transition order, ten behavior tests plus the
refusal integration test, both generators and source/evidence hashes. The
original 249 corpus lines remain an unchanged prefix. Live C++/wasm replay was
not independently repeated by that reviewer; final-head hosted CI is required.

For each assignment: audit every owned body and post-deferral continuation;
run exact native difftests and the affected callers, scoped fmt/clippy/tests,
portable wasm checks and unit validation; retain genuine UB refusal controls
separately from successful cases. Report reference toolchain identity, exact
head, compared/deferred/failed counts and remaining gaps. Non-reference local
comparisons are diagnostic evidence. Hosted reference CI and separate Sol 6.1
review gate readiness; preserve partial status wherever required.
