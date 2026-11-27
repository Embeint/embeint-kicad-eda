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
`basic_ios.h`, `basic_ios.tcc`, `ios_base.h`; libsupc++ GCC 14.2.0 `eh_throw.cc`,
`eh_catch.cc`, `eh_personality.cc` and GCC 15.2.0 `eh_catch.cc`; glibc 2.39
`nptl/unwind.c`. The findings are recorded in
`port/findings/wx-stl-compat-converter-and-stream-audit.md`.

## Owned functions

| # | C++ body | Rust | Body and branches | Successful result checked | Gap |
| --- | --- | --- | --- | --- | --- |
| 1 | `std::less<wxPoint>::operator()` (lines 31-39) | `less::less_wx_point`, `less::WxPointKey` | equal `x`: `aA.y < aB.y`; else `aA.x < aB.x` | 25x25 boundary pairs and 48 random (`less-*`); `std::map<wxPoint,int>` order and first-wins `emplace` (`map-*`) | none |
| 2 | `TowxString( const std::string_view& )` (42-45) | `to_wx_string::to_wx_string` | `wxString( data, length )`: `ConvertStr` null/empty, `cMB2WC` sizing and conversion, base `wxMBConv::ToWChar` chunking by NUL with every inner and trailing NUL kept, `wxMB2WC`/`mbsrtowcs`, failure to "" | null and empty views, all 256 single bytes and `a`+byte pairs, boundary/overlong/surrogate/truncated/NUL-split UTF-8, random bytes and random 1-6 byte sequences, 4096-byte views, each in `C` and `C.UTF-8` (`str-*`) | C library locales other than glibc `C`/`POSIX` and glibc UTF-8 (the locale is an explicit input; see Remaining gaps) |
| 3 | `operator<<( std::ostream&, const wxSize& )` (48-52) | `ostream::insert_wx_size` | five insertions, each with its sentry: tie flush, literal padding, int formatting through the imbued facet and its mutable `ios_base&` (stock padding included), short writes, host exceptions, forced unwinding, `_M_setstate` rethrow, `ios_base::failure`, null buffers, the call context and the unitbuf destructor flush, termination and abort | `size-*`: the first corpus families plus terminate boundaries, setup failures, scripted overflow/sync/`do_put`/tie paths, unwinding, facet changes, cancellation and null buffers | see Remaining gaps |
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
  defaulting to libstdc++'s byte-by-byte `basic_streambuf::xsputn`, `sync`), or a null
  `rdbuf()` (`has_rdbuf`); the imbued `num_put<char>::do_put( long )` (defaulting to the
  stock body, `stock_do_put_long`); `tie()` (`has_tie`, `flush_tie`, which a tied `OStream`
  implements with `OStream::flush`); and which of its exceptions is a forced unwind
  (`is_forced_unwind`). Each host operation is fallible: `Err` is the thrown object.
  `StreamBuf` is the portable bounded byte buffer; it grows with `try_reserve` and throws
  `BadAlloc` when it cannot.
- The call context is explicit: `CallContext::uncaught_exception` says whether an exception
  is already propagating where the operator is called (a destructor during unwinding). It is
  passed to `insert_wx_size`/`insert_wx_point` and from there to every sentry of the
  operator and to `flush_tie`, so it reaches the tied stream's `flush()` and its sentry.
  Every `~sentry` then does nothing, in both runtime models (`!uncaught_exception()` is part
  of both destructors); the operator itself is otherwise unchanged, and its own exceptions
  still propagate to the caller.
- Exceptions follow the sources: a host exception or `setstate` failure inside the `try`
  block of `__ostream_insert`, `_M_insert` or `flush` sets `badbit` through `_M_setstate`
  and propagates the same object (`InsertError::Host(e)` or `Failure`) only when
  `exceptions()` contains `badbit`, else it is swallowed; the bytes written before it stay
  written and `width( 0 )` is skipped. A forced unwind is caught first by each of those
  functions' `catch( __forced_unwind& )`: `_M_setstate( badbit )` and an unconditional
  rethrow, whatever the mask. The tie's `flush()` exception and the sentry's own
  `setstate( failbit )` failure leave the operator before any sentry exists. A failed
  iterator sets `badbit` outside the `try` block.
