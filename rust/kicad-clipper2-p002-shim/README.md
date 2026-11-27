# Clipper2 engine remainder and offset shim (clipper.engine.cpp lines 2272-3337, clipper.offset.cpp)

Native JSONL adapter for `kicad_clipper2::engine_p002` and `kicad_clipper2::offset`, the port of
unit `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002`. It answers the same requests as
the pinned C++ adapter `port/oracle/shims/clipper2_engine_p002.cpp`.

## The oracle

Several of these bodies are file-local (`inline`/`static`) functions. The C++ adapter compiles
the pinned, unmodified `clipper.engine.cpp` and `clipper.offset.cpp` into its own translation
unit with the identifier `Clipper2Lib` renamed (so its symbols never meet the linked archive's
or the other Clipper2 adapters'), with the oracle's `-fno-fast-math -ffp-contract=off` and
KiCad's public `USINGZ`. It is linked after every earlier adapter and the policy source, so no
earlier weak template selection changes. The engine-remainder bodies are private
`ClipperBase` members reached through the public `Clipper64`/`ClipperD`/`ClipperOffset`
operations, which call every one of them; the offset file's local helpers are also called
directly.

## Protocol

`{"op": ...}` inside the shared envelope. Points are `[x, y, z]`, doubles 16 lowercase hex
digits, trees a preorder list of `{"parent", "hole", "polygon"}`.

- `clip64`: subjects, open subjects and clips, clip type and fill rule (enumerator values),
  `PreserveCollinear`, `ReverseSolution`, `PipIndexThreshold`, a Z recorder (`z`: `none` or
  `recorder`, which logs each call's edges and point and sets `pt.z = 1000000 + calls`), the
  output form (`paths`, `paths_open`, `tree`, `tree_open`) and one or two executions on the
  same object. Replies: each run's success flag, closed and open paths, tree and error code,
  and the Z calls.
- `clipd`: the same for `ClipperD(precision)` on double paths; a `Clipper2Exception` (precision
  or coordinate range) is a compared result `{"exception": what}`.
- `offset`: groups added with `AddPath` or `AddPaths`, miter limit, arc tolerance,
  `PreserveCollinear`, `ReverseSolution`, a Z recorder, an optional delta-callback table (the
  callback returns `table[calls % len]` and logs `[curr, prev, path size, normals size]`),
  whether it is assigned with `SetDeltaCallback`, and up to four executions (`paths`, `tree`
  or `callback` = `Execute(DeltaCallback64, paths)`) on the same object. Optional `throws`
  (`{"delta": [calls], "z": [calls]}`, 0-based cumulative call indices) makes the recording
  callbacks throw at those calls after logging them (the Z recorder before setting `pt.z`).
  A throw ends only that execution: its reply adds `thrown` (`{"callback", "call"}`) and the
  retained group count to the partial paths/tree and error code the unwinding left, and the
  next execution runs on the same object. Replies: the groups as constructed, each
  execution's paths/tree/error code, the final normals, and the callback logs.
- `helper`: `Hypot`, `GetUnitNormal`, `NormalizeVector`, `GetAvgUnitVector`, `AlmostZero`
  (default or given epsilon), `GetPerpendic`, `GetPerpendicD`, `NegatePath`, `IsClosedPath`
  and `GetLowestClosedPathInfo` (with the caller's initial `is_neg_area`, which the reference
  only writes when it finds a lowest point).

## Guards and refusals

Both programs refuse with identical envelopes. Parser/resource guards run before execution;
the source-domain guards also inspect constructed groups and actual callback values:

- Input domain (`invalid input: domain`): clipping coordinates outside Clipper2's declared
  `-MAX_COORD..MAX_COORD`; offset coordinates outside `±2^40`; NaN `ClipperD` coordinates.
  Deltas, table values, miter limit and arc tolerance must be finite within `±2^40`
  (`invalid input: delta`). Non-finite deltas are therefore not compared, including the
  defined case of a NaN delta on single points only (skipped); this is a harness limit.
- Resource guard (`invalid input: arc steps`): with round joins or ends, every delta the
  request can produce must give at most 65,536 arc steps per circle (the reference's own
  formula, with the pinned `acos`) and at most 2^22 points in all. Within the guard the
  Rust host budgets are never reached; a native test checks the budget itself.
- Undefined offset branches (`undefined behaviour: offset`): significant offsets of empty
  non-Polygon paths, and non-Round singletons whose effective positive delta makes
  `ceil(abs_delta)` exceed `INT_MAX`. Insignificant offsets remain admitted. Singleton
  checks follow duplicate stripping, group reversal, default-delta mutation across groups,
  and the actual callback entry, including callbacks retained by later executions.
- Malformed requests: fields, enumerators, sizes, callback combinations.

Callback exceptions are modelled. The C++ adapter throws a private `CALLBACK_THROW` from
its recording callbacks and catches it around each `Execute`; the Rust adapter returns
`ClipperError::Callback` from `DeltaCallback64` and `FallibleZCallback64`. Both report what
the exception left: the partial paths a delta callback exception leaves appended, the
solution the finishing union cleared (or the tree it had built), error code, retained
groups, and, through later executions and the logs, the retained callbacks, call counts and
normals. `throws` requires a delta table for delta throws and the Z recorder for Z throws
(`invalid input: throws`), and indices below 2^20. The pre-execution guard for empty open
paths refuses the whole request even when a throw would have ended the execution before
the path, so throw cases avoid empty open paths.

## Deferrals

`Ellipse<int64_t>` (a single point offset with round joins) and the cleaned-path fallback
`Path2ContainsPath1<int64_t>` (ambiguous polytree containment) belong to unit
`thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002` and return typed
deferrals; the C++ runs the real bodies, and those cases are deferred, not compared.

## Evidence

- `generate_cases.py` writes the corpus (`port/cases/<unit>.jsonl`) and the controls
  (`port/oracle/fixtures/clipper2-engine-p002-controls.jsonl`); `--check` verifies both.
  Star input coordinates use decimal Taylor evaluation rounded once to binary64, following
  the accepted arc generator, so input generation does not depend on the host libm.
  These generated coordinates are inputs only; every expected result comes from C++.
- `kicad-difftest compare --partial` compares every case exactly; `compare_controls.py`
  requires identical refusals, each followed by an identical successful continuation;
  `examples/corpus_parity.rs` replays both in import-free wasm.
