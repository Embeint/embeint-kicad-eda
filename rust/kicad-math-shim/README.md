<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath matrix and wide integer difftest adapter

Native JSONL adapter for unit `libs.kimath.include.math.application.pure.p001`:
`libs/kimath/include/math/matrix3x3.h` and `libs/kimath/include/math/wide_int.h` at pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The unit owns 24 definitions: 20 in
`matrix3x3.h` and 4 in `wide_int.h` (`WordsToDouble`, `ToDouble`, `CrossWide`, `DotWide`).
The portable implementation is [`rust/kicad-math`](../kicad-math). The C++ counterpart is
[`port/oracle/shims/kimath_math.cpp`](../../port/oracle/shims/kimath_math.cpp), which calls
the genuine pinned header templates compiled with the oracle's fixed flags
(`-fno-fast-math -ffp-contract=off`). This crate is native (`portable = false`); it parses
requests with the shared `kicad_difftest::request::parse`.

## Instantiations

`MATRIX3x3` is a template. At the pin it is instantiated as:

- `MATRIX3x3<double>` (`MATRIX3x3D`), throughout the application.
- `MATRIX3x3<int>`, in `qa/tests/libs/kimath/math/test_matrix3x3.cpp`. That test deduces it
  from `VECTOR3I` rows, multiplies it by `VECTOR3I( 5, 5, 5 )` and by the `int` 5, and
  compares results with `==`.

The scalar operators are instantiated with `S = double` (from `Inverse`) and `S = int` (the QA
test). The adapters cover all four combinations. `MATRIX3x3D m( VECTOR3I{...}, ... )` in the
same QA file converts the vectors with `VECTOR3`'s casting constructor first. That conversion
belongs to `libs.kimath.include.math.application.ui.p002`, not to a matrix definition.

## Value encodings

| Kind | JSON |
| --- | --- |
| binary64 | exactly 16 lowercase hex digits of the IEEE-754 bits, e.g. `"3ff0000000000000"` |
| `int` | JSON integer in [-2^31, 2^31 - 1] (integer `-0` is 0) |
| `VECTOR2<T>` / `VECTOR3<T>` | array of 2 / 3 elements of `T` |
| `MATRIX3x3<T>` | array of 3 rows of 3 elements of `T` (`m_data[row][column]`) |
| `int64_t` | JSON integer in the int64 range (integer `-0` is 0) |
| `uint64_t` | JSON integer in [0, 2^64 - 1] (integer `-0` is 0) |
| `VECTOR2L` | array of 2 int64 integers |
| `KI_INT128` | canonical decimal string: optional `-`, no leading zeros, no `-0` |

Bit strings carry infinities, every NaN sign and payload (including signalling NaNs), signed
zeros and subnormals losslessly in both directions. No JSON number ever holds a float, so
non-finite results need no special case and no numeric token formatting can differ. Any bit
pattern is accepted as input.

## Operations

`input` is `{"op": NAME, ...fields}` with exactly the listed fields plus the listed optional
modifiers. A matrix argument is built with the nine-element constructor before the operation
is called.

- `type`: `"double"` (default when absent) or `"int"` selects `T`. It is allowed on every
  matrix operation.
- `scalar_type`: `"double"` (default) or `"int"` selects `S` for `mul_scalar` and `scalar_mul`.
- `state`: on `stream` only, see below.

