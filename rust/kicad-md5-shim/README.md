<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MD5 compatibility pilot

The candidate scope is `libs.kimath.src.md5_hash.application.pure.p001`,
`libs/kimath/src/md5_hash.cpp:1-289` at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, with the supporting class declaration
in `libs/kimath/include/md5_hash.h`. Final generated extraction records must
confirm ownership before a unit record or claim is published.

`kicad-md5` is the portable implementation. It forbids unsafe code, uses the
RustCrypto `md-5` block compressor, and keeps KiCad's wrapper transitions.
This crate is a native stdin/stdout adapter, explicitly `portable = false`.
`port/oracle/shims/md5.cpp` adapts the real pinned `MD5_HASH` class, without
reimplementing the reference algorithm. The existing oracle source/flag audits,
protocol and golden-review checks remain in force.

## Input and observation contract

Requests use the existing exact JSONL unit/case protocol, with
`input = {"steps": [...]}`. Each step has `op` and an integer `id` from 0 to 15.
There are 1..4096 steps. Unexpected fields are refused.

| Operation | Additional fields | Calls into the reference |
| --- | --- | --- |
| `new` | none | default constructor (replaces any old state) |
| `clone` | `source`: existing state ID | copy constructor |
| `assign` | `source`: distinct existing state ID; destination must exist | assignment |
| `init` | none | `Init()` |
| `bytes` | `data`: array of 0..255 integers; optional `repeat` defaults to 1 | `Hash(uint8_t*, uint32_t)` per repetition |
| `int` | `value`: signed 32-bit integer | `Hash(int)` |
| `finalize` | none | `Finalize()` |
| `valid` | `value`: boolean | `SetValid(value)` |
| `inspect` | none | public observations only |
| `compare` | `other`: existing state ID | both `==` and `!=` |

`data` has at most 65536 bytes, `repeat` is 0..8193, and total hashed bytes
(including integer calls) are at most 600 MiB per request. These are harness
resource bounds, not restrictions added to the portable algorithm. The portable
`hash(&[u8])` checks the upstream uint32 count before any mutation. No pointer
length mismatch, unconstructed object, private-state injection or invalid memory
is sent to C++. Self-assignment is excluded because the reference's `memcpy`
would copy between overlapping buffers; distinct-object assignment is exercised
throughout. Integer cases require the pinned Linux ABI: 8-bit bytes,
32-bit `int`, little endian (enforced by the oracle shim's static assertion).

Each step returns one event. `compare` emits both actual boolean operators;
other steps emit the target ID, `IsValid()`, `Format(true)`, `Format(false)` and
`Format()` with its real default. Exact format strings encode all sixteen digest
bytes, including the unusual B..G nibble mapping and the final space in spaced
mode. They are not normalized to standard hexadecimal. Context bytes are private
and are not directly observable; copied/divergent continuations and subsequent
finalization test their effects. No private layout or invented state is exposed.

At this pin, `Init` explicitly zeros the full buffer and stored hash. Observing
the initial hash therefore has a defined result; it is not a substitute digest
for an uninitialized C++ object. `Hash` does not invalidate or replace the stored
digest. `Finalize` mutates the running context without clearing its partial byte
count. Repeated `Finalize`, hashing after it, copying such states, independent
validity mutation and equality ignoring validity are all preserved.

## Public cases

`python rust/kicad-md5-shim/generate_cases.py` reproduces the corpus byte for byte
with LF UTF-8. All inputs are generated; no design data is used. The initial
corpus has 5,972 requests:

- Whole strings and every two-way split (including empty chunks) for lengths
  0..65, 119/120/127/128/129, 255/256/257 and 1024. Every split copies and assigns
  the partial state, completes all three states and compares actual digests.
- Strides 1/2/3/7/31/55/56/63/64/65 for each selected length, and **every**
  composition of strings of length 2..8. This is not a claim to enumerate the
  exponentially many partitions of every long string.
- Both final padding paths, partial/full/multiblock transitions, repeated
  finalization, updates after finalization, copied finalized contexts, reset,
  independent validity, unequal/equal digests and repeated assignment.
- Fifteen signed integer boundary values compared with their explicit
  little-endian bytes; 128 fixed-seed byte streams (`0x4cf939`).
- Seven public RFC 1321 byte strings and explicit zero-repeat/empty-update no-ops.
- A stream reaching 512 MiB with generated chunks, crossing the low 32-bit bit
  counter without allocating the entire stream. A copy just before the boundary
  instead crosses that counter during repeated finalization. Further copies and
  continuations check the retained high counter.

All 64 MD5 compression rounds come from the existing crate; the live cases test
their effect with public bytes covering all byte values. Context counters beyond
the 512 MiB carry and a full 64-bit bit-count wrap are not practically enumerated;
wrapping arithmetic preserves the unsigned reference transitions. No CI or parity
claim follows from preparing or storing cases. Evidence and audited symbol/branch
mapping are recorded separately after actual execution.

## Checks

From a Linux rsync copy under `~/kicad-port/pilot-md5`, with the shared heavy-build
lock acquired only when available and all Cargo/Ninja jobs capped at 8:

```sh
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo clippy --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim --all-targets -j8 -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml -p kicad-md5 -p kicad-md5-shim -j8
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-md5 --target wasm32-unknown-unknown -j8
```

Use the existing `kicad-difftest compare`, `propose`, `bless` and `check` commands
with the real oracle and this native shim. Only a committed, clean overlay may
supply the baseline provenance. A first blessing records null old values and
complete new replies in a new immutable evidence artifact. CI discovers the
unit's eventual `[difftest]` registration and runs live oracle/Rust/golden checks;
the workflow itself needs no pilot-specific edit.
