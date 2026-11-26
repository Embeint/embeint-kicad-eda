<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# wx_stl_compat: TowxString's converter and the stream insertions' runtime contracts

Unit: `libs.core.application.thin.replacement.p001` (`libs/core/wx_stl_compat.cpp`).
Reference: ubuntu-24.04 (GCC 13.3 compiler, `libstdc++6 14.2.0-4ubuntu2~24.04.1`, glibc
2.39, wxWidgets 3.2.4). Evidence for each revision is listed in section 7 with what it does
and does not bind.

## 1. TowxString converts through the C library, not wxWidgets' UTF-8 decoder

`TowxString( view )` is `wxString( view.data(), view.length() )`. In wxWidgets 3.2.4 (and
identically in 3.2.9) that is `assign( psz, n )` → `ImplStr( psz, n )` with the default
converter `wxConvLibc` → `ConvertStr` → `wxMBConv::cMB2WC` → the base `wxMBConv::ToWChar`
(because `wxMBConvLibc` only implements `MB2WC`) → `wxMB2WC` → `mbsrtowcs` in the current
`LC_CTYPE` locale (`HAVE_WCSRTOMBS` is set in the distribution `setup.h`).

Consequences, all confirmed by the oracle:

- Every NUL byte in the view becomes a 0 unit, including a trailing one: `ToWChar` converts
  NUL-separated chunks and counts each NUL inside the explicit length, and `cMB2WC` does not
  subtract a terminator for an explicit length. `"ab\0"` gives three units, `"\0"` one,
  4096 NULs 4096 units.
- Any chunk the C library rejects makes the whole result "", even after valid chunks.
- The result depends on the locale. The model takes it as an explicit input:
  - glibc `C`/`POSIX` (charset `ANSI_X3.4-1968`, `__gconv_transform_ascii_internal`): any
    byte above 0x7F fails.
  - glibc UTF-8 (`__gconv_transform_utf8_internal`, e.g. `C.UTF-8`): two- to six-byte forms
    up to 0x7FFFFFFF are accepted, including 0x110000-0x7FFFFFFF and leads 0xF5-0xFD; 0x80-0xC1,
    0xFE and 0xFF leads, overlong three- to six-byte forms, UTF-16 surrogates and broken or
    truncated sequences fail; a sequence cut by a NUL fails because the NUL terminator is
    part of the converted input.
- This differs from `wxString::FromUTF8` (`wxMBConvStrictUTF8`, stops at a NUL, accepts
  surrogates and overlong forms), so the accepted `WxString::from_utf8` model is not reused
  for TowxString.

glibc 2.39 and 2.43 `iconv/gconv_simple.c`, `iconv/loop.c`, `iconv/skeleton.c`,
`wcsmbs/mbsrtowcs_l.c`, `wcsmbs/wcsmbsload.c` and `locale/C-ctype.c` differ only in
copyright years, `bswap_32` spelling and `__gconv_mark_illegal_input` bookkeeping, which does
not change the `mbsrtowcs` result. wxWidgets 3.2.4 and 3.2.9 `strconv.cpp` differ only in the
UTF-16 element type; `wxcrt.cpp` is identical; `string.cpp`/`string.h` differ only in errno
preservation for number conversions and a `wxWARN_UNUSED` annotation.

## 2. The stream grouping ends at the first NUL byte

`operator<<( int )` goes through libstdc++'s `num_put<char>`. With a `numpunct<char>` whose
`grouping()` is `"\3\0"`, the oracle prints `2,147,483,647`, while the header
`__add_grouping` (and a direct call of it) stops at the zero group: `2147483,647`.

Cause: `numpunct` is a dual-ABI facet. `locale::_Impl::_M_install_facet` (`src/c++98/locale.cc`)
installs an old-ABI `numpunct_shim` twin (`src/c++11/cxx11-shim_facets.cc`) next to a new-ABI
facet. The runtime's `num_put<char>`, compiled for the old ABI, takes its
`__numpunct_cache` from the twin, whose `do_grouping()` returns `_M_data->_M_grouping`, a
`const char*`, as a string: everything from the first NUL is lost. These files are the same
in GCC 13.3, 14.2 and master; the hosted reference run (section 7) confirms it on the
target runtime.

A program that instantiates `num_put::_M_insert_int` (and with it
`__use_cache<__numpunct_cache<char>>`) itself interposes the runtime's copy for every
stream in the process and sees the whole string. The first version of the scripted
`num_put` facet did exactly that, through a qualified call of the inline base
`num_put<char>::do_put`, and the NUL grouping cases of the first corpus changed. The facet
now reaches the classic locale's `num_put<char>` through the runtime's `use_facet` and a
virtual `put()`; every evidence run checks with `nm` that the adapter's object instantiates
neither.

## 3. The sentry destructor differs between libstdc++ 14 and 15

