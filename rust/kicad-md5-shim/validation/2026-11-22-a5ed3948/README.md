<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Corrected graph and shared nesting integration

Runtime source: `a5ed3948399da32b64f7fad93e0394c64615f8ff`. Clean oracle export
reports that exact revision and `overlay_dirty_at_export=false`. Later maintained
notes/documentation changes alter no runtime body, corpus or output. No duplicate
build was started for the notes cleanup.

Exact graph dependency: `19035296e129cf8640e89331bc9e8ad84f0fe38c`, integrated
by merge `175e4cf46137c51c20b651d0d7c6ce6775abaeae`. Exact rust/shared #47
dependency: `4c6c1e38ce2dead8db8a3193e7525d19532523e9`, integrated by merge
`a79516bdc5a67d89ec843b09c1b220c9c8798e8f`. The merged #45 parser is retained.
These are branch integrations, not PR merges or acceptance of the final graph.

`protected-scope-proof.json` records comparison with the committed corrected
MD5 record: all 289 lines, 15 definitions, one source range, L0/pure, empty
hard/soft dependencies and gaps, all USRs and remaining protected fields match.
The merge preserved prior port metadata; only own maintained notes were refreshed.
Other unit-record changes are inherited exactly from the corrected dependency.

Actual local results:

- 59 compiler records / 822 unchanged pinned files pass source, actual-library
  linkage and fixed floating-point audits; four oracle CTests pass.
- Affected fmt and warnings-denied clippy pass; 24 Rust tests pass (11
  MD5/wrapper, 13 harness/request), plus portable MD5 wasm.
- Each real C++ and Rust adapter passes 153 complete refusal envelopes plus
  signed-zero success. Exact raw wire and expected complete replies are retained.
  Added tests bracket the 127-container bound for scalar/empty-container leaves,
  input, unknown-unit, identity and extra fields. Previous numeric regression
  requests and expected replies are an unchanged byte prefix of these fixtures.
- The registered CI path passes full source/unit validation: 1,801 records.
  All 5,973 live success cases match the actual oracle and committed golden at
  the unchanged default 60-second budget, including the real 512 MiB carry case.
  Zero failures or deferrals. Generator output is byte-identical.
- The staged summary remains **phase zero INCOMPLETE, 1/3 live matched L0
  pilots**. `PHASE0_REQUIRED=false` and extraction/configuration flags are
  preserved. Inactive/uncompiled configurations are not accepted by this result.

No algorithm, portable MD5 body, successful corpus/reply, golden, historical
immutable artifact, exact comparator or Cargo lock changes were needed. No
proposal/blessing was performed. `live-check.json` records the actual executable
and output identities; `oracle-provenance.json` records exact source/compiler/
build/link provenance. `phase0.json`, `job.log` and `validate.sh` retain the
registered checks, commands and results.

The detached job ran in new private Linux tree `~/kicad-port/md5-corrected-graph`
using independent copies/caches and the real shared mkdir lock, with at most
eight compile jobs. It released only its own locks and did not modify any prior
worker/evidence tree. Old generated records were removed only from the new
private `repo/port` copy before syncing the corrected canonical records.

**Acceptance remains pending:** parent confirmation of final independent #39
artifact review, updated hosted CI and parent review. Published `green` metadata
is explicitly local/provisional. No graph, merge or phase-gate acceptance is
claimed; keep PR #42 draft until the parent confirms otherwise.
