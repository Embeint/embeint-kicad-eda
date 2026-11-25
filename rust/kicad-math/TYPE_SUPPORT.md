# Canonical VECTOR2 / VECTOR3 port and owned-body audit

The owning unit is `libs.kimath.include.math.application.ui.p002`, status **partial**.
It owns the complete 57-USR record: 45 VECTOR2-associated and 12 VECTOR3-associated USRs.
Three extracted scalar +/- signatures cover **six constrained source bodies**; the two
`equals` USRs describe **one actual body**. Counts alone do not establish implementation or
branch qualification. This document lists all USRs, including unqualified types and branches.

| Source | Verified pinned identity |
| --- | --- |
| KiCad | `4cf939778df04d2b747bb2c4714fc5014a7565c7` |
| vector2d.h, 737 lines | blob `6ca73bdd178dd7b0ed6789c57ff5ce51c88512ed`; SHA-256 `445e01764e09e6d0b9cfbfe74af553253a09fce5a290e1940c83cb57150e9599` |
| vector3.h, 234 lines | blob `70f8bb65b08f4397b28b1dcc2183482420a67f95`; SHA-256 `eda25023ca4f79819e9b29d6723d79a6d13becf22a1c3b4967b15b1db8de228e` |
| Implementation | `src/vector.rs`; one public `Vector2<T>` and one `Vector3<T>` |
| Validation | `../kicad-vector-shim/README.md`; raw-bit doubles, exact live comparisons with partial deferrals; no golden |

Sources were read as raw Git blobs, not moving root headers. The supplied `/tmp` exports
were separately hash-verified. Initial official base was `eb4cd4724ff6f5fa2119c86248c1a4beb4847451`.

## API and instantiation boundary

Directly exercised aliases are `Vector2I` (i32), `Vector2L` (i64), `Vector2D` (f64),
`Vector3I` (i32), `Vector3U` (u32), `Vector3D` (f64). Representation remains generic and
existing generic construction/copy/alias tests remain. Rust's default generic `T=i32` also preserves bare `Vector2`/`Vector3` spellings, alongside
`Vector2I`/`Vector3I`; no duplicate public vector is introduced. Integral Vector2 ordering implements Ord for ordered collections; floating relational operators
retain unordered semantics and LexicographicalCompare stays a separate API. No Hash implementation is
provided, preserving the deliberately deleted `std::hash<VECTOR2I>` contract.

`VectorComponent::Extended` ports the trait boundary: i32 -> i64; i64/u32/f64 -> themselves.
ECOORD_MIN/MAX use that extended type; f64 MIN is **MIN_POSITIVE**, not the most negative
finite value. Defaulted/implicit VECTOR3 construction/copy/assignment are covered by Rust
Default/Copy/Clone/assignment and explicit assign_from returning mutable self. coord_type is the generic component T.

Fallible checked operations are named methods. They reject each signed UB intermediate,
not the final result alone, and preserve x-then-y(-then-z) writes on later Rust failure.
Such a state is a Rust safety extension, never claimed as observable post-UB C++ behavior.
Unsigned 3D arithmetic wraps in u32. VECTOR2 norm saturation is independent of Dot.
Perpendicular negates only y. Distance subtracts in extended_type before double conversion.
`try_cast` implements each arithmetic conversion branch for supported aliases; VECTOR2 clamps
before truncation and VECTOR3 directly casts. The actual undefined i64 upper bound is 2^63,
including after a clamp to double(INT64_MAX). Fractional negative double-to-u32 values above
-1 truncate validly to zero. Non-arithmetic fallback casts, f32, long double, unsigned64,
and other user-defined C++ instantiations remain unqualified and are not silently claimed.

The live-scalar-reference API is `Vector2Scalar::{Value,X,Y}` with
`add/sub/mul_assign_operand`: each component write rereads its operand. This expresses
C++'s `v *= v.x` without violating Rust's borrowing rules. Value-only convenience methods
keep copied operands. VECTOR3 takes its scalar by value. Normalize and SetAll return copies;
compound assignments return mutable self. Normalize's wxCHECK return for <=0/unordered is
implemented as unchanged self, including original signalling NaN/zero bits.

