<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Audit: include.font.application.ui.p001

Pinned sources: `include/font/font.h` (312 lines) and `include/font/outline_font.h` (248 lines)
at KiCad `4cf939778df04d2b747bb2c4714fc5014a7565c7`. Every function in the unit record's
`functions` list is accounted for below. The C++ oracle is
`port/oracle/shims/include_font_p001.cpp`; the Rust port is `rust/kicad-font`.

## Evidence table

| Upstream symbol | Rust | Body and branches implemented | Successful result checked | Gap |
| --- | --- | --- | --- | --- |
| `TEXT_STYLE`, `TEXT_STYLE_FLAGS`, `ITALIC_TILT` | `text_style::{TextStyle, TextStyleFlags, ITALIC_TILT}` | all six enumerators, the exact constant | `text_style` (values, tilt bits) | none |
| `IsBold`, `IsItalic`, `IsSuperscript`, `IsSubscript` | `text_style::is_*` | one bit test each | `style_predicates-*`: all 64 low combinations, `0x80000000`, `UINT32_MAX`, 120 random | none |
| `FONT::~FONT` | `Drop` of the members | empty body | every case constructs and destroys its font | none |
| `FONT::IsStroke/IsOutline/IsBold/IsItalic` (defaults) | `Font` provided methods | `return false` | `queries-base-*` (13 names), `queries-outline-*` for `IsStroke` | none |
| `FONT::GetName` | `Font::get_name` | returns `m_fontName` | `queries-*`, `outline-*`: every unit including NUL, surrogates, negatives | none |
| `FONT::NameAsToken` | `Font::name_as_token`, `wx_conv::utf8_str` | `GetName().utf8_str().data()`: strict UTF-8 of every unit (explicit length, past NUL units) into `m_fontName`'s own conversion cache, read up to the first NUL byte; a failed conversion points to a static `""` | `name_as_token-*`: 60 cases over 20 names (NUL units, a unit 0x80000000 giving a NUL byte, 0x80000041, surrogates, astral, and 6 names with a code above U+10FFFF) on every font kind, with the `wxOnAssert` calls made | the 21 cases reaching `wxFAIL_MSG` defer at `UNPORTED(include.font.application.ui.p001) wxOnAssert` in production; with the returning assertion host their arguments and the empty-token continuation compare exactly |
| `FONT::Draw` (cursorless overload) | `font::draw` | forwards with cursor `VECTOR2I( 0, 0 )`, every other argument unchanged | 12 cases: oracle result (`aGal == nullptr` returns), Rust defers | continuation `FONT::Draw` with cursor is `UNPORTED(common.font.application.ui.p001)` |
| `FONT::linesCount` | `Font::lines_count`, `font::lines_count` | empty string 0; `std::count` over all but the last unit, + 1, modulo 2^32 | `lines_count-*`: 74 texts (newline runs, NUL, CR, negative and wide units, 4096 newlines) on base and outline fonts, 148 cases | none |
| `operator<<( ostream&, const FONT& )` | `font_stream::insert_font` | eight insertions, each through its sentry; the name through wx's `operator<<( ostream&, const wxString& )` (`wxConvWhateverWorks`: libc in the C locale, then strict UTF-8; null buffer -> `setstate( badbit )`) | `insert-*`: 2402 cases over every font kind and name, 16 scripted query combinations, 8 states x 8 masks, widths -5..40 with each adjustment, fills, capacities 0..4096, unitbuf with sync success/failure (278 with a failing sync and a nonempty mask: 91 terminate, 34 return, 153 throw), no buffer, both overloads; names reaching `wxFAIL_MSG` in order after insertions that throw, terminate or set the state | the 84 cases whose conversion reaches `wxFAIL_MSG` defer at `wxOnAssert` in production and compare exactly with the returning host; no tie, throwing buffer or unwinding context |
| `operator<<( ostream&, const FONT* )` | `font_stream::insert_font_ptr` | `os << *aFont` | about half of the `insert-*` cases | null pointer refused (2 controls, undefined) |
| `OUTLINE_FONT::EMBEDDING_PERMISSION` | `outline_font::EmbeddingPermission` | five enumerators | `text_style` (values 0..4) | none |
| `OUTLINE_FONT::IsOutline` | `Font::is_outline` | `true` | `outline-*`, `queries-outline-*` | none |
| `OUTLINE_FONT::IsBold`, `IsItalic` | `Font::is_bold`, `is_italic` | null face false; else fake flag or `style_flags & FT_STYLE_FLAG_BOLD/ITALIC` | 14 face values (null, 0..7, -1..-3, `LONG_MIN/MAX`, 0x100) x 4 fake combinations | none |
| `IsFakeItalic`, `IsFakeBold`, `SetFakeBold`, `SetFakeItal` | `OutlineFont` methods | read/set the flags | `set_fake-*`: call sequences on 5 faces, state after each call | none |
| `GetFileName`, `GetFace` | `OutlineFont::get_file_name`, `get_face` | return the members | `outline-*` | none |
| `GetSuperSubSizeMultiplier` | `Font::get_super_sub_size_multiplier` | 0.64 | `outline-*` (bits) | none |
| `GetEmSize` | `OutlineFont::get_em_size` | `aSize * 1.4`; a NaN operand returned quieted as x86 SSE2 does | `em_size-*`: zeros, subnormals, extremes, overflow boundary, infinities, quiet/signalling NaNs with payloads, 150 random bit patterns | none |
| `GetSuperSubBaselineOffset` | `Font::get_super_sub_baseline_offset` | subscript first, then superscript, else 0; `-offset * 1.4` (plain run -0.0) | `baseline_offset-*`: 188 flag values | none |
| `faceSize( int )`, `faceSize()` | `OutlineFont::face_size_of`, `face_size` | `int` product with 64, times 1.4, truncated | `face_size-*` (117 defined sizes incl. 23967451/-23967451), `outline-*` (1433) | sizes whose `* 64` overflows or whose truncation leaves `int` refused (55 controls, undefined) |
| `subscriptSize( int )`, `subscriptSize()` | `OutlineFont::subscript_size_of`, `subscript_size` | `KiROUND( faceSize * 0.64 )` through the accepted `ki_round` | `subscript_size-*` (117), `outline-*` (917) | same refusals (55 controls) |

