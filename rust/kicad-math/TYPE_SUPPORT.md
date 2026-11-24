# Supporting types in kicad-math

This crate currently holds one supporting module, [`src/vector.rs`](src/vector.rs): the canonical
minimal co-port of KiCad's `VECTOR2<T>` and `VECTOR3<T>`. It exists so that several units can share
one representation before the owning unit is ported. It is not a port of that unit, and the unit's
record is unchanged.

| | |
| --- | --- |
| Owning unit | `libs.kimath.include.math.application.ui.p002` (status `todo`, unclaimed, `tests = []`; record not edited) |
| Upstream | KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7` (`port/pins.toml`) |
| `libs/kimath/include/math/vector2d.h` | 737 lines, Git blob `6ca73bdd178dd7b0ed6789c57ff5ce51c88512ed`, SHA-256 `445e01764e09e6d0b9cfbfe74af553253a09fce5a290e1940c83cb57150e9599` |
| `libs/kimath/include/math/vector3.h` | 234 lines, Git blob `70f8bb65b08f4397b28b1dcc2183482420a67f95`, SHA-256 `eda25023ca4f79819e9b29d6723d79a6d13becf22a1c3b4967b15b1db8de228e` |
| Rust source | `src/vector.rs`, Git blob `aaa1438cd25bd9fa1affe15425e1411ac4e48013` |
| Rust provenance | Byte-identical to `rust/kicad-math/src/vector.rs` at `8ef0c0e846ba8f190080ddf46c77373dcc0e4608` (draft PR #50), read from that Git object |
| Consumers | Unit `libs.kimath.include.math.application.pure.p001` (PR #50: `MATRIX3x3`, wide integers) and unit `libs.kimath.include.application.pure.p001` (PR #51: header hashing and Bézier support) |

The module text names "the matrix and wide integer unit" because PR #50 introduced it. It is
deliberately left byte-identical so that both PRs refer to one file with one type identity.

## Declarations covered

The function identifiers are the clang USRs in the owning unit's `functions` list (57 in all).

| C++ declaration (pinned line) | USR | Rust |
| --- | --- | --- |
| `template <class T = int> class VECTOR2` with public `T x, y;` (vector2d.h:65-66, 75) | (class) | `pub struct Vector2<T> { pub x: T, pub y: T }` |
| `constexpr VECTOR2()`, defined `: x{}, y{}` (vector2d.h:78, 266-269) | `c:@ST>1#T@VECTOR2@F@VECTOR2#` | `#[derive(Default)]`: `T::default()` per component, which is value initialisation for the arithmetic types used (+0.0, never -0.0; integer 0) |
| `constexpr VECTOR2( T x, T y )`, body `x = aX; y = aY;` (vector2d.h:81, 272-277) | `c:@ST>1#T@VECTOR2@F@VECTOR2#t0.0#S0_#` | `pub const fn Vector2::new(x, y)` |
| `constexpr VECTOR2( const VECTOR2<T>& )`, body `x = aVec.x; y = aVec.y;` (vector2d.h:116-120) | `c:@ST>1#T@VECTOR2@F@VECTOR2#&1>@ST>1#T@VECTOR21t0.0#` | `#[derive(Clone, Copy)]` (memberwise copy) |
| `constexpr VECTOR2<T>& operator=( const VECTOR2<T>& )`, body `x = aVector.x; y = aVector.y;` (vector2d.h:221, 342-348) | `c:@ST>1#T@VECTOR2@F@operator=#&1>@ST>1#T@VECTOR21t0.0#` | Rust assignment of the `Copy` type (memberwise copy); no separate code |
| `template <class T = int> class VECTOR3` with public `T x{}; T y{}; T z{};` (vector3.h:53-54, 63-65) | (class) | `pub struct Vector3<T> { pub x: T, pub y: T, pub z: T }` |
| `VECTOR3() = default;` with the default member initialisers (vector3.h:68) | none (defaulted) | `#[derive(Default)]` |
| `VECTOR3( T x, T y, T z )`, defined `: x( aX ), y( aY ), z( aZ )` (vector3.h:71, 118-122) | `c:@ST>1#T@VECTOR3@F@VECTOR3#t0.0#S0_#S0_#` | `pub const fn Vector3::new(x, y, z)` |
| implicit `VECTOR3` copy constructor and copy assignment | none (implicit) | `#[derive(Clone, Copy)]` |
| `typedef VECTOR2<double> VECTOR2D;` (vector2d.h:707) | (typedef) | `pub type Vector2D = Vector2<f64>` |
| `typedef VECTOR2<int32_t> VECTOR2I;` (vector2d.h:708) | (typedef) | `pub type Vector2I = Vector2<i32>` |
| `typedef VECTOR2<int64_t> VECTOR2L;` (vector2d.h:709) | (typedef) | `pub type Vector2L = Vector2<i64>` |
| `typedef VECTOR3<double> VECTOR3D;` (vector3.h:230) | (typedef) | `pub type Vector3D = Vector3<f64>` |
| `typedef VECTOR3<int> VECTOR3I;` (vector3.h:231) | (typedef) | `pub type Vector3I = Vector3<i32>` |
| `typedef VECTOR3<unsigned int> VECTOR3U;` (vector3.h:232) | (typedef) | `pub type Vector3U = Vector3<u32>` |

