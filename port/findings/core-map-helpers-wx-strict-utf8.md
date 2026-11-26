<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# wxString::FromUTF8 accepts overlong, surrogate and beyond-Unicode UTF-8

- **Unit:** `libs.core.include.core.application.thin.replacement.p001`
  (`libs/core/include/core/map_helpers.h`, the `const char*` overloads).
- **Location:** wxWidgets `src/common/strconv.cpp` (3.2.4 source audit;
  initial live binary linked 3.2.9, see the reference addendum),
  `wxMBConvStrictUTF8::ToWChar`, reached through `wxString::FromUTF8( const char* )`
  = `wxString( utf8, wxMBConvUTF8(), npos )` on the wchar_t build.
- **Status:** confirmed against the local wx 3.2.9 oracle; target 3.2.4 live
  qualification is pending. Ported as observed; no KiCad defect is claimed.

## Behaviour

The "strict" decoder validates only the lead byte (`tableUtf8Lengths`: rejects
80..C1 and F5..FF) and the `10xxxxxx` pattern of the continuation bytes. It does
not reject:

| Input | Result on the local wx 3.2.9 build |
| --- | --- |
| `E0 80 80` (overlong NUL) | one unit `0` (an embedded NUL in the key) |
| `ED A0 80` (UTF-16 surrogate) | one unit `0xD800` |
| `F4 BF BF BF` | one unit `0x13FFFF` (beyond U+10FFFF) |

Any rejected input, a truncated sequence, a null pointer and the empty string all
give the empty `wxString`. A `get_opt( map, const char* )` or `get_def( map, const
char*, ... )` call with malformed UTF-8 therefore finds the map's empty-string key
if there is one (cases `cstr-get_opt-*`, `cstr-get_def-*`).

## Consequence for the port

Rust `str::from_utf8` or `String` substitutions would reject the first three rows
and could not represent the surrogate unit; they would not match the reference.
`kicad_map_helpers::wx_string::WxString::from_utf8` ports the decoder pass by pass
over `wchar_t` units. The Windows `WC_UTF16` build behaves differently (surrogate
pairs; codes above U+10FFFF and lone surrogates take a different path) and is not
modelled.

## Evidence

`from_utf8-*` cases of `port/cases/libs.core.include.core.application.thin.replacement.p001.jsonl`
(every lead byte, each multi-byte boundary, truncations and broken continuations),
compared exactly with the live oracle (kicad-oracle sha256
`b52c653214bc993834d30f4aef11c29ea2160da915376b6ad848f86707c6c193`).