## Default arguments, parameters and order

- `Draw`'s defaults (`aMousePos = std::nullopt`, `aActiveUrl = nullptr`) are explicit
  `Option`s; every argument is passed through, and the cursor is the literal `VECTOR2I( 0, 0 )`.
  `GAL`, `TEXT_ATTRIBUTES` and `METRICS` are unported types, passed as type parameters.
- The stream chain runs its eight insertions in source order; C++17 sequences each operand
  after the insertions before it, and the virtual queries cannot change the stream.
- `GetSuperSubBaselineOffset` tests the subscript bit before the superscript bit (flags 12 give
  the subscript offset; checked).
- `faceSize( int )` multiplies `aSize * m_charSizeScaler` in `int` before converting to `double`,
  which is where the overflow refusal comes from.

## Co-ports and changed shared interfaces

- `FONT::FONT()` (empty, `font.cpp`) as `FontBase::new`, and the member initialisers of
  `OUTLINE_FONT::OUTLINE_FONT()` (`outline_font.cpp`) as `OutlineFont::new`; the constructor's
  `FT_Init_FreeType` under a static mutex is a host concern (no global state in a portable
  crate) and no inline body depends on it. Recorded in both owners' unit notes.
- `kicad-wx-stl-compat`: `OStream::insert_c_str` and a public `check_resources` (additive).

## NameAsToken is not dangling

`return GetName().utf8_str().data();` looks like a pointer into a temporary. In the pinned
wchar_t build `utf8_str()` is `mb_str( wxMBConvUTF8() )` = `AsCharBuf( conv )`, which converts
into the string's `mutable` `m_convertedToChar` cache and returns a non-owning
`wxScopedCharBuffer` view of it (`wx/string.h` 3.2.4, `AsCharBuf` and
`ConvertedBuffer::AsScopedBuffer`). The pointer therefore stays valid until that wxString is
converted again or destroyed; the evaluation is defined and compared.

## Assertions and termination are observed, not refused

The review of `e36cd8131d` found that the adapter refused two classes of defined evaluations.
Both are now public cases (`port/findings/font-header-p001-review-e36c.md`):

- `wxFAIL_MSG` in a name conversion (`wx/strconv.cpp:1165`, `wxMBConvStrictUTF8::FromWChar`):
  the oracle wraps the process's assertion handler for the operation, records the
  `wxOnAssert` arguments in the reply and calls the wrapped handler, which without a `wxApp`
  logs and returns (wx 3.2.4 `wxDefaultAssertHandler` -> `ShowAssertDialog` ->
  `DoShowAssertDialog` returns `false`). Rust receives the same arguments through the typed
  `wx_conv::OnAssert` callee. Production defers (105 cases: 21 `NameAsToken`, 84 stream);
  `--assert-host=returning` records and returns, and `check_deferrals.py` requires that run to
  leave only the 12 Draw deferrals.
- A `unitbuf` stream whose failing `sync()` makes `~sentry`'s `setstate( badbit )` throw: every
  insertion runs in a forked child whose terminate handler reports the state reached, as the
  accepted wx-stl-compat adapter does; Rust reports the accepted model's `Terminate`.

The remaining refusals are undefined evaluations (2 null `FONT*`, 110 `faceSize` /
`subscriptSize` overflow or out-of-range conversion) and 1 malformed request.
