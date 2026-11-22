<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Owned-definition audit: libs.kimath.include.application.pure.p001

Pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`. The record owns 34 `functions`
entries in seven headers. `convert_basic_shapes_to_polygon.h` declares only
out-of-line functions owned elsewhere and the `RECT_CHAMFER_POSITIONS` enum; it
contributes no definition. 29 entries are implemented; the five `bezier_curves.h`
entries remain (see the end).

| USR | Upstream | Rust | Branches and defaults | Cases |
| --- | --- | --- | --- | --- |
| `c:@F@hash_combine#&l#`, `c:hash.h@F@hash_combine#&l#` | `hash.h:28` base case (two USRs for the same `static` header function) | `hash::hash_combine` with no values | no-op | `hash-combine-0-*`, `hash-val-0` |
| `c:@FT@>2#T#pThash_combine#&l#&1t0.0#P&1t0.1#v#`, `c:hash.h@FT@…` | `hash.h:41` | `hash::hash_combine` | left-to-right fold; wrapping `size_t` arithmetic; `std::hash<T>` for bool, all 8 integer widths, float, double, `std::string` | `hash-combine-1-*`, `-2-*` (all 144 type pairs), `-3-*` (125), `hash-combine-each-*` |
| `c:@FT@>1#pThash_val#P&1t0.0#l#`, `c:hash.h@FT@…` | `hash.h:48` | `hash::hash_val` | seed `0xa82de1c0` | `hash-val-*` arities 0..3 |
| `c:@F@mmh3_rotl64#l#r#` | `mmh3_hash.h:48` (non-MSVC branch) | `mmh3_hash::mmh3_rotl64` | defined domain 1..63; other counts are UB and refused | `rotl-*` (all 63 counts) |
| `c:@S@HASH_128@F@Clear#` | `hash_128.h:34` | `Hash128::clear` | — | `hash128-*` (`cleared`) |
| `c:@S@HASH_128@F@operator==#&1$@S@HASH_128#1` | `hash_128.h:39` | `Hash128::equals`, `PartialEq` | 16-byte `memcmp` | `hash128-differ-byte-*`, `-equal-*` |
| `c:@S@HASH_128@F@ToString#1` | `hash_128.h:44` | `Hash128::to_hex_string` | upper-case, zero-filled width 16 per word, `Value64[0]` first | all `hash128-*`, every `digest` |
| `c:@S@MD5_HASH@F@IsValid#1`, `c:@S@MD5_HASH@F@SetValid#b#` | `md5_hash.h:24,26` | reused `kicad_md5::Md5Hash::{is_valid,set_valid}` | — | `md5-valid-*` |
| `c:@S@MMH3_HASH@F@MMH3_HASH#`, `…#i#` | `mmh3_hash.h:63,65` | `Mmh3Hash::new`, `with_seed` | both call `reset` | `mmh3-empty-*` |
| `c:@S@MMH3_HASH@F@reset#i#` | `mmh3_hash.h:67` | `Mmh3Hash::reset`, `reset_default` | default argument 0; `blocks` untouched | `mmh3-reset-*`, random sequences |
| `c:@S@MMH3_HASH@F@addData#*1c#l#` | `mmh3_hash.h:74` | `Mmh3Hash::add_data` | pending fill with early return; whole blocks; zero-padded tail; wrapping `len` | `mmh3-data-length-0..49`, `stream-split-*`, `pending-*`, `len-wrap-*`, `large-*` |
| `c:@S@MMH3_HASH@F@addDataV1#*1c#l#` | `mmh3_hash.h:116` | `Mmh3Hash::add_data_v1` | ignores pending; padding 1..4 counted in `len` | `mmh3-data-v1-*`, `large-v1-*` |
| `c:@S@MMH3_HASH@F@add#&1$@N@std@N@__cxx11@S@basic_string…`, `add#&1$@N@std@S@vector>#C…` | `mmh3_hash.h:138,143` | `add_string`, `add_chars` | — | `mmh3-string-*`, `mmh3-chars-*` |
| `c:@S@MMH3_HASH@F@add#I#` | `mmh3_hash.h:148` | `Mmh3Hash::add_i32` | slot `( len % 16 ) / 4`; hash only at multiples of 16 | `mmh3-ints-*`, `int-after-*` |
| `c:@S@MMH3_HASH@F@digest#` | `mmh3_hash.h:157` | `Mmh3Hash::digest` | state keeps tail/final changes | `digest-twice`, `digest-then-continue` |
| `c:@S@MMH3_HASH@F@getblock64#I#` | `mmh3_hash.h:171` | `Mmh3Hash::get_block64` | index 0..1 only | `private-helpers-*` |
| `c:@S@MMH3_HASH@F@hashBlock#` | `mmh3_hash.h:176` | `Mmh3Hash::hash_block` | — | every block, `private-helpers-*` |
| `c:@S@MMH3_HASH@F@hashTail#` | `mmh3_hash.h:193` | `Mmh3Hash::hash_tail` | all 16 `switch` entries | `hash-tail-len-0..15` |
| `c:@S@MMH3_HASH@F@fmix64#l#S` | `mmh3_hash.h:228` | `mmh3_hash::fmix64` | — | `fmix64-*` |
| `c:@S@MMH3_HASH@F@hashFinal#&$@S@HASH_128#` | `mmh3_hash.h:239` | `Mmh3Hash::hash_final` | — | `private-helpers-*`, every `digest` |
| `c:@S@TRANSFORM@F@TRANSFORM#`, `…#I#I#I#I#` | `transform.h:52,59` | `Transform::default`, `new` | identity defaults 1,0,0,1 | `transform-*` |
| `c:@S@TRANSFORM@F@operator!=#&1$@S@TRANSFORM#1` | `transform.h:68` | `PartialEq::ne` (`!eq`) | — | `transform-*` (`not_equal`, `reverse_not_equal`, `self_not_equal`) |

