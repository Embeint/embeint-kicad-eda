<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kimath header hash/transform difftest adapter

Native JSONL adapter (`portable = false`) for unit
`libs.kimath.include.application.pure.p001`, pinned KiCad
`4cf939778df04d2b747bb2c4714fc5014a7565c7`. The portable implementation is
[`rust/kicad-hash`](../kicad-hash); the C++ oracle adapter is
[`port/oracle/shims/kimath_header_hash.cpp`](../../port/oracle/shims/kimath_header_hash.cpp).
Both parse requests through the shared hardened transport
(`kicad_difftest::request::parse`, `port/oracle/protocol.cpp`) and apply the same
validation in the same order with the same messages.

```sh
python3 rust/kicad-hash-shim/generate_cases.py         # writes port/cases/<unit>.jsonl
python3 rust/kicad-hash-shim/check_guards.py ORACLE RUST_SHIM
rust/target/debug/kicad-difftest check --unit libs.kimath.include.application.pure.p001 \
  --oracle ORACLE --rust rust/target/debug/kicad-hash-shim
```

## Operations

Every request's `input` has an `op` string. Integers are JSON integers (floats,
strings and booleans are refused); `-0` is the integer zero. Hex fields are
lower-case, even length. Unknown or missing keys are refused.

| `op` | Input | Calls | Result |
| --- | --- | --- | --- |
| `std_hash` | `values`: up to 64 typed values | `std::hash<T>()( v )` per value | `hashes` (u64) |
| `hash_combine` | `seed` (u64), `values`: 0..3 | `hash_combine( seed, args... )` with exactly that arity | `seed` |
| `hash_val` | `values`: 0..3 | `hash_val( args... )` | `hash` |
| `hash_combine_each` | `seed`, `values`: up to 256 | one `hash_combine( seed, v )` per value | `seeds` after each call |
| `hash128` | `a`, `b`: 16 bytes; optional `locale` | `HASH_128`, `operator==`, `ToString`, `Clear`; with `locale`, `ToString` again under that global C++ locale | views of new/`a`/`b`, equalities, cleared view; `grouped` (hex bytes of the locale `ToString` for new/`a`/`b`/cleared) |
| `mmh3` | `steps`: 1..4096, first `new` | `MMH3_HASH` steps below | one state event per step |
| `rotl` | `x` (u64), `r` (1..63) | `mmh3_rotl64( x, r )` | `value` |
| `fmix64` | `k` (u64) | private static `MMH3_HASH::fmix64` | `value` |
| `md5_valid` | `steps`: 1..4096, first `new` | `MD5_HASH` validity steps below | `valid`, `Format( true )` per step |
| `transform` | `a`, `b`: `null` or 4 int32 | `TRANSFORM()` / `TRANSFORM( x1, y1, x2, y2 )`, `operator==` (co-port), `operator!=` | members, `equal`, `not_equal`, `reverse_not_equal`, `self_not_equal` |

Typed values: `{"type":T,"value":n}` for `i8 u8 i16 u16 i32 u32 i64 u64`
(the type's full range), `{"type":"bool","value":b}`, `{"type":"f32"|"f64","bits":hex}`
(exactly 4 or 8 bytes, most significant first, any bit pattern including NaN
payloads and signed zero) and `{"type":"string","hex":bytes}` (`std::string`,
up to 4096 bytes, not necessarily UTF-8). Three-argument calls accept only
`bool`, `i32`, `u64`, `f64` and `string` (the instantiated subset); one- and
two-argument calls accept every type pair.

`mmh3` steps: `new` (`MMH3_HASH()` or, with `seed`, `MMH3_HASH( uint32_t )`),
`reset` (default argument or `seed`), `data`/`data_v1`/`string`/`chars`
(`addData`, `addDataV1`, `add( std::string )`, `add( std::vector<char> )`; `hex`
up to 65536 bytes, optional `repeat` 0..4096, 16 MiB per case), `int`
(`add( int32_t )`), `digest`, `state` (sets private `h1`, `h2`, `blocks`, `len`),
`hash_block`, `hash_tail`, `hash_final` and `getblock64` (`index` 0..1) on the
private helpers. Each event reports `op`, `h1`, `h2`, `len`, `blocks` and, for
`digest`/`hash_final`/`getblock64`, the `HASH_128` view or block word. `blocks`
is 16 bytes of lower-case hex in which a byte the reference has never written is
`??`: the constructors leave `blocks` uninitialized, and neither program assigns
or reads a value for such a byte. A step whose reference member would read one
(`hashBlock` via `addData`/`add( int32_t )`/`hash_block`, `hashTail` via
`digest`/`hash_tail`, or `getblock64`) is refused before it runs with
`<reader> reads unwritten block bytes <list>`; the case is then a refusal, never
a corpus case.

`hash128`'s `locale` is `{"thousands_sep": 0..255, "grouping": hex}` (at most 64
grouping bytes; longer is refused with `grouping of N bytes exceeds 64`). The
oracle installs a global locale whose `numpunct<char>` returns those values
(every other facet classic) around each reference `ToString` call and restores
the previous global locale.

`md5_valid` steps: `new`, `set_valid` (`value`), `init`, `finalize`, `copy`
(copy constructor), `assign` (`operator=` into a fresh object), `bytes` (`hex`
up to 4096 bytes, 1 MiB per case), `int` and `inspect`.

## Contracts and limits

- 64-bit Linux x86-64 with libstdc++ is the qualified platform: `size_t` is
  `u64`, byte order little-endian, and `std::hash` follows libstdc++
  (`_Hash_bytes`, seed `0xc70f6907`). The Rust crate uses fixed-width integers,
  so its results do not depend on `usize` or the host (including wasm32).
- `MMH3_HASH` leaves `blocks` uninitialized. The oracle adapter does not
  initialize or read it; it tracks the bytes the pinned members write (derived
  from the source) and refuses reads of unwritten bytes, as the port does
  ([upstream issue](../../port/upstream-issues/mmh3-indeterminate-block-read.md),
  [reclassification](../../port/findings/kimath-header-hash-mmh3-unwritten-reads.md)).
- `HASH_128::ToString` depends on the global C++ locale's grouping; the port
  takes it explicitly ([locale record](../../port/findings/kimath-header-hash-tostring-locale.md)).
- `HASH_128` is filled through `Value64` and its `Value32` view is read from
  the object bytes; `ToString` reads `Value64`, as KiCad does.
- Malformed and undefined-domain requests are refusals
  (`invalid_input`/`invalid_request`), never corpus cases: `check_guards.py`
  requires the expected complete reply (parsed JSON) from both programs for 97
  requests, including transport overflow, nesting, duplicate keys, locale
  limits and the 15 original unwritten-read requests in `fixtures/`.

See [AUDIT.md](AUDIT.md) for the owned-definition audit and remaining work.