- `~sentry` does nothing while `uncaught_exception()` is true. That covers an exception
  propagating out of the operator, and a rethrown forced unwind too: `__cxa_rethrow`
  (`libsupc++/eh_throw.cc`) increments `uncaughtExceptions` for every rethrow, while
  `__cxa_begin_catch` (`eh_catch.cc`) decrements it only for C++ exceptions, so a forced
  unwind that has been caught and rethrown counts as uncaught for every destructor after it.
- A null `rdbuf()`: `basic_ios::init( nullptr )` starts with `badbit`, and `clear( state )`
  stores `state | badbit` (so `exceptions( mask )` throws when the mask contains `badbit`),
  while `_M_setstate` does not consult the buffer. The stream is therefore never good: every
  sentry fails (adding `failbit`), nothing is written, the width is not reset, and the
  destructor's `rdbuf()` test skips its sync. `flush()` returns before its sentry when
  `rdbuf()` is null, so a tied stream without a buffer is never synced.
- The facet's context: `_M_insert` calls `__np.put( *this, *this, this->fill(), __v )`,
  passing the stream itself as the facet's `ios_base&`. The hook receives an `IosBase`
  holding mutable references to the stream's flags, width and precision and its `numpunct`,
  plus the iterator, fill and value; changes it makes are the stream's, in the order it makes
  them. The stock body `stock_do_put_long` is the full `_M_insert_int<long>` for any `long`
  (a facet can pass any value or flags): digits in the base of `io.flags()` (a negative value
  in a non-decimal base is its two's complement), grouping, sign or `0`/`0x`/`0X` prefix, then
  `__pad` to `io.width()` when it exceeds the field (`adjustfield` exactly `left`: after;
  exactly `internal`: after a leading sign or `0x`/`0X`; otherwise before), `io.width( 0 )`
  and one `sputn` through the iterator, which writes nothing once it has failed. The unpadded
  field is built in a fixed 48-byte buffer (22 octal digits, 21 separators, a two-byte
  prefix); the padded field is reserved fallibly up to `MAX_PAD_WIDTH` (2^20; the reference
  builds it with `alloca`), beyond which the stock body refuses (`StreamRefusal::PadWidth`)
  before changing anything.
- Fill and output are bytes, so fills 0x00, 0x80 and 0xFF are exact. `kicad_math::stream`'s
  formatting is not reused: it pads in `char`s and has no grouping stage between digits and
  sign.
- The grouping `num_put<char>` applies ends at the first NUL byte of the facet's
  `grouping()` (the old-ABI twin facet; `src/c++98/locale.cc` `_M_install_facet`,
  `src/c++11/cxx11-shim_facets.cc`), identical in GCC 13.3, 14.2 and master, and was
  confirmed on the reference runtime by the hosted run below. `NumPunct::new` keeps at most
  the first 32 grouping bytes, which behave exactly as the whole string: a `long` field has
  at most 22 digits, so `__add_grouping` advances its index at most 21 times and never
  reaches the last index of a longer string.
- `~sentry` for a `unitbuf` stream is the only runtime difference (`SentryFlush`). Up to
  the GCC 14 branch it calls `pubsync()` whatever the state with no `try` block, inside a
  `noexcept` destructor: a throwing `sync()` (a forced unwind included: the personality
  routine calls `std::terminate` for it at the `noexcept` frame), or `-1` with `badbit` in
  the mask, calls `std::terminate` (`InsertError::Terminate`). From GCC 15 (LWG 4188) it
  syncs only a good stream, with exceptions disabled and any throw caught by `catch( ... )`,
  so a failure only sets `badbit`; a forced unwind caught there is not rethrown, so
  `__cxa_end_catch` deletes it and glibc's `unwind_cleanup` (`nptl/unwind.c`) aborts the
  process with `FATAL: exception not rethrown` (`InsertError::Abort`, the mask still
  disabled). `SentryFlush::Unconditional` (the reference ubuntu-24.04 runtime,
  `libstdc++6 14.2.0-4ubuntu2~24.04.1`) is the default; `SentryFlush::WhenGood` models the
  local GCC 16 runtime and is separate diagnostic evidence.

## Adapters

- Each insertion request configures the caller's stream exactly as the C++ adapter builds
  it, in the same order: the buffer (or `nullptr`), the tied stream (on its buffer or
  `nullptr`; flags, `clear( state )`, `exceptions( mask )`), the imbued `numpunct` (and the
  scripted `num_put`), flags, width, precision, fill, `tie`, `clear( state )`,
  `exceptions( mask )`. An `exceptions( mask )` that throws because the state intersects the
  mask is caught and reported (`setup_failure`); the mask stays set and the insertion runs on
  that stream.
- The optional `host` object scripts a `std::runtime_error` from the buffer's overflow call
  with a given 0-based index, from a given `sync()` call, or from a given `do_put` call
  before or after the runtime's own `num_put<char>::do_put` (an extern template, so the
  distribution body and numpunct cache run); the facet's changes to its `ios_base&` before
  and after delegating, its own fill and `long` value and zero to two stock calls; a tied
  stream with its own state, mask, flags, scripted `sync()` and optional null buffer; a null
  main buffer; and the cancellation of the insertion's thread at a given overflow, `sync()`,
  `do_put` or tied `sync()` call. Its reply adds the call trace (`[kind, bytes, detail]`
  events, at most 64; an insertion logs at most 36) and the tied stream's state.
