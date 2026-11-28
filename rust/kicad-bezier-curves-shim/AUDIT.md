<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# bezier_curves.cpp port audit

Unit `libs.kimath.src.bezier_curves.application.ui.p001` (`libs/kimath/src/bezier_curves.cpp`,
pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`), ported as `kicad_hash::bezier_curves` on the
header unit's `BezierPoly` and `Bezier` types (`libs.kimath.include.application.pure.p001`,
`kicad_hash::bezier`). The oracle adapter is `port/oracle/shims/kimath_src_bezier_curves.cpp`,
which calls the genuine archive bodies (private members through explicit-instantiation member
pointers); this crate mirrors its validation order and messages. Status `partial`.

## Per-symbol audit

| Upstream symbol | Rust | Body and branches implemented | Successful operation checked | Remaining gap |
| --- | --- | --- | --- | --- |
| `BEZIER_POLY( VECTOR2I ×4 )` | `BezierPoly::from_points` | four exact `VECTOR2D` conversions, `m_minSegLen = 0.0` | `construct-points4-*` (shapes, int limits); every `points4` request | none |
| `BEZIER_POLY( std::vector<VECTOR2I> )` | `BezierPoly::from_vector2i` | every point converted in order (any count) | `construct-vector-i-*` (0 to 8 points); every `vector_i` request | none |
| `GetPoly( std::vector<VECTOR2I>&, int = 10 )` | `get_poly_i` | `clear()`, the `double` overload into a buffer, `KiROUND` x then y per point | `get-poly-i-*` (478): shapes x errors (default, 1, 5, 10, 25, 100, 0, -3) x forms, 160 random curves, prior output cleared (4032 points), sizes 0/1/2/5/6 | `KiROUND` overflow defers at `kimathLogOverflow` (math util; not reached by the corpus); the assertion host |
| `GetPoly( std::vector<VECTOR2D>&, double = 10.0 )` | `get_poly_d` | error `<= 0` -> 10 (NaN kept), three points `getQuadPoly`, four `getCubicPoly`, otherwise `wxASSERT` | `get-poly-d-*` (382): shapes x errors (default, 0.5 .. 50, -1, 0, -0, inf), 160 random fractional curves, NaN/infinite control points, long prior output kept, fine outputs | the assertion host |
| `isNaN` | `is_nan` | any component NaN | `is-nan-*`, every cubic path | none |
| `isFlat` | `is_flat` | three points: projection with `std::max`/`std::min` clamps, `EuclideanNorm` (45-degree, axis and `hypot` branches); four points: both squared distances, 3/4 or 4/9 factor; else `wxASSERT`, `false` | `is-flat-quad-*` (clamps before/after/mid, axis, diagonal, degenerate, NaN, inf, every error kind), `is-flat-cubic-*`, NaN curves at infinite error | three-point NaN parameter refused ([alg.min.max]) |
| `eval` | `eval` | three and four points in the out-of-line compiled order; else `wxASSERT`, `( 0, 0 )` | `eval-*` (212): sizes 0-5, every single and paired NaN slot with NaN, finite and infinite `t`, random | the assertion host |
| `subdivide` | `subdivide` | three points (middle point from `eval`) and four points in the compiled order; else `wxASSERT` | `subdivide-*` (166): NaN slots and pairs, NaN `t`, outputs longer than needed; short outputs refused | the assertion host |
| `numberOfInflectionPoints` | `number_of_inflection_points` | 1 / 0 (two ways) / -1 | shapes, 80 random (some with 5 points), NaN; under four points refused | none |
| `findInflectionPoints` | `find_inflection_points` | both outputs zeroed first; two roots (swap, 1e-5 merge), one of either, none (negative discriminant, `a == 0`) | shapes (S-curve: two; one-inflection: one), 80 random, NaN, garbage initial outputs | none |
| `thirdControlPointDeviation` | `third_control_point_deviation` | `1e-6` threshold, then the compiled order | `third-control-point-deviation-*` (80): threshold below/at/above, NaN slots and pairs, inf, extra points, random | none |
| `recursiveSegmentation` | private | zero-length drop, flat push, halving with a `std::vector` stack | every cubic `GetPoly` (closed curve, cusp, collinear, point, wide) | loops beyond the budget refused |
| `cubicParabolicApprox` | private | NaN stop, flat stop, `d == 0` and `t > 1` fall-backs, split at `2 sqrt( e / 3d )`, the four-curve ping-pong stack | every cubic `GetPoly` | as above |
| `getCubicPoly` | private | zero inflections short-circuit; two (second not found pushes the end point); one; unknown | `s-curve`, `one-inflection`, `loop`, `cusp`, random | none |
| `getQuadPoly`, `approx_int`, `approx_inv_int` | private | Levien's parabola flattening; `n` from `ceil`; `n` points of `eval` between the end points | every three-point `GetPoly` | `n` of a NaN / beyond int refused |
| `TransformEllipseToBeziers<double>`, `<int>` | `transform_ellipse_to_beziers` | arc normalisation (`>= 0` -> `- 360`), curve count, `k`, the unit segment, the per-point rotate/scale/rotate/offset, the int casting constructor | `ellipse-*` (454): sweeps (full, partial, reversed, zero, signed zeros, subnormal, huge start, multi-turn), rotations, 120 random, prior output kept, int extremes (saturation), double NaN/inf centres, radii and rotations | count of a NaN / beyond int refused; more than 4096 curves beyond the budget |

`wxLogTrace( BEZIER_DBG, ... )` has no observable effect (no trace mask is enabled) and is not
modelled.

## Compiled operand order

IEEE 754 leaves open which of two NaN operands an operation returns; SSE2 returns the destination.
The order of every site where two distinct NaNs can meet in an observable value was read from the
GCC 13.3 (`-O3 -ffp-contract=off`) reference and is written `op(dst, src)` in the Rust, with the
instruction offsets (`port/findings/bezier-curves-port.md`):

- out-of-line `eval`: three points `P0 * omt2` (point the destination), `2.0 * omt` as `omt + omt`,
  coefficient times `P1`, `P2 * t2`; four points `omt3 * P0` and `a * P1` with the `P1` term the
  destination of the first sum, `P2 * b`, `P3 * t3`;
- out-of-line `subdivide`: each `( b - a ) * t + a` with the difference and then the product the
  destination, except `shared = t * ( rc1 - lc2 ) + lc2`;
- `thirdControlPointDeviation`: `dy * dy + dx * dx` and `s * p2.y + r * p2.x` (both swapped from
  the source), then `+ u`;
- `TransformEllipseToBeziers<double>`: `Center + point` (Center the destination), `radius * coordinate`,
  `k * sin` as `sin * k`, `cos + k sin` as `k sin + cos`.

The bodies `GetPoly` inlines need none: with finite control points every NaN of the three-point
path is the default NaN (a non-finite control point makes `n` undefined), and the cubic path only
outputs copied control points and points that passed `isFlat`, which no NaN does. They reuse the
out-of-line order. `check_sites.py` compares the four bound functions' instructions in the oracle
binary with `reference-sites.txt` and fails on any drift. `-ffp-contract=off` holds: no fused or
reassociated operation appears; GCC folds `d * d * d * d`, `1 - d`, `p * p`, `2 * M_PI / 4` and
`theta / 4` (as `theta * 0.25`), each exactly as the Rust evaluates it, and combines `std::sin` and
`std::cos` of `theta` into one `sincos` call.

## Domain, budgets and controls

Undefined (both programs refuse, "undefined bezier operation"): the `int` conversions of
`getQuadPoly`'s `n` and of the curve count from a NaN or out-of-range `ceil` ([conv.fpint]);
private subscripts outside the vectors (GCC 13.3's libstdc++ has no subscript assertion here, so
the adapter checks bounds explicitly); three-point `isFlat`'s `std::max`/`std::min` of a NaN;
`ELLIPSE<int>`'s NaN point conversion (guarded, but unreachable: `RotatePoint` folds a NaN angle
and every other value is finite).

Harness bounds ("outside the verified domain"): `GetPoly`'s loop iterations (getQuadPoly's `n`
and one per iteration of the two loops) beyond 2^20, and more than 4096 ellipse curves. A NaN on
the recursiveSegmentation stack subdivides without end (its stack grows until allocation fails);
the C++ has no bound. The adapter's guard repeats the pinned control flow and counts; the Rust
counts in the same places (`Steps`); the generator classifies exactly.

Controls (138): 102 undefined, 15 beyond the budgets, 21 malformed; all refused identically.

## Evidence

- `cargo test -p kicad-hash` (assertion continuations with a returning host, production deferrals,
  subscript refusals, compiled NaN orders of `eval`/`subdivide`/`thirdControlPointDeviation` and
  the ellipse sum, the exact step limit, oracle replies) and `-p kicad-bezier-curves-shim` (every
  public case succeeds or defers only at `wxOnAssert`, controls, defaults, malformed requests,
  budgets equal the adapter's); fmt, clippy `-D warnings`, wasm32 check of `kicad-math` and
  `kicad-hash`, `cargo +1.85 check --workspace --all-targets`.
- Local diagnostic run, extracted Ubuntu GCC 13.3 in WSL (glibc 2.43 host, hosted
  `GLIBC_TUNABLES`): 2180 cases, 2142 compared equal, 38 deferred (`wxOnAssert`), 0 failures; 138
  controls refused identically; every case and control replayed identically in import-free wasm;
  `check_sites.py` matches; `generate_cases.py --check` identical on Windows (Python 3.12) and
  Linux (3.14).
- The hosted `rust-bezier-curves-live` run (GCC 13.3, glibc 2.39) is the reference.
