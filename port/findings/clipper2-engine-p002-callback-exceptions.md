# Clipper2 offset callback exceptions and retained state

This addendum resolves the callback finding recorded as unresolved in
`clipper2-engine-p002-review-followup.md` (Sol 6.1 review of PR #96 at
`6234371e6f9f2eb9ef7ba743eb872287ded1e4b7`) and supersedes the `Rc<Cell<...>>`
description of `zCallback64_` in `clipper2-engine-p002-port.md`.

## What the pinned C++ does

`DeltaCallback64` and `ZCallback64` are `std::function`s and may throw. Neither
`clipper.offset.cpp` nor `clipper.engine.cpp` catches anything, so an exception unwinds
straight out of `ClipperOffset::Execute`:

- From the delta callback (`OffsetPoint`, the open-path caps, the singleton branch of
  `DoGroupOffset`): `group_delta_` keeps its previous value (the assignment never happens).
  `groups_` were iterated in place and stay intact. `Execute(DeltaCallback64, ...)` had
  already assigned `deltaCallback64_`, which stays assigned. `norms` keep the current path's
  normals. Paths already emplaced into `*solution` remain: for `Execute(delta, Paths64&)` that
  is the caller's vector; for `Execute(delta, PolyTree64&)` it is the internal vector (leaked),
  and the tree stays cleared.
- From the Z callback (through `ZCB`, bound into the local `Clipper64`): the local clipper is
  destroyed without `CleanUp`. `Clipper64::Execute` had cleared the caller's paths (or the
  tree is cleared and built only by `BuildTree64`), so the caller sees what the union had
  built before the throw. `zCallback64_` stays assigned; `groups_` and `norms` are unchanged.
- `error_code_` stays 0 in both cases. A later `Execute` on the same object uses the retained
  groups and callbacks.

## Port design

- `ClipperError::Callback(u64)` is the typed exception; callbacks return it (or any other
  `ClipperError`) and it propagates unchanged.
- `offset::DeltaCallback64` returns `Result<f64>`. Every call is `self.group_delta = cb(...)?`,
  so an error leaves `group_delta` unassigned, as in the C++.
- `offset::FallibleZCallback64` returns `Result<()>`;
  `ClipperOffset::set_fallible_z_callback` assigns one and `set_z_callback` wraps a
  never-throwing `ZCallback64`. `zCallback64_` is `Rc<RefCell<Option<_>>>`, borrowed (never
  taken) for each call, so it remains assigned after an error or a native unwind.
- `ExecuteInternal` binds the shared cell into the local `Clipper64` as the additive engine
  slot `ZCallbackSlot::OffsetProxy` (kind `ZCallbackKind::OffsetProxy`). The engine's
  `call_z_callback` already returns `Result`, and every call site propagates with `?`, so the
  union stops at the callback's exception. The infallible `ZCallbackSlot::User` used by
  `Clipper64::SetZCallback` is unchanged.
- `groups_` is `Rc<Vec<Group>>`. An execution reads a cloned handle instead of moving the
  groups out, so they stay in the object on any exit; `AddPath`/`AddPaths` mutate through
  `Rc::get_mut`, which is always unique outside an execution.

## Evidence

The adapters' optional `throws` request makes the recording callbacks throw at chosen
cumulative call indices; each execution reports the partial result, error code and retained
group count, and later executions show recovery. 86 cases (`offset-throw-*`) are appended
with a separate fixed-seed generator, so the first 1,571 inputs are byte-identical to the
reviewed corpus and their native Rust replies are unchanged. They sweep delta exceptions over
every call of a polygon, open path and singleton group, set and execute-assigned callbacks,
paths and trees, joined and reversed groups, Z exceptions in the union on paths and trees,
combined exceptions, exceptions on every execution, early returns (insignificant offsets,
skipped singletons, almost-zero deltas, empty solutions without a union), and 48 random
scenes. Natively they produce 123 thrown executions (93 delta, 30 Z; 29 with non-empty
partial paths). Four refusal controls cover malformed `throws`.

Native tests (`offset/tests.rs`) reproduce the review's scenarios: after a delta callback
error the partial path, group and normals remain and recovery offsets every group; after a Z
callback error the callback is called again; after a native unwind out of either callback
(`catch_unwind`) the groups and the Z callback remain.

A local smoke oracle (only the p002 adapter, WSL GCC 15.2 / glibc 2.43 with the SSE2 hwcaps
tunables, UBSan traps) matched the native Rust shim on all 1,635 non-deferred cases, with 22
genuine include-p002 deferrals (2 new), and all 48 controls; the import-free wasm replay
agreed. This is diagnostic only: authoritative reference evidence comes from the hosted
`rust-clipper2-p002-live` workflow on the locked GCC 13.3 / glibc 2.39 runtime.

The unit stays partial (the include-p002 deferrals remain), draft and without a golden until
that run and a fresh independent review.
