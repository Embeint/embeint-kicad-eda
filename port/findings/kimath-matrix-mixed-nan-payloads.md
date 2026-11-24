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

## Cause (verified against the pinned oracle's disassembly)

IEEE 754 leaves unspecified which payload survives when both operands of one operation are
NaN. x86-64 SSE2 `addsd/subsd/mulsd/divsd src, dst` returns `dst` quieted if it is a NaN,
else `src` quieted, else `fff8000000000000` for an invalid operation (Intel SDM vol. 1,
table 4-7). The compiler chooses which operand of each commutative `+` and `*` becomes `dst`
during register allocation, so the surviving payload is a property of one compiled binary,
not of the KiCad source. In the pinned oracle (sha256 `0bac7567…`, build ID `c8e72118…`, GCC
13.3, `-O3 -fno-fast-math -ffp-contract=off`) the seven `MATRIX3x3<double>` sites use mixed
orders, and `Inverse` evaluates its own inline determinant that shares cofactors with the
adjugate. Scalar products only ever meet one NaN per multiplication.

## Resolution (option 1 below)

`kicad_math` evaluates `Determinant`, `Inverse`, `operator*( M, M )`, `operator*( M, VECTOR2 )`,
`operator*( M, VECTOR3 )` and both `double` scalar products through private generated code,
`rust/kicad-math/src/generated.rs` (sha256 `d24ff0f2…`), on explicit SSE2 NaN selection,
`rust/kicad-math/src/sse2.rs`. Each operation is one call `op(dst, src)` in the pinned binary's
operand order. Non-NaN results are the C++ expression's: the same operations on the same
operands with the same rounding, proven equal up to commutativity of `+` and `*` only. The `int`
scalar products convert exactly and call the `double` product; order cannot matter there.
Integer `MATRIX3x3<int>` paths, `SetRotation` (the separate glibc `sincos` port), streaming
and the wide-integer helpers are unchanged. No input is excluded, normalized or looked up.

Evidence (static; Cargo, wasm and the live difftest run in hosted CI):

- `port/tools/kimath_nan/check_generated.py` checks that `generated.rs` is exactly the reviewed
  operation graphs, the commutativity-only finite proof against `matrix3x3.h`, the 136
  observable-node witnesses and the bounded equivalence of the seven remaining nodes (negative
  controls: a swapped NaN priority, a swapped subtraction, a false equivalence claim and an
  altered witness bit each fail it).
- A transliteration of the shipped `generated.rs` and dispatch reproduces all 117,175 retained
  pinned-oracle replies (the qualified mixed-NaN probes for all nine public operations,
  including `int` scalars) and fails thousands of them when one node's order or the reciprocal
  constant is changed or the default NaN is `7ff8…`.
- `rust/kicad-math/tests/nan_priority.rs` replays the 136 witnesses through every public entry
  point and operator; `sse2.rs` replays 6,916 SSE2 instruction results measured on x86-64; the
  shim test asserts every bit of the singular inverse.

The binding is to one compiled oracle. `port/tools/kimath_nan/README.md` separates the stable
graph checker above, which runs on every change, from machine eligibility. Machine
eligibility is the full generator gate in `port/tools/kimath_nan/gate/`, run by the manual
`frozen-reference` job of `rust-kimath-nan.yml` on a fresh build of `8ef0c0e`. It has **not
run**. The live comparison counts only for an oracle that passed it, and its hard blockers
are listed there. Until then, rust-port's difftest against its runner-built oracle is
evidence, not acceptance, and no golden is blessed. A toolchain or flag change requires
re-deriving `generated.rs` with the gate and emitter, never editing it. Other units that
inline `MATRIX3x3<double>` arithmetic (and hash or Bezier code that will reuse `sse2`)
need their own measured sites; these priorities do not carry over.

## Retained regressions

`port/cases/<unit>.jsonl` keeps the exact qualified 3,501 requests (case IDs
`minimal-qnan-determinant` and `mixed-nan-<op>-<n>`). `generate_cases.py` regenerates them
and refuses to write the corpus unless their bytes hash to `85fdf99b…`. The examples above
are from the GCC 15.2 qualification run, not the pinned GCC 13.3 oracle; the hosted run of
the pinned oracle is the reference.

## Decision (recorded; the comparison contract is unchanged)

Of the two options below, option 1 is implemented. Option 2 was not taken: every NaN bit
remains compared.

1. **Pin the oracle compiler** (exact GCC version and flags in the oracle contract). Then
   measure, on the pinned binary, which operand each NaN-sensitive operation propagates
   (pairwise distinct-payload probes determine every node of each expression tree). Port
   that into explicit, portable NaN selection: if a chosen operand is NaN return it
   quieted, and map invalid operations to `fff8000000000000`. This is deterministic on
   native and wasm, but it binds the port to one compiler's register allocation and must
   be re-measured on any oracle toolchain change.
2. **A stated plan gate for NaN payloads** that defines which NaN bits are compared. This is
   a comparison policy change for the parent/user, not for a unit owner.
