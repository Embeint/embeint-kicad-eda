<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Owned-definition audit: libs.kimath.include.application.pure.p001

Pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`. The record owns 34 `functions`
entries in seven headers. `convert_basic_shapes_to_polygon.h` declares only
out-of-line functions owned elsewhere and the `RECT_CHAMFER_POSITIONS` enum; it
contributes no definition. All 34 entries are implemented, including the five
`bezier_curves.h` entries (end of the table). No `Unported` deferral remains: every
`VECTOR2` operation the Bezier bodies call is co-ported in `kicad_math::vector`.
The open comparison questions are listed at the end.

| USR | Upstream | Rust | Branches and defaults | Cases |
| --- | --- | --- | --- | --- |
| `c:@F@hash_combine#&l#`, `c:hash.h@F@hash_combine#&l#` | `hash.h:28` base case (two USRs for the same `static` header function) | `hash::hash_combine` with no values | no-op | `hash-combine-0-*`, `hash-val-0` |
| `c:@FT@>2#T#pThash_combine#&l#&1t0.0#P&1t0.1#v#`, `c:hash.h@FT@…` | `hash.h:41` | `hash::hash_combine` | left-to-right fold; wrapping `size_t` arithmetic; `std::hash<T>` for bool, all 8 integer widths, float, double, `std::string` | `hash-combine-1-*`, `-2-*` (all 144 type pairs), `-3-*` (125), `hash-combine-each-*` |
| `c:@FT@>1#pThash_val#P&1t0.0#l#`, `c:hash.h@FT@…` | `hash.h:48` | `hash::hash_val` | seed `0xa82de1c0` | `hash-val-*` arities 0..3 |
| `c:@F@mmh3_rotl64#l#r#` | `mmh3_hash.h:48` (non-MSVC branch) | `mmh3_hash::mmh3_rotl64` | defined domain 1..63; other counts are UB and refused | `rotl-*` (all 63 counts) |
| `c:@S@HASH_128@F@Clear#` | `hash_128.h:34` | `Hash128::clear` | — | `hash128-*` (`cleared`) |
| `c:@S@HASH_128@F@operator==#&1$@S@HASH_128#1` | `hash_128.h:39` | `Hash128::equals`, `PartialEq` | 16-byte `memcmp` | `hash128-differ-byte-*`, `-equal-*` |
| `c:@S@HASH_128@F@ToString#1` | `hash_128.h:44` | `Hash128::to_hex_string` (classic), `Hash128::to_string_in` (caller `numpunct<char>`) | upper-case, zero-filled width 16 per word, `Value64[0]` first; global-locale grouping and separator (libstdc++ `_M_use_grouping`, `__add_grouping`), padding after grouping; any grouping length | all `hash128-*`, every `digest`; `hash128-locale-*` (32, groupings up to 4096 bytes) |
| `c:@S@MD5_HASH@F@IsValid#1`, `c:@S@MD5_HASH@F@SetValid#b#` | `md5_hash.h:24,26` | reused `kicad_md5::Md5Hash::{is_valid,set_valid}` | — | `md5-valid-*` |
| `c:@S@MMH3_HASH@F@MMH3_HASH#`, `…#i#` | `mmh3_hash.h:63,65` | `Mmh3Hash::new`, `with_seed` | both call `reset`; `blocks` left unwritten (reported `None`/`??`, never valued) | `mmh3-empty-*`, every first `new` event |
| `c:@S@MMH3_HASH@F@reset#i#` | `mmh3_hash.h:67` | `Mmh3Hash::reset`, `reset_default` | default argument 0; `blocks` untouched | `mmh3-reset-*`, random sequences |
| `c:@S@MMH3_HASH@F@addData#*1c#l#` | `mmh3_hash.h:74` | `Mmh3Hash::add_data` | pending fill with early return; whole blocks; zero-padded tail; wrapping `len`; refused when completing the pending block would hash unwritten bytes | `mmh3-data-length-0..49`, `stream-split-*`, `pending-*`, `len-wrap-*`, `large-*` |
| `c:@S@MMH3_HASH@F@addDataV1#*1c#l#` | `mmh3_hash.h:116` | `Mmh3Hash::add_data_v1` | ignores pending; padding 1..4 counted in `len` | `mmh3-data-v1-*`, `large-v1-*` |
| `c:@S@MMH3_HASH@F@add#&1$@N@std@N@__cxx11@S@basic_string…`, `add#&1$@N@std@S@vector>#C…` | `mmh3_hash.h:138,143` | `add_string`, `add_chars` | — | `mmh3-string-*`, `mmh3-chars-*` |
| `c:@S@MMH3_HASH@F@add#I#` | `mmh3_hash.h:148` | `Mmh3Hash::add_i32` | slot `( len % 16 ) / 4`; hash only at multiples of 16; refused when that hash would read unwritten bytes | `mmh3-ints-*`, `int-after-*` |
| `c:@S@MMH3_HASH@F@digest#` | `mmh3_hash.h:157` | `Mmh3Hash::digest` | state keeps tail/final changes | `digest-twice`, `digest-then-continue` |
| `c:@S@MMH3_HASH@F@getblock64#I#` | `mmh3_hash.h:171` | `Mmh3Hash::get_block64` | index 0..1 only; refused when the word has unwritten bytes | `private-helpers-*` |
| `c:@S@MMH3_HASH@F@hashBlock#` | `mmh3_hash.h:176` | `Mmh3Hash::hash_block` | refused unless all 16 bytes are written | every block, `private-helpers-*` |
| `c:@S@MMH3_HASH@F@hashTail#` | `mmh3_hash.h:193` | `Mmh3Hash::hash_tail` | all 16 `switch` entries; refused when bytes `0 .. len & 15` include unwritten ones | `hash-tail-len-0..15` |
| `c:@S@MMH3_HASH@F@fmix64#l#S` | `mmh3_hash.h:228` | `mmh3_hash::fmix64` | — | `fmix64-*` |
| `c:@S@MMH3_HASH@F@hashFinal#&$@S@HASH_128#` | `mmh3_hash.h:239` | `Mmh3Hash::hash_final` | — | `private-helpers-*`, every `digest` |
| `c:@S@TRANSFORM@F@TRANSFORM#`, `…#I#I#I#I#` | `transform.h:52,59` | `Transform::default`, `new` | identity defaults 1,0,0,1 | `transform-*` |
| `c:@S@TRANSFORM@F@operator!=#&1$@S@TRANSFORM#1` | `transform.h:68` | `PartialEq::ne` (`!eq`) | — | `transform-*` (`not_equal`, `reverse_not_equal`, `self_not_equal`) |
| `c:@ST>1#T@BEZIER@F@BEZIER#&1>@ST>1#T@VECTOR21t0.0#S0_#S0_#S0_#` | `bezier_curves.h:95-99` | `Bezier::new` (`const fn`) | stores the four points unchanged; both instantiations (`int`, `double`) | `bezier-*-new` (25: every special `double` bit pattern, `INT_MIN`/`INT_MAX`), `curve` of every `bezier-*` case |
| `c:@ST>1#T@BEZIER@F@PointAt#d#1` | `:110-120` | `Bezier::point_at` | the five scalar products in order; `t_m1_3 * Start + 3.0 * aT * t_m1_2 * C1 + 3.0 * t2 * t_m1 * C2 + t3 * End` left to right in `double`; `aT` outside `[0, 1]` evaluated as given; for `int` the sum is stored through the clamping, truncating `VECTOR2<int>` constructor (saturates at `INT_MIN`/`INT_MAX`, NaN refused) | `bezier-*-point-at-*` (436: 23 special `t` and 4 NaN `t` (2 for `int`) per fixed curve, overflow to infinity, saturation, random) |
| `c:@ST>1#T@BEZIER@F@Split#d#&>@ST>1#T@BEZIER1t0.0#S0_#1` | `:129-140` | `Bezier::split` returning `( aLeft, aRight )` | the six de Casteljau statements in order, each `a + aT * ( b - a )` stored as `VECTOR2<T>` before use; for `int` an overflowing difference or a NaN intermediate is refused before any output | `bezier-*-split-*` (346) |
| `c:@ST>1#T@BEZIER@F@SubCurve#d#d#1` | `:145-191` | `Bezier::sub_curve` | both clamps (NaN and `-0.0` pass); `t1 <= t0` degenerate curve at `PointAt( t0 )`; `t0 == 0 && t1 == 1` returns the curve; `t0 == 0` left half of `Split( t1 )`; `t1 == 1` right half of `Split( t0 )`; otherwise `Split( t1 )` then the left half split at `t0 / t1` | `bezier-*-sub-curve-*` (473: 32 ranges per fixed curve covering every branch, clamp and NaN path, random) |
| `c:@S@BEZIER_POLY@F@BEZIER_POLY#&1$@N@std@S@vector>#$@S@VECTOR2>#d#$@N@std@S@allocator>#S2_#` | `:41-45` | `BezierPoly::from_vector2d` | copies every point (any count, bits unchanged) into `m_ctrlPts`; `m_minSegLen = 0.0` | `bezier-poly-*` (13: empty, 1, 3, 4, 5, special bits, 4096 points, random) |

