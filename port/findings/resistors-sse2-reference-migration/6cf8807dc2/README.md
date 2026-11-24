<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Native resistor migration evidence

Source tested: `6cf8807dc2144a5bf82c4aca9804a75fa19ab4a7`, based on official
`rust@2f01eaa1d9d47d051c5e562984df56f94b103fe5`.
[Native CI run 45328928252](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45328928252)
completed successfully on 2026-11-24. It built the measurement image using exact
reviewed PR55 tooling `33535308`, then independently sealed and built this unit
source. No open topic branch was merged into it.

The pinned image and its no-cache rebuild have byte-identical installed identity
records, SHA-256 `c37568600ebf319225f8e5cb2b152bdfb90f5a8b11d1d08d4a4bb94ba1b85b8b`.
Image verification, twelve probe checks (200,000-angle run), all 76 strict oracle
compilations and eight CTests passed. The probe files are not in this small
migration artifact; those counts come from the retained job log, not a separate
parent probe replay. The image remains a measurement candidate.

The genuine live oracle and newly built Rust shim each ran the unchanged 1050
requests during **propose, bless and check**. Each report records 1050 compared,
zero failures/deferrals/unprocessed. Final check matched the generated golden.
The oracle is `aa41eb0957d0a3e7fd831051a7330e015a8403d0893071332a564ed62de0238f`;
the shim is `06dd7e9094185b8f8f2c99b1b30db4c7524c9f9964f24b1159ef46046806fea6`.
The runtime report binds the selected library hashes, loader setting, dynamic
pow result and the same oracle hash. Its digest is cited by the blessing reason.

The parent verified the **whole artifact ZIP digest**:
`7a5876e01b30aacc387c6e876647df26c558baff9f9d8eb26853e66a174ecb29`, artifact
`14892225782` (`resistor-reference-migration`). It independently verified:

- policy-valid evidence against the official old golden and unchanged corpus;
- exactly thirteen changed cases, 317 numeric bit fields and three strings;
- the other 1037 golden lines byte-identical;
- all 24,573 source-path memberships, 82 compiler-relevant source hashes and all
  915 pinned-reference file hashes against raw Git objects;
- source proof/head/tree, live provenance/report/binary hash bindings, and
  byte-identical image/rebuild identity records.

The generated golden SHA-256 is
`db61789166977ceab4ce628c252c6ed35771cd7a5bb84d0ad7ac0a3ae08e2e9e`.
It is copied verbatim from the live bless result. The original golden SHA-256 was
`8d1cc9e8c280b349294f9d512e450586c892a3170881afbba1c1b78de1a107a2`.
The new immutable approval evidence retains all old/new lines. Nothing in earlier
evidence was edited.

The native generation used the initial `6cf` approval guard. Subsequent loader
and authorization fixes at `4e1a387ceeca8b80fe4e9113efca12d2f910029e` were not
executed by that older run. The parent independently applied the stricter exact
replacement reconstruction to this artifact and it passed; original Sol
counterexamples reject. This does not relabel the native source revision.

Final current-head ordinary CI, actual wasm success and independent final data
review remain pending. Other units, cross-host qualification, PR55 adoption and
matrix/hash numerical blockers are not certified by these unit-only runs.

The complete ZIP, source proof, full CI log and reproducible parent `audit.py`
are retained at `/Users/karl/.t3/scratch/pr56-generation-45328928252/`.
