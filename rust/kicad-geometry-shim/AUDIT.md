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
| EDA_ANGLE constructors, accessors, constants | Default, explicit degrees, radians division, tenths division; vector zero, axes, diagonals and atan2; exact constant bits | Raw-bit construction and vector cases, signed zeros, extremes, NaN/inf | AsTenthsOfADegree calls unported KiROUND; non-axis/diagonal vectors defer at owned atan2 backend |
| Normalize, Normalized, NormalizeNegative/90/180/720, foldToTwoPeriods | Finite/abs threshold/fmod+copysign; each inclusive/exclusive loop and returned mutation; copy form | Six modes at wrap boundaries and adjacent doubles, random exponent sweep, huge/tiny, nonfinite | Degenerate >two-loop cases intentionally absent because source fold bounds them |
| IsZero/Horizontal/Vertical, Invert, abs; all arithmetic/comparison and assignment operators | Exact comparisons without implicit normalization; expression order and units preserved | Unary/binary cases, raw return bits; NaN comparisons | Mixed-NaN outputs checked exactly in the pinned native build |
| Sin/Cos/Tan, Arccos/Arcsin/Arctan/Arctan2 | Every cardinal/diagonal shortcut, original unnormalized radian fallback; scalar inverse wrappers (typed runtime deferral) | Shortcuts, raw sine/cosine bits; inverse/tan/atan2 cases explicitly deferred | sin/cos reuse accepted portable glibc kernel; tan/inverse/atan2 runtime calls explicitly defer to this unit |
| IsParallelTo | Both Normalize90 and -90-to-90 substitutions, final equals | Native reference returns bool; Rust explicitly defers at equals | `libs.kimath.include.math.application.ui.p001`: equals<double> |
| Snapped, Round, AsTenthsOfADegree | Nonpositive-step identity; surrounding multiplication, divisions, both pow calls, result construction | Nonpositive snap succeeds, positive snap/tenths reach typed KiROUND errors; Round reaches the owned typed `libc::pow` deferral first, as C++ evaluates pow before KiROUND | `libs.kimath.include.math.application.ui.p001`: KiROUND<double,int>; this unit: libc pow backend |
| Reflect and EDA_ORIENTATION (all operators, SetAngle/GetAngle, default, equality) | Reflection arithmetic; normalization; signed-zero canonicalization; angle-left forms and orientation difference | Raw bits of all outputs, mutation and assigned state, equality in each orientation case | Native outputs, including NaNs, checked exactly |
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
narrowing follows the pinned Linux int cast, not saturation. The C++ adapter checks individual int32 differences and the executed SEG int64 dot/norm intermediates before the native call.
Seven separate undefined-domain controls do not enter the successful corpus.

All header bodies after unavailable calls are present; no success substitutes for a
missing dependency. No header declarations implemented by other source units are
claimed here. IsCardinal/IsCardinal90/KeepUpright are out-of-line declarations, owned
by the eda_angle.cpp source unit, and are not silently implemented here.

## Validation

`generate_cases.py --check` regenerates the fixed-seed public corpus (seed 0xEDA4CF939).
The full registered own-library oracle compiles pinned, unmodified source with
`-fno-fast-math -ffp-contract=off`. Hosted `geometry-live` and the main `rust-port`
job compare every record exactly in partial mode and enumerate dependency deferrals.
No lookup, tolerance, exclusions or golden. Head-specific evidence is retained under `validation/2026-11-25-d221593095/`.

Local Mac validation: cargo fmt, clippy with warnings denied, kicad-math tests,
geometry safety/continuation tests, portable wasm build, unit-record validation.
Only one cargo target directory; builds capped at -j3. No KiCad, Windows or WSL build.

Portable runtime qualification is enforced, not inferred from build success: the full
corpus runs import-free wasm against newly executed native Rust replies. Scalar
tan/inverse/atan2 backends remain owned runtime deferrals after an observed local
acos mismatch; see port/findings/geometry-angle-libm-portability.md. The corpus has
18,050 defined-domain cases plus seven separate controls. The hosted native comparison checks
12,433 successful results exactly and enumerates 5,617 deferrals, with zero
failures/unprocessed cases. Of the controls, three are actual Rust
overflow refusals and four stop at an earlier SEG::Square dependency; C++ refuses
all seven undefined inputs. These are not successful geometry operations.

