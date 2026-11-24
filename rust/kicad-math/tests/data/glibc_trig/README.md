<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# glibc_trig test data

Data for `kicad_math::glibc_trig` (private), the portable bit-exact port of glibc 2.39's
baseline SSE2 `sin`, `cos` and `sincos` (`libc6 2.39-0ubuntu8.12`, `libm.so.6` SHA-256
`5dcc4d25466e29ca390ed6fabb371c69b65dff6fa4e91c34561057a020d6994a`, build ID
`f772fc0c42a75e38f138f8b66f1ebc8aad060525`, selected by
`GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`). The unit tests in
`src/glibc_trig/tests.rs` embed these files with `include_str!`, so they run unchanged
natively and in WebAssembly.

| File | Kind | Content |
| --- | --- | --- |
| `genuine_libm_values.txt` | runtime outputs of the locked glibc | 37 probe inputs (zeros, subnormals, branch edges, huge values, infinities, quiet and signalling NaNs): `sin`, `cos`, `sincos` |
| `genuine_set_rotation.txt` | runtime outputs of the PR50 oracle | all 285 double and 40 int `set-rotation` corpus cases; the 30 angles where std's native and wasm `sin`/`cos` differed are marked `1` |
| `genuine_sweep_sample.txt` | runtime outputs of the locked glibc | 3,199 records of the 2,000,000-angle contract sweep (separate `sin` and `cos` calls) |
| `emulated_domain.txt` | **static**, not a runtime oracle | 1,602 branch-boundary inputs and the first 1,000 seeded domain inputs; outputs of a static emulation of the locked machine code, each also equal to the glibc source model |

Each file's header names its source artifact and SHA-256. The genuine data comes from the
PR55 candidate reference image `8b70857a…` (environment `1b4…`), which is not qualified.

## Full-domain reference (hosted CI)

`reference_write_domain` (ignored) writes the 1,602 boundary inputs followed by
`KICAD_GLIBC_TRIG_COUNT` (default 1,000,000) fixed-seed inputs (seed `53696e436f73476c`,
eight interleaved families: uniform bit patterns, all exponents, the huge-argument reducer,
near multiples of pi/2, near branch thresholds, near table edges, the principal range, integer
and tenth-degree angles). The harness in `port/glibc-trig` (workflow
`.github/workflows/rust-glibc-trig.yml`) builds a probe against the genuine locked glibc,
which refuses unless `sin`, `cos` and `sincos` resolve to the locked SSE2 implementations, and
writes `x, sin, cos, sincos_sin, sincos_cos` for each input; `reference_records` (ignored)
compares every record bit for bit, natively (debug and release), as a `wasm32-wasip1` test
binary under Node's WASI, and through `SetRotation` in `wasm32-unknown-unknown`. Emulated rows
never stand in for that fresh reference.

Status: a parent run at commit `92448b5b` on macOS arm64 (Rust 1.99.1, Node 24.16.0) compiled
the package and ran its tests natively in debug and release and as `wasm32-wasip1` tests in
debug and release under Node's WASI: each run had 17 tests pass and 2 ignored, and clippy
`--all-targets -D warnings` was clean. A `wasm32-unknown-unknown` export of the public
`SetRotation` (debug and release) matched all 325 retained genuine set-rotation replies (285
double, 40 int) and the 37 retained genuine `sincos` values, with 0 mismatches and no host
imports. These compare with retained genuine records; the two ignored fresh full-domain
reference tests have NOT RUN, and no fresh C++ oracle or Linux run is claimed.
