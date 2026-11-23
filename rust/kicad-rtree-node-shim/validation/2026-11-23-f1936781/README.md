<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Validation of f1936781f375850b2d8be2f0683e2f4af278dfa1

Unit `libs.kimath.include.geometry.rtree.application.pure.p003`, local WSL Ubuntu
(Linux x86-64, GCC 15.2, Rust 1.99.1), 2026-11-23. This directory is evidence for the
tested revision named above; it was committed after that revision and changes nothing
it tested.

## Method

1. `git -c core.autocrlf=false -c core.eol=lf archive HEAD` and `port/oracle/prepare.py`
   (same settings) in the Windows worktree; `export.txt` records the clean overlay
   revision, archive and manifest hashes.
2. `scripts/final.sh` extracts the export into a fresh Linux root, builds and audits the
   own-library oracle with `port/oracle/build.sh` (`ORACLE_JOBS=8`, shared build lock),
   then runs the `rust-port` workflow steps: workspace `fmt --check`, `clippy -D
   warnings`, `test`, the portable wasm build, the policy unit tests, default
   `validate_units.py`, `generate_cases.py --check`, and `check_goldens_policy.py --ci
   --require-phase0` against the fresh oracle. It also replays the 97 refusal fixtures
   through both programs.
3. `check_goldens_policy.py --base 535a17e39cc77452ba3377a5afb4170c14e9ab81` was run in
   the Windows worktree at this revision: `Golden change evidence valid`.

## Results

| Check | Result |
| --- | --- |
| Oracle build and its 8 ctest checks | pass (`binary.sha256`, `oracle-provenance.json`) |
| fmt, clippy `-D warnings`, workspace tests | pass; 154 tests passed, 0 failed (`test.log`) |
| wasm32-unknown-unknown portable crates (includes `kicad-rtree-node`) | pass (`wasm.log`) |
| Policy tests, unit records, generator freshness | pass |
| Live difftest `check` (cases + golden) | green: 1448 cases, 0 deferred, 0 failures (`difftest-report.json`) |
| Strict phase 0 gate | COMPLETE, 3/3 live green layer-0 units (`phase0-summary.json`; the full report's hash is recorded) |
| Refusal fixtures | 97/97 identical for oracle and Rust (`refusals.*.jsonl`, `refusals.log`) |

`bless-57352b78/` holds the reports of the baseline blessing from a separate clean export
of 57352b787f (the implementation commit): compare, propose/bless and check were green
for all 1448 cases with the same oracle output hash
`4941ae0ddade8efb59eabf7b20b49f434ce8552cdd129a87f223a6db7b4d14c4` as here.

## Limits

* Local evidence only; hosted CI on the PR head is the merge gate.
* The unit record stays `partial`: see `rust/kicad-rtree-node/README.md` for the reference
  branches deliberately not reproduced.
* Extraction coverage gaps (4 parse failures, 63 uncompiled sources,
  `all_configurations_complete = false`) are pre-existing and unrelated to this unit.