Co-ports for the Bezier bodies (parent-authorized; owner
`libs.kimath.include.math.application.ui.p002` keeps status `todo`, its claim and
tests; only its `notes` list them; details in
[`rust/kicad-math/TYPE_SUPPORT.md`](../kicad-math/TYPE_SUPPORT.md)):
`operator-( VECTOR2<T>, VECTOR2<U> )` at `<int, int>` (checked) and `<double, double>`;
`operator*( const T&, VECTOR2<U> )` at `<double, int>` and `<double, double>`;
`operator+( VECTOR2<T>, VECTOR2<U> )` at `<int, double>` and `<double, double>`; and the
floating-source branch of the casting constructor, `VECTOR2<int>( VECTOR2<double> )`.
Overload resolution against the pinned header picks exactly these templates. The
defaulted `BEZIER()` (no owned USR) is `Default`, the implicit copy and assignment are
`Copy`, and `VECTOR2` default/copy are the existing PR #54 representation.

Both instantiations are exercised because both exist in KiCad: `BEZIER<double>` in
`EDA_SHAPE` (`PointAt`, `SubCurve`, hence `Split`) and `BEZIER<int>` in
`TransformEllipseToBeziers` (Altium import). For `int`, `PointAt`, `Split` and
`SubCurve` are not called in KiCad at the pin, but they are defined template bodies of
this unit and are compared as written.

