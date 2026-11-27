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
conversions, checked indexing). 36 controls exercise this: int64 overflow in `TranslatePath`,
`TrimCollinear`, `Distance`, `Length`, `NearCollinear`, `SimplifyPath`, `RDP`, the
`Path2ContainsPath1` midpoint and `PointInPolygon` inside `CheckPolytreeFullyContainsChildren`;
out-of-range or NaN conversions in `Ellipse` (steps and points), `MakePathZD`, `ScaleRect`,
`ScalePath` (Minkowski and `TrimCollinear`) and `ClipperD`; `RectClip64`'s `MidPoint`;
`GetNext`/`GetPrior` overruns; an empty open path and a huge singleton in `InflatePaths`.

### `std::vector<bool>` bounds on GCC 13.3

`_GLIBCXX_ASSERTIONS` does not check `std::vector<bool>::operator[]` in GCC 13.3's libstdc++ (its
body is `return begin()[__n];`; GCC 15's adds `__glibcxx_requires_subscript`), and UBSan cannot
see a read of a spare bit inside an allocated word. On the hosted GCC 13.3 the replay therefore
finished `control-ub-detail-next` (`current=1`, `high=2`, flags all set: `GetNext` reads
`flags[3]`) cleanly and the genuine call would have answered. The replay copy (only; the macro
`INCLUDE_P002_REPLAY` in `clipper2_include_p002_guard.cpp`) now first reads the flags at the
indices `details::GetNext` and `details::GetPrior` read, in their order, and traps on one outside
the vector, as GCC 15's assertion aborts. The genuine evaluation is unchanged. Ten more controls
(both functions, every flag set, sizes 1, 3, 63, 64 and 65: spare bits of a word and past a full
word) are refused on both compilers.

Every other `std::vector<bool>` access in the owned headers was audited:

- `SimplifyPath` calls `GetNext`/`GetPrior` only while at least two vertices are unflagged. It
  starts with `len >= 4` unflagged; at the loop head `curr` is unflagged; with exactly two
  unflagged, `GetPrior(curr)` and `GetNext(curr)` both return the other one and the loop breaks
  before flagging; with three or more it flags one vertex and the following `GetNext` calls still
  see two. Both functions overrun only when every flag is set, so these calls stay in range, and
  `flags[curr]` and the result loop use indices below `len`.
- `RDP` writes `flags[end--]` only while `end > begin`, and `flags[idx]` with `idx` 0 or a vertex
  index; every recursive `begin`/`end` is a vertex index (`end - 1` wraps only to choose the call,
  never to index). `RamerDouglasPeucker` sets `flags[0]` and `flags[len - 1]` for `len >= 5` and
  reads `flags[i]` for `i < len`.
- No other owned body, and none of `clipper.engine.cpp`, `clipper.offset.cpp` or
  `clipper.rectclip.cpp`, uses `std::vector<bool>`.

The path and `distSqr` subscripts are `std::vector` of non-`bool` elements, which GCC 13.3's
`_GLIBCXX_ASSERTIONS` checks. A randomized differential run (below) found no disagreement between
the GCC 15 oracle (library assertion), the GCC 13.3 oracle (trace) and the Rust shim.

## Harness limits (refused identically, never compared)

- Engine domain: `int64_t` boolean coordinates outside `±MAX_COORD` (as the earlier Clipper2
  adapters).
- Offset domain: `InflatePaths`/`offset_members` coordinates, deltas, miter limits and arc
  tolerances outside `±2^40` or non-finite (for `PathsD` after scaling); round offsets above
  65,536 arc steps per circle or 2^22 points. For `InflatePaths(PathsD)` the guard applies only
  when `ScalePaths<int64_t, double>`'s own bounds test (on `GetBounds<double, double>`, against
  `min_coord`/`max_coord`) passes: a request whose scaled bounds leave that range reaches the
  defined `Values exceed permitted range` exception before any conversion or offset work (the
  constructor before it only stores its arguments), whatever delta, miter limit or arc
  tolerance, and is compared (`inflate-f64-range-*`: each side, infinities, NaN points beside an
  out-of-range one, NaN delta and arc tolerance, the smallest overflowing value for precisions
  0, 2, -2 and 8 on both sides). Bounds exactly at `±max_coord` pass the test and stay refused
  by the offset domain (`control-inflate-f64-range-edge*`); `±2^40` after scaling is offset
  (`inflate-f64-domain-edge-*`), the next double above it refused.
- Minkowski: coordinates (or scaled coordinates below 2^63) beyond half the engine domain;
  at most 2^14 quadrilaterals.
- `RamerDouglasPeucker`: at most 1,024 points per path, because the C++ copies the path into
  every recursive frame. A NaN epsilon is refused (`epsilon`) only for a path whose `RDP` calls
  would not end, or exceed `4 * 1024 + 16` calls: both adapters follow the calls in the
  reference's order (`RdpNanGuard`/`rdp_nan_guard`, using the reference's comparisons and
  `PerpendicDistFromLineSqrd`) and refuse a call that repeats an active ancestor's arguments. A
  call's control flow depends only on `begin` and `end` (it writes `flags` but never reads them),
  so such a call recurses forever, and a recursion that never ends must repeat one (the
  arguments are vertex indices). It needs a NaN `epsSqrd`, when `max_d <= epsSqrd` never returns
  and a call whose points all lie on its chord keeps `idx` 0. Paths shorter than five points
  (copied without an `RDP` call) and recursions that end (the review's
  `[(0,0),(1,0),(2,2),(3,0),(4,0)]`, quiet, negative and signalling NaN; random points with no
  three collinear) are compared (`rdp-nan-*`). An overflow the reference reaches before a
  repeated call is undefined (`control-ub-rdp-nan`), as it is in the C++.
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

## Local diagnostic evidence after the review of 493e5872

The same tree was compared against two oracles built from one stage and overlay: host GCC 15.2
and Ubuntu 24.04's g++-13 13.3.0-6ubuntu2~24.04.1 (packages extracted into a private prefix, not
installed; its own C++ headers, linked against the host libstdc++ and glibc 2.43, so this is not
a full GCC 13.3 / glibc 2.39 qualification). On both, with the CI `GLIBC_TUNABLES`:

| Unit | Cases | Controls |
| --- | ---: | ---: |
| include p002 | 3,740 exact, 0 deferred | 92 identical, each with an identical continuation |
| src p002 | 1,657 exact | 48 |
| src p001 | 907 exact | 49 |
| include p001 | 1,587 exact | 83 |
| src p003 | 1,574 exact | 11 |

The import-free wasm replay of the corpus and controls agrees with native Rust. A randomized
differential run (6,000 `SimplifyPath`, finite and NaN `RamerDouglasPeucker` and
`GetNext`/`GetPrior` requests, fixed seed, not committed) gave identical replies from the GCC 15
oracle, the GCC 13.3 oracle and the Rust shim, including 418 every-flag-set refusals and 785
answered and 709 refused NaN-epsilon requests. The hosted GCC 13.3 / glibc 2.39 run remains the
reference.

## Coverage

C++ line coverage of the headers by the corpus was not measured. Every body above is called by
at least one compared case or (for `GetNext`/`GetPrior` overruns and the other undefined
branches) one control.
