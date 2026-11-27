<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-font-shim

Native JSONL adapter for unit `include.font.application.ui.p001`, mirroring
`port/oracle/shims/include_font_p001.cpp`. Each request is
`{"unit": "include.font.application.ui.p001", "case": ..., "input": {"op": ..., ...}}`.

## Fonts

- `{"kind": "base", "name": [units]}`: a FONT whose dynamic type keeps every default.
- `{"kind": "scripted", "name": [units], "stroke": b, "outline": b, "bold": b, "italic": b}`:
  a FONT overriding the four style queries.
- `{"kind": "outline", "name": [units], "file": [units], "face": null | {"style_flags": i64},
  "fake_bold": b, "fake_ital": b}`: a genuine OUTLINE_FONT; true fake flags call the setters.

Units are signed 32-bit `wchar_t` values, at most 4096.

## Operations

| `op` | Fields | Result |
| --- | --- | --- |
| `text_style` | | `values`, `italic_tilt` (bits), `embedding_permission` |
| `style_predicates` | `flags` (u32) | `bold`, `italic`, `subscript`, `superscript` |
| `queries` | `font` | `stroke`, `outline`, `bold`, `italic`, `name` |
| `lines_count` | `font`, `text` | `count` |
| `name_as_token` | `font` | always `undefined font operation` |
| `insert` | `font` (or null), `pointer`, `stream` | `bytes` (hex), `state`, `width`, `sync_calls`, `outcome` (`returned`/`failure`), `setup_threw` |
| `draw` | `font`, `text`, `position`, `mouse_pos`, `active_url` | `active_url`; Rust defers |
| `outline` | outline `font` | style queries, fake flags, `name`, `file`, `face`, `multiplier`, `face_size`, `subscript_size` |
| `set_fake` | outline `font`, `calls` (`bold`/`ital`, at most 8) | `states`: `[fake_bold, fake_italic, bold, italic]` after each call |
| `em_size` | outline `font`, `size` (bits) | `value` (bits) |
| `baseline_offset` | outline `font`, `flags` | `value` (bits) |
| `face_size`, `subscript_size` | outline `font`, `size` (i32) | `value` |

`stream` is `{"state": S, "exceptions": S, "flags": [left|right|internal|unitbuf], "width":
-4096..4096, "fill": 0..255, "buffer": null | {"capacity": null | 0..4096, "sync_fails": b}}`
with `S = {"bad": b, "eof": b, "fail": b}`. The stream starts as `std::ostream( buf )` with the
flags added, width, fill, `clear( state )` and then `exceptions( mask )` (`setup_threw` when
that throws).

`--sentry-flush=when-good` models a GCC 15+ runtime for local diagnostics; the reference
(GCC 13.3 headers, GCC 14 runtime) is the default.