The prior Bezier co-port implementations (`checked_sub`, f64 Sub/Add/Mul and f64->i32
TryFrom) remain unchanged. Their historical PR #51/header-hash evidence is not new vector
qualification. In particular private fp.rs's fixed left-NaN choice is **not generalized**.
New fallible arithmetic preserves the single distinct quieted NaN value and bound x86 invalid
arithmetic result; it defers an arithmetic site with two different quieted NaNs. New scalar-site
coverage and this implementation require fresh hosted validation;
valid mixed NaNs remain in the corpus. Pure copies, comparisons, sign changes and failed
Normalize checks retain the NaN domain. General std::hypot and f64 ostream formatting are
owned deferrals. Finite arithmetic/sqrt are provisional until exact hosted comparison;
wasm compilation is not a vector runtime-parity claim.

`vector2_add/sub/dot/mul` and `scalar_mul_vector2` have explicit common-type promotion
for i32/i64/u32/f64 combinations. The initial native adapter exercises same-type calls;
**mixed-type bodies remain unqualified**, even though implementation exists. Scalar integral
+/- with a different integral U (including narrowing result conversions) and f32 U are not
implemented as separate qualified overloads. Floating-RHS +/- for i32/i64 retain default-int
KiROUND, as do integral /double and Resize. Division by floating zero and KiROUND(NaN) are
valid paths, deferred to the helper, never rejected as UB.

## Dependency calls and continuations

No helper is co-ported or reclassified under this unit. No other owner record is edited.
The util source unit was accepted on `rust` by #59 (`kicad_math::math_util`, status partial);
its implemented `rescale<int64_t>` body is now called directly. The util.h header unit
remains `todo`, so its helpers keep their owner deferrals.

| Actual helper | Actual owner | Vector continuation retained |
| --- | --- | --- |
| RoundedHypot, UnsignedAbs, ct_sqrt/isqrt | `libs.kimath.include.math.application.ui.p001` | signed norm's min(type max), cast and return; squared norm uses equivalent bounded u128 arithmetic locally for its owned inline saturation body |
| RoundedHypotWide | `libs.kimath.src.math.util.application.ui.p001` (accepted: `math_util::rounded_hypot_wide`) | reached only inside RoundedHypot, whose header body (narrow isqrt path and wide dispatch) is still missing, so it is not called directly; no DotWide/CrossWide dependency invented |
| scalar KiROUND (NaN -> zero, saturating with logging), sign, generic rescale, scalar equals | `libs.kimath.include.math.application.ui.p001` | vector x/y calls, sign/abs branches, scalar default-int round, equals x-false/y continuation retained; sign call has its own precise owner deferral |
| rescale<int64_t> | `libs.kimath.src.math.util.application.ui.p001` | **wired**: general Resize squares, then `math_util::rescale_i64_int128` (Linux `__int128` profile, matching the oracle) and sqrt for x then y; the following KiROUND call still defers; no invented numerator<=denominator guard |
| kimathLogOverflow | `libs.kimath.src.math.util.application.ui.p001` (accepted as a host-boundary deferral) | not called by vector bodies directly; reached only inside the missing KiROUND |

KiCheckedCast is not called by vectors. RoundedHypotWide's KI_UINT128 implementation belongs
to the helper owner. The owned squared-norm body uses unsigned magnitudes and saturates
exactly, including MIN, rather than calling a nonexistent wide-dot implementation.

## Owned function / branch / observation table

Every row is an owned USR. Case prefixes identify fixtures, not a claim that every listed
branch passed. Hosted evidence is recorded separately; pending/unqualified branches remain
partial. `vector2d.h` applies except VECTOR3 rows, which use `vector3.h`. Source function
labels and USRs identify bodies exactly; constrained scalar overloads are audited together.

