# Same-USR definition bodies joined across unrelated components

Status: tool fix verified by a private read-only replay; regenerated graph and
unit records **not adopted**. Committed graph, reports and `port/units/` are
unchanged.

## Location and trigger

`materialize_definitions` (`port/extractor/model.py`) keeps every physical body
that shares a clang USR as `<usr>@definition:<hash>` copies. Before this fix it
joined every call and override to every copy, regardless of which body the
reporting translation unit actually defined.

Two unrelated global classes named `SHAPE` share the USR `c:@S@SHAPE@F@~SHAPE#`:

| Body | Definition | Kind |
| --- | --- | --- |
| primary | `libs/kimath/include/geometry/shape.h:138` | inline, virtual |
| `@definition:861533d9c907d7fb` | `plugins/3d/vrml/wrlfacet.cpp:675-688` | out of line, nonvirtual |

The committed graph therefore has edges from geometry to the VRML body, for
example `SHAPE_ARC::~SHAPE_ARC`, `SHAPE_CIRCLE::~SHAPE_CIRCLE`,
`SHAPE_ELLIPSE::~SHAPE_ELLIPSE`, `SHAPE_LINE_CHAIN_BASE::~SHAPE_LINE_CHAIN_BASE`
and `SHAPE_COMPOUND::AddShape`. Through these edges
`libs.kimath.include.geometry.application.ui.p003` has a hard dependency on
`plugins.3d.vrml.application.ui.p001`. Edges also run the other way: the
geometry body appears to call the VRML body's `std::list` operations, and the
VRML body appears to call geometry's `SHAPE_BASE::~SHAPE_BASE`.

## Caller TU, definition and link-target semantics

The V5 parse cache (`~/kicad-port/corrected-review-v5/cache`, 2,511 records,
parser `e1548c1f`) shows how each TU sees the destructor (`scripts/probe.py`):

- Every geometry and pcbnew TU that calls `~SHAPE` defines it locally at
  `shape.h:138`. Examples are `shape_arc.cpp`, `shape_compound.cpp`,
  `pcb_painter.cpp`, `exporter_vrml.cpp` and `dialog_export_vrml.cpp`. Its
  override records are local too.
- Only `plugins/3d/vrml/wrlfacet.cpp` defines the VRML body.
- The VRML callers `vrml1_faceset.cpp`, `vrml2_faceset.cpp` and
  `x3d_ifaceset.cpp` have no local definition. They declare `~SHAPE` through
  `wrlfacet.h` and reach `wrlfacet.cpp` at link time.

A TU contains at most one definition of a USR, and an inline function must be
defined in every TU that odr-uses it. A TU-local definition is therefore exactly
the body that TU's calls and overrides refer to. The geometry edges to the VRML
body are false. The VRML callers' edges are real cross-TU calls, and the source
graph cannot see which body the linker chooses for them.

## Fix

The per-TU aggregation (`locate_endpoints`) records each call and override
endpoint's TU-local definition location before TUs are merged.
`materialize_definitions` sends a located endpoint only to that body. An
endpoint with no local definition, or an unlocated legacy record, still reaches
every same-USR body. The parser and cache records are unchanged. No variant call
or override is discarded globally.

`extract_graph.py --replay-parse-runtime <checkout>` re-aggregates an existing
cache read-only, as documented in `port/extractor/README.md`.

## Regression evidence

Code commit `e564ac2c120c5322278326861158a628fac9a617` (`port(fix): resolve
same-usr call endpoints to the calling tu's body`) has the aggregation identity
`49935b1e7fc27867bcf62274ed4ca30d2317ef1d10177b2bb99fe6c5b01e161b`. The
scripts ran from a scratch directory beside the worktree, with a WSL runner
copy of that commit at `~/kicad-port/shape-collision/runner`.

- `GraphTests.test_same_name_classes_in_separate_components_resolve_to_their_tu_bodies`
  reproduces the collision from synthetic per-TU records. It also covers a
  shared inline body, a cross-TU fallback, legacy records, and an endpoint
  location that was never recorded as a definition (rejected).
- `ClangIntegrationTests.test_same_name_classes_in_separate_targets_and_read_only_cache_replay`
  runs libclang 21 through the real CLI on a nine-file, six-TU fixture:
  - It checks geometry's inline virtual base versus an out-of-line nonvirtual
    VRML `SHAPE`.
  - It checks an inline helper shared by two TUs, and a cross-TU virtual
    destructor whose base and override stay reachable.
  - It replays a parse cache written by a runtime with different aggregation
    but the same parser. The replay leaves the cache bytes unchanged, gives an
    identical graph and records zero parses.
  - It refuses a replay with missing records or a different parser.
