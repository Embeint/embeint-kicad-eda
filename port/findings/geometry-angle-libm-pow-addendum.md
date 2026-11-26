<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Addendum: glibc 2.39 pow backend for EDA_ANGLE::Round

Owner: `libs.kimath.include.geometry.application.ui.p001` (the existing owned
`libc::pow` deferral in `eda_angle.rs`). Base: `c45b38a2960fe8ccc67dace5f3bea8de96c0bf0c`
(`origin/rust`). Addendum to the immutable `geometry-angle-libm-portability.md`
and `geometry-angle-libm-backends-addendum.md`. No new unit or extractor unit is
created; glibc remains a qualification reference, not a ported KiCad unit.

The private `glibc_libm::pow` ports glibc 2.39 `sysdeps/ieee754/dbl-64/e_pow.c`
as the x86_64 multiarch `__ieee754_pow_sse2` build (non-FMA `log_inline` and
`exp_inline` splits, `Shift` rounding instead of `TOINT_INTRINSICS`,
`FLT_EVAL_METHOD` 0, `WANT_ROUNDING`), the `math_err.c` special-value helpers
and the public `pow@@GLIBC_2.29` wrapper, which only sets errno. Tables and
constants are original `e_pow_log_data.c`/`e_exp_data.c` bits, verified both
against the hash-pinned source (`check_sources.py`) and, once, against
`__pow_log_data`/`__exp_data` in the locked libm image (657 words). They are
algorithm coefficients, not recorded pow answers. NaN returns are explicit:
the kernel's `x + y` is `addsd` with x first (disassembly at libm+0x2bb00), so a
NaN x is returned quieted, then a NaN y; `x * x` of a NaN x is that quieted x;
`1 / x2` of a NaN keeps it; the non-integer negative-base path returns SSE2's
`0xfff8000000000000`. Debug-overflowing C wraparound (`2 * ix`) is written as
explicit shifts and `wrapping_*`; no `unsafe`, panics or global state.

`EDA_ANGLE::Round` now evaluates `pow( 10.0, digits )`, `KiROUND`, the second
`pow` and the division in the pinned `eda_angle.h` order. The `UNPORTED` tag and
`scalar_pow` stub are removed.

## Owned branch audit

| e_pow.c path | Ported body | Genuine evidence | Gap |
| --- | --- | --- | --- |
| y = +-0; x = 1 (signalling operand -> `x + y`) | yes | special Cartesian product, NaN payload pairs | none in value contract |
| NaN operand `x + y`, |x| = 1 with infinite y, |x| <> 1 with infinite y, `y * y` | yes | specials; infinite y with x around 1, both signs | none |
| zero/inf/NaN x: x2, odd-y negation, `__math_divzero`, `1 / x2` | yes | signed zero/inf/NaN x with every checkint class | none |
| negative finite x: checkint 0 (invalid), 1 (sign bias), 2 | yes | every exponent 0x3fe..0x435 with odd/even/fraction mantissas | none |
| |y| < 2^-65 (1 + y, 1 - y), |y| >= 2^63 (oflow/uflow), x = 1 | yes | all exponents around both thresholds | none |
| subnormal x normalization | yes | every subnormal exponent | none |
| log_inline (non-FMA split, 128 bins) | yes | every bin edge, normal and subnormal exponents | none |
| exp_inline tiny (1 + x), >= 1024 (uflow/oflow with sign), 512..1024 specialcase | yes | thresholds for bases 2, 10, e, 0.5, 0.1, 3, 1.5, 7, both signs | none |
| specialcase k > 0 (0x1p1009 scaling) and k < 0 (subnormal double-rounding fix, signed zero) | yes | 2^e for e in [-1080, 1030), +-0.5 steps, negative bases | none |
| `__math_check_oflow`/`check_uflow`, wrapper errno | values only | n/a | errno and flags are outside the value contract |

## Executed qualification (local)

Domain `pow_domain.py`: 535,736 fixed + 1,000,000 seeded = 1,535,736 pairs,
SHA-256 `6160e082f99eae22c25519fdadd09d3a388c180995c695097eb43a97fe76e7c3`
(identical on Windows Python 3.12 and WSL Ubuntu Python; no host libm used).
Genuine records came from the locked libc6 2.39-0ubuntu8.12 package through
its own loader under the unchanged tunables. The probe confirmed loader/libc/libm
identity, `_LIB_VERSION`, all existing dispatch checks, `pow@@GLIBC_2.29` at
libm+0x3a630 and its GOT slot +0xe7f58 -> `__ieee754_pow_sse2` (+0x2b810).
Reference SHA-256 `130c80bff1e50c35039789326e7b55825ac058e0eb2e17a16d70f0f4254ac895`.

| Configuration | pow records | Mismatches |
| --- | --- | --- |
| native x86_64-pc-windows-msvc debug / release (Rust 1.99.0) | 1,535,736 each | 0 / 0 |
| import-free wasm32-unknown-unknown debug / release (Node 22.22.1) | 1,535,736 each | 0 / 0 |

WASI and native Linux debug/release, and the full six-configuration report for
all six functions, run in the `rust-glibc-libm` workflow; those hosted results
are the CI gate and are reported in the draft PR, not here.

## Corpus effect (local Rust replies, before the C++ gate)

Measured by running the base shim (`c45b38a296`) and this branch's shim on
the same inputs:

| Corpus | Base results / deferred | This branch results / deferred |
| --- | --- | --- |
| original 21,538 requests | 19,102 / 2,436 (pow 1,642; util.cpp logging 189; SEG::Square 553; GRID_GEOMETRY::Area 48; ELLIPSE::GetPointAtAngle 4) | 19,979 / 1,559 (util.cpp logging 954; others unchanged) |
| plus 633 additive `pow-round-*` requests (22,171) | 19,102 / 3,069 | 20,470 / 1,701 |

All 1,642 pow deferrals are removed: 877 Round requests now return results,
and 765 continue to the next real call, KiROUND's overflow log
(`kimathLogOverflow`, owned by `libs.kimath.src.math.util.application.ui.p001`),
for scaled values that are infinite, NaN or outside int (for example digits
400, or 1e300 with any digits). Net reduction of deferrals: **877** on the
original corpus. The added requests (own seed `0x506F77`, no original line
changed) cross exact/inexact powers of ten, KiROUND ties, int edges, 0/0 NaN
results and the 10^d underflow/overflow edges; 491 succeed and 142 reach the
same logging deferral. The complete corpus agrees between native Rust replies
and import-free wasm release. Exact C++ comparison of these results is the
hosted geometry gate.

## Remaining objections

- The geometry oracle runs against the hosted runner's system glibc, while
  pow is qualified against the locked 2.39-0ubuntu8.12 package. None of the
  149 files of the pinned Debian patch archive (SHA-256 `cc7cf38a...10f6`)
  touches the pinned pow sources (inspected), but a runner glibc update is a
  separate identity the geometry job does not pin.
- Only the SSE2 / round-to-nearest / gradual-underflow value contract is
  qualified; FMA/FMA4 IFUNC variants, errno, exception flags and other
  rounding modes are outside it.
- The unit stays partial: kimathLogOverflow, SEG::Square, GRID_GEOMETRY::Area
  and ELLIPSE::GetPointAtAngle still defer to their owners. No golden is created.
