<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Audit: libs.core.include.core.application.thin.replacement.p001

Upstream: `libs/core/include/core/map_helpers.h` (122 lines) at pin
`4cf939778df04d2b747bb2c4714fc5014a7565c7`, byte-identical in the checkout (modulo
CRLF). Reference wx: Ubuntu `3.2.4+dfsg-4build1` (`wxUSE_UNICODE_WCHAR`, 32-bit
signed `wchar_t`); the wx sources read are v3.2.4 `string.h`, `string.cpp`,
`strconv.cpp` and `wxcrt.h`.

## Owned functions

| # | C++ overload | Rust | Body and branches | Successful result checked | Gap |
| --- | --- | --- | --- | --- | --- |
| 1 | `get_opt( map<wxString,V>, const wxString& )` | `get_opt_wx` | find; `nullopt` or copy | found/missing, V = int and wxString (`get_opt_wx-*`, `caller-*`) | none |
| 2 | `get_opt( map<wxString,V>, const char* )` | `get_opt_cstr` | `FromUTF8` then #1 | null, empty, invalid, embedded NUL, overlong keys (`cstr-get_opt-*`, `get_opt_cstr-*`) | none |
| 3 | `get_opt<K,V>( map<K,V>, const K& )` | `get_opt` | find; `nullopt` or copy | K, V in {int, wxString} (`get_opt-*`); wx keys use explicit template arguments so #3, not #1, runs | other K, V |
| 4 | `get_def( map, const char*, const char* = "" )` | `get_def_cstr` | key `FromUTF8`, find, copy or `FromUTF8( aDefval )` | null/invalid key and default (`cstr-get_def-*`, `get_def_cstr-*`) | default argument unusable (ambiguous), see below |
| 5 | `get_def( map, const char*, const wxString& = wxString() )` | `get_def_cstr_wx` | key `FromUTF8`, find, copy value or default | `cstr-get_def_wx-*`, `get_def_cstr_wx-*` | default argument unusable |
| 6 | `get_def( map, const wxString&, const wxString& = wxString() )` | `get_def_wx`, `get_def_wx_or_empty` | find, copy value or default | with and without `defval` (`get_def_wx-*`, `caller-ltspice-*`) | none |
| 7 | `get_def<K,V>( map, const K&, const V& = V() )` | `get_def`, `get_def_or_value_init` | find, copy value or default | with and without `defval`, K, V in {int, wxString} (`get_def-*`) | other K, V |

Overload selection in the oracle: calls without defaults go through a function
pointer of the exact overload signature; default-argument calls are written as
callers write them (`get_def( map, key )`, and `get_def<K, V>( map, key )` for wx
keys). The adapter's `static_assert`s prove at compile time that a two-argument
`get_def( std::map<wxString, wxString>, const char* )` is ambiguous (so the #4 and #5
defaults cannot be used) and that both three-argument forms resolve.

Operation order follows the reference: key conversion, then `find`, then the value
copy or the default's conversion/copy. #7's `V()` and #6's `wxString()` are
constructed at the call, before the lookup.

## wxString co-port (`kicad_map_helpers::wx_string`)

- `WxString` holds `wchar_t` units (`i32`); construction from units keeps NULs,
  negative values and surrogates.
- `from_utf8` ports `ConvertStr` (`!psz`, `!wcLen` give empty), `cMB2WC` (sizing pass,
  buffer of count + 1, length excludes the converted terminator) and
  `wxMBConvStrictUTF8::ToWChar` with `srcLen = strlen + 1`: lead-byte table,
  `srcLen < len` truncation, continuation check, no overlong/surrogate/range checks
  (see `port/findings/core-map-helpers-wx-strict-utf8.md`).
- The derived `Ord` equals `std::less<wxString>`: `Cmp() < 0`, `wxDoCmp`, signed
  element-wise `wxTmemcmp`, shorter first on a common prefix. `map_order-*` cases,
  including `INT32_MIN`, `-1` and prefix keys, compare the iteration order.

## Resource and safety contract

- Portable crate: `no_std` + `alloc`, `forbid(unsafe_code)`, no unwrap/expect/panic or
  indexing, no global state; checked for `wasm32-unknown-unknown`.
- Every copy is fallible (`TryClone`): `try_reserve_exact`, `WxError::Alloc` where C++
  throws `std::bad_alloc`. `Copy` types copy infallibly. Caller types implement
  `TryClone` with their copy constructor's semantics.
- `MAX_UNITS = 2^24` is a port-imposed bound checked before allocation
  (`WxError::CapacityExceeded`); the reference would normally succeed past it. This
  is a deliberate refusal outside any realistic key size, not reference behaviour.
- `from_utf8` scans its input twice (as wx does); input length is the caller's.
- Concurrency: the functions take shared references and hold no state; a C++ map
  mutated concurrently is undefined there and impossible here (borrowing).
- Not modelled: the Windows `WC_UTF16` build; `std::map` comparators other than the
  key's `Ord`.

## Evidence

Tested tree: this PR's head (see the PR). Commands (WSL Ubuntu, owned work root
`~/kicad-port-maphelpers`, `ORACLE_JOBS=8`):

- `port/oracle/build.sh` on a `prepare.py` stage of the pin; the oracle's own ctest
  suite (11 tests) passed; kicad-oracle sha256
  `b52c653214bc993834d30f4aef11c29ea2160da915376b6ad848f86707c6c193`.
- `kicad-difftest compare`: 3084 cases, 3084 compared, 0 deferred, 0 failures, 0
  unprocessed (status green); `check` against the blessed baseline: matched.
- `test_core_map_helpers_inputs.py` on the oracle and the Rust shim: 336 refusals,
  each followed by a successful continuation.
- `corpus_parity` in import-free wasm32 (Node 22) against the native replies: all
  3084 equal.
- Rust: fmt, clippy `-D warnings`, unit tests and the corpus/control tests.

Case groups (counts in `expected-replies.json`): `from_utf8`, `caller-*`
(string_utils, export_step, LTspice, EasyEDA Pro and literal `const char*` keys),
`cstr-*`, `map_order`, `get_opt*`, `get_def*`.
