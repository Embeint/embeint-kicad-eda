<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Audit: libs.kimath.src.transform.application.ui.p001

Upstream `libs/kimath/src/transform.cpp` (127 lines) at KiCad pin
4cf939778df04d2b747bb2c4714fc5014a7565c7. Rust: `kicad_hash::transform_source`
(`rust/kicad-hash/src/transform_source.rs`), on the canonical accepted types.

## Ownership and reuse

| C++ symbol (USR) | Rust | Owner status |
| --- | --- | --- |
| `TRANSFORM::operator==` (`c:@S@TRANSFORM@F@operator==#&1$@S@TRANSFORM#1`) | accepted co-port `impl PartialEq for Transform` in `rust/kicad-hash/src/transform.rs` (PR #51); not duplicated | this unit owns it; body unchanged, now checked against the oracle here |
| `TRANSFORM::TransformCoordinate( const VECTOR2I& )` | `Transform::transform_coordinate` | this unit |
| `TRANSFORM::TransformCoordinate( const BOX2I& )` | `Transform::transform_box` | this unit |
| `TRANSFORM::InverseTransform()` | `Transform::inverse_transform` | this unit |
| `TRANSFORM::MapAngles( EDA_ANGLE*, EDA_ANGLE* )` | `Transform::map_angles` | this unit |
| `std::hash<TRANSFORM>::operator()` | `impl StdHash for Transform` | this unit |
| `TRANSFORM` layout, constructors, `operator!=` | `kicad_hash::transform::Transform` | `libs.kimath.include.application.pure.p001` (partial), reused |
| `hash_combine`, `std::hash<int>` | `kicad_hash::hash::{hash_combine, StdHash}` | same header unit, reused |
| `VECTOR2I`, `VECTOR2D`, `VECTOR2L` | `kicad_math::vector` | `libs.kimath.include.math.application.ui.p002` (partial), reused |
| `BOX2I` default, `SetOrigin`, `SetSize`, `GetOrigin`, `GetEnd`, `SetEnd`, `GetSize`, `IsValid` | `kicad_math::box2::Box2I` | `libs.kimath.include.math.application.ui.p001` (partial), reused |
| `EDA_ANGLE( double, DEGREES_T )`, `EDA_ANGLE( VECTOR2D )`, `Cos`, `Sin`, `Invert`, `Normalize`, `operator-`, `<`, `>`, `ANGLE_0`, `ANGLE_360` | `kicad_math::eda_angle` | `libs.kimath.include.geometry.application.ui.p001` (partial), reused (glibc 2.39 SSE2 sin/cos/atan2 ports) |

No new type or helper duplicates an accepted one; no header function is co-ported; no owner
status changes. No `UNPORTED` tag exists: every callee is available.

`TRANSFORM DefaultTransform` (a mutable global, reassigned by `eeschema/lib_symbol.cpp`) is
not modelled as global state. `DEFAULT_TRANSFORM_INITIAL` is only its initial value; a host
that needs the current default owns it and passes the transform explicitly.

## Body audit

- `operator==`: `x1`, `y1`, `x2`, `y2` compared in order with `&&` short-circuit; integer
  comparison has no observable evaluation-order effect. Cases cover every subset of differing
  members on six bases (including `INT_MIN`/`INT_MAX`) and random pairs.
- `TransformCoordinate( VECTOR2I )`: four `int` products and two sums. Signed overflow is
  undefined; `checked_mul`/`checked_add` refuse exactly those inputs
  (`TransformError::IntegerOverflow`). The unspecified evaluation order of the products
  cannot change a defined result.
- `TransformCoordinate( BOX2I )`: default box; `SetOrigin( T( GetOrigin() ) )`; then
  `GetEnd()` (int origin + int64 size, narrowed by Linux's generic `KiCheckedCast<long, int>`
  modulo 2^32, which is defined) and `SetEnd( T( end ) )` (`SizeVec( pos ) - m_Pos` in int64,
  never overflowing). Only int64 overflow of `GetEnd`'s sum and int overflow of either point
  transform are refused. The result keeps negative sizes (no normalization) and is
  initialized. Wrapping ends are exercised (106 public cases).
- `InverseTransform`: `det = x1*y2 - x2*y1` (products and difference checked), then
  `y2/det`, `(-x2)/det`, `(-y1)/det`, `x1/det` in source order with truncation toward zero.
  Refused: zero determinant (`DivisionByZero`; the source comment's "never null" is not
  enforced), negation of `INT_MIN`, `INT_MIN / -1`, overflowing products or difference
  (`IntegerOverflow`). The constructor argument order `TRANSFORM( invx1, invy1, invx2, invy2 )`
  is kept. Truncating (non-unimodular) and negative determinants are exercised.
- `MapAngles`: `delta`; cos/sin, `VECTOR2D( x*x1 + y*y1, x*x2 + y*y2 )` and `EDA_ANGLE( v )` for
  each angle, assigned in place in source order; residual normalized; swap test against
  `epsilon.Invert().Normalize()` (359.9, which makes the swap unconditional:
  `port/upstream-issues/transform-mapangles-always-swaps.md`); then the ordering branches
  (`a2 < 0`: normalize `a2`; else `a1 = a1.Normalize() - 360`, normalizing `a1` first).
  The wxCHECK null-pointer branch is unrepresentable with Rust references and not exercised.
  Cos/sin are always finite, so the products and sums never produce NaN and plain IEEE
  arithmetic matches the `-ffp-contract=off` reference. No undefined domain: NaN and infinite
  angles are defined IEEE inputs and are in the public corpus.
- `std::hash<TRANSFORM>`: `std::hash<int>( x1 )` (sign-extended to 64-bit `size_t`), then
  `hash_combine( seed, y1, x2, y2 )` left to right with wrapping arithmetic.

## Evidence

| Operation | Implemented body/branches | Successful observable checked | Remaining gap |
| --- | --- | --- | --- |
| `operator==` | all four member comparisons, equal and each first-differing member | 160 exact boolean results | none |
| `std::hash<TRANSFORM>` | seed and three combines | 200 exact `size_t` values (decimal strings) | none |
| `TransformCoordinate( VECTOR2I )` | all products/sums; overflow refusal | 1,493 exact points; 1,307 overflow controls | none |
| `TransformCoordinate( BOX2I )` | origin, end narrowing (fits and wraps), SetEnd; overflow refusal | 396 exact boxes (origin, int64 size, valid); 264 overflow controls | none |
| `InverseTransform` | determinant, four quotients, truncation, both determinant signs; refusals | 221 exact matrices; 12 division-by-zero and 113 overflow controls | none |
| `MapAngles` | both mappings, residual, swap, all three ordering outcomes | 5,208 exact (swap, a1 bits, a2 bits); oracle: swap true 5,208; no reorder 3,509, a2 normalized 1,051, a1 - 360 648 | null-pointer wxCHECK branch unrepresentable |

Local run (WSL Ubuntu, GCC, glibc 2.39 with
`GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4,-AVX512F`, the qualified SSE2 runtime):
`kicad-difftest compare --partial`: 7,678 cases, 7,678 compared, 0 deferred, 0 failures, 0
unprocessed; `compare_controls.py`: 1,696 controls refused identically; import-free wasm
replay of all 9,374 requests equals native Rust. Without the tunables, glibc selects its FMA
variants and one MapAngles case (`map-random-206`) differs by one ulp in the oracle, as for
every other unit using the SSE2 trig ports; the comparison is only valid on the qualified
runtime, which CI enforces.

Status stays `partial`: no golden is blessed and the separate review and required CI on the
final head are pending.
