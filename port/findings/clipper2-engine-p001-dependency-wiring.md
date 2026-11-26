<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Clipper2 engine source p001: header deferrals resolved and the remaining engine-domain limit

Unit: `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p001` (`clipper.engine.cpp`
lines 1-2271), wiring the merged header unit
`thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001` (#72).
State: confirmed locally against the pinned oracle (WSL, own build tree); hosted counts in the
draft PR.

## Header-unit callers rerun

The header unit's corpus (1,587 cases) was rerun against the genuine oracle after the wiring.
Its 31 deferrals become 14:

| Header operation | Cases | Before | After |
| --- | ---: | --- | --- |
| `clipperd_add` (`ClipperD::Add*` -> `ClipperBase::AddPaths`) | 21 | 12 deferred (AddPaths) | all exact results |
| `stripe_index` (`StripeIndex::BuildFromPath`/`PointInPolygon`) | 5 | 5 deferred | all exact results |
| `clipper64_execute`, `clipperd_execute` | 17 | 14 deferred (AddPaths), 3 `Clipper2Exception` results | 14 deferred at `ClipperBase::ExecuteInternal` (src p002), the same 3 exception results |

1,573 header cases compare exactly; 14 remain deferred, all to src p002. The 83 header
controls are still refused identically by both programs.

## The Clipper64 engine-domain restriction stays

`clipper64_execute` refuses coordinates outside `MIN_COORD..MAX_COORD` before the pinned
engine runs. This unit does not lift it:

- `ClipperBase::AddPaths`/`AddPaths_`/`Reset` only compare coordinates, and the p001 corpus
  now runs them on the whole `int64_t` range (`engine/add_reset/extreme`).
- `Execute` reaches `ExecuteInternal` and the sweep in lines 2272 onward (`DoIntersections`,
  `DoHorizontal`, `DoTopOfScanbeam`, the joins, the output builders), which call `TopX`,
  `GetLineIntersectPt` and edge arithmetic on caller coordinates. Their precise guards belong
  to src p002, which owns those bodies; until then the C++ adapter cannot run `Execute` safely
  outside the declared range.

Next step: src p002 provides source-derived guards for its sweep arithmetic (this unit supplies
the guards for `GetDx`, `TopX`, `AreaTriangle`, `PtsReallyClose` and `StripeIndex`), then
replaces the header adapter's engine-domain check with them.
