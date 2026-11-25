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
in wasm, debug and release. The full six-configuration runtime qualification passed on
`8d5db2ee2965c480211cf0b58c4518b4201632c1`; see the executed evidence below.
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
| atan | tiny identity; signed polynomial; table reduction; reciprocal table/polynomial; saturated +/-pi/2; quiet NaN | Every table bin and reciprocal boundary, all exponents and signed specials | None in the pinned value contract |
| atan2 | Ordered NaN priority; signed zero/axes; all infinity quadrants; +/-57 exponent-gap shortcuts; both scaling paths; exact quotient correction; four separate quadrant continuations | Ordered payload Cartesian product, table and exponent edges, correlated/full-domain seeded pairs; public vector and Arctan2 wrappers | Qualified for the pinned mode; errno/flags and non-default floating environment are outside value contract |
| tan | tiny identity; polynomial/table; reducers <=25, <=1e8, branred; EADD and non-FMA MUL12/DIV2 cotangent continuation | All table boundaries, range edges and neighbouring pi/2 multiples from small to huge arguments | None in the pinned value contract |
| asin/acos | tiny and polynomial branches; all five table regions; table-seeded near-one square-root refinement; endpoints and invalid/NaN branches | Every table bin, attainable near-one root boundaries, endpoints and domain refusal values | None in the pinned value contract |
| Round | Both pow calls, multiplication, KiROUND, final division stay in source order | Added 32 requests; all 1,642 expanded-corpus Round requests remain typed deferrals | Owned libc::pow backend; separately owned KiROUND |

The 18,050 original request lines are preserved verbatim. Local expanded-corpus
reply counts are 17,721 successful and 3,433 deferred: pow 1,642; KiROUND 626;
equals 560; SEG::Square 553; GRID_GEOMETRY::Area 48; ELLIPSE::GetPointAtAngle 4.
The original 2,216 scalar deferrals removed consist of 928 atan2 and 322 each of
atan/tan/asin/acos. These are local execution counts until the C++ gate passes.
The complete new corpus also agrees with newly executed macOS arm64 native
replies in import-free wasm release, Node 24.16.0 / V8 13.6.233.17-node.49.

## Public inverse-wrapper correction

Fresh locked-loader run [45605514610](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45605514610)
on `afb4cbe9b7eaac8a128a7f7010fa48f7c899cfd8` measured zero mismatches for atan,
atan2 and tan across all 1,517,520 inputs on native debug, WASI debug and
import-free unknown-unknown debug. Asin and acos each had 497,021 mismatches:
the port returned negative indefinite for ordered `|x|>1`, while the **public**
glibc wrappers returned positive `NAN`. Valid-domain values had no mismatch.
The profiles stopped on failure, so release was not measured by that run.

This is diagnosed from `w_asin_compat.c` / `w_acos_compat.c`,
`k_standard.c` cases 2/1 and the default `_LIB_VERSION == _POSIX_` in
`s_lib_version.c`. The public wrappers bypass the IEEE kernel for these domain
inputs; matching only e_asin.c's last branch is insufficient. The implementation
now returns that positive constant on the same ordered domain path, keeping
quieted NaN payloads unchanged. Source hashes and the Sun permission notice are
recorded; the probe additionally refuses unless the actual versioned
`_LIB_VERSION` symbol at `libm+0xe8008` equals 2. No input, expected record or
comparison criterion changed. A fresh complete six-configuration rerun is required.

## Executed qualification

[Hosted run 45606399707](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45606399707)
passed on head `8d5db2ee2965c480211cf0b58c4518b4201632c1`, through merge revision
`8e4d84c58bd46bbbce2c2fe8b4d1bfaef322d21f` on base
`37f4bc0a32f0dc874236f95058803b096c457a96`. The merge and head trees both equal
`15cb6b24612bdc099f7834cce91ce8ee498417e7`. Ubuntu 24.04 x86-64, GCC 13.3.0,
Rust 1.99.1, Node 24.16.0. The fresh probe confirmed the exact locked
loader/libc/libm, all five dispatch targets, the three wrapper GOT targets,
`_LIB_VERSION == _POSIX_` (2), MXCSR control 0x1f80 and x87 control 0x037f.

| Function | Native debug/release | WASI debug/release | Import-free wasm32-unknown-unknown debug/release |
| --- | --- | --- | --- |
| atan | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches |
| atan2 | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches |
| tan | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches |
| asin | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches |
| acos | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches | 1,517,520 each; 0 mismatches |

The unchanged domain SHA-256 is
`4c9c2f84ce8174a35ba9c6fba3bfdb3d713576923670896783280edda2e0d7f4`
and genuine reference SHA-256 is
`4e4c22b8bf31caa70e3ce07f2f7dca631dd76c48563d43188a3be3ccc1ddaf02`.
The exact report, identity, source checks, tools and comparison logs are retained
in `port/glibc-libm/validation/2026-11-25-8d5db2ee29/`; they are never read by
the implementation. The reference bytes remain in the hosted artifact, hashed
by the retained report. No golden is created.

On macOS 26.5 arm64 / Rust 1.96.1 / Node 24.16.0, the corrected numerical bodies
also matched the same genuine records for all five functions natively in debug
and release and in import-free wasm debug and release. The retained local debug
log predates unused-import cleanup, which did not change the numerical bodies.
The accepted sin/cos/sincos fresh-reference workflow separately passed at the
same head ([run 45606399641](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45606399641)).

Local checks passed: 59 non-ignored math/geometry tests (3 fresh-reference tests
are ignored without their dedicated harness), fmt, clippy with warnings denied,
portable wasm check, 6 qualification harness tests, source coefficient verification,
deterministic corpus regeneration and unit-record/source accounting validation.
All builds on this Mac used one target directory and at most -j3.

[Hosted geometry run 45606399762](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45606399762)
passed at the same head and merge revision: **21,154 cases, 17,721 compared,
3,433 deferred, 0 failures, 0 unprocessed, 0 run faults, 0 extra replies**.
All seven undefined-domain controls and six lexical priority controls passed.
The complete import-free wasm release corpus matched newly executed native
Rust replies on Node 22.23.5 / V8 12.4.254.21-node.57. Compressed complete
comparison report, provenance, summary and runtime evidence are retained under
`rust/kicad-geometry-shim/validation/2026-11-25-8d5db2ee29/`.

The original corpus subset now has 14,649 compared / 3,401 deferred,
versus the historical merged 12,433 / 5,617: **2,216 deferrals removed**
(928 atan2 and 322 each atan/tan/asin/acos). The added requests give 3,072
compared and 32 pow deferrals. No original case changed. The registered-main
and final-head gates are reported in the draft PR's Evidence/Tests sections.

Remaining objections: pow is unported, and the unit remains partial because it
and the enumerated KiCad dependencies still defer. Qualification covers the
pinned glibc SSE2/POSIX/default-floating-environment value contract; it does
not qualify other IFUNC variants, libm compatibility modes, hooks, errno or
exception flags. No separate reviewer has approved the draft.
