<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Dynamic R-tree native and wasm comparison

Native JSONL adapter for `libs.kimath.include.geometry.rtree.application.pure.p001`.
`src/operations.rs` translates JSON into actual `kicad-rtree` calls; `main.rs`
uses the shared strict request parser and stdio. The C++ adapter is
`port/oracle/shims/dynamic_rtree.cpp`, registered in the normal own-library oracle.
It includes the genuine pinned header; no C++ tree algorithm is copied into it.
Explicit-instantiation member access observes the actual root and live topology.

`native/prepare.py` exports both R-tree headers and nlohmann/json directly from
Git at `4cf939778df04d2b747bb2c4714fc5014a7565c7`. The standalone driver reuses the
shared native protocol, allowing a small header-only compile without wx or a KiCad
engine. Build flags are C++20, `-O0`, `-fno-fast-math`, `-ffp-contract=off`, UBSan,
float-cast-overflow, no sanitizer recovery, and gcov. Exported source/overlay
hashes, exact revision/dirty state, compiler/platform/flags and binary SHA-256 are
retained in `provenance.json`. This is unit-native evidence, distinct from the
full audited own-library oracle's provenance.

## Wire schema

Input is `{"config":C,"steps":[...]}`. Configurations are `i32.d2.m4`,
`i32.d2.m8`, `i32.d2.m16`, `i32.d2.m28`, `i32.d3.m5`, `i32.d3.m16`, `i32.d1.m8`,
`i32.d1.m3`, `i64.d2.m16`, `i64.d3.m8` (element type, dimensions, fanout).
Steps 1..4096, at most 8192 active entries/tree, two trees `a=0/1`.
Item is null or an integer identity 0..8191, backed by real request-local pointers
in C++. Coordinates are the full `i32`/`i64` domain, with exact array lengths.

| op | Additional fields beyond `op,a` | Observation |
| --- | --- | --- |
| layout | none | actual C++ node/page sizes and alignments, element/pointer size, fill/reinsert counts |
| insert/remove | min,max,item | empty event / actual removal bool |
| bulk | entries: array of {min,max,item} | argument entries after real call (unchanged order) |
| clear | none | actual RemoveAll |
| state | none | size, empty, reference memory, full live hierarchy with child/insert bounds and data |
| iterate | skip,take (0..8192) | lazy order, cached current even at end, atEnd, equality with incremented copy; native arrow exercised |
| overlap | min,max,take | lazy matching order, cached current, empty/atEnd, copied iterator equality |
| search | min,max,stop (0..8192),seed | appended items and callback count; stop=0 still reports first hit if present |
| nearest | point,k (full int),seed | actual distance/data pairs; seed verifies replacement for empty/nonpositive k too |
| move_new/move_assign | src (0/1) | actual constructor/assignment; self move is a guarded no-op |

Signed distance arithmetic is validated with widened intermediates **only along
nodes/slots actually evaluated by the native best-first traversal**, before
calling the real `NearestNeighbors`. Bulk floating conversions are validated
before calling `BulkLoad`; values truncating to representable integers remain
valid. Refusal controls never execute known native undefined arithmetic.
Mutating C++ tests require defined signed intermediates: generators use bounded
coordinates for splitting/reinsertion while full-domain leaf cases exercise valid
extremes without area evaluation. UBSan fails the process on any missed undefined
step. This adapter is a test runner, not a hardened arbitrary-input C++ service;
mutation-overflow guards in Rust are separately tested and are not claimed as
native executions. No valid generated case is filtered or replaced.

## Corpus and checks

`generate_cases.py` writes **249 programs / 67,713 steps**, seed 20261124:
packing boundaries, empty/one/multiple levels, equal boxes, touching/degenerate
bounds, distributions forcing leaf/internal splits and nonroot reinsertions,
shuffled deletion, insertion-box fallback, null/duplicate data, root collapse,
leaf/internal orphan continuations, populated move targets and source reuse,
visitor stop boundaries, seeded outputs, lazy iterator copies and equality,
nearest ties/limits, full integer-domain leaf boxes, inverted bounds and mixed
insert/remove/query sequences. The explicit 800-entry M=8 cascade is also a Rust
behavior test with measured branch counters. Complete native topology is compared,
not just hit sets. `--check` verifies deterministic bytes.

`generate_refusals.py` writes **26 refusal controls**, kept separate from successful
corpus comparisons: malformed fields/counts, unknown instantiation, full-domain
nearest overflow, out-of-domain bulk conversion, step and active-entry budgets.
`compare_refusals.py` runs both real processes and requires every error envelope.
The Rust integration test also replays them. Errors never count as successful
operations or Unported deferrals. Generators contain no expected successful output.

```sh
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-rtree-shim -j2
python3 rust/kicad-rtree-shim/native/prepare.py --out /tmp/rtree-stage
bash rust/kicad-rtree-shim/native/build.sh /tmp/rtree-stage /tmp/rtree-live
rust/target/debug/kicad-difftest compare \
  --unit libs.kimath.include.geometry.rtree.application.pure.p001 --root "$PWD" \
  --oracle /tmp/rtree-live/rtree-oracle --rust "$PWD/rust/target/debug/kicad-rtree-shim" \
  --timeout-secs 240 --report /tmp/rtree-live/compare.json
python3 rust/kicad-rtree-shim/native/coverage.py --out /tmp/rtree-live
python3 rust/kicad-rtree-shim/compare_refusals.py \
  --oracle /tmp/rtree-live/rtree-oracle --rust "$PWD/rust/target/debug/kicad-rtree-shim"
```

Use Linux with GCC/libstdc++ for native comparisons. On Mac the same small build
can run in an **already running** Linux container environment, capped at two CPUs.
This does not start KiCad, WSL, Windows builds or an engine. Mac libc++ equal-key
ordering is not this reference platform. No container image is published.

For full wasm runtime comparison, collect fresh native replies after exact native
comparison, then embed those temporary bytes in the import-free probe:

```sh
/tmp/rtree-live/rtree-oracle < port/cases/libs.kimath.include.geometry.rtree.application.pure.p001.jsonl > /tmp/rtree-live/native.jsonl
KICAD_RTREE_WASM_REFERENCE=/tmp/rtree-live/native.jsonl \
  cargo build --locked --manifest-path rust/Cargo.toml -p kicad-rtree-shim \
  --example corpus_parity --features live-wasm --target wasm32-unknown-unknown --release -j2
node rust/kicad-rtree-shim/wasm-test.mjs rust/target/wasm32-unknown-unknown/release/examples/corpus_parity.wasm
```

These temporary replies are not committed goldens or used to implement outputs.
The probe executes every operation again and compares entire integer-valued
results and identities. `live-wasm` is opt-in so ordinary workspace checks never
require a stored oracle reply file. The dedicated hosted x86-64 workflow executes
all these stages afresh. gcov counts are observations for instantiated paths and
include unexecuted branches, not a claim of complete generic-template coverage.