| Upstream declaration and exact USR | Rust body | Observable cases | Remaining branch/type limit |
| --- | --- | --- | --- |
| KiROUND(vector)<br>`c:@F@KiROUND#&1$@S@VECTOR2>#d#` | `ki_round` | `ki_round-*` | scalar KiROUND helper deferred |
| KiROUND(x,y)<br>`c:@F@KiROUND#d#d#` | `ki_round_xy` | `ki_round_xy-*` | scalar KiROUND helper deferred |
| LexicographicalCompare<br>`c:@FT@>1#TLexicographicalCompare#&1>@ST>1#T@VECTOR21t0.0#S0_#I#` | `lexicographical_compare` | `bits-*-compare` | none for aliases; unordered x falls through to y |
| LexicographicalMax<br>`c:@FT@>1#TLexicographicalMax#&1>@ST>1#T@VECTOR21t0.0#S0_#S0_#` | `lexicographical_max` | `bits-*-compare` | second reference on ties/unordered |
| LexicographicalMin<br>`c:@FT@>1#TLexicographicalMin#&1>@ST>1#T@VECTOR21t0.0#S0_#S0_#` | `lexicographical_min` | `bits-*-compare` | second reference on ties/unordered |
| equals declaration/body spelling A<br>`c:@FT@>1#Tequals#&1>@ST>1#T@VECTOR21t0.0#S0_#S2_#^enable_if<! std::numeric_limits<type-parameter-0-0>::is_integer, bool>:::type#` | `equals` | `equals-*` | scalar equals deferred; x early false then y continuation retained |
| equals declaration/body spelling B<br>`c:@FT@>1#Tequals#&1>@ST>1#T@VECTOR21t0.0#S0_#S2_#^enable_if<!std::numeric_limits<type-parameter-0-0>::is_integer, bool>:::type#` | `equals` | `equals-*` | same actual body as preceding USR; not another implementation |
| VECTOR2 ostream<br>`c:@FT@>1#Toperator<<#&$@N@std@S@basic_ostream>#C#$@N@std@S@char_traits>#C#&1>@ST>1#T@VECTOR21t0.0#S0_#` | `stream` | `2-*-stream` | double/default formatting and custom stream state unqualified |
| VECTOR3 ostream<br>`c:@FT@>1#Toperator<<#&$@N@std@S@basic_ostream>#C#$@N@std@S@char_traits>#C#&1>@ST>1#T@VECTOR31t0.0#S0_#` | `stream` | `3-*-stream` | double/default formatting and custom stream state unqualified |
| free vector dot *<br>`c:@FT@>2#T#Toperator*#&1>@ST>1#T@VECTOR21t0.0#&1>@ST>1#T@VECTOR21t0.1# #` | `vector2_dot / dot` | `2-*-operator_dot` | mixed instantiations unqualified; distinct quieted NaN operands deferred |
| vector * scalar<br>`c:@FT@>2#T#Toperator*#&1>@ST>1#T@VECTOR21t0.0#&1t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `vector2_mul / try_mul_scalar` | `2-*-mul_scalar` | mixed instantiations unqualified; distinct quieted NaN operands deferred |
| scalar * vector<br>`c:@FT@>2#T#Toperator*#&1t0.0#&1>@ST>1#T@VECTOR21t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `scalar_mul_vector2` | `2-*-scalar_mul` | mixed instantiations unqualified; distinct quieted NaN operands deferred |
| vector + vector<br>`c:@FT@>2#T#Toperator+#&1>@ST>1#T@VECTOR21t0.0#&1>@ST>1#T@VECTOR21t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `vector2_add / try_add` | `random-*-add` | mixed instantiations unqualified; distinct quieted NaN operands deferred |
| vector + scalar (three constrained bodies)<br>`c:@FT@>2#T#Toperator+#&1>@ST>1#T@VECTOR21t0.0#&1t0.1#S1_#` | `try_add_scalar / add_float` | `2-*-add_scalar, add_float-*` | floating-T / integral-T-integral-U / integral-T-floating-U: same types and f64 RHS implemented; KiROUND deferred; other U unqualified |
| vector - vector<br>`c:@FT@>2#T#Toperator-#&1>@ST>1#T@VECTOR21t0.0#&1>@ST>1#T@VECTOR21t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `vector2_sub / try_sub` | `random-*-sub` | mixed instantiations unqualified; distinct quieted NaN operands deferred |
| vector - scalar (integral T, floating U)<br>`c:@FT@>2#T#Toperator-#&1>@ST>1#T@VECTOR21t0.0#&1t0.1#S1_#` | `sub_float` | `2-*-sub_float` | KiROUND helper deferred |
| vector - scalar (floating T or integral T/U)<br>`c:@FT@>2#T#Toperator-#&1>@ST>1#T@VECTOR21t0.0#t0.1#S1_#` | `try_sub_scalar` | `2-*-sub_scalar` | two constrained bodies; other integral U unqualified |
| VECTOR2 Cross<br>`c:@ST>1#T@VECTOR2@F@Cross#&1>@ST>1#T@VECTOR21t0.0#1` | `cross` | `random-*-cross` | distinct quieted NaN operands deferred |
| VECTOR2 Distance<br>`c:@ST>1#T@VECTOR2@F@Distance#&1>@ST>1#T@VECTOR21^VECTOR2_TRAITS<type-parameter-0-0>:::extended_type#1` | `distance` | `distance-subtract-before-double, distance-extended-argument` | subtract in extended_type first; general std::hypot deferred |
| VECTOR2 Dot<br>`c:@ST>1#T@VECTOR2@F@Dot#&1>@ST>1#T@VECTOR21t0.0#1` | `dot` | `random-*-dot` | distinct quieted NaN operands deferred; signed intermediates guarded |
| VECTOR2 EuclideanNorm<br>`c:@ST>1#T@VECTOR2@F@EuclideanNorm#1` | `euclidean_norm / norm2` | `2-*-norm, fp-sites-*-norm` | signed RoundedHypot deferred; double diagonal/axis implemented; general std::hypot deferred; other integral types unqualified |
| VECTOR2 Format<br>`c:@ST>1#T@VECTOR2@F@Format#1` | `format` | `2-*-format` | fresh classic stream; double formatting deferred |
| VECTOR2 Perpendicular<br>`c:@ST>1#T@VECTOR2@F@Perpendicular#1` | `perpendicular` | `2-*-perpendicular-min-x` | negates only y; signed-min y refused |
| VECTOR2 Resize<br>`c:@ST>1#T@VECTOR2@F@Resize#t0.0#1` | `resize` | `2-*-resize-*, floating-resize-*` | zero/diagonal/general branches retained; integral general branch runs the accepted rescale<int64_t>/sqrt (`2-*-resize-general-*`, `random-*-resize-general`); KiROUND, sign and rescale<double> deferred at actual calls; no naive normalized replacement |
| VECTOR2 SquaredDistance<br>`c:@ST>1#T@VECTOR2@F@SquaredDistance#&1>@ST>1#T@VECTOR21t0.0#1` | `squared_distance` | `random-*-squared_distance` | each extended subtraction/product/sum guarded; distinct quieted NaN operands deferred |
| VECTOR2 SquaredEuclideanNorm<br>`c:@ST>1#T@VECTOR2@F@SquaredEuclideanNorm#1` | `squared_euclidean_norm` | `2-*-squared_norm` | i32/i64 exact saturation; double distinct quieted NaN operands deferred |
| VECTOR2 default constructor<br>`c:@ST>1#T@VECTOR2@F@VECTOR2#` | `Default` | `2-*-default` | value initialization; generic non-arithmetic uses T::default |
| VECTOR2 copy constructor<br>`c:@ST>1#T@VECTOR2@F@VECTOR2#&1>@ST>1#T@VECTOR21t0.0#` | `Copy / Clone` | `bits-*-copy` | all bits preserved |
| VECTOR2 component constructor<br>`c:@ST>1#T@VECTOR2@F@VECTOR2#t0.0#S0_#` | `new` | `bits-*-new` | all bits preserved |
| VECTOR2 !=<br>`c:@ST>1#T@VECTOR2@F@operator!=#&1>@ST>1#T@VECTOR21t0.0#1` | `PartialEq::ne` | `bits-*-compare` | short circuit x then y |
| VECTOR2 *= vector<br>`c:@ST>1#T@VECTOR2@F@operator*=#&1>@ST>1#T@VECTOR21t0.0#` | `mul_assign` | `2-*-mul_assign` | x then y; alias-self vector has independent components |
| VECTOR2 *= scalar reference<br>`c:@ST>1#T@VECTOR2@F@operator*=#&1t0.0#` | `mul_assign_operand` | `2-*-mul_assign_scalar-alias-*` | live x/y alias reads; copied-value convenience also provided |
| VECTOR2 += vector<br>`c:@ST>1#T@VECTOR2@F@operator+=#&1>@ST>1#T@VECTOR21t0.0#` | `add_assign` | `2-*-add_assign` | x then y; Rust late failure retains first write |
| VECTOR2 += scalar reference<br>`c:@ST>1#T@VECTOR2@F@operator+=#&1t0.0#` | `add_assign_operand` | `2-*-add_assign_scalar-alias-*` | live x/y alias reads |
| VECTOR2 unary -<br>`c:@ST>1#T@VECTOR2@F@operator-#1` | `try_neg` | `2-*-neg` | signed-min component refused; NaN sign bit toggled without arithmetic |
| VECTOR2 -= vector<br>`c:@ST>1#T@VECTOR2@F@operator-=#&1>@ST>1#T@VECTOR21t0.0#` | `sub_assign` | `2-*-sub_assign` | x then y |
| VECTOR2 -= scalar reference<br>`c:@ST>1#T@VECTOR2@F@operator-=#&1t0.0#` | `sub_assign_operand` | `2-*-sub_assign_scalar-alias-*` | live x/y alias reads |
| VECTOR2 / double<br>`c:@ST>1#T@VECTOR2@F@operator/#d#1` | `divide` | `2-*-divide-*` | integral uses default int KiROUND even for i64; zero/NaN valid and deferred to helper |
| VECTOR2 <<br>`c:@ST>1#T@VECTOR2@F@operator<#&1>@ST>1#T@VECTOR21t0.0#1` | `less` | `bits-*-compare` | x unordered never falls through to y |
| VECTOR2 <=<br>`c:@ST>1#T@VECTOR2@F@operator<=#&1>@ST>1#T@VECTOR21t0.0#1` | `less_equal` | `bits-*-compare` | same x ordering/eq condition |
| VECTOR2 assignment<br>`c:@ST>1#T@VECTOR2@F@operator=#&1>@ST>1#T@VECTOR21t0.0#` | `assign_from / Rust assignment` | `2-*-assign, bits-*-assign` | memberwise copy and mutable-self return; source bits retained |
| VECTOR2 ==<br>`c:@ST>1#T@VECTOR2@F@operator==#&1>@ST>1#T@VECTOR21t0.0#1` | `PartialEq::eq` | `bits-*-compare` | short circuit x then y |
| VECTOR2 ><br>`c:@ST>1#T@VECTOR2@F@operator>#&1>@ST>1#T@VECTOR21t0.0#1` | `greater` | `bits-*-compare` | same x ordering/eq condition |
| VECTOR2 >=<br>`c:@ST>1#T@VECTOR2@F@operator>=#&1>@ST>1#T@VECTOR21t0.0#1` | `greater_equal` | `bits-*-compare` | same x ordering/eq condition |
| VECTOR2 converting constructor<br>`c:@ST>1#T@VECTOR2@FT@>1#TVECTOR2#&1>@ST>1#T@VECTOR21t1.0#v#` | `try_cast / Vector2CastFrom` | `2-*-cast-*` | floating target / clamped floating source / clamped integral source implemented for aliases; fallback non-arithmetic unqualified |
| VECTOR2 operator()<U><br>`c:@ST>1#T@VECTOR2@FT@>1#Toperator()#>@ST>1#T@VECTOR21t1.0#1` | `try_cast / Vector2CastFrom` | `2-*-cast_call-*` | same branches; identity copy bypasses arithmetic; non-alias U unqualified |
| VECTOR3 Cross<br>`c:@ST>1#T@VECTOR3@F@Cross#&1>@ST>1#T@VECTOR31t0.0#1` | `cross` | `3-*-cross` | T products/subtractions, not extended; cancellation UB refused; u32 wraps; distinct quieted NaN operands deferred |
| VECTOR3 Dot<br>`c:@ST>1#T@VECTOR3@F@Dot#&1>@ST>1#T@VECTOR31t0.0#1` | `dot` | `3-*-dot` | extended products and left-associated partial sum; u32 wraps; distinct quieted NaN operands deferred |
| VECTOR3 EuclideanNorm<br>`c:@ST>1#T@VECTOR3@F@EuclideanNorm#1` | `euclidean_norm` | `3-*-norm` | sqrt after extended ordered sum; integral cast truncates; distinct quieted NaN arithmetic deferred |
| VECTOR3 Normalize<br>`c:@ST>1#T@VECTOR3@F@Normalize#` | `normalize` | `3-fp-*-normalize, 3-u32-*-normalize` | norm<=0/unordered unchanged copy; +inf divides; finite paths implemented; distinct quieted NaN divisions deferred |
| VECTOR3 SetAll<br>`c:@ST>1#T@VECTOR3@F@SetAll#t0.0#` | `set_all` | `3-*-set_all` | x/y/z writes and copied return |
| VECTOR3 component constructor<br>`c:@ST>1#T@VECTOR3@F@VECTOR3#t0.0#S0_#S0_#` | `new` | `bits-*-new` | all bits preserved |
| VECTOR3 !=<br>`c:@ST>1#T@VECTOR3@F@operator!=#&1>@ST>1#T@VECTOR31t0.0#1` | `PartialEq::ne` | `bits-*-compare` | short circuit x/y/z |
| VECTOR3 *= scalar value<br>`c:@ST>1#T@VECTOR3@F@operator*=#t0.0#` | `mul_assign_scalar` | `3-*-mul_assign_scalar-alias-*` | scalar copied before first write; x/y/z mutation order |
| VECTOR3 /= scalar value<br>`c:@ST>1#T@VECTOR3@F@operator/=#t0.0#` | `div_assign_scalar` | `3-*-div_assign_scalar-alias-*` | integer zero and MIN/-1 refused; u32 division; distinct quieted NaN operands deferred |
| VECTOR3 ==<br>`c:@ST>1#T@VECTOR3@F@operator==#&1>@ST>1#T@VECTOR31t0.0#1` | `PartialEq::eq` | `bits-*-compare` | short circuit x/y/z |
| VECTOR3 converting constructor<br>`c:@ST>1#T@VECTOR3@FT@>1#TVECTOR3#&1>@ST>1#T@VECTOR31t1.0#v#` | `try_cast / Vector3CastFrom` | `3-*-cast-*` | direct unclamped conversion; i32/u32 narrowing follows C++20 modulo semantics, with direct boundary cases; other types unqualified |

