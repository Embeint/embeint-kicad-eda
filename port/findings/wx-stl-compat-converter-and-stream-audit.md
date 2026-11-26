<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# wx_stl_compat: TowxString's converter and the stream insertions' runtime contracts

Unit: `libs.core.application.thin.replacement.p001` (`libs/core/wx_stl_compat.cpp`).
State: local evidence only (GCC 15.2.0 compiler, `libstdc++6 16-20260322-1ubuntu1`,
`libc6 2.43-2ubuntu2.7`, `libwxbase3.2-1t64 3.2.9+dfsg-1`, Ubuntu 26.04 WSL). The reference
is ubuntu-24.04 (GCC 13.3 compiler, `libstdc++6 14.2.0-4ubuntu2~24.04.1`, glibc 2.39,
wxWidgets 3.2.4); its comparison is the hosted `rust-wx-stl-compat-live` and `rust-port`
runs, not this file.

## 1. TowxString converts through the C library, not wxWidgets' UTF-8 decoder

`TowxString( view )` is `wxString( view.data(), view.length() )`. In wxWidgets 3.2.4 (and
identically in 3.2.9) that is `assign( psz, n )` → `ImplStr( psz, n )` with the default
converter `wxConvLibc` → `ConvertStr` → `wxMBConv::cMB2WC` → the base `wxMBConv::ToWChar`
(because `wxMBConvLibc` only implements `MB2WC`) → `wxMB2WC` → `mbsrtowcs` in the current
`LC_CTYPE` locale (`HAVE_WCSRTOMBS` is set in the distribution `setup.h`).

Consequences, all confirmed by the local oracle:

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
`grouping()` is `"\3\0"`, the local oracle printed `2,147,483,647`, while the header
`__add_grouping` (and a direct call of it) stops at the zero group: `2147483,647`.

Cause: `numpunct` is a dual-ABI facet. `locale::_Impl::_M_install_facet` (`src/c++98/locale.cc`)
installs an old-ABI `numpunct_shim` twin (`src/c++11/cxx11-shim_facets.cc`) next to a new-ABI
facet. The runtime's `num_put<char>`, compiled for the old ABI, takes its
`__numpunct_cache` from the twin, whose `do_grouping()` returns `_M_data->_M_grouping`, a
`const char*`, as a string: everything from the first NUL is lost. These files are the same
in GCC 13.3, 14.2 and master. A program that instantiates
`__use_cache<__numpunct_cache<char>>` itself can interpose the runtime's copy and see the
whole string (observed in a standalone probe); the oracle does not.

The port applies the grouping before the first NUL (`NumPunct::effective_grouping`). The
corpus covers `[3,0]`, `[2,0,1]`, `[1,0,5]` and `[0,3]`.

## 3. The sentry destructor differs between libstdc++ 14 and 15

`basic_ostream::sentry::~sentry` for a `unitbuf` stream:

- GCC 13.3.0, 14.2.0, 14.3.0 and the `releases/gcc-14` branch: unless an exception is
  propagating, `pubsync()`, and on -1 `setstate( badbit )`, whatever the stream state. A
  throw there (badbit in `exceptions()`) terminates.
- GCC 15.2.0 and master: only when `good()`, with exceptions disabled.

The `char` sentries run inside the runtime library (`__ostream_insert` and `_M_insert` are
explicitly instantiated there), so the runtime decides. The reference runtime is GCC 14
(`Unconditional`); the local runtime is GCC 16 (`WhenGood`). Observable differences: the
number of `sync()` calls on a stream that is not good, and `badbit` when `sync()` fails.
`kicad-wx-stl-compat` models both (`SentryFlush`); the shim defaults to the reference and
accepts `--sentry-flush=when-good`.

Local evidence on commit `3b427cbfbe` (oracle sources unchanged from `bdbf2a6752`), 3329 public cases:

- Rust shim with `--sentry-flush=when-good` against the local oracle: 3329 compared exactly,
  0 deferred, 0 failures; output byte-identical (sha256 `a749805965...`).
- Default (reference) shim against the local oracle: 3258 equal, 71 differ. All 71 are
  `*-unitbuf-*` cases on a stream that is not good at some sentry destructor; the
  `tests/corpus.rs` check enumerates the same set by running both models.

All other libstdc++ paths the insertions use (`sentry` constructor, `setstate`/`clear`,
`_M_setstate`, `__ostream_insert`/`_fill`/`_write`, `_M_insert`, `operator<<( int )`,
`_M_insert_int`, `__int_to_char`, `__add_grouping`, `__pad`, `xsputn`, `ostreambuf_iterator`)
are identical in GCC 13.3, 14.2, 15.2 and master apart from pragmas, `_GLIBCXX_NODISCARD`
and the fill initialisation in `basic_ios::init`, which the adapter overrides with `fill()`.

## 4. Reference-only behaviour kept out of the corpus

- `unitbuf`, failing `sync()` and `badbit` in `exceptions()`: the reference terminates in
  `~sentry`. Both adapters refuse it ("sentry destructor would terminate"); the portable API
  reports `InsertError::Terminate`.
- A stream whose state intersects its exception mask cannot be set up without catching the
  `exceptions()` throw; both adapters refuse it.

## 5. Not ported: the `USE_KICAD_WXSTRING_HASH` body

`std::hash<wxString>::operator()` in the source is compiled only under
`USE_KICAD_WXSTRING_HASH`, which nothing defines at the pin. wxWidgets 3.2 provides
`std::hash<wxString>` (`wx/string.h`, under `wxUSE_STD_STRING`), so the KiCad body is dead
code in this build and its hash belongs to wxWidgets.

## Remaining

Hosted reference comparison (GCC 13.3 / glibc 2.39 / wx 3.2.4 / libstdc++ 14 runtime) of
all 3329 cases, the 95 controls and wasm parity against its replies; independent review.
No golden is written before both.