| op | fields | result | C++ called |
| --- | --- | --- | --- |
| `matrix_default` | | `matrix` | `MATRIX3x3()` |
| `matrix_from_vectors` | `a1`, `a2`, `a3` | `matrix` | `MATRIX3x3( VECTOR3, VECTOR3, VECTOR3 )` |
| `matrix_from_elements` | `elements` | `matrix` | `MATRIX3x3( a00, ..., a22 )` |
| `set_identity` | `matrix` | `matrix` | `SetIdentity` |
| `set_translation` | `matrix`, `translation` | `matrix` | `SetTranslation` |
| `get_translation` | `matrix` | `vector` | `GetTranslation` |
| `set_rotation` | `matrix`, `angle` (`T`) | `matrix` | `SetRotation` (libm `cos`, `sin`) |
| `set_scale` | `matrix`, `scale` | `matrix` | `SetScale` |
| `get_scale` | `matrix` | `vector` | `GetScale` |
| `determinant` | `matrix` | `value` | `Determinant` |
| `inverse` | `matrix` | `matrix` | `Inverse` (adjugate, `Determinant`, `operator*( M, double )`) |
| `transpose` | `matrix` | `matrix` | `Transpose` |
| `compare` | `matrix`, `other` | `equal`, `not_equal` | `operator==`, `operator!=` |
| `mul_matrix` | `a`, `b` | `matrix` | `operator*( M, M )` |
| `mul_vector2` | `matrix`, `vector` | `vector` | `operator*( M, VECTOR2 )` |
| `mul_vector3` | `matrix`, `vector` | `vector` | `operator*( M, VECTOR3 )` |
| `mul_scalar` | `matrix`, `scalar` (`S`) | `matrix` | `operator*( M, S )` |
| `scalar_mul` | `scalar` (`S`), `matrix` | `matrix` | `operator*( S, M )` |
| `stream` | `matrix` | `text` (+ `width_after` with `state`) | `operator<<( std::ostream&, M )` |
| `words_to_double` | `hi`, `lo` | `value` | `KIGEOM_WIDE::WordsToDouble` |
| `to_double` | `value` | `value`, `via_words` | `ToDouble( KI_INT128 )`; MSVC body `WordsToDouble( _Word[1], _Word[0] )` |
| `cross_wide` | `a`, `b` | `value`, `double` | `CrossWide`, then `ToDouble` |
| `dot_wide` | `a`, `b` | `value`, `double` | `DotWide`, then `ToDouble` |

The MSVC `ToDouble` overload cannot be compiled on Linux (`std::_Signed128`); its entire body
is `WordsToDouble` on the high and low words, which the oracle calls with those two words.
`via_words` therefore checks the MSVC branch's arithmetic, not MSVC's type.

### Undefined inputs

Only inputs with undefined behaviour in C++ are refused. Defined `int` behaviour is computed,
never guarded:

- `DotWide` with all four components `INT64_MIN` (signed `__int128` overflow):
  `DotWide overflows KI_INT128`.
- `MATRIX3x3<int>` signed overflow in the operation's own expression order:
  `<symbol> overflows int`, where the symbol is `MATRIX3x3<int>::Determinant`,
  `MATRIX3x3<int>::Inverse` (cofactors), `operator*( MATRIX3x3<int>, MATRIX3x3<int> )`,
  `operator*( MATRIX3x3<int>, VECTOR2<int> )`, `operator*( MATRIX3x3<int>, VECTOR3<int> )` or
  `operator*( MATRIX3x3<int>, int )`.
- `operator*( MATRIX3x3<int>, double )` converting a NaN or a value whose truncation is
  outside `int` back to `int`:
  `operator*( MATRIX3x3<int>, double ) converts a value outside int`. Every singular
  `MATRIX3x3<int>::Inverse` reaches this, because `1.0 / 0` makes each member infinite or NaN.

The oracle checks these preconditions before calling the genuine template; the checks
recompute only the preconditions and never supply a result. The Rust library returns
`WideIntError`/`MatrixError` for them.

### Stream state

`operator<<` writes into the caller's stream, so its output depends on that stream's state.
Without `state`, the oracle uses a fresh `std::ostringstream`. With
`"state": {"flags": [...], "precision": n, "width": n, "fill": "c"}` (all four keys
required), the stream gets exactly those `fmtflags`, replacing the defaults
(`skipws | dec`), plus that precision, width and fill before the insertion. The reply then
adds `width_after`, the stream's width afterwards, which every insertion resets to 0.

- `flags`: distinct names from `boolalpha`, `dec`, `fixed`, `hex`, `internal`, `left`, `oct`,
  `right`, `scientific`, `showbase`, `showpoint`, `showpos`, `skipws`, `unitbuf`,
  `uppercase`. Combinations such as `fixed` + `scientific` (hexfloat) or no base flag at all
  are allowed and behave as libstdc++ does.
