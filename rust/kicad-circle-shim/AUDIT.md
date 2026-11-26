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
| general branch | `CalcArcMid`, `libs.kimath.src.trigo.application.ui.p001` | behind the above |
| `Intersect( CIRCLE )`, both solutions | `RotatePoint( VECTOR2I&, const EDA_ANGLE& )`, `libs.kimath.include.application.ui.p001` | every case with intersection points defers |
| `Resize` / `KiROUND` overflow | `kimathLogOverflow`, `libs.kimath.src.math.util.application.ui.p001` | radius `INT_MIN` axis cases |
| double `Resize` with two distinct NaN components | vector unit's distinct-NaN arithmetic site, `libs.kimath.include.math.application.ui.p002` | 56 double cases |

The unported calls go through one `Callees` seam whose only production implementation is
`Pinned`. In-crate tests drive the continuations with test-only callees (the parallel decision
as declared input, the seg.h one-line bodies of `Center` and `IntersectLines`, and only the exact
0/90/180/270 degree branches of `RotatePoint`): the tan-tan parallel branch to completion, the
general branch up to `CalcArcMid` (including the `aP == intersectPoint` return), and both
`Intersect( CIRCLE )` solutions, tangency and translation for cardinal rotations. These are not
oracle evidence.

## Per-symbol and branch audit

| Upstream symbol | Branches, defaults, order | Oracle-checked successful results | Remaining gap |
| --- | --- | --- | --- |
| `CIRCLE()` | `Center = {0,0}`, then `Radius = 0` | `default`: state | none |
| `CIRCLE( const VECTOR2I&, int )` | memberwise | `new`: 99 states incl. int limits, negative radii | none |
| `CIRCLE( const CIRCLE& )` | `Center`, then `Radius`; `operator==` after copy | `copy`: 99 states + equality | none |
| defaulted `operator==` (co-port) | `Radius`, `Center` | `equals`: 16, both operand orders | none |
| `ConstructFromTanTanPt` | parallel / general; `aP == intersectPoint` early return; furthest/closest lambdas (`>` / `<=`, ties to the second / first); A-vs-B tangent choice (`>`); 7 `wxCHECK_MSG` sites returning `*this`; returns `*this` | 107 cases deferred at `SEG::Square`; oracle replies retained (all `returns_self`, 39 `wxCHECK_MSG` failures printed by the oracle and returned) | entire body behind header p003 and trigo; adapter accepts only a stated sufficient domain (below) |
| `Contains( const VECTOR2I& ) const` | int64 norm; inclusive band `Radius +/- MIN_PRECISION_IU` | `contains` const: values over the band edges, centre, int limits, negative radii | none |
| `Contains( const VECTOR2I& )` (non-const) | `( aP - Center )` in `int` (undefined on overflow), int norm `< Radius` | `contains` non_const values; 29 overflow controls | none |
| `NearestPoint( const VECTOR2I& )` | int sub; centre -> `vec.x = 1`; `Resize( Radius )`; int sum | values for every Resize branch, radius sign, limits | radius `INT_MIN` axis: 3 math-util deferrals |
| `NearestPoint( const VECTOR2D& )` | double - int; signed-zero centre test; `Resize( double( Radius ) )`; double + int | exact bits incl. single NaN payloads, infinities, subnormals, huge values | 28 distinct-NaN vector deferrals |
| `FurthestPoint( const VECTOR2I& )` | `Center - aP`; as above | as NearestPoint | 3 math-util deferrals |
| `FurthestPoint( const VECTOR2D& )` | int - double; as above | as NearestPoint | 28 distinct-NaN vector deferrals |
| `Intersect( const CIRCLE& )` | apart/nested return; co-centred return; int64 eq. (3) with truncating division; remainder `< 0` return; `KiROUND( sqrt )`; modular `VECTOR2I( int64, int64 )`; rotation; `+= Center`; second solution only for `y != 0` | 236 empty results (apart, nested, co-centred and negative radii); no generated negative-remainder cases | 193 RotatePoint deferrals; 4 eq. (3) overflow controls; 4 post-rotation overflow controls (oracle refuses, Rust defers first) |
| `Intersect( const SEG& )` | `IntersectLine`, then `SEG::Contains( p, 3 )` per point, in order | 571 values | 12 controls |
| `IntersectLine( const SEG& )` | miss (`> R+4`); tangent band (inclusive both ends) pushes `m`; chord with `int( sqrt )`; `( B - A ).Resize`; `-mTo1vec`; two int sums; degenerate line gives `m` twice | 571 values incl. band sweeps -7..7 on three orientations | 12 controls |

Undefined operations are refused identically by both programs, never hidden from valid
inputs: `int` overflow of a vector sum or difference, `std::abs( INT_MIN )` in `Resize`, int64
overflow in equation (3), and the SEG guards reached by `LineProject` and `Contains`.

## ConstructFromTanTanPt adapter domain

The oracle adapter does not guard `ConstructFromTanTanPt` operation by operation. It accepts
only non-degenerate lines and a point within 2^16, where the pinned `ApproxParallel` is true or
the lines' angle has sine at least 1/8. The review found intermediates exceeding the previously
claimed 2^24 bound; complete safety of this restricted input set is not yet established.
Requests outside the set are refused as a harness limit, not as a claim about defined C++.
The Rust shim mirrors the bound and degeneracy checks; the angle check reaches the genuine
SEG::Square deferral. A precise follow-up safety audit remains required.

## Evidence

Local diagnostic run only (WSL, GCC 15.2 / glibc 2.43, merged-tree-equivalent implementation
f6c05a433e): 4,978 public cases, 4,616 exact, 362 deferred, 0 failures; 81 controls agree.
The hosted `rust-circle-live` workflow (ubuntu-24.04: GCC 13 / glibc 2.39 / wx 3.2.4) is the
reference run and must pass on the final head. No golden is proposed.

## Partial review corrections and unresolved findings

The first independent review stopped on a provider content flag and did not
produce a completed verdict. It independently reproduced the hosted hashes and
controls but identified missing assertion effects in tan-tan continuations.
Returning the partially mutated circle reproduces only the scalar return:
`wxCHECK_MSG` also invokes the C++ assertion handler. The portable continuation
does not represent that host effect. This remains a source-contract finding;
partial status and the current SEG::Square deferral do not resolve it.

The test-only SEG::Center transcription now calls the accepted rounded vector
division, with positive/negative odd-half checks. It is still not production or
oracle evidence. The equal-radius generator search emitted zero cases, so the
previous twelve-case remainder coverage claim is removed without changing the
public corpus. See `port/findings/circle-review-assertion-and-coverage-gaps.md`
for evidence and remaining work.
