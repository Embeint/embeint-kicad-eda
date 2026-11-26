<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-map-helpers-shim

Native JSONL adapter for unit
`libs.core.include.core.application.thin.replacement.p001`
(`libs/core/include/core/map_helpers.h`), the Rust side of the exact difftest. The
C++ side is `port/oracle/shims/core_map_helpers.cpp`, which instantiates the pinned
header against the distribution wxWidgets 3.2.4 (wchar_t build). Both adapters use
the schema below with the same validation order and refusal messages.

## Values

| Field type | JSON | C++ | Rust |
| --- | --- | --- | --- |
| `int` | integer token in `[-2^31, 2^31 - 1]` | `int` | `i32` |
| wide string | array (at most 4096) of integer `wchar_t` units in the same range | `wxString( const wchar_t*, size_t )` | `WxString::from_units` |
| bytes | `null` (null pointer) or array (at most 4096) of integers 0..255 | `const char*` to the bytes plus a terminator | `Option<&CStr>` up to the first NUL |
| map | array (at most 256) of `[key, value]` pairs | `std::map::emplace` in order | `BTreeMap` entry, first kept |

Results report wide strings as arrays of units and optionals as
`{"found": bool, "value": v | null}`.

## Operations

| `op` | Fields (optional) | Upstream call |
| --- | --- | --- |
| `from_utf8` | `text` (bytes) | `wxString::FromUTF8( const char* )` (co-port check) |
| `map_order` | `key_type`, `value_type`, `map` | iteration order of `std::map<K, V>` (co-port `Ord` check) |
| `get_opt_wx` | `value_type`, `map`, `key` (wide) | `get_opt( const std::map<wxString, V>&, const wxString& )` |
| `get_opt_cstr` | `value_type`, `map`, `key` (bytes) | `get_opt( const std::map<wxString, V>&, const char* )` |
| `get_opt` | `key_type`, `value_type`, `map`, `key` | `get_opt<K, V>( const std::map<K, V>&, const K& )` |
| `get_def_cstr` | `map`, `key` (bytes), `defval` (bytes) | `get_def( map, const char*, const char* )` |
| `get_def_cstr_wx` | `map`, `key` (bytes), `defval` (wide) | `get_def( map, const char*, const wxString& )` |
| `get_def_wx` | `map`, `key` (wide), (`defval`) | `get_def( map, const wxString&, const wxString& = wxString() )` |
| `get_def` | `key_type`, `value_type`, `map`, `key`, (`defval`) | `get_def<K, V>( map, const K&, const V& = V() )` |

`key_type` and `value_type` are `"int"` or `"wx"`. The `get_def_*` string operations
use `std::map<wxString, wxString>`. Without `defval` the call uses the default
argument. Validation order: `op` (a string, then a known name), the field set,
`key_type`, `value_type`, `map`, `key`, `defval`.

Refusals are `invalid_input` with one of: `expected string: op`, `unknown op`,
`unexpected or missing field`, `expected type: key_type`, `expected type: value_type`,
`expected map`, `expected map entry`, `expected int`, `expected wide string`,
`expected bytes`. A refused allocation (unreachable within these bounds) is an
`execution_error`.

## Corpus

`generate_cases.py` writes the public corpus, the refusal controls
(`port/oracle/fixtures/core-map-helpers-controls.jsonl`) and `expected-replies.json`
from caller keys, decoder and ordering boundaries and a fixed-seed stream. It never
computes a reply; `--check` verifies the committed files. See `AUDIT.md`.
