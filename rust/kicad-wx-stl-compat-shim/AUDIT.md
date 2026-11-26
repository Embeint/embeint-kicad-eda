<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Audit: libs.core.application.thin.replacement.p001

Upstream: `libs/core/wx_stl_compat.cpp` (59 lines) and its header
`libs/core/include/core/wx_stl_compat.h` at pin `4cf939778df04d2b747bb2c4714fc5014a7565c7`.
The source is already compiled into the pinned `core` library the oracle links
(`reference/core/CMakeFiles/core.dir/wx_stl_compat.cpp.o`); no extension of `pin.py` is
needed. Dependency sources read for the audit (downloaded, not vendored): wxWidgets v3.2.4
and v3.2.9 `string.h`, `string.cpp`, `strconv.h`, `strconv.cpp`, `wxcrt.cpp`, `buffer.h`;
glibc 2.39 and 2.43 `iconv/gconv_simple.c`, `iconv/loop.c`, `iconv/skeleton.c`,
`wcsmbs/mbsrtowcs_l.c`, `wcsmbs/wcsmbsload.c`, `locale/C-ctype.c`; libstdc++ GCC 13.3.0,
14.2.0, 14.3.0, `releases/gcc-14`, 15.2.0 and master `ostream`/`ostream.h`, `ostream.tcc`,
`ostream_insert.h`, `locale_facets.tcc`, `streambuf.tcc`, `streambuf_iterator.h`,
`basic_ios.h`, `basic_ios.tcc`. The findings are recorded in
`port/findings/wx-stl-compat-converter-and-stream-audit.md`.

## Owned functions

| # | C++ body | Rust | Body and branches | Successful result checked | Gap |
| --- | --- | --- | --- | --- | --- |
| 1 | `std::less<wxPoint>::operator()` (lines 31-39) | `less::less_wx_point`, `less::WxPointKey` | equal `x`: `aA.y < aB.y`; else `aA.x < aB.x` | 25x25 boundary pairs and 48 random (`less-*`); `std::map<wxPoint,int>` order and first-wins `emplace` (`map-*`) | none |
| 2 | `TowxString( const std::string_view& )` (42-45) | `to_wx_string::to_wx_string` | `wxString( data, length )`: `ConvertStr` null/empty, `cMB2WC` sizing and conversion, base `wxMBConv::ToWChar` chunking by NUL with every inner and trailing NUL kept, `wxMB2WC`/`mbsrtowcs`, failure to "" | null and empty views, all 256 single bytes and `a`+byte pairs, boundary/overlong/surrogate/truncated/NUL-split UTF-8, random bytes and random 1-6 byte sequences, 4096-byte views, each in `C` and `C.UTF-8` (`str-*`) | other locales (not modelled; the locale is an explicit input) |
| 3 | `operator<<( std::ostream&, const wxSize& )` (48-52) | `ostream::insert_wx_size` | five insertions, each with its sentry; literal padding, int formatting, short writes, exception propagation, unitbuf sync | `size-*`: values, flags, widths, fills, adjustment, precision, groupings, states and masks, every capacity cut, unitbuf/sync | none |
| 4 | `operator<<( std::ostream&, const wxPoint& )` (55-59) | `ostream::insert_wx_point` | as #3 with `x`/`y` | `point-*`, the same families | none |
| - | `std::hash<wxString>::operator()` under `USE_KICAD_WXSTRING_HASH` (24-29) | not ported | not compiled: the macro is defined nowhere at the pin, and wxWidgets 3.2 defines `std::hash<wxString>` itself (`wx/string.h`, `wxUSE_STD_STRING`) | - | wxWidgets-owned; no KiCad body exists in the build |

`USE_KICAD_WXPOINT_LESS_AND_HASH` is defined unconditionally by the header, so #1 is
compiled; the header's `std::hash<wxPoint>::operator()` is `= delete`.

The adapter calls #2 through a `wxString (*)( const std::string_view& )` pointer and #3/#4
through `std::ostream& (*)( std::ostream&, const T& )` pointers to the declarations in the
header, and #1 through `std::less<wxPoint>{}`: all resolve to the out-of-line definitions in
`libcore.a`.

## TowxString contract

- The narrow constructor uses `wxConvLibc` (`wxSTRING_DEFAULT_CONV_ARG`), a
  `wxMBConvLibc`, not the strict UTF-8 converter of `FromUTF8`. The accepted
  `kicad_map_helpers::wx_string::WxString::from_utf8` model therefore does not apply and is
  not reused; only `WxString` construction from units is.
