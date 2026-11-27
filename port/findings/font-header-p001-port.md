<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# include/font headers: port, oracle closure and wxWidgets behaviour

Unit: `include.font.application.ui.p001` (`include/font/font.h`, `include/font/outline_font.h`
at `4cf939778df04d2b747bb2c4714fc5014a7565c7`). Rust: `rust/kicad-font`; oracle adapter:
`port/oracle/shims/include_font_p001.cpp`; evidence table: `rust/kicad-font-shim/AUDIT.md`.

## 1. Home of the font layer

`rust/PORT_PLAN.md` lists fonts under L1 with `kicad-core` as a working name. Fonts get their
own portable crate, `kicad-font`, because they need the math types, the wxString model and the
byte-exact `std::ostream` model (`kicad-math`, `kicad-map-helpers`, `kicad-wx-stl-compat`),
which `kicad-core` (the `libs/core` port) does not depend on, and because the later outline font
units bring FreeType and HarfBuzz host boundaries that belong together. It adds no external
crate.

## 2. Genuine fonts in the oracle need the pinned font sources

`FONT::FONT()` is out-of-line in `font.cpp`, and `OUTLINE_FONT`'s key function
(`GetInterline`) puts its vtable in `outline_font.cpp`, so neither class can be instantiated
without those translation units. The oracle compiles them unchanged into a `font-reference`
archive with `-ffunction-sections -fdata-sections`; with `--gc-sections` the final link keeps
only what the adapter reaches: the constructors, the vtable's virtual functions and everything
they call (`glyph.cpp`, `outline_decomposer.cpp`, HarfBuzz and FreeType), and `FONT::Draw` with
its closure (`text_attributes.cpp`, `markup_parser.cpp` with PEGTL, `string_utils.cpp`,
`color4d.cpp`). `ADVANCED_CFG::GetCfg` stays the fail-closed
`reference/geometry_unreachable_config.cpp`. The archive needs `libfreetype-dev`,
`libharfbuzz-dev`, `libfontconfig-dev` (`outline_font.cpp` includes `font/fontconfig.h`) and
`pkg-config`; every workflow that builds the shared oracle now installs them, because
`rust-port.yml` replays every unit's cases on one oracle.

Not changed: `port/reference-env` (an unqualified candidate image whose package lock does not
include these packages; its next lock refresh must add them before it can build this oracle,
so until then the dispatch-only image builds of the current oracle, such as `rust-port.yml`'s
`migrate_resistor_sse2` job, would fail at configuration) and `rust-kimath-nan.yml`, which
builds a pinned historical revision.

The adapter sets the protected/private members the out-of-line loaders would set
(`m_fontName`, `m_fontFileName`, `m_face`) through explicit-instantiation member pointers, as
the wx-stl-compat adapter does for libstdc++ members; `m_face` points to a value-initialised
`FT_FaceRec` owned by the adapter, of which the inline bodies read only `style_flags`.

## 3. `NameAsToken` is defined in the pinned build

`inline const char* NameAsToken() const { return GetName().utf8_str().data(); }` reads like a
pointer into a destroyed temporary. In wxWidgets 3.2.4's `wxUSE_UNICODE_WCHAR` build,
`utf8_str()` is `mb_str( wxMBConvUTF8() )`, and `mb_str` is `AsCharBuf( conv )`: it converts
into the string's `mutable ConvertedBuffer<char> m_convertedToChar` and returns
`m_convertedToChar.AsScopedBuffer()`, a non-owning `wxScopedCharBuffer` view
(`wx/string.h:3742-3775`, `3718-3721`); on a failed conversion it returns a non-owning view of a
static `""`. The pointer outlives the temporary and stays valid until that wxString is
converted again or destroyed, so the evaluation is defined. (In a `wxUSE_UNICODE_UTF8` build
the view is of the string's own storage.) An earlier commit of this PR treated it as
undefined; that was wrong and is corrected within the PR.

The conversion is `wxMBConvStrictUTF8::FromWChar` with an explicit length, so it encodes NUL
units and continues past them; callers read up to the first NUL byte. Each unit is masked with
`0x7FFFFFFF` first (a unit `0x80000000` encodes a NUL byte, `0x80000041` an `A`), surrogates are
encoded, and a code above U+10FFFF anywhere in the name reaches `wxFAIL_MSG`.

## 4. The stream operator's wxString insertion

wx's `operator<<( std::ostream&, const wxString& )` is `os << str.c_str()`, and for a
`wxCStrData` in the wchar_t build `os << wxConvWhateverWorks.cWX2MB( str )`:
`DoConvertWC2MB( in, wxNO_LEN )` stops at the first NUL unit; `wxWhateverWorksConv::FromWChar`
tries `wxConvLibc` (`wcsrtombs`; in the C locale only 0x01-0x7F convert) and then
`wxConvUTF8` (the strict encoder above). A null buffer would reach `os << (const char*)
nullptr`, which sets `badbit` outside any sentry; in the C locale it happens only after the
`wxFAIL_MSG` path. A local probe (glibc 2.43, wx 3.2.9) confirmed the libc-then-UTF-8 order
and the masked NUL byte; the oracle cases confirm the rest.

## 5. Refusals of defined inputs

Two classes of defined evaluations are refused identically by both programs instead of being
observed: names reaching `wxFAIL_MSG` (the handler is a wx host effect that may block on stdin
or abort; the Rust port defers it as `UNPORTED(include.font.application.ui.p001) wxOnAssert`
and implements the continuation), and `unitbuf` streams with a failing `sync()` and a nonempty
exception mask, where libstdc++ 14's `noexcept` sentry destructor may call `std::terminate`.
Observing termination needs the forked-child machinery of the wx-stl-compat adapter; that is
left for a later unit that needs it.

## 6. Runtime sentry behaviour

Every insertion's sentry destructor syncs a `unitbuf` stream unconditionally on the GCC 14
reference runtime and only when good on GCC 15+. The shim's default is the reference; its
`--sentry-flush=when-good` diagnostic mode differs on 183 of the 2008 insert cases, so the
corpus exercises that difference. The local oracle (GCC 13.3 compiler, WSL Ubuntu 26.04) matched
the reference mode on every case.
