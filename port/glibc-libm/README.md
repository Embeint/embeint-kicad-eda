<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Pinned glibc angle backends

This is qualification tooling, not a runtime dependency or a golden generator.
The private `kicad_math::glibc_libm` ports glibc 2.39's baseline SSE2 `atan`,
`atan2`, `tan`, `asin`, `acos`. `pow` remains an owned typed deferral. The
accepted sin/cos/sincos bodies are unchanged; only `branred`'s visibility and its
test-module path change so tan can reuse the exact accepted reducer and the
import-free qualification wrapper can compile those private sources.

## Source and licence

Canonical source: GNU glibc 2.39, the Ubuntu `glibc_2.39-0ubuntu8.12` source
package. `source.json` pins the original archive and each file used. The archive
SHA-256 is `f77bd47cf8170c57365ae7bf86696c118adb3b120d3259c64c502d3dc1e2d926`;
the Debian patch archive SHA-256 is
`cc7cf38a3060a282011e83c946b6cec1aebe9b04e21e133449abe28340a710f6`.
No Ubuntu patch touches the ported source files or multiarch wrappers (inspected
patch set). As with glibc_trig, the `.dsc` hashes were checked, not its PGP
signature or the source archive signature. Binary equivalence is a separate
fresh runtime measurement.

The IBM Accurate Mathematical Library/FSF notices are retained in each derived
file. The public inverse wrappers also preserve the Sun Microsystems notice
for `k_standard.c`'s permissively licensed domain cases. The source is LGPL-2.1-or-later, GPL-compatible. This derivative uses
LGPL-2.1 section 3 to distribute it under GPL-3.0-or-later, like kicad-math.
`check_sources.py` verifies original coefficient/table bits against those sources;
the tables are glibc polynomial coefficients, not recorded function answers.
`powtwo.tbl` is replaced by exact binary64 power-of-two bit construction.

## Dispatch and floating environment

Use the same setting as the geometry oracle and glibc_trig:
`GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`.
`atan`/`atan2`/`tan` use `ifunc-avx-fma4.h`. `asin`/`acos` use `ifunc-fma4.h`.
Both select SSE2 under these tunables. No further dispatch setting is needed.
All five 2.39 bodies have no multiprecision fallback. The non-FMA `dla.h`
EMULV and MUL12 are different algorithms and are preserved separately.

`qualification.json` gives the offsets in the exact libm locked by
`../glibc-trig/reference.lock.json` (same libc6 2.39-0ubuntu8.12 package,
libm SHA-256 `5dcc4d25466e29ca390ed6fabb371c69b65dff6fa4e91c34561057a020d6994a`).
Offsets were read from build-ID debug file
`f772fc0c42a75e38f138f8b66f1ebc8aad060525`, SHA-256
`1f04267ef6a17ca283bee49009e7e5cca022d461ccdaeb519b217d4970f28157`
(`libc6-dbg_2.39-0ubuntu8.12_amd64.deb`, SHA-256
`f3cc19b512e408de0f8a4f6e1bc7a97dcb48aacd075e4cc31a0d0f88fa3873f0`).
The probe checks loaded file identities, `atan` and `tan` resolved offsets, the
three public inverse wrapper offsets, **and the actual IFUNC GOT slots each
wrapper calls**. It executes the public functions through volatile bit inputs
and function pointers. Nothing links a source model in place of the oracle.

Round-to-nearest and gradual underflow are required; the probe refuses unless
MXCSR control is `0x1f80` and x87 control is `0x037f`. Only returned values are
modelled, not errno/exception flags or non-default rounding environments.
Special NaN paths explicitly preserve SSE2 sign/payload/quieting, including
atan2's first x-NaN path (disassembly `addsd %xmm0,%xmm1` selects x). Tan's infinite inputs produce SSE2's negative indefinite NaN. The public
asin/acos wrappers return **positive** `NAN` for ordered `|x|>1` (including
infinities), through `k_standard.c` cases 2/1 at the default `_LIB_VERSION ==
_POSIX_` (2). The probe pins that mode at `libm+0xe8008`; IEEE bypass/SVID modes
and user matherr hooks are outside this reference contract. NaNs bypass the
ordered domain comparison and retain their quieted payload. The first fresh
run exposed this wrapper distinction; no input or expected record was changed.

## Qualification

`domain.py` writes 517,520 fixed pairs plus 1,000,000 fixed-seed pairs, **1,517,520
calls per function per target/profile**. Fixed cases cover all table-bin
boundaries and neighbouring doubles, all exponents, the atan2 +/-57 exponent-gap
branches, reciprocal range edges, subnormals, signed zero, infinities, ordered
quiet/signalling NaN payload pairs, and tan's three argument-reduction edges.
Seeded modes cover full raw bits, the inverse functions' successful domain,
near-one root regions, correlated atan2 magnitudes, and huge tan arguments.
No tolerance, case exclusions, result lookup or premature golden.

The `rust-glibc-libm.yml` job downloads and hash-checks the locked libc package,
binds it to the reviewed input lock, runs only its own loader with a cleared
environment, and compares native debug/release, WASI debug/release, and import-free
wasm32-unknown-unknown debug/release bit for bit. The last wrapper compiles the
same private source files under `forbid(unsafe_code)`; the separate existing
geometry job checks the complete public EDA_ANGLE continuations against C++ and
then import-free wasm. All local builds use `rust/target` and at most -j3.

A green report requires one full-count, zero-mismatch result for each of five
functions on all six configurations. `report.json` hashes the domain, genuine
records, probe, source verification and logs. CI artifacts are fresh evidence;
the implementation never reads them. Run:

```
python3 port/glibc-libm/reference.py domain --out <out>
# Put the reviewed inputs.lock.json in <out> first (workflow fetches its commit).
python3 port/glibc-libm/reference.py reference --out <out>
KICAD_GLIBC_LIBM_REFERENCE=<out>/reference.bin KICAD_GLIBC_LIBM_COUNT=1517520 \
  cargo test --locked --manifest-path rust/Cargo.toml -p kicad-math --lib \
  glibc_libm -- --include-ignored --nocapture
```

Executed results and remaining objections are recorded in the additive finding
`port/findings/geometry-angle-libm-backends-addendum.md`, not inferred here.
