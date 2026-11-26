<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Addendum: glibc 2.39 hypot for VECTOR2<double>::EuclideanNorm

Owner: `libs.kimath.include.math.application.ui.p002` (the existing owned
`std::hypot` deferral in `vector.rs`). Base: `4738a9c23f8c6068784d26ba991a690044343892`
(`origin/rust`). Additive to `vector-unique-nan-arithmetic.md` and to the glibc
angle/pow addenda (`geometry-angle-libm-portability.md`,
`geometry-angle-libm-backends-addendum.md`, `geometry-angle-libm-pow-addendum.md`),
which stay unchanged. No new unit or extractor unit is created; glibc remains a
qualification reference, not a ported KiCad unit.

## What changed

`vector2d.h:304-306` ends the floating `EuclideanNorm` with
`static_cast<T>( std::hypot( x, y ) )` after the 45-degree and axis shortcuts.
`norm2` for f64 now returns `crate::hypot::hypot(x, y)` instead of
`Unported`. `Distance` reaches the same call for every extended type. The
`UNPORTED(libs.kimath.include.math.application.ui.p002): std::hypot` tag is removed.

`hypot.rs` already held the accepted shared finite body (`hypot::finite`, glibc
2.39 e_hypot.c after its first branch, used by `BOX2::GetDiameter`). It is not
duplicated or changed. This change adds `hypot::hypot`, the complete public
`__hypot`:

- `!isfinite(x) || !isfinite(y)`: if either operand is infinite and neither is
  signalling (`issignaling_inline`, the `2 * (ix ^ 0x0008...)` test), return +inf;
  otherwise `return x + y`. The locked libm computes it as `addsd %xmm1,%xmm0`
  (libm+0x28486; x is the first operand). A NaN x is returned quieted, else the
  quieted NaN y. This is written out because wasm NaN propagation is unspecified.
- finite operands continue into `hypot::finite` unchanged.

Arc construction PR #85 (open, not imported) separately wires finite operands to
`hypot::finite` at the same `norm2` line. This change is a superset: finite
operands reach exactly that body, so the overlap is mechanical and the parent
resolves it through `origin/rust`. PR #81 (ellipse, unaccepted) has 36 hypot
deferrals; it was not imported or edited. Rerun it after integration.

## Binary binding and source

- Locked libm (SHA-256 `5dcc4d25...994a`, debug ID `f772fc0c...0525`):
  `hypot@@GLIBC_2.35` = `__hypot` = `__ieee754_hypot` = `__hypot_finite@GLIBC_2.15`
  at libm+0x28450 (963 bytes, plain FUNC). The SVID compat `hypot@GLIBC_2.2.5`
  (`__hypot_compat`, libm+0x12db0) is a separate body. No relocation or IFUNC
  names hypot, and 2.39 has no x86_64 multiarch hypot, so the cpu tunables do not
  select it. The probe checks the default/versioned binding, the compat address
  and the `__hypot_finite` alias before writing records.
- The disassembly has no FMA: it is the non-FMA kernel in source order with the
  ternary ax/ay selection (generic `USE_FMAX_BUILTIN`/`USE_FMIN_BUILTIN` 0). Its
  constants are bit-identical to the macros: abs mask, DBL_MAX, +inf, 0x1p+511,
  0x1p-54, 0x1p-600, 0x1p+600, 0x1p-459, 0x1p+54, 4.0 and DBL_MIN at
  libm+0x8fa80..0x99028. `ay / EPS` compiles to `ay * 0x1p+54` and `/ SCALE` to
  `* 0x1p+600`; these are exact power-of-two scalings, so they give the same bits.
- Source: e_hypot.c `5bb30147...514e`, dbl-64 w_hypot.c (`/* Not needed.  */`)
  `dc041a65...cdc9fa`, math/w_hypot_compat.c `bda9448d...7f6b` and both
  math-use-builtins headers, all pinned in `source.json` from the hash-checked
  `glibc_2.39.orig.tar.xz`. `check_sources.py` checks the four exponents
  against `hypot.rs`, the builtin selection, the empty wrapper and the GLIBC_2_35
  version. A wrong TINY or LARGE exponent is refused (checked). None of the
  pinned Debian patch archive's files touches these sources (same inspected
  patch set as pow).
- The vector C++ oracle imports `hypot` from `GLIBC_2.35` (`objdump -T`: `DF *UND*
  (GLIBC_2.35) hypot`). It is a real runtime libm call, not inlined or folded.

## Owned branch audit