Each covered body stores its arguments unchanged, with no arithmetic, so every bit is kept: NaN
payloads (quiet and signalling), signed zeros, infinities, subnormals and integer extremes. The
aliases are type aliases, so `Vector2D` and `Vector2<f64>` are one type, also through the crate
root re-exports.

## Arithmetic and conversion for `BEZIER<T>` (PR #51)

Unit `libs.kimath.include.application.pure.p001` owns the inline `BEZIER<T>` constructor,
`PointAt`, `Split` and `SubCurve` (`libs/kimath/include/bezier_curves.h:95-191`). KiCad
instantiates `BEZIER` with `double` (`EDA_SHAPE`) and `int` (`TransformEllipseToBeziers`, the
Altium importer). With the parent's authorization (no claimant for ui.p002), exactly the
`VECTOR2` definitions those bodies call were co-ported, and only for the instantiations they
use. Overload resolution was checked against the pinned header: for `VECTOR2 + VECTOR2` and
`VECTOR2 - VECTOR2` the vector-vector templates are more specialised than the constrained
vector-scalar ones, and `double * VECTOR2<T>` matches only the scalar-first template.

| C++ definition (pinned lines), USR | Instantiation called by | Branch / body | Rust |
| --- | --- | --- | --- |
| free `operator-( const VECTOR2<T>&, const VECTOR2<U>& )` (vector2d.h:490-495), `c:@FT@>2#T#Toperator-#&1>@ST>1#T@VECTOR21t0.0#&1>@ST>1#T@VECTOR21t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `<int, int>`: `BEZIER<int>::Split` (`C1 - Start` and the five later differences) | `VECTOR2<int>( aLHS.x - aRHS.x, aLHS.y - aRHS.y )`; signed overflow is undefined | `Vector2::<i32>::checked_sub` (`Err(IntegerOverflow)`) |
| the same | `<double, double>`: `BEZIER<double>::Split` | `VECTOR2<double>` component differences | `impl Sub for Vector2<f64>` |
| free `operator*( const T& aScalar, const VECTOR2<U>& )` (vector2d.h:541-545), `c:@FT@>2#T#Toperator*#&1t0.0#&1>@ST>1#T@VECTOR21t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `<double, int>`: `BEZIER<int>::PointAt`/`Split`; `<double, double>`: `BEZIER<double>` | `VECTOR2<double>( aScalar * aVector.x, aScalar * aVector.y )`, `int` converted exactly | `impl Mul<Vector2<i32>> for f64`, `impl Mul<Vector2<f64>> for f64` |
| free `operator+( const VECTOR2<T>&, const VECTOR2<U>& )` (vector2d.h:461-466), `c:@FT@>2#T#Toperator+#&1>@ST>1#T@VECTOR21t0.0#&1>@ST>1#T@VECTOR21t0.1#>@ST>1#T@VECTOR21^common_type<type-parameter-0-0, type-parameter-0-1>:::type#` | `<int, double>`: `BEZIER<int>::Split`; `<double, double>`: both (`PointAt` sums in `double` for both) | `VECTOR2<double>( aLHS.x + aRHS.x, aLHS.y + aRHS.y )` | `impl Add<Vector2<f64>> for Vector2<i32>`, `impl Add<Vector2<f64>> for Vector2<f64>` |
| casting constructor `template <typename CastingType> VECTOR2( const VECTOR2<CastingType>& )` (vector2d.h:84-113), `c:@ST>1#T@VECTOR2@FT@>1#TVECTOR2#&1>@ST>1#T@VECTOR21t1.0#v#` | `VECTOR2<int>` from `VECTOR2<double>`: every `BEZIER<int>` intermediate and the `PointAt<int>` return | only the `std::is_floating_point<CastingType>` branch (:92-99): `static_cast<int>( std::clamp( v, double( INT_MIN ), double( INT_MAX ) ) )`; `std::clamp` passes NaN through and converting it is undefined | `impl TryFrom<Vector2<f64>> for Vector2<i32>` (`Err(NanToInteger)`) |

