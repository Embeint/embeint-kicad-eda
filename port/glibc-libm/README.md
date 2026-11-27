<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Pinned glibc angle backends

This is qualification tooling, not a runtime dependency or a golden generator.
The private `kicad_math::glibc_libm` ports glibc 2.39's baseline SSE2 `atan`,
`atan2`, `tan`, `asin`, `acos` and `pow` (the `EDA_ANGLE::Round` callee) and
`fma` (`__fma_sse2`, the explicit `std::fma` calls of trigo.cpp's three-point
`CalcArcCenter`), and qualifies the shared `kicad_math::hypot` (the
`VECTOR2<double>::EuclideanNorm` `std::hypot` callee). `kicad_math::reference_libm`
forwards each of them unchanged. The
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
for `k_standard.c`'s permissively licensed domain cases. The GNU/IBM source is
LGPL-2.1-or-later, GPL-compatible. This derivative uses
LGPL-2.1 section 3 to distribute it under GPL-3.0-or-later, like kicad-math.
`check_sources.py` verifies original coefficient/table bits against those sources;
the tables are glibc polynomial coefficients, not recorded function answers.
`powtwo.tbl` is replaced by exact binary64 power-of-two bit construction.

`pow` is the 2018 Arm optimized-routines derivative in glibc's dbl-64
`e_pow.c` (FSF copyright, LGPL-2.1-or-later, notice retained in `pow.rs` and
`pow_table.rs`), built as x86_64 `__ieee754_pow_sse2`: no `__FP_FAST_FMA`, no
`TOINT_INTRINSICS`, `FLT_EVAL_METHOD` 0 (`math_narrow_eval` is the identity),
`WANT_ROUNDING` and `HIGH_ORDER_BIT_IS_SET_FOR_SNAN` 0. `check_sources.py`
also verifies every `e_pow_log_data.c` and `e_exp_data.c` field read by pow
(the scaled poly initializers fold exactly); the unused pad column is omitted.
The same 657 words were separately matched against `__pow_log_data` and
`__exp_data` in the locked libm image. `math_err.c`'s helpers keep their
values (`0x1p769`/`0x1p-767` squares, signed division by zero, SSE2's negative
indefinite for `(x - x) / (x - x)`); errno and flags are not modelled.

`hypot` is glibc 2.39 `sysdeps/ieee754/dbl-64/e_hypot.c` (FSF, 2021-2024,
LGPL-2.1-or-later; notice retained in `rust/kicad-math/src/hypot.rs`), non-FMA
kernel. `check_sources.py` verifies its `SCALE`/`LARGE_VAL`/`TINY_VAL`/`EPS`
exponents against `hypot.rs`, the generic `USE_FMAX_BUILTIN`/`USE_FMIN_BUILTIN` 0
(ternary operand selection), the empty dbl-64 `w_hypot.c` and the `GLIBC_2_35`
version. Its finite body is the accepted shared one also used by BOX2; this
qualification adds the non-finite branch and covers both.

`fma` is glibc 2.39 `sysdeps/ieee754/dbl-64/s_fma.c` (FSF, 2010-2024,
LGPL-2.1-or-later; notice retained in `rust/kicad-math/src/glibc_libm/fma.rs`) as
`sysdeps/x86_64/fpu/multiarch/s_fma.c` builds it into `__fma_sse2`: generic
`USE_FMA_BUILTIN` 0 and x86 `TININESS_AFTER_ROUNDING` 1. `check_sources.py` pins
all four files and checks those settings, the special returns and the Dekker
constant. Its final addition runs in `FE_TOWARDZERO` and reads `FE_INEXACT`
(round to odd); the port computes that sum and flag exactly from the
round-to-nearest sum and its TwoSum error, as Rust cannot set the rounding mode.
Only its special returns can make a NaN. The locked body compiles `x * y + z` as
`mulsd %xmm1,%xmm0; addsd %xmm2,%xmm0`, `x * y` as `mulsd %xmm1,%xmm0` and
`(z + x) + y` as `(x + z) + y` (z is the only possible NaN there), so x's NaN
wins over y's and the product's (or its invalid indefinite) over z's.

