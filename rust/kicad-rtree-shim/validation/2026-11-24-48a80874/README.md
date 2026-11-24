<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Local dynamic-tree validation

Tested implementation/integration: `48a80874093c78521a826219a0d7227582d252bf`, initial
implementation `ee0ab5324679bc770044b22b5d3236091b08c097`, accepted official base
`1ac339f229d8ccb8ccb297f489493622903cc468` (initial requested base `eb4cd4724f`).
Following commits only record evidence/docs/status unless separately documented.

`summary.json` records exact counts, tool versions and source/binary SHA-256.
`compare.json` is the real shared difftest report, with every case accounted for.
All 249 successful cases / 67,713 steps compare exactly, zero deferrals/failures.
All 26 separate refusal controls agree on the native/Rust processes. Six portable
behavior tests and the shim integration test pass, as do fmt, clippy `-D warnings`,
source-accounting validation, wasm compilation and complete import-free wasm replay
of all requests against freshly collected native replies. The transient 18 MB
native reply file was embedded only in the wasm probe, never used by either
implementation and not committed as a golden. Its digest is retained.

The native binary was compiled from an earlier dirty Git export, with actual
pinned headers and actual overlay bytes. `provenance.json` honestly retains that
export revision/dirty bit and compiler/platform/flags/binary hash. Every overlay
source hash in it was checked against the tested integration revision. Rust checks
were repeated after its accepted-base merge. This is **not** a claim that this
local native binary was compiled from a clean final checkout. Hosted CI rebuilds
from the PR checkout on Linux x86-64, and remains a separate pending authority.

Linux native execution used an already-running Mac Docker environment, Debian
bookworm arm64 GCC 12.2/libstdc++, image
`sha256:c1999f81da8e1f787006d58cbaece8ac4837d7ae04ad16784d829470f4bebc8d`, capped
at two CPUs / 3 GiB. Shared-mount gcov writes were slow, so final compilation ran
on the container filesystem and the resulting binary/reports were copied out.
The initial copy reported ownership-preservation errors on the Mac mount; the
copied binary SHA-256 was verified against build provenance before execution.
No engine/GUI, Windows or WSL build was started.

`dynamic-header-coverage.json.gz` is actual gcov data captured after the single
complete successful corpus comparison, before refusal replay and native reply
collection. `coverage-summary.json` reports 644/660 executed function
instantiations, 6,756/7,256 executed line entries and 4,159/5,480 taken branches.
Compiler-generated functions, template instantiations and exception edges are
included; these counts do not certify every one of the 57 extracted USRs/branches.
Unexecuted paths remain in the report and the owned-body audit.

Commands: focused `cargo fmt --check`, `cargo clippy --all-targets -j2 -- -D warnings`,
`cargo test -j2`, `cargo check --target wasm32-unknown-unknown -j2`, both generator
`--check` modes, `port/tools/validate_units.py`; standalone pinned export/build;
`kicad-difftest compare --timeout-secs 240`; native `coverage.py`;
`compare_refusals.py`; `cargo build --example corpus_parity --features live-wasm
--target wasm32-unknown-unknown --release -j2` with the fresh reply file, then Node
`wasm-test.mjs`. See the parent shim README and committed workflow for exact paths.

No tolerance, case exclusion, dependency deferral or golden blessing occurred.
The unit remains partial; separate review and hosted x86-64 comparison remain.
