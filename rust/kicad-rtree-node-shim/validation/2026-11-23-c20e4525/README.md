<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation of c20e45256d374904f2035aa9c14e040945e3c8ec

Unit `libs.kimath.include.geometry.rtree.application.pure.p003` after the five fixes from
the first independent Sol 6.1 review of PR 52. Local WSL Ubuntu (Linux x86-64, GCC 15.2,
Rust 1.99.1), 2026-11-23. This directory is evidence for the tested revision named
above; it was committed after that revision and changes nothing it tested. The earlier
`2026-11-23-f1936781/` run is historical and left unchanged.

## Method

As in the earlier run: a clean LF `git archive` export and pinned reference from the
Windows worktree (`export.txt`). `scripts/final.sh` then builds and audits a fresh
own-library oracle (`ORACLE_JOBS=8`, shared build lock) and runs every `rust-port`
workflow step plus the refusal replay. `check_goldens_policy.py --base` was run in the
Windows worktree against the PR base `535a17e39cc77452ba3377a5afb4170c14e9ab81` and
against the previous head `ee820d03983338d0f18f79586e906741f4fa73b8`: both valid.

The golden was blessed from a separate clean export of `b477fb857e`, the last code
commit (`bless-b477fb85/`, `scripts/bless2.sh`), in two verified steps:

1. A delta over the committed 1448-case golden: evidence
   `port/goldens/evidence/<unit>-review-additions.json` lists exactly the 50 appended
   `review-*` case IDs (indexes 1448..1497, old value null). The original 1448 replies
   are a byte-identical prefix of the new golden.
2. The complete golden against the PR base, where no golden exists yet, as the policy
   requires: `<unit>-review-consolidated.json`. It produced a byte-identical golden.

`<unit>-baseline.json` from the first blessing is unchanged.

## Results

| Check | Result |
| --- | --- |
| Oracle build and its 8 ctest checks | pass |
| fmt, clippy `-D warnings`, workspace tests | pass; 157 tests passed, 0 failed |
| wasm32-unknown-unknown portable crates | pass |
| Policy tests, unit records, generator freshness | pass |
| Live difftest `check` (cases + golden) | green: 1498 cases, 0 deferred, 0 failures; oracle output sha256 `7a303d26...` equals the golden |
| Strict phase 0 gate | COMPLETE, 3/3 live green layer-0 units |
| Refusal fixtures | 108/108 identical for oracle and Rust |

## Limits

* Local evidence only; hosted CI on the PR head is the merge gate.
* No sanitizer build of the oracle adapter was run here. The adapters keep the reference
  inside its defined domain through explicit guards, and the review's UBSan probes
  established the reference behaviour for the five findings.
* The unit stays `partial` for the deliberately unported branches listed in
  `rust/kicad-rtree-node/README.md`: foreign-node `Free`/`Allocate`, other instantiations,
  NEON, and identity after address reuse.