- `unwinding: true` runs the operator in the destructor of a local object while
  `std::runtime_error( "outer" )` propagates (`std::uncaught_exceptions()` is checked to be
  1); the destructor catches the operator's own exceptions and the reply reports that the
  outer exception arrived intact (`outer_preserved`). It excludes a cancellation script.
- A cancellation script runs the operator on its own thread, the streams staying on the
  calling one; the host operation calls `pthread_cancel( pthread_self() )` and
  `pthread_testcancel()`, so glibc's forced unwind leaves it. The adapter's own handlers
  catch none of it; the joined thread's `PTHREAD_CANCELED` gives the `forced_unwind` outcome.
- The C++ adapter runs each insertion in a forked child that writes its reply to a pipe
  and installs a terminate handler and a `SIGABRT` handler reporting the state reached. A
  destructor that terminates the reference process (or aborts a GCC 15 or later one) is
  therefore an observed `terminate` (or `abort`) outcome, not a refusal, and every other
  request in the process continues.
- The only domain refusals left are the port bound below and `unwinding excludes cancel`.
  217 refusal controls (`port/oracle/fixtures/wx-stl-compat-controls.jsonl`) cover malformed
  envelopes, stream, host, facet, cancellation and tie fields, check order and those
  refusals; each is followed by a successful insertion and conversion in the same process.

## Refusals and resource bounds

- Port bounds: a bounded buffer holds at most 2^20 bytes; an insertion with `width` above
  2^20 into a host that reports no capacity is refused before any change (the reference
  buffer would grow by the whole padding); the stock `do_put( long )` refuses to pad to more
  than 2^20 bytes. Adapter bounds: views of 4096 bytes, groupings of 16 bytes, maps of 256
  entries, scripted call indexes up to 64 (overflow up to 2^20), facet widths up to 4096,
  at most two stock calls per facet call.
- Portable crate: `no_std` + `alloc`, `forbid(unsafe_code)`, no unwrap/expect/panic or
  indexing, no global or thread-local state; every allocation is fallible (`try_reserve`,
  `try_reserve_exact`, `WxString::from_units`) or fixed-size; no allocating `Clone`;
  checked for `wasm32-unknown-unknown`. The call context and forced-unwind classification
  are explicit inputs from the host, not thread or process state.

## Remaining gaps

- `std::bad_alloc` from the reference runtime itself (the numpunct cache, the locale) and
  `std::bad_alloc` from the adapter's buffer are not exercised: the Rust buffer's
  `BadAlloc` path is modelled and unit-checked, not compared. The adapter's terminate and
  `SIGABRT` reporters build JSON and so allocate; they are not allocation-safe, and a
  reporting failure would surface as a harness failure, not as a result.
- A tied stream that has its own tie is expressible (a tied `OStream` whose host ties
  another) but the adapters tie at most one stream, untied itself; tie cycles are undefined
  and not modelled.
- A facet that reaches beyond its `ios_base&` (`imbue`, `iword`/`pword`,
  `register_callback`, or a downcast to the `basic_ios` for its state, mask, fill or buffer)
  is not modelled. Arbitrary bytes a host facet writes through the buffer are host
  behaviour. Other `do_put` overloads are not reached by the owned bodies. `numpunct`
  virtuals are evaluated when the locale is built, before any insertion.
