# Clipper2 header remainder (include p002): port, audit and harness limits

Unit: `thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002`
(`clipper.h`, `clipper.minkowski.h`, `clipper.offset.h`, `clipper.rectclip.h`,
`clipper.triangulation.h`, `clipper.version.h` at KiCad 4cf939778df0, Clipper2 2.0.1).
Status: confirmed by local diagnostic comparison; the hosted GCC 13.3 / glibc 2.39 run on the
PR head is the reference.

## What is ported

| Header | Rust | Notes |
| --- | --- | --- |
| `clipper.h` | `kicad_clipper2::clipper` | Every inline body: `BooleanOp` (4), `Intersect`/`Union`/`Difference`/`Xor` (64 and D, `Union` with and without clips), `InflatePaths` (2), `TranslatePath(s)` (template and overloads), `RectClip`/`RectClipLines` (4 each, `RectD` scaling included), `details::PolyPathToPaths64/D`, `PolyPath64ContainsChildren`, `OutlinePolyPath`, `OutlinePolyPath64/D`, `MakePathGeneric`, `GetNext`, `GetPrior`, both `operator<<`, `PolyTreeToPaths64/D`, `CheckPolytreeFullyContainsChildren`, `MakePath` (2), `MakePathD` (2), `MakePathZ`, `MakePathZD`, `TrimCollinear` (2), `Distance`, `Length`, `NearCollinear`, `Ellipse` (2), `SimplifyPath(s)`, `Path2ContainsPath1`, `RDP`, `RamerDouglasPeucker` (2). |
| `clipper.minkowski.h` | `kicad_clipper2::clipper_minkowski` | `detail::Minkowski`, `detail::Union`, `MinkowskiSum`/`MinkowskiDiff` (64 and D). |
| `clipper.offset.h` | `kicad_clipper2::offset` (co-port from PR #96, adopted) | `JoinType`, `EndType`, `DeltaCallback64`, `Group` members, `ClipperOffset` members, constructor defaults, accessors, `Clear`, `ErrorCode`, `SetZCallback`, `SetDeltaCallback`. Audited against the header; the constructor defaults, accessors, `Clear` and `ErrorCode` are now compared (`offset_members`). `~ClipperOffset() { Clear(); }` has no observable effect; Rust drops the object. |
| `clipper.rectclip.h` | `kicad_clipper2::rectclip` (co-port from PR #77, adopted) | `Location` order, `OutPt2` (arena indices), `RectClip64(const Rect64&)` member initialisation order (`rect_`, `AsPath`, `MidPoint`), `RectClipLines64`. Audited; reached through every `RectClip`/`RectClipLines` case, including the `MidPoint` overflow refusal. |
| `clipper.triangulation.h` | `kicad_clipper2::clipper_triangulation` | `TriangulateResult`; both `Triangulate` declarations return `Unported` naming `thirdparty.clipper2.clipper2lib.src.thirdparty.ui.p001` (blocked). KiCad's `thirdparty/clipper2/CMakeLists.txt` does not compile `clipper.triangulation.cpp`, so a KiCad caller would not link; no oracle request exists. |
| `clipper.version.h` | `CLIPPER2_VERSION` | `"2.0.1"`, compared. |

Supporting additions outside the module: `kicad_math::reference_libm::pow` forwards to the
accepted private glibc 2.39 `pow` body (`std::pow(10, precision)`); no new backend.
`kicad_clipper2::stream::insert_u64` models `operator<<(unsigned long)` for `size_t`.

## Wiring

`Ellipse<int64_t>` (single points with round joins in `ClipperOffset::DoGroupOffset`) and
`Path2ContainsPath1<int64_t>` (the cleaned-path fallback of the engine's
`Path2ContainsPath1`/`Path2ContainsPath1Indexed`) are now real calls. Measured against the same
oracle (local diagnostic run):

| Caller | Cases | Before (accepted `rust`, 4a8cb2ead4) | After |
| --- | ---: | --- | --- |
| src p002 | 1,657 | 1,635 exact, 22 deferred (16 `Path2ContainsPath1`, 6 `Ellipse`) | 1,657 exact, 0 deferred |
| src p001 | 907 | 901 exact, 6 deferred (`Path2ContainsPath1`) | 907 exact, 0 deferred |
| include p001 | 1,587 | 1,587 exact | 1,587 exact |
| src p003 | 1,574 | 1,574 exact | 1,574 exact |

The 28 formerly deferred cases (src p001: `engine/ring/split/{square,triangle,figure8,loop,concave}/1`,
`engine/ring/contains_ambiguous`; src p002: `random-007/141/286`, `clipd-random-25`,
`offset-single-round`, `offset-callback-set`, `offset-random-063/153`, `tree-split-*` (12),
`offset-throw-random-05/08`) now compare exactly. Those units' records, READMEs and findings
still describe the deferrals; they are not owned here and are left for their owners or the
parent to update. Two of their unit tests and the src p001 shim's corpus test asserted the
deferrals and were updated with the wiring.

## Oracle

`port/oracle/shims/clipper2_include_p002.cpp` includes the genuine headers and links the pinned
archive. Undefined behaviour is decided by a forked UBSan/`_GLIBCXX_ASSERTIONS` replay of the
same request evaluation compiled against the pinned `clipper.engine.cpp`, `clipper.offset.cpp`
and `clipper.rectclip.cpp` (renamed namespace), as in the src p003 adapter. The Rust port
refuses the same inputs with `ClipperError::Undefined` (checked integer arithmetic, checked
conversions, checked indexing). 25 controls exercise this: int64 overflow in `TranslatePath`,
`TrimCollinear`, `Distance`, `Length`, `NearCollinear`, `SimplifyPath`, `RDP`, the
`Path2ContainsPath1` midpoint and `PointInPolygon` inside `CheckPolytreeFullyContainsChildren`;
out-of-range or NaN conversions in `Ellipse` (steps and points), `MakePathZD`, `ScaleRect`,
`ScalePath` (Minkowski and `TrimCollinear`) and `ClipperD`; `RectClip64`'s `MidPoint`;
`GetNext`/`GetPrior` overruns; an empty open path and a huge singleton in `InflatePaths`.

## Harness limits (refused identically, never compared)

- Engine domain: `int64_t` boolean coordinates outside `±MAX_COORD` (as the earlier Clipper2
  adapters).
- Offset domain: `InflatePaths`/`offset_members` coordinates, deltas, miter limits and arc
  tolerances outside `±2^40` or non-finite (for `PathsD` after scaling); round offsets above
  65,536 arc steps per circle or 2^22 points. Consequence: `InflatePaths(PathsD)`'s
  `ScalePaths` range exception (`range_error_i`) is not reached by the corpus.
- Minkowski: coordinates (or scaled coordinates below 2^63) beyond half the engine domain;
  at most 2^14 quadrilaterals.
- `RamerDouglasPeucker`: NaN epsilon refused (its recursion repeats a call on nearly every path;
  the Rust port detects a call whose arguments repeat an active ancestor and refuses it as
  undefined, unit-tested only); at most 1,024 points per path, because the C++ copies the path
  into every recursive frame.
- `Ellipse`: at most 2^20 points (an estimate in both adapters; inputs whose step count would
  be an undefined conversion are refused by this guard first when the estimate is finite).
- Sizes: 2^16 points per request; 4,096 points per `SimplifyPath` path, tree nodes, flags and
  `MakePath` values; `MakePath`'s array forms are compiled for N in {2, 4, 6, 8} (pairs) and
  {3, 6, 9} (triples) only; `MakePath` element types `int64_t`, `int32_t`, `uint64_t`,
  `uint8_t`, `double`.

## Rust-side deviations of form (not of value)

- Wrappers that construct `ClipperOffset` or `RectClip64` take the host budget those co-ported
  classes require; `Ellipse`, `RDP` and Minkowski take explicit point/call/quad bounds
  (`ResourceLimit` beyond them, never reached inside the guards).
- Trees are addressed as `(PolyTree, PolyPathId)`; the `PathsD`/`PolyTreeD` boolean wrapper
  works on the tree root, as the engine's `Execute` port does.
- Recursions (`PolyPathToPaths64`, `PolyPath64ContainsChildren`, `OutlinePolyPath64/D`, `RDP`)
  run on explicit stacks in the reference's order.
- The `MakePath` `static_assert`s are compile-time `const` assertions on the array length;
  the integral-only overloads take a `PathInteger` bound.

## Coverage

C++ line coverage of the headers by the corpus was not measured. Every body above is called by
at least one compared case or (for `GetNext`/`GetPrior` overruns and the other undefined
branches) one control.
