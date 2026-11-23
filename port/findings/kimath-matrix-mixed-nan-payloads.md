<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MATRIX3x3 arithmetic on two different NaNs does not match the oracle

Unit: `libs.kimath.include.math.application.pure.p001`. Affected: `Determinant`, `Inverse`,
`operator*( M, M )`, `operator*( M, VECTOR2 )` and `operator*( M, VECTOR3 )` for
`MATRIX3x3<double>`, when an input contains two or more NaNs with different bit patterns.
These are valid, defined inputs; nothing about them is undefined behaviour. The adapters
accept them, and they must not be rejected, normalized or excluded.

## Evidence

- Independent review (Mac arm64 smoke oracle: genuine adapter, Apple clang; Rust 1.99.1
  debug): 1,472 of 3,500 fixed-seed requests differ. The minimal case is
  `minimal-qnan-determinant`: C++ returns `fff8000000000002` and Rust `7ff8000000000001`.
- Qualified WSL2 Linux x86-64 run of exact head `8dd2dc0b` (Ryzen 9 5950X, glibc 2.43, GCC
  15.2, audited `-O3 -fno-fast-math -ffp-contract=off`, Rust 1.99.1 debug): the original 3,485
  cases match, and `minimal-qnan-determinant` agrees on Linux (`7ff8000000000001` on both
  sides). The 3,501-request set from `gen_mixed_nan.py` (sha256 `6b334d54…`, requests
  `85fdf99b…`) differs on 1,824 of 3,500 requests:

  | Operation | Differences (of 500) |
  | --- | ---: |
  | determinant | 265 |
  | inverse | 486 |
  | mul_matrix | 498 |
  | mul_vector2 | 405 |
  | mul_vector3 | 170 |
  | mul_scalar, scalar_mul | 0 |

  Examples (oracle, then Rust): `mixed-nan-determinant-1` `7ff8000000000001` /
  `fff8000000000003`; `mixed-nan-determinant-2` `7ff8000000000002` / `7ff8000000000001`. The
  glibc CPU variant does not change these results. Neither side returns errors. Full
  requests, replies and differences are in the WSL evidence
  (`C:\Users\KarlVonRichter\.t3\pr50-qual-8dd2dc0b\evidence`, manifest `e6909307…`,
  `mixed-nan/differences.json` `dd4a6ce8…`).
- The hosted runner builds the oracle with GCC 13.3 / glibc 2.39. Its results for this set
  are unknown.

## Cause (inference, not verified against disassembly)

IEEE 754 leaves unspecified which payload survives when both operands of one operation are
NaN. On x86-64 SSE the result is the first source operand's NaN. Compilers may swap the
operands of commutative `+` and `*` during register allocation, and vectorize at `-O3`, so
the surviving payload depends on the compiler, its version and the optimization level. It
does not follow from the KiCad source. The minimal case agreeing on Linux GCC 15 but not
on Mac clang is consistent with this. Scalar products agree because each element meets at
most one NaN.

## Retained regressions

`port/cases/<unit>.jsonl` keeps the exact qualified 3,501 requests (case IDs
`minimal-qnan-determinant` and `mixed-nan-<op>-<n>`). `generate_cases.py` regenerates them
and refuses to write the corpus unless their bytes hash to `85fdf99b…`. Until this is
resolved, the live difftest fails on them and no golden can be blessed.

## Decision required (not taken here)

Matching the reference exactly needs a reference-environment decision:

1. **Pin the oracle compiler** (exact GCC version and flags in the oracle contract). Then
   measure, on the pinned binary, which operand each NaN-sensitive operation propagates
   (pairwise distinct-payload probes determine every node of each expression tree). Port
   that into explicit, portable NaN selection: if a chosen operand is NaN return it
   quieted, and map invalid operations to `fff8000000000000`. This is deterministic on
   native and wasm, but it binds the port to one compiler's register allocation and must
   be re-measured on any oracle toolchain change.
2. **A stated plan gate for NaN payloads** that defines which NaN bits are compared. This is
   a comparison policy change for the parent/user, not for a unit owner.

The current Rust port relies on hardware and LLVM operand order, so it also differs between
native targets and wasm (where the payload of a NaN result is nondeterministic). Either
option above replaces that.
