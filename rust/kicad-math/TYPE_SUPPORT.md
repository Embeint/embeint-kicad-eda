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

## Not covered

Everything else in the two headers belongs to the owning unit and is absent, not guessed. That is
52 of the unit's 57 USRs (5 are covered above), plus the class-level declarations below:

- `VECTOR2`: the casting constructor and `operator()` (clamped conversions between component
  types), `Cross`, `Dot`, `Distance`, `SquaredDistance`, `EuclideanNorm`, `SquaredEuclideanNorm`,
  `Perpendicular`, `Resize`, `Format`, the compound assignments, unary `-`, `operator/`, the six
  comparison operators, `equals`, `LexicographicalCompare`/`Min`/`Max`, the free `+`, `-` and `*`
  operators, `operator<<` and both `KiROUND` overloads.
- `VECTOR3`: the casting constructor, `Cross`, `Dot`, `EuclideanNorm`, `Normalize`, `SetAll`,
  `operator==`, `operator!=`, `operator*=`, `operator/=` and `operator<<`.
- `VECTOR2_TRAITS`/`VECTOR3_TRAITS` and the members derived from them: `extended_type`,
  `coord_type`, `ECOORD_MAX` and `ECOORD_MIN`.
- The deleted `std::hash<VECTOR2I>` specialisation (Rust `Hash` is simply not derived).
- The default template argument `T = int`: Rust has none, so `VECTOR2<>` is spelled `Vector2I`.

The derived `Debug` is a Rust diagnostic aid only. It is not a port of `operator<<` and its output
must not be compared with KiCad's. No equality (`PartialEq`), ordering, hashing, arithmetic, norm,
rounding or formatting behaviour is provided. The aliases do not give the generic operations of the
C++ templates.

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
