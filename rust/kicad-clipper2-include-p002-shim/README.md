# Clipper2 clipper.h and Minkowski header shim (include unit p002)

Native JSONL adapter for `kicad_clipper2::clipper`, `clipper_minkowski` and
`clipper_triangulation` and the `clipper.offset.h` members, the port of unit
`thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`. It answers the same
requests as the pinned C++ adapter `port/oracle/shims/clipper2_include_p002.cpp`.

## The oracle

The headers are inline bodies, so the C++ adapter includes the pinned, unmodified
`clipper2/clipper.h` (with `clipper.version.h`) under KiCad's public `USINGZ`, links the pinned
clipper2 archive (whose `Clipper64`, `ClipperD`, `ClipperOffset` and `RectClip64` members the
wrappers call) and calls every overload, including the default-argument forms. Its request
evaluation (`port/oracle/reference/clipper2_include_p002_ops.inc`) is compiled a second time
against the pinned `clipper.engine.cpp`, `clipper.offset.cpp` and `clipper.rectclip.cpp` in a
renamed namespace with UBSan in trap mode and `_GLIBCXX_ASSERTIONS`; every request runs on that
copy first in a forked child. A trap, failed assertion or fault there refuses the request as
`undefined behaviour: <op>`; a child beyond the replay time (20 s) or memory (4 GiB) limit
refuses it as `harness limit: <op>`. Only a clean replay lets the genuine call answer. The
adapter is linked after every earlier adapter, so no earlier weak template selection changes.

## Protocol

`{"op": ...}` inside the shared envelope. Points are `[x, y, z]`, doubles 16 lowercase hex
digits, `type` is `i64` or `f64`, trees a preorder list of `{"parent", "hole", "polygon"}`.
A `null` optional argument selects the C++ default argument (only a suffix may be omitted).
Calls that may throw a `Clipper2Exception` reply `{"<value>": ..., "exception": null}` or
`{"<value>": null, "exception": what}`.

- `boolean`: `BooleanOp` (paths and tree), `Intersect`, `Union` (two forms), `Difference`, `Xor`;
  `prefill` polygons are added to the tree first, so the reply shows when it is replaced or
  cleared.
- `inflate`: `InflatePaths` (`Paths64` and `PathsD` forms).
- `offset_members`: the `ClipperOffset` constructor (0 to 4 arguments), setters, `AddPath(s)`,
  `Clear`, getters, `ErrorCode` and an optional `Execute`.
- `translate`, `rectclip` (`RectClip`/`RectClipLines`, `Rect64`/`RectD`, path or paths form).
- `polytree`: `PolyTreeToPaths64/D`, `details::PolyPathToPaths64/D`,
  `CheckPolytreeFullyContainsChildren`, `details::PolyPath64ContainsChildren` and the
  `operator<<` insertions (with an optional stream state), on any node of a built tree;
  `outline`: `details::OutlinePolyPath`.
- `make_path`: `MakePath`, `MakePathD` (vector and array forms), `MakePathZ`, `MakePathZD`
  for `int64_t`, `int32_t`, `uint64_t`, `uint8_t` and `double` elements.
- `trim`, `distance`, `length`, `near_collinear`, `ellipse` (center and rect forms),
  `simplify`, `contains` (`Path2ContainsPath1`), `rdp`, `minkowski`, `detail`
  (`details::GetNext`/`GetPrior`), `version`.

## Guards and refusals

Both programs refuse with identical envelopes, before any reference call:

- Input domain (`invalid input: domain`): boolean `int64_t` coordinates outside Clipper2's
  `-MAX_COORD..MAX_COORD`; offset coordinates, deltas, miter limits and arc tolerances outside
  `±2^40` or non-finite, for `PathsD` after scaling (so `ScalePaths`' range exception is beyond
  the harness domain); Minkowski coordinates (or scaled coordinates below 2^63) beyond half the
  engine domain.
- Resource guards: round offsets at most 65,536 arc steps per circle and 2^22 points
  (`arc steps`, as the src p002 adapter); `Ellipse` at most 2^20 points (`steps`); at most
  2^14 Minkowski quadrilaterals, 1,024 points per `RamerDouglasPeucker` path (it copies the
  path into every recursive call), 4,096 per `SimplifyPath` path, 4,096 tree nodes, flags and
  `MakePath` values, and 2^16 points per request (`size`).
- `RamerDouglasPeucker` with a NaN epsilon (`epsilon`): its recursion does not end on nearly
  every path; the port refuses the repeating call as undefined, the harness refuses the input.
- Malformed requests: fields, enumerators, defaults, sizes, element ranges.

Everything else, including undefined reference evaluation, is decided per request by the
replay; the Rust port refuses the same inputs with `ClipperError::Undefined`.

## Deferrals

None that the corpus reaches. `Triangulate` is declared by `clipper.triangulation.h` but its
body (`clipper.triangulation.cpp`, unit `thirdparty.clipper2.clipper2lib.src.thirdparty.ui.p001`)
is not compiled by KiCad, so it returns a typed deferral and has no request.

## Evidence

- `generate_cases.py` writes the corpus (`port/cases/<unit>.jsonl`) and the controls
  (`port/oracle/fixtures/clipper2-include-p002-controls.jsonl`); `--check` verifies both. All
  inputs are generated; every expected result comes from C++.
- `kicad-difftest compare --partial` compares every case exactly; `compare_controls.py`
  requires identical refusals, each followed by an identical successful continuation (a single
  point offset with round joins, through `Ellipse`); `examples/corpus_parity.rs` replays both in
  import-free wasm.
