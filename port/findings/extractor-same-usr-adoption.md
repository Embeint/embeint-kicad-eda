# Same-USR definition endpoints: canonical adoption

Status: adopted. Addendum to
[extractor-same-usr-definition-endpoints.md](extractor-same-usr-definition-endpoints.md),
which remains unchanged. The committed graph, inventory, generator manifest, unit
report, measurement, drift reports and `port/units/` now use the reviewed
aggregation. No extractor behaviour changed in this step.

## What was run

- A read-only replay of the V5 parse cache (`~/kicad-port/corrected-review-v5`)
  with the final tool tree at `4738a9c23f`: aggregation identity
  `30ca71dc50af99de46f34d50b4ac1343543ecf77852f6d380f369e6a89118870`, parse
  identity `e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d`.
  All 2,511 records were reused and none was parsed. The cache listing and
  content hashes are equal before and after. It exits 1 for the same four
  `thirdparty/fmt` parse failures as V5. The guarded aggregation from the
  reviewed follow-up gives the same graph semantics as the PR #79 replay; only
  the aggregation identity differs.
- The guarded generator-dependency `capture` against the replay inventory, then
  `apply` twice to the replayed canonical graph with byte-identical results. The
  recipes, Ninja catalog, source and generator relations equal the retained
  capture. The retained capture is refused on the replay inventory because its
  source identity binds the V5 inventory.
- The canonical split over the accepted records, measurement and drift. After
  accepted `rust` advanced to `98a12c1123`, it was merged and the split was rerun
  over the merged records: values were identical, and 11 accepted records were
  rewritten only in the splitter's `[difftest]` key order.

Scripts, logs and comparisons are in
`port/extractor/generated/evidence/same-usr-adoption/` and its `rust-98a12c11/`
subdirectory. A replay is not a new parse or parser qualification.

## Semantic deltas against accepted rust

- **Graph:** 649,723 to 647,925 edges: 0 added, 1,798 removed (1,355 static,
  210 virtual, 233 override). 17,587 to 17,576 overrides: 0 added, 11 removed.
  Every removed edge has an endpoint in one of the 207 same-USR copy groups.
  Functions (107,254), wx classifications, definition copies and variants,
  template aliases, unresolved records (8,899,609), translation units (2,511) and
  the inventory's source accounting are unchanged. Provenance adds the
  aggregation identity, endpoint policy and replay record; the inventory differs
  only in provenance and the recaptured generator inputs.
- **Geometry p003:** the five edges from `SHAPE_ARC`, `SHAPE_CIRCLE`,
  `SHAPE_ELLIPSE`, `SHAPE_LINE_CHAIN_BASE` destructors and
  `SHAPE_COMPOUND::AddShape` into the VRML `~SHAPE` body are gone. The unit's
  hard dependencies are geometry p001, math ui p001/p002, circle p001 and seg
  p001; it moves from layer 13 to 5. Its soft dependencies are unchanged.
- **Retained VRML reachability:** `WRL1FACESET`, `WRL2FACESET` and
  `X3DIFACESET::TranslateToSG` have no TU-local `~SHAPE` definition, so they
  still reach both bodies. Hard dependents of
  `plugins.3d.vrml.application.ui.p001` fall from 22 units to the VRML v1, v2,
  generated v2 and x3d units.
- **Unit records:** only `depends_on`, `soft_depends_on` and `layer` differ from
  accepted rust. Every status, claim, timestamp, note (including co-port notes),
  test, difftest registration, function, range, line count and classification
  equals the accepted record. The 1,801-unit count and 2,380,947 accounted lines
  are unchanged. The maximum layer falls from 391 to 376, back-edges from 9,548
  to 9,496 (weight 106,538 to 105,471) and hard unit edges from 46,894 to 46,709.
  1,060 records change. 426 soft edges became hard and 401 hard edges became
  soft; 237 unit dependencies disappear and none is new. No claimed, partial or
  green unit's dependencies or layer change; 23 blocked units change, mostly
  only in layer. Details are in `rust-98a12c11/checks.json`.
- **Measurement:** besides the graph metrics, unit statuses now reflect the
  merged records (3 green, 24 partial, 64 blocked, 1,710 todo). The corrected
  run's failure records gain the replay's `cache_reused_from` trace; the four
  fmt failures and their diagnostics are otherwise unchanged.
- **Drift:** the reports are regenerated for 27 ported units. The accepted graph
  produces identical drift reports over the same records, so drift is unaffected
  by the adoption.

## Limits

These remain as recorded in the original finding: cross-TU calls to a USR with
several out-of-line bodies stay conservative, `wx` names are still unioned per
USR, and `definition_variants` still pairs the two `~SHAPE` bodies. Parsing and
all-configuration completeness remain false (`--require-complete` fails as
before) with the four fmt parse failures, 63 uncovered implementation sources
and 64 blocked units.
