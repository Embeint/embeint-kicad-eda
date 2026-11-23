<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation of ee68b7c3f9724dc769bf7e2553e34be1f2311bb4

Unit `libs.kimath.include.geometry.rtree.application.pure.p003` after the third
independent Sol 6.1 review's two fixes: null ownership queries, and caller-owned external
nodes plus constructed unused page slots. The tested revision is on official `rust`
`9261e1705d5cd0d53abb2964cfbd0a3b461f4d42` (merged in `992f07f9b7`). Local WSL Ubuntu
(Linux x86-64, GCC 15.2, Rust 1.99.1), 2026-11-23. This directory is evidence for the
tested revision named above; it was committed after it and changes nothing it tested.
Earlier validation directories are historical and unchanged.

## Method

1. A clean LF `git archive` export and the pinned reference were made from the Windows
   worktree (`export.txt`).
2. `scripts/final.sh` builds and audits a fresh own-library oracle (`ORACLE_JOBS=8`,
   shared build lock), runs every `rust-port` workflow step (CI runs this partial unit with
   `--partial`) and replays the refusal fixtures.
3. `scripts/sanitize.sh` replays all cases and refusals through a focused ASan+UBSan build
   of the real adapter, protocol and registry with the pinned headers, and requires output
   byte-identical to the audited oracle with empty stderr. Sanitizers corroborate; the
   support and refusal decisions rest on the source contracts and object-lifetime rules
   described in `rust/kicad-rtree-node/README.md`.
4. `check_goldens_policy.py --base` was run in the Windows worktree against `9261e170...`
   (PR base), `7fe3bd2b...`, `9aa7e680...` (last independently reviewed head) and
   `992f07f9...` (integration merge): all valid.

The golden was blessed from a separate clean export of the code head `65831d5acb`
(`bless-65831d5a/`, `scripts/bless4.sh`):

- `<unit>-storage-additions.json` lists exactly the 13 appended `review3-*` cases
  (indexes 1512..1524, old value null); the 1512 earlier replies are a byte-identical
  prefix.
- `<unit>-storage-consolidated.json` covers the complete golden against the PR base.
- All earlier evidence is unchanged.

## Results

| Check | Result |
| --- | --- |
| Oracle build and its 8 ctest checks | pass |
| fmt, clippy `-D warnings`, workspace tests | pass; 185 tests passed, 0 failed |
| wasm32-unknown-unknown portable crates | pass |
| Policy tests, unit records, generator freshness | pass |
| Live `check --partial` | `green`, mode `partial`: 1525 compared, 0 deferred, golden matched |
| Strict phase 0 gate | COMPLETE, 3/3 live green pilots; one live partial unit (this one) |
| Refusal fixtures | 128/128 identical for oracle and Rust |
| ASan+UBSan adapter replay | 1525 cases and 128 refusals byte-identical, no diagnostics |

## Limits

* Local evidence only; hosted CI on the PR head is the merge gate. This head has not been
  independently reviewed.
* The unit stays `partial`: other ELEMTYPE/DATATYPE instantiations, NEON, and identity
  after heap address reuse remain unported.