| e_hypot.c path | Ported | Genuine evidence | Gap |
| --- | --- | --- | --- |
| inf with non-signalling other (incl. qNaN) -> +inf | `hypot` | specials product, seeded non-finite mode | none in value contract |
| inf with signalling NaN, any NaN with finite: `x + y` (x NaN first) | `nan_add` | 4 qNaN x 3 sNaN payloads, both orders and signs | none |
| ax > 0x1p+511: shortcut `ax + ay`, scaled kernel, overflow to inf | accepted `finite` | threshold +-3 ulps, DBL_MAX, the DBL_MAX/sqrt2 edge | errno only |
| ay < 0x1p-459: shortcut, scaled-up kernel, subnormal results | accepted `finite` | threshold +-3 ulps, subnormal pairs, triples to 2^-1074 | underflow flag only |
| common: `ay <= ax * EPS` shortcut, kernel | accepted `finite` | shortcut at every exponent, all exponents x relative offsets | none |
| kernel `h <= 2 * ay` split | accepted `finite` | ay/ax near float(1/sqrt(3)) at many scales | none |
| `handle_errno`, `math_check_force_underflow_nonneg` | values only | n/a | errno and flags are outside the value contract |

## Executed qualification (local)

Domain `hypot_domain.py`: 607,712 fixed + 1,000,000 seeded (own seed) =
1,607,712 pairs, SHA-256 `a4b02776b8f98e27a26700c415616914301fd4563cd1a67465893a4327f4f2a8`.
The hash is identical on Windows Python 3.12.0 and WSL Python 3.14.4. No host
libm is used; `decimal` supplies 1/sqrt(3) and 1/sqrt(2). Genuine records came
from the locked libc6 2.39-0ubuntu8.12 package through its own loader with a
cleared environment and the unchanged tunables. The probe confirmed loader/libc/libm
identity, `_LIB_VERSION`, all existing angle/pow dispatch checks and the new hypot
bindings. Reference SHA-256
`2972d4008f433f2e052824625af0136c63abaa9cbb10ac854f40ab97d8aaa3cd`.

| Configuration | hypot records | Mismatches |
| --- | --- | --- |
| native x86_64-pc-windows-msvc debug / release (Rust 1.99) | 1,607,712 each | 0 / 0 |
| native x86_64-unknown-linux-gnu debug / release (WSL, Rust 1.99.1) | 1,607,712 each | 0 / 0 |
| import-free wasm32-unknown-unknown debug / release (Node 22.22.1) | 1,607,712 each | 0 / 0 |

A mutant that reversed the NaN operand order was caught at once (multi-NaN
payload pairs), so the records do discriminate. WASI debug/release and the full
seven-function, six-configuration report run in `rust-glibc-libm`. Those hosted
results are the gate and are reported in the PR, not here.

## Corpus effect

Local Rust replies, base `4738a9c23f` shim vs this branch on the same inputs,
all nine kicad-math corpora:

| Corpus | Base results / deferred | Branch results / deferred |
| --- | --- | --- |
| vector, accepted 10,286 | 9,639 / 647 (hypot 176) | 9,815 / 471 |
| vector, +1,922 additive `hypot-*` (seed `0x48797074`) | 28 / 1,894 | 1,922 / 0 |
| util/box (`include.math.application.ui.p001`) 15,931 | 14,324 / 1,607 (hypot 5) | 14,329 / 1,602 |
| geometry, hash, kimath math, arc chord, eda_angle, math util, clipper2 | unchanged | unchanged |

All 2,070 vector and 5 util/box hypot deferrals now return results. No case
moved to a different deferral (no newly exposed `Unported`), and no previously
successful reply changed in any corpus. Remaining vector deferrals: 428
arithmetic NaN-site and operator deferrals, 40 `kimathLogOverflow`, 3 double
stream formatting. The util/box shim's pinned count test changes from
[14324, 1607] to [14329, 1602] for exactly these 5 cases.

Exact C++ comparison against an own-built oracle (`port/oracle/build.sh`, own
work root, build lock, 8 jobs; oracle SHA-256 `64ab4e14...a2e7`), local Ubuntu
glibc 2.43 / GCC 15.2, not the reference environment:

| Unit | Cases | Compared exactly | Deferred | Failures |
| --- | --- | --- | --- | --- |
| vector | 12,208 | 11,737 | 471 | 0 |
| util/box | 15,931 | 14,329 | 1,602 | 0 |
| geometry | 22,171 | 20,464 | 1,701 | 6 |

The six geometry failures are `std::fixed` stream casing of inf/nan (`INF` vs
`inf`, `stream-{6,7,8}-False-fixed-{0,-1}`) from the local libstdc++. The
geometry Rust replies are byte-identical on base and branch, and PR #85 reports
the same local-only mismatch on accepted `origin/rust`. All 8,860
originally accepted vector comparisons and 251 previously deferred cases now
compare, with accepted inputs unchanged. The hosted `rust-util-box-live` job
(ubuntu-24.04, glibc 2.39) and its `check_caller_regressions.py` are the gate.

## Remaining objections

- The oracle jobs use the hosted runner's system glibc 2.39 package, while hypot
  is qualified against the locked 2.39-0ubuntu8.12 package; a runner glibc
  update is a separate identity those jobs do not pin (as for pow).
- Only the value contract under round-to-nearest with gradual underflow is
  qualified. errno (ERANGE from `handle_errno`), overflow/underflow/invalid
  flags and other rounding modes are not modelled, and none of these is claimed.
- The unit stays partial (arithmetic NaN sites, double stream formatting,
  util.cpp logging). No golden is created before parent review.
