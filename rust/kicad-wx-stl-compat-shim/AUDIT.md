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
`basic_ios.h`, `basic_ios.tcc`, `ios_base.h`. The findings are recorded in
`port/findings/wx-stl-compat-converter-and-stream-audit.md`.

## Owned functions

| # | C++ body | Rust | Body and branches | Successful result checked | Gap |
| --- | --- | --- | --- | --- | --- |
| 1 | `std::less<wxPoint>::operator()` (lines 31-39) | `less::less_wx_point`, `less::WxPointKey` | equal `x`: `aA.y < aB.y`; else `aA.x < aB.x` | 25x25 boundary pairs and 48 random (`less-*`); `std::map<wxPoint,int>` order and first-wins `emplace` (`map-*`) | none |
| 2 | `TowxString( const std::string_view& )` (42-45) | `to_wx_string::to_wx_string` | `wxString( data, length )`: `ConvertStr` null/empty, `cMB2WC` sizing and conversion, base `wxMBConv::ToWChar` chunking by NUL with every inner and trailing NUL kept, `wxMB2WC`/`mbsrtowcs`, failure to "" | null and empty views, all 256 single bytes and `a`+byte pairs, boundary/overlong/surrogate/truncated/NUL-split UTF-8, random bytes and random 1-6 byte sequences, 4096-byte views, each in `C` and `C.UTF-8` (`str-*`) | C library locales other than glibc `C`/`POSIX` and glibc UTF-8 (the locale is an explicit input; see Remaining gaps) |
| 3 | `operator<<( std::ostream&, const wxSize& )` (48-52) | `ostream::insert_wx_size` | five insertions, each with its sentry: tie flush, literal padding, int formatting through the imbued facet, short writes, host exceptions, `_M_setstate` rethrow, `ios_base::failure`, unitbuf destructor flush and termination | `size-*`: the first corpus families plus terminate boundaries, setup failures and scripted overflow/sync/`do_put`/tie paths | see Remaining gaps |
| 4 | `operator<<( std::ostream&, const wxPoint& )` (55-59) | `ostream::insert_wx_point` | as #3 with `x`/`y` | `point-*`, the same families | see Remaining gaps |
| - | `std::hash<wxString>::operator()` under `USE_KICAD_WXSTRING_HASH` (24-29) | not ported | not compiled in the audited configuration: the macro is defined nowhere in the pinned tree or its CMake, and wxWidgets 3.2 defines `std::hash<wxString>` itself (`wx/string.h`, `wxUSE_STD_STRING`) | - | a build that defines `USE_KICAD_WXSTRING_HASH` itself (e.g. through `CXXFLAGS`, against a wxWidgets without `std::hash<wxString>`) would compile this body; it is not ported or claimed |

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
  port's allocation bound; the corpus's 4096-byte views are far below it. Both passes are
  allocation-checked: the units are reserved with `try_reserve_exact` and never pushed
  beyond it.

## Stream contract