## Evidence and remaining work

`tests/vector.rs` retains constructor, copy, const, generic-type and alias identity checks.
`tests/bezier_operations.rs` retains existing consumer regression coverage. New
`tests/vector_operations.rs` covers unique/ambiguous quieted NaNs, scalar aliases, per-intermediate overflow, late-failure
mutation, cast intervals, extended-before-double Distance, norm saturation, Normalize state
and return, unordered comparison, second-reference identity and f64 ECOORD_MIN. It also checks
that integral Resize now runs the accepted `rescale<int64_t>` and reaches the KiROUND deferral;
an in-module test checks the exact rescale/sqrt lengths, including an `__int128`-only product.

The deterministic generator emits 9,582 valid cases and 30 separate UB refusal controls.
Every corpus case runs in the real pinned C++ and in Rust; Rust deferrals are explicit error
envelopes with actual owner, while C++ must return a real result. Refusals are outside the
success corpus and assert a later request survives. There is no tolerance, lookup table,
recorded-answer fallback, default success or blessed golden. The initial live report must
retain exact compared/deferred/failure counts before any qualification claim.

Still missing: the named helper implementations and their downstream continuations' live
execution, per-site selection between distinct quieted arithmetic NaNs, general double hypot/libm parity,
default-double stream formatting, supplied stream flags/precision/width/locale/failure and
chaining, other template types and mixed-type native coverage, vector wasm runtime parity.
Fresh stream Format differs from supplied-stream insertion; the latter consumes width at the
first string insertion and never appends a newline. These custom stream states have no
runner entry yet and are explicitly unimplemented. There is no claim of 57 fully ported USRs.


The initial live result and its exact revision binding are retained at
`../kicad-vector-shim/validation/2026-11-24-f98485d7/`. The unique-NaN rule's measured
motivation and the remaining ambiguous sites are recorded in
`../../port/findings/vector-unique-nan-arithmetic.md`.
Those observations are not an output lookup and do not certify later code without a fresh run.

The current adapter invokes the actual public common-type arithmetic functions for free
vector operators and scalar-first multiplication. Assignment observes the returned reference
through `assign_from`; every compound mutation compares its actual returned address with the
destination address. The C++ adapter observes the real assignment-expression reference too.
Rust const-evaluation of arithmetic/assignment beyond the existing constructors remains
unqualified. Mixed scalar +/- with a different integral U, f32 scalar specializations and
non-arithmetic fallback casts are not implemented as separate public overloads; they are
explicit missing instantiations, not certified by same-type cases.
