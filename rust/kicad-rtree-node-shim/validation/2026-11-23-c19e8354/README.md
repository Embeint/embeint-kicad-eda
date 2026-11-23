<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation of c19e8354e61cda8fef50b5578c349c16ef8e84f6

Unit `libs.kimath.include.geometry.rtree.application.pure.p003` after porting foreign
allocation (the second independent Sol 6.1 review of PR 52), with official `rust`
`7fe3bd2b7aab40f7489212ebd0f3e65c358215b2` (PR 53, partial-unit reporting) merged in.
Local WSL Ubuntu (Linux x86-64, GCC 15.2, Rust 1.99.1), 2026-11-23. This directory is
evidence for the tested revision named above; it was committed after that revision and
changes nothing it tested. The earlier `2026-11-23-f1936781/` and `2026-11-23-c20e4525/`
runs are historical and left unchanged.

## Method

1. A clean LF `git archive` export and the pinned reference were made from the Windows
   worktree (`export.txt`).
2. `scripts/final.sh` builds and audits a fresh own-library oracle (`ORACLE_JOBS=8`,
   shared build lock), then runs every `rust-port` workflow step with the merged harness.
   CI runs this partial unit with `--partial`. It then replays the refusal fixtures and
   runs `scripts/sanitize.sh`.
3. `scripts/sanitize.sh` builds a focused ASan+UBSan executable (`scripts/sanitize_driver.cpp`)
   from the real protocol, registry, rtree adapter and pinned headers, registering only
   this unit. It replays all cases and refusals and requires output byte-identical to the
   audited oracle and empty sanitizer stderr. It is a probe, not a replacement for the
   audited oracle.
4. `check_goldens_policy.py --base` was run in the Windows worktree against
   `7fe3bd2b...` (the PR base after the merge), `535a17e3...` (the original base) and
   `af6630a5...` (the previous head): all valid.

The golden was blessed from a separate clean export of the merged code head
`ec32907394` (`bless-ec329073/`, `scripts/bless3.sh`):

- `<unit>-foreign-additions.json` lists exactly the 14 appended `review2-*` cases
  (indexes 1498..1511, old value null); the 1498 earlier replies are a byte-identical
  prefix.
- `<unit>-foreign-consolidated.json` covers the complete golden against the PR base.
- The earlier `-baseline`, `-review-additions` and `-review-consolidated` evidence is
  unchanged.

## Results

| Check | Result |
| --- | --- |
| Oracle build and its 8 ctest checks | pass |
| fmt, clippy `-D warnings`, workspace tests | pass; 175 tests passed, 0 failed |
| wasm32-unknown-unknown portable crates | pass |
| Policy tests (merged suite), unit records, generator freshness | pass |
| Live difftest `check --partial` (cases + golden) | `green`, mode `partial`: 1512 compared, 0 deferred, 0 failures, golden matched |
| Strict phase 0 gate | COMPLETE, 3/3 live green layer-0 pilots; this unit is the one live partial unit, listed in `partial_units_without_deferrals` |
| Refusal fixtures | 116/116 identical for oracle and Rust |
| ASan+UBSan adapter replay | 1512 cases and 116 refusals byte-identical to the oracle; no diagnostics |

## Limits

* Local evidence only; hosted CI on the PR head is the merge gate.
* The unit stays `partial` for the deliberately unported behaviour listed in
  `rust/kicad-rtree-node/README.md`: other ELEMTYPE/DATATYPE instantiations, NEON, and
  identity after heap address reuse. Foreign `Free`/`Allocate` is no longer among them.