## Dispatch and floating environment

Use the same setting as the geometry oracle and glibc_trig:
`GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`.
`atan`/`atan2`/`tan` use `ifunc-avx-fma4.h`. `asin`/`acos`/`pow` use
`ifunc-fma4.h`. Both select SSE2 under these tunables. Callers built against
2.39 bind `pow@@GLIBC_2.29` (libm+0x3a630), the `w_pow_template.c` wrapper,
which only sets errno and returns the `__ieee754_pow` value unchanged; the SVID
`pow@GLIBC_2.2.5` compat wrapper is not involved. The probe checks that
default/versioned binding and the wrapper's IRELATIVE GOT slot (+0xe7f58 ->
`__ieee754_pow_sse2`, +0x2b810). Its NaN `x + y` returns follow the kernel's
`addsd` with x as the first operand: a NaN x wins, quieted, else a NaN y. No further dispatch setting is needed.
Callers built against 2.39 bind `hypot@@GLIBC_2.35` (libm+0x28450), which is
e_hypot.c's `__hypot` itself (also `__hypot_finite@GLIBC_2.15`): dbl-64
`w_hypot.c` is empty and x86_64 has no multiarch hypot, so there is no IFUNC or
GOT slot and the tunables do not affect it. The SVID `hypot@GLIBC_2.2.5`
(`__hypot_compat`, libm+0x12db0, `_LIB_VERSION`/matherr overflow handling) is not
involved; the probe checks both bindings. Disassembly of the locked body shows no
FMA and the non-finite `return x + y` as `addsd %xmm1,%xmm0` (x first): a NaN x
is returned quieted, else the quieted NaN y; an infinity returns the +inf constant
unless an operand is signalling (`issignaling_inline`).
`fma@@GLIBC_2.2.5` (libm+0x33320) is an IFUNC choosing `__fma_fma3`,
`__fma_fma4` or `__fma_sse2`; under these tunables `dlsym` and the oracle's
`fma@plt` resolve `__fma_sse2` (libm+0x32810), which the probe checks.
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
atan2's first x-NaN path (disassembly `addsd %xmm0,%xmm1` selects x). Tan's
infinite inputs produce SSE2's negative indefinite NaN. The public
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

`pow_domain.py` separately writes 535,736 fixed pairs plus 1,000,000 seeded
pairs, **1,535,736 pow calls per target/profile**, as 24-byte (x, y, pow)
records (`reference.py pow-domain` / `pow-reference`). Fixed cases follow
e_pow.c: special operand products (signalling/quiet NaN order, signed zero,
infinities), the |y| < 2^-65 and >= 2^63 checks, checkint at every exponent,
every log_inline table bin across normal and subnormal exponents, x near one,
exp_inline's 512/1024 thresholds, the overflow and subnormal/underflow
specialcase edges with positive and negative bases, and 10^d for every
d in [-1100, 1100] (all `EDA_ANGLE::Round` digit counts reach these edges).
The generator uses only exact and `decimal` arithmetic, never the host libm;
its hash is the same on Windows and Linux.

`hypot_domain.py` writes 607,712 fixed pairs plus 1,000,000 seeded pairs,
**1,607,712 hypot calls per target/profile** (`reference.py hypot-domain` /
`hypot-reference`, 24-byte records). Fixed cases follow e_hypot.c: the full
Cartesian product of signed zeros, subnormal/normal edges, 0x1p+511, 0x1p-459,
DBL_MAX, infinities and four quiet plus three signalling NaN payloads; every
exponent (including subnormal) against relative exponents around the 0x1p-54
shortcut and kernel in both orders; both scaling thresholds with their shortcut
and kernel continuations; the shortcut at every exponent; the kernel's
h <= 2 * ay split (ay/ax near 1/sqrt(3)); overflow next to DBL_MAX; exact
Pythagorean triples at every scale down to 2^-1074 and subnormal pairs. Seeded
modes cover raw bits, related exponents, both scaling regions, the kernel split
and random non-finite operands. Only `decimal` supplies the two irrational
constants; the hash is the same on Windows and Linux.

