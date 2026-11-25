<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Revision-specific evidence

Runtime implementation/adapter: `a189f6fe4a87d78d69fa802b8cf567b22e7d0a1f`.
Official base: `98cbf0b63eb68541eb5b4ba7d5cc2d03b71d2cf2`.
Reading/native source: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
The evidence commit adds two defined empty-1D-bulk programs, coverage extraction,
and documentation; runtime implementation and adapter source stay unchanged.
Their hashes and the expanded corpus hash are in `summary.json`.

Local native export was clean at the implementation revision. `provenance.json`
binds its pinned headers, overlay, Linux arm64 GCC 12.2/libstdc++ UBSan/float-cast
sanitizer flags and actual binary digest. The Mac host uses Rust 1.96.1 arm64;
`compare.json` records 153 compared, zero deferred/failed/unprocessed and the
expanded 153-program / 21,440-step corpus digest. Its oracle path is the local
Docker launch script, so the report's `oracle_sha256` hashes that script; the
**actual ELF** hash is `provenance.json:binary_sha256`. No reference body is
replaced, tolerance applied, input dropped or output looked up.

All 27 separate refusal controls agree on both processes. Five portable behavior
tests and six affected p001 regression tests pass. Focused fmt/clippy with denied
warnings, wasm compilation, deterministic generators and unit validation pass.
The full expanded corpus replays against freshly collected native replies in an
import-free wasm binary on Node v24.16.0 / V8 13.6.233.17-node.49. Runtime binary
and corpus digests are retained; native replies remain disposable and no golden
is committed.

`hosted-initial-*` retains the original 151-program successful run from
[hosted validation](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45583419889),
on GitHub merge `447f8c4e73d9f3c9cc1e811a77ca4b2115c48e0e` for PR head
`a189f6fe4a87d78d69fa802b8cf567b22e7d0a1f`: Linux x86-64 GCC 13.3/libstdc++,
Rust 1.99.1, 151 compared / zero deferred/failed; 27 controls; full original-corpus
wasm replay on Node v22.23.5 / V8 12.4.254.21-node.57. This is historical evidence
for that exact initial head/corpus. The PR links the final expanded-corpus CI run.

`owned-header-coverage.json.gz` contains only gcov entries for the two unmodified
owned headers. `coverage-summary.json` aggregates the ten instantiated
configurations; counters include repeated corpus runs and controls, not just one
comparison. 513/515 functions, 4,936/5,083 line entries and 2,643/3,340 branches
were executed. The two unexecuted functions are 1D nonempty bulk comparator
lambdas; reaching them would first evaluate undefined axis-1 access. Empty 1D bulk
is supported and tested. The only source lines never executed in any configuration
are COW 452 (private null ensureWritable guard) and 534 (private not-owned assertion).
Other untaken per-instantiation branches are visible in the raw evidence; no full
branch/generic/platform coverage is claimed. See the portable crate's body map
for supported behavior and p003 type/allocation boundaries.