`double` NaN results use the private `fp` module: one NaN operand is returned quieted (sign
and payload kept); an invalid operation without a NaN operand gives x86's negative indefinite
NaN `0xfff8_0000_0000_0000`; both rules match the live oracle on every case. Every non-NaN
result is plain IEEE arithmetic, which Rust computes identically on every target, and the
explicit rules make the native and wasm builds agree on NaN bits too. With two NaN operands
the left operand is kept. That is a fixed choice, not a property of the reference: IEEE 754
leaves it open, and the compiled oracle keeps the left operand in some statements and the
right in others (`port/findings/kimath-header-hash-bezier-two-nan-payloads.md`). A vector
operator with one fixed order cannot express that, so `kicad_hash`'s `BEZIER<double>` port
evaluates its component arithmetic directly in the bound reference's per-site order
(`bezier_f64_sites`); these operators keep their behaviour and remain the canonical co-ports
(`BEZIER<int>` still uses them, where only `aT` can be NaN and any NaN is refused).

Evidence: `tests/bezier_operations.rs` (each instantiation; overflow of either component;
signed zeros; promotion of `INT_MIN`/`INT_MAX`; the clamp boundaries `double( INT_MAX )`,
`2147483647.9`, `2147483648.0`, infinities and `±DBL_MAX` saturating; truncation toward zero of
positive and negative fractions and subnormals; NaN refusal; operand and generated NaN bits) and
the unit's oracle cases (`bezier-*` in `port/cases/libs.kimath.include.application.pure.p001.jsonl`
and the refused `BEZIER<int>` requests in
`rust/kicad-hash-shim/fixtures/bezier-int-undefined.requests.jsonl`), compared exactly against
the pinned C++ calling these operators.

The owning unit's record keeps its status (`todo`), claim, tests and every generated field; only
its `notes` list these co-ports.

## Not covered

Everything else in the two headers belongs to the owning unit and is absent, not guessed. That is
48 of the unit's 57 USRs (5 are covered above and 4 partially, for the instantiations and branch
listed in the previous section), plus the class-level declarations below:

- `VECTOR2`: the other branches of the casting constructor (floating or integral target from
  other types) and `operator()`, `Cross`, `Dot`, `Distance`, `SquaredDistance`, `EuclideanNorm`,
  `SquaredEuclideanNorm`, `Perpendicular`, `Resize`, `Format`, the compound assignments, unary `-`,
  `operator/`, the six comparison operators, `equals`, `LexicographicalCompare`/`Min`/`Max`, the
  free `+`, `-` and `*` operators at other instantiations and their vector-scalar overloads, the
  dot-product `operator*( VECTOR2, VECTOR2 )`, `operator<<` and both `KiROUND` overloads.
- `VECTOR3`: the casting constructor, `Cross`, `Dot`, `EuclideanNorm`, `Normalize`, `SetAll`,
  `operator==`, `operator!=`, `operator*=`, `operator/=` and `operator<<`.
- `VECTOR2_TRAITS`/`VECTOR3_TRAITS` and the members derived from them: `extended_type`,
  `coord_type`, `ECOORD_MAX` and `ECOORD_MIN`.
- The deleted `std::hash<VECTOR2I>` specialisation (Rust `Hash` is simply not derived).
- The default template argument `T = int`: Rust has none, so `VECTOR2<>` is spelled `Vector2I`.

The derived `Debug` is a Rust diagnostic aid only. It is not a port of `operator<<` and its output
must not be compared with KiCad's. No equality (`PartialEq`), ordering, hashing, norm, rounding or
formatting behaviour is provided, and no arithmetic beyond the instantiations listed above. The
aliases do not give the generic operations of the C++ templates.

## Evidence

`tests/vector.rs` checks default construction (all bits zero for floats; zero for each integer
type), component construction in order for every pair and triple of 14 f64 bit patterns,
integer extremes for each alias, `const` construction, copy, clone and assignment keeping bits
with the source unchanged, generic component types (f32 NaN payloads, `u8`, `i16`, a non-`Copy`
`String`), and alias and re-export type identity through `TypeId` and by passing values between
spellings.

There are no oracle cases or goldens for these representations. Their bodies are memberwise stores,
which the Rust tests check directly. The operations that would need the oracle are the ones listed
above as not covered. The owning unit stays `todo` with no tests, and nothing here makes it
`partial` or `green`.

## Extending these types

Later co-ports add `impl` blocks and functions to `src/vector.rs` for these same types. They do not
add other public vector types or a second vector crate. A co-port that needs more of `VECTOR2` or
`VECTOR3` agrees it with the owning unit's claimant (or with the parent while the unit is
unclaimed), records the added symbols here and in that unit's notes, and checks the added
operations against the C++ oracle.
