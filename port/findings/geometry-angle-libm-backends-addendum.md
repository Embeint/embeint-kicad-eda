<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Addendum: glibc 2.39 angle libm backends

Owner: `libs.kimath.include.geometry.application.ui.p001`.
Base: `37f4bc0a32f0dc874236f95058803b096c457a96` (`origin/rust`).
Addendum to the immutable `geometry-angle-libm-portability.md`.

The private safe Rust `glibc_libm` module ports the complete glibc 2.39 SSE2
atan, atan2, tan, asin and acos value algorithms, constants and tables. It
preserves every source branch, rounding and continuation. There is no host
libm or explicit FMA. Existing sin/cos/sincos outputs are unchanged. Tan reuses
the accepted branred reducer. No 2.39 multiprecision fallback remains in these
five functions. Source/licence provenance, packaged kernel/wrapper addresses,
actual IFUNC GOT-slot checks and the unchanged CI tunables are documented in
`port/glibc-libm/README.md`. Both the AVX/FMA4 selector used by atan/atan2/tan
and the FMA4 selector used by asin/acos select SSE2 with those tunables.

The new fresh-reference job compares 1,517,520 inputs per function, including
517,520 fixed boundary/special pairs and one million seeded pairs, natively and
in wasm, debug and release. Qualification is pending at this initial commit;
no bit-exact runtime qualification is claimed until hosted execution succeeds.
The existing geometry corpus gains 3,104 additive requests from a separate fixed
seed to exercise general-vector angles, all inverse wrappers, Tan and Round.

Pow remains deferred. Its non-FMA log/exp table implementation, special rounding
in the subnormal/overflow paths, negative-base integer classification and NaN
operand precedence require another complete backend and qualification. This PR
prioritizes the five backends used by general geometry; it does not replace pow
with exponentiation or a power-of-ten lookup. Round still defers at its first
pow call, before KiROUND, retaining both pow calls and the continuation. KiROUND
and equals stubs are untouched, as assigned to the separate util-header worker.
The geometry unit remains partial. No golden is created.