- `precision`, `width`: integers in [-64, 512]. This is a protocol resource bound against
  multi-megabyte expansions, not a claim about the C++ domain. A negative precision means 6
  for floating point; a width only pads the first `"| "`.
- `fill`: one printable ASCII character.
- Locale: the classic `"C"` locale only. The oracle never changes the global locale, and the
  port models no other decimal point or grouping.

The Rust API is `Matrix3x3::stream_text( &mut StreamState )` with `kicad_math::StreamState`.
There is deliberately no `Display` implementation that would ignore caller state.

Refusal messages, in validation order: `expected op string`, `unknown operation`,
`unexpected or missing field`, `expected type: double or int`,
`expected scalar_type: double or int`, then per field in the order listed:
`expected matrix: F`, `expected vector: F`, `expected double bit string: F`,
`integer out of range: F`, `expected int128 string: F`, the undefined-input messages above,
and for `state` last, `expected stream state` or `integer out of range: precision|width`.
`test_refusals.py ORACLE RUST` sends 260 refusal probes (each followed by a valid request) to
both programs and requires identical complete replies.

## Cases

`generate_cases.py` writes 7,460 public fixed-seed requests to
`port/cases/libs.kimath.include.math.application.pure.p001.jsonl`. Groups are appended, so
earlier requests never change:

| Group | Cases | Contents |
| --- | ---: | --- |
| original | 3,485 | Unchanged and byte-identical to the qualified corpus (sha256 `c21be36d…`). Boundary values, singular, rank-deficient, near-singular, overflowing and underflowing matrices, all nine equality positions, `%.6g` boundaries and ties, `WordsToDouble` halfway/sticky cases at every leading-zero count, `ToDouble` ties, int64 extremes, 33-bit deltas. Its arithmetic cases combine NaNs of one bit pattern only. |
| `int-*`, `double-*-int-*` | 185 | `MATRIX3x3<int>` incl. the three QA checks, int extremes, `SetRotation<int>` angles whose double cosine is exactly ±1, defined products at the int boundary, inverses with truncation toward zero, and `S = int` / `S = double` scalars for both element types. |
| `stream-state-*` | 279 | Every floatfield (default, fixed, scientific, hexfloat), with `showpos`, `showpoint`, `uppercase`, precisions -1 to 40 and 512, widths, fills and adjustments, integer base/showbase/showpos/uppercase, the reviewer's fixed-precision-2 example, and fixed-seed random states. |
| `set-rotation-variant-*` | 10 | Angles where glibc's FMA and SSE2 variants differ by 1 ulp (qualified WSL probe). |
| `minimal-qnan-determinant`, `mixed-nan-*` | 3,501 | The exact qualified mixed-NaN request set (sha256 `85fdf99b…`, checked by the generator). Valid inputs with at least two different NaNs. |

## Known reference-environment dependence

These are recorded findings, not tolerances. Each needs a parent/user decision.

- [Mixed NaNs](../../port/findings/kimath-matrix-mixed-nan-payloads.md): which NaN
  payload survives depends on the compiler's operand order. The qualified Linux run
  mismatches on 1,824 of 3,500 requests.
- [libm](../../port/findings/kimath-matrix-libm-sin-cos.md) and its
  [qualified addendum](../../port/findings/kimath-matrix-libm-sin-cos-qualified-addendum.md):
  `set_rotation` uses the host libm. wasm differs on 24 of 275 angles, and glibc's ifunc
  variant changes `sin`/`cos` by 1 ulp for some angles.
- [Stream](../../port/findings/kimath-matrix-stream-libstdcxx-lwg4084.md): libstdc++ 15
  prints `INF`/`NAN` for `uppercase` + `fixed`, while earlier runtimes print `inf`/`nan`. The
  port follows the pre-15 runtime of the hosted CI gate.
- Platform formatting: glibc prints a NaN's sign (`-nan`, `+nan` with `showpos`) and prints
  subnormal `%a` as `0x0.<hex>p-1022`. Apple's libc does neither, so a macOS smoke oracle is
  not the qualified platform.

## Validation

See `validation/` for the recorded runs: commands, environment, counts and hashes.