`basic_ostream::sentry::~sentry` for a `unitbuf` stream, when no exception propagates:

- GCC 13.3.0, 14.2.0, 14.3.0 and the `releases/gcc-14` branch: `pubsync()`, and on -1
  `setstate( badbit )`, whatever the stream state, with no `try` block. The destructor is
  `noexcept`: a throwing `sync()`, or a throwing `setstate` (badbit in `exceptions()`),
  calls `std::terminate`.
- GCC 15.2.0 and master (LWG 4188): only when `good()`, with exceptions disabled
  (`_Disable_exceptions` writes the mask member directly, so restoring it never throws) and
  anything `sync()` throws caught, so a failure only sets `badbit`.

The `char` sentries run inside the runtime library (`__ostream_insert`, `_M_insert` and
`flush` are explicitly instantiated there), so the runtime decides. The reference runtime is
GCC 14 (`SentryFlush::Unconditional`, the default); the local runtime is GCC 16
(`SentryFlush::WhenGood`, shim flag `--sentry-flush=when-good`, diagnostic only).

## 4. A failing first write unwinds past the terminating flush

A good stream with `unitbuf`, a failing `sync()` and `badbit` in `exceptions()` does not
always terminate on the reference runtime. If the first literal does not fit the buffer,
its short write calls `setstate( badbit )`, which throws `ios_base::failure` inside
`__ostream_insert`'s `try` block; `_M_setstate( badbit )` rethrows it, and the sentry is
destroyed during unwinding, which skips the flush: the outcome is `ios_failure` with no
`sync()` call. With right or internal adjustment the padding is written before the text,
with left adjustment after it; either way the first insertion needs `max( width, length )`
bytes (length 8 for `" width=\""`, 4 for `" x=\""`). From that capacity the first
insertion completes, its destructor syncs, `sync()` fails, `setstate` throws from the
`noexcept` destructor and the process terminates after one `sync()` call. On GCC 15 the same
stream gets `badbit` quietly and the next sentry throws `ios_base::failure`.

The first version refused every such request; the corpus now has 264 boundary cases (three
adjustments, five widths around each literal length, every capacity up to one past the
boundary), and a stream that is not good with the same settings terminates in its first
destructor. The adapters observe termination instead of predicting it: the C++ adapter runs
each insertion in a forked child whose terminate handler writes the state reached.

## 5. Setup failures are defined streams

`exceptions( mask )` on a state that intersects the mask sets the mask, then throws from
`clear( rdstate() )`. The stream is usable afterwards. An insertion on it then either throws
at the first sentry (a `bad` state: `setstate( failbit )` intersects the mask) or, for an
`eof`/`fail` state, fails every sentry quietly and returns, a `unitbuf` destructor still
syncing on GCC 14. These were refused as unconstructible; the corpus has 63 such streams per
operator, with and without `unitbuf` and a failing `sync()`.

## 6. Throwing host operations and tied streams

The caller's buffer, facet and tie may throw. The libstdc++ bodies catch anything thrown
inside the `try` blocks of `__ostream_insert`, `_M_insert` and `flush`: `_M_setstate(
badbit )` sets `badbit` and rethrows the same object only when `badbit` is in the mask, so a
`std::runtime_error` from `overflow` (through the default `xsputn`, byte by byte) or from
`do_put` propagates with its identity and the bytes already written stay; without `badbit`
in the mask it is swallowed, the width is not reset, and later sentries add `failbit`. The
sentry constructor flushes `tie()` for a good stream before anything else; the tied
stream's `flush()` runs its own sentry, `pubsync()` in a `try` block and its own destructor,
and its exception (a scripted `runtime_error`, or `ios_base::failure` from its own mask)
leaves the operator before any output. A `sync()` that throws inside the GCC 14 destructor
terminates; inside the GCC 15 destructor it only sets `badbit`.

The corpus scripts these paths (1390 cases with the sections above): overflow throws at
every byte of the output and around padding and capacity cuts, `do_put` throws before and
after the stock body (with grouping, short buffers and a buffer throw inside the body),
`sync()` throws at each call with every mask, and tied streams with every state, mask,
`sync()` result, throw and `unitbuf`. Each reply carries the trace of host calls, so the
order of writes, syncs, tie flushes and facet calls is compared, not only the end state.

## 7. Evidence

