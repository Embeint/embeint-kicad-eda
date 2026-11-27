# Clipper2 engine remainder and offset port (src p002)

Unit: `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p002`
(`thirdparty/clipper2/Clipper2Lib/src/clipper.engine.cpp` lines 2272-3337 and
`thirdparty/clipper2/Clipper2Lib/src/clipper.offset.cpp` lines 1-661), PR #96.

## What was ported and wired

- `rust/kicad-clipper2/src/engine_p002.rs`: every body of lines 2272-3337 on engine p001's
  arenas, minima and vertex stores. `ClipperBase::Split`, `CheckJoinLeft`, `CheckJoinRight`
  and `SwapPositionsInAEL` replace src p001's typed deferrals;
  `ClipperBase::ExecuteInternal`, `Clipper64::BuildPaths64/BuildTree64` and
  `ClipperD::BuildPathsD/BuildTreeD` replace the header unit's. Engine p001 exposes only
  crate-private access (record accessors, walks, checked arithmetic, `stable_sort`).
- `rust/kicad-clipper2/src/offset.rs`: every body of `clipper.offset.cpp`, with the minimum
  `clipper.offset.h` co-port recorded in the include p002 record.
- `ProcessIntersectList`'s `std::sort` is not stable; equal intersection points keep the
  order libstdc++'s introsort leaves them in. The port uses the accepted
  `kicad_core::stdalgo::sort` (introsort, threshold 16, median of three, heap fallback), so
  the order matches the oracle's.
- `CheckSplitOwner` and `RecursiveCheckOwners` recurse in the reference. The port runs them
  on explicit, fallibly grown stacks with the same visiting order and the same resumption
  after each call. A stack deeper than twice (CheckSplitOwner) or once (RecursiveCheckOwners)
  the output records can only be a cycle, on which the reference recurses forever; it is
  refused as a non-terminating walk.
- `ClipperOffset` uses `std::sin`, `std::cos`, `std::acos` and `std::atan2`. The accepted
  glibc 2.39 baseline SSE2 bodies in kicad-math were crate-private; with the parent's
  agreement a facade, `kicad_math::reference_libm`, now forwards to them unchanged (commit
  "port(feat): expose the exact glibc trig backends of kicad-math"). `std::sqrt` and
  `std::ceil` are exact in Rust; `std::min` keeps its NaN operand order.
- `ClipperOffset::ExecuteInternal` binds `ZCB` to the object (`std::bind(&ZCB, this, ...)`), so
  the clipper reads `zCallback64_` at call time. The port shares it through
  `Rc<Cell<Option<ZCallback64>>>`; the proxy takes the callback out for the call and puts it
  back, which no other code can observe.
- `Execute(DeltaCallback64, paths)` assigns the member permanently, as the C++ does; later
  `Execute(delta, ...)` calls keep using it (checked natively and in the corpus).

## Remaining deferrals

Both belong to `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`
(`clipper.h`) and return `Unported` at the call, after every earlier mutation:

- `Clipper2Lib::Ellipse<int64_t>`: a single-point path offset with round joins and a delta
  of at least 1 (`DoGroupOffset`); the continuation (z fill, solution push) is ported.
- `Clipper2Lib::Path2ContainsPath1<int64_t>`: the cleaned-path fallback of the OutPt
  containment tests (src p001), reached from `ProcessHorzJoins`, `CheckSplitOwner` and
  `RecursiveCheckOwners` in polytree mode when the point test is ambiguous.

## Harness limits (not source omissions)

- Callback exceptions are not modelled: the accepted `ZCallback64`, `ZCallbackD` and the
  co-ported `DeltaCallback64` return no error, so a throwing C++ callback has no Rust
  counterpart.
- Non-finite deltas, delta-table values, miter limits and arc tolerances are refused by both
  adapters before execution. This also excludes one defined reference behaviour (a NaN delta
  on single points only, which `group_delta_ < 1` skips); every other non-finite delta
  reaches an undefined `double` to integer conversion.
- Clipping coordinates are limited to Clipper2's declared `MAX_COORD` and offset coordinates
  and deltas to `±2^40`; round offsets must stay within 65,536 arc steps per circle and 2^22
  points (the reference's formula with the pinned `acos`). Both adapters refuse the same
  requests identically; the Rust host point budget itself is checked natively.
- The private engine bodies are compared through the public Clipper64/ClipperD/ClipperOffset
  operations (outputs, trees, success flags, error codes and Z callback traces), not through
  step-by-step state dumps; `GetHorzExtendedHorzSeg`, which the reference never calls, is
  ported but not reached.
- The local oracle runs GCC 15 / glibc 2.43 in WSL (diagnostic); the target is the hosted
  GCC 13.3 / glibc 2.39 runner. The unit stays partial without a golden until that evidence
  and a fresh independent review exist.

## Measured (local diagnostic oracle)

WSL GCC 15.2 / glibc 2.43 with `glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`, the oracle
built by `port/oracle/build.sh` from raw-blob exports of the PR tree (all 15 oracle ctests
pass):

| Unit | Cases | Exact | Deferred | Failures | Controls |
| --- | ---: | ---: | ---: | ---: | ---: |
| src p002 (this unit) | 1,557 | 1,537 | 20 | 0 | 23 (each with an identical continuation) |
| src p001 | 907 | 901 (was 881) | 6 (was 26) | 0 | 49 |
| include p001 | 1,587 | 1,587 (was 1,573) | 0 (was 14) | 0 | 83 |
| src p003 | 1,574 | 1,574 | 0 | 0 | 11 |

The 20 deferrals are 16 `Path2ContainsPath1<int64_t>` and 4 `Ellipse<int64_t>`; src p001's
remaining 6 are all `Path2ContainsPath1<int64_t>`. Line coverage of the owned ranges by the
corpus (gcov, `-O0` coverage build of the same adapter): `clipper.engine.cpp` 2272-3337
574/583, `clipper.offset.cpp` 332/335. Unreached: the four `AddNewIntersectNode`
rounding-correction assignments (2544-2552), the `MoveSplits` append (2449) and
`CheckSplitOwner`'s early returns (3122, 3128, 3134); in the offset file, line 601 is the
`error_code_` branch this version never sets, and 300 and 614 are continuation lines of
statements that ran. These paths are ported and audited by inspection, not by the corpus.
