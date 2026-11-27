# CIRCLE source audit

Unit `libs.kimath.src.geometry.circle.application.ui.p001` (`libs/kimath/src/geometry/circle.cpp`,
380 lines, pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`). Implementation:
`rust/kicad-math/src/circle.rs`. Oracle adapter: `port/oracle/shims/kimath_src_circle.cpp`.

## Reused backends and co-ports

No second geometry model is introduced. The bodies call the accepted `Seg` source port
(`LineProject`, `LineDistance`, `ParallelSeg`, `PerpendicularSeg`, `Contains`, `ApproxParallel`),
`Vector2` (`try_sub`/`try_add`/`try_neg`, `EuclideanNorm` through `RoundedHypot`,
`SquaredEuclideanNorm`, `Resize` for `int` and `double`), `EdaAngle::from_vector` (atan2 through
the qualified glibc backend), `KiROUND` (`util::round_i32`) and the reference NaN helpers (`fp`).

Minimum co-ports, ownership unchanged and recorded in the owners' notes:

| Symbol | Owner | Rust |
| --- | --- | --- |
| `CIRCLE::Radius`, `CIRCLE::Center`, defaulted `operator==` | `libs.kimath.include.geometry.application.pure.p001` | `Circle` fields in declaration order, `#[derive(PartialEq, Eq)]` |
| `SHAPE::MIN_PRECISION_IU` (= 4) | `libs.kimath.include.geometry.application.ui.p003` | `circle::MIN_PRECISION_IU` |

## Deferred calls (typed `Unported`, continuation implemented)

| Call site | Callee and owner | Effect |
| --- | --- | --- |
| `ConstructFromTanTanPt` first statement | `SEG::ApproxParallel` (accepted) -> `SEG::Square`, `libs.kimath.include.geometry.application.ui.p003` | every tan-tan case defers |
| parallel branch | `SEG::Center`, header p003 | behind the above |
| general branch (4 sites) | `SEG::IntersectLines`, header p003 | behind the above |
| general branch, inside `CalcArcMid` (trigo.cpp, ported as `kicad_math::trigo::calc_arc_mid`) | `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` (trigo.h), `libs.kimath.include.application.ui.p001` | behind the above |
| `Intersect( CIRCLE )`, both solutions | `RotatePoint( VECTOR2I&, const EDA_ANGLE& )`, `libs.kimath.include.application.ui.p001` | every case with intersection points defers |
| `Resize` / `KiROUND` overflow | `kimathLogOverflow`, `libs.kimath.src.math.util.application.ui.p001` | radius `INT_MIN` axis cases |
| double `Resize` with two distinct NaN components | vector unit's distinct-NaN arithmetic site, `libs.kimath.include.math.application.ui.p002` | 56 double cases |
| failed `wxCHECK_MSG` (7 sites) | `wxOnAssert` host effect, this unit (no unit ports the wx handler) | behind `SEG::Square`; continuation returns `*this` |

The unported calls go through one `Callees` seam whose only production implementation is
`Pinned`. In-crate tests drive the continuations with test-only callees (the parallel decision
as declared input, the seg.h one-line bodies of `Center` and `IntersectLines`, and only the exact
0/90/180/270 degree branches of `RotatePoint`, optionally with a recording assertion host): the
tan-tan parallel branch to completion and to its failed line 102 check (`tan_tan_pt-hand-2`'s
oracle state, with the host and with the `wxOnAssert` deferral), the failed line 115 check, the
general branch through `CalcArcMid` (the ported trigo.cpp body with its trigo.h wrapper forwarded,
to completion on two requests whose oracle replies are asserted, and the `aP == intersectPoint`
return), and both
`Intersect( CIRCLE )` solutions, tangency and translation for cardinal rotations. These are not
oracle evidence.

## Per-symbol and branch audit

| Upstream symbol | Branches, defaults, order | Oracle-checked successful results | Remaining gap |
| --- | --- | --- | --- |
| `CIRCLE()` | `Center = {0,0}`, then `Radius = 0` | `default`: state | none |
| `CIRCLE( const VECTOR2I&, int )` | memberwise | `new`: 99 states incl. int limits, negative radii | none |
| `CIRCLE( const CIRCLE& )` | `Center`, then `Radius`; `operator==` after copy | `copy`: 99 states + equality | none |
| defaulted `operator==` (co-port) | `Radius`, `Center` | `equals`: 16, both operand orders | none |
| `ConstructFromTanTanPt` | parallel / general; `aP == intersectPoint` early return; furthest/closest lambdas (`>` / `<=`, ties to the second / first); A-vs-B tangent choice (`>`); 7 `wxCHECK_MSG` sites: host assertion effect (`wxOnAssert` deferral without a host), then `*this` with the partial mutation; returns `*this` | 107 cases deferred at `SEG::Square`; oracle replies retained (all `returns_self`, 39 `wxCHECK_MSG` failures printed by the oracle and returned); 3 undefined controls | entire body behind header p003 and trigo; the assertion effect has no host |
| `Contains( const VECTOR2I& ) const` | int64 norm; inclusive band `Radius +/- MIN_PRECISION_IU` | `contains` const: values over the band edges, centre, int limits, negative radii | none |
| `Contains( const VECTOR2I& )` (non-const) | `( aP - Center )` in `int` (undefined on overflow), int norm `< Radius` | `contains` non_const values; 29 overflow controls | none |
| `NearestPoint( const VECTOR2I& )` | int sub; centre -> `vec.x = 1`; `Resize( Radius )`; int sum | values for every Resize branch, radius sign, limits | radius `INT_MIN` axis: 3 math-util deferrals |
| `NearestPoint( const VECTOR2D& )` | double - int; signed-zero centre test; `Resize( double( Radius ) )`; double + int | exact bits incl. single NaN payloads, infinities, subnormals, huge values; `aP` at a zero centre with each signed-zero combination (`*-centre-0-*`, `*-centre-4-*`) | 28 distinct-NaN vector deferrals |
| `FurthestPoint( const VECTOR2I& )` | `Center - aP`; as above | as NearestPoint | 3 math-util deferrals |
| `FurthestPoint( const VECTOR2D& )` | int - double; as above | as NearestPoint | 28 distinct-NaN vector deferrals |
| `Intersect( const CIRCLE& )` | apart/nested return; co-centred return; int64 eq. (3) with truncating division; remainder `< 0` return (unreachable, below); `KiROUND( sqrt )`; modular `VECTOR2I( int64, int64 )`; rotation; `+= Center`; second solution only for `y != 0` | 236 empty results (apart, nested, co-centred and negative radii) | 193 RotatePoint deferrals; 4 eq. (3) overflow controls; 4 post-rotation overflow controls (oracle refuses, Rust defers first) |
| `Intersect( const SEG& )` | `IntersectLine`, then `SEG::Contains( p, 3 )` per point, in order | 571 values | 12 controls |
| `IntersectLine( const SEG& )` | miss (`> R+4`); tangent band (inclusive both ends) pushes `m`; chord with `int( sqrt )`; `( B - A ).Resize`; `-mTo1vec`; two int sums; degenerate line gives `m` twice | 571 values incl. band sweeps -7..7 on three orientations | 12 controls |

Undefined operations are refused identically by both programs, never hidden from valid
inputs: `int` overflow of a vector sum or difference, `std::abs( INT_MIN )` in `Resize`, int64
overflow in equation (3), and the SEG guards reached by `LineProject` and `Contains`.

## Unreachable negative remainder in Intersect( CIRCLE )

With `d > 0`, the two preceding tests `d <= r1 + r2` and `d >= |r1 - r2|` imply `r1, r2 >= 0`
and `|d - r1| <= r2 <= d + r1`. For the exact quotient `x0 = ( d^2 + r1^2 - r2^2 ) / ( 2d )`
these give `( d - r1 )^2 <= r2^2 <= ( d + r1 )^2`, so `|x0| <= r1`; the truncated `x` has
`|x| <= |x0|`, so `r1^2 - x^2 >= 0` whenever equation (3) is defined. The `< 0` return is
implemented as in C++ but cannot be reached; no fixture is invented for it.

## ConstructFromTanTanPt adapter guard

The oracle adapter guards `ConstructFromTanTanPt` operation by operation (`guardTanTanPt`):
every integer operation of the body and of its `SEG` (`ApproxParallel`, `LineProject`, `Center`,
`ParallelSeg`, `IntersectLines`, `LineDistance`, `PerpendicularSeg`), `CalcArcMid` and
`RotatePoint` callees, in evaluation order and following the pinned control flow, with each
dependent value taken from the genuine function after its own guard. It replaces an earlier
sufficient domain (2^16 coordinates, sine at least 1/8) that admitted two overflows found in
review. There is no remaining domain restriction, and the Rust shim no longer mirrors one.
A local UBSan cross-check (diagnostic) found its refusals equal to the genuine sources'
undefined evaluations; see `port/findings/circle-deferrals-and-adapter-domain.md`.

## Evidence

Local diagnostic run (WSL, GCC 15.2 / glibc 2.43) of this head's adapter: all 4,978 public
cases are accepted, with oracle output SHA-256 `d765f242...` identical to the earlier hosted
run; Rust 4,616 exact, 362 deferred, 0 failures (Rust output SHA-256 `88c44791...`, also
unchanged). All 84 controls are refused by the oracle; Rust refuses 77 and defers 7 first
(4 RotatePoint, 3 SEG::Square). The hosted `rust-circle-live` workflow (ubuntu-24.04: GCC 13 /
glibc 2.39 / wx 3.2.4) is the reference run and must pass on the final head. No golden is
proposed.

## Review findings and their resolution

The completed independent review of 48e156721a found an admitted tan-tan overflow, the
missing assertion effects, a test-only midpoint truncation and evidence-accounting errors; a
follow-up sweep found a second admitted overflow. The adapter now guards tan-tan operation by
operation (above); the seven `wxCHECK_MSG` sites call a typed host effect; the test-only
SEG::Center transcription uses the accepted rounded division; the signed-zero centre cases
now carry signed zeros; the remainder branch is recorded as unreachable. See
`port/findings/circle-review-assertion-and-coverage-gaps.md`. The `VECTOR3::Normalize`
omission is recorded for its own owner in `port/findings/vector3-normalize-assertion-effect.md`.

## trigo.cpp caller wiring

`Pinned::calc_arc_mid` now calls the port of `libs.kimath.src.trigo.application.ui.p001`
(`kicad_math::trigo::calc_arc_mid`). That body computes the differences and angles and defers at
its own trigo.h `RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& )` wrapper, which the
header unit `libs.kimath.include.application.ui.p001` owns. The 70 public cases and 2 controls
that deferred at `CalcArcMid` now defer at that wrapper instead (no case changes state; the
circle corpus is unchanged and the two controls' `rust_unported` owner is updated). Local
diagnostic difftest against a GCC 13.3 oracle build: 4,978 cases, 4,636 compared, 342 deferred,
0 failures; all 84 controls as recorded.
