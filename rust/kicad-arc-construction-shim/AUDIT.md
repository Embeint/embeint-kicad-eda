<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Arc construction port audit

Unit `libs.kimath.src.geometry.arc_construction.application.ui.p001` owns all eight
definitions of `libs/kimath/src/geometry/arc_construction.cpp` (207 lines) at KiCad pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Rust: `kicad_math::arc_construction`
(`rust/kicad-math/src/arc_construction.rs`). Oracle adapter:
`port/oracle/shims/kimath_src_arc_construction.cpp`, which calls the genuine pinned bodies
compiled in the oracle's own `kimath` target.

## Source audit and evidence

| Upstream symbol | Implemented body and branches | Successful operation and observable result checked | Remaining gap |
| --- | --- | --- | --- |
| `crossZ` (anon) | `p = aY*bX`, `e = fma(aY,bX,-p)`, `fma(aX,bY,-p) - e`, explicit `std::fma` as `f64::mul_add` | through every collinear, near-collinear, apex-rounding and tangent-on-line case, observed through `valid`/`mid` | none |
| `toIU` (anon) | `!isfinite` / `abs > INT_MAX - 1` refusal, then `KiROUND` x then y (accepted `util::round_i32`; always in the non-logging range) | `project-limit-*` at INT_MAX, INT_MAX-1, INT_MAX-2 and INT_MIN corners; refusals keep the cursor / default mid | none |
| `buildOnChord` (anon) | start/end set first; `len <= 0`, non-finite or non-positive radius refusals; shallow `half^2/(r+offset)` vs deep `r+offset` height; toIU refusal (mid stays default); apex-on-chord refusal (mid kept); center, radius, valid | every field of every solution, including partially filled refusals (`build-apex-on-chord` 79, `build-toiu-fail` 287, `build-bad-radius` 1) | `len <= 0` is unreachable from every caller (each checks the chord first); ported, not exercised |
| `ProjectToChordBisector` | zero chord returns cursor; projection via `Dot`; toIU refusal returns cursor | 364 project cases + 436 mid-drag projections | none |
| `ArcThroughPoints` | `d == 0` default solution (start/end zero); circumcenter, `centerT`, side from sign of `d` | 488 cases, both orientations, collinear/coincident/shallow/1 nm sagitta/limits | none |
| `ArcFromStartEndMidDrag` | projection, through, `mid = projected` only when valid | 436 cases, on-chord and apex-rounding refusals | none |
| `ArcFromStartEndCenterDrag` | zero chord default; `centerT` sign (`< 0` only; zero and -0 give +1); `!aMajor` flips; direct `std::hypot( len/2, centerT )` | 634 cases, both postures, centers on the chord, at the midpoint, far along the bisector | none |
| `ArcFromStartTangentEnd` | tangent norm `<= 0` / non-finite default; `dd <= 0` or `cross == 0` default; `k`, normal, `len = sqrt(dd)`, chord normal, side from `cross` | 990 cases: QA sweep (432), random magnitudes 1e-6..1e6, subnormal, overflowing, signed-zero, NaN-on-axis and signalling-NaN tangents | 20 tangents with an infinite or NaN operand in the general norm branch defer (below) |
| `ARC_SOLUTION` (header) | layout and defaults (`valid = false`, `radius = 0.0`, zero vectors) as `ArcSolution` | every reply reports all six fields | minimum co-port recorded in owner `libs.kimath.include.geometry.application.pure.p001`, which remains todo |

Branch labels come from `generate_cases.py --branches` (a Python mirror that never writes a
result). Every reachable label is required by the generator.

## Dependencies

- `VECTOR2<double>` `EuclideanNorm`, `SquaredEuclideanNorm`, `Dot`, `operator+`,
  `operator*( VECTOR2, scalar )`: canonical `kicad_math::vector` bodies of
  `libs.kimath.include.math.application.ui.p002` (partial).
