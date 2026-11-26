<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Next Rust port batches

Planning snapshot: 26 November 2026, accepted `rust` at
`4738a9c23f8c6068784d26ba991a690044343892`. Maintain this plan on `main` as the
port advances. This is a scheduling guide, not a claim list or a change to the
port gates. The scoped draft PRs remain the authoritative claims.
Recheck accepted dependencies and open
claims before assigning work. Implementation uses Opus 5.5; each implementation
receives an independent Sol 6.1 review of its exact head.

The current batch owns `seg.cpp`, Clipper2 source p001 and the pure core headers.
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

## Continuous four-stream cycle

Keep four independent implementation streams moving. The parent coordinates
ownership, integration, reviews and refills; workers deliver one scoped unit or
prerequisite fix at a time. Opus 5.5 implements and a separate Sol 6.1 reviewer
checks a concrete exact head. Review and CI can continue while an implementer
takes the next independent assignment. Do not count an idle worker waiting on
CI as a reason to leave an otherwise usable implementation slot empty.

| Stream | Current assignment | Next dependency-ready work |
| --- | --- | --- |
| Geometry | SEG passed checks, then imported accepted extractor #79 at `b908f439a0`; fresh CI required. Graph adoption active. Ellipse #81 has 11 reference-CI NaN mismatches at `5c42b69677`; focused Opus repair active, fresh Sol review required afterward. Distribute #84 passed Sol review at `33fa41f687`; hosted CI pending. Main Opus slot implements pinned hypot. | Adopt graph with final-head provenance; circle after accepted SEG, then geometry headers p003, trigo, common kimath headers and arcs. |
| Polygon operations | Engine #76 passed checks at `6f58bc73fa`, then imported accepted extractor #79 at `401ebc2624`; fresh CI required. Rectclip #77 likewise awaits fresh CI at `b88832a7d2`. Transform #82 reviewed; reference prose corrected at `b2918f6b31`. Opus implementing graph adoption as independent fallback. | Source p002 after accepted engine p001, then public wrapper headers after rectclip and p002. |
| Core and general utilities | PR #73 awaits fresh CI at `ab6fd8f3ea`. Map helpers #78 corrected at `1cadc88eb8`, Sol recheck passed; target wx qualification pending. Arc #85 owner notes fixed at `eaa34442d1`. Vector2wx #86 passed Sol review; wasm comment corrected at `3861aaa7b2`. Freed Opus slot now implements Unix policy. | Hold wx_stl_compat reuse until map helpers is accepted. Keep pure headers partial for allocation, stream, concurrency and scoped-timer gaps. Audit UI core headers before line-chain work. |
| Numeric backends and spatial follow-ups | Pow PR #80 accepted. R-tree follow-up PR #83 at `813623a66b` passed Sol review, hosted checks pending. Opus now assigned poisson_disk. | Model newly exposed kimathLogOverflow host effects with explicit ownership/host assertion policy; further measured numeric and spatial gaps. Audit copy-on-write code before claiming a similar fixed-depth problem. |

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

## Recommended next assignments

| Order | Assignment | Start condition | Result to demonstrate |
| --- | --- | --- | --- |
| 1 | Adopt corrected geometry dependency graph | Opus active on accepted extractor PR #79 | Replay final-head provenance, retain generated dependency capture and regenerate graph, units, measurement, drift and accounting through tooling. |
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

## Independent small-unit queue

The units below have satisfied recorded hard dependencies at the snapshot.
Keep one unit per claim/PR, even if one worker handles several sequentially.

| Priority | Unit | Scope and validation focus |
| --- | --- | --- |
| A | `ellipse.cpp` — 127 lines; delivered PR #81 | Initial Sol source review found no functional defect, but target CI found 11 NaN mismatches at `5c42b69677`. Focused Opus repair and fresh Sol review are required. |
| B | `transform.cpp` — 127 lines; delivered PR #82 | Sol found no implementation defect; local-reference prose fixed at `b2918f6b31`. Final-head hosted CI pending. |
| C | `arc_construction.cpp` — 207 lines; delivered PR #85 | Sol found no source correctness defect; co-port owner notes fixed at `eaa34442d1`. Final-head hosted reference CI pending. |
| D | `distribute.cpp` — 89 lines; delivered PR #84 | Sol independently replayed all 2,758 cases and 679 controls at `33fa41f687` without findings; hosted reference CI pending. |
| E | `poisson_disk.cpp` — 143 lines; Opus active | Reproduce Boost MT19937, distribution algorithms, draw/evaluation order, trig bits and ordered samples. Bound tiny-radius storage/work; spacing alone is insufficient evidence. |
| F | `vector2wx.h` — 56 lines; delivered PR #86 | Sol found no correctness blocker; wasm evidence comment corrected at `3861aaa7b2`. Final-head hosted CI pending. |
| G | Unix `policy.cpp` — 35 lines; Opus active | Two genuine constant-return platform bodies; retain the ignored key argument without importing wxString behavior. No recorded hard dependencies. |

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

Map helpers PR #78 initially claimed green after 3,084 exact replies. Sol found
the implicit generic default skipped a copy and the UTF-8 logical limit counted
the terminator. Both are fixed with regressions at `1cadc88eb8`. The parent
reran 3,084 live C++/Rust/golden replies, 336 controls plus successful followups
on both processes, and import-free wasm replay; all agree. The baseline prose
incorrectly named wx 3.2.4, whereas its nested provenance records the actual
local wx 3.2.9 binary. Immutable golden/evidence remain historical, corrected by
an additive finding; the unit is partial pending target-runner evidence.
Sol's fresh recheck of the corrected head found no remaining issue and
independently passed seven library tests, two corpus/control tests and all
3,084 fresh C++/Rust/golden comparisons.

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
