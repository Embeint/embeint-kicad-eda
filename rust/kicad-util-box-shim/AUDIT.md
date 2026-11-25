# util.h / BOX2 ownership and validation

Unit `libs.kimath.include.math.application.ui.p001` owns the complete pinned
`math/util.h` (299 lines), `box2.h` (990), and `box2_minmax.h` (116) at
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. Base: `37f4bc0a32`. It remains **partial**.
No source header or util.cpp is modified; the oracle compiles the actual pinned headers.
No golden, tolerance, stored-answer fallback, filtered case or exclusion is used.

`src/util.rs` and `src/box2.rs` in kicad-math are the canonical implementation.
Vectors remain the accepted `Vector2<T>`; util.cpp integer rescale and
RoundedHypotWide are called, not duplicated. `eda_angle.rs` only changes the two
header-helper wrappers. Its scalar_libm/scalar_atan2/scalar_pow stubs and
`glibc_trig` are unchanged. Portable crates continue to forbid unsafe code.

## Body/branch evidence map

The unit record retains the complete clang USR list. These groups account for
all owned definitions, including code after unavailable calls. Case prefixes name
public input cases, not hardcoded outputs. Counts and passing status require the
live reports; a mapping is not proof of complete qualification.

| Owned body/overloads | Rust body and source branches | Corpus evidence and remaining limits |
| --- | --- | --- |
| KiROUND<S,R> | util::ki_round/RoundSource: integer comparisons without double conversion; NaN, upper/lower pre-saturation; llround; final clamp; quiet bypass and nonquiet logging; clamp return after call | round-* / round-default-*; f32/f64 and 8/16/32/64-bit signed/unsigned sources plus distinct C++ long long, targets through signed64/unsigned32 and explicit long long; logging retains util.cpp owner; C++ long double/custom types unqualified |
| KiCheckedCast<S,R> | ki_checked_cast/CheckedCast generic numeric casts; checked_cast_long_long_to_int special logging/clamp | cast-* directly exercises long long->int; Linux generic long->int is exercised by box extremes; other generic pairs have Rust implementation but no complete native qualification |
| rescale<T> | generic multiplication then division, promoted small ints/unsigned wrapping, f32/f64; i32/i64 dispatch to accepted source owner | rescale-*; native i32/i64/f32/f64; other integer types implemented but not native-qualified |
| sign<T> | comparison subtraction, including unordered/-0 | sign-* for all adapter types |
| ct_sqrt, ct_sqrt_helper | checked source addition/division and interval halving; bounded refusal for nonconvergent input | ct_sqrt-*, ct-helper-*; convergent nonnegative inputs; malformed helper ranges are not defined successful inputs |
| isqrt<T> | negative sqrt_max return, double estimate, upward/downward corrections | isqrt-* including signed minima and u64 max |
| UnsignedAbs, RoundedHypot | exact unsigned magnitude incl MIN, narrowMax branch, isqrt rounding, accepted wide callee | abs-*, hypot-* at all dispatch boundaries and MIN/MAX |
| equals<T> | abs-difference strict absolute check; largest abs branch; inclusive relative check; default epsilon | equals-* including epsilon zero/negative/default, inf, NaN and -0; f32/f64 |
| BOX2 default, pos/size default, ByCorners, ByCenter, Normalize | validity, constructor range checks, y-before-x normalization, promoted/coordinate subtraction, vector half-size rounding through default-int KiROUND | box-*-ctor-* includes inverted/raw/zero/default/maximum; scalar type i32/i64/f64 |
| SetMaximum | floating lowest/2; same-width long max/2; widened int twice max | ctor maximum and maximum operations |
| Compute | empty preserves old state, first point init, full min/max scan, origin before size writes | ctor compute and *-compute-* on default and initialized boxes |
| getters and aliases, Centre/GetCenter, GetSizeMax | cast and operation order; size max returns int | *-getters, box-extreme-*; float GetSizeMax out-of-range casts refused |
| SetOrigin/SetSize/SetX/SetY/SetWidth/SetHeight/SetEnd | vector/scalar forms; validity writes; no implicit normalization in setters | setter operations; set_size vector exposed (scalar alias delegates to same body in Rust) |
| Move/Offset/GetWithOffset | checked coordinate writes x then y; copied result preserves validity | offset/offset_xy/move/with_offset |
| Contains point/xy/box | extended relative coords, negative sizes, inclusive boundaries, short circuit origin/end | contains/contains_xy/contains_box with inside/edge/outside cases |
| Intersects box, Intersect | normalize copies; extended ends vs narrowed getters; inclusive intersects, strict positive-area intersection; zero-box fallback | intersects/intersect with separated/edge/degenerate/inverted boxes |
| Intersects segment | contains short circuits, three side calls, false continuation | segment-*; actual SegmentIntersectsSegment call defers to libs.kimath.src.trigo.application.ui.p001 |
| Intersects rotated box | invalid short circuit, normalized angle, epsilon cardinal branches, 90-degree +1 inflation, A/B/C loops | rotated-* exercises invalid/0/180/360/90/270/epsilon/general; RotatePoint calls defer to libs.kimath.include.application.ui.p001; all loops/continuations implemented but deferred general path not executed |
| IntersectsCircle/IntersectsCircleEdge | invalid short circuit, normalized nearest/farthest, squared comparisons, width int/2 vs double/2, +1 outer margin, inner test | circle/circle_edge including zero/negative radii and widths |
| Inflate/GetInflated overloads, inflateAxis | positive/negative sizes, collapse branches, outward coordinate clamps, x-then-y writes | inflate/inflated/uniform, box-random-* and direct outward saturation test |
| Merge box/point | uninitialized source/destination branches; normalize/end expansion and setters | merge/merge_point across all constructor modes |
| GetBoundingBoxRotated | corners order, four RotatePoint calls, min/max scan, origin/end writes | rotated-bbox-*; dependency call defers, downstream scan implemented |
| Area/Diagonal/SquaredDiagonal | extended product; accepted vector norm and saturating squared norm | area/diagonal/squared_diagonal; vector double general hypot still defers to vector owner |
| SquaredDistance/Distance point/box | extended bounds, max difference, checked squares/sum, ordered box axis branches, sqrt, conditional integer KiROUND | distance/squared_distance and *_box, both signed axis directions; UB controls separate |
| NearestPoint/FarthestPointTo | normalize copy, clamp, center and four edge choices | nearest/farthest with inside/edge/outside points |
| equality/inequality, IsValid | normalize copies; coordinate/size comparison; validity independent of comparisons | compare, serialized state/validity on every mutation |
| Format | exact enclosing strings and canonical vector Format/stream calls | format-*; double stream defers to vector owner |
| IsBOX2Safe, BOX2ISafe overloads, box KiROUND | exact +/-INT_MAX bounds, clamp-before-truncation, integral extended add, constructor normalization, component round order | safe, safe_parts, safe_box, round_box; NaN cast UB controls separate |
| BOX2I_MINMAX all constructors | default, edges, point/xy, box getters, two points, real SEG endpoint fields; arc BBox trait dependency then from_box continuation | minmax-*; adapter constructs actual pinned SEG/SHAPE_ARC; Rust arc BBox defers to libs.kimath.src.geometry.shape_arc.application.ui.p001 |
| minmax Intersects/Merge/GetCenter/GetDiameter/BOX2 conversion | inclusive common area; min/max writes; int64 sum/truncation; extended difference and shared finite hypot backend; safe integral conversion | minmax-* and 300 fixed-seed diameter inputs incl full i32 span |