- `KiROUND`: accepted `util::round_i32` of `libs.kimath.include.math.application.ui.p001`.
- `std::hypot` (direct call) and the finite general branch of `VECTOR2<double>::EuclideanNorm`:
  the accepted shared glibc 2.39 finite-input backend `kicad_math::hypot::finite` (introduced by
  the util/box unit for `GetDiameter`). `std::sqrt` and `std::fma` are correctly rounded, so the
  IEEE result is unique on every target.
- `ArcChordParams` is not used: `arc_construction.cpp` does not call it.

### Co-port in the vector owner (status unchanged)

`VECTOR2<double>::EuclideanNorm` (`c:@ST>1#T@VECTOR2@F@EuclideanNorm#1`, vector2d.h:281-307),
`impl VectorComponent for f64 { fn norm2 }` in `rust/kicad-math/src/vector.rs`: the general
`std::hypot( x, y )` branch now returns `hypot::finite(x, y)` when both operands are finite. Infinite
or NaN operands still return `Unported` with the vector owner's tag
(`UNPORTED(libs.kimath.include.math.application.ui.p002)`), since glibc's special-value path and
its NaN selection are not ported. This is recorded in that unit's notes. Caller reruns against
the local pinned oracle:

| Unit | Cases | Before (accepted) | After |
| --- | ---: | --- | --- |
| vector `ui.p002` | 10,286 | 9,639 exact / 647 deferred | 9,723 exact / 563 deferred / 0 failures |
| util/box `ui.p001` | 15,931 | 14,324 / 1,607 | 14,329 / 1,602 / 0 failures |
| arc chord, eda_angle, math util | 1,611 / 4,704 / 10,656 | all exact | unchanged, all exact |

Open topic #81 (ellipse) records 36 `EuclideanNorm std::hypot` deferrals against this vector
body; once both merge, the finite ones compare instead and its asserted counts need the parent's
integration.

## Deferrals (20)

`tangent-special-{inf-general,nan-general,nan-both,inf-nan}-{0..4}`: `ArcFromStartTangentEnd`'s
first call, `aTangent.EuclideanNorm()`, reaches `std::hypot` with a non-finite operand and returns
`Unported { upstream: "VECTOR2<double>::EuclideanNorm std::hypot", unit:
"libs.kimath.include.math.application.ui.p002" }`. The continuation (the `tangentLen` tests and
the rest of the body) is implemented and runs for every other tangent, including non-finite
tangents that take the diagonal or axis branches.

## Domain and controls

No valid input leaves the defined C++ domain: points are `int`, every `KiROUND` is guarded by
`toIU`, the remaining arithmetic is IEEE double, and `hypot`, `fma` and `sqrt` are defined for
every operand. There are therefore no undefined-domain controls. The 20 harness controls in
`port/oracle/fixtures/arc-construction-controls.jsonl` (malformed requests) are refused
identically by both programs before any pinned call, checked by `compare_controls.py`; they never
enter the public corpus. NaN payloads never reach an output: every NaN tangent path ends in a
default solution or a deferral, and the `inf * 0` centre offset of an overflowing `k` is discarded
by the radius check.

## Evidence

Local, WSL Ubuntu 26.04 (glibc 2.43, GCC 15; not the hosted ubuntu-24.04 / glibc 2.39
reference, so these runs are not qualification of the glibc 2.39 hypot backend), own work root `~/kicad-port-arcconstruction`, oracle built by
`port/oracle/build.sh` (-j8, shared lock), `GLIBC_TUNABLES` as in hosted CI, oracle sha256
`f49977363148d66e74e981881cd52dc2701fcb3bc2683034702d9ac5955944b7`:

- `kicad-difftest compare --partial`: 2,912 cases, 2,892 exact, 20 deferred, 0 failures,
  0 unprocessed, no run faults or extra replies (cases sha256 `65c257e0...`).
- `compare_controls.py`: 20 harness controls identical.
- import-free wasm replay of every case and control equal to the native replies.
- The geometry-header and kicad-math corpora each report one stream-formatting mismatch locally
  on both this topic and accepted `origin/rust` 4738a9c23f, from the local toolchain rather than
  this change; hosted CI is the authority for them.

No golden is proposed. The unit stays `partial` (deferrals, and pending hosted live comparison
and independent review).