- With an explicit length, `wxMBConv::ToWChar` appends a terminator when the last byte is
  not NUL, converts each NUL-terminated chunk with `mbsrtowcs` and counts every NUL inside
  the input, the last byte included. `cMB2WC` does not subtract the terminator for an
  explicit length. So `"ab\0"` gives three units and `"\0"` one; a failing chunk anywhere
  gives "".
- The C library's `LC_CTYPE` is an explicit `LibcCtype` input, never a global. `C` is
  glibc's ASCII charset (bytes above 0x7F illegal); `Utf8` is glibc's UTF-8 decoder (2-6
  byte forms up to 0x7FFFFFFF; overlong 3-6 byte forms, surrogates, 0x80-0xC1/0xFE/0xFF
  leads and truncated sequences illegal). The decoders are identical in glibc 2.39 and 2.43
  apart from error bookkeeping that does not change `mbsrtowcs`' result.
- Results are compared as signed 32-bit `wchar_t` units. `MAX_UNITS` (2^24) remains the
  port's allocation bound; the corpus's 4096-byte views are far below it.

## Stream contract

- `OStream` models a `basic_ostream<char>` with no `tie()`, a buffer with a byte capacity
  and a configurable `sync()` result, a `numpunct<char>` (separator byte, grouping bytes as
  signed `char`) and `fmtflags` (the accepted `kicad_math::stream::FmtFlags`). Fill and
  output are bytes, so fills 0x00, 0x80 and 0xFF are exact. `kicad_math::stream`'s
  formatting is not reused: it pads in `char`s and has no grouping stage between digits and
  sign; the integer stage here follows `_M_insert_int` (digits, grouping, sign/base prefix).
- The grouping `num_put<char>` applies ends at the first NUL byte of the facet's
  `grouping()`: installing a `numpunct<char>` also installs an old-ABI twin shim
  (`src/c++98/locale.cc` `_M_install_facet`, `src/c++11/cxx11-shim_facets.cc`), the
  runtime's old-ABI `num_put` caches from that twin, and the twin's `do_grouping()` rebuilds
  the string from a `const char*`. Found by the first local comparison (`*-group-three-zero-*`
  grouped by three throughout, where the header's `__add_grouping` stops at the 0); the
  source is identical in GCC 13.3, 14.2 and master. `NumPunct::effective_grouping` models it.
  A program that instantiates `__use_cache<__numpunct_cache<char>>` itself can interpose
  the runtime's copy and see the full string; the oracle does not.
- Width is consumed by the first literal; the later integers always see width 0 (or a stream
  that is not good), so `num_put` padding is unreachable from the owned bodies and not
  modelled, while its `width( 0 )` is.
- `sentry`, `setstate`/`clear`, `_M_setstate` rethrow rules, `__ostream_fill` stopping at the
  first failed `sputc`, `sputn` short writes, and the unwinding/no-unwinding paths of
  `~sentry` follow the sources above. They are identical in GCC 13.3 through master except
  `~sentry`: up to the GCC 14 branch it syncs a `unitbuf` stream whatever its state (and a
  throwing `setstate` there terminates); from GCC 15 only when `good()`, with exceptions
  disabled. `SentryFlush::Unconditional` (the reference ubuntu-24.04 runtime,
  `libstdc++6 14.2.0-4ubuntu2~24.04.1`) is the default; `SentryFlush::WhenGood` models the
  local GCC 16 runtime.

## Refusals and resource bounds

- Port bounds: a bounded buffer holds at most 2^20 bytes; an unbounded buffer refuses an
  insertion with `width` above 2^20 before any change (the reference would allocate all the
  padding). Both adapters refuse the same requests.
- Adapter contract (both programs): a stream whose state intersects its exception mask (its
  setup throws), and `unitbuf` + failing `sync()` + `badbit` in the mask (reference
  `std::terminate`). The portable API still models the latter as `InsertError::Terminate`.
- 95 refusal controls (`port/oracle/fixtures/wx-stl-compat-controls.jsonl`) cover malformed
  envelopes and fields, check order and the three domain refusals; each is followed by a
  successful insertion and conversion in the same process.
- Portable crate: `no_std` + `alloc`, `forbid(unsafe_code)`, no unwrap/expect/panic or
  indexing, no global or thread-local state; checked for `wasm32-unknown-unknown`.

## Deferrals

None. No owned body calls an unported KiCad function; every dependency is the distribution
wxWidgets, glibc or libstdc++, modelled here for the paths reached.