Co-port (parent-agreed minimum, owner unchanged): `TRANSFORM::operator==`
(`c:@S@TRANSFORM@F@operator==#&1$@S@TRANSFORM#1`, `libs/kimath/src/transform.cpp:31-37`,
unit `libs.kimath.src.transform.application.ui.p001`) as `PartialEq::eq` in
`rust/kicad-hash/src/transform.rs`. That unit's `TransformCoordinate`,
`InverseTransform`, `MapAngles` and `std::hash<TRANSFORM>` are not ported here.

## Remaining owned work

Not implemented and not represented by any case (so no deferral is counted):

- `c:@ST>1#T@BEZIER@F@BEZIER#&1>@ST>1#T@VECTOR21t0.0#S0_#S0_#S0_#` (`bezier_curves.h:95`)
- `c:@ST>1#T@BEZIER@F@PointAt#d#1` (`:110`)
- `c:@ST>1#T@BEZIER@F@Split#d#&>@ST>1#T@BEZIER1t0.0#S0_#1` (`:129`)
- `c:@ST>1#T@BEZIER@F@SubCurve#d#d#1` (`:145`, including its clamping, `t1 <= t0`,
  whole-curve, `t0 == 0`, `t1 == 1` and two-split branches)
- `c:@S@BEZIER_POLY@F@BEZIER_POLY#&1$@N@std@S@vector>#$@S@VECTOR2>#d#…` (`:41`)

They need the canonical `Vector2` type from `kicad-math` (PR #50, not yet on
`origin/rust`). `PointAt`/`Split`/`SubCurve` also call `VECTOR2` arithmetic owned
by `libs.kimath.include.math.application.ui.p002` (`operator-`, `operator+`,
scalar `operator*` with `std::common_type`, and the `VECTOR2<int>( VECTOR2<double> )`
casting constructor); their ownership is being agreed with the math worker before
any co-port, otherwise those calls return `Err(Unported { .., unit:
"libs.kimath.include.math.application.ui.p002" })` and the continuations are kept.
`BEZIER_POLY`'s class is otherwise owned by `libs.kimath.src.bezier_curves.application.ui.p001`.