- `OStream<H: StreamHost>` models a `basic_ostream<char>`. What the caller's program
  supplies around it is the host: its `streambuf` with no put area (`overflow`, `xsputn`
  defaulting to libstdc++'s byte-by-byte `basic_streambuf::xsputn`, `sync`), the imbued
  `num_put<char>::do_put( long )` (defaulting to the stock body, `StockPut`) and `tie()`
  (`has_tie`, `flush_tie`, which a tied `OStream` implements with `OStream::flush`). Each
  host operation is fallible: `Err` is the thrown object. `StreamBuf` is the portable
  bounded byte buffer; it grows with `try_reserve` and throws `BadAlloc` when it cannot.
- Exceptions follow the sources: a host exception or `setstate` failure inside the `try`
  block of `__ostream_insert`, `_M_insert` or `flush` sets `badbit` through `_M_setstate`
  and propagates the same object (`InsertError::Host(e)` or `Failure`) only when
  `exceptions()` contains `badbit`, else it is swallowed; the bytes written before it stay
  written and `width( 0 )` is skipped. The tie's `flush()` exception and the sentry's own
  `setstate( failbit )` failure leave the operator before any sentry exists. A failed
  iterator sets `badbit` outside the `try` block. `~sentry` does nothing while an exception
  propagates.
- Fill and output are bytes, so fills 0x00, 0x80 and 0xFF are exact. `kicad_math::stream`'s
  formatting is not reused: it pads in `char`s and has no grouping stage between digits and
  sign; the integer stage here follows `_M_insert_int` (digits, grouping, sign/base prefix),
  built in a fixed 32-byte field (the widest `int` field is 11 octal digits, 10 separators
  and a two-byte prefix), with no allocation.
- The grouping `num_put<char>` applies ends at the first NUL byte of the facet's
  `grouping()` (the old-ABI twin facet; `src/c++98/locale.cc` `_M_install_facet`,
  `src/c++11/cxx11-shim_facets.cc`), identical in GCC 13.3, 14.2 and master, and was
  confirmed on the reference runtime by the hosted run below. `NumPunct::new` keeps at most
  the first 32 grouping bytes, which behave exactly as the whole string: an `int` field has
  at most 11 digits, so `__add_grouping` advances its index at most 10 times and never
  reaches the last index of a longer string.
- The current non-mutating host hook leaves width zero after the first successful literal.
  This is an API restriction, not a C++ invariant: a real facet can mutate ios state and
  make stock integer padding reachable. That context and padding behavior remain missing.
- `~sentry` for a `unitbuf` stream is the only runtime difference (`SentryFlush`). Up to
  the GCC 14 branch it calls `pubsync()` whatever the state with no `try` block, inside a
  `noexcept` destructor: a throwing `sync()`, or `-1` with `badbit` in the mask, calls
  `std::terminate` (`InsertError::Terminate`). From GCC 15 (LWG 4188) it syncs only a good
  stream, with exceptions disabled and any throw caught, so a failure only sets `badbit`.
  `SentryFlush::Unconditional` (the reference ubuntu-24.04 runtime,
  `libstdc++6 14.2.0-4ubuntu2~24.04.1`) is the default; `SentryFlush::WhenGood` models the
  local GCC 16 runtime and is separate diagnostic evidence.

## Adapters

- Each insertion request configures the caller's stream exactly as the C++ adapter builds
  it, in the same order: the buffer, the tied stream (flags, `clear( state )`,
  `exceptions( mask )`), the imbued `numpunct` (and the scripted `num_put`), flags, width,
  precision, fill, `tie`, `clear( state )`, `exceptions( mask )`. An `exceptions( mask )`
  that throws because the state intersects the mask is caught and reported
  (`setup_failure`); the mask stays set and the insertion runs on that stream.
- The optional `host` object scripts a `std::runtime_error` from the buffer's overflow call
  with a given 0-based index, from a given `sync()` call, from a given `do_put` call before
  or after the runtime's own `num_put<char>::do_put` (an extern template, so the
  distribution body and numpunct cache run), and a tied stream with its own state, mask,
  flags and scripted `sync()`. Its reply adds the call trace (`[kind, bytes, detail]`
  events, at most 64; an insertion logs at most 30) and the tied stream's state.
- The C++ adapter runs each insertion in a forked child that writes its reply to a pipe
  and installs a terminate handler reporting the state reached. A destructor that
  terminates the reference process is therefore an observed `terminate` outcome, not a
  refusal, and every other request in the process continues.
- The only domain refusal left is the port bound below. 140 refusal controls
  (`port/oracle/fixtures/wx-stl-compat-controls.jsonl`) cover malformed envelopes, stream,
  host and tie fields, check order and that bound; each is followed by a successful
  insertion and conversion in the same process.

## Refusals and resource bounds

- Port bounds: a bounded buffer holds at most 2^20 bytes; an insertion with `width` above
  2^20 into a host that reports no capacity is refused before any change (the reference
  buffer would grow by the whole padding). Adapter bounds: views of 4096 bytes, groupings of
  16 bytes, maps of 256 entries, scripted call indexes up to 64 (overflow up to 2^20).
- Portable crate: `no_std` + `alloc`, `forbid(unsafe_code)`, no unwrap/expect/panic or
  indexing, no global or thread-local state; every allocation is fallible (`try_reserve`,
  `try_reserve_exact`, `WxString::from_units`) or fixed-size; checked for
  `wasm32-unknown-unknown`.

## Remaining gaps

- Insertions while another exception is already propagating (`uncaught_exception()` true,
  e.g. from a destructor during unwinding) skip every `unitbuf` flush; the model assumes
  none is propagating, and no case exercises it.