## Reference-specific choices and limits

On Linux, int64_t is `long`, not `long long`. The pinned KiCheckedCast only
clamps/logs **long long -> int**. The explicitly typed BOX2 casts therefore use
C++20 modulo narrowing on the hosted reference; they must not acquire logging or
clamping. VECTOR2 converting constructors independently clamp. Rotation code also
distinguishes implicit scalar narrowing from converting vector construction.

`hypot.rs` is a shared finite-input backend derived from glibc tag glibc-2.39,
`sysdeps/ieee754/dbl-64/e_hypot.c`, non-FMA branches, originally LGPL-2.1-or-later. The derived Rust file is
distributed as GPL-3.0-or-later under LGPL section 3, as in the accepted glibc_trig port.
Source: https://github.com/bminor/glibc/blob/glibc-2.39/sysdeps/ieee754/dbl-64/e_hypot.c.
Original bytes SHA-256: `5bb30147cb13eade6e28eddac0a69b04de71d2850090166e36c94236ac9c514e`.
It is used for minmax diameter's finite converted integer differences. It does
not change the separate worker's glibc_trig or angle libm functions, and does not
claim to qualify the wider floating vector hypot domain. No host hypot is used.
Its zero/large/tiny/correction branches preserve source operation order; errno
and floating exception flags are outside the portable API.

