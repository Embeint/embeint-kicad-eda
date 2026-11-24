<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash: BEZIER<double> results where two distinct NaN payloads meet

Unit `libs.kimath.include.application.pure.p001`, PR #51. Reference: `BEZIER<double>::PointAt`,
`Split` and `SubCurve` (`libs/kimath/include/bezier_curves.h:110-191`) with the free `VECTOR2`
operators (`libs/kimath/include/math/vector2d.h:461-545`) at pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Status: **implemented as per-site operand order
bound to the candidate reference build; static and offline checks pass; the hosted live run is
pending**. The unit stays `partial` and no golden is blessed until that run agrees.

## Trigger

A `double` addition, subtraction, multiplication or division whose two operands are NaNs with
different sign or payload, for example `aT * ( C1 - Start )` with a NaN `aT` and a NaN control
point, or `( t_m1_3 * Start ) + ( 3.0 * aT * t_m1_2 * C1 )` with NaNs in `Start` and `C1`.

IEEE 754 leaves open which input NaN the result carries. C++ adds nothing. On x86-64 an SSE2
instruction `op dst, src` returns `dst` quieted when it is a NaN, otherwise `src` quieted (Intel
SDM vol. 1, table 4-7, SSE column), so the result depends on which C++ operand the compiler
allocates to the destination register. For `-` and `/` the encoding forces the left operand;
for the commutative `+` and `*` it is a register-allocation choice. That is a property of the
generated code, not of the KiCad source.

## History

The first port kept the left operand everywhere. 38 corpus cases failed on the WSL GCC 15.2
oracle and, identically, on the GCC 13.3 candidate oracle (all 41 unit failures had the same
IDs; the two oracles' corpus outputs are byte-identical, sha256 `1ef5a58c...`):
`bezier-f64-nan-curve-0-split-0` .. `-7`, `bezier-f64-nan-curve-0-sub-curve-0`, `-1`, `-2`, `-4`;
`bezier-f64-nan-curve-1-point-at-0` .. `-3`, `bezier-f64-nan-curve-1-split-0` .. `-7`,
`bezier-f64-nan-curve-1-sub-curve-0` .. `-4`; `bezier-f64-random-007`, `-023`, `-031`, `-059`,
`-071`, `-091`, `-095`, `-111`, `-119`. Fitting one choice per operation kind or per source site
still left 22 and 11 failures (`rust/kicad-hash-shim/validation/2026-11-23-9bf572d4/nan-fit.*`): the order differs
between statements and between the inlined and out-of-line copies of the same member.

## Derivation from the genuine candidate oracle

Bound build: `kicad-oracle` sha256 `5e0a8c3620d69af0b7239be7852d8e26e0bc92e372e82e444613ec9dc3c6610f`,
built from this PR's `a5ceb98984d29ca9420527ce75fa481abd9c3727` in the PR #55 candidate image
(`sha256:8b70857a...a3597`, GCC 13.3.0-6ubuntu2~24.04.1, `-O3 -fno-fast-math -ffp-contract=off`,
SSE2 baseline). Its adapter object `CMakeFiles/kicad-oracle.dir/shims/kimath_header_hash.cpp.o`
(sha256 `a6b793f510022586dc4b9f9469f2ff9a63f98de51ec33669ec8467bbd6670012`) holds every site:

| Site | Where | Operand order (dst first) |
| --- | --- | --- |
| adapter `point_at` | `PointAt` inlined in `KimathHeaderHash` +0x24bf5..+0x24c74 (packed) | coefficient products: coefficient is dst; first sum: the `C1` term is dst (right operand); second and third sums: the running sum; `t3 * End`: `t3` |
| `SubCurve`, empty or reversed range | out-of-line `BEZIER<double>::PointAt` (packed) | as above, except `t3 * End`: `End` is dst, and `( 3.0 * t2 ) * t_m1`: `t_m1` |
| every `Split` (adapter `split` and both `SubCurve` splits) | out-of-line `BEZIER<double>::Split` (packed) | per `a + aT * ( b - a )`: `b - a`; the difference is dst of the product, except the last statement (`shared`), where `aT` is dst; the product is dst of the sum |
| `SubCurve` second split parameter | `divsd` in `KimathHeaderHash` +0x25077 | `t0 / t1` (forced) |

`rust/kicad-hash/src/bezier_f64_sites.rs` writes each instruction as `op(dst, src)` and cites
its offset; `rust/kicad-hash/src/sse2.rs` (temporary, to be replaced by the canonical
`kicad-math` scalar helpers) applies the SSE2 rules with the semantics and argument order of
the reviewed PR50 matrix prototype. The expression trees are the C++ ones; only operands of
`+` and `*` are swapped, which no non-NaN result can show, so finite arithmetic order is
unchanged. `BEZIER<int>` keeps the generic bodies: every NaN there is a quieted `aT` or the
default NaN and any NaN is refused when stored, so its integer conversion and undefined
behaviour handling are untouched.

Choices that can never be observed are recorded for fidelity only: in `PointAt`, products with
`Start`, `C2` and `End` feed sums whose dst is already a NaN whenever the coefficient is (all
coefficients are NaN exactly when `aT` is), and `SubCurve` reaches its degenerate `PointAt`
only with a clamped, non-NaN `t0`. The GCC 15.2 review oracle (`34572378...`) inlines the
degenerate `PointAt` as scalar code whose product orders differ per component, but only in
these unobservable products; its observable choices equal the candidate's, which is why the
two oracles agree on every retained reply.

## Evidence (static and offline; no live run)

- An independent bit-level Python model of the derived order reproduces all 600 `BEZIER<double>`
  calls of the candidate's retained corpus replies; the old left-operand model reproduces exactly
  the 38 failures. Flipping any single observable choice is detected by retained genuine replies
  (1 to 25 corpus cases each); the unobservable ones are detected by none, as the dataflow proves.
- The patched Rust shim (native Windows x86-64, debug and release, byte-identical replies)
  reproduces all 2641 retained candidate replies and all 2641 retained GCC 15 replies, the
  reviewer's 10,884 adversarial GCC 15 replies and 3 minimal repros, the 15 + 325 refusal
  fixtures, and passes `check_guards.py` (441). wasm32 in Node reproduces the native digest.
- On the 1,983 supplementary sensitive requests (`generate_sensitive.py`), the Rust replies equal
  the Python model's predictions; every observable choice is asked by 24 to 579 of them. These
  requests have not been run against a live oracle.

## Binding and drift

`rust/kicad-hash-shim/reference-bindings.json` records the normalised arithmetic instruction
sequence of the three site functions (sha256 of the signature included) and the derivation
identities. `check_reference_bindings.py` refuses any oracle whose sites differ, so another
compiler, flag or adapter change fails instead of being compared. The recorded order must then
be re-derived and reviewed, never re-recorded to accept drift. The GCC 15 review binary and an
unrelated adapter object are refused (negative controls).

Next step: the hosted live run in the qualified image (`hosted_validation.sh`), an independent
review, and only then a corrected golden.