- `std::bad_alloc` from the reference runtime itself (the numpunct cache, the locale) and
  `std::bad_alloc` from the adapter's buffer are not exercised: the Rust buffer's
  `BadAlloc` path is modelled and unit-checked, not compared.
- A tied stream that has its own tie, or a tie cycle, is not modelled or exercised (the
  adapters tie at most one stream, untied itself).
- Host facets are scripted throws around the stock body; a `num_put` facet that writes
  different bytes, or other `do_put` overloads, are host behaviour outside the owned bodies
  and are not exercised. `numpunct` virtuals are evaluated when the locale is built, before
  any insertion.
- `forced_unwind` (thread cancellation) through the insertions is not modelled.
- TowxString in C library locales other than glibc `C`/`POSIX` and UTF-8, and other C
  libraries, are not modelled.
- `USE_KICAD_WXSTRING_HASH` builds: see the table.

## Licence and attribution

The owned bodies are KiCad's (GPL-3.0-or-later). The port also reimplements, from reading
their sources, behaviour of three dependency libraries; no upstream text is copied, and the
port is distributed under GPL-3.0-or-later:

| Library | Files modelled | Copyright | Licence | Decision |
| --- | --- | --- | --- | --- |
| libstdc++ (GCC 13.3.0, 14.2.0, 15.2.0) | `ostream`, `ostream.h`, `ostream.tcc`, `ostream_insert.h`, `locale_facets.tcc`, `streambuf.tcc`, `basic_ios.h`, `basic_ios.tcc`, `src/c++98/locale.cc`, `src/c++11/cxx11-shim_facets.cc` | (C) 1997-2024 Free Software Foundation, Inc. | GPL-3.0-or-later WITH GCC-exception-3.1 | GPL-3.0-or-later is the base licence; the runtime exception is an additional permission the port does not need. Attribution in `kicad-wx-stl-compat/src/ostream.rs`. |
| GNU C Library 2.39 | `iconv/gconv_simple.c`, `iconv/loop.c`, `iconv/skeleton.c`, `wcsmbs/mbsrtowcs_l.c` | (C) 1997-2024 Free Software Foundation, Inc. | LGPL-2.1-or-later | Distributed under GPL-3.0-or-later as LGPL-2.1 section 3 permits (as the accepted `kicad-math` glibc port does). Attribution in `kicad-wx-stl-compat/src/to_wx_string.rs`. |
| wxWidgets 3.2.4 | `src/common/string.cpp`, `src/common/strconv.cpp` | (c) 1998 Vadim Zeitlin; (c) 1999 Ove Kaaven, Robert Roebling, Vaclav Slavik; (c) 2000-2003 Vadim Zeitlin; (c) 2004 Ryan Norton, Fredrik Roubert | wxWindows Library Licence 3.1 (LGPL-2.0-or-later with an exception) | Distributed under GPL-3.0-or-later as LGPL-2.0 section 3 permits; the exception is not needed. Attribution in `kicad-wx-stl-compat/src/to_wx_string.rs`. |

The oracle adapter links these libraries as distribution binaries and does not modify them.
The accepted `WxString`, `WxPoint`, `WxSize` and `FmtFlags` models are reused unchanged.

## Deferrals

None. No owned body calls an unported KiCad function; every dependency is the distribution
wxWidgets, glibc or libstdc++, modelled here for the paths reached.

## Second independent review

Review of 058165f437 requests changes for missing ambient unwinding, mutable
facet ios context/stock padding, distinguished forced-unwind propagation and
null-buffer clear/sentry/flush behavior. These are source-contract findings,
not just absent cases. The additional allocating-Clone finding is corrected by
removing Clone from StreamBuf/OStream and the shim Sink. The source and corpus
semantics are unchanged;20focused tests, fmt/clippy, portable wasm checking and
generator checks pass. The other four findings still block readiness.

Local export caveat: the saved918-file manifest had880CRLF-only differences
from raw pinned Git blobs. Its files match their manifest, but that is not raw
Git-byte identity or target qualification. The saved runtime comparison remains
diagnostic. See port/findings/wx-stl-compat-second-review-gaps.md for detailed
source, resource-reporting and evidence limits.
