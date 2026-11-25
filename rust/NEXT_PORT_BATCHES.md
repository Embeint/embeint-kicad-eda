<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Next Rust port batches

Planning snapshot: 26 November 2026, accepted `rust` at
`c45b38a2960fe8ccc67dace5f3bea8de96c0bf0c`. Maintain this plan on `main` as the
port advances. This is a scheduling guide, not a claim list or a change to the
port gates. The scoped draft PRs remain the authoritative claims.
Recheck accepted dependencies and open
claims before assigning work. Implementation uses Opus 5.5; each implementation
receives an independent Sol 6.1 review of its exact head.

The current batch owns `seg.cpp`, Clipper2 source p001 and the pure core headers.
SEG has delivered draft [PR #74](https://github.com/Embeint/embeint-kicad-eda/pull/74),
implementation `f85e09907681ad748217ae5f91c31f55177010fc`. Sol 6.1 reviewed those
source bodies with no behavioral finding; its ownership-note finding is fixed
in head `0e9845fad7eb4f38fb7d2fe21cf6d56fc467ecbc`. Reference CI remains
outstanding. Do not base new port topics on these open topics: wait for accepted
`rust`, or take independent work below.

## Continuous four-stream cycle

Keep four independent implementation streams moving. The parent coordinates
ownership, integration, reviews and refills; workers deliver one scoped unit or
prerequisite fix at a time. Opus 5.5 implements and a separate Sol 6.1 reviewer
checks a concrete exact head. Review and CI can continue while an implementer
takes the next independent assignment. Do not count an idle worker waiting on
CI as a reason to leave an otherwise usable implementation slot empty.

| Stream | Current assignment | Next dependency-ready work |
| --- | --- | --- |
| Geometry | SEG PR #74 in reference CI after Sol review; Opus investigating/fixing the geometry-to-VRML extractor collision | `circle.cpp` after accepted SEG, then geometry headers p003, trigo, common kimath headers and arcs. Use ellipse or transform if acceptance/regeneration blocks the chain. |
| Polygon operations | Delivered draft PR #76 at `96c1c83d80`; Sol review and reference CI underway. Local header deferrals reduced from 31 to 14. | Source p002 after accepted p001; rectclip p003 is independent fallback when Opus can run; then public wrapper headers. |
| Core and general utilities | Recovered PR #73 at `eecececf42`; Sol replay agrees on 669 new and 6,227 existing cases. Review identified allocation, stream, concurrency and scoped-timer gaps; keep partial and correct the claims. | Complete review documentation and await reference CI; resume Opus work when available. Independent small-unit queue follows. Audit UI core headers before line-chain work. |
| Numeric backends and spatial follow-ups | Opus implementing pinned glibc `pow` | R-tree depth-limit removal; further measured math/backend gaps, with explicit ownership and source-derived scope. |

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

## Recommended next assignments

| Order | Assignment | Start condition | Result to demonstrate |
| --- | --- | --- | --- |
| 1 | Audit geometry-to-VRML dependency edges | Independent investigation now | Establish the correct destructor targets and a reproducible extractor regression before changing generated scheduling records. |
| 2 | `circle.cpp` (380 lines), then geometry headers UI p003 (1,686 lines), as separate unit PRs | Accepted SEG; resolve the VRML edge before claiming the header unit | Complete SEG inline operations and shared shape interfaces; rerun SEG and `poly_ystripes` continuations. |
| 3 | Clipper2 source pure p002 (1,727 lines) | Accepted current engine p001 | Real `ExecuteInternal`, output builders and owned offset bodies; rerun the existing Execute cases through successful polygon operations. |
| 4 | Pinned glibc `pow` backend | Independent of the three current topics; serialize overlapping math integration edits | Replace the angle backend deferral with exact native and wasm results, then rerun angle rounding cases. |

Use the four streams above for the geometry sequence, Clipper2, core/general
utilities and numeric/spatial follow-ups. Use the small units below when a
dependency, review or build lock holds up one of those streams.
Reviews begin only after a concrete implementation head is available.

## Geometry dependency audit

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

The current worker owns engine source lines 1–2,271, source pure p001.
Source pure p002 owns engine lines 2,272–3,337 and `clipper.offset.cpp` lines
1–661. This includes `ExecuteInternal`, `BuildPaths64`, `BuildTree64`,
`BuildPathsD` and `BuildTreeD`. Consequently the first engine unit alone cannot
remove every one of #72's 31 deferred cases or finish polygon booleans.

After p002, port source pure p003 (`clipper.rectclip.cpp`, 1,027 lines), which
is independently schedulable from the accepted core/engine headers. Then port
include pure p002 (1,151 lines: public wrappers, Minkowski and associated
headers), whose hard dependencies include both source p002 and p003.

Validate all four boolean operations, fill rules, holes/tree hierarchy, open
paths, Z handling, collinear preservation, reverse solutions and repeated
execution. Replace #72's temporary coordinate-range restriction only once
precise guards cover the arithmetic reached across both engine units. Keep
the remaining restriction explicit until then. `SHAPE_POLY_SET` still has
geometry prerequisites after Clipper2 is complete.

## Independent small-unit queue

All five units below have satisfied recorded hard dependencies at the snapshot.
Keep one unit per claim/PR, even if one worker handles several sequentially.

| Priority | Unit | Scope and validation focus |
| --- | --- | --- |
| A | `ellipse.cpp` — 127 lines | Wire `ELLIPSE::GetPointAtAngle` in the existing geometry header port; exact angle/rotation arithmetic and degenerate radii. |
| B | `transform.cpp` — 127 lines | Integer transforms, composition/inverse behaviour and exact overflow domains; distinct from `transform_trs.cpp`. |
| C | `arc_construction.cpp` — 207 lines | Degenerate and boundary constructions, exact rounding and existing vector/angle backends. |
| D | `distribute.cpp` — 89 lines | Fewer than three items, negative gaps, fractional/tie rounding, original order and evaluated integer overflow. |
| E | `poisson_disk.cpp` — 143 lines | Reproduce Boost MT19937, distribution algorithms, draw/evaluation order, trig bits and ordered samples. Bound tiny-radius storage/work; spacing alone is insufficient evidence. |

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

Remove #61's fixed 64-level R-tree limit as a separate focused follow-up. Reuse
the explicit-stack/active-node-budget approach demonstrated by closed #60,
without merging its obsolete topic branch. Preserve insertion/removal/query and
cursor order, test defined trees deeper than 64, and check allocation refusal
and interrupted-mutation lifecycle. Audit every remaining depth guard rather
than raising the constant.

For each assignment: audit every owned body and post-deferral continuation;
run exact native difftests and the affected callers, scoped fmt/clippy/tests,
portable wasm checks and unit validation; retain genuine UB refusal controls
separately from successful cases. Report reference toolchain identity, exact
head, compared/deferred/failed counts and remaining gaps. Non-reference local
comparisons are diagnostic evidence. Hosted reference CI and separate Sol 6.1
review gate readiness; preserve partial status wherever required.