Original head `6f272c818852adf3d640aee315d1f4756d86eaa8` (first corpus, 3329 cases), hosted
`Rust wx_stl_compat live validation` run 45860674564 on ubuntu-24.04: `g++ (Ubuntu
13.3.0-6ubuntu2~24.04.1) 13.3.0`, `libstdc++6 14.2.0-4ubuntu2~24.04.1`, `libc6
2.39-0ubuntu8.12`, `libwxbase3.2-1t64 3.2.4+dfsg-4build1`; oracle binary sha256
`cfcfb7a8957c8a418809b57a6b115a8445b16ff3b0e6b7e27d82a5e1142b0e74`, source manifest
`96919a297e46c4b089520b34292f770d75b192cdae722ff1a801aa6aaee2c7c0` (archive
`2925d325a428b8ec2c6663c514327f11473b75869e6fd5eaeb37e2893117dd1d`), overlay revision
`c276ce2db40b2d215168ac187f706bd7dde54ab6`; 3329 compared exactly, 0 deferred, 0 failures,
with the default reference model; native replies sha256
`495f44b62abdf5001c3ed974b628c79441dd7e1a730ccf55aed71fed7dee3dd3`. It binds the target
runtime's grouping and sentry behaviour for the first corpus at that head only. The
corrected model reproduces those 3329 replies byte for byte, but the run does not validate
the new code or cases.

Corrected code at `6ccd4c4cde08588a521af14e2e48a6edbdb6f401` (overlay tree `port/oracle`
`f235dbbc54dd9c35b186a1cc2755e22fe09f4aa8`, unchanged by the documentation commits after
it), local Ubuntu 26.04 WSL, oracle built from a raw-blob export by `build.sh`: compiler
`g++ (Ubuntu 15.2.0-16ubuntu1) 15.2.0`, `libc6 2.43-2ubuntu2.7`, `libwxbase3.2-1t64
3.2.9+dfsg-1`; source manifest `c322e56929f4b8dedffeda018f9ff2906a76c96cf46b936d92dfdf935fb7c8f1`
(archive `1c36954a7cf90d8dcdbc57d5a42fc17f453f7e549ad4c746961eda6fdc1f7f5c`, 918 pinned files),
adapter `shims/core_wx_stl_compat.cpp` sha256
`cd16608d44d437eba8528960b880f48711c7e0099e0378c3ce8d9e4b2ad551cb`, oracle binary sha256
`b8aa4f454cefa8c60a9ea2a1639799c07fe45a9b6df544cb163f79e9a9ec44bc`, corpus sha256
`f22ace8504af6bb5c2ffcb25c87b81f96e3d794e06b45fefdc37f2ec18725242` (4719 cases); all 16 oracle
ctest checks pass.

- Local runtime `libstdc++6 16-20260322-1ubuntu1`: difftest 4719 compared, 0 deferred, 0
  failures with `--sentry-flush=when-good`; native and Rust replies byte-identical (sha256
  `c893a5eeae00db33ea71a70b77a26d5f7a07b67531f036b7c291bca8442f86dc`), and so is the Windows
  MSVC Rust shim; import-free wasm32 replay of all 4719 under Node 22.22.1. The default
  reference model differs on 509 cases there, all through a `unitbuf` destructor (main or
  tied stream), as `tests/corpus.rs` asserts.
- The same binary on the genuine Ubuntu 24.04 runtime package
  `libstdc++6_14.2.0-4ubuntu2~24.04.1_amd64.deb` (deb sha256
  `a51f8de7829211db961a31f02158058ad1a95f92ac6d0a5dff6350e2821c54c0`, `libstdc++.so.6.0.33`
  sha256 `1fd75fe70354a416d75aef22bcae68c47bd25d20e2d0568c30b1a9838cf62f11`, loaded through
  `LD_LIBRARY_PATH`, confirmed by `ldd`; glibc and wxWidgets stay the local 2.43 and 3.2.9):
  difftest 4719 compared, 0 deferred, 0 failures with the default reference model; replies
  byte-identical (sha256 `8607e44c6b789b83f80d83ceb731c663dd3ccf9d336ddfdabd73da320af6efcc`),
  including 303 observed `terminate`, 617 `ios_failure` and 138 `runtime_error` outcomes; the
  first 3329 replies are byte-identical to the hosted target replies above; import-free
  wasm32 replay of all 4719 with the reference model passes.
- 140 controls with continuations pass on the oracle (both runtimes) and the Rust shim.

This is not target evidence: the compiler (GCC 15.2 headers instead of 13.3) and the C
library and wxWidgets differ from the reference image. The hosted `rust-wx-stl-compat-live`
and `rust-port` runs of the corrected head on ubuntu-24.04 are the target comparison; no
golden is written before them and an independent review.

## 8. Not ported: the `USE_KICAD_WXSTRING_HASH` body

`std::hash<wxString>::operator()` in the source is compiled only under
`USE_KICAD_WXSTRING_HASH`. In the audited configuration nothing defines it: the pinned tree
contains only the `#ifdef`, and its CMake sets no such definition. wxWidgets 3.2 provides
`std::hash<wxString>` (`wx/string.h`, under `wxUSE_STD_STRING`), so the KiCad body is not
compiled in this build. A build that defines the macro itself would compile it; it is not
ported or claimed for such a build.