- An `unwinding` insertion whose thread is cancelled (a forced unwind leaving a destructor
  during unwinding, which terminates in the caller's frame, not the operator's) is refused
  by the adapters rather than exercised.
- TowxString in C library locales other than glibc `C`/`POSIX` and UTF-8, and other C
  libraries, are not modelled.
- `USE_KICAD_WXSTRING_HASH` builds: see the table.

## Licence and attribution

The owned bodies are KiCad's (GPL-3.0-or-later). The port also reimplements, from reading
their sources, behaviour of three dependency libraries; no upstream text is copied, and the
port is distributed under GPL-3.0-or-later:

| Library | Files modelled | Copyright | Licence | Decision |
| --- | --- | --- | --- | --- |
| libstdc++ (GCC 13.3.0, 14.2.0, 15.2.0) | `ostream`, `ostream.h`, `ostream.tcc`, `ostream_insert.h`, `locale_facets.tcc`, `streambuf.tcc`, `streambuf_iterator.h`, `basic_ios.h`, `basic_ios.tcc`, `src/c++98/locale.cc`, `src/c++11/cxx11-shim_facets.cc`, `libsupc++/eh_throw.cc`, `eh_catch.cc`, `eh_personality.cc` | (C) 1997-2024 Free Software Foundation, Inc. | GPL-3.0-or-later WITH GCC-exception-3.1 | GPL-3.0-or-later is the base licence; the runtime exception is an additional permission the port does not need. Attribution in `kicad-wx-stl-compat/src/ostream.rs`. |
| GNU C Library 2.39 | `iconv/gconv_simple.c`, `iconv/loop.c`, `iconv/skeleton.c`, `wcsmbs/mbsrtowcs_l.c`, `nptl/unwind.c` (the abort on an unrethrown forced unwind) | (C) 1997-2024 Free Software Foundation, Inc. | LGPL-2.1-or-later | Distributed under GPL-3.0-or-later as LGPL-2.1 section 3 permits (as the accepted `kicad-math` glibc port does). Attribution in `kicad-wx-stl-compat/src/to_wx_string.rs` and, for `nptl/unwind.c`, `ostream.rs`. |
| wxWidgets 3.2.4 | `src/common/string.cpp`, `src/common/strconv.cpp` | (c) 1998 Vadim Zeitlin; (c) 1999 Ove Kaaven, Robert Roebling, Vaclav Slavik; (c) 2000-2003 Vadim Zeitlin; (c) 2004 Ryan Norton, Fredrik Roubert | wxWindows Library Licence 3.1 (LGPL-2.0-or-later with an exception) | Distributed under GPL-3.0-or-later as LGPL-2.0 section 3 permits; the exception is not needed. Attribution in `kicad-wx-stl-compat/src/to_wx_string.rs`. |

The oracle adapter links these libraries as distribution binaries and does not modify them.
The accepted `WxString`, `WxPoint`, `WxSize` and `FmtFlags` models are reused unchanged.

## Deferrals

None. No owned body calls an unported KiCad function; every dependency is the distribution
wxWidgets, glibc or libstdc++, modelled here for the paths reached.

## Second independent review

Review of 058165f437 requested changes for missing ambient unwinding, mutable facet ios
context and stock padding, distinguished forced-unwind propagation and null-buffer
clear/sentry/flush behaviour; its allocating-Clone finding was corrected at 4aaae34988. The
four behaviour findings are corrected in the stream contract above and compared on 990 new
public cases (`*-unwinding-*`, `*-facet-*`, `*-cancel-*`, `*-null-rdbuf-*`, after the
earlier 4719, which are unchanged) and 77 new controls. Each of the review's genuine-library
probes is a case: `point-unwinding-would-terminate-w0` (all of ` x="1" y="2"`, no sync, good,
the outer exception intact), `point-facet-probe` (` x="****+7" y="****+8"`, precision 42,
`showpos`, width 0), `point-cancel-overflow-00-mask-none` (`forced_unwind`, `badbit` only)
and `point-null-rdbuf-good-mask-none-unitbuf` (`badbit|failbit`, width 9). See
`port/findings/wx-stl-compat-review-corrections.md`.

The local reference export is now byte-identical to the raw pinned blobs: the earlier
saved stage was exported by `prepare.py` with Git's Windows `core.autocrlf=true`, which
converts `text=auto` files to CRLF. The hosted job now checks every staged file against
`git cat-file blob` before building.
