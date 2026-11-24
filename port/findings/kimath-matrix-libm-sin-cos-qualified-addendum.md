<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Addendum: qualified evidence for the SetRotation libm dependence

Addendum to `kimath-matrix-libm-sin-cos.md` (unit
`libs.kimath.include.math.application.pure.p001`). It replaces that record's statement that a
`partial` status would cover the gap. A successful, non-deferred case whose result differs
between environments is not resolved by a status label.

## Qualified evidence

- **wasm.** An independent review compiled the exact library for `wasm32-unknown-unknown`
  (Rust 1.99.1, run under Node 24.16). 24 of the 275 corpus angles differ from native arm64.
  Example `set-rotation-29`, angle `4000c152382d7365`: cos is `bfdffffffffffffd` native and
  `bfdffffffffffffc` in wasm. The earlier `wasm32-wasip1` probe found the same 24 cases.
- **CPU variant.** The WSL2 Linux x86-64 run (Ryzen 9 5950X, glibc 2.43, GCC 15.2) used
  `GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX2,-FMA,-FMA4,-AVX`, which changes the loader's active
  feature bits. Over 2,000,275 fixed-seed angles (seed 20261123), glibc's `sin` changed in
  1,397 cases and `cos` in 1,196, by 1 ulp each. Example: `sin( bfc9720f0476d1e0 )` is
  `…77e5` by default and `…77e4` in the variant. None of the 275 original corpus angles is
  affected, so the original corpus could not detect this. The oracle imports `sincos`, Rust
  imports `sin` and `cos`, and within each variant all three agree on every probed angle.
  Corpus outputs of both programs were byte-identical across the two variants.

## Retained regressions

Ten of the probe's variant-sensitive angles are now corpus cases:
`set-rotation-variant-sin-0..4` and `set-rotation-variant-cos-0..4`. Within one environment C++
and Rust agree on them. A golden recorded on an FMA-variant machine differs from one recorded
on an SSE2-variant machine, so the corpus now exposes the variability instead of hiding it.

## Decision required (not taken here)

A deterministic portable `sin`/`cos` cannot match a reference whose own result depends on the
CPU. Any bit-exact prototype must first fix which reference variant it reproduces:

1. **Pin the reference variant and runtime**: the glibc version, and an explicit
   `GLIBC_TUNABLES` choice (or a CPU requirement) for the oracle in CI and in every golden
   run. Then port that glibc variant's double-precision `sin`/`cos` (and `sincos`, which
   matched) to portable Rust. For the FMA build, GCC's contraction sites must be reproduced
   with `mul_add`. Verify exactly against the pinned oracle on the corpus and a large fixed
   probe, natively and on `wasm32-unknown-unknown`.
2. **A plan gate defining transcendental results per host or with a stated tolerance.** This
   is a comparison policy change for the parent/user.

No prototype was built for this revision. Without a pinned variant there is no single
reference it could match, and the owner has no Linux environment to measure one. The current
port keeps calling the host libm, which matches the oracle natively on the same host.
