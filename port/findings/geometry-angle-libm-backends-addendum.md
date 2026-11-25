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

## Owned branch audit

| Operation | Body and continuation | Evidence requested | Remaining gap |
| --- | --- | --- | --- |
| atan | tiny identity; signed polynomial; table reduction; reciprocal table/polynomial; saturated +/-pi/2; quiet NaN | Every table bin and reciprocal boundary, all exponents and signed specials | Fresh runtime gate pending |
| atan2 | Ordered NaN priority; signed zero/axes; all infinity quadrants; +/-57 exponent-gap shortcuts; both scaling paths; exact quotient correction; four separate quadrant continuations | Ordered payload Cartesian product, table and exponent edges, correlated/full-domain seeded pairs; public vector and Arctan2 wrappers | Fresh runtime gate pending; errno/flags and non-default floating environment are outside value contract |
| tan | tiny identity; polynomial/table; reducers <=25, <=1e8, branred; EADD and non-FMA MUL12/DIV2 cotangent continuation | All table boundaries, range edges and neighbouring pi/2 multiples from small to huge arguments | Fresh runtime gate pending |
| asin/acos | tiny and polynomial branches; all five table regions; table-seeded near-one square-root refinement; endpoints and invalid/NaN branches | Every table bin, attainable near-one root boundaries, endpoints and domain refusal values | Fresh runtime gate pending |
| Round | Both pow calls, multiplication, KiROUND, final division stay in source order | Added 32 requests; all 1,642 expanded-corpus Round requests remain typed deferrals | Owned libc::pow backend; separately owned KiROUND |

The 18,050 original request lines are preserved verbatim. Local expanded-corpus
reply counts are 17,721 successful and 3,433 deferred: pow 1,642; KiROUND 626;
equals 560; SEG::Square 553; GRID_GEOMETRY::Area 48; ELLIPSE::GetPointAtAngle 4.
The original 2,216 scalar deferrals removed consist of 928 atan2 and 322 each of
atan/tan/asin/acos. These are local execution counts until the C++ gate passes.
The complete new corpus also agrees with newly executed macOS arm64 native
replies in import-free wasm release, Node 24.16.0 / V8 13.6.233.17-node.49.