- WSL suite with libclang: 39 tests pass (`logs/tests.log`). Windows: 39 run,
  13 libclang tests skipped, all pass.
- Negative control (`scripts/control.sh`, `logs/control.log`): the same two
  tests fail when the old location-blind union is restored.

## Private replay of the V5 cache

`scripts/replay.sh` ran under `~/kicad-port.build-lock`: 3 min 33 s and 10.0 GiB
peak RSS. All 2,511 records were reused and none were parsed. The cache file set
was unchanged before and after (`logs/cache-*.txt`). It exits 1 for the same four
recorded `thirdparty/fmt` parse failures as V5. Identities are in
`results/identities.json`.

The comparison with the V5 original AST graph `40861e9c`
(`results/graph-comparison.json`):

- Functions, wx classification, unresolved records, copies, variants, aliases and
  translation units are identical.
- 0 edges were added and 1,798 were removed (1,355 static, 210 virtual,
  233 override). 0 overrides were added and 11 were removed. Every removed edge
  has a same-USR copy endpoint.
- The 207 copy groups (244 bodies) are mostly same-named functions in separate
  executables, such as `common`/`pcbnew` `COMPONENT`, `NETLIST`, and `Kiface`
  in eight programs. Four groups are Clipper2 preprocessor alternatives in
  `clipper.core.h`. A removed edge is a call one body made being attributed to
  another body.
- 32 `~SHAPE` edges are removed. 25 run into the VRML body from geometry,
  pcbnew, eeschema and 3d-viewer callers, including the five geometry edges
  named above. Six run from the geometry body to the VRML body's `std::list`
  calls, and one runs from the VRML body to `SHAPE_BASE::~SHAPE_BASE`. The
  three VRML `TranslateToSG` callers still reach both bodies through the
  cross-TU fallback.

`scripts/split.sh` split that graph into a disposable copy of the committed
`port/units/` (`results/unit-record-changes.json.gz`):

- There are still 1,801 units with 2,380,947 lines. Back-edges fell from 9,548
  to 9,496 and the maximum layer from 391 to 376.
- Default validation passes. `--require-complete` fails exactly as it does
  today.
- Only `depends_on`, `soft_depends_on` and `layer` change, in 1,060 records. No
  status, claim, note, test, function, range, line count or classification
  changes. No claimed, partial or green unit's dependencies change.
- 426 soft edges became hard, all of them previously recorded back-edges. No new
  unit dependency appears. 401 hard edges became soft, and 210 hard and 27 soft
  edges disappeared.
- `libs.kimath.include.geometry.application.ui.p003` drops
  `plugins.3d.vrml.application.ui.p001` and moves from layer 13 to 5. Its hard
  dependencies are then geometry p001, math p001/p002, circle p001 and seg p001.
  Geometry pure p002, ui p006, `shape_compound` p001 and `shape_poly_set` p001
  also drop that dependency. Only the VRML v1, v2, x3d and generated v2 units
  still depend on VRML p001.

## Remaining work and limits

Sol 6.1 independently reviewed `664493feb0684460b82ad9580d972cb6a7a5b27a`
with no blocking findings and reproduced all 39 libclang tests. Its optional
cache-race finding is now hardened: replay also refuses a missing record at
consumption time, so disappearance after preflight cannot enqueue a parse or
write a cache entry under zero-parse provenance. A platform-independent mocked
race regression checks this path; the current suite passes 40/40 under WSL
and 27 tests with 13 libclang skips on Windows. The large V5 replay measurements
above still bind the original aggregation identity, not this later guard change;
canonical adoption must regenerate provenance from its actual final tool head.

- Adoption is a separate step: it needs this replay output, canonical split,
  the verified generator-dependency `apply` with the retained capture,
  measurement and drift reports, documentation counts, and a regenerated
  `port/units/`. Until then, the committed p003 record still shows the VRML
  dependency.
- Cross-TU calls to a USR with several out-of-line bodies remain conservative.
  This covers the three VRML callers. Telling them apart needs link-target
  information that the source graph does not record.
- `wx` names are still unioned per USR, and copies inherit the primary's union.
  `definition_variants` lists the two `~SHAPE` bodies as one variant pair. The
  replay shows no classification change, so neither is fixed here.
