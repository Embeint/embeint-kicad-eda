# Clipper2 rectangle clipping source port (p003)

Unit: `thirdparty.clipper2.clipper2lib.src.thirdparty.pure.p003`
(`thirdparty/clipper2/Clipper2Lib/src/clipper.rectclip.cpp:1:1027`, Clipper2 2.0.1 at the
KiCad pin). Rust: `rust/kicad-clipper2/src/rectclip.rs`. Confirmation state: measured on the
oracle built from this branch (WSL, GCC 15.2, Release, `-ffp-contract=off`).

## Ownership and interfaces

- Every function body in the file is ported; none is deferred and no `UNPORTED` tag exists.
- The only hard dependency is the accepted core header unit
  (`...include.clipper2.thirdparty.pure.p001`): `Point64`, `Rect64` (`AsPath`, `MidPoint`,
  `IsEmpty`, `Intersects`, `Contains`), `GetBounds`, `CrossProductSign`, `IsCollinear`,
  `GetLineIntersectPt`, `PointInPolygon`. They are called, not reimplemented.
- Minimum co-port of `clipper.rectclip.h` (owned by
  `...include.clipper2.thirdparty.pure.p002`, unclaimed): `enum class Location`
  (`rectclip::Location`), `OutPt2` (private, arena indices), and the two inline
  constructors (`RectClip64::new`, `RectClipLines64::new`, member order `rect_`,
  `rect_as_path_`, `rect_mp_`). The p002 wrappers `RectClip`/`RectClipLines` in
  `clipper.h` need exactly `RectClip64::new(&Rect64, RectClipBudget)`,
  `RectClipLines64::new(..)` and `execute(&[Path64]) -> Result<Paths64>`; the `RectD`
  overloads additionally need p001's `ScaleRect`/`ScalePaths`, which exist.
- No KiCad application source calls Rect clipping directly (searched `libs`, `common`,
  `pcbnew`, `eeschema`, `include`); the callers are the p002 header wrappers.

## Undefined behaviour

| Expression | Trigger | Port | Control |
| --- | --- | --- | --- |
| `Rect64::MidPoint` in the constructor | `left + right` or `top + bottom` overflows | core `mid_point` refuses | `ub-midpoint-x`, `ub-midpoint-y-lines`, `ub-midpoint-near-max` |
| `CrossProductSign` differences | coordinates whose differences overflow `int64_t` | core refuses | `ub-segment-intersection`, `ub-is-clockwise`, `ub-path1-contains-path2`, `ub-clip-wide`, `ub-lines-wide`, `ub-clip-span-crossing` |
| `rectPath[n]` in `GetIntersection` | a rectangle path shorter than 4 | refused at the subscript | `ub-get-intersection-short-path`, `ub-get-intersection-empty-path` |
| `rect_as_path_[Location::Inside]` in `AddCorner` | a corner walk reaching `Inside` | refused at the subscript | none found: 70,000 grid-snapped degenerate inputs never reached it |

The oracle decides each of these with the pinned source itself (UBSan trap mode and
`_GLIBCXX_ASSERTIONS` in a forked replay), so no coordinate range is fenced. Defined inputs
across the whole `int64_t` span (for example `[I64_MIN+10, .., I64_MAX-10]` rectangles with
inside or wholly outside paths, and `MAX_COORD`-sized paths) are corpus cases.

Loops that the C++ would not end (a corner walk towards `Inside`, `TidyEdges` revisiting
the same slots) are bounded by the host `RectClipBudget` (`ResourceLimit`, reported as
`harness limit`). No input reaching such a loop was found; the budget is exercised by Rust
unit tests only (`budgets_are_resource_limits`), which is a harness bound, not a
reference result.

## Evidence

Commands (WSL, own tree `~/kicad-port-rectclip`, build lock, `-j8`): `port/oracle/build.sh`
(ctest passed), `kicad-difftest compare --partial`, `compare_controls.py`, wasm
`corpus_parity` under Node 22; on Windows: `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test`, `cargo check --target wasm32-unknown-unknown`, `generate_cases.py --check`.

- Corpus: 1574 cases, 1574 compared exactly, 0 deferred, 0 failures (difftest status green).
  The record declares `partial`: the global gate checks a `green` record strictly against
  a golden, and adopting one is a separate reviewed step.
  667 `Execute` cases (335 polygon, 332 line), 564 with nonempty output, 970 output paths;
  907 helper cases.
- Controls: 11, refused identically by both programs.
- Differential fuzz (not committed, fixed seeds 1 and 2): 70,000 random grid-snapped
  polygon/line requests (rectangles 1-6 wide, points on, beside and between the edges,
  1-3 calls per object): 0 mismatches, 0 refusals.
- Native Rust output on Windows and Linux is byte-identical; wasm replay matches.
- The accepted p001 corpus re-run against this oracle: 1556 compared, 31 deferred (its
  existing engine deferrals), 0 failures; 83 controls identical.
- gcov of the pinned source over the 667 `Execute` cases: 98.8% of 575 lines, 99.4% of
  647 branches executed, 86.1% taken (58 of the untaken are exception edges).

| Operation | Bodies / branches | Successful result checked | Remaining gap |
| --- | --- | --- | --- |
| `RectClip64::Execute` | empty rect, `<3` points, outside, inside copy, `ExecuteInternal`, `CheckEdges`, `TidyEdges` split and rejoin, `GetPath` | output paths with `z` across 335 cases and multi-call objects | lines 445-446 and 455-463 (empty path, every point on the boundary) are unreachable from `Execute`, whose bounds filter removes those paths first |
| `RectClipLines64::Execute` | empty rect, outside, `<2` points, entering, exiting, passing through, boundary-only | 332 cases | line 944 `IsEmpty` re-check unreachable from `Execute` |
| `GetSegmentIntersection` and the other helpers | every branch and argument order | 907 direct cases | the `GetIntersection` default case's final `false` (line 202) is not reached by the `Execute` cases (gcov measured `Execute` only); direct helper cases exercise every location |
| `UnlinkOp`, `UnlinkOpBack`, `AddToEdge`, `UncoupleEdge`, `SetNewOwner` | pointer helpers | through `Execute` only | a null `edge` in `UncoupleEdge` and a single-node `UnlinkOp` are not reached |
