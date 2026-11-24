<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# MATRIX3x3::SetRotation depends on the platform libm

Unit: `libs.kimath.include.math.application.pure.p001`.
Location: `libs/kimath/include/math/matrix3x3.h:271-279` at pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, `MATRIX3x3<T>::SetRotation`, which calls
`cos( aAngle )` and `sin( aAngle )`.

## Trigger

The C++ result is whatever the C library's `cos`/`sin` return. Those functions are not
correctly rounded and differ between implementations in the last bit for some arguments.
The port (`kicad_math::matrix3x3::Matrix3x3::set_rotation`) calls `f64::cos`/`f64::sin`. A
native build links the host C library, the same library as the oracle on the same host, so
the native Rust shim reproduces the oracle's bits by construction there. A wasm build
uses a different, musl-derived libm and does not.

## Evidence

Local probe on Apple silicon (macOS 26, arm64), with the 3,485-case corpus of this unit at the
implementation commit:

- Rust native shim versus the same shim built for `wasm32-wasip1` and run under Node 24's
  WASI: 3,461 cases identical, 24 different. All 24 are `set_rotation` cases (of 275), and
  each differs only in the cosine/sine members. Every other operation (constructors,
  setters/getters, determinant, inverse, transpose, products, scalar products, equality,
  stream text, `WordsToDouble`, both `ToDouble` branches, `CrossWide`, `DotWide`) is
  identical in wasm and native.
- `wasm32-wasip1` uses wasi-libc's libm; `wasm32-unknown-unknown` (the portable CI target)
  uses the Rust `libm` port in compiler-builtins. Both are musl/FreeBSD-msun lineage. The
  probe establishes that wasm and native results differ. It does not measure the
  `wasm32-unknown-unknown` result directly, which has no runner in this repository.

On glibc x86-64 the reference itself can depend on the CPU: glibc's `sin`/`cos` are ifunc
selected (an FMA build on FMA-capable processors). The oracle and the native Rust shim run in
the same process environment and select the same variant. A golden recorded on one machine
can therefore only be assumed reproducible on machines whose glibc picks the same variant.
The validation record for this unit states which environment produced the golden.

## Status and next step

Unconfirmed as a KiCad defect. It is not one: KiCad depends on the platform libm by design.
It is a portability gap of this port. The native difftest is unaffected, but the portable
rule that the same input gives the same native and wasm result does not hold for
`SetRotation`.

No tolerance or result normalization is applied. The concrete next step is a port-wide
decision (it affects every kimath unit that calls `sin`, `cos`, `atan2`, `hypot`, `exp`
or `log`). One option is a pure-Rust libm that reproduces glibc 2.39's double-precision
routines bit for bit, choosing one ifunc variant and pinning the oracle to it. The other is
a stated plan gate that defines transcendental results per host. Until that decision, this
unit stays `partial` even if every native difftest case passes.