## Pinned source identities

| Path | Lines | SHA-256 |
| --- | --- | --- |
| `libs/kimath/include/geometry/eda_angle.h` | 603 | `d1ee4f40c0ad417635dbf57d7677e2c7ade53e689d14cd1c2435d49fbb215e8f` |
| `libs/kimath/include/geometry/ellipse.h` | 110 | `939dda7508d6898e4fc75b39f2a093da9306f3c9d487c59128b165b3accc4c6b` |
| `libs/kimath/include/geometry/grid_geometry.h` | 82 | `f082d18565b270efcb56d82d935ae95f5c2a1cbb0a3c0c6c2a6bdb86bd869f1a` |
| `libs/kimath/include/geometry/poly_ystripes_index.h` | 280 | `92aaa53992b8128bc2226a2478d98a4d1397cf59f44b29260c1c686e7b3ddb9a` |

## Native application-configuration link boundary

Constructing a genuine SHAPE_POLY_SET retains its full vtable, including unrelated
triangulation/fracture entry points that reference ADVANCED_CFG::GetCfg. The small
own-library oracle does not link the application settings/filesystem closure.
`port/oracle/reference/geometry_unreachable_config.cpp` is a link-only fail-closed
guard: GetCfg always throws execution_error, never returns a stand-in config.
The pinned classes, contour accessors, stripe algorithm and segment/math bodies are
unmodified. A successful whole-corpus comparison therefore also proves that no
compared operation required application config. This oracle does not qualify
triangulation/fracture or any future operation that reaches that guard.

## Executed native evidence

[Hosted run 45585616158](https://github.com/Embeint/embeint-kicad-eda/actions/runs/45585616158)
tested head d2215930955ec09c7a27bf7ebe5ff615a59a5e2d via merge revision
1d251be432bd952c6ad1472929b7c9eb7ddff755 on Ubuntu 24.04 x86-64, GCC 13.3.0,
glibc 2.39, wxWidgets 3.2.4. Result: 18,050 cases, **12,433 compared, 5,617 deferred,
0 failures, 0 unprocessed, 0 run faults, 0 extra replies**. The seven separate
undefined-domain controls passed. Complete import-free wasm execution passed
against newly executed native Rust replies, including all dependency errors.

`validation/2026-11-25-d221593095/compare.json.gz` is the complete unmodified
runner report, compressed only for storage. `summary.json` records its raw SHA-256,
executables, case/reply hashes and deferral counts; `oracle-provenance.json` records
compiler flags, source closure and overlay hashes. These are validation artifacts;
the implementation and case generator never read them. No golden was proposed.

This evidence does not make the unit green: the scalar libc backends owned here
and the enumerated KiCad dependencies still return typed Unported errors.

## Grid priority input contract

The unsigned priority transport accepts the JSON integer spelling `-0` as zero,
matching nlohmann's signed integer zero. Arbitrary-precision serde_json preserves
that spelling, so as_u64 alone is insufficient; the reader also accepts a checked
nonnegative as_i64 value. Floating `-0.0` and `0.0` remain invalid integer inputs.
Six literal-request controls are separate from the UB controls and successful
geometry corpus: four genuine priority comparisons and two input-type refusals.
They run through both native adapters and import-free wasm.

The archived d221593095 evidence predates this transport fix. It remains historical
evidence for the unchanged 18,050-case corpus, not a substitute for final-head CI.
The PR links current-head native and registered-main reports.

## Angle libm backend addendum

The private glibc 2.39 SSE2 atan/atan2/tan/asin/acos backends replace the five
owned scalar stubs. Pow, KiROUND and equals retain their typed deferrals and
continuations. The historical evidence above is unchanged. Current-head
qualification and exact geometry counts belong to
`port/findings/geometry-angle-libm-backends-addendum.md`; provenance and the
1,517,520-input-per-function native/wasm harness are in `port/glibc-libm/README.md`.
The additive geometry corpus has 21,154 requests. The unit remains partial.