The `rust-glibc-libm.yml` job downloads and hash-checks the locked libc package,
binds it to the reviewed input lock, runs only its own loader with a cleared
environment, and compares native debug/release, WASI debug/release, and import-free
wasm32-unknown-unknown debug/release bit for bit. The last wrapper compiles the
same private source files under `forbid(unsafe_code)`; the separate existing
geometry job checks the complete public EDA_ANGLE continuations against C++ and
then import-free wasm. All local builds use `rust/target` and at most -j3.

`fma_domain.py` writes 145,213 fixed triples plus 1,000,000 seeded triples,
**1,145,213 fma calls per target/profile**, as a 24-byte (x, y, z) domain and
32-byte (x, y, z, fma) records (`reference.py fma-domain` / `fma-reference`).
Fixed cases follow s_fma.c: the special-operand returns over a signed Cartesian
product of zeros, subnormal/normal edges, 2^+-600, DBL_MAX, infinities and three
quiet plus two signalling NaN payloads; every exponent-adjustment branch and its
boundaries (ue + ve against 968, 1076, 3017 and 3070; ue, ve, we against 1994;
we against 0..4, 53, 218); exact zero results of both signs; cancellation against
the rounded product (z = -fl(x * y) and neighbours, leaving the round-to-odd
term); halfway and subnormal results. Seeded modes cover raw bits, cancellation,
near-overflow and tiny products, huge z, non-finite operands and moderate values.

A green report requires one full-count, zero-mismatch result for each of eight
functions (pow, hypot and fma with their own counts) on all six configurations. `report.json` hashes the domain, genuine
records, probe, source verification and logs. CI artifacts are fresh evidence;
the implementation never reads them. Run:

```
python3 port/glibc-libm/reference.py domain --out <out>
# Put the reviewed inputs.lock.json in <out> first (workflow fetches its commit).
python3 port/glibc-libm/reference.py reference --out <out>
python3 port/glibc-libm/reference.py pow-domain --out <out>
python3 port/glibc-libm/reference.py pow-reference --out <out>
python3 port/glibc-libm/reference.py hypot-domain --out <out>
python3 port/glibc-libm/reference.py hypot-reference --out <out>
python3 port/glibc-libm/reference.py fma-domain --out <out>
python3 port/glibc-libm/reference.py fma-reference --out <out>
KICAD_GLIBC_LIBM_REFERENCE=<out>/reference.bin KICAD_GLIBC_LIBM_COUNT=1517520 \
KICAD_GLIBC_LIBM_POW_REFERENCE=<out>/pow-reference.bin KICAD_GLIBC_LIBM_POW_COUNT=1535736 \
KICAD_GLIBC_LIBM_HYPOT_REFERENCE=<out>/hypot-reference.bin KICAD_GLIBC_LIBM_HYPOT_COUNT=1607712 \
KICAD_GLIBC_LIBM_FMA_REFERENCE=<out>/fma-reference.bin KICAD_GLIBC_LIBM_FMA_COUNT=1145213 \
  cargo test --locked --manifest-path rust/Cargo.toml -p kicad-math --lib \
  glibc_libm -- --include-ignored --nocapture
```

Executed results and remaining objections are recorded in the additive findings
`port/findings/geometry-angle-libm-backends-addendum.md`,
`port/findings/geometry-angle-libm-pow-addendum.md` and
`port/findings/vector-hypot-glibc-addendum.md`, not inferred here.
