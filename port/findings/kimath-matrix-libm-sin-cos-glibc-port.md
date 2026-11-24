<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Addendum: portable port of the pinned glibc sin/cos/sincos

Addendum to `kimath-matrix-libm-sin-cos.md` and `-qualified-addendum.md` (unit
`libs.kimath.include.math.application.pure.p001`). It takes option 1 of the qualified
addendum for the pinned reference: glibc 2.39 as shipped in Ubuntu `libc6 2.39-0ubuntu8.12`
(`libm.so.6` SHA-256 `5dcc4d25466e29ca390ed6fabb371c69b65dff6fa4e91c34561057a020d6994a`,
build ID `f772fc0c42a75e38f138f8b66f1ebc8aad060525`), with the baseline SSE2 variants selected
by `GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`.

## What the oracle calls

GCC folds `SetRotation`'s `cos( aAngle )` and `sin( aAngle )` into one `sincos` call for both
instantiations: the PR50 oracle (`kicad-oracle` SHA-256 `0bac7567…`) has exactly two calls,
`sincos@plt` in `DoubleOps` (`MATRIX3x3<double>`) and in `IntOps` (`MATRIX3x3<int>`, after
`cvtsi2sd`, followed by truncation), and imports no `sin` or `cos`. `__sincos_sse2` is
evaluated differently from `__sin_sse2` and `__cos_sse2`: for `0.855469 <= |x| < 2.426265` it
reduces with `a = y + hp1, da = (y - a) + hp1` where `__sin` uses `do_cos (hp0 - |x|, hp1)`,
and for larger `|x|` it negates the reduced argument instead of the result. On the 2,000,000
sweep angles and 101,602 domain inputs the source model never gave `sincos` bits different
from `sin`/`cos`, but that is not a proof, so the three functions are ported separately and
`SetRotation` uses `sincos`.

## Port

`kicad_math::glibc_trig` (private) ports `s_sin.c`, `s_sincos.c`, `branred.c`, `branred.h`,
`usncs.h` and `sincostab.c` of the Ubuntu source package `glibc_2.39-0ubuntu8.12` (orig
`f77bd47c…`, debian `cc7cf38a…`). No Ubuntu patch touches these files or the x86-64 multiarch
wrappers. Source identity rests on the hashes listed in the `.dsc`: neither its PGP signature
nor the archive's signed `Sources` index was verified; binary equivalence is shown by the
checks below instead.

The port covers every branch, both argument reducers (`reduce_sincos`, and the 2/pi-digit
`__branred` for `|x| >= 105414350`), signed zeros, subnormals, infinities (default NaN
`0xfff8000000000000`, as SSE2 `divsd` gives for `inf / inf`) and NaNs (returned quietened with
sign and payload, as `x / x` gives). Finite paths are the source's IEEE operations in order,
without fused multiply-add or division; there is no host libm and no lookup of recorded
results. The `__sincostab` table and every constant equal the bytes the locked machine code
loads. The glibc files are LGPL-2.1-or-later; `glibc_trig.rs` keeps their IBM and FSF notices
and, as LGPL-2.1 section 3 permits, is distributed under GPL-3.0-or-later like the crate.

## Evidence

- A Python transliteration of the source equals the genuine outputs on the 2,000,000-angle
  contract sweep, the 37 probe values (`sin`, `cos`, `sincos`) and all 650 set-rotation oracle
  replies (debug and release; including the 24 + 6 angles where std differed natively and in
  wasm).
- A static emulator of the locked `__sin_sse2`, `__cos_sse2`, `__sincos_sse2` and `__branred`
  machine code (objdump of the exact ELF, constants read from it) equals that transliteration
  on 1,602 branch-boundary inputs plus 300,000 fixed-seed full-domain inputs, executing every
  instruction except padding, the stack-protector failure, the non-round-to-nearest MXCSR
  save/restore paths, and one `__branred` path for `x1` exponents below 427, which no valid
  input reaches.
- The transliteration, emulator and comparison scripts are private scratch tooling and are
  not part of this repository; their results are recorded here and in the data file headers.
  The genuine data came from candidate reference image `8b70857a…` (environment `1b4…`),
  which remains UNQUALIFIED.
- The Rust module is a line-by-line port of the same source; its unit tests compare it with the
  retained genuine data and the emulated domain. A parent run at commit `92448b5b` on macOS
  arm64 (Rust 1.99.1, Node 24.16.0) compiled the package and ran its tests natively in debug
  and release and as `wasm32-wasip1` tests in debug and release under Node's WASI: each run had
  17 tests pass and 2 ignored, and clippy `--all-targets -D warnings` was clean. A
  `wasm32-unknown-unknown` export of the public `SetRotation` (debug and release) matched all
  325 retained genuine set-rotation replies (285 double, 40 int) and the 37 retained genuine
  `sincos` values, with 0 mismatches and no host imports. These compare with retained genuine
  records; the two ignored fresh full-domain reference tests have NOT RUN, and no fresh C++
  oracle or Linux run is claimed.

The claim that results agree on every target follows from construction: IEEE binary64
operations in a fixed order, no contraction, no host libm, under the default floating-point
environment (round to nearest, no flush-to-zero or denormals-are-zero). Only the runs listed
here measure it.

## Remaining conditions

- The genuine full-domain comparison (`port/glibc-trig/README.md`, workflow
  `rust-glibc-trig.yml`) must pass natively (debug, release), as `wasm32-wasip1` tests (debug,
  release) and through `SetRotation` in `wasm32-unknown-unknown` (debug, release). It is a
  targeted check of this prerequisite: the math unit's full corpus gate stays as it is and
  still fails on the known mixed-NaN cases until those are integrated.
- The live oracle must run with the SSE2 variant (the reference image sets the tunables);
  on an FMA-capable runner without `GLIBC_TUNABLES` the `set-rotation-variant-*` cases differ
  by design.
- Not modelled because they do not change values: `errno`, the underflow flag for subnormal
  arguments, and the caller's rounding mode (the C functions force round-to-nearest; the
  reference process runs with MXCSR `0x1f80`).