Co-port (parent-agreed minimum, owner unchanged): `TRANSFORM::operator==`
(`c:@S@TRANSFORM@F@operator==#&1$@S@TRANSFORM#1`, `libs/kimath/src/transform.cpp:31-37`,
unit `libs.kimath.src.transform.application.ui.p001`) as `PartialEq::eq` in
`rust/kicad-hash/src/transform.rs`. That unit's `TransformCoordinate`,
`InverseTransform`, `MapAngles` and `std::hash<TRANSFORM>` are not ported here.

The `BEZIER_POLY` type itself, its two `VECTOR2I` constructors, `GetPoly`, its
private helpers and `TransformEllipseToBeziers` belong to
`libs.kimath.src.bezier_curves.application.ui.p001`, which implements them on these types
in `kicad_hash::bezier_curves` (the two fields are `pub(crate)` for it). No call from an
owned body reaches them, so no deferral is needed.

## Refusals (undefined behaviour in the reference)

Refused before the reference runs, identically in both programs and never corpus
cases (`check_guards.py`): MMH3 reads of never-written block bytes; `mmh3_rotl64`
counts outside 1..63; `getblock64` indexes outside 0..1; `BEZIER<int>` evaluations
whose `int` subtraction overflows or that convert a NaN to `int` (325 generated
requests in `fixtures/bezier-int-undefined.*`).

## Reference-build properties (no deferrals; the unit stays `partial` until a live run)

- Two distinct NaN payloads meeting in one `BEZIER<double>` operation: the port
  follows the bound reference build's operand order per statement
  (`kicad_hash::bezier_f64_sites`), derived from the disassembly of the candidate
  oracle object and checked by `check_reference_bindings.py`
  ([record](../../port/findings/kimath-header-hash-bezier-two-nan-payloads.md)).
- A zero byte in a `ToString` grouping: the qualified libstdc++ runtimes apply the
  caller's bytes up to the first NUL byte (COW-ABI twin shim); the port does the
  same and keeps the caller's bytes unchanged
  ([record](../../port/findings/kimath-header-hash-tostring-locale.md)).

Both reproduce all 41 previously failing replies of the retained genuine oracles
offline. The live hosted run and a reviewed corrected golden are still to come.
