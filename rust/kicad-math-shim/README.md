<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath matrix and wide integer difftest adapter

Native JSONL adapter for unit `libs.kimath.include.math.application.pure.p001`:
`libs/kimath/include/math/matrix3x3.h` and `libs/kimath/include/math/wide_int.h` at pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The portable implementation is
[`rust/kicad-math`](../kicad-math); the C++ counterpart is
[`port/oracle/shims/kimath_math.cpp`](../../port/oracle/shims/kimath_math.cpp), which calls
the genuine pinned header templates (`MATRIX3x3<double>`, `KIGEOM_WIDE::WordsToDouble`,
`ToDouble`, `CrossWide`, `DotWide`) compiled with the oracle's fixed flags
(`-fno-fast-math -ffp-contract=off`). This crate is native (`portable = false`); it parses
requests with the shared `kicad_difftest::request::parse`.

## Value encodings

| Kind | JSON |
| --- | --- |
| binary64 | exactly 16 lowercase hex digits of the IEEE-754 bits, e.g. `"3ff0000000000000"` |
| `VECTOR2D` / `VECTOR3D` | array of 2 / 3 binary64 strings |
| `MATRIX3x3D` | array of 3 rows of 3 binary64 strings (`m_data[row][column]`) |
| `int64_t` | JSON integer in the int64 range (integer `-0` is 0) |
| `uint64_t` | JSON integer in [0, 2^64 - 1] (integer `-0` is 0) |
| `VECTOR2L` | array of 2 int64 integers |
| `KI_INT128` | canonical decimal string: optional `-`, no leading zeros, no `-0` |

Bit strings carry infinities, every NaN sign and payload (including signalling NaNs), signed
zeros and subnormals losslessly in both directions. No JSON number ever holds a float, so
non-finite results need no special case and no numeric token formatting can differ. Any bit
pattern is accepted as input.

## Operations

`input` is `{"op": NAME, ...fields}` with exactly the listed fields. A matrix argument is
built with the nine-element constructor before the operation is called.

| op | fields | result | C++ called |
| --- | --- | --- | --- |
| `matrix_default` | | `matrix` | `MATRIX3x3()` |
| `matrix_from_vectors` | `a1`, `a2`, `a3` | `matrix` | `MATRIX3x3( VECTOR3, VECTOR3, VECTOR3 )` |
| `matrix_from_elements` | `elements` | `matrix` | `MATRIX3x3( a00, ..., a22 )` |
| `set_identity` | `matrix` | `matrix` | `SetIdentity` |
| `set_translation` | `matrix`, `translation` | `matrix` | `SetTranslation` |
| `get_translation` | `matrix` | `vector` | `GetTranslation` |
| `set_rotation` | `matrix`, `angle` | `matrix` | `SetRotation` (libm `cos`, `sin`) |
| `set_scale` | `matrix`, `scale` | `matrix` | `SetScale` |
| `get_scale` | `matrix` | `vector` | `GetScale` |
| `determinant` | `matrix` | `value` | `Determinant` |
| `inverse` | `matrix` | `matrix` | `Inverse` (adjugate, `Determinant`, `operator*( M, S )`) |
| `transpose` | `matrix` | `matrix` | `Transpose` |
| `compare` | `matrix`, `other` | `equal`, `not_equal` | `operator==`, `operator!=` |
| `mul_matrix` | `a`, `b` | `matrix` | `operator*( M, M )` |
| `mul_vector2` | `matrix`, `vector` | `vector` | `operator*( M, VECTOR2 )` |
| `mul_vector3` | `matrix`, `vector` | `vector` | `operator*( M, VECTOR3 )` |
| `mul_scalar` | `matrix`, `scalar` | `matrix` | `operator*( M, S )`, `S = double` |
| `scalar_mul` | `scalar`, `matrix` | `matrix` | `operator*( S, M )`, `S = double` |
| `stream` | `matrix` | `text` | `operator<<( std::ostream&, M )` on a fresh `std::ostringstream` |
| `words_to_double` | `hi`, `lo` | `value` | `KIGEOM_WIDE::WordsToDouble` |
| `to_double` | `value` | `value`, `via_words` | `ToDouble( KI_INT128 )`; MSVC body `WordsToDouble( _Word[1], _Word[0] )` |
| `cross_wide` | `a`, `b` | `value`, `double` | `CrossWide`, then `ToDouble` |
| `dot_wide` | `a`, `b` | `value`, `double` | `DotWide`, then `ToDouble` |

The MSVC `ToDouble` overload cannot be compiled on Linux (`std::_Signed128`); its entire body
is `WordsToDouble` on the high and low words, which the oracle calls with those two words.
`via_words` therefore checks the MSVC branch's arithmetic, not MSVC's type.

`DotWide` with all four components `INT64_MIN` is signed overflow (undefined behaviour) in the
C++; both adapters refuse that single input with `invalid_input`
`DotWide overflows KI_INT128` before any arithmetic, and the Rust library returns
`WideIntError::Overflow`. Every other int64 input is accepted.

Refusal messages, in validation order: `expected op string`, `unknown operation`,
`unexpected or missing field`, then per field in the order listed:
`expected matrix: F`, `expected vector: F`, `expected double bit string: F`,
`integer out of range: F`, `expected int128 string: F`. `test_refusals.py ORACLE RUST`
sends 187 refusal probes (each followed by a valid request) to both programs and requires
identical complete replies.

## Cases

`generate_cases.py` writes the 3,485 public fixed-seed requests in
`port/cases/libs.kimath.include.math.application.pure.p001.jsonl`: boundary values (signed
zeros, subnormals, extremes, infinities, NaNs), singular, rank-deficient, near-singular,
overflowing and underflowing matrices, a difference at each of the nine equality positions,
`%.6g` boundaries and exact decimal ties, `WordsToDouble` halfway and sticky cases for every
leading-zero count with both signs, `ToDouble` ties at every third exponent, int64 extremes and
33-bit coordinate deltas.

NaN selection: copy-only operations (constructors, setters, getters, transpose, equality,
stream) carry arbitrary NaN payloads and signalling NaNs. Arithmetic cases combine NaNs of
one bit pattern only. Wherever an infinity can generate a NaN, the only NaN used is the
x86-64 default NaN `fff8000000000000`. Which payload survives when two different NaNs meet
in one IEEE operation depends on the compiler's operand order, not on the KiCad source, so
such mixtures are not reference behaviour. The adapters still accept them. This is a corpus
selection, not an input restriction.

## Platform limits

- Linux x86-64 is the qualified numerical platform. The NaN generated by an invalid
  operation is `fff8000000000000` on x86-64. It is positive on AArch64 and has an
  unspecified sign in wasm.
- `stream` follows glibc's `printf`: a NaN with its sign bit set prints `-nan`. Apple's libc
  prints `nan`.
- `set_rotation` uses the platform libm. See
  [the libm finding](../../port/findings/kimath-matrix-libm-sin-cos.md): native results
  match the oracle on the same host, wasm results differ for some angles, and glibc's
  ifunc choice can make the reference CPU-dependent.

## Validation

See `validation/` for the recorded runs: commands, environment, counts and hashes.
