<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# COW/packed live comparison

Native JSONL host for unit `libs.kimath.include.geometry.rtree.application.pure.p002`.
The implementation and body audit are in `../kicad-rtree-cow-packed/README.md`.

`generate_cases.py` produces **153 programs / 21,440 steps**, fixed seed
`0xC0A5A002`, from public synthetic coordinates and request-local item IDs.
Both trees instantiate int/2D fanouts 4/8/16/28, int/3D fanouts 5/16,
int/1D fanouts 3/8, int64/2D/16 and int64/3D/8. COW nonempty 1D bulk is an
undefined-reference control, not a dropped successful operation. Equal keys,
odd/negative centers, touching/point/inverted/full-domain bounds, empty trees,
multi-level layouts, early stops, >64-level COW paths, 2,000-cell allocator chains,
clone divergence, destruction, moves, builder reuse and iterator contracts are
covered. Every supported configuration runs both trees. Counts measure programs,
not branch completeness; see the body audit for remaining limitations.

`generate_refusals.py` produces **27 separate controls**. These verify protocol,
resource, destroyed/moved-tree, undefined 1D bulk and iterator bounds refusals;
they never contribute to successful comparison counts. Signed-overflow refusals
are tested in Rust without executing native undefined behavior. The oracle does
not preflight all insert area/split overflows; do not feed arbitrary UB input to
it. Corpus arithmetic stays in the reference's defined domain, including
full-domain leaves that never evaluate area/extent arithmetic.

`native/prepare.py` exports all four rtree headers and JSON directly with
`git show` at KiCad pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`. It hashes
source/overlay bytes and records revision/dirty state. `native/build.sh` compiles
the real `port/oracle/shims/cow_packed_rtree.cpp` against those unmodified bytes,
with GCC/libstdc++, `-ffp-contract=off`, no fast-math, UBSan,
float-cast-overflow, no sanitizer recovery, and gcov instrumentation. No KiCad
application or GUI starts. Explicit-instantiation member access observes actual
COW refcounts/topology/allocator-chain memory and all packed storage rows/counts/
offsets. The main own-library oracle also registers this same adapter through
`registry.cpp`; its existing CMake shim glob includes it automatically.

Local Mac uses one `rust/target` and at most -j3. Example native host:

```sh
python3 rust/kicad-rtree-cow-packed-shim/native/prepare.py --out <stage>
bash rust/kicad-rtree-cow-packed-shim/native/build.sh <stage> <native-out>
cargo build --locked --manifest-path rust/Cargo.toml -p kicad-difftest -p kicad-rtree-cow-packed-shim -j3
rust/target/debug/kicad-difftest compare --unit libs.kimath.include.geometry.rtree.application.pure.p002 --root "$PWD" --oracle <native-out>/rtree-oracle --rust "$PWD/rust/target/debug/kicad-rtree-cow-packed-shim" --report <native-out>/compare.json
python3 rust/kicad-rtree-cow-packed-shim/compare_refusals.py --oracle <native-out>/rtree-oracle --rust "$PWD/rust/target/debug/kicad-rtree-cow-packed-shim"
```

Hosted `rust-port` discovers the unit's `[difftest]` registration and compares every
case with the audited full own-library oracle. The scoped
`rust-rtree-cow-packed-live.yml` independently rebuilds the sanitized pinned-header
adapter, exact-compares all programs, checks refusal controls, then compiles an
import-free wasm executable embedding **fresh native replies** through
`KICAD_COW_PACKED_WASM_REFERENCE` and replays the whole corpus in Node/V8.
This is live differential evidence, not a committed golden. Both workflows upload
revision/source/compiler/binary binding and comparison artifacts.

Initial validation: five portable behavior tests, the 27-control shim test,
focused fmt/clippy and portable wasm compilation pass on Mac arm64 Rust 1.96.1.
The expanded 153-program native comparison (zero deferred/failed) and full wasm
runtime replay pass against Linux arm64 GCC 12.2/libstdc++ with UBSan. The initial
151-program hosted x86-64 GCC 13.3/Rust 1.99.1 run also passed exact native and
wasm replay. Revision, digests, counts and coverage observations are retained in
`validation/2026-11-25-a189f6fe`; the PR links final hosted runs. All runtime source
is unchanged from implementation revision `a189f6fe4a87d78d69fa802b8cf567b22e7d0a1f`.

`native/coverage.py` retains actual gcov observations for both owned headers.
Local counters include repeated corpus runs and separate controls: 513/515
instantiated functions, 4,936/5,083 line entries and 2,643/3,340 branches executed.
The two unexecuted functions are nonempty 1D bulk-sort comparator instantiations
(the reference reads outside the coordinate array before reaching them). Empty 1D
bulk is separately exercised. Only source lines 452 (null ensureWritable return)
and 534 (not-owned assertion) have no execution in any configuration; both are
private guards unreachable through the exposed valid tree API. Instantiation and
branch counts are compiler-specific observations, not complete generic coverage.