Existing VectorComponent arithmetic supplies the accepted unique-NaN rule and
refuses ambiguous two-payload arithmetic with the vector owner's tag. No new
left-NaN rule is generalized from fp.rs. Scalar equals only returns booleans;
its intermediate NaN payload is unobservable. f32 generic arithmetic and
float-to-float casts outside this corpus remain unqualified for mixed payloads.

Portable APIs return typed refusals for signed-overflow intermediates, integral
division by zero, undefined float casts and invalid clamp bounds. Defined
KiROUND clamping is never rejected as UB. The C++ adapter's guards compute
execution-domain checks, not replies. Extreme general rotation/SEG/arc native
arithmetic is not yet exhaustively guarded/qualified; corpus inputs for those
calls are small. No claim of arbitrary untrusted native-oracle input safety or
full type/branch coverage is made. Rust checks its arithmetic independently.

## Validation

Fixed-seed public corpora are additive and deterministic. The owned corpus has
15,876 inputs and 11 separate UB controls. Vector has 10,286 inputs (+704),
geometry 18,434 (+384). No prior corpus case is deleted or excluded. Native
comparison reports must show zero previously passing regressions. The scoped
workflow runs all three corpora and replays this unit's freshly executed native
replies, including deferrals, in import-free wasm. Other units stay partial.

Local macOS arm64 Rust tests/clippy/wasm build are useful portability checks, not
Linux x86-64 oracle evidence. Hosted reports and exact tested revision are added
under validation after successful live comparison. No separate review is claimed.

## Retained live result

[Snapshot](validation/2026-11-25-d7a17b8e4c/README.md) binds the implementation head,
tested merge, source manifest, full reports, runtime and compiler provenance.
Hosted run 45607429865 passed 15,828 cases: **14,221 exact / 1,607 deferred / zero
failures**. Vector: **9,639 exact / 647 deferred** of 10,286. Geometry: **13,814 exact /
4,620 deferred** of 18,434. All have zero unprocessed cases, faults and extra replies.
All 11 UB controls and wasm replay passed. Every previously passing case remains
compared; 139 accepted vector and 997 geometry deferrals now compare exactly.

The owned corpus defers 1,447 calls to util.cpp kimathLogOverflow, 78 to header
RotatePoint wrappers, 63 to source SegmentIntersectsSegment, 15 to the vector
unit's float formatting/general hypot, and four to SHAPE_ARC::BBox. The caller
reports enumerate their remaining owners. This is an implemented partial unit,
not green or complete native qualification of arbitrary template instantiations.

The current generator additionally includes 36 successful box-level KiROUND
continuations and 12 explicit Linux generic narrowing constructor/getter/
Normalize cases. These extend the retained 15,828-case snapshot to 15,876;
fresh live evidence is required for the additions rather than treating the
logging-only round_box cases as successful continuation coverage.
