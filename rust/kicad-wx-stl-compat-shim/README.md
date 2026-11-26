<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# kicad-wx-stl-compat-shim

Native JSONL adapter for unit `libs.core.application.thin.replacement.p001`
(`libs/core/wx_stl_compat.cpp`). It mirrors `port/oracle/shims/core_wx_stl_compat.cpp`
field for field, including the validation order and every refusal message, and calls
`kicad-wx-stl-compat` for each result. The audit is in [AUDIT.md](AUDIT.md).

```text
kicad-wx-stl-compat-shim [--sentry-flush=unconditional|when-good]
```

Without an argument (and as registered for the difftest) the stream model uses the
reference runtime's sentry destructor. `--sentry-flush=when-good` selects the GCC 15 and
later destructor, for comparing against an oracle linked to such a runtime (the local
WSL host's GCC 16 `libstdc++`).

## Operations

Every request's `input` has an `op` and exactly the fields listed. Points and sizes are
`[x, y]`, two JSON integers in the 32-bit `int` range.

| `op` | Fields | Calls | Result |
| --- | --- | --- | --- |
| `less_point` | `a`, `b` | `std::less<wxPoint>()( a, b )` | `{"less": bool}` |
| `point_map` | `entries`: at most 256 `[[x, y], int]` | `std::map<wxPoint, int>::emplace` of each, in order | `{"keys": [[x, y]...], "values": [...]}` in map order |
| `to_wx_string` | `ctype`: `"C"` or `"C.UTF-8"`; `view`: lowercase hex bytes (at most 4096) or `null` | `TowxString( view )` with `setlocale( LC_CTYPE, ctype )` | `{"units": [wchar_t...]}` |
| `insert_size` | `value`, `stream` | `operator<<( out, wxSize( x, y ) )` | the stream after the call, below |
| `insert_point` | `value`, `stream` | `operator<<( out, wxPoint( x, y ) )` | the stream after the call, below |

`view: null` is a `std::string_view` with a null `data()`; `""` is a non-null empty view.

The `stream` object configures the caller's `std::ostream`, in this order: a stream buffer
that accepts `capacity` bytes (`null`: unbounded) and whose `sync()` returns -1 when
`sync_fails`; `imbue` of the classic locale with a `numpunct<char>` returning
`thousands_sep` (a byte) and `grouping` (at most 16 bytes); `flags( flags )` (a list of
`fmtflags` names: `boolalpha dec fixed hex internal left oct right scientific showbase
showpoint showpos skipws unitbuf uppercase`); `width`, `precision` (64-bit integers);
`fill` (a byte); `clear( state )`; `exceptions( mask )` (`state` and `exceptions` are lists
of `bad eof fail`). All ten fields are required.

An insertion's result reports `bytes` (lowercase hex of everything written), `state`,
`exceptions`, `flags`, `width`, `precision`, `fill`, `sync_calls` (calls of `sync()`),
`outcome` (`returned`, or `ios_failure` when `ios_base::failure` propagated) and
`same_stream` (the operator returned its argument).

## Refusals

Malformed fields are refused with `invalid_input` and one of: `expected string: op`,
`unknown operation`, `unexpected or missing field`, `expected point`,
`component out of range`, `expected entries`, `expected entry`, `value out of range`,
`expected ctype`, `expected view`, `unexpected or missing stream field`,
`expected iostate`, `expected fmtflags`, `expected streamsize`, `expected byte`,
`expected grouping`, `expected capacity`, `expected sync_fails`.

After parsing, three stream configurations are refused before any call:

- `state intersects exceptions`: such a stream cannot be set up; `exceptions( mask )`
  throws.
- `sentry destructor would terminate`: `unitbuf`, a failing `sync()` and `badbit` in
  `exceptions()`. On the reference runtime the first sentry destructor calls `pubsync()`,
  which fails, and `setstate( badbit )` then throws from a `noexcept` destructor:
  `std::terminate`. (GCC 15 and later would not throw; both adapters refuse regardless.)
- `unbounded width exceeds limit`: `width` above 2^20 with an unbounded buffer; the
  reference would allocate all the padding. A bounded buffer stops the padding at its
  capacity, so any width is accepted there.

`port/oracle/fixtures/wx-stl-compat-controls.jsonl` lists 95 such requests with their exact
refusal; `port/oracle/test_wx_stl_compat_inputs.py` replays them against either program,
each followed by successful requests.

## Corpus

`generate_cases.py` writes the public fixed-seed corpus
(`port/cases/libs.core.application.thin.replacement.p001.jsonl`, 3329 requests) and the
controls fixture; `--check` verifies both. `tests/corpus.rs` checks the Rust side's counts
and invariants; `examples/corpus_parity.rs` with `wasm-test.mjs` replays the whole corpus in
import-free wasm32 against this revision's native C++ replies
(`KICAD_WX_STL_COMPAT_WASM_REFERENCE`).
