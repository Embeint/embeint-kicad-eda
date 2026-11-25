# Geometry header port audit

Unit: `libs.kimath.include.geometry.application.ui.p001` (partial).
Reference: KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
Official base: `98cbf0b63eb68541eb5b4ba7d5cc2d03b71d2cf2` on `origin/rust`.
No upstream or open topic branch was merged. No golden is supplied.

The full four pinned headers are the authority, including bodies missing from the
extracted USR list. The implementation is in the existing safe `kicad-math` crate.
The canonical `Vector2<T>` and source-unit `rescale_i32` are reused.

| Operation / upstream symbols | Implemented bodies / branches | Observable evidence | Remaining gap |
| --- | --- | --- | --- |
| EDA_ANGLE constructors, accessors, constants | Default, explicit degrees, radians division, tenths division; vector zero, axes, diagonals and atan2; exact constant bits | Raw-bit construction and vector cases, signed zeros, extremes, NaN/inf | AsTenthsOfADegree calls unported KiROUND |
| Normalize, Normalized, NormalizeNegative/90/180/720, foldToTwoPeriods | Finite/abs threshold/fmod+copysign; each inclusive/exclusive loop and returned mutation; copy form | Six modes at wrap boundaries and adjacent doubles, random exponent sweep, huge/tiny, nonfinite | Degenerate >two-loop cases intentionally absent because source fold bounds them |
| IsZero/Horizontal/Vertical, Invert, abs; all arithmetic/comparison and assignment operators | Exact comparisons without implicit normalization; expression order and units preserved | Unary/binary cases, raw return bits; NaN comparisons | Mixed-NaN selection still needs native qualification |
| Sin/Cos/Tan, Arccos/Arcsin/Arctan/Arctan2 | Every cardinal/diagonal shortcut, original unnormalized radian fallback; scalar inverse wrappers | Shortcuts, ordinary scalar libm and domain/NaN cases | sin/cos reuse accepted portable glibc kernel; other libm calls compile on wasm but bit identity there is unqualified |
| IsParallelTo | Both Normalize90 and -90-to-90 substitutions, final equals | Native reference returns bool; Rust explicitly defers at equals | `libs.kimath.include.math.application.ui.p001`: equals<double> |
| Snapped, Round, AsTenthsOfADegree | Nonpositive-step identity; surrounding multiplication, divisions, both pow calls, result construction | Nonpositive snap succeeds, positive snap/round/tenths reach typed dependency errors | `libs.kimath.include.math.application.ui.p001`: KiROUND<double,int> |
| Reflect and EDA_ORIENTATION (all operators, SetAngle/GetAngle, default, equality) | Reflection arithmetic; normalization; signed-zero canonicalization; angle-left forms and orientation difference | Raw bits of all outputs, mutation and assigned state, equality in each orientation case | Native mixed-NaN qualification pending |
| Angle/orientation ostream operators | Delegate to accepted f64 StreamInsert with caller state; width mutation retained | general/fixed/scientific/hex, precision/width/fill/showpos/uppercase; normalized orientation output | Existing StreamState resource budget is explicit |
| ELLIPSE<T> default and GetArcStart/EndPoint | Generic canonical Vector2 fields; default radii/angles zero; wrappers forward actual start/end angles | i32/f64 defaults observed; C++ calls real endpoint APIs | `libs.kimath.src.geometry.ellipse.application.ui.p001`: GetPointAtAngle. Out-of-line constructors/mirror/evaluation are not owned here |
| GRID_GEOMETRY default members and TakesPrecedenceOver | Cartesian default, zero fields; priority unequal branch; both Area calls and strict comparison continuation | Higher/lower unsigned priorities including UINT_MAX; equal-priority reference executes real Area | `libs.kimath.src.geometry.grid_geometry.application.ui.p001`: Area; Snap/Contains are out of line |
| POLY_YSTRIPES_INDEX default/Build/yToStripe | Clear before early return; sum all vertices; skip short/horizontal rings; outline/hole edges, sqrt/clamp, Y bounds/ceil height, inclusive stripe insertion; Linux signed narrowing | C++ adapter constructs genuine SHAPE_POLY_SET; empty/degenerate/repeated vertices, holes, winding, overlapping outlines, nine outlines, extreme Y, rebuild and fixed-seed rectangles | yRange==0 after nonhorizontal filtering is unreachable; 65536-stripe cap needs >2^32 edges and cannot be reached before signed totalEdges overflow |
| Contains | Empty early return; stack/heap crossing counters; source >= crossing convention; accepted rescale; per-outline parity union; accuracy<=1 bypass; full fallback loops and hole skip after deferred Square | Interior, boundary, holes, union, outside, accuracy defaults/-1/0/1/2/10; genuine successful inside results even with accuracy>1 | `libs.kimath.include.geometry.application.ui.p003`: SEG::Square; then `libs.kimath.src.geometry.seg.application.ui.p001`: SquaredDistance. The latter continuation is implemented but currently unreachable after the Square deferral |

## Contour boundary and safety

`PolygonContours` is a borrowed ordered contour snapshot (canonical integer-nanometre
vectors), not a second polygon kernel or a claim on SHAPE_POLY_SET bodies. The native
adapter materializes actual `SHAPE_POLY_SET` outlines/holes and permits repeated vertices;
no fake geometry-kind stand-in computes oracle answers. A future polygon owner can
supply these views without copying coordinates or changing the index algorithm.

Input-dependent edge/stripe/crossing allocations use try_reserve; integer differences
and counters are checked. No arbitrary coordinate or point-count cap is introduced.
The reference totalEdges int domain is checked before allocation. The query's stripe
narrowing follows the pinned Linux int cast, not saturation. The C++ adapter's int64
subtraction preflight refuses only actual signed-int UB sites, before the native call.
Three separate refusal controls do not enter the successful corpus.

All header bodies after unavailable calls are present; no success substitutes for a
missing dependency. No header declarations implemented by other source units are
claimed here. IsCardinal/IsCardinal90/KeepUpright are out-of-line declarations, owned
by the eda_angle.cpp source unit, and are not silently implemented here.

## Validation

`generate_cases.py --check` regenerates the fixed-seed public corpus (seed 0xEDA4CF939).
The full registered own-library oracle compiles pinned, unmodified source with
`-fno-fast-math -ffp-contract=off`. Hosted `geometry-live` and the main `rust-port`
job compare every record exactly in partial mode and enumerate dependency deferrals.
No lookup, tolerance, exclusions or golden. See the PR for head-specific native counts.

Local Mac validation: cargo fmt, clippy with warnings denied, kicad-math tests,
geometry safety/continuation tests, portable wasm build, unit-record validation.
Only one cargo target directory; builds capped at -j3. No KiCad, Windows or WSL build.
